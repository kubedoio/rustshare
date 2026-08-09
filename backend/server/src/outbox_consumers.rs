//! Reference integration-event consumer: the durable Files "memory
//! projection" (ADR-0031 / issue #212).
//!
//! This consumer is the reference implementation of the [`OutboxConsumer`]
//! contract: it is idempotent (a durable receipt plus the business effect in
//! one consumer-local transaction), fails closed on poison events and on
//! event types it does not own, and never touches the actual file content —
//! it projects only the safe `event.data` fields (`name`, `mime_type`,
//! `size`, `version`) into `integration_reference_effects`.
//!
//! Hard security invariant (see `event_does_not_grant_resource_access` in
//! `backend/tests/outbox_integration_test.rs`): possession of an event (or
//! of anything serialized inside its `data`) grants no Files access. This
//! consumer never consults the authorizer and never fetches content.

use rustshare_integration_events::event_types::{FILES_FILE_CREATED_V1, FILES_FILE_UPDATED_V1};
use rustshare_integration_events::{event_matches_subscription, OutboxConsumer};
use rustshare_integration_events::{redact::redact_error, ConsumerOutcome, IntegrationEvent};
use sqlx::PgPool;

/// Stable consumer identity for the reference memory projection.
pub const REFERENCE_MEMORY_PROJECTION_CONSUMER_ID: &str = "io.elembra.test.memory-projection";

/// Durable consumer that projects Files file metadata from integration-event
/// `data` into `integration_reference_effects`.
///
/// `enable_effects: false` simulates a read-only consumer: receipts are
/// recorded but no effect row is written (used to prove the receipt path in
/// isolation).
pub struct ReferenceMemoryProjectionConsumer {
    pool: PgPool,
    enable_effects: bool,
}

impl ReferenceMemoryProjectionConsumer {
    /// Create the consumer over `pool`. With `enable_effects` the durable
    /// effect row is written on first processing; without it the consumer is
    /// read-only (receipt only).
    pub fn new(pool: PgPool, enable_effects: bool) -> Self {
        Self {
            pool,
            enable_effects,
        }
    }
}

impl Default for ReferenceMemoryProjectionConsumer {
    /// Defaults to `enable_effects(true)` over the local dev database (the
    /// same `DATABASE_URL` fallback the storage-layer tests use). Only
    /// suitable for tests.
    fn default() -> Self {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://rustshare:changeme@localhost:5432/rustshare".to_string()
        });
        Self::new(
            PgPool::connect_lazy(&database_url)
                .expect("DATABASE_URL must be a valid PostgreSQL URL"),
            true,
        )
    }
}

#[async_trait::async_trait]
impl OutboxConsumer for ReferenceMemoryProjectionConsumer {
    fn consumer_id(&self) -> &str {
        REFERENCE_MEMORY_PROJECTION_CONSUMER_ID
    }

    fn subscriptions(&self) -> Vec<String> {
        vec![
            FILES_FILE_CREATED_V1.to_string(),
            FILES_FILE_UPDATED_V1.to_string(),
        ]
    }

