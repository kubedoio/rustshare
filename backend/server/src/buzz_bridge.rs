//! Optional Elembra Chat → Buzz membership bridge.
//!
//! The bridge is disabled unless a separately provisioned service key is
//! configured. It consumes only the Chat admission events from the durable
//! Integration Outbox and signs NIP-43 membership commands as the bridge
//! identity, never as a human Buzz identity.

use crate::buzz_gateway::{BuzzGatewayClient, BuzzMembership};
use crate::services::chat_membership::verify_buzz_membership;
use futures_util::{SinkExt, StreamExt};
use nostr::{EventBuilder, JsonUtil, Keys, RelayUrl, Timestamp};
use rustshare_integration_events::{ConsumerOutcome, IntegrationEvent, OutboxConsumer};
use rustshare_resource_auth::{
    build_buzz_admission_event_at, build_buzz_conditional_admission_event_at,
    BuzzAdmissionOperation, BUZZ_RELAY_ADD_MEMBER_KIND, BUZZ_RELAY_REMOVE_MEMBER_KIND,
};
use serde::Deserialize;
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, warn};

const ADMIT_EVENT: &str = "io.elembra.chat.buzz.admission.requested.v1";
const REVOKE_EVENT: &str = "io.elembra.chat.buzz.admission.revoked.v1";
const CONSUMER_ID: &str = "io.elembra.chat.buzz-bridge.v1";

#[derive(Debug, Deserialize)]
pub(crate) struct AdmissionData {
    pub(crate) operation: BuzzAdmissionOperation,
    pub(crate) admission_id: uuid::Uuid,
    pub(crate) community_id: String,
    pub(crate) relay_url: String,
    #[serde(default)]
    pub(crate) relay_pubkey: Option<String>,
    pub(crate) buzz_pubkey: String,
    #[serde(default)]
    pub(crate) command_version: Option<u8>,
    #[serde(default)]
    pub(crate) command_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub(crate) expected_revision: Option<u64>,
}

/// A Buzz service/admin identity that applies Elembra's durable membership
/// decisions to the mapped relay.
pub struct BuzzAdmissionBridge {
    keys: Keys,
    pool: Option<PgPool>,
    gateway: Option<BuzzGatewayClient>,
}

impl BuzzAdmissionBridge {
    /// Read the service key from the explicit bridge-only environment setting.
    /// Missing configuration disables the consumer; it never falls back to a
    /// human binding key or an Elembra/OIDC credential.
    fn from_env() -> Option<Self> {
        let secret = std::env::var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY").ok()?;
        match Keys::parse(&secret) {
            Ok(keys) => Some(Self {
                keys,
                pool: None,
                gateway: None,
            }),
            Err(error) => {
                warn!(error = %error, "invalid Chat bridge service key; bridge disabled");
                None
            }
        }
    }

    pub fn from_env_with_pool(pool: PgPool) -> Option<Self> {
        let mut bridge = Self::from_env()?;
        bridge.gateway =
            BuzzGatewayClient::new(bridge.keys.clone(), reqwest::Client::builder()).ok();
        bridge.pool = Some(pool);
        Some(bridge)
    }

    async fn publish(
        &self,
        data: &AdmissionData,
        created_at: Timestamp,
    ) -> Result<Option<bool>, String> {
        let operation_kind = match data.operation {
            BuzzAdmissionOperation::Admit => BUZZ_RELAY_ADD_MEMBER_KIND,
            BuzzAdmissionOperation::Revoke => BUZZ_RELAY_REMOVE_MEMBER_KIND,
        };
        let command = match (data.command_id, data.expected_revision) {
            (Some(command_id), Some(revision)) if data.command_version == Some(1) => {
                build_buzz_conditional_admission_event_at(
                    data.operation,
                    &data.buzz_pubkey,
                    &self.keys,
                    created_at,
                    command_id,
                    revision,
                )
            }
            (None, None) if data.command_version.is_none() => build_buzz_admission_event_at(
                data.operation,
                &data.buzz_pubkey,
                &self.keys,
                created_at,
            ),
            _ => return Err("incomplete conditional Buzz command".to_string()),
        }
        .map_err(|error| error.to_string())?;
        if command.kind.as_u16() != operation_kind {
            return Err("constructed Buzz command kind mismatch".to_string());
        }

        self.publish_signed(&data.relay_url, command, data.command_version == Some(1))
            .await
    }

