//! Optional Elembra Chat → Buzz membership bridge.
//!
//! The bridge is disabled unless a separately provisioned service key is
//! configured. It consumes only the Chat admission events from the durable
//! Integration Outbox and signs NIP-43 membership commands as the bridge
//! identity, never as a human Buzz identity.

use futures_util::{SinkExt, StreamExt};
use nostr::{EventBuilder, Keys, RelayUrl, Timestamp};
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
}

impl BuzzAdmissionBridge {
    /// Read the service key from the explicit bridge-only environment setting.
    /// Missing configuration disables the consumer; it never falls back to a
    /// human binding key or an Elembra/OIDC credential.
    pub fn from_env() -> Option<Self> {
        let secret = std::env::var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY").ok()?;
        match Keys::parse(&secret) {
            Ok(keys) => Some(Self { keys, pool: None }),
            Err(error) => {
                warn!(error = %error, "invalid Chat bridge service key; bridge disabled");
                None
            }
        }
    }

    pub fn from_env_with_pool(pool: PgPool) -> Option<Self> {
        let mut bridge = Self::from_env()?;
        bridge.pool = Some(pool);
        Some(bridge)
    }

    async fn publish(
        &self,
        data: &AdmissionData,
        created_at: Timestamp,
    ) -> Result<Option<bool>, String> {
        let relay = RelayUrl::parse(&data.relay_url).map_err(|error| error.to_string())?;
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
                    if data.command_version == Some(1) && reason == "stale_revision" {
                        return Ok(Some(false));
                    }
                    if data.command_version == Some(1) {
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
        let conditional = data.command_version.is_some()
            || data.command_id.is_some()
            || data.expected_revision.is_some();
        if conditional
            && (data.command_version != Some(1)
                || data.command_id.is_none()
                || data.expected_revision.is_none())
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
            let valid = validate_conditional_command(
                self.pool.as_ref().expect("checked above"),
                event,
                &data,
                &actor_pubkey,
            )
            .await;
            if !matches!(valid, Ok(true)) {
                return ConsumerOutcome::Permanent {
                    reason: "conditional command does not match its durable record".to_string(),
                };
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
