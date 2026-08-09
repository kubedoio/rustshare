//! Integration tests for the durable integration-event outbox (ADR-0031,
//! GitHub issue #212).
//!
//! Exercises the real store + reference consumer + dispatcher against the
//! dev database:
//!
//! * atomic publish (outbox row commits with the source mutation, never
//!   without it) — tests 1–3;
//! * lazy fan-out, recovery, at-least-once + idempotency, lease fencing and
//!   concurrent workers — tests 4–7;
//! * retry backoff, dead-lettering, requeue and redaction — tests 8–10;
//! * fail-closed behavior (poison rows, validation, ownership) — tests
//!   11–12;
//! * security invariants (an event grants no access; serialized authority
//!   context is inert data) — tests 13–14;
//! * transport-neutral envelope contract — test 15.
//!
//! The outbox tables are global (not tenant-scoped), so every test takes a
//! shared `SERIAL` guard and cleans up exactly the rows it created. Run with:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test --test outbox_integration_test -p rustshare-server \
//!     -- --ignored --test-threads=1

mod contracts;
use contracts::common::{setup_test_env, TestContext};

use bytes::Bytes;
use rustshare_core::domain::{
    ActionCapability, ApplicationId, ApplicationRegistry, PrincipalId, TenantId, WorkspaceId,
};
use rustshare_core::services::{
    IntegrationEventFacts, IntegrationEventPublisher, IntegrationPublishError, PermissionResolver,
};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_integration_events::event_types::event_types::{
    FILES_FILE_CREATED_V1, FILES_FILE_UPDATED_V1,
};
use rustshare_integration_events::{ActorRef, ConsumerOutcome, IntegrationEvent, OutboxConsumer};
use rustshare_resource_auth::{
    Decision, PrincipalContext, ResourceOwnerRegistry, ResourceRef, SourceAuthorizer, FILES_READ,
};
use rustshare_server::authz::FilesResourceOwner;
use rustshare_server::config::OutboxWorkerConfig;
use rustshare_server::outbox_consumers::{
    extract_projection, ReferenceMemoryProjectionConsumer, REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
};
use rustshare_server::outbox_dispatcher::OutboxDispatcher;
use rustshare_storage::{OutboxConfig, OutboxStore, OutboxStoreError};
use sqlx::Row;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

/// The outbox/delivery/receipt/effect tables are process-global, so all
/// tests touching them run under one serialization guard (same convention as
/// the Phase-1 storage tests).
static SERIAL: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Build an `OutboxStore` over the harness pool with the canonical
/// first-party Application registry (Files owns the file created/updated
/// event types).
async fn setup_store(ctx: &TestContext) -> Arc<OutboxStore> {
    let registry = Arc::new(ApplicationRegistry::first_party().unwrap());
    Arc::new(OutboxStore::new(ctx.pool.clone(), registry))
}

/// A valid Files file-created envelope for `tenant_id` (the reference
/// consumer projects `data` into its effect table).
fn files_created_event(tenant_id: Uuid) -> IntegrationEvent {
    IntegrationEvent::builder()
        .source("elembra://io.elembra.files")
        .r#type(FILES_FILE_CREATED_V1)
        .tenant_id(TenantId(tenant_id))
        .workspace_id(WorkspaceId(tenant_id))
        .actor(ActorRef::Principal(PrincipalId(Uuid::new_v4())))
        .data(serde_json::json!({
            "name": "created.txt",
            "mime_type": "text/plain",
            "size": 42,
            "version": "sha256:0123abcdef",
        }))
        .build()
        .unwrap()
}

/// Publish `event` atomically via the store's own transaction (no FileService
/// involved — used by the dispatcher-level tests).
async fn publish(store: &OutboxStore, event: &IntegrationEvent) {
    let mut tx = store.pool().begin().await.unwrap();
    store.insert_in_tx(&mut tx, event).await.unwrap();
    tx.commit().await.unwrap();
}