    async fn publish_signed(
        &self,
        relay_url: &str,
        command: nostr::Event,
        conditional: bool,
    ) -> Result<Option<bool>, String> {
        let relay = RelayUrl::parse(relay_url).map_err(|error| error.to_string())?;
        let (mut socket, _) = connect_async(relay.as_str())
            .await
            .map_err(|error| format!("relay connect failed: {error}"))?;
        let command_id = command.id.to_string();
        let event_message = serde_json::json!(["EVENT", command]).to_string();
        socket
            .send(Message::Text(event_message.clone().into()))
            .await
            .map_err(|error| format!("relay send failed: {error}"))?;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while let Some(message) = tokio::time::timeout_at(deadline, socket.next())
            .await
            .map_err(|_| "relay response timeout".to_string())?
            .transpose()
            .map_err(|error| format!("relay receive failed: {error}"))?
        {
            let Message::Text(text) = message else {
                continue;
            };
            let response: Value = serde_json::from_str(&text)
                .map_err(|error| format!("invalid relay response: {error}"))?;
            let Some(items) = response.as_array() else {
                continue;
            };
            match items.first().and_then(Value::as_str) {
                Some("AUTH") => {
                    let challenge = items
                        .get(1)
                        .and_then(Value::as_str)
                        .ok_or_else(|| "relay AUTH lacked a challenge".to_string())?;
                    let auth = EventBuilder::auth(challenge, relay.clone())
                        .sign_with_keys(&self.keys)
                        .map_err(|error| error.to_string())?;
                    socket
                        .send(Message::Text(
                            serde_json::json!(["AUTH", auth]).to_string().into(),
                        ))
                        .await
                        .map_err(|error| format!("relay AUTH send failed: {error}"))?;
                    socket
                        .send(Message::Text(event_message.clone().into()))
                        .await
                        .map_err(|error| format!("relay retry send failed: {error}"))?;
                }
                Some("OK") => {
                    if items.get(1).and_then(Value::as_str) != Some(command_id.as_str()) {
                        continue;
                    }
                    let accepted = items.get(2).and_then(Value::as_bool).unwrap_or(false);
                    if accepted {
                        return Ok(Some(true));
                    }
                    let reason = items.get(3).and_then(Value::as_str).unwrap_or("rejected");
                    if reason.to_ascii_lowercase().contains("auth") {
                        continue;
                    }
                    if conditional && reason == "stale_revision" {
                        return Ok(Some(false));
                    }
                    if conditional {
                        return Ok(None);
                    }
                    return Err(format!("Buzz relay rejected membership command: {reason}"));
                }
                Some("NOTICE") => {
                    debug!(relay = %relay, notice = %text, "Buzz relay notice");
                }
                _ => {}
            }
        }
        Err("Buzz relay closed before acknowledging membership command".to_string())
    }
}

#[async_trait::async_trait]
impl OutboxConsumer for BuzzAdmissionBridge {
    fn consumer_id(&self) -> &str {
        CONSUMER_ID
    }

    fn subscriptions(&self) -> Vec<String> {
        vec![ADMIT_EVENT.to_string(), REVOKE_EVENT.to_string()]
    }

    async fn process(&self, event: &IntegrationEvent) -> ConsumerOutcome {
        let data: AdmissionData = match serde_json::from_value(event.data.clone()) {
            Ok(data) => data,
            Err(_) => {
                return ConsumerOutcome::Permanent {
                    reason: "invalid Chat admission event data".to_string(),
                }
            }
        };
        let expected_operation = match event.r#type.as_str() {
            ADMIT_EVENT => BuzzAdmissionOperation::Admit,
            REVOKE_EVENT => BuzzAdmissionOperation::Revoke,
            _ => {
                return ConsumerOutcome::Permanent {
                    reason: "unsupported Chat bridge event".to_string(),
                }
            }
        };
        if data.operation != expected_operation
            || data.community_id.is_empty()
            || data.admission_id.is_nil()
        {
            return ConsumerOutcome::Permanent {
                reason: "invalid Chat admission command".to_string(),
            };
        }
        let conditional = data.command_version.is_some();
        if (conditional && (data.command_version != Some(1) || data.command_id.is_none()))
            || (!conditional && (data.command_id.is_some() || data.expected_revision.is_some()))
        {
            return ConsumerOutcome::Permanent {
                reason: "invalid conditional Chat command".to_string(),
            };
        }
        if conditional && self.pool.is_none() {
            return ConsumerOutcome::Retryable {
                reason: "durable conditional command handler is unavailable".to_string(),
            };
        }
        if conditional {
            let actor_pubkey = self.keys.public_key().to_hex();
            let pool = self.pool.as_ref().expect("checked above");
            let replacement = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM chat_identity_replacement_commands
                 WHERE command_id=$1 AND outbox_event_id=$2)",
            )
            .bind(data.command_id.expect("checked above"))
            .bind(event.id)
            .fetch_one(pool)
            .await;
            match replacement {
                Ok(true) => {
                    let valid =
                        validate_conditional_command(pool, event, &data, &actor_pubkey).await;
                    if !matches!(valid, Ok(true)) {
                        return ConsumerOutcome::Permanent {
                            reason:
                                "conditional replacement command does not match its durable record"
                                    .to_string(),
                        };
                    }
                }
                Ok(false) => return self.process_v1alpha2_lifecycle(event, data).await,
                Err(_) => {
                    return ConsumerOutcome::Retryable {
                        reason: "cannot validate durable Buzz command".to_string(),
                    }
                }
            }
        } else if let Some(pool) = &self.pool {
            match mapping_protocol(pool, event, &data).await {
                Ok(Some(version)) if version == "v1alpha2" => {
                    return ConsumerOutcome::Permanent {
                        reason: "legacy membership command rejected for v1alpha2 community".into(),
                    }
                }
                Ok(Some(version)) if version == "v1alpha1" => {}
                Ok(None) => {
                    return ConsumerOutcome::Permanent {
                        reason: "Chat membership command does not match its community mapping"
                            .into(),
                    }
                }
                Err(_) => {
                    return ConsumerOutcome::Retryable {
                        reason: "cannot verify Chat community protocol policy".into(),
                    }
                }
                _ => unreachable!(),
            }
        }
        let created_at = Timestamp::from_secs(event.time.timestamp().max(0) as u64);
        match self.publish(&data, created_at).await {
            Ok(accepted) if conditional => {
                let pool = self.pool.as_ref().expect("checked above");
                match record_conditional_outcome(pool, event, &data, accepted).await {
                    Ok(()) => ConsumerOutcome::Processed,
                    Err(_) => ConsumerOutcome::Retryable {
                        reason: "cannot persist conditional Buzz result".to_string(),
                    },
                }
            }
            Ok(Some(_)) => ConsumerOutcome::Processed,
            Ok(None) => ConsumerOutcome::Retryable {
                reason: "legacy Buzz command was rejected".to_string(),
            },
            Err(reason) => ConsumerOutcome::Retryable { reason },
        }
    }
}

