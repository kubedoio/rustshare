//! Deterministic object-storage fault-injection regression for the file
//! create / re-upload / streaming-upload flows (Phase 3 failure atomicity).
//!
//! Every test drives the real `FileService` against the real PostgreSQL
//! metadata store, the real durable integration-event outbox and a real
//! S3-compatible object store, but wraps the object store in a
//! [`FailingObjectStore`] whose failpoints are toggled by the test. No sleeps,
//! no timing races: each failpoint is an atomic flag flipped before the call
//! under test.
//!
//! Invariants proven here:
//!
//! * **A** — a `put` / `put_from_path` failure aborts the operation before the
//!   metadata transaction starts: no `files` row, no `file_versions` row, no
//!   aggregate event, no outbox row, unrelated files untouched.
//! * **B** — an `exists` failure aborts the upload before any write.
//! * **C** — a failed write never damages content that another file already
//!   references (content-addressed dedup integrity).
//! * **D** — an *ambiguous* put (object written, caller sees an error) leaves
//!   no metadata/events, is retry-safe, and produces exactly one committed
//!   file/version/outbox row on retry.
//! * **E** — a failure after a successful blob write but before commit rolls
//!   the database back completely and leaves the now-unreferenced blob in
//!   place (no inline delete).
//! * **F** — post-commit replication queueing is a documented no-op with zero
//!   enabled replication targets; the known false-error ambiguity after a
//!   successful commit is recorded but not asserted (it cannot be triggered
//!   deterministically through the public API with zero targets).
//!
//! Known limitations, deliberately not covered by assertions:
//!
//! * A blob that is written but never referenced by any committed row is never
//!   enqueued for GC. The DELETE/UPDATE triggers that populate `object_gc_queue`
//!   from `files` / `file_versions` are unconditional — they enqueue the old
//!   `storage_key` of any deleted or re-keyed row, with no reference awareness —
//!   so a write whose transaction rolls back (or whose response is lost) leaves
//!   an orphan the current GC design cannot reclaim: no committed row ever
//!   existed to fire the trigger. Tests assert the orphan is *present* and
//!   *absent from the queue* to pin that contract; reclamation needs a separate
//!   design. Test D pairs that negative assertion with a positive control — it
//!   deletes a committed file row and observes the key in the queue — so the
//!   assertion is proven able to detect an enqueue rather than being vacuously
//!   true. Deciding whether a queued key is still referenced is the GC worker's
//!   job (it re-checks the database before deleting), not the trigger's.
//! * Post-commit `queue_replication_if_needed` failures propagate to the caller
//!   *after* a successful commit, so a returning error does not imply the
//!   mutation was rolled back (false-error ambiguity). This is flagged for a
//!   separate PR and is not asserted here beyond the zero-target early return.
//!
//! Each test arms a [`FixtureCleanupGuard`] alongside its explicit cleanup, so
//! an assertion that panics still removes the tenant rows and S3 objects it
//! created instead of leaking them into the disposable stack.
//!
//! Run against the disposable local stack (never the developer database):
//!
//! ```text
//! DATABASE_URL=postgres://rustshare:<pw>@127.0.0.1:15432/rustshare_test \
//! S3_ENDPOINT=http://127.0.0.1:19000 S3_BUCKET=rustshare-test-integration \
//! AWS_ACCESS_KEY_ID=<rustfs-root-user> AWS_SECRET_ACCESS_KEY=<rustfs-root-password> \
//! RUSTSHARE_TEST_DISPOSABLE_DB=1 RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1 \
//!   cargo test -p rustshare-server --test object_store_failure_atomicity_test \
//!     -- --ignored --test-threads=1
//! ```

mod contracts;

use contracts::common::{cleanup_tenant, setup_test_env, TestContext};

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use bytes::Bytes;
use rustshare_core::domain::ApplicationRegistry;
use rustshare_core::services::{
    FileError, FileService, IntegrationEventPublisher, ObjectStoreOps, PermissionResolver,
};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore, OutboxStore};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

/// The outbox tables are process-global, so every test serializes on this
/// guard and cleans up exactly the rows it created (same convention as the
/// Phase-1 storage and outbox suites). `--test-threads=1` alone is not enough
/// because the suite also runs next to other ignored suites in CI.
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

const CREATED_EVENT_TYPE: &str = "io.elembra.files.file.created.v1";
const UPDATED_EVENT_TYPE: &str = "io.elembra.files.file.updated.v1";

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
// Failing object store
// ---------------------------------------------------------------------------

/// Deterministic failpoints for the service-facing object-store trait.
///
/// Every flag is an atomic toggled by the test before the call under test, so
/// the injected failure is race-free and needs no sleeps.
#[derive(Default)]
struct FailConfig {
    /// Return `Err` from `put` before any object is written.
    fail_put: AtomicBool,
    /// Delegate `put` (the object *is* written) and then return `Err`: the
    /// "lost response / ambiguous ack" case.
    put_writes_then_fails: AtomicBool,
    /// Return `Err` from `exists`.
    fail_exists: AtomicBool,
    /// Return `Err` from `put_from_path` before any object is written.
    fail_put_from_path: AtomicBool,
    /// Observability counters.
    put_calls: AtomicUsize,
    put_from_path_calls: AtomicUsize,
    exists_calls: AtomicUsize,
    delete_calls: AtomicUsize,
}

/// Delegating wrapper around the real object store with explicit failpoints.
struct FailingObjectStore {
    inner: Arc<ObjectStore>,
    config: Arc<FailConfig>,
}

impl FailingObjectStore {
    fn new(inner: Arc<ObjectStore>, config: Arc<FailConfig>) -> Self {
        Self { inner, config }
    }
}

