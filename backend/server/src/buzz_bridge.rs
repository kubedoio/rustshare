//! Optional Elembra Chat → Buzz membership bridge.
//!
//! The bridge is disabled unless a separately provisioned service key is
//! configured. It consumes only the Chat admission events from the durable
//! Integration Outbox and signs NIP-43 membership commands as the bridge
//! identity, never as a human Buzz identity.

use futures_util::{SinkExt, StreamExt};
use nostr::{EventBuilder, Keys, RelayUrl, Timestamp};
use rustshare_core::domain::TenantId;
use rustshare_integration_events::{ConsumerOutcome, IntegrationEvent, OutboxConsumer};
use rustshare_resource_auth::{
    build_buzz_admission_event_at, BuzzAdmissionOperation, BUZZ_RELAY_ADD_MEMBER_KIND,
    BUZZ_RELAY_REMOVE_MEMBER_KIND,
};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, warn};

const ADMIT_EVENT: &str = "io.elembra.chat.buzz.admission.requested.v1";
const REVOKE_EVENT: &str = "io.elembra.chat.buzz.admission.revoked.v1";
const CONSUMER_ID: &str = "io.elembra.chat.buzz-bridge.v1";

#[derive(Debug, Deserialize)]
struct AdmissionData {
    operation: BuzzAdmissionOperation,
    admission_id: uuid::Uuid,
    community_id: String,
    relay_url: String,
    buzz_pubkey: String,
}

/// A Buzz service/admin identity that applies Elembra's durable membership
/// decisions to the mapped relay.
pub struct BuzzAdmissionBridge {
    keys: Keys,
}

impl BuzzAdmissionBridge {
    /// Read the service key from the explicit bridge-only environment setting.
    /// Missing configuration disables the consumer; it never falls back to a
    /// human binding key or an Elembra/OIDC credential.
    pub fn from_env() -> Option<Self> {
        let secret = std::env::var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY").ok()?;
        match Keys::parse(&secret) {
            Ok(keys) => Some(Self { keys }),
            Err(error) => {
                warn!(error = %error, "invalid Chat bridge service key; bridge disabled");
                None
            }
        }
    }

    async fn publish(&self, data: &AdmissionData, created_at: Timestamp) -> Result<(), String> {
        let relay = RelayUrl::parse(&data.relay_url).map_err(|error| error.to_string())?;
        let operation_kind = match data.operation {
            BuzzAdmissionOperation::Admit => BUZZ_RELAY_ADD_MEMBER_KIND,
            BuzzAdmissionOperation::Revoke => BUZZ_RELAY_REMOVE_MEMBER_KIND,
        };
        let command = build_buzz_admission_event_at(
            data.operation,
            &data.buzz_pubkey,
            &self.keys,
            created_at,
        )
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
                        return Ok(());
                    }
                    let reason = items.get(3).and_then(Value::as_str).unwrap_or("rejected");
                    if reason.to_ascii_lowercase().contains("auth") {
                        continue;
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
        if event.tenant_id != TenantId(event.workspace_id.0) {
            return ConsumerOutcome::Permanent {
                reason: "tenant/workspace scope mismatch".to_string(),
            };
        }
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
        let created_at = Timestamp::from_secs(event.time.timestamp().max(0) as u64);
        match self.publish(&data, created_at).await {
            Ok(()) => ConsumerOutcome::Processed,
            Err(reason) => ConsumerOutcome::Retryable { reason },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::WorkspaceId;
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
    async fn bridge_rejects_cross_scope_envelopes_before_relay_access() {
        let bridge = BuzzAdmissionBridge {
            keys: Keys::generate(),
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
                "buzz_pubkey": "00".repeat(32)
            }),
        );
        event.workspace_id = WorkspaceId(Uuid::new_v4());
        let result = bridge.process(&event).await;
        assert!(matches!(result, ConsumerOutcome::Permanent { .. }));
    }
}
