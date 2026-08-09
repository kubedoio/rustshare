//! Transactional integration outbox (ADR-0031).
//!
//! PostgreSQL-backed durable transport for Integration Events:
//!
//! * [`OutboxStore::insert_in_tx`] writes an outbox row atomically inside the
//!   source mutation's transaction (the mutation can never commit without its
//!   event);
//! * [`OutboxStore::claim_batch`] claims deliveries with a fencing token and
//!   lease for at-least-once dispatch;
//! * [`OutboxStore::acknowledge`] / [`OutboxStore::fail_retryable`] /
//!   [`OutboxStore::dead_letter`] / [`OutboxStore::requeue`] implement the
//!   delivery lifecycle (retry with backoff, dead-letter after exhaustion,
//!   operator requeue);
//! * [`OutboxStore::maintenance`] compacts fully-delivered outbox rows after
//!   the retention window.
//!
//! # Global (non-tenant-scoped) claims
//!
//! `claim_batch` deliberately has NO tenant filter: the delivery ledger and
//! outbox are platform-global, so a consumer with a stable identity (e.g. a
//! Memory projection serving all tenants) processes events across tenants.
//! This is intentional — the envelope still enforces tenant/workspace
//! equality on every published event, and a consumer that must not mix
//! tenants applies its own scope rule in `process` (the reference consumer
//! merely records each event's tenant id with its effect). Per-tenant claim
//! scoping is a future consumer contract, not a transport concern.
//!
//! See `docs/adr/0031-durable-integration-events.md` and
//! `docs/specs/integration-event-v1alpha1.md`.

use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rustshare_core::domain::{
    ApplicationId, ApplicationRegistry, PrincipalId, TenantId, WorkspaceId,
};
use rustshare_core::services::{
    IntegrationEventFacts, IntegrationEventPublisher, IntegrationPublishError,
};
use rustshare_integration_events::event::{ActorRef, EventValidationError, IntegrationEvent};
use rustshare_integration_events::redact::redact_error;
use rustshare_resource_auth::resource_ref::ResourceRef;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Errors raised by the outbox store.
///
/// Note: `Display`/`Error` are hand-written rather than derived because the
/// `PoisonDelivery` variant has a field named `source`, which thiserror would
/// auto-detect as the error source (`String` is not an error type).
#[derive(Debug)]
pub enum OutboxStoreError {
    /// Envelope validation failed (or the event could not be serialized).
    InvalidEvent(String),
    /// The event type is not declared by the source application's manifest.
    OwnershipRejected(String),
    /// Underlying database failure.
    Storage(sqlx::Error),
    /// A claimed row failed envelope re-validation.
    ///
    /// `claim_batch` dead-letters such rows (redacted reason) and skips them
    /// so a corrupt row cannot crash the dispatcher; this variant is used for
    /// diagnostics and for callers that need to surface poison rows.
    PoisonDelivery {
        source: String,
        event_id: Uuid,
        reason: String,
    },
}

impl fmt::Display for OutboxStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutboxStoreError::InvalidEvent(message) => {
                write!(f, "invalid integration event: {message}")
            }
            OutboxStoreError::OwnershipRejected(event_type) => {
                write!(f, "event type not owned by application: {event_type}")
            }
            OutboxStoreError::Storage(error) => write!(f, "storage error: {error}"),
            OutboxStoreError::PoisonDelivery {
                source,
                event_id,
                reason,
            } => write!(f, "poison delivery {source}/{event_id}: {reason}"),
        }
    }
}

impl std::error::Error for OutboxStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OutboxStoreError::Storage(error) => Some(error),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for OutboxStoreError {
    fn from(error: sqlx::Error) -> Self {
        OutboxStoreError::Storage(error)
    }
}

impl From<EventValidationError> for OutboxStoreError {
    fn from(error: EventValidationError) -> Self {
        OutboxStoreError::InvalidEvent(error.to_string())
    }
}

impl From<OutboxStoreError> for IntegrationPublishError {
    fn from(error: OutboxStoreError) -> Self {
        match error {
            OutboxStoreError::InvalidEvent(message) => {
                IntegrationPublishError::InvalidEvent(message)
            }
            OutboxStoreError::OwnershipRejected(message) => {
                IntegrationPublishError::OwnershipRejected(message)
            }
            OutboxStoreError::Storage(database_error) => {
                IntegrationPublishError::Persistence(database_error.to_string())
            }
            OutboxStoreError::PoisonDelivery { .. } => {
                IntegrationPublishError::Persistence(error.to_string())
            }
        }
    }
}

/// One claimed delivery handed to the dispatcher.
#[derive(Debug, Clone)]
pub struct ClaimedEvent {
    pub consumer_id: String,
    pub source: String,
    pub event_id: Uuid,
    /// Fencing token; regenerated on every claim, so a stale worker holding
    /// an older token can never acknowledge the new lease.
    pub claim_token: Uuid,
    pub attempt_count: i32,
    /// The envelope deserialized from `event_json` and re-validated.
    pub event: IntegrationEvent,
}

/// Dispatcher/lease configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxConfig {
    /// Maximum rows claimed per batch.
    pub claim_batch_size: i64,
    /// Lease duration in seconds for a claimed delivery.
    pub lease_secs: i64,
    /// Maximum attempts before a delivery is dead-lettered.
    pub max_attempts: i32,
    /// Initial retry backoff in milliseconds.
    pub backoff_initial_ms: u64,
    /// Maximum retry backoff in milliseconds.
    pub backoff_max_ms: u64,
    /// Outbox retention in hours before fully-delivered rows are compacted;
    /// `0` disables retention cleanup.
    pub retention_hours: i64,
}