impl BuzzAdmissionBridge {
    async fn process_v1alpha2_lifecycle(
        &self,
        event: &IntegrationEvent,
        data: AdmissionData,
    ) -> ConsumerOutcome {
        if data.command_id != Some(event.id) || data.expected_revision.is_some() {
            return ConsumerOutcome::Permanent {
                reason: "ordinary v1alpha2 membership intent must use its outbox event ID".into(),
            };
        }
        let Some(pool) = &self.pool else {
            return ConsumerOutcome::Retryable {
                reason: "v1alpha2 membership storage is unavailable".into(),
            };
        };
        let Some(gateway) = &self.gateway else {
            return ConsumerOutcome::Retryable {
                reason: "Buzz membership authority client is unavailable".into(),
            };
        };
        let mapping = match lifecycle_mapping(pool, event, &data).await {
            Ok(Some(mapping)) => mapping,
            Ok(None) => {
                return ConsumerOutcome::Permanent {
                    reason: "v1alpha2 event does not match an opted-in Chat mapping".into(),
                }
            }
            Err(_) => {
                return ConsumerOutcome::Retryable {
                    reason: "cannot validate v1alpha2 Chat mapping".into(),
                }
            }
        };
        match ensure_lifecycle_operation(pool, event, &data, &mapping).await {
            Ok(true) => {}
            Ok(false) => {
                return match block_lifecycle_operation(pool, event, None).await {
                    Ok(true) => ConsumerOutcome::Permanent {
                        reason: "v1alpha2 operation mapping changed after it was recorded".into(),
                    },
                    Ok(false) => ConsumerOutcome::Processed,
                    Err(_) => ConsumerOutcome::Retryable {
                        reason: "cannot persist changed v1alpha2 mapping state".into(),
                    },
                }
            }
            Err(_) => {
                return ConsumerOutcome::Retryable {
                    reason: "cannot persist durable v1alpha2 membership operation".into(),
                }
            }
        }
        match lifecycle_operation_state(pool, event).await {
            Ok(Some(state)) if state == "completed" => return ConsumerOutcome::Processed,
            Ok(Some(state)) if state == "blocked" => {
                return ConsumerOutcome::Permanent {
                    reason: "v1alpha2 membership operation is durably blocked".into(),
                }
            }
            Ok(Some(_)) => {}
            _ => {
                return ConsumerOutcome::Retryable {
                    reason: "cannot load v1alpha2 operation state".into(),
                }
            }
        }
        let current = match latest_lifecycle_attempt(pool, event).await {
            Ok(attempt) => attempt,
            Err(_) => {
                return ConsumerOutcome::Retryable {
                    reason: "cannot load durable v1alpha2 command".into(),
                }
            }
        };
        if let Some(attempt) = current
            .as_ref()
            .filter(|attempt| attempt.outcome != "stale_revision")
        {
            if attempt.outcome != "accepted" {
                let command = match nostr::Event::from_json(attempt.signed_event.to_string()) {
                    Ok(command)
                        if command.id.to_hex() == attempt.command_id.to_string()
                            && command.pubkey.to_hex() == self.keys.public_key().to_hex() =>
                    {
                        command
                    }
                    _ => {
                        return ConsumerOutcome::Permanent {
                            reason: "persisted Buzz command payload is invalid".into(),
                        }
                    }
                };
                match self.publish_signed(&mapping.relay_url, command, true).await {
                    Ok(Some(true)) => {
                        if mark_lifecycle_attempt(pool, event, attempt.attempt, "accepted")
                            .await
                            .is_err()
                        {
                            return ConsumerOutcome::Retryable {
                                reason: "cannot persist Buzz relay receipt".into(),
                            };
                        }
                    }
                    Ok(Some(false)) => {
                        if mark_lifecycle_attempt(pool, event, attempt.attempt, "stale_revision")
                            .await
                            .is_err()
                        {
                            return ConsumerOutcome::Retryable {
                                reason: "cannot persist stale Buzz revision".into(),
                            };
                        }
                        return ConsumerOutcome::Retryable {
                            reason: "Buzz membership revision is stale; fresh read required".into(),
                        };
                    }
                    Ok(None) => {
                        return match block_lifecycle_operation(
                            pool,
                            event,
                            Some(attempt.attempt),
                        )
                        .await
                        {
                            Ok(true) => ConsumerOutcome::Permanent {
                                reason: "Buzz relay definitively rejected conditional membership command".into(),
                            },
                            Ok(false) => ConsumerOutcome::Processed,
                            Err(_) => ConsumerOutcome::Retryable {
                                reason: "cannot persist definitive Buzz rejection".into(),
                            },
                        };
                    }
                    Err(_) => {
                        let _ =
                            mark_lifecycle_attempt(pool, event, attempt.attempt, "ambiguous").await;
                        return ConsumerOutcome::Retryable {
                            reason:
                                "Buzz command delivery is ambiguous; retrying exact signed payload"
                                    .into(),
                        };
                    }
                }
            }
            if attempt.outcome == "accepted"
                || lifecycle_attempt_accepted(pool, event, attempt.attempt)
                    .await
                    .unwrap_or(false)
            {
                return self
                    .verify_lifecycle_result(
                        pool,
                        gateway,
                        event,
                        &data,
                        &mapping,
                        attempt.expected_revision,
                    )
                    .await;
            }
        }

        if data.operation == BuzzAdmissionOperation::Admit && !mapping.admission_current {
            return match block_lifecycle_operation(pool, event, None).await {
                Ok(true) => ConsumerOutcome::Permanent {
                    reason: "stale Chat admission cannot activate a revoked binding".into(),
                },
                Ok(false) => ConsumerOutcome::Processed,
                Err(_) => ConsumerOutcome::Retryable {
                    reason: "cannot persist stale Chat admission rejection".into(),
                },
            };
        }

        let minimum_revision = current
            .as_ref()
            .map_or(0, |attempt| attempt.expected_revision.saturating_add(1));
        let observation = match verify_buzz_membership(
            pool,
            gateway,
            &mapping.relay_url,
            &mapping.relay_pubkey,
            mapping.community_id,
            &data.buzz_pubkey,
            minimum_revision,
        )
        .await
        {
            Ok(observation) => observation,
            Err(_) => {
                return ConsumerOutcome::Retryable {
                    reason: "fresh signed Buzz membership read failed".into(),
                }
            }
        };
        let desired = match data.operation {
            BuzzAdmissionOperation::Admit => BuzzMembership::Active,
            BuzzAdmissionOperation::Revoke => BuzzMembership::Revoked,
        };
        if observation.membership == desired {
            return match complete_lifecycle_operation(pool, event, &observation).await {
                Ok(()) => ConsumerOutcome::Processed,
                Err(_) => ConsumerOutcome::Retryable {
                    reason: "cannot persist confirmed Buzz membership".into(),
                },
            };
        }
        if data.operation == BuzzAdmissionOperation::Admit
            && observation.membership == BuzzMembership::Revoked
        {
            return match block_lifecycle_operation(pool, event, None).await {
                Ok(true) => ConsumerOutcome::Permanent {
                    reason: "Buzz revocation tombstone prevents readmission".into(),
                },
                Ok(false) => ConsumerOutcome::Processed,
                Err(_) => ConsumerOutcome::Retryable {
                    reason: "cannot persist Buzz revocation tombstone rejection".into(),
                },
            };
        }
        let attempt_no = match next_lifecycle_attempt(pool, event).await {
            Ok(attempt) => attempt,
            Err(_) => {
                return ConsumerOutcome::Retryable {
                    reason: "cannot allocate conditional Buzz command".into(),
                }
            }
        };
        let command_id = if attempt_no == 1 {
            data.command_id.unwrap_or(event.id)
        } else {
            uuid::Uuid::new_v4()
        };
        let command = match build_buzz_conditional_admission_event_at(
            data.operation,
            &data.buzz_pubkey,
            &self.keys,
            Timestamp::now(),
            command_id,
            observation.membership_revision,
        ) {
            Ok(command) => command,
            Err(_) => {
                return ConsumerOutcome::Permanent {
                    reason: "cannot sign conditional Buzz command".into(),
                }
            }
        };
        let payload = match serde_json::to_value(&command) {
            Ok(payload) => payload,
            Err(_) => {
                return ConsumerOutcome::Permanent {
                    reason: "cannot encode conditional Buzz command".into(),
                }
            }
        };
        match create_lifecycle_attempt(
            pool,
            event,
            attempt_no,
            command_id,
            observation.membership_revision,
            &payload,
        )
        .await
        {
            Ok(()) => ConsumerOutcome::Retryable {
                reason: "conditional Buzz command persisted; delivery will retry".into(),
            },
            Err(_) => ConsumerOutcome::Retryable {
                reason: "cannot persist signed conditional Buzz command".into(),
            },
        }
    }

