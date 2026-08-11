//! Durable Memory consumer: the Buzz → Elembra Memory chat projection
//! (ADR-0033/ADR-0034).
//!
//! [`MemoryChatProjectionConsumer`] consumes the bridge's durable
//! `io.elembra.chat.buzz.event.observed.v1` events (published by
//! `BuzzObservationService` in `buzz_observation.rs`) and projects them into
//! the Memory catalog — exactly one `memory_catalog` record per Buzz message
//! per tenant, idempotently.
//!
//! The consumer follows the reference [`OutboxConsumer`] contract
//! (`backend/tests/contracts/reference_consumer.rs`):
//!
//! * it is idempotent — a durable receipt plus the business effect are
//!   recorded atomically in ONE consumer-local transaction
//!   ([`MemoryCatalogStore::upsert_from_event_in_tx`]);
//! * it fails closed on poison envelopes and on event types it does not own
//!   (both are dead-lettered, never retried);
//! * a per-tenant read failure (policy or body lookup) is `Retryable` — the
//!   policy is read BEFORE the receipt gate, so a transient read failure
//!   never consumes the event;
//! * the durable receipt is written on first processing even when the policy
//!   skips the event (that event will never produce a record); retryable
//!   outcomes never leave partial effects (the store writes receipt + effect
//!   in the same transaction, so a rollback undoes both).
//!
//! The body copy is never carried in the durable envelope (reference-first):
//! when `content_indexing` is on, the consumer fetches the indexing copy from
//! the bridge observation index by event id. A missing body never blocks
//! projection — the record is still created with reference-only status.

use rustshare_integration_events::event_types::CHAT_BUZZ_EVENT_OBSERVED_V1;
use rustshare_integration_events::{
    event_matches_subscription, redact::redact_error, ConsumerOutcome, IntegrationEvent,
    OutboxConsumer,
};
use rustshare_memory::event::ObservedChatEventData;
use rustshare_storage::{ChatIdentityStore, ChatObservationStore, MemoryCatalogStore};
use sqlx::PgPool;
use tracing::{debug, info, warn};

/// Stable consumer identity for the Buzz → Memory chat projection.
pub const MEMORY_CHAT_PROJECTION_CONSUMER_ID: &str = "io.elembra.memory.chat-projection.v1";

/// Durable consumer that projects observed Buzz chat events into the Memory
/// catalog under the tenant's projection policy.
pub struct MemoryChatProjectionConsumer {
    pool: PgPool,
    chat_identity: ChatIdentityStore,
    observations: ChatObservationStore,
    catalog: MemoryCatalogStore,
    consumer_id: String,
}

impl MemoryChatProjectionConsumer {
    pub fn new(
        pool: PgPool,
        chat_identity: ChatIdentityStore,
        observations: ChatObservationStore,
        catalog: MemoryCatalogStore,
    ) -> Self {
        Self {
            pool,
            chat_identity,
            observations,
            catalog,
            consumer_id: MEMORY_CHAT_PROJECTION_CONSUMER_ID.to_string(),
        }
    }

    /// Build a consumer with an isolated identity for integration-test
    /// binaries. Production always uses [`Self::new`] and the stable manifest
    /// identity; test binaries must not share global receipt rows.
    #[cfg(any(test, debug_assertions))]
    #[doc(hidden)]
    pub fn new_for_test(
        pool: PgPool,
        chat_identity: ChatIdentityStore,
        observations: ChatObservationStore,
        catalog: MemoryCatalogStore,
        consumer_id: impl Into<String>,
    ) -> Self {
        Self {
            pool,
            chat_identity,
            observations,
            catalog,
            consumer_id: consumer_id.into(),
        }
    }
}

#[async_trait::async_trait]
impl OutboxConsumer for MemoryChatProjectionConsumer {
    fn consumer_id(&self) -> &str {
        &self.consumer_id
    }

