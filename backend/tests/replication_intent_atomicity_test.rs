//! Transactional replication intent for file mutations (post-fix contract).
//!
//! The durable replication intent (the `replication_jobs` row and the file
//! version's `queued` state) is recorded INSIDE the file-mutation transaction,
//! alongside the `files` / `file_versions` rows, the aggregate event and the
//! integration outbox row. A failure at any step therefore rolls the whole
//! mutation back: the service can no longer report a committed mutation as a
//! failure, and the replication obligation can no longer be half-recorded.
//!
//! The post-commit `ReplicationStateChanged` event publication is best-effort
//! by design: the committed job row is the authoritative intent, so an event
//! publication failure is logged and must not fail the request.
//!
//! Invariants proven here:
//!
//! * **A** — with an enabled target and no faults, upload commits the file,
//!   the version (`replication_state = 'queued'`), exactly one worker-eligible
//!   job row and the `io.elembra.files.file.created.v1` outbox row
//!   atomically, and the post-commit `ReplicationStateChanged` event is
//!   published (deterministic without faults).
//! * **B** — a replication-job insert failure aborts the whole mutation: no
//!   `files` row, no `file_versions` row, no outbox row.
//! * **C** — a version `replication_state` update failure (via the
//!   edit-overwrite upsert path) aborts the whole mutation the same way.
//! * **D** — a `ReplicationStateChanged` event-insert failure happens
//!   post-commit and must NOT fail the request: the mutation, the job row
//!   (worker-eligible) and the outbox row stay committed; only the event is
//!   absent.
//! * **E** — an enabled-target lookup failure aborts the whole mutation.
//! * **F** — with zero enabled targets, behavior is unchanged: `Ok`, version
//!   state `primary_written`, no job row.
//! * **G** — retries stay idempotent: re-uploading identical content is a
//!   no-op success, and an edit-overwrite that reuses a version id appends a
//!   second job row for that version. Multiple jobs per version are accepted
//!   existing behavior: this PR deliberately adds no unique constraint (an
//!   edit-overwrite replaces the version row's content, and each mutation
//!   queues its own intent; the worker processes every job idempotently).
//! * **H** — the inserted job row satisfies the exact lease predicate the
//!   replication worker uses (`lease_replication_jobs`), without running the
//!   worker.
//!
//! Snapshot semantics (documented limitation): the enabled-target count is
//! read inside the mutation transaction. A target enabled concurrently after
//! the count is not part of that mutation's obligations; it is reconciled by
//! future uploads, each of which queues its own intent.
//!
//! Failure injection is deterministic: PL/pgSQL triggers that
//! `RAISE EXCEPTION`, created and dropped inside each test against the
//! disposable database (a table rename stands in for a broken target lookup,
//! because `DROP TABLE` is blocked by the `replication_attempts` foreign key).
//! No sleeps, no timing races.
//!
//! Run against the disposable local stack (never the developer database):
//!
//! ```text
//! DATABASE_URL=postgres://rustshare:<pw>@127.0.0.1:15432/rustshare_test \
//! S3_ENDPOINT=http://127.0.0.1:19000 S3_BUCKET=rustshare-test-integration \
//! AWS_ACCESS_KEY_ID=<rustfs-root-user> AWS_SECRET_ACCESS_KEY=<rustfs-root-password> \
//! RUSTSHARE_TEST_DISPOSABLE_DB=1 RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1 \
//!   cargo test -p rustshare-server --test replication_intent_atomicity_test \
//!     -- --ignored --test-threads=1
//! ```
//!
//! (Historical note: the pre-fix defect — post-commit queueing reporting `Err`
//! for an already-committed mutation — was reproduced against this suite's
//! four injection points before the fix; the before/after evidence matrix is
//! in the PR body.)

mod contracts;

use contracts::common::{cleanup_tenant, setup_test_env, TestContext};

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use bytes::Bytes;
use rustshare_core::domain::ApplicationRegistry;
use rustshare_core::services::{FileError, FileService, IntegrationEventPublisher};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore, OutboxStore};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

/// The replication-target / trigger fixtures are process-global, so every test
/// serializes on this guard (same convention as the failure-atomicity suite).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

const CREATED_EVENT_TYPE: &str = "io.elembra.files.file.created.v1";
const UPDATED_EVENT_TYPE: &str = "io.elembra.files.file.updated.v1";
/// `EventType` serializes with `#[serde(tag = "type")]`, which is exactly what
/// the `events.event_type` column stores.
const REPLICATION_STATE_CHANGED_EVENT_TYPE: &str = r#"{"type":"ReplicationStateChanged"}"#;