    async fn verify_lifecycle_result(
        &self,
        pool: &PgPool,
        gateway: &BuzzGatewayClient,
        event: &IntegrationEvent,
        data: &AdmissionData,
        mapping: &LifecycleMapping,
        expected_revision: u64,
    ) -> ConsumerOutcome {
        let observation = match verify_buzz_membership(
            pool,
            gateway,
            &mapping.relay_url,
            &mapping.relay_pubkey,
            mapping.community_id,
            &data.buzz_pubkey,
            expected_revision.saturating_add(1),
        )
        .await
        {
            Ok(observation) => observation,
            Err(_) => {
                return ConsumerOutcome::Retryable {
                    reason: "Buzz command acknowledged; authoritative read-back is pending".into(),
                }
            }
        };
        let desired = match data.operation {
            BuzzAdmissionOperation::Admit => BuzzMembership::Active,
            BuzzAdmissionOperation::Revoke => BuzzMembership::Revoked,
        };
        if observation.membership != desired {
            return ConsumerOutcome::Retryable {
                reason: "Buzz command acknowledgement did not confirm requested membership".into(),
            };
        }
        match complete_lifecycle_operation(pool, event, &observation).await {
            Ok(()) => ConsumerOutcome::Processed,
            Err(_) => ConsumerOutcome::Retryable {
                reason: "cannot persist confirmed Buzz membership".into(),
            },
        }
    }
}

#[derive(Debug)]
struct LifecycleMapping {
    mapping_id: uuid::Uuid,
    community_id: uuid::Uuid,
    relay_url: String,
    relay_pubkey: String,
    admission_current: bool,
}

struct LifecycleAttempt {
    attempt: i32,
    command_id: uuid::Uuid,
    expected_revision: u64,
    signed_event: Value,
    outcome: String,
}

async fn mapping_protocol(
    pool: &PgPool,
    event: &IntegrationEvent,
    data: &AdmissionData,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT m.membership_protocol_version FROM chat_workspace_communities m
         JOIN chat_buzz_admissions a ON a.tenant_id=m.tenant_id AND a.mapping_id=m.mapping_id
         WHERE a.tenant_id=$1 AND a.admission_id=$2 AND a.buzz_pubkey=$3
           AND m.workspace_id=$4 AND m.community_id=$5",
    )
    .bind(event.tenant_id.0)
    .bind(data.admission_id)
    .bind(&data.buzz_pubkey)
    .bind(event.workspace_id.0)
    .bind(&data.community_id)
    .fetch_optional(pool)
    .await
}