/// Remove every outbox-side row for the given events (all four tables; the
/// delivery/receipt/effect tables are keyed by source+event_id, so deleting
/// by those two columns covers every consumer).
/// Empty the outbox tables. Safe because every test in this file holds
/// `SERIAL`; rows leaked by an aborted previous run would otherwise be
/// claimed by the next test (claims filter by event type, not by test).
async fn clean_slate(pool: &sqlx::PgPool) {
    for table in [
        "integration_reference_effects",
        "integration_consumer_receipts",
        "integration_deliveries",
        "integration_outbox",
    ] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn cleanup_events(pool: &sqlx::PgPool, events: &[&IntegrationEvent]) {
    for event in events {
        for table in [
            "integration_reference_effects",
            "integration_consumer_receipts",
            "integration_deliveries",
            "integration_outbox",
        ] {
            sqlx::query(&format!(
                "DELETE FROM {table} WHERE source = $1 AND event_id = $2"
            ))
            .bind(&event.source)
            .bind(event.id)
            .execute(pool)
            .await
            .unwrap();
        }
    }
}

async fn delivery_state(
    pool: &sqlx::PgPool,
    consumer_id: &str,
    event: &IntegrationEvent,
) -> String {
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

async fn effect_count(pool: &sqlx::PgPool, consumer_id: &str, events: &[&IntegrationEvent]) -> i64 {
    let ids: Vec<Uuid> = events.iter().map(|e| e.id).collect();
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_reference_effects WHERE consumer_id = $1 AND event_id = ANY($2)",
    )
    .bind(consumer_id)
    .bind(&ids)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn receipt_count(
    pool: &sqlx::PgPool,
    consumer_id: &str,
    events: &[&IntegrationEvent],
) -> i64 {
    let ids: Vec<Uuid> = events.iter().map(|e| e.id).collect();
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_consumer_receipts WHERE consumer_id = $1 AND event_id = ANY($2)",
    )
    .bind(consumer_id)
    .bind(&ids)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A dispatcher wired to the reference consumer over `store`.
fn reference_dispatcher(
    store: Arc<OutboxStore>,
    pool: &sqlx::PgPool,
    config: OutboxWorkerConfig,
) -> Arc<OutboxDispatcher> {
    let consumer = Arc::new(ReferenceMemoryProjectionConsumer::new(pool.clone(), true));
    Arc::new(OutboxDispatcher::new(
        store,
        vec![consumer as Arc<dyn OutboxConsumer>],
        config,
        "test-worker".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// 1. Atomic publish: the outbox row commits with the source mutation
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn publishes_file_events_atomically_on_upload() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let owner = ctx.create_test_user("upload_owner").await;
    let publisher: Arc<dyn IntegrationEventPublisher<sqlx::Transaction<'static, sqlx::Postgres>>> =
        store.clone();
    let file_service = ctx.file_service().with_integration_publisher(publisher);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hello.txt");
    std::fs::write(&path, b"hello outbox").unwrap();
    let file = file_service
        .upload_file_from_path(
            owner.id,
            "hello.txt".to_string(),
            None,
            &path,
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .unwrap();

    // A NEW file → exactly one `file.created.v1` outbox row, correct tenant.
    let rows = sqlx::query(
        "SELECT event_type, tenant_id, event_json FROM integration_outbox WHERE application_id = 'io.elembra.files' AND tenant_id = $1 ORDER BY created_at, event_id",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "upload must publish exactly one event");
    assert_eq!(
        rows[0].try_get::<String, _>("event_type").unwrap(),
        FILES_FILE_CREATED_V1
    );
    assert_eq!(
        rows[0].try_get::<Uuid, _>("tenant_id").unwrap(),
        ctx.tenant_id
    );
    let first_event: IntegrationEvent = serde_json::from_value(
        rows[0]
            .try_get::<serde_json::Value, _>("event_json")
            .unwrap(),
    )
    .unwrap();
    // The created event carries the versioned resource reference: version is
    // the sha256 content selector of the uploaded content.
    assert_eq!(
        first_event.data["version"],
        format!("sha256:{}", file.content_hash)
    );

    // A NEW VERSION of the same file → a second row, `file.updated.v1`.
    std::fs::write(&path, b"hello outbox v2").unwrap();
    let updated = file_service
        .upload_file_from_path(
            owner.id,
            "hello.txt".to_string(),
            None,
            &path,
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .unwrap();
    let rows = sqlx::query(
        "SELECT event_type, event_json FROM integration_outbox WHERE application_id = 'io.elembra.files' AND tenant_id = $1 ORDER BY created_at, event_id",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        rows.len(),
        2,
        "versioned upload must publish exactly two events"
    );
    assert_eq!(
        rows[1].try_get::<String, _>("event_type").unwrap(),
        FILES_FILE_UPDATED_V1
    );
    let second_event: IntegrationEvent = serde_json::from_value(
        rows[1]
            .try_get::<serde_json::Value, _>("event_json")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(second_event.tenant_id.0, ctx.tenant_id);
    assert_eq!(second_event.data["name"], "hello.txt");
    assert_eq!(
        second_event.data["version"],
        format!("sha256:{}", updated.content_hash)
    );

    let all_events = [&first_event, &second_event];
    cleanup_events(&ctx.pool, &all_events).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 2. Required outbox failure ⇒ source mutation rolls back
// ---------------------------------------------------------------------------

/// Test-only publisher that always fails — proves the FileService aborts the
/// whole upload transaction when the outbox insert fails.
struct FailingPublisher;

#[async_trait::async_trait]
impl IntegrationEventPublisher<sqlx::Transaction<'static, sqlx::Postgres>> for FailingPublisher {
    async fn publish_in_tx(
        &self,
        _tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        _facts: &IntegrationEventFacts<'_>,
    ) -> Result<(), IntegrationPublishError> {
        Err(IntegrationPublishError::Persistence("boom".to_string()))
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn outbox_insert_failure_rolls_back_file_mutation() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let owner = ctx.create_test_user("rollback_owner").await;
    let failing: Arc<dyn IntegrationEventPublisher<sqlx::Transaction<'static, sqlx::Postgres>>> =
        Arc::new(FailingPublisher);
    let file_service = ctx.file_service().with_integration_publisher(failing);

    let result = file_service
        .upload_file(
            owner.id,
            "rollback.txt".to_string(),
            None,
            Bytes::from_static(b"must not survive"),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await;
    assert!(
        result.is_err(),
        "upload must fail when the outbox insert fails"
    );

    let found = ctx
        .metadata_store
        .find_file_by_path("/rollback.txt", owner.id)
        .await
        .unwrap();
    assert!(found.is_none(), "failed upload must not leave a file row");
    let outbox_rows = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1",
    )
    .bind(ctx.tenant_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(outbox_rows, 0, "failed upload must not leave an outbox row");

    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 3. Rollback after a successful outbox insert leaves no event
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn rollback_after_outbox_insert_leaves_no_event() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let event = files_created_event(ctx.tenant_id);

    let mut tx = store.pool().begin().await.unwrap();
    store.insert_in_tx(&mut tx, &event).await.unwrap();
    drop(tx); // rollback without commit

    let count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_outbox WHERE source = $1 AND event_id = $2",
    )
    .bind(&event.source)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "aborted transaction must not leave an outbox row");

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 4. Lazy fan-out + offline consumer recovery
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn offline_consumer_recovers_and_processes() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    // Fan-out is lazy: no consumer has run yet, so no delivery row exists.
    let delivery_rows = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_deliveries WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(REFERENCE_MEMORY_PROJECTION_CONSUMER_ID)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        delivery_rows, 0,
        "fan-out must be lazy (no delivery row yet)"
    );

    let dispatcher = reference_dispatcher(store.clone(), &ctx.pool, OutboxWorkerConfig::default());
    dispatcher.tick().await;

    assert_eq!(
        effect_count(
            &ctx.pool,
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &[&event]
        )
        .await,
        1,
        "first tick must apply the projection effect"
    );
    assert_eq!(
        delivery_state(&ctx.pool, REFERENCE_MEMORY_PROJECTION_CONSUMER_ID, &event).await,
        "processed"
    );

    // Idempotent: a second tick changes nothing.
    dispatcher.tick().await;
    assert_eq!(
        effect_count(
            &ctx.pool,
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &[&event]
        )
        .await,
        1
    );
    assert_eq!(
        delivery_state(&ctx.pool, REFERENCE_MEMORY_PROJECTION_CONSUMER_ID, &event).await,
        "processed"
    );

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 5. At-least-once delivery + idempotency: repeated delivery, single effect
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn duplicate_delivery_produces_single_effect() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    let consumer = Arc::new(ReferenceMemoryProjectionConsumer::new(
        ctx.pool.clone(),
        true,
    ));
    let subscriptions = vec![FILES_FILE_CREATED_V1.to_string()];
    for round in 1..=3 {
        let claimed = store
            .claim_batch(
                REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
                &subscriptions,
                &OutboxConfig::default(),
                &format!("worker-{round}"),
            )
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1, "round {round} must claim the event");
        let outcome = consumer.process(&claimed[0].event).await;
        assert_eq!(outcome, ConsumerOutcome::Processed, "round {round}");
        assert!(
            store
                .acknowledge(
                    REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
                    &claimed[0].source,
                    claimed[0].event_id,
                    claimed[0].claim_token,
                )
                .await
                .unwrap(),
            "round {round} ack"
        );
        if round < 3 {
            // Force a re-delivery of the same event (as if the delivery row
            // were reset by an operator or a crashed worker's lease cycle).
            sqlx::query(
                "UPDATE integration_deliveries SET state = 'pending', available_at = now(), claim_token = NULL, claim_expires_at = NULL WHERE consumer_id = $1 AND event_id = $2",
            )
            .bind(REFERENCE_MEMORY_PROJECTION_CONSUMER_ID)
            .bind(event.id)
            .execute(&ctx.pool)
            .await
            .unwrap();
        }
    }

    assert_eq!(
        effect_count(
            &ctx.pool,
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &[&event]
        )
        .await,
        1,
        "three deliveries must produce exactly one effect"
    );
    assert_eq!(
        receipt_count(
            &ctx.pool,
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &[&event]
        )
        .await,
        1,
        "exactly one durable receipt"
    );

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 6. Worker crash: lease expiry re-claims, stale token cannot acknowledge
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn worker_crash_lease_expiry_recovers() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    // A worker claims the event and "crashes" before processing.
    let claimed = store
        .claim_batch(
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &[FILES_FILE_CREATED_V1.to_string()],
            &OutboxConfig::default(),
            "crashed-worker",
        )
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    let stale_token = claimed[0].claim_token;

    // Simulate the crash: the lease expires while the delivery is claimed.
    sqlx::query(
        "UPDATE integration_deliveries SET state = 'claimed', claim_expires_at = now() - interval '1 second' WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(REFERENCE_MEMORY_PROJECTION_CONSUMER_ID)
    .bind(event.id)
    .execute(&ctx.pool)
    .await
    .unwrap();

    let dispatcher = reference_dispatcher(store.clone(), &ctx.pool, OutboxWorkerConfig::default());
    dispatcher.tick().await;

    let row = sqlx::query(
        "SELECT state, attempt_count FROM integration_deliveries WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(REFERENCE_MEMORY_PROJECTION_CONSUMER_ID)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<String, _>("state").unwrap(), "processed");
    assert!(
        row.try_get::<i32, _>("attempt_count").unwrap() >= 2,
        "recovery must count a second attempt"
    );
    assert_eq!(
        effect_count(
            &ctx.pool,
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &[&event]
        )
        .await,
        1
    );
    // The crashed worker's stale claim token must no longer acknowledge.
    assert!(!store
        .acknowledge(
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &event.source,
            event.id,
            stale_token,
        )
        .await
        .unwrap());

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 7. Concurrent workers: claim fencing + receipt ⇒ no duplicate effects
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn concurrent_workers_no_duplicate_effect() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let events: Vec<IntegrationEvent> =
        (0..5).map(|_| files_created_event(ctx.tenant_id)).collect();
    for event in &events {
        publish(&store, event).await;
    }
    let event_refs: Vec<&IntegrationEvent> = events.iter().collect();

    let consumer = Arc::new(ReferenceMemoryProjectionConsumer::new(
        ctx.pool.clone(),
        true,
    ));
    let consumer_a = consumer.clone() as Arc<dyn OutboxConsumer>;
    let consumer_b = consumer as Arc<dyn OutboxConsumer>;
    let dispatcher_a = Arc::new(OutboxDispatcher::new(
        store.clone(),
        vec![consumer_a],
        OutboxWorkerConfig::default(),
        "concurrent-worker-a".to_string(),
    ));
    let dispatcher_b = Arc::new(OutboxDispatcher::new(
        store.clone(),
        vec![consumer_b],
        OutboxWorkerConfig::default(),
        "concurrent-worker-b".to_string(),
    ));

    // Both workers race for the same batch; claim fencing (FOR UPDATE SKIP
    // LOCKED + per-row claim state) and the consumer's ON CONFLICT receipt
    // make duplicate effects impossible even if both ever claimed the same
    // event.
    tokio::join!(dispatcher_a.tick(), dispatcher_b.tick());

    assert_eq!(
        effect_count(
            &ctx.pool,
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &event_refs
        )
        .await,
        5,
        "exactly one effect per event"
    );
    assert_eq!(
        receipt_count(
            &ctx.pool,
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &event_refs
        )
        .await,
        5,
        "exactly one receipt per event"
    );

    cleanup_events(&ctx.pool, &event_refs).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// Stub consumers for the retry/DLQ tests
// ---------------------------------------------------------------------------

/// Fails retryably (reason embeds a secret to prove redaction) the first
/// `failures` times, then succeeds.
struct FlakyConsumer {
    failures_remaining: tokio::sync::Mutex<u8>,
    secret: &'static str,
    consumer_id: String,
}

impl FlakyConsumer {
    fn new(consumer_id: &str, failures: u8, secret: &'static str) -> Self {
        Self {
            failures_remaining: tokio::sync::Mutex::new(failures),
            secret,
            consumer_id: consumer_id.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl OutboxConsumer for FlakyConsumer {
    fn consumer_id(&self) -> &str {
        &self.consumer_id
    }
    fn subscriptions(&self) -> Vec<String> {
        vec![FILES_FILE_CREATED_V1.to_string()]
    }
    async fn process(&self, _event: &IntegrationEvent) -> ConsumerOutcome {
        let mut remaining = self.failures_remaining.lock().await;
        if *remaining > 0 {
            *remaining -= 1;
            return ConsumerOutcome::Retryable {
                reason: format!("transient failure: {}", self.secret),
            };
        }
        ConsumerOutcome::Processed
    }
}

/// Fails retryably until `succeed` flips, then succeeds. The failure reason
/// embeds a secret to prove persisted diagnostics are redacted.
struct AlwaysRetryThenSucceedConsumer {
    succeed: AtomicBool,
    secret: &'static str,
    consumer_id: String,
}

#[async_trait::async_trait]
impl OutboxConsumer for AlwaysRetryThenSucceedConsumer {
    fn consumer_id(&self) -> &str {
        &self.consumer_id
    }
    fn subscriptions(&self) -> Vec<String> {
        vec![FILES_FILE_CREATED_V1.to_string()]
    }
    async fn process(&self, _event: &IntegrationEvent) -> ConsumerOutcome {
        if self.succeed.load(Ordering::Relaxed) {
            ConsumerOutcome::Processed
        } else {
            ConsumerOutcome::Retryable {
                reason: format!("auth failed {}", self.secret),
            }
        }
    }
}

/// Always fails permanently (poison handling).
struct AlwaysPermanentConsumer {
    consumer_id: String,
}

#[async_trait::async_trait]
impl OutboxConsumer for AlwaysPermanentConsumer {
    fn consumer_id(&self) -> &str {
        &self.consumer_id
    }
    fn subscriptions(&self) -> Vec<String> {
        vec![FILES_FILE_CREATED_V1.to_string()]
    }
    async fn process(&self, _event: &IntegrationEvent) -> ConsumerOutcome {
        ConsumerOutcome::Permanent {
            reason: "poison data".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// 8. Retry backoff then success
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn retry_backoff_then_success() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    const SECRET: &str = "token=abc123secret";
    let consumer = Arc::new(FlakyConsumer::new("io.elembra.test.flaky", 2, SECRET));
    let dispatcher = Arc::new(OutboxDispatcher::new(
        store.clone(),
        vec![consumer as Arc<dyn OutboxConsumer>],
        OutboxWorkerConfig {
            backoff_initial_ms: 1,
            backoff_max_ms: 10,
            ..OutboxWorkerConfig::default()
        },
        "backoff-worker".to_string(),
    ));
    let consumer_id = "io.elembra.test.flaky";

    // Tick 1: first failure → pending with backoff, attempt 1, redacted error.
    dispatcher.tick().await;
    let row = sqlx::query(
        "SELECT state, attempt_count, last_error FROM integration_deliveries WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(consumer_id)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<String, _>("state").unwrap(), "pending");
    assert_eq!(row.try_get::<i32, _>("attempt_count").unwrap(), 1);
    let last_error = row
        .try_get::<Option<String>, _>("last_error")
        .unwrap()
        .expect("failed delivery must persist a reason");
    assert!(
        !last_error.contains("abc123secret"),
        "secret leaked: {last_error}"
    );
    assert!(
        last_error.contains("[REDACTED]"),
        "reason must be redacted: {last_error}"
    );

    // Tick 2: second failure (attempt 2), backoff again.
    sqlx::query(
        "UPDATE integration_deliveries SET available_at = now() - interval '1 second' WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(consumer_id)
    .bind(event.id)
    .execute(&ctx.pool)
    .await
    .unwrap();
    dispatcher.tick().await;
    let attempt: i32 = sqlx::query_scalar(
        "SELECT attempt_count FROM integration_deliveries WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(consumer_id)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(attempt, 2);

    // Tick 3: success → processed, attempt count 3 (claim incremented it).
    sqlx::query(
        "UPDATE integration_deliveries SET available_at = now() - interval '1 second' WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(consumer_id)
    .bind(event.id)
    .execute(&ctx.pool)
    .await
    .unwrap();
    dispatcher.tick().await;
    let row = sqlx::query(
        "SELECT state, attempt_count FROM integration_deliveries WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(consumer_id)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<String, _>("state").unwrap(), "processed");
    assert_eq!(row.try_get::<i32, _>("attempt_count").unwrap(), 3);

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 9. Exhausted retries dead-letter + operator requeue + redaction proof
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn exhausted_retries_dead_letter_and_requeue() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    const SECRET: &str = "token=secretvalue123";
    let consumer = Arc::new(AlwaysRetryThenSucceedConsumer {
        succeed: AtomicBool::new(false),
        secret: SECRET,
        consumer_id: "io.elembra.test.dlq".to_string(),
    });
    let dispatcher = Arc::new(OutboxDispatcher::new(
        store.clone(),
        vec![consumer.clone() as Arc<dyn OutboxConsumer>],
        OutboxWorkerConfig {
            max_attempts: 3,
            backoff_initial_ms: 1,
            backoff_max_ms: 10,
            ..OutboxWorkerConfig::default()
        },
        "dlq-worker".to_string(),
    ));
    let consumer_id = "io.elembra.test.dlq";

    // Tick until dead-lettered (bounded; each tick is one claim).
    for _ in 0..10 {
        dispatcher.tick().await;
        if delivery_state(&ctx.pool, consumer_id, &event).await == "dead_lettered" {
            break;
        }
        sqlx::query(
            "UPDATE integration_deliveries SET available_at = now() - interval '1 second' WHERE consumer_id = $1 AND event_id = $2",
        )
        .bind(consumer_id)
        .bind(event.id)
        .execute(&ctx.pool)
        .await
        .unwrap();
    }

    let row = sqlx::query(
        "SELECT state, attempt_count, dead_lettered_at, last_error FROM integration_deliveries WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(consumer_id)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<String, _>("state").unwrap(), "dead_lettered");
    assert_eq!(row.try_get::<i32, _>("attempt_count").unwrap(), 3);
    assert!(row
        .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("dead_lettered_at")
        .unwrap()
        .is_some());
    let last_error = row
        .try_get::<Option<String>, _>("last_error")
        .unwrap()
        .expect("dead letter must persist the final reason");
    assert!(
        !last_error.contains("secretvalue123"),
        "secret leaked into the dead letter: {last_error}"
    );
    assert!(
        last_error.contains("[REDACTED]"),
        "reason must be redacted: {last_error}"
    );

    // The DLQ view exposes metadata only (no event payload by construction —
    // `DeadLetterEntry` has no `event_json` field).
    let dlq = store
        .list_dead_letters(Some(consumer_id), 10)
        .await
        .unwrap();
    assert_eq!(dlq.len(), 1);
    assert_eq!(dlq[0].event_id, event.id);
    assert_eq!(dlq[0].event_type, FILES_FILE_CREATED_V1);

    // Operator requeue → pending, attempts reset, history kept.
    assert!(store
        .requeue(consumer_id, &event.source, event.id)
        .await
        .unwrap());
    let state = delivery_state(&ctx.pool, consumer_id, &event).await;
    assert_eq!(state, "pending");
    let attempt: i32 = sqlx::query_scalar(
        "SELECT attempt_count FROM integration_deliveries WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(consumer_id)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(attempt, 0);

    // Requeue + healthy consumer → processed.
    consumer.succeed.store(true, Ordering::Relaxed);
    dispatcher.tick().await;
    assert_eq!(
        delivery_state(&ctx.pool, consumer_id, &event).await,
        "processed"
    );

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 10. Permanent failure dead-letters immediately
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn permanent_failure_dead_letters() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    let consumer = Arc::new(AlwaysPermanentConsumer {
        consumer_id: "io.elembra.test.permanent".to_string(),
    });
    let dispatcher = Arc::new(OutboxDispatcher::new(
        store.clone(),
        vec![consumer as Arc<dyn OutboxConsumer>],
        OutboxWorkerConfig::default(),
        "permanent-worker".to_string(),
    ));
    let consumer_id = "io.elembra.test.permanent";

    dispatcher.tick().await;

    assert_eq!(
        delivery_state(&ctx.pool, consumer_id, &event).await,
        "dead_lettered",
        "a permanent failure must dead-letter on the first attempt"
    );
    let dlq = store
        .list_dead_letters(Some(consumer_id), 10)
        .await
        .unwrap();
    assert_eq!(dlq.len(), 1);
    assert_eq!(dlq[0].event_id, event.id);
    assert_eq!(dlq[0].consumer_id, consumer_id);
    assert_eq!(
        dlq[0].last_error.as_deref(),
        Some("poison data"),
        "the DLQ entry carries only safe metadata"
    );

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 11. Poison rows cannot crash the dispatcher
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn poison_event_cannot_crash_dispatcher() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    // Corrupt the stored envelope so claim-time re-validation fails. JSONB
    // rejects syntactically invalid JSON at write time, so poison means
    // valid JSON that is not a decodable envelope (same as the Phase-1
    // store-level test).
    sqlx::query(
        "UPDATE integration_outbox SET event_json = '{\"not\":\"an envelope\"}' WHERE source = $1 AND event_id = $2",
    )
    .bind(&event.source)
    .bind(event.id)
    .execute(&ctx.pool)
    .await
    .unwrap();

    let dispatcher = reference_dispatcher(store.clone(), &ctx.pool, OutboxWorkerConfig::default());
    dispatcher.tick().await;

    assert!(
        dispatcher
            .status()
            .last_tick_ok
            .load(std::sync::atomic::Ordering::Relaxed),
        "a poison row must not crash the tick"
    );
    let row = sqlx::query(
        "SELECT state, last_error FROM integration_deliveries WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(REFERENCE_MEMORY_PROJECTION_CONSUMER_ID)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(row.try_get::<String, _>("state").unwrap(), "dead_lettered");
    let last_error = row
        .try_get::<Option<String>, _>("last_error")
        .unwrap()
        .unwrap();
    assert!(
        last_error.contains("undecodable event_json"),
        "expected poison diagnostic, got: {last_error}"
    );

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 12. Validation fails closed at the store
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn validation_fails_closed() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;

    // Tenant/workspace mismatch → envelope validation rejects.
    let mut mismatched = files_created_event(ctx.tenant_id);
    mismatched.workspace_id = WorkspaceId(Uuid::new_v4());
    let mut tx = store.pool().begin().await.unwrap();
    let result = store.insert_in_tx(&mut tx, &mismatched).await;
    tx.rollback().await.unwrap();
    assert!(
        matches!(result, Err(OutboxStoreError::InvalidEvent(_))),
        "tenant/workspace mismatch must fail closed: {result:?}"
    );

    // An event type the source application's manifest does not publish →
    // ownership rejected (passes envelope validation, fails the registry).
    let mail_event = IntegrationEvent::builder()
        .source("elembra://io.elembra.mail")
        .r#type("io.elembra.mail.mail.created.v1")
        .tenant_id(TenantId(ctx.tenant_id))
        .workspace_id(WorkspaceId(ctx.tenant_id))
        .data(serde_json::json!({}))
        .build()
        .unwrap();
    let mut tx = store.pool().begin().await.unwrap();
    let result = store.insert_in_tx(&mut tx, &mail_event).await;
    tx.rollback().await.unwrap();
    assert!(
        matches!(result, Err(OutboxStoreError::OwnershipRejected(_))),
        "unowned event types must be rejected: {result:?}"
    );

    // The canonical Files event is accepted.
    let valid = files_created_event(ctx.tenant_id);
    let mut tx = store.pool().begin().await.unwrap();
    store.insert_in_tx(&mut tx, &valid).await.unwrap();
    tx.commit().await.unwrap();

    cleanup_events(&ctx.pool, &[&mismatched, &mail_event, &valid]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 13. An event's ResourceRef grants no Files access by itself
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn event_does_not_grant_resource_access() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;

    // Full source-authorizer setup (same as source_authorization_test.rs).
    let repo = Arc::new(PermissionResolverRepository::new(ctx.pool.clone()));
    let resolver = Arc::new(PermissionResolver::new(Arc::clone(&repo)));
    let application_registry = ApplicationRegistry::first_party().unwrap();
    let mut owner_registry = ResourceOwnerRegistry::new();
    owner_registry
        .register(
            Arc::new(FilesResourceOwner::new(
                Arc::clone(&resolver),
                repo,
                ctx.metadata_store.clone(),
                ctx.object_store.clone(),
            )),
            &application_registry,
        )
        .expect("Files owner registers against the canonical registry");
    let authorizer = SourceAuthorizer::new(owner_registry);

    let owner = ctx.create_test_user("event_owner").await;
    let stranger = ctx.create_test_user("event_stranger").await;
    let file = ctx
        .create_test_file(owner.id, None, "secret.txt", b"top secret")
        .await;

    // Publish a created event whose envelope references the file.
    let resource = ResourceRef::new(
        ApplicationId::new("io.elembra.files"),
        "file",
        file.id.to_string(),
    );
    let event = IntegrationEvent::builder()
        .source("elembra://io.elembra.files")
        .r#type(FILES_FILE_CREATED_V1)
        .tenant_id(TenantId(ctx.tenant_id))
        .workspace_id(WorkspaceId(ctx.tenant_id))
        .resource(resource.clone())
        .data(serde_json::json!({"name": "secret.txt"}))
        .build()
        .unwrap();
    publish(&store, &event).await;

    // Possession of the event's ResourceRef grants nothing by itself: a user
    // without a share is denied, even though the event "knows" the file.
    let stranger_ctx = PrincipalContext::user(
        PrincipalId(stranger.id),
        TenantId(ctx.tenant_id),
        WorkspaceId(ctx.tenant_id),
    );
    assert_eq!(
        authorizer
            .authorize(&stranger_ctx, &ActionCapability::new(FILES_READ), &resource)
            .await,
        Decision::Deny,
        "an event's ResourceRef must not grant Files access"
    );

    // Sanity: the same authorizer allows the actual owner (proves the
    // harness works and the denial above is about missing authority, not a
    // broken authorizer).
    let owner_ctx = PrincipalContext::user(
        PrincipalId(owner.id),
        TenantId(ctx.tenant_id),
        WorkspaceId(ctx.tenant_id),
    );
    assert_eq!(
        authorizer
            .authorize(&owner_ctx, &ActionCapability::new(FILES_READ), &resource)
            .await,
        Decision::Allow
    );

    // The reference consumer never consults the authorizer or fetches
    // content — it only projects `event.data`. Code-level invariant: the
    // consumer's only queries touch the receipt/effect tables (see
    // outbox_consumers.rs); nothing here constructs authorization from the
    // event.
    let consumer = Arc::new(ReferenceMemoryProjectionConsumer::new(
        ctx.pool.clone(),
        true,
    ));
    let outcome = consumer.process(&event).await;
    assert_eq!(outcome, ConsumerOutcome::Processed);

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 14. A serialized PrincipalContext inside event data is inert data
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn serialized_principal_context_is_not_trusted() {
    // HARD SECURITY INVARIANT: a `PrincipalContext` is constructed only at a
    // trusted boundary (an authenticated handler request). A serialized
    // context smuggled inside event `data` (or anywhere else in an
    // integration event) is NOT authorization proof — nothing in the outbox
    // pipeline may deserialize it into a context or build authority from it.
    // The reference consumer treats `data` as opaque JSON and projects only
    // the documented fields; this test documents that the effect table
    // cannot hold such a context at all.
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;

    // A "privileged" user context serialized as JSON, as a hostile or buggy
    // producer might embed it.
    let privileged = PrincipalContext::user(
        PrincipalId(Uuid::new_v4()),
        TenantId(ctx.tenant_id),
        WorkspaceId(ctx.tenant_id),
    );
    let embedded = serde_json::to_value(&privileged).unwrap();
    let event = IntegrationEvent::builder()
        .source("elembra://io.elembra.files")
        .r#type(FILES_FILE_CREATED_V1)
        .tenant_id(TenantId(ctx.tenant_id))
        .workspace_id(WorkspaceId(ctx.tenant_id))
        .data(serde_json::json!({
            "name": "context.txt",
            "mime_type": "text/plain",
            "size": 3,
            "version": "sha256:context",
            "principal_context": embedded,
        }))
        .build()
        .unwrap();
    publish(&store, &event).await;

    // The consumer processes it as plain data.
    let consumer = Arc::new(ReferenceMemoryProjectionConsumer::new(
        ctx.pool.clone(),
        true,
    ));
    let outcome = consumer.process(&event).await;
    assert_eq!(outcome, ConsumerOutcome::Processed);

    // The effect row projects exactly name/mime/size/version — nothing else.
    let row = sqlx::query(
        "SELECT name, mime_type, size, version FROM integration_reference_effects WHERE consumer_id = $1 AND event_id = $2",
    )
    .bind(REFERENCE_MEMORY_PROJECTION_CONSUMER_ID)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        row.try_get::<Option<String>, _>("name").unwrap().as_deref(),
        Some("context.txt")
    );
    assert_eq!(
        row.try_get::<Option<String>, _>("mime_type")
            .unwrap()
            .as_deref(),
        Some("text/plain")
    );
    assert_eq!(row.try_get::<Option<i64>, _>("size").unwrap(), Some(3));
    assert_eq!(
        row.try_get::<Option<String>, _>("version")
            .unwrap()
            .as_deref(),
        Some("sha256:context")
    );

    // The effect table has no column that could store an embedded context.
    let columns = sqlx::query_scalar::<_, String>(
        "SELECT column_name FROM information_schema.columns WHERE table_name = 'integration_reference_effects'",
    )
    .fetch_all(&ctx.pool)
    .await
    .unwrap();
    assert!(
        !columns
            .iter()
            .any(|c| c.contains("principal") || c.contains("actor") || c.contains("context")),
        "the projection table must not be able to persist authority context: {columns:?}"
    );

    // The extraction helper (used by the consumer) copies only the four
    // projection fields.
    assert_eq!(
        extract_projection(&event).0,
        Some("context.txt".to_string())
    );
    assert!(event.data.get("principal_context").is_some());

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 15. The envelope round-trip never involves an internal EventType enum
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn contract_independence_from_internal_event_enum() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    let claimed = store
        .claim_batch(
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &[FILES_FILE_CREATED_V1.to_string()],
            &OutboxConfig::default(),
            "roundtrip-worker",
        )
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    let claimed_event = &claimed[0].event;

    // The envelope is transport-neutral: `type` is a plain namespaced string
    // on the wire. The internal EventStore `EventType` enum (an entirely
    // different contract) is never involved in the round trip.
    let value = serde_json::to_value(claimed_event).unwrap();
    assert_eq!(value["type"], FILES_FILE_CREATED_V1);
    assert!(value["type"].is_string());
    let reparsed: IntegrationEvent = serde_json::from_value(value).unwrap();
    assert_eq!(&reparsed, claimed_event);
    assert_eq!(reparsed.r#type, FILES_FILE_CREATED_V1);
    assert_eq!(reparsed.source, "elembra://io.elembra.files");

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}