    fn subscriptions(&self) -> Vec<String> {
        vec![CHAT_BUZZ_EVENT_OBSERVED_V1.to_string()]
    }

    async fn process(&self, event: &IntegrationEvent) -> ConsumerOutcome {
        // Fail closed: a poison envelope is never retried.
        if let Err(error) = event.validate() {
            let reason = redact_error(&error.to_string(), 512);
            warn!(
                consumer_id = self.consumer_id(),
                source = %event.source,
                event_id = %event.id,
                event_type = %event.r#type,
                reason = %reason,
                "rejecting invalid integration event (poison)"
            );
            return ConsumerOutcome::Permanent { reason };
        }
        // Defense in depth: the dispatcher already filters by subscription,
        // but a consumer must fail closed on anything outside its declared
        // types (e.g. a mis-configured dispatcher or a drifted manifest).
        if !event_matches_subscription(&event.r#type, &self.subscriptions()) {
            let reason = redact_error(
                &format!(
                    "event type `{}` is not handled by consumer {}",
                    event.r#type,
                    self.consumer_id()
                ),
                512,
            );
            warn!(
                consumer_id = self.consumer_id(),
                source = %event.source,
                event_id = %event.id,
                event_type = %event.r#type,
                "rejecting event outside the consumer's subscriptions"
            );
            return ConsumerOutcome::Permanent { reason };
        }

        let data: ObservedChatEventData = match serde_json::from_value(event.data.clone()) {
            Ok(data) => data,
            Err(error) => {
                let reason =
                    redact_error(&format!("malformed observed-event payload: {error}"), 512);
                warn!(
                    consumer_id = self.consumer_id(),
                    source = %event.source,
                    event_id = %event.id,
                    event_type = %event.r#type,
                    reason = %reason,
                    "rejecting malformed observed-event payload (poison)"
                );
                return ConsumerOutcome::Permanent { reason };
            }
        };
        // Never project an unverified or malformed payload (fail closed).
        if let Err(error) = data.validate() {
            let reason = redact_error(&error.to_string(), 512);
            warn!(
                consumer_id = self.consumer_id(),
                source = %event.source,
                event_id = %event.id,
                event_type = %event.r#type,
                reason = %reason,
                "rejecting unverified or malformed observed-event payload (poison)"
            );
            return ConsumerOutcome::Permanent { reason };
        }

        let mut tx = match self.pool.begin().await {
            Ok(tx) => tx,
            Err(error) => {
                return ConsumerOutcome::Retryable {
                    reason: redact_error(&error.to_string(), 512),
                }
            }
        };

        // Read the per-tenant policy BEFORE the receipt gate (inside the tx is
        // fine): a policy read failure must retry rather than consume the
        // event under a defaulted/absent policy.
        let policy = match self
            .chat_identity
            .projection_policy(event.tenant_id, event.workspace_id)
            .await
        {
            Ok(policy) => policy,
            Err(error) => {
                let _ = tx.rollback().await;
                return ConsumerOutcome::Retryable {
                    reason: redact_error(&error.to_string(), 512),
                };
            }
        };

        // Indexing-copy body, when the tenant has opted in. A missing
        // observation row leaves `content` None: body absence must never block
        // projection (the record is still created with reference-only status).
        let content = if policy.content_indexing {
            match self
                .observations
                .get_by_event_id(event.tenant_id, &data.buzz.event_id)
                .await
            {
                Ok(row) => row.and_then(|row| row.body),
                Err(error) => {
                    let _ = tx.rollback().await;
                    return ConsumerOutcome::Retryable {
                        reason: redact_error(&error.to_string(), 512),
                    };
                }
            }
        } else {
            None
        };

        // Durable receipt + effect in one tx (the store gates on the receipt,
        // applies the policy, and writes receipt + record atomically).
        let record = match self
            .catalog
            .upsert_from_event_in_tx(&mut tx, self.consumer_id(), event, &data, &policy, content)
            .await
        {
            Ok(record) => record,
            Err(error) => {
                let _ = tx.rollback().await;
                return ConsumerOutcome::Retryable {
                    reason: redact_error(&error.to_string(), 512),
                };
            }
        };

        if let Err(error) = tx.commit().await {
            return ConsumerOutcome::Retryable {
                reason: redact_error(&error.to_string(), 512),
            };
        }

        match record {
            Some(record) => {
                info!(
                    consumer_id = self.consumer_id(),
                    tenant_id = %event.tenant_id,
                    message_id = %record.message_id,
                    event_type = ?record.event_type,
                    record_id = %record.record_id,
                    "projected chat event into the memory catalog"
                );
            }
            None => {
                // Durable outcome: nothing to do. The receipt was already
                // written on first processing by the store — a duplicate
                // delivery, a policy skip (disabled or never-eligible channel),
                // or a tombstone with no prior record.
                debug!(
                    consumer_id = self.consumer_id(),
                    source = %event.source,
                    event_id = %event.id,
                    event_type = %event.r#type,
                    "chat event consumed without projection (duplicate delivery, policy skip, or tombstone without prior record)"
                );
            }
        }
        ConsumerOutcome::Processed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::{TenantId, WorkspaceId};
    use serde_json::json;
    use uuid::Uuid;

    /// A consumer over a lazy pool. The unit tests here never touch the
    /// database: every path under test fails closed (Permanent) before any
    /// query runs.
    fn consumer() -> MemoryChatProjectionConsumer {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://rustshare:changeme@localhost:5432/rustshare".to_string()
        });
        let pool = PgPool::connect_lazy(&database_url).unwrap();
        let observations = ChatObservationStore::new(pool.clone());
        MemoryChatProjectionConsumer::new(
            pool.clone(),
            ChatIdentityStore::new(pool.clone()),
            observations.clone(),
            MemoryCatalogStore::with_observation_store(pool, observations),
        )
    }

    fn event_with_data(data: serde_json::Value) -> IntegrationEvent {
        let tenant = TenantId(Uuid::new_v4());
        IntegrationEvent::builder()
            .source("elembra://io.elembra.chat")
            .r#type(CHAT_BUZZ_EVENT_OBSERVED_V1)
            .tenant_id(tenant)
            .workspace_id(WorkspaceId(tenant.0))
            .data(data)
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn event_type_outside_subscriptions_is_permanent() {
        let tenant = TenantId(Uuid::new_v4());
        let event = IntegrationEvent::builder()
            .source("elembra://io.elembra.chat")
            .r#type("io.elembra.chat.share.revoked.v1")
            .tenant_id(tenant)
            .workspace_id(WorkspaceId(tenant.0))
            .data(json!({}))
            .build()
            .unwrap();
        let outcome = consumer().process(&event).await;
        assert!(matches!(outcome, ConsumerOutcome::Permanent { .. }));
    }

    #[tokio::test]
    async fn malformed_event_is_permanent_not_retryable() {
        // A poison envelope (tenant != workspace) must be dead-lettered, not
        // retried.
        let mut event = event_with_data(json!({"buzz": {}}));
        event.workspace_id = WorkspaceId(Uuid::new_v4());
        let outcome = consumer().process(&event).await;
        assert!(
            matches!(outcome, ConsumerOutcome::Permanent { .. }),
            "a poison event must be dead-lettered, not retried: {outcome:?}"
        );
    }

    #[tokio::test]
    async fn malformed_observed_payload_is_permanent() {
        // A valid envelope whose `data` is not an ObservedChatEventData payload
        // must be dead-lettered at the payload parse, before any projection.
        let outcome = consumer()
            .process(&event_with_data(json!({"buzz": {}})))
            .await;
        assert!(
            matches!(outcome, ConsumerOutcome::Permanent { .. }),
            "a malformed observed-event payload must be dead-lettered: {outcome:?}"
        );
    }
}