async fn lifecycle_mapping(
    pool: &PgPool,
    event: &IntegrationEvent,
    data: &AdmissionData,
) -> Result<Option<LifecycleMapping>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT m.mapping_id,m.relay_url AS current_relay_url,m.relay_pubkey AS current_relay_pubkey,
           (a.active AND b.status='active' AND b.revoked_at IS NULL
             AND COALESCE(u.disabled_at IS NULL,false) AND m.active
             AND COALESCE(ae.enabled,false) AND COALESCE(aup.enabled,true)) AS admission_current
         FROM chat_workspace_communities m
         JOIN chat_buzz_admissions a ON a.tenant_id=m.tenant_id AND a.mapping_id=m.mapping_id
         JOIN chat_identity_bindings b ON b.tenant_id=a.tenant_id AND b.binding_id=a.binding_id
         LEFT JOIN users u ON u.tenant_id=b.tenant_id AND u.id=b.principal_id
         LEFT JOIN application_enablements ae ON ae.tenant_id=m.tenant_id
           AND ae.workspace_id=m.workspace_id AND ae.application_id='io.elembra.chat'
         LEFT JOIN application_user_preferences aup ON aup.user_id=b.principal_id
           AND aup.application_id='io.elembra.chat'
         WHERE m.tenant_id=$1 AND m.workspace_id=$2 AND m.community_id=$3
           AND m.membership_protocol_version='v1alpha2' AND m.relay_pubkey IS NOT NULL
           AND a.admission_id=$4 AND a.buzz_pubkey=$5",
    )
    .bind(event.tenant_id.0)
    .bind(event.workspace_id.0)
    .bind(&data.community_id)
    .bind(data.admission_id)
    .bind(&data.buzz_pubkey)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let Ok(community_id) = uuid::Uuid::parse_str(&data.community_id) else {
        return Ok(None);
    };
    let Some(relay_pubkey) = data.relay_pubkey.as_deref() else {
        return Ok(None);
    };
    if relay_pubkey.len() != 64 || !relay_pubkey.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(None);
    }
    let admission_current: bool = row.try_get("admission_current")?
        && row.try_get::<String, _>("current_relay_url")? == data.relay_url
        && row
            .try_get::<Option<String>, _>("current_relay_pubkey")?
            .as_deref()
            == Some(relay_pubkey);
    Ok(Some(LifecycleMapping {
        mapping_id: row.try_get("mapping_id")?,
        community_id,
        relay_url: data.relay_url.clone(),
        relay_pubkey: relay_pubkey.to_ascii_lowercase(),
        admission_current,
    }))
}

async fn ensure_lifecycle_operation(
    pool: &PgPool,
    event: &IntegrationEvent,
    data: &AdmissionData,
    mapping: &LifecycleMapping,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT community_id,relay_url,relay_pubkey,membership_protocol_version
         FROM chat_workspace_communities
         WHERE tenant_id=$1 AND mapping_id=$2 FOR UPDATE",
    )
    .bind(event.tenant_id.0)
    .bind(mapping.mapping_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(false);
    };
    let mapped_community: String = row.try_get("community_id")?;
    let mapped_relay: String = row.try_get("relay_url")?;
    let mapped_pin: Option<String> = row.try_get("relay_pubkey")?;
    let protocol: String = row.try_get("membership_protocol_version")?;
    let protocol_valid = protocol == "v1alpha2" && mapped_community == data.community_id;
    let admission_valid = protocol_valid
        && mapping.admission_current
        && mapped_relay == mapping.relay_url
        && mapped_pin.as_deref() == Some(mapping.relay_pubkey.as_str());
    let state = if (data.operation == BuzzAdmissionOperation::Admit && !admission_valid)
        || !protocol_valid
    {
        "blocked"
    } else {
        "pending"
    };
    sqlx::query(
        "INSERT INTO chat_buzz_membership_operations
           (source,event_id,tenant_id,workspace_id,mapping_id,admission_id,operation,
            community_id,relay_url,relay_pubkey,pubkey,state)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         ON CONFLICT (source,event_id) DO NOTHING",
    )
    .bind(&event.source)
    .bind(event.id)
    .bind(event.tenant_id.0)
    .bind(event.workspace_id.0)
    .bind(mapping.mapping_id)
    .bind(data.admission_id)
    .bind(match data.operation {
        BuzzAdmissionOperation::Admit => "admit",
        BuzzAdmissionOperation::Revoke => "revoke",
    })
    .bind(mapping.community_id)
    .bind(&mapping.relay_url)
    .bind(&mapping.relay_pubkey)
    .bind(&data.buzz_pubkey)
    .bind(state)
    .execute(&mut *tx)
    .await?;
    let matches = sqlx::query_scalar(
        "SELECT EXISTS (
           SELECT 1 FROM chat_buzz_membership_operations
           WHERE source=$1 AND event_id=$2 AND tenant_id=$3 AND workspace_id=$4
             AND mapping_id=$5 AND admission_id=$6 AND operation=$7
             AND community_id=$8 AND relay_url=$9 AND relay_pubkey=$10 AND pubkey=$11
         )",
    )
    .bind(&event.source)
    .bind(event.id)
    .bind(event.tenant_id.0)
    .bind(event.workspace_id.0)
    .bind(mapping.mapping_id)
    .bind(data.admission_id)
    .bind(match data.operation {
        BuzzAdmissionOperation::Admit => "admit",
        BuzzAdmissionOperation::Revoke => "revoke",
    })
    .bind(mapping.community_id)
    .bind(&mapping.relay_url)
    .bind(&mapping.relay_pubkey)
    .bind(&data.buzz_pubkey)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(matches)
}