// ---------------------------------------------------------------------------
// Disposable-target guards
// ---------------------------------------------------------------------------

fn is_disposable_test_database_name(database_name: &str) -> bool {
    database_name == "rustshare_test" || database_name.starts_with("rustshare_test_")
}

/// Only loopback endpoints, credential-free URLs and `rustshare-test*` buckets
/// may receive the fault-injected writes.
fn is_disposable_object_store_target(endpoint: &str, bucket: &str) -> bool {
    let Ok(endpoint) = url::Url::parse(endpoint) else {
        return false;
    };
    let local_endpoint = matches!(endpoint.host_str(), Some("localhost" | "127.0.0.1"));
    local_endpoint
        && matches!(endpoint.scheme(), "http" | "https")
        && endpoint.username().is_empty()
        && endpoint.password().is_none()
        && (bucket == "rustshare-test"
            || bucket.starts_with("rustshare-test-")
            || bucket.starts_with("rustshare-test_"))
}

fn assert_disposable_test_database(database_url: &str) {
    let parsed = url::Url::parse(database_url).expect("DATABASE_URL must be a PostgreSQL URL");
    let database_name = parsed.path().trim_start_matches('/');
    assert!(
        is_disposable_test_database_name(database_name),
        "refusing to run fault-injection tests unless the database name is `rustshare_test` or starts with `rustshare_test_`"
    );
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB")
            .ok()
            .as_deref(),
        Some("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_DB=1 only after confirming this specifically named database is disposable"
    );
}

fn assert_disposable_test_object_store() {
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE")
            .ok()
            .as_deref(),
        Some("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1 only after confirming the object store is disposable"
    );
    let endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .expect("an explicit local S3_ENDPOINT or RUSTFS_ENDPOINT is required");
    let bucket = std::env::var("S3_BUCKET")
        .or_else(|_| std::env::var("RUSTFS_BUCKET"))
        .expect("an explicit rustshare-test S3_BUCKET or RUSTFS_BUCKET is required");
    assert!(
        is_disposable_object_store_target(&endpoint, &bucket),
        "refusing fault-injected object-store writes unless the endpoint is loopback and the bucket is rustshare-test[-_]*"
    );
}

fn assert_disposable_environment() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point at the disposable rustshare_test database");
    assert_disposable_test_database(&database_url);
    assert_disposable_test_object_store();
}

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

type TxPublisher = Arc<dyn IntegrationEventPublisher<sqlx::Transaction<'static, sqlx::Postgres>>>;

type RealFileService =
    FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository>;

/// The real durable outbox publisher, so outbox assertions are meaningful
/// rather than an artifact of an unattached publisher.
fn outbox_publisher(ctx: &TestContext) -> TxPublisher {
    let registry = Arc::new(ApplicationRegistry::first_party().unwrap());
    Arc::new(OutboxStore::new(ctx.pool.clone(), registry))
}

fn real_file_service(ctx: &TestContext) -> RealFileService {
    ctx.file_service()
        .with_integration_publisher(outbox_publisher(ctx))
}

fn blob_key(content: &[u8]) -> String {
    format!("blobs/{}", hex::encode(Sha256::digest(content)))
}

/// Seed exactly one enabled replication target. The endpoint is deliberately
/// unreachable (`http://127.0.0.1:1`); no replication worker runs in tests, so
/// the job row is never consumed. `replication_targets` is global (no tenant
/// column): the caller MUST remove the row again (see [`FixtureCleanupGuard`]).
async fn seed_enabled_replication_target(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO replication_targets (id, name, destination_type, endpoint, enabled) \
         VALUES ($1, $2, 's3', 'http://127.0.0.1:1', TRUE)",
    )
    .bind(id)
    .bind(format!("atomicity-test-target-{id}"))
    .execute(pool)
    .await
    .expect("seed enabled replication target");
    id
}

async fn count_enabled_targets(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM replication_targets WHERE enabled = TRUE",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn file_row_count(ctx: &TestContext, path: &str, owner_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM files WHERE path = $1 AND owner_id = $2")
        .bind(path)
        .bind(owner_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap()
}

async fn tenant_outbox_count(ctx: &TestContext, event_type: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1 AND event_type = $2",
    )
    .bind(ctx.tenant_id)
    .bind(event_type)
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
}