impl Default for OutboxConfig {
    fn default() -> Self {
        Self {
            claim_batch_size: 50,
            lease_secs: 60,
            max_attempts: 5,
            backoff_initial_ms: 1000,
            backoff_max_ms: 300_000,
            retention_hours: 168,
        }
    }
}

/// Per-consumer delivery count for one state.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ConsumerCount {
    pub consumer_id: String,
    pub state: String,
    pub count: i64,
}

/// Operator-safe dead-letter metadata (no `event_json`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeadLetterEntry {
    pub consumer_id: String,
    pub source: String,
    pub event_id: Uuid,
    pub event_type: String,
    pub tenant_id: Uuid,
    pub workspace_id: Uuid,
    pub attempt_count: i32,
    pub first_attempt_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub dead_lettered_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

/// Row returned by the claim queries (delivery columns + outbox payload).
#[derive(Debug, sqlx::FromRow)]
struct ClaimedRow {
    source: String,
    event_id: Uuid,
    claim_token: Uuid,
    attempt_count: i32,
    event_json: serde_json::Value,
}

/// Transactional integration outbox backed by PostgreSQL.
pub struct OutboxStore {
    pool: PgPool,
    application_registry: Arc<ApplicationRegistry>,
}

impl OutboxStore {
    pub fn new(pool: PgPool, application_registry: Arc<ApplicationRegistry>) -> Self {
        Self {
            pool,
            application_registry,
        }
    }

    /// Access the underlying database pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// The Application registry used for event-type ownership checks.
    pub fn registry(&self) -> &ApplicationRegistry {
        &self.application_registry
    }