async fn latest_lifecycle_attempt(
    pool: &PgPool,
    event: &IntegrationEvent,
) -> Result<Option<LifecycleAttempt>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT attempt,command_id,expected_revision,signed_event,outcome
         FROM chat_buzz_membership_command_attempts WHERE source=$1 AND event_id=$2
         ORDER BY attempt DESC LIMIT 1",
    )
    .bind(&event.source)
    .bind(event.id)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(LifecycleAttempt {
            attempt: row.try_get("attempt")?,
            command_id: row.try_get("command_id")?,
            expected_revision: u64::try_from(row.try_get::<i64, _>("expected_revision")?)
                .map_err(|_| sqlx::Error::Protocol("invalid membership revision".into()))?,
            signed_event: row.try_get("signed_event")?,
            outcome: row.try_get("outcome")?,
        })
    })
    .transpose()
}

async fn lifecycle_operation_state(
    pool: &PgPool,
    event: &IntegrationEvent,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT state FROM chat_buzz_membership_operations WHERE source=$1 AND event_id=$2",
    )
    .bind(&event.source)
    .bind(event.id)
    .fetch_optional(pool)
    .await
}

async fn next_lifecycle_attempt(
    pool: &PgPool,
    event: &IntegrationEvent,
) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(MAX(attempt),0)+1 FROM chat_buzz_membership_command_attempts
         WHERE source=$1 AND event_id=$2",
    )
    .bind(&event.source)
    .bind(event.id)
    .fetch_one(pool)
    .await
}