impl ObjectStoreOps for FailingObjectStore {
    async fn acquire_blob_write_lock(&self, key: &str) -> anyhow::Result<Box<dyn Send>> {
        self.inner.acquire_blob_write_lock(key).await
    }

    async fn put(&self, key: &str, data: Bytes) -> anyhow::Result<()> {
        self.config.put_calls.fetch_add(1, Ordering::SeqCst);
        if self.config.fail_put.load(Ordering::SeqCst) {
            anyhow::bail!("injected put failure for {key}");
        }
        if self.config.put_writes_then_fails.load(Ordering::SeqCst) {
            self.inner.put(key, data).await?;
            anyhow::bail!("injected lost-ack put failure for {key}");
        }
        self.inner.put(key, data).await
    }

    async fn put_from_path(&self, key: &str, path: &Path) -> anyhow::Result<()> {
        self.config
            .put_from_path_calls
            .fetch_add(1, Ordering::SeqCst);
        if self.config.fail_put_from_path.load(Ordering::SeqCst) {
            anyhow::bail!("injected put_from_path failure for {key}");
        }
        self.inner.put_from_path(key, path).await
    }

    async fn exists(&self, key: &str) -> anyhow::Result<bool> {
        self.config.exists_calls.fetch_add(1, Ordering::SeqCst);
        if self.config.fail_exists.load(Ordering::SeqCst) {
            anyhow::bail!("injected exists failure for {key}");
        }
        self.inner.exists(key).await
    }

    async fn get(&self, key: &str) -> anyhow::Result<Bytes> {
        self.inner.get(key).await
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.config.delete_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.delete(key).await
    }
}

type TxPublisher = Arc<dyn IntegrationEventPublisher<sqlx::Transaction<'static, sqlx::Postgres>>>;

type FailingFileService =
    FileService<EventStore, MetadataStore, FailingObjectStore, PermissionResolverRepository>;

type RealFileService =
    FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository>;

/// Build the same service graph as [`TestContext::file_service`], but with the
/// fault-injecting object store as the third generic parameter.
fn failing_file_service(ctx: &TestContext, store: Arc<FailingObjectStore>) -> FailingFileService {
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(ctx.pool.clone()),
    )));
    FileService::new(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        store,
        ctx.broadcaster.clone(),
        permission_resolver,
    )
}

/// The real durable outbox publisher, so "no outbox row" assertions are
/// meaningful rather than an artifact of an unattached publisher.
fn outbox_publisher(ctx: &TestContext) -> TxPublisher {
    let registry = Arc::new(ApplicationRegistry::first_party().unwrap());
    Arc::new(OutboxStore::new(ctx.pool.clone(), registry))
}

/// A publisher that always fails *inside* the caller's transaction, forcing a
/// rollback after the file rows and the aggregate event were already inserted.
struct FailingPublisher;

#[async_trait::async_trait]
impl IntegrationEventPublisher<sqlx::Transaction<'static, sqlx::Postgres>> for FailingPublisher {
    async fn publish_in_tx(
        &self,
        _tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        _facts: &rustshare_core::services::IntegrationEventFacts<'_>,
    ) -> Result<(), rustshare_core::services::IntegrationPublishError> {
        Err(
            rustshare_core::services::IntegrationPublishError::Persistence(
                "injected publisher failure".to_string(),
            ),
        )
    }
}

// ---------------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------------

fn blob_key(content: &[u8]) -> String {
    format!("blobs/{}", hex::encode(Sha256::digest(content)))
}

async fn tenant_file_count(ctx: &TestContext) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM files WHERE tenant_id = $1")
        .bind(ctx.tenant_id)
        .fetch_one(&ctx.pool)
        .await
        .unwrap()
}

async fn tenant_outbox_count(ctx: &TestContext) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1")
        .bind(ctx.tenant_id)
        .fetch_one(&ctx.pool)
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

/// Number of committed Files create/update integration events for the tenant.
async fn tenant_file_outbox_events(ctx: &TestContext) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*)::bigint FROM integration_outbox \
         WHERE tenant_id = $1 AND event_type IN ($2, $3)",
    )
    .bind(ctx.tenant_id)
    .bind(CREATED_EVENT_TYPE)
    .bind(UPDATED_EVENT_TYPE)
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

/// Number of committed `file_versions` rows carrying this content hash in this
/// tenant. `file_versions` has its own `tenant_id`, so the count is tenant
/// scoped: another suite running against the same disposable database cannot
/// make a "no version row" assertion pass or fail by accident.
async fn version_rows_with_hash(ctx: &TestContext, content_hash: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*)::bigint FROM file_versions WHERE content_hash = $1 AND tenant_id = $2",
    )
    .bind(content_hash)
    .bind(ctx.tenant_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
}

async fn file_events(ctx: &TestContext, user_id: Uuid, event_type: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*)::bigint FROM events WHERE user_id = $1 AND event_type LIKE $2",
    )
    .bind(user_id)
    .bind(format!("%{event_type}%"))
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
}

/// Number of `object_gc_queue` rows for this key.
///
/// `object_gc_queue` has no tenant column — it is primary-keyed by
/// `object_key` alone — so this lookup is key-scoped rather than tenant-scoped.
/// That is sound here because every key passed in is content-addressed from a
/// per-test `Uuid` tag or taken from a per-test file row.
async fn gc_queue_entries(ctx: &TestContext, object_key: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*)::bigint FROM object_gc_queue WHERE object_key = $1")
        .bind(object_key)
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

async fn latest_version_id(ctx: &TestContext, file_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "SELECT id FROM file_versions WHERE file_id = $1 ORDER BY version_number DESC LIMIT 1",
    )
    .bind(file_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
}

