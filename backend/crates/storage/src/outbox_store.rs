//! Transactional integration outbox (ADR-0031).
//!
//! PostgreSQL-backed durable transport for Integration Events:
//!
//! * [`OutboxStore::insert_in_tx`] writes an outbox row atomically inside the
//!   source mutation's transaction (the mutation can never commit without its
//!   event) and eagerly creates pending delivery obligations for every
//!   registered consumer whose subscriptions match the event type;
//! * [`OutboxStore::register_consumer`] / [`OutboxStore::set_consumer_enabled`]
//!   durably register consumers and their subscription patterns — the durable
//!   registration is authoritative for both fan-out and claiming;
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
    valid_event_type, ApplicationId, ApplicationRegistry, PrincipalId, TenantId, WorkspaceId,
};
use rustshare_core::services::{
    IntegrationEventActor, IntegrationEventFacts, IntegrationEventPublisher,
    IntegrationPublishError,
};
use rustshare_integration_events::consumer::event_matches_subscription;
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
    /// A consumer subscription pattern is not valid event-type syntax
    /// (exact type or `prefix.*`).
    InvalidSubscription(String),
    /// A durable consumer must declare at least one subscription pattern.
    /// An empty pattern list cannot be discovered at eager fan-out, so no
    /// durable obligation would ever be created — registration is rejected
    /// and no consumer row is written.
    EmptySubscriptionList(String),
    /// Re-registering an existing consumer with a different subscription
    /// set. v1alpha1 subscription contracts are immutable: a changed
    /// contract requires a new (versioned) consumer identity or a future
    /// migration API. No consumer or subscription row is changed.
    ConsumerRegistrationConflict { consumer_id: String },
    /// A duplicate publish reused a `(source, event_id)` that already exists
    /// with a different payload — the caller's transaction must be rolled
    /// back (this store does not partially roll back the caller's tx).
    EventIdentityConflict { source: String, event_id: Uuid },
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
            OutboxStoreError::InvalidSubscription(pattern) => {
                write!(f, "invalid consumer subscription pattern: {pattern}")
            }
            OutboxStoreError::EmptySubscriptionList(consumer_id) => {
                write!(
                    f,
                    "consumer {consumer_id} must declare at least one subscription pattern (empty subscriptions are rejected)"
                )
            }
            OutboxStoreError::ConsumerRegistrationConflict { consumer_id } => {
                write!(
                    f,
                    "consumer {consumer_id} is already registered with a different subscription set (v1alpha1 subscription contracts are immutable)"
                )
            }
            OutboxStoreError::EventIdentityConflict { source, event_id } => {
                write!(f, "event identity conflict for {source}/{event_id}: a publish reused the event id with a different payload")
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
            OutboxStoreError::InvalidSubscription(message) => {
                IntegrationPublishError::InvalidEvent(message)
            }
            // Registration-only errors: never produced on the publish path
            // (the conversion exists solely for `insert_in_tx`); mapped to
            // `Persistence` for exhaustiveness.
            OutboxStoreError::EmptySubscriptionList(consumer_id) => {
                IntegrationPublishError::Persistence(format!(
                    "consumer {consumer_id} must declare at least one subscription pattern"
                ))
            }
            OutboxStoreError::ConsumerRegistrationConflict { consumer_id } => {
                IntegrationPublishError::Persistence(format!(
                    "consumer {consumer_id} registration conflict: v1alpha1 subscription contracts are immutable"
                ))
            }
            OutboxStoreError::EventIdentityConflict { source, event_id } => {
                IntegrationPublishError::Persistence(format!(
                    "event identity conflict for {source}/{event_id}: event id reused with a different payload"
                ))
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

/// Durable registration of one integration-event consumer.
///
/// The registration table is authoritative for both eager fan-out at publish
/// time and claim filtering: an unregistered consumer receives no obligations
/// and is never claimed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumerRegistration {
    pub consumer_id: String,
    /// Claim gating only — obligations are created regardless of `enabled`.
    pub enabled: bool,
    /// Subscription patterns (exact event types or `prefix.*`), sorted.
    /// Never empty: durable registration requires at least one explicit
    /// pattern (see [`Self::register_consumer`]).
    pub subscriptions: Vec<String>,
    /// When the consumer was first registered; never updated by re-registration.
    pub registered_at: DateTime<Utc>,
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
    /// type, insert the outbox row atomically inside `tx`, and eagerly create
    /// a pending delivery obligation for every registered consumer whose
    /// subscription patterns match the event type (same connection — the
    /// obligations are atomic with the outbox row).
    ///
    /// Idempotency: republishing the same `(source, event_id)` with an
    /// identical payload succeeds silently (the duplicate insert is a no-op
    /// and obligations are re-ensured via `ON CONFLICT DO NOTHING`);
    /// republishing with a *different* payload fails with
    /// [`OutboxStoreError::EventIdentityConflict`] and the caller must roll
    /// back its transaction.
    ///
    /// Obligations are created for registered consumers regardless of their
    /// `enabled` flag — `enabled` only gates claiming, so a disabled or
    /// offline consumer never loses events.
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
            // Event-id idempotency hardening: a duplicate (source, event_id)
            // must carry the identical payload. Anything else is a call-site
            // bug and fails the caller's transaction (no partial rollback
            // attempted here — the caller owns the tx).
            let existing = sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT event_json FROM integration_outbox WHERE source = $1 AND event_id = $2",
            )
            .bind(&event.source)
            .bind(event.id)
            .fetch_optional(&mut **tx)
            .await?;
            if let Some(existing) = existing {
                if existing != event_json {
                    return Err(OutboxStoreError::EventIdentityConflict {
                        source: event.source.clone(),
                        event_id: event.id,
                    });
                }
                tracing::debug!(
                    source = %event.source,
                    event_id = %event.id,
                    "duplicate integration event publish ignored (identical payload)"
                );
            }
        }
        self.fan_out_obligations(tx, event).await?;
        Ok(())
    }

    /// Eager fan-out: insert a `pending` delivery obligation for every
    /// registered consumer whose subscription patterns match `event`'s type,
    /// inside the caller's transaction (same connection).
    ///
    /// `enabled` is deliberately NOT consulted: obligations must exist so a
    /// disabled or offline consumer can catch up after re-enablement.
    ///
    /// Only consumers registered at or before the event row's `created_at`
    /// gain obligations — registration establishes entitlement going
    /// forward, never retroactively (ADR-0031). The gate closes two leaks:
    /// the duplicate-identical-publish path (a consumer registered after the
    /// original event must not gain an obligation when the identical event is
    /// republished later) and a narrow publish/registration race (a
    /// registration committing mid-publish must not backfill an event that
    /// already existed). The normal publish path is unaffected: every
    /// pre-existing matching consumer has `registered_at <= created_at`.
    async fn fan_out_obligations(
        &self,
        tx: &mut Transaction<'static, Postgres>,
        event: &IntegrationEvent,
    ) -> Result<(), OutboxStoreError> {
        // The outbox row was inserted by the caller in this same transaction;
        // on the duplicate path `created_at` is preserved from the original
        // insert, which is exactly the event's authoritative creation time.
        let event_created_at: DateTime<Utc> = sqlx::query_scalar!(
            r#"
            SELECT created_at FROM integration_outbox WHERE source = $1 AND event_id = $2
            "#,
            &event.source,
            event.id,
        )
        .fetch_one(&mut **tx)
        .await?;
        let subscriptions = sqlx::query!(
            r#"
            SELECT s.consumer_id, s.pattern
            FROM integration_consumer_subscriptions s
            JOIN integration_consumers c ON c.consumer_id = s.consumer_id
            WHERE c.registered_at <= $1
            ORDER BY s.consumer_id, s.pattern
            "#,
            event_created_at,
        )
        .fetch_all(&mut **tx)
        .await?;
        let mut by_consumer: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for subscription in subscriptions {
            by_consumer
                .entry(subscription.consumer_id)
                .or_default()
                .push(subscription.pattern);
        }
        for (consumer_id, patterns) in by_consumer {
            if !event_matches_subscription(&event.r#type, &patterns) {
                continue;
            }
            sqlx::query!(
                r#"
                INSERT INTO integration_deliveries
                    (consumer_id, source, event_id, event_type, tenant_id, workspace_id,
                     state, available_at)
                VALUES ($1, $2, $3, $4, $5, $6, 'pending', now())
                ON CONFLICT (consumer_id, source, event_id) DO NOTHING
                "#,
                consumer_id,
                &event.source,
                event.id,
                &event.r#type,
                event.tenant_id.0,
                event.workspace_id.0,
            )
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    /// Register a consumer, or re-register it with the identical
    /// subscription set. The whole operation runs on one transaction (one
    /// connection), so it is atomic with respect to concurrent publishes
    /// and registrations.
    ///
    /// Every durable consumer MUST declare at least one explicit
    /// subscription pattern (an exact event type or a `.*`-terminated
    /// prefix); an empty list is rejected with
    /// [`OutboxStoreError::EmptySubscriptionList`] and no consumer row is
    /// created. An empty pattern set cannot be discovered at eager fan-out,
    /// so no durable obligation would ever be created — broad consumers use
    /// an explicit prefix such as `io.elembra.*`.
    ///
    /// v1alpha1 subscription contracts are immutable:
    /// * an unknown consumer is created with `enabled = true` and the given
    ///   subscription set;
    /// * re-registering an existing consumer with the identical normalized
    ///   subscription set (sorted + deduplicated) is an idempotent success
    ///   — `registered_at` and `enabled` are preserved unchanged (nothing
    ///   is written);
    /// * re-registering with a different subscription set fails with
    ///   [`OutboxStoreError::ConsumerRegistrationConflict`] and changes
    ///   neither the consumer row nor the subscription rows. A changed
    ///   contract requires a new (versioned) consumer identity or a future
    ///   migration API.
    ///
    /// Patterns are exact event types or `.*`-terminated prefixes (see
    /// [`valid_subscription_pattern`]).
    pub async fn register_consumer(
        &self,
        consumer_id: &str,
        subscriptions: &[String],
    ) -> Result<(), OutboxStoreError> {
        for pattern in subscriptions {
            if !valid_subscription_pattern(pattern) {
                return Err(OutboxStoreError::InvalidSubscription(pattern.clone()));
            }
        }
        if subscriptions.is_empty() {
            return Err(OutboxStoreError::EmptySubscriptionList(
                consumer_id.to_string(),
            ));
        }
        let mut subscriptions: Vec<String> = subscriptions.to_vec();
        subscriptions.sort();
        subscriptions.dedup();

        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query!(
            r#"
            INSERT INTO integration_consumers (consumer_id, enabled, registered_at, updated_at)
            VALUES ($1, true, now(), now())
            ON CONFLICT (consumer_id) DO NOTHING
            "#,
            consumer_id
        )
        .execute(&mut *tx)
        .await?;

        if inserted.rows_affected() == 1 {
            // First registration: create the consumer and its subscription
            // rows atomically.
            for pattern in &subscriptions {
                sqlx::query!(
                    r#"
                    INSERT INTO integration_consumer_subscriptions (consumer_id, pattern)
                    VALUES ($1, $2)
                    ON CONFLICT (consumer_id, pattern) DO NOTHING
                    "#,
                    consumer_id,
                    pattern
                )
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await?;
            return Ok(());
        }

        // Existing consumer (or a concurrent creation won the race and
        // committed before us): v1alpha1 subscription contracts are
        // immutable. An identical normalized set is an idempotent no-op —
        // `enabled` and `registered_at` are preserved because nothing is
        // written. A different set is a typed conflict; the tx rolls back
        // on drop and no row is changed.
        let existing_patterns = sqlx::query_scalar!(
            r#"
            SELECT pattern
            FROM integration_consumer_subscriptions
            WHERE consumer_id = $1
            ORDER BY pattern
            "#,
            consumer_id
        )
        .fetch_all(&mut *tx)
        .await?;
        if existing_patterns != subscriptions {
            return Err(OutboxStoreError::ConsumerRegistrationConflict {
                consumer_id: consumer_id.to_string(),
            });
        }
        tx.commit().await?;
        Ok(())
    }

    /// Set a consumer's `enabled` flag. Returns `false` when the consumer is
    /// not registered. Disabling does NOT remove pending obligations; it only
    /// stops claiming until re-enabled.
    pub async fn set_consumer_enabled(
        &self,
        consumer_id: &str,
        enabled: bool,
    ) -> Result<bool, OutboxStoreError> {
        let result = sqlx::query!(
            r#"
            UPDATE integration_consumers
            SET enabled = $2, updated_at = now()
            WHERE consumer_id = $1
            "#,
            consumer_id,
            enabled
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Whether a consumer is enabled; `false` when the consumer is unknown.
    pub async fn is_consumer_enabled(&self, consumer_id: &str) -> Result<bool, OutboxStoreError> {
        let enabled = sqlx::query_scalar!(
            "SELECT enabled FROM integration_consumers WHERE consumer_id = $1",
            consumer_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(enabled.unwrap_or(false))
    }

    /// All registered consumers with their subscription patterns, ordered by
    /// `consumer_id`.
    pub async fn list_consumers(&self) -> Result<Vec<ConsumerRegistration>, OutboxStoreError> {
        let consumers = sqlx::query!(
            r#"
            SELECT consumer_id, enabled, registered_at
            FROM integration_consumers
            ORDER BY consumer_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        let subscriptions = sqlx::query!(
            r#"
            SELECT consumer_id, pattern
            FROM integration_consumer_subscriptions
            ORDER BY consumer_id, pattern
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        let mut by_consumer: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for subscription in subscriptions {
            by_consumer
                .entry(subscription.consumer_id)
                .or_default()
                .push(subscription.pattern);
        }
        Ok(consumers
            .into_iter()
            .map(|consumer| {
                let consumer_id = consumer.consumer_id;
                ConsumerRegistration {
                    subscriptions: by_consumer.remove(&consumer_id).unwrap_or_default(),
                    consumer_id,
                    enabled: consumer.enabled,
                    registered_at: consumer.registered_at,
                }
            })
            .collect())
    }

    /// The sorted subscription patterns for one consumer; empty when the
    /// consumer is unknown.
    pub async fn consumer_subscriptions(
        &self,
        consumer_id: &str,
    ) -> Result<Vec<String>, OutboxStoreError> {
        let patterns = sqlx::query_scalar!(
            r#"
            SELECT pattern
            FROM integration_consumer_subscriptions
            WHERE consumer_id = $1
            ORDER BY pattern
            "#,
            consumer_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(patterns)
    }

    /// Load a consumer's durable registration (subscriptions, `enabled`,
    /// `registered_at`); `None` when the consumer is not registered.
    async fn consumer_registration(
        &self,
        consumer_id: &str,
    ) -> Result<Option<ConsumerRegistration>, OutboxStoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query!(
            r#"
            SELECT enabled, registered_at
            FROM integration_consumers
            WHERE consumer_id = $1
            "#,
            consumer_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(None); // uncommitted tx rolls back on drop
        };
        let subscriptions = sqlx::query_scalar!(
            r#"
            SELECT pattern
            FROM integration_consumer_subscriptions
            WHERE consumer_id = $1
            ORDER BY pattern
            "#,
            consumer_id
        )
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(ConsumerRegistration {
            consumer_id: consumer_id.to_string(),
            enabled: row.enabled,
            subscriptions,
            registered_at: row.registered_at,
        }))
    }

    /// Claim a batch of events for one consumer.
    ///
    /// The consumer's durable registration (subscriptions, `enabled`,
    /// `registered_at` — see [`Self::register_consumer`]) is authoritative:
    /// an unregistered or disabled consumer claims nothing, and its pending
    /// obligations remain untouched until it is registered/enabled.
    ///
    /// Two steps, each in its own transaction (claim atomicity per row is
    /// what matters):
    /// 1. re-claim existing delivery rows that are `pending` and available or
    ///    `claimed` with an expired lease;
    /// 2. first delivery / safety net: insert a claimed delivery row for
    ///    outbox events that have no delivery row yet for this consumer.
    ///    Step 2 never creates historical backlog: only events created at or
    ///    after the consumer's `registered_at` are eligible.
    ///
    /// Subscription matching: claim candidates are filtered by the durable
    /// subscription patterns. `.*` prefix patterns are expanded against the
    /// distinct event types present in the source table (deliveries for
    /// step 1, outbox for step 2) so the SQL `event_type = ANY($2)` filter is
    /// complete; see [`expanded_subscription_filter`]. The SQL predicate is
    /// `$2::text[] IS NOT NULL AND event_type = ANY($2)` — fail closed: an
    /// absent/NULL filter must match nothing, never everything. A claimed
    /// batch therefore never contains an event whose type does not match the
    /// durable patterns (additionally enforced on the deserialized envelope
    /// below). Durable registrations always declare at least one explicit
    /// pattern (see [`Self::register_consumer`]); if a registration were
    /// ever empty, the filter fails closed and nothing is claimed.
    ///
    /// The total returned rows never exceed `config.claim_batch_size`: step 1
    /// is bounded by `claim_batch_size` and step 2 by the remainder.
    ///
    /// No tenant filter by design: the delivery ledger and outbox are
    /// platform-global, so a consumer with a stable identity (e.g. a Memory
    /// projection serving all tenants) processes events across tenants. This
    /// is intentional — the envelope still enforces tenant/workspace equality
    /// on every published event, and a consumer that must not mix tenants
    /// applies its own scope rule in `process` (the reference consumer merely
    /// records each event's tenant id with its effect). Per-tenant claim
    /// scoping is a future consumer contract, not a transport concern.
    ///
    /// Every claim regenerates `claim_token` (`gen_random_uuid()`), so a
    /// stale worker holding an old token can never acknowledge the new lease
    /// (fencing). Rows whose `event_json` fails envelope re-validation are
    /// dead-lettered (redacted reason) and skipped, so a corrupt row cannot
    /// crash the dispatcher.
    pub async fn claim_batch(
        &self,
        consumer_id: &str,
        config: &OutboxConfig,
        worker_id: &str,
    ) -> Result<Vec<ClaimedEvent>, OutboxStoreError> {
        // The durable registration is authoritative; a disabled consumer
        // keeps its obligations but is not claimed.
        let Some(registration) = self.consumer_registration(consumer_id).await? else {
            return Ok(Vec::new());
        };
        if !registration.enabled {
            return Ok(Vec::new());
        }

        let mut rows = self
            .reclaim_existing(consumer_id, &registration.subscriptions, config, worker_id)
            .await?;
        // Bounded total: step 2 may only fill the remainder of the batch.
        let remaining = (config.claim_batch_size - rows.len() as i64).max(0);
        if remaining > 0 {
            rows.extend(
                self.first_delivery(
                    consumer_id,
                    &registration.subscriptions,
                    config,
                    worker_id,
                    remaining,
                )
                .await?,
            );
        }

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
            // Invariant guard: the SQL filter above is complete, but never
            // hand a consumer an event outside its durable subscriptions.
            if !event_matches_subscription(&event.r#type, &registration.subscriptions) {
                tracing::warn!(
                    %consumer_id,
                    event_type = %event.r#type,
                    "skipping claimed delivery outside durable subscriptions"
                );
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
    ///
    /// `.*` prefix patterns are expanded against the distinct event types
    /// present in this consumer's delivery rows — the only types step 1 can
    /// match — so the SQL `event_type = ANY($2)` filter is complete for the
    /// candidate set.
    async fn reclaim_existing(
        &self,
        consumer_id: &str,
        subscriptions: &[String],
        config: &OutboxConfig,
        worker_id: &str,
    ) -> Result<Vec<ClaimedRow>, OutboxStoreError> {
        let mut tx = self.pool.begin().await?;
        let distinct_types: Vec<String> = sqlx::query_scalar!(
            r#"
            SELECT DISTINCT event_type FROM integration_deliveries WHERE consumer_id = $1
            "#,
            consumer_id
        )
        .fetch_all(&mut *tx)
        .await?;
        let subscription_filter = expanded_subscription_filter(subscriptions, &distinct_types);
        let rows = sqlx::query_as::<_, ClaimedRow>(
            r#"
            WITH updated AS (
                WITH candidates AS (
                    SELECT d.source, d.event_id
                    FROM integration_deliveries d
                    WHERE d.consumer_id = $1
                      AND ( (d.state = 'pending' AND d.available_at <= now())
                            OR (d.state = 'claimed' AND d.claim_expires_at <= now()) )
                      AND ($2::text[] IS NOT NULL AND d.event_type = ANY($2))
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
        .bind(subscription_filter.as_deref())
        .bind(config.claim_batch_size)
        .bind(worker_id)
        .bind(config.lease_secs)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows)
    }

    /// Step 2 of [`Self::claim_batch`]: first delivery / safety net — insert
    /// a claimed delivery row for outbox events without a delivery row for
    /// this consumer.
    ///
    /// Bounded by `limit` (the batch remainder from [`Self::claim_batch`]) so
    /// the total returned never exceeds `config.claim_batch_size`. `.*`
    /// prefix patterns are expanded against the distinct event types in the
    /// outbox — the only types step 2 can produce. Events created before the
    /// consumer's `registered_at` are never backfilled.
    async fn first_delivery(
        &self,
        consumer_id: &str,
        subscriptions: &[String],
        config: &OutboxConfig,
        worker_id: &str,
        limit: i64,
    ) -> Result<Vec<ClaimedRow>, OutboxStoreError> {
        let mut tx = self.pool.begin().await?;
        let distinct_types: Vec<String> =
            sqlx::query_scalar!("SELECT DISTINCT event_type FROM integration_outbox")
                .fetch_all(&mut *tx)
                .await?;
        let subscription_filter = expanded_subscription_filter(subscriptions, &distinct_types);
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
                      AND o.created_at >= (SELECT registered_at FROM integration_consumers WHERE consumer_id = $1)
                      AND NOT EXISTS (
                          SELECT 1 FROM integration_deliveries d
                          WHERE d.consumer_id = $1 AND d.source = o.source AND d.event_id = o.event_id
                      )
                      AND ($2::text[] IS NOT NULL AND o.event_type = ANY($2))
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
        .bind(subscription_filter.as_deref())
        .bind(limit)
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
    /// With eager fan-out, every event published while at least one consumer
    /// is registered carries an obligation row per entitled consumer: an
    /// event is compacted only when every consumer obligated at publication
    /// has reached `processed`. Events with zero obligations (no registered
    /// consumer at publish time) are compactable after the retention window,
    /// and dead-lettered obligations block compaction so operators keep
    /// visibility.
    ///
    /// The FK cascade removes the processed delivery rows; `integration_consumer_receipts`
    /// rows are deliberately not FK-linked and remain as harmless durable
    /// idempotency records. Pass `retention_hours <= 0` to disable.
    ///
    /// The `NOT EXISTS` anti-join lives in the DELETE's outer WHERE — NOT in a
    /// materialized CTE — on purpose. Under READ COMMITTED, when this DELETE
    /// blocks on a delivery row updated concurrently (e.g. a claim racing the
    /// compaction), EvalPlanQual re-evaluates the outer WHERE against the new
    /// row version, so a delivery that became `claimed`/`pending` mid-statement
    /// re-blocks compaction. A CTE is materialized once from the statement's
    /// first snapshot: after a lock wait the outer DELETE would re-check only
    /// the `source`/`event_id` equality, and a delivery that was claimed or
    /// inserted after that snapshot could be cascade-deleted with the outbox
    /// row.
    pub async fn maintenance(&self, retention_hours: i64) -> Result<u64, OutboxStoreError> {
        if retention_hours <= 0 {
            return Ok(0);
        }
        let result = sqlx::query!(
            r#"
            DELETE FROM integration_outbox o
            WHERE o.created_at < now() - ($1::int8 * interval '1 hour')
              AND NOT EXISTS (
                  SELECT 1 FROM integration_deliveries d
                  WHERE d.source = o.source AND d.event_id = o.event_id AND d.state <> 'processed'
              )
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

/// Expand `.*` subscription patterns against the distinct event types
/// present in the source table so the claim SQL's `event_type = ANY($2)`
/// filter is complete: a prefix can only match types that actually exist in
/// the source. Exact patterns pass through unchanged; the result is
/// deduplicated and sorted.
///
/// An empty pattern list yields `Some(empty)`, which matches nothing in the
/// SQL filter — claim paths fail closed. Durable registrations are
/// non-empty by contract ([`OutboxStore::register_consumer`] rejects empty
/// lists), so this guard is defensive only.
fn expanded_subscription_filter(
    subscriptions: &[String],
    distinct_types: &[String],
) -> Option<Vec<String>> {
    if subscriptions.is_empty() {
        return Some(Vec::new());
    }
    let mut expanded = Vec::new();
    for pattern in subscriptions {
        if pattern.ends_with(".*") {
            expanded.extend(
                distinct_types
                    .iter()
                    .filter(|event_type| {
                        event_matches_subscription(event_type, std::slice::from_ref(pattern))
                    })
                    .cloned(),
            );
        } else {
            expanded.push(pattern.clone());
        }
    }
    expanded.sort();
    expanded.dedup();
    Some(expanded)
}

/// Validate a durable subscription pattern: a non-empty string of at most
/// 256 chars over `[a-z0-9-_.]`, either an exact event type (checked with
/// [`valid_event_type`]) or a `prefix.*` wildcard whose prefix is non-empty
/// and does not end with `.` (a trailing dot could never match a segment).
fn valid_subscription_pattern(pattern: &str) -> bool {
    if pattern.is_empty() || pattern.len() > 256 {
        return false;
    }
    let Some(prefix) = pattern.strip_suffix(".*") else {
        return valid_event_type(pattern);
    };
    !prefix.is_empty()
        && !prefix.ends_with('.')
        && prefix.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_' || b == b'.'
        })
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
            .resource(resource)
            .data(facts.data.clone())
            .build()
            .map_err(|error| IntegrationPublishError::InvalidEvent(error.to_string()))?;
        // Actor attribution: only an authenticated Elembra Principal is
        // attributed. External actions (e.g. a public-share session upload)
        // omit `elembraActor` entirely — the resource owner is never used as
        // a fallback actor.
        let mut event = event;
        event.actor = match facts.actor {
            IntegrationEventActor::Principal(id) => Some(ActorRef::Principal(PrincipalId(id))),
            IntegrationEventActor::External => None,
        };
        event
            .validate()
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
    /// by these tests), so they must run serially: claims are
    /// subscription-filtered and every test cleans up exactly its own rows,
    /// but a leaked row from an aborted run would still break the next
    /// test's counts.
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
    /// otherwise leak into another test's fixtures (claims are
    /// subscription-filtered, so a leak must not be claimed, but counts
    /// would still break). Consumer registrations are cleared too so a
    /// leaked registration cannot fan out obligations into another test's
    /// fixtures.
    async fn clean_slate(pool: &PgPool) {
        sqlx::query("DELETE FROM integration_consumer_subscriptions")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM integration_consumers")
            .execute(pool)
            .await
            .unwrap();
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

        // Registered before publishing: fan-out creates an obligation only
        // for the subscribed event type.
        store
            .register_consumer(
                &consumer_id,
                &["io.elembra.files.file.created.v1".to_string()],
            )
            .await
            .unwrap();
        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &created).await.unwrap();
        store.insert_in_tx(&mut tx, &updated).await.unwrap();
        tx.commit().await.unwrap();

        let config = OutboxConfig::default();
        let claimed = store
            .claim_batch(&consumer_id, &config, "test-worker")
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
    async fn empty_subscription_registration_is_rejected() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let consumer_id = format!("io.elembra.test.empty-{}", Uuid::new_v4());
        clean_slate(&pool).await;

        // An empty subscription list cannot be discovered at eager fan-out,
        // so it is rejected outright and no consumer row is created.
        let result = store.register_consumer(&consumer_id, &[]).await;
        assert!(
            matches!(result, Err(OutboxStoreError::EmptySubscriptionList(_))),
            "empty registration must fail with a typed error: {result:?}"
        );
        let consumers = store.list_consumers().await.unwrap();
        assert!(
            !consumers.iter().any(|c| c.consumer_id == consumer_id),
            "a rejected registration must not create a consumer row"
        );
        assert!(
            store
                .consumer_subscriptions(&consumer_id)
                .await
                .unwrap()
                .is_empty(),
            "a rejected registration must not create subscription rows"
        );
        assert!(!store.is_consumer_enabled(&consumer_id).await.unwrap());

        // Nothing was written: a later valid registration still works.
        store
            .register_consumer(&consumer_id, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();
        assert_eq!(
            store.consumer_subscriptions(&consumer_id).await.unwrap(),
            vec!["io.elembra.files.*".to_string()]
        );

        sqlx::query("DELETE FROM integration_consumers WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn acknowledge_fencing_rejects_stale_tokens() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let event = test_event("io.elembra.files.file.created.v1");
        let consumer_id = format!("io.elembra.test.fencing-{}", Uuid::new_v4());
        clean_slate(&pool).await;
        store
            .register_consumer(&consumer_id, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        let claimed = store
            .claim_batch(&consumer_id, &OutboxConfig::default(), "worker-a")
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
        store
            .register_consumer(&consumer_id, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        let first = store
            .claim_batch(&consumer_id, &OutboxConfig::default(), "worker-a")
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
            .claim_batch(&consumer_id, &OutboxConfig::default(), "worker-b")
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
        store
            .register_consumer(&consumer_id, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        let claimed = store
            .claim_batch(&consumer_id, &config, "worker")
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
            .claim_batch(&consumer_id, &config, "worker")
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
    async fn duplicate_identical_publish_idempotent() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let event = test_event("io.elembra.files.file.created.v1");
        let consumer_id = format!("io.elembra.test.idem-{}", Uuid::new_v4());
        clean_slate(&pool).await;
        store
            .register_consumer(&consumer_id, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();

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
        let delivery_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_deliveries WHERE consumer_id = $1 AND source = $2 AND event_id = $3",
        )
        .bind(&consumer_id)
        .bind(&event.source)
        .bind(event.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            delivery_count, 1,
            "single obligation row for a duplicate publish"
        );

        sqlx::query("DELETE FROM integration_deliveries WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&event]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn duplicate_conflicting_publish_fails_and_rolls_back_caller_tx() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let event = test_event("io.elembra.files.file.created.v1");
        clean_slate(&pool).await;

        // First publish commits.
        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        // A retry of the same (source, event_id) with a different payload
        // fails the caller's transaction wholesale.
        let mut conflicting = event.clone();
        conflicting.data = json!({"name": "different.txt", "mime_type": "text/plain", "size": 99});
        let mut tx = store.pool().begin().await.unwrap();
        // A marker row proves the caller's transaction rolls back wholesale.
        sqlx::query(
            "INSERT INTO integration_consumer_receipts (consumer_id, source, event_id, event_type, tenant_id, workspace_id) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind("io.elembra.test.scratch")
        .bind(&event.source)
        .bind(event.id)
        .bind(&event.r#type)
        .bind(event.tenant_id.0)
        .bind(event.workspace_id.0)
        .execute(&mut *tx)
        .await
        .unwrap();
        let result = store.insert_in_tx(&mut tx, &conflicting).await;
        assert!(matches!(
            result,
            Err(OutboxStoreError::EventIdentityConflict { .. })
        ));
        drop(tx); // caller rolls back

        let marker_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_consumer_receipts WHERE consumer_id = 'io.elembra.test.scratch'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(marker_count, 0, "caller tx rolled back wholesale");
        let outbox_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_outbox WHERE source = $1 AND event_id = $2",
        )
        .bind(&event.source)
        .bind(event.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(outbox_count, 1, "committed first publish survives");

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
            actor: IntegrationEventActor::Principal(actor),
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
    async fn publisher_trait_omits_actor_for_external() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let tenant = Uuid::new_v4();
        let facts = IntegrationEventFacts {
            event_type: "io.elembra.files.file.created.v1",
            resource_type: "file",
            resource_id: &Uuid::new_v4().to_string(),
            version: None,
            data: json!({"name": "external.txt", "mime_type": "text/plain", "size": 4}),
            tenant_id: tenant,
            workspace_id: tenant,
            actor: IntegrationEventActor::External,
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
        assert_eq!(
            event.actor, None,
            "External must omit elembraActor entirely (no owner fallback)"
        );

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
        store
            .register_consumer(&consumer_id, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();

        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &created).await.unwrap();
        store.insert_in_tx(&mut tx, &updated).await.unwrap();
        tx.commit().await.unwrap();

        let config = OutboxConfig::default();
        let claimed = store
            .claim_batch(&consumer_id, &config, "worker")
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
        store
            .register_consumer(&consumer_id, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();

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
            .claim_batch(&consumer_id, &OutboxConfig::default(), "worker")
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

    #[test]
    fn subscription_pattern_validation() {
        for valid in ["io.elembra.files.file.created.v1", "io.elembra.files.*"] {
            assert!(
                valid_subscription_pattern(valid),
                "{valid} should be a valid subscription pattern"
            );
        }
        for invalid in [
            "",
            "*",
            "io.elembra.files.",
            "io.elembra.*.v1",
            "hello world.*",
            "io.elembra.Files.*",
        ] {
            assert!(
                !valid_subscription_pattern(invalid),
                "{invalid} should be rejected"
            );
        }
        assert!(
            !valid_subscription_pattern(&"x".repeat(257)),
            "oversized patterns should be rejected"
        );
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn register_consumer_idempotent_until_contract_changes() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let consumer_id = format!("io.elembra.test.register-{}", Uuid::new_v4());
        clean_slate(&pool).await;

        store
            .register_consumer(
                &consumer_id,
                &["io.elembra.files.file.created.v1".to_string()],
            )
            .await
            .unwrap();
        let registered_at = sqlx::query_scalar::<_, DateTime<Utc>>(
            "SELECT registered_at FROM integration_consumers WHERE consumer_id = $1",
        )
        .bind(&consumer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(store.is_consumer_enabled(&consumer_id).await.unwrap());
        assert_eq!(
            store.consumer_subscriptions(&consumer_id).await.unwrap(),
            vec!["io.elembra.files.file.created.v1".to_string()]
        );

        // Re-register with the identical subscription set (unsorted input
        // with a duplicate — compared as a sorted, deduplicated set):
        // idempotent success, `enabled` and `registered_at` preserved.
        assert!(store
            .set_consumer_enabled(&consumer_id, false)
            .await
            .unwrap());
        store
            .register_consumer(
                &consumer_id,
                &[
                    "io.elembra.files.file.created.v1".to_string(),
                    "io.elembra.files.file.created.v1".to_string(),
                ],
            )
            .await
            .unwrap();
        assert!(
            !store.is_consumer_enabled(&consumer_id).await.unwrap(),
            "re-registration must not reset enabled"
        );
        assert_eq!(
            store.consumer_subscriptions(&consumer_id).await.unwrap(),
            vec!["io.elembra.files.file.created.v1".to_string()],
            "identical re-registration must not change the subscription rows"
        );
        let registered_at_after = sqlx::query_scalar::<_, DateTime<Utc>>(
            "SELECT registered_at FROM integration_consumers WHERE consumer_id = $1",
        )
        .bind(&consumer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            registered_at_after, registered_at,
            "registered_at is only ever set on first registration"
        );

        // Re-register with a DIFFERENT subscription set: typed conflict,
        // and neither the consumer row nor the subscription rows change.
        let result = store
            .register_consumer(
                &consumer_id,
                &[
                    "io.elembra.files.*".to_string(),
                    "io.elembra.files.file.updated.v1".to_string(),
                ],
            )
            .await;
        assert!(
            matches!(
                result,
                Err(OutboxStoreError::ConsumerRegistrationConflict { .. })
            ),
            "a changed subscription contract must conflict: {result:?}"
        );
        assert_eq!(
            store.consumer_subscriptions(&consumer_id).await.unwrap(),
            vec!["io.elembra.files.file.created.v1".to_string()],
            "a conflicted re-registration must leave subscription rows unchanged"
        );
        assert!(
            !store.is_consumer_enabled(&consumer_id).await.unwrap(),
            "a conflicted re-registration must leave enabled unchanged"
        );
        let registered_at_after_conflict = sqlx::query_scalar::<_, DateTime<Utc>>(
            "SELECT registered_at FROM integration_consumers WHERE consumer_id = $1",
        )
        .bind(&consumer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            registered_at_after_conflict, registered_at,
            "a conflicted re-registration must leave registered_at unchanged"
        );

        // Invalid patterns are rejected.
        let result = store
            .register_consumer(&consumer_id, &["io.elembra.files.".to_string()])
            .await;
        assert!(matches!(
            result,
            Err(OutboxStoreError::InvalidSubscription(_))
        ));

        sqlx::query("DELETE FROM integration_consumer_subscriptions WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM integration_consumers WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn publish_creates_obligations_for_matching_registered_consumers() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let consumer_a = format!("io.elembra.test.obligation-a-{}", Uuid::new_v4());
        let consumer_b = format!("io.elembra.test.obligation-b-{}", Uuid::new_v4());
        let consumer_c = format!("io.elembra.test.obligation-c-{}", Uuid::new_v4());
        clean_slate(&pool).await;

        store
            .register_consumer(
                &consumer_a,
                &["io.elembra.files.file.created.v1".to_string()],
            )
            .await
            .unwrap();
        store
            .register_consumer(
                &consumer_b,
                &["io.elembra.files.file.updated.v1".to_string()],
            )
            .await
            .unwrap();
        store
            .register_consumer(&consumer_c, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();

        let created = test_event("io.elembra.files.file.created.v1");
        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &created).await.unwrap();
        tx.commit().await.unwrap();

        // Exact match and prefix match get obligations; non-matching does not.
        assert_eq!(
            delivery_state(&pool, &consumer_a, &created).await,
            "pending"
        );
        assert_eq!(
            delivery_state(&pool, &consumer_c, &created).await,
            "pending"
        );
        let count_b = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_deliveries WHERE consumer_id = $1 AND source = $2 AND event_id = $3",
        )
        .bind(&consumer_b)
        .bind(&created.source)
        .bind(created.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count_b, 0, "non-matching consumer gets no obligation");

        sqlx::query("DELETE FROM integration_deliveries")
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&created]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn publish_creates_no_backlog_for_events_before_registration() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let consumer_id = format!("io.elembra.test.nobacklog-{}", Uuid::new_v4());
        clean_slate(&pool).await;

        let event = test_event("io.elembra.files.file.created.v1");
        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        // Registered after the publish: no obligation and no backfill.
        store
            .register_consumer(&consumer_id, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();
        let claimed = store
            .claim_batch(&consumer_id, &OutboxConfig::default(), "worker")
            .await
            .unwrap();
        assert!(
            claimed.is_empty(),
            "pre-registration events must never be claimed"
        );
        let delivery_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_deliveries WHERE consumer_id = $1",
        )
        .bind(&consumer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(delivery_count, 0, "no delivery row was created");

        cleanup(&pool, &[&event]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn fan_out_skips_consumers_registered_after_event_creation() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let consumer_id = format!("io.elembra.test.fanoutgate-{}", Uuid::new_v4());
        clean_slate(&pool).await;

        let event = test_event("io.elembra.files.file.created.v1");
        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();

        // Consumer registers AFTER the event was created: registration
        // establishes entitlement going forward, never retroactively.
        store
            .register_consumer(&consumer_id, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();

        // Idempotent republish of the identical event (same id + payload):
        // the fan-out gate must NOT backfill an obligation for a consumer
        // registered after the event's original creation.
        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();
        let outbox_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_outbox WHERE source = $1 AND event_id = $2",
        )
        .bind(&event.source)
        .bind(event.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(outbox_count, 1, "duplicate publish stays a single row");
        let delivery_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_deliveries WHERE consumer_id = $1",
        )
        .bind(&consumer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            delivery_count, 0,
            "no obligation for a consumer registered after the event"
        );

        let claimed = store
            .claim_batch(&consumer_id, &OutboxConfig::default(), "worker")
            .await
            .unwrap();
        assert!(
            claimed.is_empty(),
            "claim_batch must return nothing for a post-event registration"
        );

        sqlx::query("DELETE FROM integration_consumers WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&event]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn disabled_consumer_not_claimed_but_obligation_kept() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let consumer_id = format!("io.elembra.test.disabled-{}", Uuid::new_v4());
        clean_slate(&pool).await;
        store
            .register_consumer(&consumer_id, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();
        assert!(store
            .set_consumer_enabled(&consumer_id, false)
            .await
            .unwrap());

        let event = test_event("io.elembra.files.file.created.v1");
        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &event).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            delivery_state(&pool, &consumer_id, &event).await,
            "pending",
            "obligations are created regardless of enabled"
        );

        let claimed = store
            .claim_batch(&consumer_id, &OutboxConfig::default(), "worker")
            .await
            .unwrap();
        assert!(claimed.is_empty(), "disabled consumer is not claimed");
        assert_eq!(
            delivery_state(&pool, &consumer_id, &event).await,
            "pending",
            "obligation kept while disabled"
        );

        assert!(store
            .set_consumer_enabled(&consumer_id, true)
            .await
            .unwrap());
        let claimed = store
            .claim_batch(&consumer_id, &OutboxConfig::default(), "worker")
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1, "re-enabled consumer claims its backlog");
        assert_eq!(claimed[0].event, event);

        sqlx::query("DELETE FROM integration_deliveries WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&event]).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn claim_batch_total_respects_batch_size() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let consumer_id = format!("io.elembra.test.batchsize-{}", Uuid::new_v4());
        clean_slate(&pool).await;
        store
            .register_consumer(&consumer_id, &["io.elembra.files.*".to_string()])
            .await
            .unwrap();
        // Move registered_at into the past so step 2's guard never blocks.
        sqlx::query(
            "UPDATE integration_consumers SET registered_at = now() - interval '1 day' WHERE consumer_id = $1",
        )
        .bind(&consumer_id)
        .execute(&pool)
        .await
        .unwrap();

        let events: Vec<IntegrationEvent> = (0..6)
            .map(|_| test_event("io.elembra.files.file.created.v1"))
            .collect();
        let mut tx = store.pool().begin().await.unwrap();
        for event in &events {
            store.insert_in_tx(&mut tx, event).await.unwrap();
        }
        tx.commit().await.unwrap();
        // Fan-out created 6 pending obligations; drop the deliveries for the
        // last three so they exist only in the outbox (step-2 candidates).
        for event in &events[3..] {
            sqlx::query(
                "DELETE FROM integration_deliveries WHERE consumer_id = $1 AND source = $2 AND event_id = $3",
            )
            .bind(&consumer_id)
            .bind(&event.source)
            .bind(event.id)
            .execute(&pool)
            .await
            .unwrap();
        }

        let config = OutboxConfig {
            claim_batch_size: 4,
            ..OutboxConfig::default()
        };
        let first = store
            .claim_batch(&consumer_id, &config, "worker")
            .await
            .unwrap();
        assert!(
            first.len() <= 4,
            "total returned must never exceed claim_batch_size"
        );
        assert_eq!(first.len(), 4, "3 existing deliveries + 1 first delivery");
        let second = store
            .claim_batch(&consumer_id, &config, "worker")
            .await
            .unwrap();
        assert_eq!(second.len(), 2, "second claim returns the rest");
        assert!(
            second
                .iter()
                .all(|c| !first.iter().any(|f| f.event_id == c.event_id)),
            "no event claimed twice"
        );

        sqlx::query("DELETE FROM integration_deliveries WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        for event in &events {
            cleanup(&pool, &[event]).await;
        }
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn claim_batch_skips_events_outside_subscriptions() {
        let _db_guard = DB_TEST_LOCK.lock().await;
        let (store, pool) = setup().await;
        let consumer_id = format!("io.elembra.test.subfilter-{}", Uuid::new_v4());
        clean_slate(&pool).await;
        store
            .register_consumer(
                &consumer_id,
                &["io.elembra.files.file.created.v1".to_string()],
            )
            .await
            .unwrap();

        let created = test_event("io.elembra.files.file.created.v1");
        let updated = test_event("io.elembra.files.file.updated.v1");
        let mut tx = store.pool().begin().await.unwrap();
        store.insert_in_tx(&mut tx, &created).await.unwrap();
        store.insert_in_tx(&mut tx, &updated).await.unwrap();
        tx.commit().await.unwrap();
        // Fan-out only created an obligation for `created`; plant a pending
        // delivery for the non-subscribed type directly.
        sqlx::query(
            "INSERT INTO integration_deliveries (consumer_id, source, event_id, event_type, tenant_id, workspace_id, state, available_at) VALUES ($1, $2, $3, $4, $5, $6, 'pending', now())",
        )
        .bind(&consumer_id)
        .bind(&updated.source)
        .bind(updated.id)
        .bind(&updated.r#type)
        .bind(updated.tenant_id.0)
        .bind(updated.workspace_id.0)
        .execute(&pool)
        .await
        .unwrap();

        let claimed = store
            .claim_batch(&consumer_id, &OutboxConfig::default(), "worker")
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1, "only the subscribed event is claimed");
        assert_eq!(claimed[0].event, created);
        assert_eq!(
            delivery_state(&pool, &consumer_id, &updated).await,
            "pending",
            "non-subscribed delivery is never claimed"
        );

        sqlx::query("DELETE FROM integration_deliveries WHERE consumer_id = $1")
            .bind(&consumer_id)
            .execute(&pool)
            .await
            .unwrap();
        cleanup(&pool, &[&created, &updated]).await;
    }
}
