//! Integration tests for the durable integration-event outbox (ADR-0031,
//! GitHub issue #212).
//!
//! Exercises the real store + reference consumer + dispatcher against the
//! dev database:
//!
//! * atomic publish (outbox row commits with the source mutation, never
//!   without it) — tests 1–3;
//! * durable consumer registration with eager fan-out obligations, offline
//!   recovery, at-least-once + idempotency, lease fencing and concurrent
//!   workers — tests 4–7;
//! * retry backoff, dead-lettering, requeue and redaction — tests 8–10;
//! * fail-closed behavior (poison rows, validation, ownership) — tests
//!   11–12;
//! * security invariants (an event grants no access; serialized authority
//!   context is inert data) — tests 13–14;
//! * transport-neutral envelope contract — test 15;
//! * durable-registration regressions: retention survival for an offline
//!   consumer, no historical backlog for new consumers, actor attribution
//!   (public-share uploads are never attributed to the owner), event-identity
//!   conflicts, and claim-batch bounds — tests 16–21;
//! * registration-contract regressions: empty subscription lists are
//!   rejected, an explicit broad-prefix consumer's offline obligation
//!   survives retention, identical re-registration is idempotent, and a
//!   changed subscription contract conflicts without touching rows or
//!   obligations — tests 22–25.
//!
//! The outbox tables are global (not tenant-scoped), so every test takes a
//! shared `SERIAL` guard and cleans up exactly the rows it created. Run with:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test --test outbox_integration_test -p rustshare-server \
//!     -- --ignored --test-threads=1

mod contracts;
use contracts::common::{setup_test_env, TestContext};
use contracts::reference_consumer::{
    ensure_effect_table, extract_projection, ReferenceMemoryProjectionConsumer,
    REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
};

use bytes::Bytes;
use rustshare_core::domain::{
    ActionCapability, ApplicationId, ApplicationRegistry, PrincipalId, TenantId, WorkspaceId,
};
use rustshare_core::services::{
    FileUploadActor, IntegrationEventFacts, IntegrationEventPublisher, IntegrationPublishError,
    PermissionResolver,
};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_integration_events::event_types::{FILES_FILE_CREATED_V1, FILES_FILE_UPDATED_V1};
use rustshare_integration_events::{ActorRef, ConsumerOutcome, IntegrationEvent, OutboxConsumer};
use rustshare_resource_auth::{
    Decision, PrincipalContext, ResourceOwnerRegistry, ResourceRef, SourceAuthorizer, FILES_READ,
};
use rustshare_server::authz::FilesResourceOwner;
use rustshare_server::config::OutboxWorkerConfig;
use rustshare_server::outbox_dispatcher::OutboxDispatcher;
use rustshare_storage::{OutboxConfig, OutboxStore, OutboxStoreError};
use sqlx::Row;
use std::collections::HashSet;
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

/// Remove every outbox-side row the suite creates (all four tables plus the
/// durable registrations) and ensure the test-support effect table exists.
/// Safe because every test in this file holds `SERIAL`; rows leaked by an
/// aborted previous run would otherwise be claimed by the next test (claims
/// filter by event type, not by test).
///
/// Deleting the `integration_consumers` rows also resets `registered_at`,
/// which tests with a registration-time gate (16, 17, 21) depend on.
async fn clean_slate(pool: &sqlx::PgPool) {
    ensure_effect_table(pool).await.unwrap();
    for table in [
        "integration_reference_effects",
        "integration_consumer_receipts",
        "integration_consumer_subscriptions",
        "integration_consumers",
        "integration_deliveries",
        "integration_outbox",
    ] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(pool)
            .await
            .unwrap();
    }
}