    /// Validate the envelope, verify the source application owns the event
    /// type, and insert the outbox row atomically inside `tx`.
    ///
    /// Idempotent: publishing the same `(source, event_id)` twice succeeds
    /// silently (the duplicate insert is a no-op, logged at debug).
    pub async fn insert_in_tx(
        &self,
        tx: &mut Transaction<'static, Postgres>,
        event: &IntegrationEvent,
    ) -> Result<(), OutboxStoreError> {
        event.validate()?;
        let application = event.source_application()?;
        if !self
            .application_registry
            .owns_event_type(&application, &event.r#type)
        {
            return Err(OutboxStoreError::OwnershipRejected(event.r#type.clone()));
        }
        let event_json = serde_json::to_value(event).map_err(|error| {
            OutboxStoreError::InvalidEvent(format!("event serialization failed: {error}"))
        })?;
        let result = sqlx::query!(
            r#"
            INSERT INTO integration_outbox (source, event_id, event_type, application_id, tenant_id, workspace_id, event_json, created_at, available_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())
            ON CONFLICT (source, event_id) DO NOTHING
            "#,
            &event.source,
            event.id,
            &event.r#type,
            &application.0,
            event.tenant_id.0,
            event.workspace_id.0,
            &event_json,
        )
        .execute(&mut **tx)
        .await?;
        if result.rows_affected() == 0 {
            tracing::debug!(
                source = %event.source,
                event_id = %event.id,
                "duplicate integration event publish ignored"
            );
        }
        Ok(())
    }

    /// Claim a batch of events for one consumer.
    ///
    /// Two steps, each in its own transaction (claim atomicity per row is
    /// what matters):
    /// 1. re-claim existing delivery rows that are `pending` and available or
    ///    `claimed` with an expired lease;
    /// 2. first delivery: insert a delivery row for outbox events that have
    ///    no delivery row yet for this consumer.
    ///
    /// `subscriptions` is the concrete event-type list (the caller expands
    /// `.*` prefix patterns via `event_matches_subscription`); an empty list
    /// subscribes to everything.
    ///
    /// Every claim regenerates `claim_token` (`gen_random_uuid()`), so a
    /// stale worker holding an old token can never acknowledge the new lease
    /// (fencing). Rows whose `event_json` fails envelope re-validation are
    /// dead-lettered (redacted reason) and skipped, so a corrupt row cannot
    /// crash the dispatcher.
    pub async fn claim_batch(
        &self,
        consumer_id: &str,
        subscriptions: &[String],
        config: &OutboxConfig,
        worker_id: &str,
    ) -> Result<Vec<ClaimedEvent>, OutboxStoreError> {
        let subscription_filter: Option<&[String]> = if subscriptions.is_empty() {
            None
        } else {
            Some(subscriptions)
        };

        let mut rows = self
            .reclaim_existing(consumer_id, subscription_filter, config, worker_id)
            .await?;
        rows.extend(
            self.first_delivery(consumer_id, subscription_filter, config, worker_id)
                .await?,
        );

        // Defensive dedupe: a row cannot normally appear in both steps.
        let mut seen = std::collections::HashSet::new();
        let mut claimed = Vec::with_capacity(rows.len());
        for row in rows {
            if !seen.insert((row.source.clone(), row.event_id)) {
                continue;
            }
            let event = match serde_json::from_value::<IntegrationEvent>(row.event_json.clone()) {
                Ok(event) => event,
                Err(error) => {
                    self.poison_delivery(
                        consumer_id,
                        &row,
                        &format!("undecodable event_json: {error}"),
                    )
                    .await?;
                    continue;
                }
            };
            if let Err(error) = event.validate() {
                self.poison_delivery(
                    consumer_id,
                    &row,
                    &format!("invalid event envelope: {error}"),
                )
                .await?;
                continue;
            }
            claimed.push(ClaimedEvent {
                consumer_id: consumer_id.to_string(),
                source: row.source,
                event_id: row.event_id,
                claim_token: row.claim_token,
                attempt_count: row.attempt_count,
                event,
            });
        }
        Ok(claimed)
    }

    /// Step 1 of [`Self::claim_batch`]: re-claim existing pending or
    /// lease-expired delivery rows.
    async fn reclaim_existing(
        &self,
        consumer_id: &str,
        subscription_filter: Option<&[String]>,
        config: &OutboxConfig,
        worker_id: &str,
    ) -> Result<Vec<ClaimedRow>, OutboxStoreError> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, ClaimedRow>(
            r#"
            WITH updated AS (
                WITH candidates AS (
                    SELECT d.source, d.event_id
                    FROM integration_deliveries d
                    WHERE d.consumer_id = $1
                      AND ( (d.state = 'pending' AND d.available_at <= now())
                            OR (d.state = 'claimed' AND d.claim_expires_at <= now()) )
                      AND ($2::text[] IS NULL OR d.event_type = ANY($2))
                    ORDER BY d.available_at, d.event_id
                    LIMIT $3
                    FOR UPDATE OF d SKIP LOCKED
                )
                UPDATE integration_deliveries d
                SET state = 'claimed', claimed_by = $4, claim_token = gen_random_uuid(),
                    claim_expires_at = now() + ($5::int8 * interval '1 second'),
                    attempt_count = d.attempt_count + 1,
                    first_attempt_at = COALESCE(d.first_attempt_at, now()),
                    last_attempt_at = now()
                FROM candidates c
                WHERE d.consumer_id = $1 AND d.source = c.source AND d.event_id = c.event_id
                RETURNING d.source, d.event_id, d.claim_token, d.attempt_count
            )
            SELECT u.source, u.event_id, u.claim_token, u.attempt_count, o.event_json
            FROM updated u
            JOIN integration_outbox o ON o.source = u.source AND o.event_id = u.event_id
            "#,
        )
        .bind(consumer_id)
        .bind(subscription_filter)
        .bind(config.claim_batch_size)
        .bind(worker_id)
        .bind(config.lease_secs)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows)
    }

    /// Step 2 of [`Self::claim_batch`]: first delivery — insert a claimed
    /// delivery row for outbox events without a delivery row for this
    /// consumer.
    async fn first_delivery(
        &self,
        consumer_id: &str,
        subscription_filter: Option<&[String]>,
        config: &OutboxConfig,
        worker_id: &str,
    ) -> Result<Vec<ClaimedRow>, OutboxStoreError> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query_as::<_, ClaimedRow>(
            r#"
            WITH inserted AS (
                INSERT INTO integration_deliveries
                    (consumer_id, source, event_id, event_type, tenant_id, workspace_id,
                     state, available_at, claimed_by, claim_token, claim_expires_at,
                     attempt_count, first_attempt_at, last_attempt_at)
                SELECT $1, c.source, c.event_id, c.event_type, c.tenant_id, c.workspace_id,
                       'claimed', now(), $4, gen_random_uuid(),
                       now() + ($5::int8 * interval '1 second'), 1, now(), now()
                FROM (
                    SELECT o.source, o.event_id, o.event_type, o.tenant_id, o.workspace_id
                    FROM integration_outbox o
                    WHERE o.available_at <= now()
                      AND NOT EXISTS (
                          SELECT 1 FROM integration_deliveries d
                          WHERE d.consumer_id = $1 AND d.source = o.source AND d.event_id = o.event_id
                      )
                      AND ($2::text[] IS NULL OR o.event_type = ANY($2))
                    ORDER BY o.available_at, o.created_at
                    LIMIT $3
                    FOR UPDATE OF o SKIP LOCKED
                ) c
                ON CONFLICT (consumer_id, source, event_id) DO NOTHING
                RETURNING consumer_id, source, event_id, claim_token, attempt_count
            )
            SELECT i.source, i.event_id, i.claim_token, i.attempt_count, o.event_json
            FROM inserted i
            JOIN integration_outbox o ON o.source = i.source AND o.event_id = i.event_id
            "#,
        )
        .bind(consumer_id)
        .bind(subscription_filter)
        .bind(config.claim_batch_size)
        .bind(worker_id)
        .bind(config.lease_secs)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows)
    }

    /// Acknowledge a claimed delivery as processed. Returns `true` when the
    /// delivery was updated; `false` when the claim token no longer matches
    /// (lost lease) or the row is absent — the caller logs and moves on, this
    /// is not an error.
    pub async fn acknowledge(
        &self,
        consumer_id: &str,
        source: &str,
        event_id: Uuid,
        claim_token: Uuid,
    ) -> Result<bool, OutboxStoreError> {
        let result = sqlx::query!(
            r#"
            UPDATE integration_deliveries
            SET state = 'processed', processed_at = now(), claimed_by = NULL,
                claim_token = NULL, claim_expires_at = NULL, last_error = NULL
            WHERE consumer_id = $1 AND source = $2 AND event_id = $3 AND claim_token = $4
            "#,
            consumer_id,
            source,
            event_id,
            claim_token
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Mark a claimed delivery retryable with exponential backoff, or
    /// dead-letter it when attempts are exhausted.
    ///
    /// The delivery's `attempt_count` is already incremented at claim time;
    /// when it reaches `config.max_attempts` this method dead-letters the
    /// delivery instead of requeueing it (explicit, so the dispatcher needs
    /// no extra step). Backoff is `backoff_initial_ms * 2^(attempts - 1)`,
    /// capped at `backoff_max_ms`, at least 1 second.
    ///
    /// The reason is redacted before persistence. Returns `false` when the
    /// claim token no longer matches (lost lease).
    pub async fn fail_retryable(
        &self,
        consumer_id: &str,
        source: &str,
        event_id: Uuid,
        claim_token: Uuid,
        reason: &str,
        config: &OutboxConfig,
    ) -> Result<bool, OutboxStoreError> {
        let attempt_count: Option<i32> = sqlx::query_scalar!(
            r#"
            SELECT attempt_count FROM integration_deliveries
            WHERE consumer_id = $1 AND source = $2 AND event_id = $3 AND claim_token = $4
            "#,
            consumer_id,
            source,
            event_id,
            claim_token
        )
        .fetch_optional(&self.pool)
        .await?;
        let Some(attempt_count) = attempt_count else {
            return Ok(false); // lost lease or row absent
        };
        if attempt_count >= config.max_attempts {
            return self
                .dead_letter(consumer_id, source, event_id, claim_token, reason)
                .await;
        }
        let exponent = ((attempt_count - 1).max(0) as u32).min(63);
        let delay_ms = config
            .backoff_initial_ms
            .saturating_mul(1u64 << exponent)
            .min(config.backoff_max_ms)
            .max(1);
        let delay_secs = (delay_ms / 1000).max(1) as i64;
        let reason = redact_error(reason, 512);
        let result = sqlx::query!(
            r#"
            UPDATE integration_deliveries
            SET state = 'pending', claimed_by = NULL, claim_token = NULL, claim_expires_at = NULL,
                available_at = now() + ($5::int8 * interval '1 second'), last_error = $6
            WHERE consumer_id = $1 AND source = $2 AND event_id = $3 AND claim_token = $4
            "#,
            consumer_id,
            source,
            event_id,
            claim_token,
            delay_secs,
            &reason
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Dead-letter a claimed delivery. The reason is redacted before
    /// persistence. Returns `false` when the claim token no longer matches
    /// (lost lease).
    pub async fn dead_letter(
        &self,
        consumer_id: &str,
        source: &str,
        event_id: Uuid,
        claim_token: Uuid,
        reason: &str,
    ) -> Result<bool, OutboxStoreError> {
        let reason = redact_error(reason, 512);
        let result = sqlx::query!(
            r#"
            UPDATE integration_deliveries
            SET state = 'dead_lettered', dead_lettered_at = now(), claimed_by = NULL,
                claim_token = NULL, claim_expires_at = NULL, last_error = $5
            WHERE consumer_id = $1 AND source = $2 AND event_id = $3 AND claim_token = $4
            "#,
            consumer_id,
            source,
            event_id,
            claim_token,
            &reason
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Requeue a dead-lettered delivery as pending (operator repair).
    ///
    /// Only rows in state `dead_lettered` are requeued; `attempt_count` is
    /// reset to 0, `first_attempt_at` history is preserved, and no new event
    /// identity is generated.
    pub async fn requeue(
        &self,
        consumer_id: &str,
        source: &str,
        event_id: Uuid,
    ) -> Result<bool, OutboxStoreError> {
        let result = sqlx::query!(
            r#"
            UPDATE integration_deliveries
            SET state = 'pending', available_at = now(), claim_token = NULL,
                claimed_by = NULL, claim_expires_at = NULL, attempt_count = 0,
                last_error = NULL, dead_lettered_at = NULL
            WHERE consumer_id = $1 AND source = $2 AND event_id = $3 AND state = 'dead_lettered'
            "#,
            consumer_id,
            source,
            event_id
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Per-consumer delivery counts grouped by state (`pending`, `claimed`,
    /// `processed`, `dead_lettered`). Callers filter by state as needed.
    pub async fn pending_counts(&self) -> Result<Vec<ConsumerCount>, OutboxStoreError> {
        let rows = sqlx::query_as::<_, ConsumerCount>(
            r#"
            SELECT consumer_id, state, count(*)::bigint AS count
            FROM integration_deliveries
            GROUP BY consumer_id, state
            ORDER BY consumer_id, state
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Age in seconds of the oldest pending delivery, `None` when there are
    /// no pending deliveries. Optionally scoped to one consumer.
    pub async fn oldest_pending_age_seconds(
        &self,
        consumer_id: Option<&str>,
    ) -> Result<Option<f64>, OutboxStoreError> {
        let age = match consumer_id {
            Some(consumer_id) => {
                sqlx::query_scalar!(
                    r#"
                    SELECT EXTRACT(EPOCH FROM (now() - min(available_at)))::float8
                    FROM integration_deliveries
                    WHERE state = 'pending' AND consumer_id = $1
                    "#,
                    consumer_id
                )
                .fetch_one(&self.pool)
                .await?
            }
            None => {
                sqlx::query_scalar!(
                    r#"
                    SELECT EXTRACT(EPOCH FROM (now() - min(available_at)))::float8
                    FROM integration_deliveries
                    WHERE state = 'pending'
                    "#,
                )
                .fetch_one(&self.pool)
                .await?
            }
        };
        Ok(age)
    }

    /// Total number of dead-lettered deliveries.
    pub async fn dlq_count(&self) -> Result<i64, OutboxStoreError> {
        let count = sqlx::query_scalar!(
            r#"
            SELECT count(*)::bigint FROM integration_deliveries WHERE state = 'dead_lettered'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count.unwrap_or(0))
    }

    /// List dead-lettered deliveries (metadata only — no `event_json` in the
    /// result, safe for operators). Optionally scoped to one consumer.
    pub async fn list_dead_letters(
        &self,
        consumer_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<DeadLetterEntry>, OutboxStoreError> {
        let rows = sqlx::query_as::<_, DeadLetterEntry>(
            r#"
            SELECT consumer_id, source, event_id, event_type, tenant_id, workspace_id,
                   attempt_count, first_attempt_at, last_attempt_at, dead_lettered_at, last_error
            FROM integration_deliveries
            WHERE state = 'dead_lettered' AND ($1::text IS NULL OR consumer_id = $1)
            ORDER BY dead_lettered_at DESC NULLS LAST
            LIMIT $2
            "#,
        )
        .bind(consumer_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Compact outbox rows older than `retention_hours` whose deliveries are
    /// all `processed` (or gone). Returns the number of deleted outbox rows.
    ///
    /// The FK cascade removes the processed delivery rows; `integration_consumer_receipts`
    /// rows are deliberately not FK-linked and remain as harmless durable
    /// idempotency records. Pass `retention_hours <= 0` to disable.
    pub async fn maintenance(&self, retention_hours: i64) -> Result<u64, OutboxStoreError> {
        if retention_hours <= 0 {
            return Ok(0);
        }
        let result = sqlx::query!(
            r#"
            WITH expired AS (
                SELECT o.source, o.event_id
                FROM integration_outbox o
                WHERE o.created_at < now() - ($1::int8 * interval '1 hour')
                  AND NOT EXISTS (
                      SELECT 1 FROM integration_deliveries d
                      WHERE d.source = o.source AND d.event_id = o.event_id AND d.state <> 'processed'
                  )
            )
            DELETE FROM integration_outbox o
            USING expired e
            WHERE o.source = e.source AND o.event_id = e.event_id
            "#,
            retention_hours
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Dead-letter a claimed row whose envelope failed re-validation and skip
    /// it. Logs the redacted reason; a corrupt row must not crash the
    /// dispatcher.
    async fn poison_delivery(
        &self,
        consumer_id: &str,
        row: &ClaimedRow,
        reason: &str,
    ) -> Result<(), OutboxStoreError> {
        let reason = redact_error(reason, 512);
        let error = OutboxStoreError::PoisonDelivery {
            source: row.source.clone(),
            event_id: row.event_id,
            reason: reason.clone(),
        };
        tracing::warn!(%consumer_id, %error, "skipping poison integration delivery");
        self.dead_letter(
            consumer_id,
            &row.source,
            row.event_id,
            row.claim_token,
            &reason,
        )
        .await?;
        Ok(())
    }
}

/// Files integration-event publisher adapter wired into `FileService`
/// (rustshare-core seam): builds the envelope from the neutral facts and
/// inserts it atomically into the outbox.
#[async_trait::async_trait]
impl IntegrationEventPublisher<Transaction<'static, Postgres>> for OutboxStore {
    async fn publish_in_tx(
        &self,
        tx: &mut Transaction<'static, Postgres>,
        facts: &IntegrationEventFacts<'_>,
    ) -> Result<(), IntegrationPublishError> {
        // The FileService seam only emits Files integration events today
        // (v1alpha1); the source application is the Files application.
        let application = ApplicationId::new("io.elembra.files");
        let resource =
            ResourceRef::new(application.clone(), facts.resource_type, facts.resource_id);
        let resource = match facts.version {
            Some(version) => resource.with_version(version),
            None => resource,
        };
        let event = IntegrationEvent::builder()
            .source(format!("elembra://{application}"))
            .r#type(facts.event_type)
            .subject(resource.to_uri())
            .tenant_id(TenantId(facts.tenant_id))
            .workspace_id(WorkspaceId(facts.workspace_id))
            .actor(ActorRef::Principal(PrincipalId(facts.actor_user_id)))
            .resource(resource)
            .data(facts.data.clone())
            .build()
            .map_err(|error| IntegrationPublishError::InvalidEvent(error.to_string()))?;
        self.insert_in_tx(tx, &event).await.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::{ApplicationRegistry, PrincipalId, TenantId, WorkspaceId};
    use rustshare_integration_events::event::{ActorRef, IntegrationEvent};
    use serde_json::json;
    use sqlx::Row;

    const TEST_DATABASE_URL: &str = "postgres://rustshare:changeme@localhost:5432/rustshare";

    /// The DB tests share one dev database (its outbox tables are only used
    /// by these tests), so they must run serially: a claim with an empty
    /// subscription picks up every pending event, including another test's.
    static DB_TEST_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

    async fn setup() -> (OutboxStore, PgPool) {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());
        let pool = PgPool::connect(&database_url).await.unwrap();
        let registry = Arc::new(ApplicationRegistry::first_party().unwrap());
        (OutboxStore::new(pool.clone(), registry), pool)
    }

    fn test_event(event_type: &str) -> IntegrationEvent {
        let tenant = TenantId(Uuid::new_v4());
        IntegrationEvent::builder()
            .source("elembra://io.elembra.files")
            .r#type(event_type)
            .tenant_id(tenant)
            .workspace_id(WorkspaceId(tenant.0))
            .actor(ActorRef::Principal(PrincipalId(Uuid::new_v4())))
            .data(json!({"name": "test.txt", "mime_type": "text/plain", "size": 4}))
            .build()
            .unwrap()
    }

    async fn cleanup(pool: &PgPool, events: &[&IntegrationEvent]) {
        for event in events {
            sqlx::query("DELETE FROM integration_outbox WHERE source = $1 AND event_id = $2")
                .bind(&event.source)
                .bind(event.id)
                .execute(pool)
                .await
                .unwrap();
        }
    }

    /// Empty the outbox tables. Safe because the DB tests are serialized by
    /// `DB_TEST_LOCK`; rows left behind by an aborted previous run would
    /// otherwise leak into another test's empty-subscription claim.
    async fn clean_slate(pool: &PgPool) {
        sqlx::query("DELETE FROM integration_deliveries")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM integration_outbox")
            .execute(pool)
            .await
            .unwrap();
    }

    async fn delivery_state(pool: &PgPool, consumer_id: &str, event: &IntegrationEvent) -> String {
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM integration_deliveries WHERE consumer_id = $1 AND source = $2 AND event_id = $3",
        )
        .bind(consumer_id)
        .bind(&event.source)
        .bind(event.id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn insert_and_select_back_round_trips() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let event = test_event("io.elembra.files.file.created.v1");
        clean_slate(&pool).await;

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        let row = sqlx::query(
            "SELECT source, event_id, event_type, application_id, tenant_id, workspace_id, event_json FROM integration_outbox WHERE source = $1 AND event_id = $2",
        )
        .bind(&event.source)
        .bind(event.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.try_get::<String, _>("source").unwrap(),
            "elembra://io.elembra.files"
        );
        assert_eq!(
            row.try_get::<String, _>("event_type").unwrap(),
            "io.elembra.files.file.created.v1"
        );
        assert_eq!(
            row.try_get::<String, _>("application_id").unwrap(),
            "io.elembra.files"
        );
        assert_eq!(
            row.try_get::<Uuid, _>("tenant_id").unwrap(),
            event.tenant_id.0
        );
        assert_eq!(
            row.try_get::<Uuid, _>("workspace_id").unwrap(),
            event.workspace_id.0
        );
        let parsed: IntegrationEvent =
            serde_json::from_value(row.try_get::<serde_json::Value, _>("event_json").unwrap())
                .unwrap();
        assert_eq!(parsed, event);

        cleanup(&pool, &[&event]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn tenant_workspace_mismatch_is_rejected() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let mut event = test_event("io.elembra.files.file.created.v1");
        event.workspace_id = WorkspaceId(Uuid::new_v4());
        clean_slate(&pool).await;

        let mut tx = store.pool().begin().await.unwrap();
        let result = store.insert_in_tx(&mut tx, &event).await;
        tx.rollback().await.unwrap();
        assert!(matches!(result, Err(OutboxStoreError::InvalidEvent(_))));

        cleanup(&pool, &[&event]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn event_type_ownership_is_enforced() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let owned = test_event("io.elembra.files.file.created.v1");
        clean_slate(&pool).await;
        // Passes envelope validation (files segment matches the source) but
        // is not declared in the Files manifest's publishes.
        let unowned = test_event("io.elembra.files.share.revoked.v1");

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &owned).await.unwrap();
        let result = store.insert_in_tx(&mut tx, &unowned).await;
        tx.rollback().await.unwrap();
        assert!(matches!(
            result,
            Err(OutboxStoreError::OwnershipRejected(_))
        ));

        cleanup(&pool, &[&owned, &unowned]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn rollback_drops_the_outbox_row() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let event = test_event("io.elembra.files.file.created.v1");
        clean_slate(&pool).await;

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        drop(tx); // rollback without commit

        let count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_outbox WHERE source = $1 AND event_id = $2",
        )
        .bind(&event.source)
        .bind(event.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);

        cleanup(&pool, &[&event]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn claim_first_delivery_filters_by_subscription() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let created = test_event("io.elembra.files.file.created.v1");
        let updated = test_event("io.elembra.files.file.updated.v1");
        let consumer_id = format!("io.elembra.test.claim-filter-{}", Uuid::new_v4());
        clean_slate(&pool).await;

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &created).await.unwrap();
        store.insert_in_tx(&mut tx, &updated).await.unwrap();
        tx.commit().await.unwrap();

        let config = OutboxConfig::default();
        let claimed = store
            .claim_batch(
                &consumer_id,
                &["io.elembra.files.file.created.v1".to_string()],
                &config,
                "test-worker",
            )
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].event, created);
        assert_eq!(claimed[0].consumer_id, consumer_id);
        assert_eq!(claimed[0].attempt_count, 1);
        assert!(!claimed[0].claim_token.is_nil());
        assert_eq!(
            delivery_state(&pool, &consumer_id, &created).await,
            "claimed"
        );
        // The non-matching event has no delivery row yet.
        let delivery_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_deliveries WHERE consumer_id = $1",
        )
        .bind(&consumer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(delivery_count, 1);

        sqlx::query("DELETE FROM integration_deliveries WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&created, &updated]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn empty_subscriptions_claim_everything() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let created = test_event("io.elembra.files.file.created.v1");
        let updated = test_event("io.elembra.files.file.updated.v1");
        let consumer_id = format!("io.elembra.test.claim-all-{}", Uuid::new_v4());
        clean_slate(&pool).await;

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &created).await.unwrap();
        store.insert_in_tx(&mut tx, &updated).await.unwrap();
        tx.commit().await.unwrap();

        let claimed = store
            .claim_batch(&consumer_id, &[], &OutboxConfig::default(), "test-worker")
            .await
            .unwrap();
        assert_eq!(claimed.len(), 2);

        sqlx::query("DELETE FROM integration_deliveries WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&created, &updated]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn acknowledge_fencing_rejects_stale_tokens() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let event = test_event("io.elembra.files.file.created.v1");
        let consumer_id = format!("io.elembra.test.fencing-{}", Uuid::new_v4());
        clean_slate(&pool).await;

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        let claimed = store
            .claim_batch(&consumer_id, &[], &OutboxConfig::default(), "worker-a")
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        let claimed = &claimed[0];

        // Wrong token: rejected, delivery stays claimed.
        assert!(!store
            .acknowledge(
                &consumer_id,
                &claimed.source,
                claimed.event_id,
                Uuid::new_v4()
            )
            .await
            .unwrap());
        assert_eq!(delivery_state(&pool, &consumer_id, &event).await, "claimed");

        // Right token: acknowledged.
        assert!(store
            .acknowledge(
                &consumer_id,
                &claimed.source,
                claimed.event_id,
                claimed.claim_token
            )
            .await
            .unwrap());
        assert_eq!(
            delivery_state(&pool, &consumer_id, &event).await,
            "processed"
        );

        // Acknowledge again with the same token: token was cleared, so false.
        assert!(!store
            .acknowledge(
                &consumer_id,
                &claimed.source,
                claimed.event_id,
                claimed.claim_token
            )
            .await
            .unwrap());

        sqlx::query("DELETE FROM integration_deliveries WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&event]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn lease_expiry_reclaims_with_new_token_and_incremented_attempts() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let event = test_event("io.elembra.files.file.created.v1");
        let consumer_id = format!("io.elembra.test.lease-{}", Uuid::new_v4());
        clean_slate(&pool).await;

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        let first = store
            .claim_batch(&consumer_id, &[], &OutboxConfig::default(), "worker-a")
            .await
            .unwrap();
        assert_eq!(first.len(), 1);
        let first_token = first[0].claim_token;

        // Simulate a crashed worker: the lease expires while still claimed.
        sqlx::query(
            "UPDATE integration_deliveries SET state = 'claimed', claim_expires_at = now() - interval '1 second' WHERE consumer_id = $1",
        )
        .bind(&consumer_id)
        .execute(&pool)
        .await
        .unwrap();

        let second = store
            .claim_batch(&consumer_id, &[], &OutboxConfig::default(), "worker-b")
            .await
            .unwrap();
        assert_eq!(second.len(), 1);
        assert_ne!(
            second[0].claim_token, first_token,
            "token must rotate on re-claim"
        );
        assert_eq!(second[0].attempt_count, 2);
        // The stale worker's token must no longer acknowledge the delivery.
        assert!(!store
            .acknowledge(&consumer_id, &event.source, event.id, first_token)
            .await
            .unwrap());

        sqlx::query("DELETE FROM integration_deliveries WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&event]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn fail_retryable_backs_off_then_dead_letters_on_exhaustion_and_requeues() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let event = test_event("io.elembra.files.file.created.v1");
        let consumer_id = format!("io.elembra.test.retry-{}", Uuid::new_v4());
        clean_slate(&pool).await;
        let config = OutboxConfig::default();
        let secret = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJzZWNyZXQifQ.signature";

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        let claimed = store
            .claim_batch(&consumer_id, &[], &config, "worker")
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        let claimed = &claimed[0];

        // First failure: retryable with backoff; reason redacted.
        assert!(store
            .fail_retryable(
                &consumer_id,
                &claimed.source,
                claimed.event_id,
                claimed.claim_token,
                &format!("transient failure: {secret}"),
                &config
            )
            .await
            .unwrap());
        let row = sqlx::query(
            "SELECT state, available_at, attempt_count, last_error FROM integration_deliveries WHERE consumer_id = $1 AND event_id = $2",
        )
        .bind(&consumer_id)
        .bind(event.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.try_get::<String, _>("state").unwrap(), "pending");
        assert!(
            row.try_get::<chrono::DateTime<Utc>, _>("available_at")
                .unwrap()
                > chrono::Utc::now() - chrono::Duration::seconds(2)
        );
        assert_eq!(row.try_get::<i32, _>("attempt_count").unwrap(), 1);
        let last_error = row
            .try_get::<Option<String>, _>("last_error")
            .unwrap()
            .unwrap();
        assert!(
            !last_error.contains("eyJhbGci"),
            "secret leaked into last_error: {last_error}"
        );
        assert!(last_error.contains("[REDACTED]"));

        // Exhaust attempts: force the count to max and fail again.
        sqlx::query(
            "UPDATE integration_deliveries SET attempt_count = $1, available_at = now() WHERE consumer_id = $2 AND event_id = $3",
        )
        .bind(config.max_attempts)
        .bind(&consumer_id)
        .bind(event.id)
        .execute(&pool)
        .await
        .unwrap();
        let claimed = store
            .claim_batch(&consumer_id, &[], &config, "worker")
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        assert!(store
            .fail_retryable(
                &consumer_id,
                &claimed[0].source,
                claimed[0].event_id,
                claimed[0].claim_token,
                "exhausted",
                &config
            )
            .await
            .unwrap());
        assert_eq!(
            delivery_state(&pool, &consumer_id, &event).await,
            "dead_lettered"
        );

        // Requeue: pending again with attempt_count reset and history kept.
        assert!(store
            .requeue(&consumer_id, &event.source, event.id)
            .await
            .unwrap());
        let row = sqlx::query(
            "SELECT state, attempt_count, last_error, dead_lettered_at, first_attempt_at FROM integration_deliveries WHERE consumer_id = $1 AND event_id = $2",
        )
        .bind(&consumer_id)
        .bind(event.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.try_get::<String, _>("state").unwrap(), "pending");
        assert_eq!(row.try_get::<i32, _>("attempt_count").unwrap(), 0);
        assert!(row
            .try_get::<Option<String>, _>("last_error")
            .unwrap()
            .is_none());
        assert!(row
            .try_get::<Option<chrono::DateTime<Utc>>, _>("dead_lettered_at")
            .unwrap()
            .is_none());
        assert!(
            row.try_get::<Option<chrono::DateTime<Utc>>, _>("first_attempt_at")
                .unwrap()
                .is_some(),
            "first_attempt_at history preserved"
        );
        // Requeue again from pending is a no-op.
        assert!(!store
            .requeue(&consumer_id, &event.source, event.id)
            .await
            .unwrap());

        sqlx::query("DELETE FROM integration_deliveries WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&event]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn duplicate_publish_is_idempotent() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let event = test_event("io.elembra.files.file.created.v1");
        clean_slate(&pool).await;

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        let count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_outbox WHERE source = $1 AND event_id = $2",
        )
        .bind(&event.source)
        .bind(event.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);

        cleanup(&pool, &[&event]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn publisher_trait_builds_envelope_from_facts() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let tenant = Uuid::new_v4();
        let actor = Uuid::new_v4();
        let facts = IntegrationEventFacts {
            event_type: "io.elembra.files.file.created.v1",
            resource_type: "file",
            resource_id: &Uuid::new_v4().to_string(),
            version: Some("sha256:0123abcdef"),
            data: json!({"name": "facts.txt", "mime_type": "text/plain", "size": 9}),
            tenant_id: tenant,
            workspace_id: tenant,
            actor_user_id: actor,
        };

        let mut tx = store.pool().begin().await.unwrap();
        store
            .publish_in_tx(&mut tx, &facts)
            .await
            .map_err(|e| e.to_string())
            .unwrap();
        tx.commit().await.unwrap();

        let event_json = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT event_json FROM integration_outbox WHERE source = 'elembra://io.elembra.files' AND application_id = 'io.elembra.files' ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let event: IntegrationEvent = serde_json::from_value(event_json).unwrap();
        event.validate().unwrap();
        assert_eq!(event.r#type, "io.elembra.files.file.created.v1");
        assert_eq!(event.tenant_id.0, tenant);
        assert_eq!(event.actor, Some(ActorRef::Principal(PrincipalId(actor))));
        let resource = event.resource.unwrap();
        assert_eq!(resource.application.0, "io.elembra.files");
        assert_eq!(resource.resource_type, "file");
        assert_eq!(resource.version.as_deref(), Some("sha256:0123abcdef"));
        assert_eq!(event.data["name"], "facts.txt");
        assert_eq!(event.subject, Some(resource.to_uri()));

        sqlx::query("DELETE FROM integration_outbox WHERE source = $1 AND event_id = $2")
            .bind(&event.source)
            .bind(event.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn observability_and_maintenance() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let created = test_event("io.elembra.files.file.created.v1");
        let updated = test_event("io.elembra.files.file.updated.v1");
        let consumer_id = format!("io.elembra.test.observability-{}", Uuid::new_v4());
        clean_slate(&pool).await;

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &created).await.unwrap();
        store.insert_in_tx(&mut tx, &updated).await.unwrap();
        tx.commit().await.unwrap();

        let config = OutboxConfig::default();
        let claimed = store
            .claim_batch(&consumer_id, &[], &config, "worker")
            .await
            .unwrap();
        assert_eq!(claimed.len(), 2);

        // One processed, one failed (retryable → pending).
        let first = &claimed[0];
        store
            .acknowledge(
                &consumer_id,
                &first.source,
                first.event_id,
                first.claim_token,
            )
            .await
            .unwrap();
        let second = &claimed[1];
        store
            .fail_retryable(
                &consumer_id,
                &second.source,
                second.event_id,
                second.claim_token,
                "transient",
                &config,
            )
            .await
            .unwrap();

        let counts = store.pending_counts().await.unwrap();
        let consumer_counts: Vec<_> = counts
            .iter()
            .filter(|c| c.consumer_id == consumer_id)
            .collect();
        assert_eq!(consumer_counts.len(), 2); // processed + pending
        assert_eq!(
            consumer_counts
                .iter()
                .find(|c| c.state == "pending")
                .unwrap()
                .count,
            1
        );
        assert_eq!(
            consumer_counts
                .iter()
                .find(|c| c.state == "processed")
                .unwrap()
                .count,
            1
        );

        let age = store
            .oldest_pending_age_seconds(Some(&consumer_id))
            .await
            .unwrap();
        // The pending delivery's available_at sits 1s in the future (backoff),
        // so the age is a small negative number; just assert it is present
        // and bounded.
        let age = age.unwrap();
        assert!(age > -5.0 && age < 60.0, "unexpected age {age}");

        assert_eq!(store.dlq_count().await.unwrap(), 0);
        assert!(store
            .list_dead_letters(Some(&consumer_id), 10)
            .await
            .unwrap()
            .is_empty());

        // Maintenance with a tiny retention window compacts nothing while a
        // delivery is pending.
        assert_eq!(store.maintenance(0).await.unwrap(), 0, "disabled retention");
        assert_eq!(
            store.maintenance(1).await.unwrap(),
            0,
            "pending delivery blocks compaction"
        );

        sqlx::query("DELETE FROM integration_deliveries WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&created, &updated]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn poison_deliveries_are_dead_lettered_and_skipped() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let event = test_event("io.elembra.files.file.created.v1");
        let consumer_id = format!("io.elembra.test.poison-{}", Uuid::new_v4());
        clean_slate(&pool).await;

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        // Corrupt the stored envelope so claim-time re-validation fails.
        sqlx::query(
            "UPDATE integration_outbox SET event_json = '{\"not\":\"an envelope\"}' WHERE source = $1 AND event_id = $2",
        )
        .bind(&event.source)
        .bind(event.id)
        .execute(&pool)
        .await
        .unwrap();

        let claimed = store
            .claim_batch(&consumer_id, &[], &OutboxConfig::default(), "worker")
            .await
            .unwrap();
        assert!(claimed.is_empty(), "poison row must be skipped");
        assert_eq!(
            delivery_state(&pool, &consumer_id, &event).await,
            "dead_lettered"
        );

        let dlq = store
            .list_dead_letters(Some(&consumer_id), 10)
            .await
            .unwrap();
        assert_eq!(dlq.len(), 1);
        let last_error = dlq[0].last_error.as_deref().unwrap_or_default();
        assert!(
            last_error.contains("undecodable event_json"),
            "expected poison diagnostic, got: {last_error}"
        );
        assert!(!last_error.contains("envelope"));

        sqlx::query("DELETE FROM integration_deliveries WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&event]).await;
    }
}