async fn version_rows_with_hash(ctx: &TestContext, content_hash: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM file_versions WHERE content_hash = $1 AND tenant_id = $2",
    )
    .bind(content_hash)
    .bind(ctx.tenant_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
}

async fn file_version_count(ctx: &TestContext, file_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM file_versions WHERE file_id = $1")
        .bind(file_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap()
}

async fn latest_version_state(ctx: &TestContext, file_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT replication_state FROM file_versions WHERE file_id = $1 ORDER BY version_number DESC LIMIT 1",
    )
    .bind(file_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
}

async fn latest_version_id(ctx: &TestContext, file_id: Uuid) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM file_versions WHERE file_id = $1 ORDER BY version_number DESC LIMIT 1",
    )
    .bind(file_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
}

async fn replication_job_count(ctx: &TestContext, version_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM replication_jobs WHERE file_version_id = $1")
        .bind(version_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap()
}

/// Replication job rows for a version, as (id, status, storage_key).
async fn replication_job_rows(ctx: &TestContext, version_id: Uuid) -> Vec<(Uuid, String, String)> {
    sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT id, status, storage_key FROM replication_jobs WHERE file_version_id = $1 ORDER BY created_at ASC",
    )
    .bind(version_id)
    .fetch_all(&ctx.pool)
    .await
    .unwrap()
}

/// Number of replication job rows for a version that satisfy the worker lease
/// candidate predicate from `lease_replication_jobs`
/// (`status IN ('queued','retrying') AND next_attempt_at <= NOW()`), ignoring
/// the lease-expiry half (freshly inserted jobs have `leased_at IS NULL`).
async fn worker_eligible_job_count(ctx: &TestContext, version_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM replication_jobs \
         WHERE file_version_id = $1 AND status IN ('queued', 'retrying') AND next_attempt_at <= NOW()",
    )
    .bind(version_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
}

async fn replication_state_changed_event_count(ctx: &TestContext, file_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM events WHERE aggregate_id = $1 AND event_type = $2",
    )
    .bind(file_id)
    .bind(REPLICATION_STATE_CHANGED_EVENT_TYPE)
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
}