    async fn process(&self, event: &IntegrationEvent) -> ConsumerOutcome {
        // Fail closed: a poison envelope is never retried.
        if let Err(error) = event.validate() {
            let reason = redact_error(&error.to_string(), 512);
            tracing::warn!(
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
            tracing::warn!(
                consumer_id = self.consumer_id(),
                source = %event.source,
                event_id = %event.id,
                event_type = %event.r#type,
                "rejecting event outside the consumer's subscriptions"
            );
            return ConsumerOutcome::Permanent { reason };
        }

        // `event.data` is the safe projection. NEVER fetch the actual file
        // from object storage here; a consumer must not re-authorize or copy
        // source content.
        let (name, mime_type, size, version) = extract_projection(event);

        let mut tx = match self.pool.begin().await {
            Ok(tx) => tx,
            Err(error) => {
                return ConsumerOutcome::Retryable {
                    reason: redact_error(&error.to_string(), 512),
                }
            }
        };

        // Durable idempotency receipt: exactly one row per
        // (consumer, source, event) ever. `DO NOTHING` + `rows_affected()`
        // is the deduplication gate — the effect below is applied only on
        // the first processing.
        let receipt_result = sqlx::query(
            r#"
            INSERT INTO integration_consumer_receipts
                (consumer_id, source, event_id, event_type, tenant_id, workspace_id, processed_at)
            VALUES ($1, $2, $3, $4, $5, $6, now())
            ON CONFLICT (consumer_id, source, event_id) DO NOTHING
            "#,
        )
        .bind(self.consumer_id())
        .bind(&event.source)
        .bind(event.id)
        .bind(&event.r#type)
        .bind(event.tenant_id.0)
        .bind(event.workspace_id.0)
        .execute(&mut *tx)
        .await;
        let first_processing = match receipt_result {
            Ok(result) => result.rows_affected() == 1,
            Err(error) => {
                let _ = tx.rollback().await;
                return ConsumerOutcome::Retryable {
                    reason: redact_error(&error.to_string(), 512),
                };
            }
        };

        if !first_processing {
            // Duplicate delivery: the effect was already applied by the
            // first processing; nothing to do (at-least-once + idempotency).
            metrics::counter!(
                "outbox_duplicate_receipt_total",
                "consumer" => self.consumer_id().to_string()
            )
            .increment(1);
            tracing::debug!(
                consumer_id = self.consumer_id(),
                source = %event.source,
                event_id = %event.id,
                event_type = %event.r#type,
                "duplicate delivery skipped (receipt already present)"
            );
            return ConsumerOutcome::Processed;
        }

        if self.enable_effects {
            let effect_result = sqlx::query(
                r#"
                INSERT INTO integration_reference_effects
                    (consumer_id, source, event_id, event_type, tenant_id, workspace_id,
                     name, mime_type, size, version, processed_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
                ON CONFLICT (consumer_id, source, event_id) DO NOTHING
                "#,
            )
            .bind(self.consumer_id())
            .bind(&event.source)
            .bind(event.id)
            .bind(&event.r#type)
            .bind(event.tenant_id.0)
            .bind(event.workspace_id.0)
            .bind(&name)
            .bind(&mime_type)
            .bind(size)
            .bind(&version)
            .execute(&mut *tx)
            .await;
            if let Err(error) = effect_result {
                let _ = tx.rollback().await;
                return ConsumerOutcome::Retryable {
                    reason: redact_error(&error.to_string(), 512),
                };
            }
        }

        if let Err(error) = tx.commit().await {
            return ConsumerOutcome::Retryable {
                reason: redact_error(&error.to_string(), 512),
            };
        }
        tracing::info!(
            consumer_id = self.consumer_id(),
            source = %event.source,
            event_id = %event.id,
            event_type = %event.r#type,
            tenant_id = %event.tenant_id,
            "applied reference projection effect"
        );
        ConsumerOutcome::Processed
    }
}

/// Extract the safe projection fields from `event.data`.
///
/// All fields are optional in the Files file-event payload; absent or
/// wrong-typed values become `None` (persisted as NULL). Only these fields
/// are ever copied out of an event — anything else in `data` (including
/// serialized authority context) stays inert data.
pub fn extract_projection(
    event: &IntegrationEvent,
) -> (Option<String>, Option<String>, Option<i64>, Option<String>) {
    let name = event
        .data
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let mime_type = event
        .data
        .get("mime_type")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let size = event
        .data
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .map(|n| n.min(i64::MAX as u64) as i64);
    let version = event
        .data
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    (name, mime_type, size, version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
    use rustshare_integration_events::ActorRef;
    use serde_json::json;
    use uuid::Uuid;

    fn event_with_data(data: serde_json::Value) -> IntegrationEvent {
        let tenant = TenantId(Uuid::new_v4());
        IntegrationEvent::builder()
            .source("elembra://io.elembra.files")
            .r#type(FILES_FILE_CREATED_V1)
            .tenant_id(tenant)
            .workspace_id(WorkspaceId(tenant.0))
            .actor(ActorRef::Principal(PrincipalId(Uuid::new_v4())))
            .data(data)
            .build()
            .unwrap()
    }

    #[test]
    fn extract_projection_reads_all_fields() {
        let event = event_with_data(json!({
            "name": "architecture.md",
            "mime_type": "text/markdown",
            "size": 12420,
            "version": "sha256:0123abcdef",
        }));
        assert_eq!(
            extract_projection(&event),
            (
                Some("architecture.md".to_string()),
                Some("text/markdown".to_string()),
                Some(12420),
                Some("sha256:0123abcdef".to_string()),
            )
        );
    }

    #[test]
    fn extract_projection_treats_absent_and_wrong_typed_fields_as_none() {
        let event = event_with_data(json!({}));
        assert_eq!(extract_projection(&event), (None, None, None, None));

        let event = event_with_data(json!({
            "name": 42,
            "mime_type": null,
            "size": "not-a-number",
            "version": ["list"],
        }));
        assert_eq!(extract_projection(&event), (None, None, None, None));
    }

    #[test]
    fn extract_projection_never_copies_embedded_context() {
        // A serialized PrincipalContext smuggled inside `data` must stay
        // inert: the projection only ever copies name/mime_type/size/version.
        let event = event_with_data(json!({
            "name": "context.txt",
            "mime_type": "text/plain",
            "size": 3,
            "principal_context": {
                "principalId": "01234567-89ab-cdef-0123-456789abcdef",
                "principalKind": "user",
            },
        }));
        assert_eq!(
            extract_projection(&event),
            (
                Some("context.txt".to_string()),
                Some("text/plain".to_string()),
                Some(3),
                None
            )
        );
        assert!(
            event.data.get("principal_context").is_some(),
            "the context stays in the envelope as inert data"
        );
    }

    #[tokio::test]
    async fn malformed_event_is_permanent_not_retryable() {
        let consumer = ReferenceMemoryProjectionConsumer::default();
        let mut event = event_with_data(json!({"name": "x.txt"}));
        event.workspace_id = WorkspaceId(Uuid::new_v4()); // tenant != workspace
        let outcome = consumer.process(&event).await;
        assert!(
            matches!(outcome, ConsumerOutcome::Permanent { .. }),
            "a poison event must be dead-lettered, not retried: {outcome:?}"
        );
    }

    #[tokio::test]
    async fn event_type_outside_subscriptions_is_permanent() {
        let consumer = ReferenceMemoryProjectionConsumer::default();
        let tenant = TenantId(Uuid::new_v4());
        let event = IntegrationEvent::builder()
            .source("elembra://io.elembra.files")
            .r#type("io.elembra.files.share.revoked.v1")
            .tenant_id(tenant)
            .workspace_id(WorkspaceId(tenant.0))
            .data(json!({}))
            .build()
            .unwrap();
        let outcome = consumer.process(&event).await;
        assert!(matches!(outcome, ConsumerOutcome::Permanent { .. }));
    }
}