async fn create_lifecycle_attempt(
    pool: &PgPool,
    event: &IntegrationEvent,
    attempt: i32,
    command_id: uuid::Uuid,
    expected_revision: u64,
    signed_event: &Value,
) -> Result<(), sqlx::Error> {
    let expected_revision = i64::try_from(expected_revision)
        .map_err(|_| sqlx::Error::Protocol("membership revision overflow".into()))?;
    let mut tx = pool.begin().await?;
    let operation_state: Option<String> = sqlx::query_scalar(
        "SELECT state FROM chat_buzz_membership_operations
         WHERE source=$1 AND event_id=$2 FOR UPDATE",
    )
    .bind(&event.source)
    .bind(event.id)
    .fetch_one(&mut *tx)
    .await?;
    if operation_state.as_deref() != Some("pending") {
        tx.rollback().await?;
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO chat_buzz_membership_command_attempts
           (source,event_id,attempt,command_id,expected_revision,signed_event,outcome)
         VALUES ($1,$2,$3,$4,$5,$6,'pending') ON CONFLICT (source,event_id,attempt) DO NOTHING",
    )
    .bind(&event.source)
    .bind(event.id)
    .bind(attempt)
    .bind(command_id)
    .bind(expected_revision)
    .bind(signed_event)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn mark_lifecycle_attempt(
    pool: &PgPool,
    event: &IntegrationEvent,
    attempt: i32,
    outcome: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE chat_buzz_membership_command_attempts SET outcome=$1 WHERE source=$2 AND event_id=$3 AND attempt=$4 AND outcome IN ('pending','ambiguous')")
        .bind(outcome).bind(&event.source).bind(event.id).bind(attempt).execute(pool).await?;
    Ok(())
}

async fn lifecycle_attempt_accepted(
    pool: &PgPool,
    event: &IntegrationEvent,
    attempt: i32,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT outcome='accepted' FROM chat_buzz_membership_command_attempts WHERE source=$1 AND event_id=$2 AND attempt=$3")
        .bind(&event.source).bind(event.id).bind(attempt).fetch_one(pool).await
}

async fn complete_lifecycle_operation(
    pool: &PgPool,
    event: &IntegrationEvent,
    observation: &crate::buzz_gateway::BuzzMembershipObservation,
) -> Result<(), sqlx::Error> {
    let result = sqlx::query("UPDATE chat_buzz_membership_operations SET state='completed',verification_request_id=$1,accepted_revision=$2,accepted_state=$3,completed_at=clock_timestamp() WHERE source=$4 AND event_id=$5 AND state='pending'")
        .bind(uuid::Uuid::parse_str(&observation.request_id).map_err(|_| sqlx::Error::Protocol("invalid verification request id".into()))?)
        .bind(i64::try_from(observation.membership_revision).map_err(|_| sqlx::Error::Protocol("membership revision overflow".into()))?)
        .bind(match observation.membership { BuzzMembership::Active => "active", BuzzMembership::Absent => "absent", BuzzMembership::Revoked => "revoked" })
        .bind(&event.source).bind(event.id).execute(pool).await?;
    if result.rows_affected() == 1
        || lifecycle_operation_state(pool, event).await?.as_deref() == Some("completed")
    {
        Ok(())
    } else {
        Err(sqlx::Error::Protocol(
            "v1alpha2 membership operation was not pending".into(),
        ))
    }
}

async fn block_lifecycle_operation(
    pool: &PgPool,
    event: &IntegrationEvent,
    attempt: Option<i32>,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    if let Some(attempt) = attempt {
        sqlx::query("UPDATE chat_buzz_membership_command_attempts SET outcome='rejected' WHERE source=$1 AND event_id=$2 AND attempt=$3 AND outcome IN ('pending','ambiguous')")
            .bind(&event.source).bind(event.id).bind(attempt).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE chat_buzz_membership_operations SET state='blocked' WHERE source=$1 AND event_id=$2 AND state='pending'")
        .bind(&event.source).bind(event.id).execute(&mut *tx).await?;
    let state: Option<String> = sqlx::query_scalar(
        "SELECT state FROM chat_buzz_membership_operations WHERE source=$1 AND event_id=$2",
    )
    .bind(&event.source)
    .bind(event.id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    match state.as_deref() {
        Some("blocked") => Ok(true),
        Some("completed") => Ok(false),
        _ => Err(sqlx::Error::Protocol(
            "v1alpha2 membership operation cannot be blocked".into(),
        )),
    }
}

pub(crate) async fn validate_conditional_command(
    pool: &PgPool,
    event: &IntegrationEvent,
    data: &AdmissionData,
    actor_pubkey: &str,
) -> Result<bool, sqlx::Error> {
    let Some(command_id) = data.command_id else {
        return Ok(false);
    };
    let Some(revision) = data.expected_revision else {
        return Ok(false);
    };
    let Ok(community_id) = uuid::Uuid::parse_str(&data.community_id) else {
        return Ok(false);
    };
    let Ok(revision) = i64::try_from(revision) else {
        return Ok(false);
    };
    let row = sqlx::query("SELECT c.phase,c.community_id,c.pubkey,c.kind,c.expected_revision,c.actor_pubkey,
        r.tenant_id,r.workspace_id,r.replacement_binding_id,r.community_id AS operation_community,r.relay_url
        FROM chat_identity_replacement_commands c JOIN chat_identity_replacements r USING(operation_id)
        WHERE c.command_id=$1 AND c.outbox_event_id=$2")
        .bind(command_id)
        .bind(event.id)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else { return Ok(false) };
    let phase: String = row.try_get("phase")?;
    let kind: i32 = row.try_get("kind")?;
    let expected_kind = match data.operation {
        BuzzAdmissionOperation::Admit => BUZZ_RELAY_ADD_MEMBER_KIND as i32,
        BuzzAdmissionOperation::Revoke => BUZZ_RELAY_REMOVE_MEMBER_KIND as i32,
    };
    Ok(
        row.try_get::<uuid::Uuid, _>("community_id")? == community_id
            && row.try_get::<uuid::Uuid, _>("operation_community")? == community_id
            && row.try_get::<String, _>("relay_url")? == data.relay_url
            && row.try_get::<String, _>("pubkey")? == data.buzz_pubkey
            && row.try_get::<i64, _>("expected_revision")? == revision
            && row.try_get::<String, _>("actor_pubkey")? == actor_pubkey
            && row.try_get::<uuid::Uuid, _>("replacement_binding_id")? == data.admission_id
            && row.try_get::<uuid::Uuid, _>("tenant_id")? == event.tenant_id.0
            && row.try_get::<uuid::Uuid, _>("workspace_id")? == event.workspace_id.0
            && kind == expected_kind
            && ((phase == "revoke" && data.operation == BuzzAdmissionOperation::Revoke)
                || (phase == "admit" && data.operation == BuzzAdmissionOperation::Admit)),
    )
}

pub(crate) async fn record_conditional_outcome(
    pool: &PgPool,
    event: &IntegrationEvent,
    data: &AdmissionData,
    accepted: Option<bool>,
) -> Result<(), sqlx::Error> {
    let command_id = data.command_id.expect("conditional command validated");
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await?;
    let owner = sqlx::query(
        "SELECT r.tenant_id,r.principal_id FROM chat_identity_replacement_commands c
        JOIN chat_identity_replacements r USING(operation_id)
        WHERE c.command_id=$1 AND c.outbox_event_id=$2",
    )
    .bind(command_id)
    .bind(event.id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(owner) = owner else {
        tx.rollback().await?;
        return Err(sqlx::Error::RowNotFound);
    };
    let tenant_id: uuid::Uuid = owner.try_get("tenant_id")?;
    let principal_id: uuid::Uuid = owner.try_get("principal_id")?;
    if sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM users WHERE id=$1 AND tenant_id=$2 FOR NO KEY UPDATE",
    )
    .bind(principal_id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await?
    .is_none()
    {
        tx.rollback().await?;
        return Err(sqlx::Error::RowNotFound);
    }
    let command = sqlx::query("SELECT c.operation_id,c.phase,c.outcome,c.operation_generation,
        r.tenant_id,r.principal_id,r.state,r.actor_id,r.generation,r.transient_failures
        FROM chat_identity_replacement_commands c JOIN chat_identity_replacements r USING(operation_id)
        WHERE c.command_id=$1 AND c.outbox_event_id=$2 FOR UPDATE OF r,c")
        .bind(command_id).bind(event.id).fetch_optional(&mut *tx).await?;
    let Some(command) = command else {
        tx.rollback().await?;
        return Err(sqlx::Error::RowNotFound);
    };
    let phase: String = command.try_get("phase")?;
    let previous_outcome: String = command.try_get("outcome")?;
    if !matches!(previous_outcome.as_str(), "pending" | "ambiguous") {
        tx.commit().await?;
        return Ok(());
    }
    let state: String = command.try_get("state")?;
    let command_generation: i64 = command.try_get("operation_generation")?;
    let generation: i64 = command.try_get("generation")?;
    let transient_failures: i16 = command.try_get("transient_failures")?;
    let from = if phase == "revoke" {
        "pending_revocation"
    } else {
        "pending_replacement_admission"
    };
    let next = if phase == "revoke" {
        "pending_revocation_verification"
    } else {
        "pending_membership_verification"
    };
    let outcome = match accepted {
        Some(true) => "delivered",
        Some(false) => "stale",
        None => "rejected",
    };
    sqlx::query("UPDATE chat_identity_replacement_commands SET outcome=$2 WHERE command_id=$1 AND outcome IN ('pending','ambiguous')")
        .bind(command_id).bind(outcome).execute(&mut *tx).await?;
    if accepted == Some(true) && state == from && generation == command_generation + 1 {
        sqlx::query("UPDATE chat_identity_replacements SET state=$3,generation=generation+1,transient_failures=0,failure_code=NULL,blocked_at=NULL,retry_at=clock_timestamp(),lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp()
            WHERE operation_id=$1 AND tenant_id=$2 AND state=$4 AND generation=$5")
            .bind(command.try_get::<uuid::Uuid,_>("operation_id")?).bind(command.try_get::<uuid::Uuid,_>("tenant_id")?).bind(next).bind(from).bind(generation).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO chat_identity_replacement_audit(audit_id,tenant_id,operation_id,actor_id,from_state,to_state)
            VALUES(gen_random_uuid(),$1,$2,$3,$4,$5)")
            .bind(command.try_get::<uuid::Uuid,_>("tenant_id")?).bind(command.try_get::<uuid::Uuid,_>("operation_id")?)
            .bind(command.try_get::<uuid::Uuid,_>("actor_id")?).bind(from).bind(next).execute(&mut *tx).await?;
    } else if accepted == Some(false) && state == from && generation == command_generation + 1 {
        let next = if phase == "revoke" {
            "pending_revocation"
        } else {
            "pending_revocation_verification"
        };
        let exhausted = transient_failures + 1 >= 5;
        let reason = if exhausted {
            "replacement_retries_exhausted"
        } else {
            "replacement_stale_revision_retry"
        };
        sqlx::query("UPDATE chat_identity_replacements SET state=$3,generation=generation+1,
            transient_failures=transient_failures+1,
            blocked_at=CASE WHEN transient_failures+1 >= 5 THEN clock_timestamp() ELSE NULL END,
            retry_at=CASE WHEN transient_failures+1 >= 5 THEN clock_timestamp()
                          ELSE clock_timestamp()+LEAST(300,5*power(2,transient_failures)::integer)*interval '1 second' END,
            failure_code=$6,lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp()
            WHERE operation_id=$1 AND tenant_id=$2 AND state=$4 AND generation=$5")
            .bind(command.try_get::<uuid::Uuid, _>("operation_id")?)
            .bind(command.try_get::<uuid::Uuid, _>("tenant_id")?)
            .bind(next)
            .bind(from)
            .bind(generation)
            .bind(reason)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO chat_identity_replacement_audit(audit_id,tenant_id,operation_id,actor_id,from_state,to_state,reason_code)
            VALUES(gen_random_uuid(),$1,$2,$3,$4,$5,$6)")
            .bind(command.try_get::<uuid::Uuid, _>("tenant_id")?)
            .bind(command.try_get::<uuid::Uuid, _>("operation_id")?)
            .bind(command.try_get::<uuid::Uuid, _>("actor_id")?)
            .bind(from)
            .bind(next)
            .bind(reason)
            .execute(&mut *tx)
            .await?;
    } else if accepted.is_none() && state == from && generation == command_generation + 1 {
        sqlx::query("UPDATE chat_identity_replacements SET failure_code='buzz_command_rejected',blocked_at=clock_timestamp(),retry_at=clock_timestamp(),lease_token=NULL,lease_expires_at=NULL,updated_at=clock_timestamp()
            WHERE operation_id=$1 AND tenant_id=$2 AND state=$3 AND generation=$4")
            .bind(command.try_get::<uuid::Uuid, _>("operation_id")?)
            .bind(command.try_get::<uuid::Uuid, _>("tenant_id")?)
            .bind(from)
            .bind(generation)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO chat_identity_replacement_audit(audit_id,tenant_id,operation_id,actor_id,from_state,to_state,reason_code)
            VALUES(gen_random_uuid(),$1,$2,$3,$4,$4,'buzz_command_rejected')")
            .bind(command.try_get::<uuid::Uuid, _>("tenant_id")?)
            .bind(command.try_get::<uuid::Uuid, _>("operation_id")?)
            .bind(command.try_get::<uuid::Uuid, _>("actor_id")?)
            .bind(from)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::{TenantId, WorkspaceId};
    use rustshare_integration_events::ConsumerOutcome;
    use uuid::Uuid;

    fn event(event_type: &str, tenant: Uuid, data: Value) -> IntegrationEvent {
        IntegrationEvent::builder()
            .source("elembra://io.elembra.chat")
            .r#type(event_type)
            .tenant_id(TenantId(tenant))
            .workspace_id(WorkspaceId(tenant))
            .data(data)
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn bridge_rejects_operation_type_mismatch_without_contacting_buzz() {
        let bridge = BuzzAdmissionBridge {
            keys: Keys::generate(),
            pool: None,
            gateway: None,
        };
        let result = bridge
            .process(&event(
                ADMIT_EVENT,
                Uuid::new_v4(),
                serde_json::json!({
                    "operation": "revoke",
                    "admission_id": Uuid::new_v4(),
                    "community_id": "community",
                    "relay_url": "wss://relay.example.test",
                    "buzz_pubkey": "00".repeat(32)
                }),
            ))
            .await;
        assert!(matches!(result, ConsumerOutcome::Permanent { .. }));
    }

    #[tokio::test]
    async fn conditional_command_requires_a_database_record_even_with_distinct_tenant_and_workspace_ids(
    ) {
        let bridge = BuzzAdmissionBridge {
            keys: Keys::generate(),
            pool: None,
            gateway: None,
        };
        let tenant = Uuid::new_v4();
        let mut event = event(
            ADMIT_EVENT,
            tenant,
            serde_json::json!({
                "operation": "admit",
                "admission_id": Uuid::new_v4(),
                "community_id": "community",
                "relay_url": "wss://relay.example.test",
                "buzz_pubkey": "00".repeat(32),
                "command_version": 1,
                "command_id": Uuid::new_v4(),
                "expected_revision": 0
            }),
        );
        event.workspace_id = WorkspaceId(Uuid::new_v4());
        let result = bridge.process(&event).await;
        assert!(matches!(result, ConsumerOutcome::Retryable { .. }));
    }
}