/// Create a trigger that raises on `BEFORE INSERT ON replication_jobs`.
async fn inject_job_insert_failure(pool: &PgPool) {
    sqlx::query(
        "CREATE OR REPLACE FUNCTION inject_replication_job_insert_failure() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected replication job insert failure'; END; $$",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER inject_replication_job_insert \
         BEFORE INSERT ON replication_jobs FOR EACH ROW \
         EXECUTE FUNCTION inject_replication_job_insert_failure()",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Create a trigger that raises on `BEFORE UPDATE OF replication_state ON
/// file_versions`.
async fn inject_version_state_update_failure(pool: &PgPool) {
    sqlx::query(
        "CREATE OR REPLACE FUNCTION inject_version_state_update_failure() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected version state update failure'; END; $$",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER inject_version_state_update \
         BEFORE UPDATE OF replication_state ON file_versions FOR EACH ROW \
         EXECUTE FUNCTION inject_version_state_update_failure()",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Create a trigger that raises on inserts of `ReplicationStateChanged` events.
async fn inject_replication_event_insert_failure(pool: &PgPool) {
    sqlx::query(
        "CREATE OR REPLACE FUNCTION inject_replication_event_insert_failure() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected replication event insert failure'; END; $$",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER inject_replication_event_insert \
         BEFORE INSERT ON events FOR EACH ROW \
         WHEN (NEW.event_type = '{\"type\":\"ReplicationStateChanged\"}') \
         EXECUTE FUNCTION inject_replication_event_insert_failure()",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn drop_injected_triggers(pool: &PgPool) {
    sqlx::query("DROP TRIGGER IF EXISTS inject_replication_job_insert ON replication_jobs")
        .execute(pool)
        .await
        .ok();
    sqlx::query("DROP TRIGGER IF EXISTS inject_version_state_update ON file_versions")
        .execute(pool)
        .await
        .ok();
    sqlx::query("DROP TRIGGER IF EXISTS inject_replication_event_insert ON events")
        .execute(pool)
        .await
        .ok();
}

/// Rename `replication_targets` away so the enabled-target count fails with
/// "relation does not exist" (non-destructive: `DROP TABLE` is blocked by the
/// `replication_attempts` foreign key, and a rename keeps every constraint
/// intact for the rename-back in teardown).
async fn hide_replication_targets_table(pool: &PgPool) {
    sqlx::query("ALTER TABLE replication_targets RENAME TO replication_targets_injected_missing")
        .execute(pool)
        .await
        .unwrap();
}

async fn restore_replication_targets_table(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("ALTER TABLE replication_targets_injected_missing RENAME TO replication_targets")
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete every fixture row this suite creates, for one tenant.
async fn cleanup_fixture(ctx: &TestContext, user_ids: &[Uuid], blob_keys: &[String]) {
    sqlx::query("DELETE FROM integration_outbox WHERE tenant_id = $1")
        .bind(ctx.tenant_id)
        .execute(&ctx.pool)
        .await
        .ok();
    for user_id in user_ids {
        sqlx::query("DELETE FROM events WHERE user_id = $1")
            .bind(user_id)
            .execute(&ctx.pool)
            .await
            .ok();
    }
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
    for key in blob_keys {
        sqlx::query("DELETE FROM object_gc_queue WHERE object_key = $1")
            .bind(key)
            .execute(&ctx.pool)
            .await
            .ok();
    }
    for key in blob_keys {
        let _ = ctx.object_store.delete(key).await;
    }
}

/// Panic-path cleanup for tenant rows, S3 objects, the seeded replication
/// target and any injected trigger (all of which are global or cross-tenant).
struct FixtureCleanupGuard {
    database_url: String,
    tenant_id: Uuid,
    user_ids: Vec<Uuid>,
    blob_keys: Vec<String>,
    target_ids: Vec<Uuid>,
    object_store: Arc<ObjectStore>,
    armed: bool,
}

impl FixtureCleanupGuard {
    fn new(ctx: &TestContext) -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://rustshare:changeme@localhost:5432/rustshare".to_string()
            }),
            tenant_id: ctx.tenant_id,
            user_ids: Vec::new(),
            blob_keys: Vec::new(),
            target_ids: Vec::new(),
            object_store: ctx.object_store.clone(),
            armed: true,
        }
    }

    fn track_users(&mut self, user_ids: &[Uuid]) {
        self.user_ids.extend_from_slice(user_ids);
    }

    fn track_keys(&mut self, blob_keys: &[String]) {
        self.blob_keys.extend_from_slice(blob_keys);
    }

    fn track_target(&mut self, target_id: Uuid) {
        self.target_ids.push(target_id);
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for FixtureCleanupGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let database_url = self.database_url.clone();
        let tenant_id = self.tenant_id;
        let user_ids = std::mem::take(&mut self.user_ids);
        let blob_keys = std::mem::take(&mut self.blob_keys);
        let target_ids = std::mem::take(&mut self.target_ids);
        let object_store = self.object_store.clone();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build the panic-path cleanup runtime");
            runtime.block_on(async move {
                match PgPoolOptions::new()
                    .acquire_timeout(Duration::from_secs(5))
                    .connect(&database_url)
                    .await
                {
                    Ok(pool) => {
                        // Undo a possibly leftover table rename first so the
                        // trigger drops and target deletes below can run.
                        restore_replication_targets_table(&pool).await.ok();
                        drop_injected_triggers(&pool).await;
                        for target_id in &target_ids {
                            sqlx::query("DELETE FROM replication_targets WHERE id = $1")
                                .bind(target_id)
                                .execute(&pool)
                                .await
                                .ok();
                        }
                        sqlx::query("DELETE FROM integration_outbox WHERE tenant_id = $1")
                            .bind(tenant_id)
                            .execute(&pool)
                            .await
                            .ok();
                        for user_id in &user_ids {
                            sqlx::query("DELETE FROM events WHERE user_id = $1")
                                .bind(user_id)
                                .execute(&pool)
                                .await
                                .ok();
                        }
                        cleanup_tenant(&pool, tenant_id).await;
                        for key in &blob_keys {
                            sqlx::query("DELETE FROM object_gc_queue WHERE object_key = $1")
                                .bind(key)
                                .execute(&pool)
                                .await
                                .ok();
                        }
                        for key in &blob_keys {
                            let _ = object_store.delete(key).await;
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "FixtureCleanupGuard: panic-path cleanup could not connect to the \
                             disposable database for tenant {tenant_id}, so its fixtures were \
                             left behind: {error}"
                        );
                    }
                }
            });
            let _ = finished_tx.send(());
        });
        match finished_rx.recv_timeout(Duration::from_secs(30)) {
            Ok(()) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                eprintln!(
                    "FixtureCleanupGuard: panic-path cleanup for tenant {tenant_id} did not \
                     finish within 30s; abandoning the detached cleanup thread and leaving its \
                     fixtures behind"
                );
                return;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                eprintln!(
                    "FixtureCleanupGuard: panic-path cleanup for tenant {tenant_id} panicked \
                     before it reported completion; its fixtures may be left behind"
                );
            }
        }
        let _ = handle.join();
    }
}