/// Snapshot of a file row + its version count, used to prove unrelated files
/// are untouched by a failed mutation.
#[derive(Debug, PartialEq, Eq)]
struct FileSnapshot {
    content_hash: String,
    size: i64,
    current_version: i32,
    version_count: i64,
}

async fn snapshot_file(ctx: &TestContext, file_id: Uuid) -> FileSnapshot {
    let (content_hash, size, current_version) = sqlx::query_as::<_, (String, i64, i32)>(
        "SELECT content_hash, size, current_version FROM files WHERE id = $1",
    )
    .bind(file_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    FileSnapshot {
        content_hash,
        size,
        current_version,
        version_count: file_version_count(ctx, file_id).await,
    }
}

/// Delete every fixture row this suite creates, for one tenant.
///
/// Order matters: outbox rows are not tenant-cascaded on delete, the aggregate
/// `events` table has no tenant column (only `user_id`), and deleting `files` /
/// `file_versions` runs the GC triggers, so the queued keys are removed last.
async fn cleanup_fixture(ctx: &TestContext, user_ids: &[Uuid], blob_keys: &[String]) {
    cleanup_fixture_rows(
        &ctx.pool,
        ctx.tenant_id,
        user_ids,
        blob_keys,
        &ctx.object_store,
    )
    .await;
}

/// Pool-and-store form of [`cleanup_fixture`], shared with the panic-path
/// [`FixtureCleanupGuard`], which cannot borrow the test's [`TestContext`].
async fn cleanup_fixture_rows(
    pool: &PgPool,
    tenant_id: Uuid,
    user_ids: &[Uuid],
    blob_keys: &[String],
    object_store: &ObjectStore,
) {
    sqlx::query("DELETE FROM integration_outbox WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .ok();
    for user_id in user_ids {
        sqlx::query("DELETE FROM events WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .ok();
    }
    cleanup_tenant(pool, tenant_id).await;
    for key in blob_keys {
        sqlx::query("DELETE FROM object_gc_queue WHERE object_key = $1")
            .bind(key)
            .execute(pool)
            .await
            .ok();
    }
    for key in blob_keys {
        let _ = object_store.delete(key).await;
    }
}

/// Deletes the tenant rows and S3 objects a test registered here if it unwinds
/// before the explicit [`cleanup_fixture`] call runs: a panicking assertion
/// would otherwise leak the tenant, its users, its event/outbox rows and every
/// blob the test wrote.
///
/// `Drop` cannot await, so the cleanup runs on a dedicated thread with its own
/// current-thread runtime and its own pool (the same shape as
/// `chat_app_read_test`'s `FixtureCleanupGuard`). Keeping it off the test's
/// runtime means a panic inside the cleanup surfaces as a join error instead of
/// a second panic that would abort the whole test binary mid-unwind.
///
/// Cleanup is fail-fast: its pool has a 5s acquire/connect timeout and its
/// thread signals completion over an `mpsc` channel, so `Drop` waits at most
/// 30s instead of joining unbounded. A wedged database or object store must not
/// hang the whole binary while the test holds the [`SERIAL`] mutex; on timeout
/// the thread is abandoned and the fixtures may leak, which is preferable to a
/// deadlock.
struct FixtureCleanupGuard {
    database_url: String,
    tenant_id: Uuid,
    user_ids: Vec<Uuid>,
    blob_keys: Vec<String>,
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
            object_store: ctx.object_store.clone(),
            armed: true,
        }
    }

    /// Register fixtures as soon as they are created, so a panic at any later
    /// point in the test still finds them.
    fn track_users(&mut self, user_ids: &[Uuid]) {
        self.user_ids.extend_from_slice(user_ids);
    }

    /// Track a blob key as soon as it can exist — including keys that are only
    /// ever written by a mutation that then rolls back, which no database query
    /// could recover afterwards.
    fn track_keys(&mut self, blob_keys: &[String]) {
        self.blob_keys.extend_from_slice(blob_keys);
    }

    /// Disarm once the explicit cleanup has run, so the guard does not delete
    /// the same fixtures twice on the way out.
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
        let object_store = self.object_store.clone();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build the panic-path cleanup runtime");
            runtime.block_on(async move {
                match PgPoolOptions::new()
                    // sqlx 0.8 has no `connect_timeout` on `PoolOptions`; its
                    // `acquire_timeout` is the same knob (sqlx serializes the
                    // field as "connect_timeout") and bounds `connect` too.
                    .acquire_timeout(Duration::from_secs(5))
                    .connect(&database_url)
                    .await
                {
                    Ok(pool) => {
                        cleanup_fixture_rows(
                            &pool,
                            tenant_id,
                            &user_ids,
                            &blob_keys,
                            &object_store,
                        )
                        .await;
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
            // Signal completion so `Drop` can bound its wait instead of joining
            // unbounded.
            let _ = finished_tx.send(());
        });
        // Wait for the thread to report progress, but never forever: a blocked
        // DELETE or a hung object store must not deadlock the test process (and
        // the SERIAL mutex) on the way out of a panicking test. On timeout the
        // thread is detached and abandoned; the fixtures may leak, which is
        // preferable to hanging the binary.
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
        // The thread has finished, so joining only reaps it; a panic on the
        // cleanup thread is dropped here rather than propagated.
        let _ = handle.join();
    }
}

// ---------------------------------------------------------------------------
// A. put / put_from_path failure before commit
// ---------------------------------------------------------------------------

/// **Invariant A** — a `put` (create), a `put` (re-upload of an existing path)
/// and a `put_from_path` (streaming upload) failure all return `Err` before the
/// metadata transaction starts: no `files` / `file_versions` row, no aggregate
/// event, no `integration_outbox` row, and a pre-existing unrelated file is
/// byte-for-byte unchanged.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn a_put_failure_before_commit_leaves_no_metadata_events_or_outbox() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let config = Arc::new(FailConfig::default());
    let store = Arc::new(FailingObjectStore::new(
        ctx.object_store.clone(),
        config.clone(),
    ));
    let service = failing_file_service(&ctx, store.clone())
        .with_integration_publisher(outbox_publisher(&ctx));

    let owner = ctx.create_test_user("a_put_fail_owner").await;
    let other = ctx.create_test_user("a_put_fail_other").await;
    cleanup_guard.track_users(&[owner.id, other.id]);
    let tag = Uuid::new_v4();

    // An unrelated, pre-existing file owned by another user.
    let baseline_content = format!("a-baseline-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&baseline_content)]);
    let baseline_name = format!("a-baseline-{tag}.txt");
    let baseline_path = format!("/{baseline_name}");
    let baseline = service
        .upload_file(
            other.id,
            baseline_name.clone(),
            None,
            Bytes::from(baseline_content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .unwrap();
    let baseline_snapshot = snapshot_file(&ctx, baseline.id).await;
    assert_eq!(file_row_count(&ctx, &baseline_path, other.id).await, 1);
    let outbox_before = tenant_outbox_count(&ctx).await;
    let files_before = tenant_file_count(&ctx).await;

    // --- create path: put fails before anything is written -----------------
    let create_content = format!("a-create-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&create_content)]);
    let create_name = format!("a-create-{tag}.txt");
    let create_path = format!("/{create_name}");
    config.fail_put.store(true, Ordering::SeqCst);
    let create_err = service
        .upload_file(
            owner.id,
            create_name.clone(),
            None,
            Bytes::from(create_content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect_err("create must fail when put fails");
    assert!(
        matches!(create_err, FileError::Storage(_)),
        "put failure must surface as a storage error, got {create_err:?}"
    );
    config.fail_put.store(false, Ordering::SeqCst);

    assert_eq!(
        file_row_count(&ctx, &create_path, owner.id).await,
        0,
        "failed create must not leave a files row"
    );
    assert_eq!(
        version_rows_with_hash(&ctx, &hex::encode(Sha256::digest(&create_content))).await,
        0,
        "failed create must not leave a file_versions row"
    );
    assert_eq!(
        file_events(&ctx, owner.id, "FileUploaded").await,
        0,
        "failed create must not append a FileUploaded event"
    );
    assert_eq!(
        tenant_outbox_count(&ctx).await,
        outbox_before,
        "failed create must not publish an outbox event"
    );
    assert_eq!(
        tenant_file_count(&ctx).await,
        files_before,
        "failed create must not change the tenant's file count"
    );
    assert!(
        !ctx.object_store
            .exists(&blob_key(&create_content))
            .await
            .unwrap(),
        "a put that failed before writing must leave no object behind"
    );

    // --- re-upload path: update of an existing path fails -------------------
    let update_content = format!("a-update-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&update_content)]);
    config.fail_put.store(true, Ordering::SeqCst);
    let update_err = service
        .upload_file(
            other.id,
            baseline_name.clone(),
            None,
            Bytes::from(update_content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect_err("re-upload must fail when put fails");
    assert!(matches!(update_err, FileError::Storage(_)));
    config.fail_put.store(false, Ordering::SeqCst);

    assert_eq!(
        snapshot_file(&ctx, baseline.id).await,
        baseline_snapshot,
        "a failed re-upload must leave the existing file's row and versions untouched"
    );
    assert_eq!(
        version_rows_with_hash(&ctx, &hex::encode(Sha256::digest(&update_content))).await,
        0,
        "failed re-upload must not leave a file_versions row"
    );
    assert_eq!(
        file_events(&ctx, other.id, "FileModified").await,
        0,
        "failed re-upload must not append a FileModified event"
    );
    assert_eq!(
        file_events(&ctx, other.id, "FileUploaded").await,
        1,
        "only the successful baseline upload may have emitted FileUploaded"
    );
    assert_eq!(
        tenant_outbox_count(&ctx).await,
        outbox_before,
        "failed re-upload must not publish an outbox event"
    );

    // --- streaming path: put_from_path fails --------------------------------
    let stream_content = format!("a-stream-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&stream_content)]);
    let stream_name = format!("a-stream-{tag}.txt");
    let stream_path = format!("/{stream_name}");
    let dir = tempfile::tempdir().unwrap();
    let local_path = dir.path().join(&stream_name);
    std::fs::write(&local_path, &stream_content).unwrap();

    let calls_before = config.put_from_path_calls.load(Ordering::SeqCst);
    config.fail_put_from_path.store(true, Ordering::SeqCst);
    let stream_err = service
        .upload_file_from_path(
            owner.id,
            stream_name.clone(),
            None,
            &local_path,
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect_err("streaming upload must fail when put_from_path fails");
    assert!(matches!(stream_err, FileError::Storage(_)));
    config.fail_put_from_path.store(false, Ordering::SeqCst);
    assert_eq!(
        config.put_from_path_calls.load(Ordering::SeqCst),
        calls_before + 1,
        "the streaming path must have attempted exactly one put_from_path"
    );

    assert_eq!(file_row_count(&ctx, &stream_path, owner.id).await, 0);
    assert_eq!(
        version_rows_with_hash(&ctx, &hex::encode(Sha256::digest(&stream_content))).await,
        0
    );
    assert_eq!(file_events(&ctx, owner.id, "FileUploaded").await, 0);
    assert_eq!(tenant_outbox_count(&ctx).await, outbox_before);
    assert_eq!(
        tenant_file_outbox_events(&ctx).await,
        1,
        "only the baseline create may be present: no create/update outbox row from a failed put"
    );
    assert!(!ctx
        .object_store
        .exists(&blob_key(&stream_content))
        .await
        .unwrap());

    // Unrelated file still intact and downloadable.
    let downloaded = ctx.object_store.get(&baseline.storage_key()).await.unwrap();
    assert_eq!(downloaded.as_ref(), baseline_content.as_slice());

    cleanup_fixture(
        &ctx,
        &[owner.id, other.id],
        &[
            baseline.storage_key(),
            blob_key(&create_content),
            blob_key(&update_content),
            blob_key(&stream_content),
        ],
    )
    .await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// B. exists() failure
// ---------------------------------------------------------------------------

/// **Invariant B** — when the dedup probe (`exists`) fails, the upload returns
/// `Err` before any storage write and before the metadata transaction: no
/// metadata, no event, no outbox row, and other content is untouched.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn b_exists_failure_aborts_before_any_write() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let config = Arc::new(FailConfig::default());
    let store = Arc::new(FailingObjectStore::new(
        ctx.object_store.clone(),
        config.clone(),
    ));
    let service = failing_file_service(&ctx, store.clone())
        .with_integration_publisher(outbox_publisher(&ctx));

    let owner = ctx.create_test_user("b_exists_fail_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let tag = Uuid::new_v4();

    let baseline_content = format!("b-baseline-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&baseline_content)]);
    let baseline_name = format!("b-baseline-{tag}.txt");
    let baseline = service
        .upload_file(
            owner.id,
            baseline_name,
            None,
            Bytes::from(baseline_content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .unwrap();
    let baseline_snapshot = snapshot_file(&ctx, baseline.id).await;
    let outbox_before = tenant_outbox_count(&ctx).await;
    let files_before = tenant_file_count(&ctx).await;

    let content = format!("b-exists-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&content)]);
    let name = format!("b-exists-{tag}.txt");
    let path = format!("/{name}");
    let puts_before = config.put_calls.load(Ordering::SeqCst);

    config.fail_exists.store(true, Ordering::SeqCst);
    let err = service
        .upload_file(
            owner.id,
            name,
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect_err("upload must fail when exists fails");
    assert!(matches!(err, FileError::Storage(_)));
    config.fail_exists.store(false, Ordering::SeqCst);

    assert_eq!(
        config.put_calls.load(Ordering::SeqCst),
        puts_before,
        "an exists failure must abort before any put attempt"
    );
    assert_eq!(file_row_count(&ctx, &path, owner.id).await, 0);
    assert_eq!(
        version_rows_with_hash(&ctx, &hex::encode(Sha256::digest(&content))).await,
        0
    );
    assert_eq!(file_events(&ctx, owner.id, "FileUploaded").await, 1);
    assert_eq!(tenant_outbox_count(&ctx).await, outbox_before);
    assert_eq!(
        tenant_file_outbox_events(&ctx).await,
        1,
        "an exists failure must not publish a create/update outbox row"
    );
    assert_eq!(tenant_file_count(&ctx).await, files_before);
    assert!(
        !ctx.object_store.exists(&blob_key(&content)).await.unwrap(),
        "no object may be written when exists fails"
    );
    assert_eq!(snapshot_file(&ctx, baseline.id).await, baseline_snapshot);
    assert_eq!(
        ctx.object_store
            .get(&baseline.storage_key())
            .await
            .unwrap()
            .as_ref(),
        baseline_content.as_slice()
    );

    cleanup_fixture(
        &ctx,
        &[owner.id],
        &[baseline.storage_key(), blob_key(&content)],
    )
    .await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// C. content-addressed dedup integrity
// ---------------------------------------------------------------------------

/// **Invariant C** — a failed write for file Y must not damage content that
/// file X already references: X's blob stays byte-identical and downloadable,
/// X's row/versions are unchanged, X's key is not queued for GC, and a later
/// successful upload of Y neither deletes nor overwrites X's object.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn c_failed_write_preserves_content_referenced_by_another_file() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let config = Arc::new(FailConfig::default());
    let store = Arc::new(FailingObjectStore::new(
        ctx.object_store.clone(),
        config.clone(),
    ));
    let service = failing_file_service(&ctx, store.clone())
        .with_integration_publisher(outbox_publisher(&ctx));

    let owner = ctx.create_test_user("c_dedup_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let tag = Uuid::new_v4();

    // File X: committed successfully, references its blob.
    let x_content = format!("c-x-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&x_content)]);
    let x_name = format!("c-x-{tag}.txt");
    let x = service
        .upload_file(
            owner.id,
            x_name,
            None,
            Bytes::from(x_content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .unwrap();
    let x_snapshot = snapshot_file(&ctx, x.id).await;
    let x_key = x.storage_key();
    assert!(ctx.object_store.exists(&x_key).await.unwrap());
    assert_eq!(
        gc_queue_entries(&ctx, &x_key).await,
        0,
        "a referenced blob must not sit in the GC queue"
    );

    // Attempted upload of a DIFFERENT file Y whose put writes then reports an
    // error — the ambiguous-ack shape that must never touch X.
    let y_content = format!("c-y-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&y_content)]);
    let y_name = format!("c-y-{tag}.txt");
    let y_path = format!("/{y_name}");
    config.put_writes_then_fails.store(true, Ordering::SeqCst);
    let err = service
        .upload_file(
            owner.id,
            y_name.clone(),
            None,
            Bytes::from(y_content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect_err("ambiguous put must surface as an error");
    assert!(matches!(err, FileError::Storage(_)));
    config.put_writes_then_fails.store(false, Ordering::SeqCst);

    // X is untouched by the failure.
    assert_eq!(
        snapshot_file(&ctx, x.id).await,
        x_snapshot,
        "a failed write for another file must not change X"
    );
    assert!(
        ctx.object_store.exists(&x_key).await.unwrap(),
        "X's object must still exist after a failed write for Y"
    );
    assert_eq!(
        ctx.object_store.get(&x_key).await.unwrap().as_ref(),
        x_content.as_slice(),
        "X's object content must be byte-identical after a failed write for Y"
    );
    assert_eq!(gc_queue_entries(&ctx, &x_key).await, 0);
    assert_eq!(file_row_count(&ctx, &y_path, owner.id).await, 0);

    // Y's content uploads successfully afterwards: X must survive intact.
    let y = service
        .upload_file(
            owner.id,
            y_name.clone(),
            None,
            Bytes::from(y_content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .unwrap();
    assert_ne!(y.id, x.id);
    assert_eq!(file_row_count(&ctx, &y_path, owner.id).await, 1);
    assert_eq!(snapshot_file(&ctx, x.id).await, x_snapshot);
    assert_eq!(
        ctx.object_store.get(&x_key).await.unwrap().as_ref(),
        x_content.as_slice(),
        "a successful upload of Y's different content must not overwrite X's blob"
    );
    assert!(ctx.object_store.exists(&x_key).await.unwrap());
    assert_eq!(gc_queue_entries(&ctx, &x_key).await, 0);
    assert_eq!(
        ctx.object_store
            .get(&y.storage_key())
            .await
            .unwrap()
            .as_ref(),
        y_content.as_slice()
    );

    cleanup_fixture(
        &ctx,
        &[owner.id],
        &[x_key, y.storage_key(), blob_key(&y_content)],
    )
    .await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// D. ambiguous put (write succeeded, response lost)
// ---------------------------------------------------------------------------

/// **Invariant D** — an ambiguous put (object written, error returned) must
/// create no metadata/events. The retry with failpoints disabled then commits
/// exactly one file/version/outbox row (no duplicates), reusing the orphaned
/// blob.
///
/// Known limitation (asserted, not just documented): the orphaned blob is left
/// in place and is **absent from `object_gc_queue`**. `object_gc_queue` is
/// populated only by unconditional DELETE/UPDATE triggers on rows that hold a
/// `storage_key`, so a blob written for a mutation that rolled back (or whose
/// ack was lost) is never enqueued: no committed row existed to fire the
/// trigger. Reclaiming never-referenced blobs is outside the current GC design
/// and needs a separate mechanism.
///
/// The negative assertion is paired with a positive control at the end of the
/// test: deleting a file row in the same tenant does enqueue its key, which
/// proves the check can observe an enqueue at all.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn d_ambiguous_put_is_retry_safe_and_leaves_one_committed_row_set() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let config = Arc::new(FailConfig::default());
    let store = Arc::new(FailingObjectStore::new(
        ctx.object_store.clone(),
        config.clone(),
    ));
    let service = failing_file_service(&ctx, store.clone())
        .with_integration_publisher(outbox_publisher(&ctx));

    let owner = ctx.create_test_user("d_ambiguous_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let tag = Uuid::new_v4();

    // Unrelated committed file whose objects must survive.
    let other_content = format!("d-other-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&other_content)]);
    let other = service
        .upload_file(
            owner.id,
            format!("d-other-{tag}.txt"),
            None,
            Bytes::from(other_content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .unwrap();
    let outbox_before = tenant_outbox_count(&ctx).await;
    let files_before = tenant_file_count(&ctx).await;

    let content = format!("d-ambiguous-{tag}").into_bytes();
    let name = format!("d-ambiguous-{tag}.txt");
    let path = format!("/{name}");
    let key = blob_key(&content);
    cleanup_guard.track_keys(std::slice::from_ref(&key));

    // Attempt 1: the object is written, then the caller sees an error.
    config.put_writes_then_fails.store(true, Ordering::SeqCst);
    let err = service
        .upload_file(
            owner.id,
            name.clone(),
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect_err("ambiguous put must surface as an error");
    assert!(matches!(err, FileError::Storage(_)));
    config.put_writes_then_fails.store(false, Ordering::SeqCst);

    assert_eq!(file_row_count(&ctx, &path, owner.id).await, 0);
    assert_eq!(
        version_rows_with_hash(&ctx, &hex::encode(Sha256::digest(&content))).await,
        0
    );
    assert_eq!(file_events(&ctx, owner.id, "FileUploaded").await, 1); // baseline only
    assert_eq!(tenant_outbox_count(&ctx).await, outbox_before);
    assert_eq!(tenant_file_count(&ctx).await, files_before);

    // The orphan from the ambiguous write is present...
    assert!(
        ctx.object_store.exists(&key).await.unwrap(),
        "the ambiguous write must have stored the object"
    );
    assert_eq!(
        ctx.object_store.get(&key).await.unwrap().as_ref(),
        content.as_slice()
    );
    // ...and is NOT enqueued for GC: never-referenced blobs are out of scope
    // for the trigger-populated queue (documented limitation, see doc comment).
    assert_eq!(
        gc_queue_entries(&ctx, &key).await,
        0,
        "a never-referenced blob must not appear in object_gc_queue"
    );

    // Unrelated file untouched.
    assert_eq!(
        ctx.object_store
            .get(&other.storage_key())
            .await
            .unwrap()
            .as_ref(),
        other_content.as_slice()
    );

    // Retry with failpoints disabled: the dedup probe now sees the orphan and
    // skips the put, then commits exactly one file/version/outbox row.
    let puts_before_retry = config.put_calls.load(Ordering::SeqCst);
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
        .expect("retry must succeed once the failpoint is cleared");
    assert_eq!(retried.current_version, 1);
    assert_eq!(file_row_count(&ctx, &path, owner.id).await, 1);
    assert_eq!(file_version_count(&ctx, retried.id).await, 1);
    assert_eq!(
        file_events(&ctx, owner.id, "FileUploaded").await,
        2,
        "baseline + retried create, exactly once each"
    );
    assert_eq!(
        tenant_outbox_count(&ctx).await,
        outbox_before + 1,
        "the retry must publish exactly one outbox event, with no duplicate"
    );
    let created_rows = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1 AND event_type = $2",
    )
    .bind(ctx.tenant_id)
    .bind(CREATED_EVENT_TYPE)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        created_rows, 2,
        "baseline + retried create are the only created events"
    );
    let events_for_file = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM events WHERE user_id = $1 AND aggregate_id = $2",
    )
    .bind(owner.id)
    .bind(retried.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        events_for_file, 1,
        "exactly one aggregate event may reference the retried file"
    );

    // The retry reused the orphaned blob: the dedup probe short-circuited the
    // write, so the `put` path used by this `Bytes` upload was not taken again.
    // The snapshot below therefore only covers `put`; this retry never reaches
    // `put_from_path` (the `exists` probe short-circuits both branches of the
    // `UploadSource` match, so a `Path` upload would be skipped the same way).
    assert_eq!(
        config.put_calls.load(Ordering::SeqCst),
        puts_before_retry,
        "the retry must not issue a second put for the orphaned blob"
    );
    assert!(ctx.object_store.exists(&key).await.unwrap());
    assert_eq!(
        ctx.object_store.get(&key).await.unwrap().as_ref(),
        content.as_slice()
    );

    // Positive control for the "orphan not enqueued" assertion above. The
    // enqueue trigger on `files` is unconditional: any DELETE of a row that
    // holds a `storage_key` inserts that key into `object_gc_queue`, whether or
    // not another row still references the blob. Reference-awareness is the GC
    // worker's concern (it re-checks the database before deleting), not the
    // trigger's. The before/after pair proves this check observes the
    // enqueue transition; without it, the earlier `gc_queue_entries == 0`
    // assertion could never fail.
    assert_eq!(
        gc_queue_entries(&ctx, &other.storage_key()).await,
        0,
        "the control key must not be queued while its file row still exists"
    );
    sqlx::query("DELETE FROM files WHERE id = $1")
        .bind(other.id)
        .execute(&ctx.pool)
        .await
        .expect("delete the control file row");
    assert_eq!(
        gc_queue_entries(&ctx, &other.storage_key()).await,
        1,
        "deleting a referenced file must enqueue its storage key for GC"
    );
    assert_eq!(
        gc_queue_entries(&ctx, &key).await,
        0,
        "deleting an unrelated file must not enqueue the retried file's key"
    );

    cleanup_fixture(
        &ctx,
        &[owner.id],
        &[other.storage_key(), key, blob_key(&other_content)],
    )
    .await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// E. failure after a successful blob write, before commit
// ---------------------------------------------------------------------------

/// **Invariant E** — when the blob write succeeds but the metadata transaction
/// fails, the database is fully rolled back while the (now unreferenced) blob
/// remains, with no inline delete and no GC-queue entry.
///
/// Injection choice: the integration-event publisher is forced to fail inside
/// the service transaction. That is deterministic (no table locks, no
/// timeouts) and genuinely exercises "blob written, transaction abandoned":
/// `upload_file_with_actor_impl` has already executed `append_in_tx`,
/// `create_file_in_tx` and `create_file_version_in_tx` on the same `tx` by the
/// time the publisher is called, so the rollback provably covers inserted rows.
/// A `LOCK TABLE files IN ACCESS EXCLUSIVE MODE` alternative was rejected: it
/// requires timeout handling and can silently serialize behind unrelated
/// cleanup transactions.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn e_failure_after_blob_write_rolls_back_and_retry_succeeds() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);
    let config = Arc::new(FailConfig::default());
    let store = Arc::new(FailingObjectStore::new(
        ctx.object_store.clone(),
        config.clone(),
    ));
    // The object store delegates normally here; only the publisher fails.
    let failing = failing_file_service(&ctx, store.clone())
        .with_integration_publisher(Arc::new(FailingPublisher));

    let owner = ctx.create_test_user("e_rollback_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let tag = Uuid::new_v4();

    // Baseline committed through the real pipeline.
    let real_service: RealFileService = ctx
        .file_service()
        .with_integration_publisher(outbox_publisher(&ctx));
    let baseline_content = format!("e-baseline-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&baseline_content)]);
    let baseline = real_service
        .upload_file(
            owner.id,
            format!("e-baseline-{tag}.txt"),
            None,
            Bytes::from(baseline_content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .unwrap();
    let baseline_snapshot = snapshot_file(&ctx, baseline.id).await;
    let outbox_before = tenant_outbox_count(&ctx).await;
    let files_before = tenant_file_count(&ctx).await;

    let content = format!("e-rollback-{tag}").into_bytes();
    let name = format!("e-rollback-{tag}.txt");
    let path = format!("/{name}");
    let key = blob_key(&content);
    cleanup_guard.track_keys(std::slice::from_ref(&key));

    let err = failing
        .upload_file(
            owner.id,
            name.clone(),
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect_err("upload must fail when the integration publisher fails");
    assert!(matches!(err, FileError::Database(_)), "got {err:?}");

    // PostgreSQL fully rolled back, including rows inserted inside the tx.
    assert_eq!(file_row_count(&ctx, &path, owner.id).await, 0);
    assert_eq!(
        version_rows_with_hash(&ctx, &hex::encode(Sha256::digest(&content))).await,
        0
    );
    assert_eq!(file_events(&ctx, owner.id, "FileUploaded").await, 1); // baseline only
    assert_eq!(tenant_outbox_count(&ctx).await, outbox_before);
    assert_eq!(tenant_file_count(&ctx).await, files_before);
    assert_eq!(snapshot_file(&ctx, baseline.id).await, baseline_snapshot);
    assert_eq!(
        ctx.object_store
            .get(&baseline.storage_key())
            .await
            .unwrap()
            .as_ref(),
        baseline_content.as_slice(),
        "the rollback must not damage an unrelated committed object"
    );

    // The blob written before the failed transaction stays behind...
    assert!(
        ctx.object_store.exists(&key).await.unwrap(),
        "the pre-commit blob write must not be undone by the rollback"
    );
    assert_eq!(
        ctx.object_store.get(&key).await.unwrap().as_ref(),
        content.as_slice()
    );
    // ...with no inline delete and no GC-queue entry.
    assert_eq!(
        config.delete_calls.load(Ordering::SeqCst),
        0,
        "the failure path must not delete the blob inline"
    );
    assert_eq!(gc_queue_entries(&ctx, &key).await, 0);

    // Retry through the real pipeline commits exactly once.
    let retried = real_service
        .upload_file(
            owner.id,
            name.clone(),
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect("retry must succeed once the publisher is healthy");
    assert_eq!(file_row_count(&ctx, &path, owner.id).await, 1);
    assert_eq!(file_version_count(&ctx, retried.id).await, 1);
    assert_eq!(file_events(&ctx, owner.id, "FileUploaded").await, 2);
    assert_eq!(tenant_outbox_count(&ctx).await, outbox_before + 1);
    assert!(ctx.object_store.exists(&key).await.unwrap());

    cleanup_fixture(
        &ctx,
        &[owner.id],
        &[baseline.storage_key(), key, blob_key(&baseline_content)],
    )
    .await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// F. post-commit contract (documented, not fault-injected)
// ---------------------------------------------------------------------------

/// **Invariant F** — post-commit contract, pinned without fault injection.
///
/// With zero enabled replication targets, `queue_replication_if_needed`
/// returns `Ok` early after the transaction has committed, so a successful
/// create yields exactly one committed `file_versions` row, exactly one
/// `io.elembra.files.file.created.v1` outbox row, and `Ok` to the caller.
///
/// Known limitation (documented, deliberately not asserted): the replication
/// queueing happens *after* `commit_transaction`. If it ever fails, the service
/// returns `Err` even though the mutation is durably committed — a caller
/// cannot distinguish "nothing happened" from "committed but replication
/// queueing failed". The error is a false-error ambiguity for the commit path.
/// It cannot be triggered deterministically through the public API while there
/// are zero enabled replication targets (the early return skips it entirely),
/// so no assertion is made here; changing it is a separate PR.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn f_successful_create_commits_one_version_one_outbox_event() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    let config = Arc::new(FailConfig::default());
    let store = Arc::new(FailingObjectStore::new(
        ctx.object_store.clone(),
        config.clone(),
    ));
    let service = failing_file_service(&ctx, store.clone())
        .with_integration_publisher(outbox_publisher(&ctx));

    let owner = ctx.create_test_user("f_post_commit_owner").await;
    cleanup_guard.track_users(&[owner.id]);
    let tag = Uuid::new_v4();
    let content = format!("f-content-{tag}").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&content)]);

    // Zero enabled replication targets is the precondition for the early
    // return, not a property this suite asserts about the product: a populated
    // disposable database must fail loudly as a precondition violation rather
    // than read as a service regression.
    let targets = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM replication_targets WHERE enabled = TRUE",
    )
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    if targets != 0 {
        panic!(
            "precondition: expected no enabled replication targets in disposable DB, found {targets}"
        );
    }

    let file = service
        .upload_file(
            owner.id,
            format!("f-{tag}.txt"),
            None,
            Bytes::from(content.clone()),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect("create must succeed and return Ok with zero replication targets");
    assert_eq!(file.current_version, 1);

    assert_eq!(file_version_count(&ctx, file.id).await, 1);
    let version_id = latest_version_id(&ctx, file.id).await;
    assert_eq!(
        replication_job_count(&ctx, version_id).await,
        0,
        "no replication job may be queued with zero enabled targets"
    );

    let outbox_rows = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1 AND event_type = $2",
    )
    .bind(ctx.tenant_id)
    .bind(CREATED_EVENT_TYPE)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        outbox_rows, 1,
        "exactly one created event must be published"
    );
    assert_eq!(file_events(&ctx, owner.id, "FileUploaded").await, 1);

    // A re-upload of new content commits a second version plus an updated event.
    let updated_content = format!("f-content-{tag}-v2").into_bytes();
    cleanup_guard.track_keys(&[blob_key(&updated_content)]);
    let updated = service
        .upload_file(
            owner.id,
            format!("f-{tag}.txt"),
            None,
            Bytes::from(updated_content),
            "text/plain".to_string(),
            ctx.tenant_id,
        )
        .await
        .expect("re-upload must succeed");
    assert_eq!(updated.id, file.id);
    assert_eq!(updated.current_version, 2);
    assert_eq!(file_version_count(&ctx, file.id).await, 2);
    let updated_rows = sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM integration_outbox WHERE tenant_id = $1 AND event_type = $2",
    )
    .bind(ctx.tenant_id)
    .bind(UPDATED_EVENT_TYPE)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        updated_rows, 1,
        "exactly one updated event must be published"
    );

    cleanup_fixture(
        &ctx,
        &[owner.id],
        &[
            blob_key(&content),
            blob_key(format!("f-content-{tag}-v2").as_bytes()),
        ],
    )
    .await;
    cleanup_guard.disarm();
}