/// Register the reference consumer durably (subscriptions + effect table),
/// so publish-time eager fan-out creates its delivery obligations.
async fn register_ref_consumer(store: &OutboxStore, pool: &sqlx::PgPool) {
    ensure_effect_table(pool).await.unwrap();
    let consumer = ReferenceMemoryProjectionConsumer::new(pool.clone(), true);
    store
        .register_consumer(
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &consumer.subscriptions(),
        )
        .await
        .unwrap();
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
    // A registered consumer gets an eager delivery obligation at publish
    // time, so register before the uploads.
    register_ref_consumer(&store, &ctx.pool).await;
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

    // Eager fan-out: the registered consumer already has a pending delivery
    // obligation for the event, straight after publish.
    let obligation = sqlx::query(
        "SELECT state FROM integration_deliveries WHERE consumer_id = $1 AND source = $2 AND event_id = $3",
    )
    .bind(REFERENCE_MEMORY_PROJECTION_CONSUMER_ID)
    .bind(&first_event.source)
    .bind(first_event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        obligation.try_get::<String, _>("state").unwrap(),
        "pending",
        "a registered consumer must get an eager delivery obligation on publish"
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
    // Both events carried an obligation for the registered consumer.
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_deliveries WHERE consumer_id = $1 AND event_id = ANY($2)",
        )
        .bind(REFERENCE_MEMORY_PROJECTION_CONSUMER_ID)
        .bind(&[first_event.id, second_event.id][..])
        .fetch_one(&ctx.pool)
        .await
        .unwrap(),
        2,
        "each published event must create one pending obligation"
    );

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
// 4. Eager fan-out + offline consumer recovery
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn offline_consumer_recovers_and_processes() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    register_ref_consumer(&store, &ctx.pool).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    // Fan-out is eager: the registered consumer's delivery obligation row
    // exists right after publish (state pending); nothing is claimed until a
    // worker runs.
    assert_eq!(
        delivery_state(&ctx.pool, REFERENCE_MEMORY_PROJECTION_CONSUMER_ID, &event).await,
        "pending",
        "a registered consumer must have a pending obligation at publish time"
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
    register_ref_consumer(&store, &ctx.pool).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    let consumer = Arc::new(ReferenceMemoryProjectionConsumer::new(
        ctx.pool.clone(),
        true,
    ));
    for round in 1..=3 {
        let claimed = store
            .claim_batch(
                REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
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
    register_ref_consumer(&store, &ctx.pool).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    // A worker claims the event and "crashes" before processing.
    let claimed = store
        .claim_batch(
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
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
    register_ref_consumer(&store, &ctx.pool).await;
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
    let consumer_id = "io.elembra.test.flaky";
    // The stub consumer's durable registration must exist before publish so
    // the event fans out a pending obligation for it.
    store
        .register_consumer(consumer_id, &[FILES_FILE_CREATED_V1.to_string()])
        .await
        .unwrap();
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    const SECRET: &str = "token=abc123secret";
    let consumer = Arc::new(FlakyConsumer::new(consumer_id, 2, SECRET));
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
    let consumer_id = "io.elembra.test.dlq";
    store
        .register_consumer(consumer_id, &[FILES_FILE_CREATED_V1.to_string()])
        .await
        .unwrap();
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    const SECRET: &str = "token=secretvalue123";
    let consumer = Arc::new(AlwaysRetryThenSucceedConsumer {
        succeed: AtomicBool::new(false),
        secret: SECRET,
        consumer_id: consumer_id.to_string(),
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
    let consumer_id = "io.elembra.test.permanent";
    store
        .register_consumer(consumer_id, &[FILES_FILE_CREATED_V1.to_string()])
        .await
        .unwrap();
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    let consumer = Arc::new(AlwaysPermanentConsumer {
        consumer_id: consumer_id.to_string(),
    });
    let dispatcher = Arc::new(OutboxDispatcher::new(
        store.clone(),
        vec![consumer as Arc<dyn OutboxConsumer>],
        OutboxWorkerConfig::default(),
        "permanent-worker".to_string(),
    ));

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
    register_ref_consumer(&store, &ctx.pool).await;
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
    // contracts/reference_consumer.rs); nothing here constructs
    // authorization from the event.
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
    register_ref_consumer(&store, &ctx.pool).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    let claimed = store
        .claim_batch(
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
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

// ---------------------------------------------------------------------------
// 16. An offline consumer's pending obligation survives retention
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn long_offline_consumer_survives_retention() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    register_ref_consumer(&store, &ctx.pool).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    // The consumer is offline: no worker ever claims. Age the outbox row past
    // any retention horizon.
    sqlx::query(
        "UPDATE integration_outbox SET created_at = now() - interval '10 days' WHERE source = $1 AND event_id = $2",
    )
    .bind(&event.source)
    .bind(event.id)
    .execute(&ctx.pool)
    .await
    .unwrap();

    // Retention with a 1-hour horizon must NOT compact: the pending
    // obligation blocks deletion.
    let deleted = store.maintenance(1).await.unwrap();
    assert_eq!(deleted, 0, "a pending obligation must block compaction");
    let outbox_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_outbox WHERE source = $1 AND event_id = $2",
    )
    .bind(&event.source)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(outbox_count, 1, "the aged outbox row must still exist");
    assert_eq!(
        delivery_state(&ctx.pool, REFERENCE_MEMORY_PROJECTION_CONSUMER_ID, &event).await,
        "pending",
        "the obligation must still be pending"
    );

    // When the consumer comes back online, the event is still delivered.
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
        "the offline period must not lose the event"
    );
    assert_eq!(
        delivery_state(&ctx.pool, REFERENCE_MEMORY_PROJECTION_CONSUMER_ID, &event).await,
        "processed"
    );

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 17. A newly registered consumer gets no historical backlog
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn newly_registered_consumer_gets_no_historical_backlog() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    // Publish BEFORE any consumer is registered: no obligation is created.
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_deliveries WHERE event_id = $1",
        )
        .bind(event.id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap(),
        0,
        "no consumer was registered at publish time, so no obligation exists"
    );

    // Register the consumer after the fact.
    register_ref_consumer(&store, &ctx.pool).await;

    // Claiming must not backfill events created before registration.
    let claimed = store
        .claim_batch(
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &OutboxConfig::default(),
            "no-backlog-worker",
        )
        .await
        .unwrap();
    assert!(claimed.is_empty(), "no historical backlog may be claimed");

    // A dispatcher tick (which also re-registers) processes nothing either.
    let dispatcher = reference_dispatcher(store.clone(), &ctx.pool, OutboxWorkerConfig::default());
    dispatcher.tick().await;
    assert_eq!(
        effect_count(
            &ctx.pool,
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &[&event]
        )
        .await,
        0,
        "no effect for pre-registration events"
    );
    assert_eq!(
        receipt_count(
            &ctx.pool,
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &[&event]
        )
        .await,
        0,
        "no receipt for pre-registration events"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM integration_deliveries WHERE event_id = $1",
        )
        .bind(event.id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap(),
        0,
        "no obligation row may be created for pre-registration events"
    );

    // With no obligations at all, the event is fully delivered (vacuously)
    // and retention may compact it once it ages out.
    sqlx::query(
        "UPDATE integration_outbox SET created_at = now() - interval '10 days' WHERE source = $1 AND event_id = $2",
    )
    .bind(&event.source)
    .bind(event.id)
    .execute(&ctx.pool)
    .await
    .unwrap();
    let deleted = store.maintenance(1).await.unwrap();
    assert_eq!(deleted, 1, "an obligation-free event is deletable");
    let outbox_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_outbox WHERE source = $1 AND event_id = $2",
    )
    .bind(&event.source)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(outbox_count, 0, "retention must compact the old event");

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 18. A public-share upload is never attributed to the owner
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn public_share_upload_actor_not_attributed_to_owner() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let owner = ctx.create_test_user("public_share_owner").await;
    let publisher: Arc<dyn IntegrationEventPublisher<sqlx::Transaction<'static, sqlx::Postgres>>> =
        store.clone();
    let file_service = ctx.file_service().with_integration_publisher(publisher);

    // Same actor shape the public-share handler builds (see
    // handlers/public_shares.rs): no authenticated user.
    let public_actor = FileUploadActor {
        actor_type: "public_share_session".to_string(),
        actor_user_id: None,
        actor_share_id: Some(Uuid::new_v4()),
        actor_share_session_id: Some(Uuid::new_v4()),
        actor_display_name: Some("Anonymous Uploader".to_string()),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pub.txt");
    std::fs::write(&path, b"public upload").unwrap();
    file_service
        .upload_file_with_actor_from_path(
            owner.id,
            public_actor,
            "pub.txt".to_string(),
            None,
            &path,
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .unwrap();

    let rows = sqlx::query(
        "SELECT event_json FROM integration_outbox WHERE tenant_id = $1 ORDER BY created_at, event_id",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    let public_event: IntegrationEvent = serde_json::from_value(
        rows[0]
            .try_get::<serde_json::Value, _>("event_json")
            .unwrap(),
    )
    .unwrap();

    // No `elembraActor` extension on the wire, and no owner id anywhere in
    // the payload: the owner is NEVER used as a fallback actor.
    assert!(
        public_event.actor.is_none(),
        "a public-share upload must have no actor, got: {:?}",
        public_event.actor
    );
    let serialized = serde_json::to_value(&public_event).unwrap();
    assert!(
        serialized.get("elembraActor").is_none(),
        "the wire envelope must not carry elembraActor"
    );
    assert!(
        !public_event
            .data
            .to_string()
            .contains(&owner.id.to_string()),
        "event data must not reference the file owner: {}",
        public_event.data
    );

    // Contrast: an authenticated upload (any user) IS attributed.
    let path2 = dir.path().join("auth.txt");
    std::fs::write(&path2, b"authenticated upload").unwrap();
    file_service
        .upload_file_from_path(
            owner.id,
            "auth.txt".to_string(),
            None,
            &path2,
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .unwrap();
    let rows = sqlx::query(
        "SELECT event_json FROM integration_outbox WHERE tenant_id = $1 ORDER BY created_at, event_id",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    let auth_event: IntegrationEvent = serde_json::from_value(
        rows[1]
            .try_get::<serde_json::Value, _>("event_json")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        auth_event.actor,
        Some(ActorRef::Principal(PrincipalId(owner.id))),
        "an authenticated upload must be attributed to the acting principal"
    );
    let serialized = serde_json::to_value(&auth_event).unwrap();
    assert_eq!(
        serialized["elembraActor"],
        format!("principal:{}", owner.id),
        "the wire actor must be the canonical principal reference"
    );

    cleanup_events(&ctx.pool, &[&public_event, &auth_event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 19. A shared-recipient upload is attributed to the acting user
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn shared_recipient_upload_actor_is_acting_user() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let owner = ctx.create_test_user("shared_owner_upload").await;
    let recipient = ctx.create_test_user("shared_recipient_upload").await;
    let publisher: Arc<dyn IntegrationEventPublisher<sqlx::Transaction<'static, sqlx::Postgres>>> =
        store.clone();
    let file_service = ctx.file_service().with_integration_publisher(publisher);

    // A recipient uploads into the owner's root folder: the file still
    // belongs to the owner, but the acting user is the recipient.
    let actor = FileUploadActor {
        actor_type: "user".to_string(),
        actor_user_id: Some(recipient.id),
        actor_share_id: None,
        actor_share_session_id: None,
        actor_display_name: None,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shared.txt");
    std::fs::write(&path, b"recipient upload").unwrap();
    let file = file_service
        .upload_file_with_actor_from_path(
            owner.id,
            actor,
            "shared.txt".to_string(),
            None,
            &path,
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .unwrap();
    assert_eq!(
        file.owner_id, owner.id,
        "the file itself still belongs to the owner"
    );

    let rows = sqlx::query(
        "SELECT event_json FROM integration_outbox WHERE tenant_id = $1 ORDER BY created_at, event_id",
    )
    .bind(ctx.tenant_id)
    .fetch_all(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    let event: IntegrationEvent = serde_json::from_value(
        rows[0]
            .try_get::<serde_json::Value, _>("event_json")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        event.actor,
        Some(ActorRef::Principal(PrincipalId(recipient.id))),
        "the event actor must be the acting user, not the file owner"
    );
    let serialized = serde_json::to_value(&event).unwrap();
    assert_eq!(
        serialized["elembraActor"],
        format!("principal:{}", recipient.id)
    );

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 20. Republishing the same event identity with different payload conflicts
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn duplicate_event_identity_conflict_rolls_back_caller() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;

    // Republishing the identical payload is idempotent.
    let mut tx = store.pool().begin().await.unwrap();
    store.insert_in_tx(&mut tx, &event).await.unwrap();
    tx.commit().await.unwrap();

    // Same (source, event_id) with different data → identity conflict.
    let mut different_data = event.clone();
    different_data.data = serde_json::json!({"name": "other.txt"});
    let mut tx = store.pool().begin().await.unwrap();
    let result = store.insert_in_tx(&mut tx, &different_data).await;
    assert!(
        matches!(result, Err(OutboxStoreError::EventIdentityConflict { .. })),
        "a payload change under a stable event id must conflict: {result:?}"
    );
    tx.rollback().await.unwrap();

    // Same (source, event_id) with a different type → identity conflict too.
    let mut different_type = event.clone();
    different_type.r#type = FILES_FILE_UPDATED_V1.to_string();
    let mut tx = store.pool().begin().await.unwrap();
    let result = store.insert_in_tx(&mut tx, &different_type).await;
    assert!(
        matches!(result, Err(OutboxStoreError::EventIdentityConflict { .. })),
        "a type change under a stable event id must conflict: {result:?}"
    );
    tx.rollback().await.unwrap();

    // Caller-tx rollback proof: marker rows written before the conflicting
    // publish must vanish with the caller's rollback, and the outbox keeps
    // exactly the one original row.
    let marker_id = Uuid::new_v4();
    let mut tx = store.pool().begin().await.unwrap();
    sqlx::query(
        r#"
        INSERT INTO integration_consumer_receipts
            (consumer_id, source, event_id, event_type, tenant_id, workspace_id, processed_at)
        VALUES ('identity-conflict-marker', $1, $2, $3, $4, $5, now())
        "#,
    )
    .bind(&event.source)
    .bind(marker_id)
    .bind(&event.r#type)
    .bind(event.tenant_id.0)
    .bind(event.workspace_id.0)
    .execute(&mut *tx)
    .await
    .unwrap();
    let result = store.insert_in_tx(&mut tx, &different_data).await;
    assert!(
        matches!(
            result,
            Err(OutboxStoreError::EventIdentityConflict { ref source, event_id })
                if source == &event.source && event_id == event.id
        ),
        "the conflict must name the offending identity: {result:?}"
    );
    drop(tx); // rollback without commit

    let marker_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_consumer_receipts WHERE consumer_id = 'identity-conflict-marker'",
    )
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        marker_count, 0,
        "the caller's tx rollback must remove its rows"
    );
    let outbox_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_outbox WHERE source = $1 AND event_id = $2",
    )
    .bind(&event.source)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        outbox_count, 1,
        "conflicting republishes must never add or replace the outbox row"
    );

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 21. claim_batch respects the batch bound across both claim phases
// ---------------------------------------------------------------------------

/// Insert an outbox row directly (bypassing the store) with a valid
/// envelope, `created_at = now()`.
async fn insert_outbox_row(pool: &sqlx::PgPool, event: &IntegrationEvent) {
    sqlx::query(
        r#"
        INSERT INTO integration_outbox
            (source, event_id, event_type, application_id, tenant_id, workspace_id, event_json,
             created_at, available_at)
        VALUES ($1, $2, $3, 'io.elembra.files', $4, $5, $6, now(), now())
        "#,
    )
    .bind(&event.source)
    .bind(event.id)
    .bind(&event.r#type)
    .bind(event.tenant_id.0)
    .bind(event.workspace_id.0)
    .bind(serde_json::to_value(event).unwrap())
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a directly-created pending delivery obligation for `event` (the
/// event's outbox row must exist first).
async fn insert_pending_delivery(pool: &sqlx::PgPool, consumer_id: &str, event: &IntegrationEvent) {
    sqlx::query(
        r#"
        INSERT INTO integration_deliveries
            (consumer_id, source, event_id, event_type, tenant_id, workspace_id, state, available_at)
        VALUES ($1, $2, $3, $4, $5, $6, 'pending', now() - interval '1 second')
        "#,
    )
    .bind(consumer_id)
    .bind(&event.source)
    .bind(event.id)
    .bind(&event.r#type)
    .bind(event.tenant_id.0)
    .bind(event.workspace_id.0)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn claim_batch_respects_bound_across_phases() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    register_ref_consumer(&store, &ctx.pool).await;
    // Registered long ago, so step 2 (first delivery) may backfill rows
    // created "now".
    sqlx::query(
        "UPDATE integration_consumers SET registered_at = now() - interval '1 day' WHERE consumer_id = $1",
    )
    .bind(REFERENCE_MEMORY_PROJECTION_CONSUMER_ID)
    .execute(&ctx.pool)
    .await
    .unwrap();

    // 3 events with pending delivery rows (claimable by step 1) and 3 events
    // with no delivery row (only reachable via step 2).
    let mut with_delivery = Vec::new();
    for _ in 0..3 {
        let event = files_created_event(ctx.tenant_id);
        insert_outbox_row(&ctx.pool, &event).await;
        insert_pending_delivery(&ctx.pool, REFERENCE_MEMORY_PROJECTION_CONSUMER_ID, &event).await;
        with_delivery.push(event);
    }
    let mut without_delivery = Vec::new();
    for _ in 0..3 {
        let event = files_created_event(ctx.tenant_id);
        insert_outbox_row(&ctx.pool, &event).await;
        without_delivery.push(event);
    }

    let config = OutboxConfig {
        claim_batch_size: 4,
        ..OutboxConfig::default()
    };
    let first = store
        .claim_batch(
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &config,
            "batch-worker-1",
        )
        .await
        .unwrap();
    assert_eq!(
        first.len(),
        4,
        "first claim must respect the batch bound (3 re-claims + 1 first-delivery)"
    );

    let second = store
        .claim_batch(
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &config,
            "batch-worker-2",
        )
        .await
        .unwrap();
    assert_eq!(second.len(), 2, "the remainder must be claimed next");

    // Every event is claimed exactly once across the two batches.
    let mut seen: HashSet<(String, Uuid)> = HashSet::new();
    for claimed in first.iter().chain(second.iter()) {
        assert!(
            seen.insert((claimed.source.clone(), claimed.event_id)),
            "event {:?}/{} claimed twice",
            claimed.source,
            claimed.event_id
        );
    }
    assert_eq!(
        seen.len(),
        6,
        "all six events must be claimed exactly once in total"
    );

    // Nothing is left for a third claim (all rows claimed / delivered).
    let third = store
        .claim_batch(
            REFERENCE_MEMORY_PROJECTION_CONSUMER_ID,
            &config,
            "batch-worker-3",
        )
        .await
        .unwrap();
    assert!(
        third.is_empty(),
        "a third claim must find nothing claimable"
    );

    let all_events: Vec<&IntegrationEvent> = with_delivery
        .iter()
        .chain(without_delivery.iter())
        .collect();
    cleanup_events(&ctx.pool, &all_events).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 22. Empty subscription registration is rejected
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn empty_subscription_registration_is_rejected() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let consumer_id = format!("io.elembra.test.empty-{}", Uuid::new_v4());

    // An empty pattern list cannot be discovered at eager fan-out, so no
    // durable obligation would ever be created: registration must fail with
    // a typed error and must not create any rows.
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
    assert!(
        !store.is_consumer_enabled(&consumer_id).await.unwrap(),
        "a rejected registration must not enable anything"
    );

    // The consumer id is still usable afterwards with an explicit pattern.
    store
        .register_consumer(&consumer_id, &["io.elembra.*".to_string()])
        .await
        .unwrap();
    assert_eq!(
        store.consumer_subscriptions(&consumer_id).await.unwrap(),
        vec!["io.elembra.*".to_string()]
    );

    sqlx::query("DELETE FROM integration_consumers WHERE consumer_id = $1")
        .bind(&consumer_id)
        .execute(&ctx.pool)
        .await
        .unwrap();
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 23. An offline broad-prefix consumer's obligation survives retention
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn broad_prefix_consumer_offline_obligation_survives_retention() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let consumer_id = format!("io.elembra.test.broad-{}", Uuid::new_v4());

    // Broad consumers must declare an explicit prefix — NOT an empty list.
    store
        .register_consumer(&consumer_id, &["io.elembra.*".to_string()])
        .await
        .unwrap();
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;
    assert_eq!(
        delivery_state(&ctx.pool, &consumer_id, &event).await,
        "pending",
        "a matching broad-prefix consumer must get an eager obligation"
    );

    // The consumer stays fully offline. Age the outbox row past any
    // retention horizon and run maintenance: the pending obligation must
    // block compaction.
    sqlx::query(
        "UPDATE integration_outbox SET created_at = now() - interval '10 days' WHERE source = $1 AND event_id = $2",
    )
    .bind(&event.source)
    .bind(event.id)
    .execute(&ctx.pool)
    .await
    .unwrap();
    let deleted = store.maintenance(1).await.unwrap();
    assert_eq!(deleted, 0, "a pending obligation must block compaction");
    let outbox_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_outbox WHERE source = $1 AND event_id = $2",
    )
    .bind(&event.source)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(outbox_count, 1, "the aged outbox row must still exist");
    assert_eq!(
        delivery_state(&ctx.pool, &consumer_id, &event).await,
        "pending",
        "the obligation must still be pending"
    );

    // The consumer returns later and processes the event.
    let claimed = store
        .claim_batch(&consumer_id, &OutboxConfig::default(), "late-worker")
        .await
        .unwrap();
    assert_eq!(
        claimed.len(),
        1,
        "the offline period must not lose the event"
    );
    assert_eq!(claimed[0].event, event);
    let consumer = Arc::new(ReferenceMemoryProjectionConsumer::new(
        ctx.pool.clone(),
        true,
    ));
    let outcome = consumer.process(&claimed[0].event).await;
    assert_eq!(outcome, ConsumerOutcome::Processed);
    assert!(store
        .acknowledge(
            &consumer_id,
            &claimed[0].source,
            claimed[0].event_id,
            claimed[0].claim_token,
        )
        .await
        .unwrap());
    assert_eq!(
        delivery_state(&ctx.pool, &consumer_id, &event).await,
        "processed"
    );

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 24. Re-registering with identical subscriptions is idempotent
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn re_registration_with_identical_subscriptions_is_idempotent() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let consumer_id = format!("io.elembra.test.rereg-idem-{}", Uuid::new_v4());

    store
        .register_consumer(
            &consumer_id,
            &[
                "io.elembra.files.file.created.v1".to_string(),
                "io.elembra.files.file.updated.v1".to_string(),
            ],
        )
        .await
        .unwrap();
    let registered_at = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
        "SELECT registered_at FROM integration_consumers WHERE consumer_id = $1",
    )
    .bind(&consumer_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert!(store.is_consumer_enabled(&consumer_id).await.unwrap());

    // Disable, then re-register with the identical normalized set — passed
    // unsorted and with a duplicate. Must succeed idempotently, preserving
    // `enabled` and `registered_at`.
    assert!(store
        .set_consumer_enabled(&consumer_id, false)
        .await
        .unwrap());
    store
        .register_consumer(
            &consumer_id,
            &[
                "io.elembra.files.file.updated.v1".to_string(),
                "io.elembra.files.file.created.v1".to_string(),
                "io.elembra.files.file.updated.v1".to_string(),
            ],
        )
        .await
        .unwrap();
    assert!(
        !store.is_consumer_enabled(&consumer_id).await.unwrap(),
        "identical re-registration must not reset enabled"
    );
    assert_eq!(
        store.consumer_subscriptions(&consumer_id).await.unwrap(),
        vec![
            "io.elembra.files.file.created.v1".to_string(),
            "io.elembra.files.file.updated.v1".to_string(),
        ],
        "the durable set is the sorted, deduplicated contract"
    );
    let registered_at_after = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
        "SELECT registered_at FROM integration_consumers WHERE consumer_id = $1",
    )
    .bind(&consumer_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        registered_at_after, registered_at,
        "identical re-registration must not update registered_at"
    );
    // `list_consumers` reflects the unchanged registration.
    let consumers = store.list_consumers().await.unwrap();
    let registration = consumers
        .iter()
        .find(|c| c.consumer_id == consumer_id)
        .expect("consumer must be listed");
    assert!(!registration.enabled);
    assert_eq!(registration.registered_at, registered_at);

    sqlx::query("DELETE FROM integration_consumers WHERE consumer_id = $1")
        .bind(&consumer_id)
        .execute(&ctx.pool)
        .await
        .unwrap();
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// 25. Re-registering with different subscriptions conflicts, rows untouched
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn re_registration_with_different_subscriptions_conflicts() {
    let _guard = SERIAL.lock().await;
    let ctx = setup_test_env().await;
    clean_slate(&ctx.pool).await;
    let store = setup_store(&ctx).await;
    let consumer_id = format!("io.elembra.test.rereg-conflict-{}", Uuid::new_v4());

    store
        .register_consumer(&consumer_id, &[FILES_FILE_CREATED_V1.to_string()])
        .await
        .unwrap();
    let registered_at = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
        "SELECT registered_at FROM integration_consumers WHERE consumer_id = $1",
    )
    .bind(&consumer_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    // A pending obligation exists for pattern A (eager fan-out at publish).
    let event = files_created_event(ctx.tenant_id);
    publish(&store, &event).await;
    assert_eq!(
        delivery_state(&ctx.pool, &consumer_id, &event).await,
        "pending"
    );

    // Re-register with pattern B: v1alpha1 contracts are immutable.
    let result = store
        .register_consumer(&consumer_id, &[FILES_FILE_UPDATED_V1.to_string()])
        .await;
    assert!(
        matches!(
            result,
            Err(OutboxStoreError::ConsumerRegistrationConflict { .. })
        ),
        "a changed subscription contract must conflict: {result:?}"
    );
    // No row changed: subscriptions, enabled and registered_at are intact.
    assert_eq!(
        store.consumer_subscriptions(&consumer_id).await.unwrap(),
        vec![FILES_FILE_CREATED_V1.to_string()],
        "subscription rows must be unchanged after a conflict"
    );
    assert!(
        store.is_consumer_enabled(&consumer_id).await.unwrap(),
        "the consumer row must be unchanged after a conflict"
    );
    let registered_at_after = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
        "SELECT registered_at FROM integration_consumers WHERE consumer_id = $1",
    )
    .bind(&consumer_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(registered_at_after, registered_at);

    // The pre-existing pending obligation is untouched: maintenance must not
    // delete the event while it is pending…
    sqlx::query(
        "UPDATE integration_outbox SET created_at = now() - interval '10 days' WHERE source = $1 AND event_id = $2",
    )
    .bind(&event.source)
    .bind(event.id)
    .execute(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        store.maintenance(1).await.unwrap(),
        0,
        "a pending obligation must block compaction after a conflicted re-registration"
    );
    let outbox_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_outbox WHERE source = $1 AND event_id = $2",
    )
    .bind(&event.source)
    .bind(event.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(outbox_count, 1);

    // …and the obligation remains claimable afterwards.
    let claimed = store
        .claim_batch(
            &consumer_id,
            &OutboxConfig::default(),
            "post-conflict-worker",
        )
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1, "the obligation must still be claimable");
    assert_eq!(claimed[0].event, event);
    assert!(store
        .acknowledge(
            &consumer_id,
            &claimed[0].source,
            claimed[0].event_id,
            claimed[0].claim_token,
        )
        .await
        .unwrap());
    assert_eq!(
        delivery_state(&ctx.pool, &consumer_id, &event).await,
        "processed"
    );

    cleanup_events(&ctx.pool, &[&event]).await;
    ctx.cleanup().await;
}