// ---------------------------------------------------------------------------
// A. happy path: mutation and replication intent commit atomically
// ---------------------------------------------------------------------------

/// **Invariant A** — with an enabled target and no faults, the upload commits
/// the file row, the version row (`replication_state = 'queued'`), exactly one
/// worker-eligible job row and the `io.elembra.files.file.created.v1` outbox
/// row in one transaction, and the best-effort post-commit
/// `ReplicationStateChanged` event is published (asserted because without
/// faults the publication is deterministic).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn a_target_enabled_commits_mutation_and_intent_atomically() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let service = real_file_service(&ctx);

    let owner = ctx.create_test_user("a_atomic_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let target_id = seed_enabled_replication_target(&ctx.pool).await;
    cleanup_guard.track_target(target_id);

    let tag = Uuid::new_v4();
    let content = format!("a-content-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&content)]);
    let name = format!("a-{tag}.txt");
    let path = format!("/{name}");

    let file = service
        .upload_file(
            owner.id,
            name,
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect("upload must succeed with an enabled target and no faults");
    assert_eq!(file.current_version, 1);

    assert_eq!(file_row_count(&ctx, &path, owner.id).await, 1);
    assert_eq!(
        latest_version_state(&ctx, file.id).await,
        "queued",
        "the version state must be queued inside the mutation transaction"
    );
    assert_eq!(
        tenant_outbox_count(&ctx, CREATED_EVENT_TYPE).await,
        1,
        "the created outbox event must commit with the mutation"
    );

    let version_id = latest_version_id(&ctx, file.id).await;
    let jobs = replication_job_rows(&ctx, version_id).await;
    assert_eq!(
        jobs.len(),
        1,
        "exactly one replication job must be queued for the new version"
    );
    let (job_id, job_status, job_key) = &jobs[0];
    assert_ne!(*job_id, Uuid::nil());
    assert_eq!(job_status, "queued");
    assert_eq!(job_key, &blob_key(&content));
    assert_eq!(
        worker_eligible_job_count(&ctx, version_id).await,
        1,
        "the job must be immediately eligible for the replication worker"
    );

    assert_eq!(
        replication_state_changed_event_count(&ctx, file.id).await,
        1,
        "without faults the best-effort post-commit event must be published"
    );

    cleanup_fixture(&ctx, &[owner.id], &[blob_key(&content)]).await;
    sqlx::query("DELETE FROM replication_targets WHERE id = $1")
        .bind(target_id)
        .execute(&ctx.pool)
        .await
        .unwrap();
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// B. replication-job insert failure rolls back the whole mutation
// ---------------------------------------------------------------------------

/// **Invariant B** — the job insert now runs INSIDE the mutation transaction,
/// so an injected job-insert failure aborts the upload completely: no `files`
/// row, no `file_versions` row and no outbox row may survive ("no committed
/// mutation without durable replication intent").
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn b_job_insert_failure_rolls_back_entire_mutation() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let service = real_file_service(&ctx);

    let owner = ctx.create_test_user("b_job_fail_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let target_id = seed_enabled_replication_target(&ctx.pool).await;
    cleanup_guard.track_target(target_id);
    inject_job_insert_failure(&ctx.pool).await;

    let tag = Uuid::new_v4();
    let content = format!("b-content-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&content)]);
    let name = format!("b-{tag}.txt");
    let path = format!("/{name}");

    let result = service
        .upload_file(
            owner.id,
            name,
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await;

    let err =
        result.expect_err("a replication-intent failure inside the mutation must surface as Err");
    assert!(
        matches!(err, FileError::Database(_)),
        "the injected trigger failure must surface as a database error, got {err:?}"
    );

    assert_eq!(
        file_row_count(&ctx, &path, owner.id).await,
        0,
        "no committed files row may survive an in-transaction intent failure"
    );
    assert_eq!(
        version_rows_with_hash(&ctx, &hex::encode(Sha256::digest(&content))).await,
        0,
        "no committed file_versions row may survive an in-transaction intent failure"
    );
    assert_eq!(
        tenant_outbox_count(&ctx, CREATED_EVENT_TYPE).await,
        0,
        "no outbox row may survive an in-transaction intent failure"
    );

    drop_injected_triggers(&ctx.pool).await;
    cleanup_fixture(&ctx, &[owner.id], &[blob_key(&content)]).await;
    sqlx::query("DELETE FROM replication_targets WHERE id = $1")
        .bind(target_id)
        .execute(&ctx.pool)
        .await
        .unwrap();
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// C. version-state update failure rolls back the whole mutation
// ---------------------------------------------------------------------------

/// **Invariant C** — post-fix the version's `replication_state` is written by
/// the version upsert inside the mutation transaction. An edit-overwrite
/// reuses the version number, so the upsert takes its `ON CONFLICT DO UPDATE`
/// branch and touches `replication_state`; the injected update failure
/// therefore fires INSIDE the transaction and must roll the whole edit back
/// (file row, version row, outbox event and any second job row).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn c_version_state_update_failure_rolls_back_entire_mutation() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let service = real_file_service(&ctx);

    let owner = ctx.create_test_user("c_state_fail_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let target_id = seed_enabled_replication_target(&ctx.pool).await;
    cleanup_guard.track_target(target_id);

    let tag = Uuid::new_v4();
    let content = format!("c-content-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&content)]);
    let name = format!("c-{tag}.txt");

    // Baseline upload commits v1 queued with one job.
    let file = service
        .upload_file(
            owner.id,
            name,
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect("baseline upload must succeed");
    let version_id = latest_version_id(&ctx, file.id).await;
    assert_eq!(latest_version_state(&ctx, file.id).await, "queued");
    assert_eq!(replication_job_count(&ctx, version_id).await, 1);

    // Edit-overwrite: same version number, new content. The upsert's
    // DO UPDATE touches replication_state, so the trigger fires in-tx.
    inject_version_state_update_failure(&ctx.pool).await;
    let edited_content = format!("c-edited-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&edited_content)]);

    let result = service
        .edit_file(
            file.id,
            owner.id,
            Bytes::from(edited_content.clone()),
            "overwrite",
            None,
        )
        .await;

    let err = result.expect_err("an in-transaction version-state failure must surface as Err");
    assert!(
        matches!(err, FileError::Database(_)),
        "the injected trigger failure must surface as a database error, got {err:?}"
    );

    // The entire edit rolled back; only the committed v1 state remains.
    assert_eq!(
        file_version_count(&ctx, file.id).await,
        1,
        "the failed overwrite must not leave a second version row"
    );
    assert_eq!(
        latest_version_id(&ctx, file.id).await,
        version_id,
        "the failed overwrite must not replace the version row"
    );
    assert_eq!(latest_version_state(&ctx, file.id).await, "queued");
    assert_eq!(
        replication_job_count(&ctx, version_id).await,
        1,
        "the failed overwrite must not leave a second replication job"
    );
    assert_eq!(
        tenant_outbox_count(&ctx, UPDATED_EVENT_TYPE).await,
        0,
        "the failed overwrite must not leave an outbox row"
    );

    drop_injected_triggers(&ctx.pool).await;
    cleanup_fixture(
        &ctx,
        &[owner.id],
        &[blob_key(&content), blob_key(&edited_content)],
    )
    .await;
    sqlx::query("DELETE FROM replication_targets WHERE id = $1")
        .bind(target_id)
        .execute(&ctx.pool)
        .await
        .unwrap();
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// D. event-publication failure is best-effort and must not fail the request
// ---------------------------------------------------------------------------

/// **Invariant D** — the `ReplicationStateChanged` event publication stays
/// post-commit and is best-effort: an injected event-insert failure must NOT
/// fail the upload. The mutation, the worker-eligible job row and the outbox
/// row stay committed; only the event is absent. This pins the best-effort
/// contract (pre-fix this exact scenario returned `Err` for a committed
/// mutation).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn d_event_publication_failure_keeps_committed_state_and_returns_ok() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let service = real_file_service(&ctx);

    let owner = ctx.create_test_user("d_event_fail_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let target_id = seed_enabled_replication_target(&ctx.pool).await;
    cleanup_guard.track_target(target_id);
    inject_replication_event_insert_failure(&ctx.pool).await;

    let tag = Uuid::new_v4();
    let content = format!("d-content-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&content)]);
    let name = format!("d-{tag}.txt");
    let path = format!("/{name}");

    let file = service
        .upload_file(
            owner.id,
            name,
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect("a post-commit best-effort event failure must never fail the request");

    assert_eq!(file_row_count(&ctx, &path, owner.id).await, 1);
    assert_eq!(latest_version_state(&ctx, file.id).await, "queued");
    assert_eq!(tenant_outbox_count(&ctx, CREATED_EVENT_TYPE).await, 1);

    let version_id = latest_version_id(&ctx, file.id).await;
    assert_eq!(replication_job_count(&ctx, version_id).await, 1);
    assert_eq!(
        worker_eligible_job_count(&ctx, version_id).await,
        1,
        "the job must remain worker-eligible even when the event publication failed"
    );
    assert_eq!(
        replication_state_changed_event_count(&ctx, file.id).await,
        0,
        "the injected event-insert failure must leave the event absent"
    );

    drop_injected_triggers(&ctx.pool).await;
    cleanup_fixture(&ctx, &[owner.id], &[blob_key(&content)]).await;
    sqlx::query("DELETE FROM replication_targets WHERE id = $1")
        .bind(target_id)
        .execute(&ctx.pool)
        .await
        .unwrap();
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// E. enabled-target lookup failure rolls back the whole mutation
// ---------------------------------------------------------------------------

/// **Invariant E** — the enabled-target count now runs INSIDE the mutation
/// transaction, so a broken `replication_targets` relation fails the upload
/// before anything commits: no `files` row, no `file_versions` row, no outbox
/// row (pre-fix the same fault reported `Err` for a committed mutation).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn e_target_lookup_failure_rolls_back_entire_mutation() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let service = real_file_service(&ctx);

    let owner = ctx.create_test_user("e_lookup_fail_owner").await;
    cleanup_guard.track_users(&[owner.id]);

    hide_replication_targets_table(&ctx.pool).await;

    let tag = Uuid::new_v4();
    let content = format!("e-content-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&content)]);
    let name = format!("e-{tag}.txt");
    let path = format!("/{name}");

    let result = service
        .upload_file(
            owner.id,
            name,
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await;

    let err = result.expect_err("an in-transaction target-lookup failure must surface as Err");
    assert!(
        matches!(err, FileError::Database(_)),
        "the missing relation must surface as a database error, got {err:?}"
    );

    assert_eq!(
        file_row_count(&ctx, &path, owner.id).await,
        0,
        "no committed files row may survive an in-transaction lookup failure"
    );
    assert_eq!(
        version_rows_with_hash(&ctx, &hex::encode(Sha256::digest(&content))).await,
        0,
        "no committed file_versions row may survive an in-transaction lookup failure"
    );
    assert_eq!(
        tenant_outbox_count(&ctx, CREATED_EVENT_TYPE).await,
        0,
        "no outbox row may survive an in-transaction lookup failure"
    );

    restore_replication_targets_table(&ctx.pool).await.unwrap();
    cleanup_fixture(&ctx, &[owner.id], &[blob_key(&content)]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// F. zero enabled targets keeps the pre-replication behavior
// ---------------------------------------------------------------------------

/// **Invariant F** — with zero enabled replication targets, uploads return
/// `Ok` with version state `primary_written` and no job row, exactly as before
/// replication tracking existed.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn f_no_enabled_targets_keeps_primary_written_and_no_job() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let service = real_file_service(&ctx);

    let owner = ctx.create_test_user("f_no_targets_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let tag = Uuid::new_v4();
    let content = format!("f-content-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&content)]);
    let name = format!("f-{tag}.txt");

    // A populated target table from a leaking suite must fail loudly as a
    // precondition violation rather than read as a service regression.
    let targets = count_enabled_targets(&ctx.pool).await;
    if targets != 0 {
        panic!("precondition: expected no enabled replication targets in the disposable DB, found {targets}");
    }

    let file = service
        .upload_file(
            owner.id,
            name,
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect("upload must succeed with zero enabled targets");

    assert_eq!(latest_version_state(&ctx, file.id).await, "primary_written");
    let version_id = latest_version_id(&ctx, file.id).await;
    assert_eq!(
        replication_job_count(&ctx, version_id).await,
        0,
        "no replication job may be queued with zero enabled targets"
    );
    assert_eq!(tenant_outbox_count(&ctx, CREATED_EVENT_TYPE).await, 1);

    cleanup_fixture(&ctx, &[owner.id], &[blob_key(&content)]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// G. retries: idempotent re-upload, second job for an overwritten version
// ---------------------------------------------------------------------------

/// **Invariant G** — retries stay safe with replication enabled: re-uploading
/// identical content and path is a no-op success (no second version, no second
/// job), and an edit-overwrite that reuses the version id queues a second job
/// for that same version. Multiple jobs per version are accepted existing
/// behavior — this PR deliberately adds no unique constraint: each mutation
/// queues its own durable intent and the worker processes every job
/// idempotently against the content-addressed blob.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn g_overwrite_reaccepts_second_job_for_same_version() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let service = real_file_service(&ctx);

    let owner = ctx.create_test_user("g_retry_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let target_id = seed_enabled_replication_target(&ctx.pool).await;
    cleanup_guard.track_target(target_id);

    let tag = Uuid::new_v4();
    let content = format!("g-content-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&content)]);
    let name = format!("g-{tag}.txt");

    let file = service
        .upload_file(
            owner.id,
            name.clone(),
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect("initial upload must succeed");
    let version_id = latest_version_id(&ctx, file.id).await;
    assert_eq!(replication_job_count(&ctx, version_id).await, 1);

    // Re-upload of identical content and path: no-op success, no new intent.
    let retried = service
        .upload_file(
            owner.id,
            name.clone(),
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect("re-uploading identical content must not error");
    assert_eq!(retried.id, file.id);
    assert_eq!(file_version_count(&ctx, file.id).await, 1);
    assert_eq!(
        replication_job_count(&ctx, version_id).await,
        1,
        "a no-op re-upload must not queue a second job"
    );

    // Edit-overwrite with new content: the version id is reused (upsert), and
    // the mutation queues a second durable intent for it.
    let edited_content = format!("g-edited-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&edited_content)]);
    let edited = service
        .edit_file(
            file.id,
            owner.id,
            Bytes::from(edited_content.clone()),
            "overwrite",
            None,
        )
        .await
        .expect("edit-overwrite must succeed");
    assert_eq!(edited.current_version, 1);

    let jobs = replication_job_rows(&ctx, version_id).await;
    assert_eq!(
        jobs.len(),
        2,
        "an overwrite queues a second job for the same version id (no unique \
         constraint by design of this change)"
    );
    assert_ne!(jobs[0].0, jobs[1].0, "the two jobs must be distinct rows");
    assert_eq!(jobs[0].1, "queued");
    assert_eq!(jobs[1].1, "queued");
    assert_eq!(latest_version_state(&ctx, file.id).await, "queued");
    assert_eq!(tenant_outbox_count(&ctx, UPDATED_EVENT_TYPE).await, 1);

    cleanup_fixture(
        &ctx,
        &[owner.id],
        &[blob_key(&content), blob_key(&edited_content)],
    )
    .await;
    sqlx::query("DELETE FROM replication_targets WHERE id = $1")
        .bind(target_id)
        .execute(&ctx.pool)
        .await
        .unwrap();
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// H. the inserted job satisfies the worker lease predicate
// ---------------------------------------------------------------------------

/// **Invariant H** — the job row inserted by the mutation is immediately
/// leaseable by the replication worker's own query. This calls
/// `MetadataStore::lease_replication_jobs` (the exact predicate the worker
/// uses, metadata.rs) without running the worker loop, and observes the job
/// transition to `syncing`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn h_inserted_job_satisfies_worker_lease_predicate() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let service = real_file_service(&ctx);

    let owner = ctx.create_test_user("h_lease_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let target_id = seed_enabled_replication_target(&ctx.pool).await;
    cleanup_guard.track_target(target_id);

    let tag = Uuid::new_v4();
    let content = format!("h-content-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&content)]);
    let name = format!("h-{tag}.txt");

    let file = service
        .upload_file(
            owner.id,
            name,
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect("upload must succeed");
    let version_id = latest_version_id(&ctx, file.id).await;

    let lease_token = Uuid::new_v4();
    let leased = ctx
        .metadata_store
        .lease_replication_jobs(5, 300, lease_token)
        .await
        .expect("leasing must succeed");
    let ours: Vec<_> = leased
        .iter()
        .filter(|job| job.file_version_id == version_id)
        .collect();
    assert_eq!(
        ours.len(),
        1,
        "the worker lease query must pick up the inserted job immediately"
    );
    assert_eq!(
        ours[0].status,
        rustshare_core::domain::ReplicationJobStatus::Syncing
    );
    assert_eq!(ours[0].storage_key, blob_key(&content));

    cleanup_fixture(&ctx, &[owner.id], &[blob_key(&content)]).await;
    sqlx::query("DELETE FROM replication_targets WHERE id = $1")
        .bind(target_id)
        .execute(&ctx.pool)
        .await
        .unwrap();
    cleanup_guard.disarm();
}
