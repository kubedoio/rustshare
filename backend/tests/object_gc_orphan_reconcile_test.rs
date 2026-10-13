//! Phase 3.4B producer contract tests: safe orphan-blob reconciliation.
//!
//! The reconciler turns read-only orphan observations (Phase 3.4A) into
//! durable GC queue candidates: listed, aged, unreferenced blobs in an
//! unversioned bucket are enqueued insert-only; everything else is skipped or
//! counted as uncertain. It never deletes — deletion only ever happens in the
//! separately leased GC worker.
//!
//! Scenarios that exercise the object store run against real RustFS; unsafe
//! versioning states run against deterministic fakes so they stay hermetic.
//! Locking uses REAL PostgreSQL advisory transactions even in fake scenarios
//! so later concurrency tests stay honest.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use rustshare_server::object_gc_reconcile::{
    reconcile_orphan_blobs, BlobInspector, BlobLockProvider, BoxReconcileFuture, HeldLock,
    HeldLockBox, OrphanReconcileReport, OrphanReconciliationConfig, ReconcileError,
    ReconcileStatus, ORPHAN_RECONCILE_REASON,
};
use rustshare_server::object_gc_scan::{
    BlobListPage, BlobLister, ListedBlob, OrphanScanConfig, ScanError,
};
use rustshare_storage::{BucketVersioningState, MetadataStore, ObjectStore, ObjectStoreOptions};
use sha2::{Digest, Sha256};
use sqlx::{
    postgres::PgConnection, postgres::PgPoolOptions, Executor, PgPool, Postgres, Transaction,
};
use uuid::Uuid;

static TEST_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

struct TestEnv {
    _guard: tokio::sync::OwnedMutexGuard<()>,
    pool: PgPool,
    metadata: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
}

async fn setup() -> TestEnv {
    let guard = TEST_LOCK
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
        .lock_owned()
        .await;
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect database");
    sqlx::migrate!("../migrations")
        .run(&pool)
        .await
        .expect("apply migrations");
    // Test-seeded rows use a suite-specific reason prefix; rows the producer
    // itself enqueued use the fixed production reason. Both are cleared so a
    // rerun starts from a clean queue.
    sqlx::query(
        "DELETE FROM object_gc_queue WHERE reason LIKE 'object_gc_orphan_reconcile_test:%'",
    )
    .execute(&pool)
    .await
    .expect("clean prior seeded candidates");
    sqlx::query("DELETE FROM object_gc_queue WHERE reason = $1")
        .bind(ORPHAN_RECONCILE_REASON)
        .execute(&pool)
        .await
        .expect("clean prior reconciler candidates");

    let bucket = std::env::var("S3_BUCKET")
        .or_else(|_| std::env::var("RUSTFS_BUCKET"))
        .unwrap_or_else(|_| "rustshare-test".to_string());
    let object_store = ObjectStore::new_with_options(
        s3_endpoint(),
        s3_region(),
        bucket,
        ObjectStoreOptions {
            auto_create_bucket: true,
        },
    )
    .await
    .expect("create object store")
    .with_blob_lock_pool(pool.clone());

    TestEnv {
        _guard: guard,
        pool: pool.clone(),
        metadata: Arc::new(MetadataStore::new(pool)),
        object_store: Arc::new(object_store),
    }
}

fn blob(content: &[u8]) -> String {
    format!("blobs/{}", hex::encode(Sha256::digest(content)))
}

fn scan_config(max_keys: u32, min_age: Duration) -> OrphanScanConfig {
    OrphanScanConfig::new(max_keys, min_age).expect("valid scan config")
}

fn reconcile_config(max_enqueue: u32, min_age: Duration) -> OrphanReconciliationConfig {
    OrphanReconciliationConfig::new(true, max_enqueue, 24, scan_config(1000, min_age))
        .expect("valid reconcile config")
}

async fn count_reason_rows(pool: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM object_gc_queue WHERE reason = $1")
        .bind(ORPHAN_RECONCILE_REASON)
        .fetch_one(pool)
        .await
        .expect("count reconciler candidates")
}

async fn candidate_row(
    pool: &PgPool,
    key: &str,
) -> Option<rustshare_storage::metadata::ObjectGcCandidateRow> {
    MetadataStore::new(pool.clone())
        .get_object_gc_candidate(key)
        .await
        .expect("read candidate")
}

async fn count_reason_rows_for_key(pool: &PgPool, key: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM object_gc_queue WHERE reason = $1 AND object_key = $2",
    )
    .bind(ORPHAN_RECONCILE_REASON)
    .bind(key)
    .fetch_one(pool)
    .await
    .expect("count reconciler candidates for key")
}

fn assert_candidate_row_unchanged(
    before: &rustshare_storage::metadata::ObjectGcCandidateRow,
    after: &rustshare_storage::metadata::ObjectGcCandidateRow,
) {
    assert_eq!(before.id, after.id, "id changed");
    assert_eq!(before.object_key, after.object_key, "object_key changed");
    assert_eq!(before.reason, after.reason, "reason changed");
    assert_eq!(before.not_before, after.not_before, "not_before changed");
    assert_eq!(before.created_at, after.created_at, "created_at changed");
    assert_eq!(
        before.first_seen_at, after.first_seen_at,
        "first_seen_at changed"
    );
    assert_eq!(
        before.last_seen_at, after.last_seen_at,
        "last_seen_at changed"
    );
    assert_eq!(
        before.attempt_count, after.attempt_count,
        "attempt_count changed"
    );
    assert_eq!(
        before.last_attempt_at, after.last_attempt_at,
        "last_attempt_at changed"
    );
    assert_eq!(before.last_error, after.last_error, "last_error changed");
    assert_eq!(before.state, after.state, "state changed");
    assert_eq!(before.locked_at, after.locked_at, "locked_at changed");
    assert_eq!(before.locked_by, after.locked_by, "locked_by changed");
    assert_eq!(
        before.completed_at, after.completed_at,
        "completed_at changed"
    );
    assert_eq!(before.updated_at, after.updated_at, "updated_at changed");
    assert_eq!(
        before.operator_hold, after.operator_hold,
        "operator_hold changed"
    );
}

fn digest(key: &str) -> &str {
    key.strip_prefix("blobs/").expect("blob prefix")
}

fn s3_endpoint() -> String {
    std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .unwrap_or_else(|_| "http://localhost:9000".to_string())
}

fn s3_region() -> String {
    std::env::var("S3_REGION")
        .or_else(|_| std::env::var("RUSTFS_REGION"))
        .unwrap_or_else(|_| "us-east-1".to_string())
}

async fn insert_user(pool: &PgPool, suffix: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, storage_quota) VALUES ($1, $2, $3, 'test', 'Orphan Reconcile', 1000000)",
    )
    .bind(user_id)
    .bind(format!("orphan-reconcile-{suffix}-{user_id}"))
    .bind(format!("orphan-reconcile-{suffix}-{user_id}@test.local"))
    .execute(pool)
    .await
    .expect("create user");
    user_id
}

async fn insert_file_reference(pool: &PgPool, user_id: Uuid, key: &str, size: i64) -> Uuid {
    let file_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id) VALUES ($1, 'reconcile.bin', $2, $3, 'application/octet-stream', $4, $5, $6)",
    )
    .bind(file_id)
    .bind(format!("/orphan-reconcile-{}.bin", Uuid::new_v4()))
    .bind(size)
    .bind(digest(key))
    .bind(key)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("create file reference");
    file_id
}

/// An object store on a uniquely named bucket (auto-created) so a test sees
/// exactly the blobs it put there, regardless of the shared disposable bucket.
async fn unique_object_store(env: &TestEnv, bucket: &str) -> Arc<ObjectStore> {
    Arc::new(
        ObjectStore::new_with_options(
            s3_endpoint(),
            s3_region(),
            bucket.to_string(),
            ObjectStoreOptions {
                auto_create_bucket: true,
            },
        )
        .await
        .expect("create unique-bucket object store")
        .with_blob_lock_pool(env.pool.clone()),
    )
}

/// Raw S3 client (path-style) for store administration the ObjectStore
/// abstraction does not expose, e.g. enabling bucket versioning.
async fn raw_s3_client() -> aws_sdk_s3::Client {
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .endpoint_url(s3_endpoint())
        .region(aws_config::Region::new(s3_region()))
        .load()
        .await;
    let s3_config = aws_sdk_s3::config::Builder::from(&config)
        .force_path_style(true)
        .build();
    aws_sdk_s3::Client::from_conf(s3_config)
}

/// Best-effort removal of a scratch bucket and its contents.
async fn delete_bucket_best_effort(bucket: &str) {
    let client = raw_s3_client().await;
    let _ = client.delete_bucket().bucket(bucket).send().await;
}

/// Best-effort emptying of a scratch bucket so `delete_bucket` can succeed:
/// S3-compatible stores refuse to delete a bucket that still holds objects
/// (or versioned delete markers), and a versioned scratch bucket may hold
/// versions that a plain listing does not show.
async fn empty_bucket_best_effort(bucket: &str) {
    let client = raw_s3_client().await;

    let mut continuation = None;
    loop {
        let mut request = client.list_objects_v2().bucket(bucket);
        if let Some(token) = continuation.take() {
            request = request.continuation_token(token);
        }
        let Ok(page) = request.send().await else {
            return;
        };
        let objects: Vec<_> = page
            .contents()
            .iter()
            .map(|object| {
                aws_sdk_s3::types::ObjectIdentifier::builder()
                    .key(object.key().expect("listed object has a key"))
                    .build()
                    .expect("object identifier")
            })
            .collect();
        if !objects.is_empty() {
            let _ = client
                .delete_objects()
                .bucket(bucket)
                .delete(
                    aws_sdk_s3::types::Delete::builder()
                        .set_objects(Some(objects))
                        .build()
                        .expect("delete request"),
                )
                .send()
                .await;
        }
        match page.next_continuation_token().map(str::to_string) {
            Some(token) => continuation = Some(token),
            None => break,
        }
    }

    // A versioned bucket can hold versions and delete markers that the plain
    // listing above does not report; either keeps DeleteBucket from
    // succeeding on a store that enforces emptiness.
    let mut key_marker = None;
    let mut version_id_marker = None;
    loop {
        let mut request = client.list_object_versions().bucket(bucket);
        if let Some(marker) = key_marker.take() {
            request = request.key_marker(marker);
        }
        if let Some(marker) = version_id_marker.take() {
            request = request.version_id_marker(marker);
        }
        let Ok(page) = request.send().await else {
            return;
        };
        let mut removals: Vec<_> = page
            .versions()
            .iter()
            .map(|version| {
                aws_sdk_s3::types::ObjectIdentifier::builder()
                    .key(version.key().expect("versioned object has a key"))
                    .version_id(version.version_id().expect("version has an id"))
                    .build()
                    .expect("object identifier")
            })
            .collect();
        removals.extend(page.delete_markers().iter().map(|marker| {
            aws_sdk_s3::types::ObjectIdentifier::builder()
                .key(marker.key().expect("delete marker has a key"))
                .version_id(marker.version_id().expect("delete marker has an id"))
                .build()
                .expect("object identifier")
        }));
        if !removals.is_empty() {
            let _ = client
                .delete_objects()
                .bucket(bucket)
                .delete(
                    aws_sdk_s3::types::Delete::builder()
                        .set_objects(Some(removals))
                        .build()
                        .expect("delete request"),
                )
                .send()
                .await;
        }
        if page.is_truncated().unwrap_or(false) {
            key_marker = page.next_key_marker().map(str::to_string);
            version_id_marker = page.next_version_id_marker().map(str::to_string);
        } else {
            break;
        }
    }
}

/// Advisory-lock guard backed by a real open transaction, exactly like
/// `ObjectStore::acquire_blob_lock` implements it.
struct PgTransactionLock {
    _transaction: Transaction<'static, Postgres>,
}
impl HeldLock for PgTransactionLock {}

/// A lock provider that takes REAL per-key advisory transactions from the test
/// pool, so concurrency behavior matches production even in fake scenarios.
struct PgLockProvider {
    pool: PgPool,
}

impl BlobLockProvider for PgLockProvider {
    fn acquire<'a>(&'a self, key: &'a str) -> BoxReconcileFuture<'a, Option<HeldLockBox>> {
        Box::pin(async move {
            let mut transaction = self
                .pool
                .begin()
                .await
                .map_err(|error| ReconcileError::Probe(error.to_string()))?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 7277856))")
                .bind(key)
                .execute(&mut *transaction)
                .await
                .map_err(|error| ReconcileError::Probe(error.to_string()))?;
            Ok(Some(Box::new(PgTransactionLock {
                _transaction: transaction,
            }) as HeldLockBox))
        })
    }
}

/// How `FakeInspector` answers HeadObject probes.
#[derive(Debug, Clone)]
enum HeadBehavior {
    /// Every key reports this timestamp (use the listed timestamp to model an
    /// unchanged object, or a different one to model a replacement).
    Timestamp(DateTime<Utc>),
    /// Every key reports 404/missing.
    Missing,
    /// Every probe fails with a transport-style error.
    Fail,
}

/// Deterministic inspector: canned paged listings, scripted versioning probe
/// and scripted HeadObject behavior.
struct FakeInspector {
    pages: Vec<Vec<ListedBlob>>,
    fail_on_page: Option<usize>,
    versioning: Result<BucketVersioningState, ReconcileError>,
    head: HeadBehavior,
    calls: Arc<Mutex<Vec<Option<String>>>>,
}

impl FakeInspector {
    fn new(keys: Vec<Vec<String>>) -> Self {
        let last_modified = Utc::now() - chrono::Duration::hours(48);
        Self {
            pages: keys
                .into_iter()
                .map(|page| {
                    page.into_iter()
                        .map(|key| ListedBlob { key, last_modified })
                        .collect()
                })
                .collect(),
            fail_on_page: None,
            versioning: Ok(BucketVersioningState::Unversioned),
            head: HeadBehavior::Timestamp(last_modified),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn with_versioning(
        mut self,
        versioning: Result<BucketVersioningState, ReconcileError>,
    ) -> Self {
        self.versioning = versioning;
        self
    }

    fn with_head(mut self, head: HeadBehavior) -> Self {
        self.head = head;
        self
    }

    fn with_listed_timestamp(mut self, last_modified: DateTime<Utc>) -> Self {
        for page in &mut self.pages {
            for blob in page {
                blob.last_modified = last_modified;
            }
        }
        if let HeadBehavior::Timestamp(ts) = &mut self.head {
            *ts = last_modified;
        }
        self
    }

    fn fail_on_page(mut self, page: usize) -> Self {
        self.fail_on_page = Some(page);
        self
    }
}

impl BlobLister for FakeInspector {
    fn list_page<'a>(
        &'a self,
        continuation: Option<&'a str>,
        max_keys: u32,
    ) -> Pin<Box<dyn Future<Output = Result<BlobListPage, ScanError>> + Send + 'a>> {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("calls lock")
                .push(continuation.map(str::to_string));
            let index = match continuation {
                None => 0,
                Some(token) => token
                    .strip_prefix("page-")
                    .and_then(|index| index.parse::<usize>().ok())
                    .expect("unknown continuation token"),
            };
            if self.fail_on_page == Some(index) {
                return Err(ScanError::List("synthetic list failure".to_string()));
            }
            let blobs: Vec<ListedBlob> = self
                .pages
                .get(index)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .take(max_keys as usize)
                .collect();
            Ok(BlobListPage {
                blobs,
                next_continuation: (index + 1 < self.pages.len())
                    .then(|| format!("page-{}", index + 1)),
            })
        })
    }
}

impl BlobInspector for FakeInspector {
    fn head_last_modified<'a>(
        &'a self,
        _key: &'a str,
    ) -> BoxReconcileFuture<'a, Option<DateTime<Utc>>> {
        Box::pin(async move {
            match &self.head {
                HeadBehavior::Timestamp(ts) => Ok(Some(*ts)),
                HeadBehavior::Missing => Ok(None),
                HeadBehavior::Fail => {
                    Err(ReconcileError::Probe("synthetic head failure".to_string()))
                }
            }
        })
    }

    fn bucket_versioning<'a>(&'a self) -> BoxReconcileFuture<'a, BucketVersioningState> {
        let versioning = self.versioning.clone();
        Box::pin(async move { versioning })
    }
}

fn fake_locks(env: &TestEnv) -> PgLockProvider {
    PgLockProvider {
        pool: env.pool.clone(),
    }
}

/// Boxed future handed to the pool's `after_connect` machinery.
type ConnectFuture<'a> = Pin<Box<dyn Future<Output = Result<(), sqlx::Error>> + Send + 'a>>;

/// Runs the privilege downgrade on one physical connection. A generic helper
/// (rather than an `async move` block inside the higher-ranked closure) keeps
/// the connection borrow's lifetime an explicit early-bound parameter, which
/// the closure's `for<'c>` bound then forwards; rustc cannot infer that shape
/// through a captured `&mut` inside the closure body itself.
fn set_role_future<'c>(conn: &'c mut PgConnection, role: String) -> ConnectFuture<'c> {
    Box::pin(async move {
        let statement = format!("SET ROLE {role}");
        conn.execute(statement.as_str()).await?;
        Ok(())
    })
}

/// Builds the `after_connect` callback that downgrades every physical
/// connection of the m2 denied-role pool to the restricted role (see
/// `m2_reference_check_error_never_enqueues`). A named constructor with
/// explicit closure parameter and return types keeps the higher-ranked
/// signature inferable, which an unannotated capturing closure is not.
fn set_role_on_connect(
    role: String,
) -> impl Fn(&mut PgConnection, sqlx::pool::PoolConnectionMetadata) -> ConnectFuture<'_>
       + Send
       + Sync
       + 'static {
    move |conn: &mut PgConnection, _meta: sqlx::pool::PoolConnectionMetadata| -> ConnectFuture<'_> {
        Box::pin(set_role_future(conn, role.clone()))
    }
}

/// Page real-bucket reconcile invocations until the bucket listing is fully
/// covered. The disposable bucket is shared with other suites, so one bounded
/// invocation may not reach every key.
async fn reconcile_full_coverage(
    env: &TestEnv,
    config: &OrphanReconciliationConfig,
) -> Vec<OrphanReconcileReport> {
    let mut reports = Vec::new();
    let mut token = None;
    loop {
        let report = reconcile_orphan_blobs(
            config,
            env.object_store.as_ref() as &dyn BlobInspector,
            env.object_store.as_ref() as &dyn BlobLockProvider,
            &env.metadata,
            token,
        )
        .await
        .expect("reconcile invocation");
        token = report.next_continuation_token.clone();
        let done = token.is_none();
        reports.push(report);
        if done {
            return reports;
        }
    }
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn q_disabled_config_produces_zero_mutations() {
    let env = setup().await;
    let config = OrphanReconciliationConfig::new(false, 100, 24, scan_config(1000, Duration::ZERO))
        .expect("disabled config must construct without validation");

    let error = reconcile_orphan_blobs(
        &config,
        env.object_store.as_ref() as &dyn BlobInspector,
        env.object_store.as_ref() as &dyn BlobLockProvider,
        &env.metadata,
        None,
    )
    .await
    .expect_err("a disabled reconciler must refuse to run");

    assert!(
        matches!(error, ReconcileError::Disabled),
        "expected ReconcileError::Disabled, got {error:?}"
    );
    assert_eq!(
        count_reason_rows(&env.pool).await,
        0,
        "a disabled reconciler must never enqueue"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn a_unversioned_bucket_enqueues_aged_unreferenced_blob_once() {
    let env = setup().await;
    let content = format!("orphan-reconcile-a-{}", Uuid::new_v4()).into_bytes();
    let key = blob(&content);
    env.object_store
        .put(&key, content.into())
        .await
        .expect("put orphan blob");

    // min_object_age of zero: S3 last_modified cannot be backdated, so age
    // zero treats the freshly written blob as eligible (age is a
    // consideration floor, not a safety proof).
    let config = reconcile_config(500, Duration::ZERO);

    // Pass 1: page until our key has a durable row. The shared bucket may
    // contain other suites' blobs, so total enqueue counts are asserted as
    // lower bounds; the per-key row is asserted exactly.
    let pass_one = reconcile_full_coverage(&env, &config).await;
    assert!(
        pass_one.iter().any(|report| report.enqueued >= 1),
        "first pass must enqueue at least our key, got {pass_one:?}"
    );
    let row = candidate_row(&env.pool, &key).await.unwrap_or_else(|| {
        panic!("our key must have a durable candidate row; reports: {pass_one:?}")
    });
    assert_eq!(row.reason, ORPHAN_RECONCILE_REASON);
    assert_eq!(row.state, "pending");
    assert!(!row.operator_hold);

    // Pass 2 (conflict-skip / idempotency): full coverage again. Every
    // enqueueable blob now has a row, so nothing new may be inserted and our
    // key must classify as an existing queue row. This assumes no concurrent
    // external writer enqueues fresh unreferenced blobs into the shared
    // disposable bucket mid-test (CI runs the suites serially).
    let pass_two = reconcile_full_coverage(&env, &config).await;
    assert!(
        pass_two.iter().all(|report| report.enqueued == 0),
        "a second full pass must be insert-only-idempotent, got {pass_two:?}"
    );
    let skipped_existing: u64 = pass_two
        .iter()
        .map(|report| report.skipped_existing_queue_rows)
        .sum();
    assert!(
        skipped_existing >= 1,
        "our key must classify as an existing queue row on the second pass"
    );
    assert_eq!(
        candidate_row(&env.pool, &key)
            .await
            .expect("row persists")
            .id,
        row.id,
        "the existing queue row must be left untouched"
    );
    assert_eq!(
        count_reason_rows(&env.pool).await,
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT object_key) FROM object_gc_queue WHERE reason = $1"
        )
        .bind(ORPHAN_RECONCILE_REASON)
        .fetch_one(&env.pool)
        .await
        .expect("count distinct keys"),
        "one key must never have two queue rows"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn p_versioning_gate_blocks_non_unversioned_buckets() {
    let env = setup().await;
    let config = reconcile_config(10, Duration::ZERO);

    for state in [
        BucketVersioningState::Enabled,
        BucketVersioningState::Suspended,
        BucketVersioningState::Unknown,
    ] {
        let key = blob(format!("orphan-reconcile-p-{state:?}-{}", Uuid::new_v4()).as_bytes());
        let inspector = FakeInspector::new(vec![vec![key.clone()]]).with_versioning(Ok(state));
        let error =
            reconcile_orphan_blobs(&config, &inspector, &fake_locks(&env), &env.metadata, None)
                .await
                .unwrap_err_or_report("a non-unversioned bucket must block reconciliation");
        match error {
            ReconcileError::VersioningUnsafe(observed) => assert_eq!(
                observed, state,
                "the gate must report the observed unsafe state"
            ),
            other => panic!("expected VersioningUnsafe({state:?}), got {other:?}"),
        }
    }

    assert_eq!(
        count_reason_rows(&env.pool).await,
        0,
        "a blocked reconciler must never enqueue"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn p_versioning_gate_fails_closed_on_probe_error() {
    let env = setup().await;
    let config = reconcile_config(10, Duration::ZERO);
    let key = blob(format!("orphan-reconcile-p-probe-{}", Uuid::new_v4()).as_bytes());
    let inspector = FakeInspector::new(vec![vec![key.clone()]]).with_versioning(Err(
        ReconcileError::Probe("synthetic versioning probe failure".to_string()),
    ));

    let error = reconcile_orphan_blobs(&config, &inspector, &fake_locks(&env), &env.metadata, None)
        .await
        .expect_err("a versioning probe failure must fail closed");

    assert!(
        matches!(error, ReconcileError::VersioningUnknown),
        "expected ReconcileError::VersioningUnknown, got {error:?}"
    );
    assert_eq!(
        count_reason_rows(&env.pool).await,
        0,
        "a fail-closed reconciler must never enqueue"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn h_existing_operator_hold_row_is_preserved() {
    let env = setup().await;
    let key = blob(format!("orphan-reconcile-h-{}", Uuid::new_v4()).as_bytes());
    sqlx::query(
        r#"
        INSERT INTO object_gc_queue
            (object_key, reason, state, attempt_count, last_attempt_at, last_error,
             operator_hold, locked_at, locked_by, completed_at,
             not_before, created_at, updated_at)
        VALUES ($1, 'object_gc_orphan_reconcile_test:operator-hold', 'operator_hold', 5,
                NOW(), 'permanent test failure',
                true, NOW(), 'held-worker', NOW(),
                NOW() + INTERVAL '30 hours', NOW(), NOW())
        "#,
    )
    .bind(&key)
    .execute(&env.pool)
    .await
    .expect("insert operator-held candidate");
    let before = candidate_row(&env.pool, &key)
        .await
        .expect("candidate exists before");

    let inspector = FakeInspector::new(vec![vec![key.clone()]]);
    let report = reconcile_orphan_blobs(
        &reconcile_config(10, Duration::ZERO),
        &inspector,
        &fake_locks(&env),
        &env.metadata,
        None,
    )
    .await
    .expect("reconcile over a held row");

    assert_eq!(report.enqueued, 0, "a held key must never be re-enqueued");
    assert_eq!(
        report.skipped_operator_holds, 1,
        "a held row must classify as skipped_operator_holds"
    );
    assert_eq!(report.skipped_existing_queue_rows, 0);

    let after = candidate_row(&env.pool, &key)
        .await
        .expect("candidate exists after");
    assert_candidate_row_unchanged(&before, &after);
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn cap_stops_mid_run_and_resumes_with_current_page_token() {
    let env = setup().await;
    let keys: Vec<String> = (0..3)
        .map(|index| blob(format!("orphan-reconcile-cap-{index}-{}", Uuid::new_v4()).as_bytes()))
        .collect();
    let inspector = FakeInspector::new(vec![keys.clone()]);
    let config = reconcile_config(1, Duration::ZERO);

    // First run: cap=1 stops mid-page after the first enqueue. The resume
    // token is the token that yielded the current page (None for the first
    // page), so resume re-scans the page; insert-only enqueueing makes that
    // safe.
    let first = reconcile_orphan_blobs(&config, &inspector, &fake_locks(&env), &env.metadata, None)
        .await
        .expect("first reconcile run");
    assert_eq!(first.status, ReconcileStatus::EnqueueCapReached);
    assert!(!first.listing_complete);
    assert_eq!(first.enqueued, 1);
    assert_eq!(
        first.next_continuation_token, None,
        "cap stop must resume at the token that yielded the current page"
    );

    // Resume until completion; every key ends up with exactly one row.
    let mut total_enqueued = first.enqueued;
    let mut token = first.next_continuation_token.clone();
    let mut guard_pages = 0;
    loop {
        guard_pages += 1;
        assert!(guard_pages < 10, "resume must terminate");
        let report =
            reconcile_orphan_blobs(&config, &inspector, &fake_locks(&env), &env.metadata, token)
                .await
                .expect("resumed reconcile run");
        total_enqueued += report.enqueued;
        token = report.next_continuation_token.clone();
        if report.listing_complete {
            assert_eq!(
                report.status,
                ReconcileStatus::Completed,
                "only a fully covered listing completes"
            );
            break;
        }
    }
    assert_eq!(total_enqueued, 3, "each key must be enqueued exactly once");
    for key in &keys {
        assert!(
            candidate_row(&env.pool, key).await.is_some(),
            "key {key} must have a durable row"
        );
    }
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn partial_listing_error_returns_partial_report_not_a_complete_one() {
    let env = setup().await;
    let keys: Vec<String> = (0..2)
        .map(|index| {
            blob(format!("orphan-reconcile-partial-{index}-{}", Uuid::new_v4()).as_bytes())
        })
        .collect();
    let inspector = FakeInspector::new(vec![keys.clone(), keys.clone()]).fail_on_page(1);

    let report = reconcile_orphan_blobs(
        &reconcile_config(10, Duration::ZERO),
        &inspector,
        &fake_locks(&env),
        &env.metadata,
        None,
    )
    .await
    .expect("a listing failure must still return the partial report");

    assert_eq!(report.status, ReconcileStatus::PartialListingError);
    assert!(!report.listing_complete);
    assert_eq!(report.enqueued, 2, "only the first page may be enqueued");
    assert_eq!(
        report.objects_listed, 2,
        "counters must reflect exactly what was processed"
    );
    assert_eq!(
        report.next_continuation_token.as_deref(),
        Some("page-1"),
        "resume must retry the page that failed"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn recent_or_replaced_objects_are_never_enqueued() {
    let env = setup().await;
    let recent_key = blob(format!("orphan-reconcile-recent-{}", Uuid::new_v4()).as_bytes());
    let replaced_key = blob(format!("orphan-reconcile-replaced-{}", Uuid::new_v4()).as_bytes());

    // Listed as brand new; head agrees (same timestamp) → too recent.
    let now = Utc::now();
    let recent = FakeInspector::new(vec![vec![recent_key.clone()]]).with_listed_timestamp(now);

    // Listed as old; head reports a NEWER timestamp → object was replaced
    // after listing → uncertain, never enqueued.
    let replaced = FakeInspector::new(vec![vec![replaced_key.clone()]])
        .with_head(HeadBehavior::Timestamp(now));

    let report = reconcile_orphan_blobs(
        &reconcile_config(10, Duration::from_secs(3600)),
        &recent,
        &fake_locks(&env),
        &env.metadata,
        None,
    )
    .await
    .expect("reconcile recent object");
    assert_eq!(report.objects_too_recent, 1);
    assert_eq!(report.enqueued, 0);
    assert!(
        candidate_row(&env.pool, &recent_key).await.is_none(),
        "a too-recent object must never be enqueued"
    );

    let report = reconcile_orphan_blobs(
        &reconcile_config(10, Duration::ZERO),
        &replaced,
        &fake_locks(&env),
        &env.metadata,
        None,
    )
    .await
    .expect("reconcile replaced object");
    assert_eq!(report.uncertain_s3, 1);
    assert_eq!(report.enqueued, 0);
    assert!(
        candidate_row(&env.pool, &replaced_key).await.is_none(),
        "a replaced object must never be enqueued"
    );

    // HeadObject 404 under the lock: the object vanished after listing.
    let missing_key = blob(format!("orphan-reconcile-missing-{}", Uuid::new_v4()).as_bytes());
    let missing =
        FakeInspector::new(vec![vec![missing_key.clone()]]).with_head(HeadBehavior::Missing);
    let report = reconcile_orphan_blobs(
        &reconcile_config(10, Duration::ZERO),
        &missing,
        &fake_locks(&env),
        &env.metadata,
        None,
    )
    .await
    .expect("reconcile missing object");
    assert_eq!(report.skipped_missing, 1);
    assert_eq!(report.enqueued, 0);
    assert!(
        candidate_row(&env.pool, &missing_key).await.is_none(),
        "a missing object must never be enqueued"
    );

    // HeadObject transport failure: uncertainty, never an enqueue decision.
    let failing_key = blob(format!("orphan-reconcile-headfail-{}", Uuid::new_v4()).as_bytes());
    let failing = FakeInspector::new(vec![vec![failing_key.clone()]]).with_head(HeadBehavior::Fail);
    let report = reconcile_orphan_blobs(
        &reconcile_config(10, Duration::ZERO),
        &failing,
        &fake_locks(&env),
        &env.metadata,
        None,
    )
    .await
    .expect("reconcile head-failing object");
    assert_eq!(report.uncertain_s3, 1);
    assert_eq!(report.enqueued, 0);
    assert!(
        candidate_row(&env.pool, &failing_key).await.is_none(),
        "a head-probe failure must never be enqueued"
    );
}

/// Wraps the real ObjectStore HeadObject probe and commits a durable file
/// reference on the FIRST call — deterministically modeling a reference
/// created between the listing and the under-lock re-check, without sleeps.
struct InsertReferenceOnFirstHead<'a> {
    inner: &'a ObjectStore,
    pool: PgPool,
    user_id: Uuid,
    listed: ListedBlob,
    fired: AtomicBool,
}

impl BlobLister for InsertReferenceOnFirstHead<'_> {
    fn list_page<'a>(
        &'a self,
        _continuation: Option<&'a str>,
        _max_keys: u32,
    ) -> Pin<Box<dyn Future<Output = Result<BlobListPage, ScanError>> + Send + 'a>> {
        Box::pin(async move {
            Ok(BlobListPage {
                blobs: vec![self.listed.clone()],
                next_continuation: None,
            })
        })
    }
}

impl BlobInspector for InsertReferenceOnFirstHead<'_> {
    fn head_last_modified<'a>(
        &'a self,
        key: &'a str,
    ) -> BoxReconcileFuture<'a, Option<DateTime<Utc>>> {
        Box::pin(async move {
            if !self.fired.swap(true, Ordering::SeqCst) {
                insert_file_reference(&self.pool, self.user_id, key, 1).await;
            }
            self.inner
                .head_object_last_modified(key)
                .await
                .map_err(|error| ReconcileError::Probe(error.to_string()))
        })
    }

    fn bucket_versioning<'a>(&'a self) -> BoxReconcileFuture<'a, BucketVersioningState> {
        Box::pin(async move { Ok(BucketVersioningState::Unversioned) })
    }
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn c_referenced_file_blob_is_not_enqueued() {
    let env = setup().await;
    let content = format!("orphan-reconcile-c-{}", Uuid::new_v4()).into_bytes();
    let key = blob(&content);
    env.object_store
        .put(&key, content.clone().into())
        .await
        .expect("put referenced blob");
    let user_id = insert_user(&env.pool, "c").await;
    insert_file_reference(&env.pool, user_id, &key, content.len() as i64).await;

    let reports = reconcile_full_coverage(&env, &reconcile_config(500, Duration::ZERO)).await;
    let skipped_referenced: u64 = reports.iter().map(|report| report.skipped_referenced).sum();
    assert!(
        skipped_referenced >= 1,
        "our referenced key must be counted as skipped_referenced, got {reports:?}"
    );
    assert!(
        candidate_row(&env.pool, &key).await.is_none(),
        "a referenced blob must never get a queue row"
    );
    assert_eq!(
        count_reason_rows_for_key(&env.pool, &key).await,
        0,
        "the producer must never record its own reason for a referenced key"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn d_reference_created_between_discovery_and_enqueue_prevents_enqueue() {
    let env = setup().await;
    let content = format!("orphan-reconcile-d-{}", Uuid::new_v4()).into_bytes();
    let key = blob(&content);
    env.object_store
        .put(&key, content.into())
        .await
        .expect("put blob");
    let user_id = insert_user(&env.pool, "d").await;

    // List the blob at its real HeadObject timestamp so the under-lock
    // freshness/replacement re-check sees an unchanged object; the reference
    // insert fires on the first head call, before the reference count.
    let head_ts = env
        .object_store
        .head_object_last_modified(&key)
        .await
        .expect("head blob")
        .expect("blob exists");
    let inspector = InsertReferenceOnFirstHead {
        inner: env.object_store.as_ref(),
        pool: env.pool.clone(),
        user_id,
        listed: ListedBlob {
            key: key.clone(),
            last_modified: head_ts,
        },
        fired: AtomicBool::new(false),
    };

    let report = reconcile_orphan_blobs(
        &reconcile_config(10, Duration::ZERO),
        &inspector,
        env.object_store.as_ref() as &dyn BlobLockProvider,
        &env.metadata,
        None,
    )
    .await
    .expect("reconcile with a mid-flight reference creation");

    assert!(
        inspector.fired.load(Ordering::SeqCst),
        "the side effect must actually have run"
    );
    assert_eq!(
        report.skipped_referenced, 1,
        "the reference created between discovery and enqueue must be observed"
    );
    assert_eq!(report.enqueued, 0);
    assert!(
        candidate_row(&env.pool, &key).await.is_none(),
        "a blob referenced by enqueue time must never get a queue row"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn e_reference_creation_serializes_with_enqueue_via_advisory_lock() {
    let env = setup().await;
    let key = blob(format!("orphan-reconcile-e-{}", Uuid::new_v4()).as_bytes());
    let user_id = insert_user(&env.pool, "e").await;

    // tx1 takes the same per-key advisory lock the producer will want and
    // holds it open, so the producer must wait instead of enqueueing against
    // a stale zero-reference observation. Remember tx1's backend pid: the
    // waiter is identified by matching classid/objid against the lock tx1
    // actually holds (avoids reimplementing hashtextextended's split).
    let mut tx1: Transaction<'_, Postgres> = env.pool.begin().await.expect("begin tx1");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 7277856))")
        .bind(&key)
        .execute(&mut *tx1)
        .await
        .expect("take per-key advisory lock in tx1");
    let tx1_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *tx1)
        .await
        .expect("tx1 backend pid");

    let config = reconcile_config(10, Duration::ZERO);
    let inspector = FakeInspector::new(vec![vec![key.clone()]]);
    let locks = env.object_store.clone();
    let metadata = env.metadata.clone();
    let reconcile_task = tokio::spawn(async move {
        reconcile_orphan_blobs(
            &config,
            &inspector,
            locks.as_ref() as &dyn BlobLockProvider,
            &metadata,
            None,
        )
        .await
    });

    // Poll pg_locks (bounded) until the reconcile session is actually seen
    // WAITING on the advisory lock tx1 holds. The observation matters: it
    // proves the enqueue path serializes against same-key reference creation
    // rather than merely happening to observe the row afterwards.
    let mut observed_waiting = false;
    for _ in 0..50 {
        let waiting: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM pg_locks waiting
                WHERE waiting.locktype = 'advisory'
                  AND waiting.granted = false
                  AND (waiting.classid, waiting.objid) IN (
                      SELECT holding.classid, holding.objid
                      FROM pg_locks holding
                      WHERE holding.locktype = 'advisory'
                        AND holding.granted = true
                        AND holding.pid = $1
                  )
            )
            "#,
        )
        .bind(tx1_pid)
        .fetch_one(&env.pool)
        .await
        .expect("poll pg_locks");
        if waiting {
            observed_waiting = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        observed_waiting,
        "the reconcile session must be observed waiting on the per-key advisory lock"
    );

    // Only now make the reference durable and release the lock; the producer
    // must recheck under the lock and skip the key.
    let file_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id) VALUES ($1, 'reconcile.bin', $2, 1, 'application/octet-stream', $3, $4, $5)",
    )
    .bind(file_id)
    .bind(format!("/orphan-reconcile-e-{}.bin", Uuid::new_v4()))
    .bind(digest(&key))
    .bind(&key)
    .bind(user_id)
    .execute(&mut *tx1)
    .await
    .expect("create file reference inside tx1");
    tx1.commit().await.expect("commit tx1 and release lock");

    let report = tokio::time::timeout(Duration::from_secs(30), reconcile_task)
        .await
        .expect("reconcile must unblock once the lock is released")
        .expect("reconcile task")
        .expect("reconcile run");
    assert_eq!(
        report.skipped_referenced, 1,
        "the reference committed while the producer waited must be observed"
    );
    assert_eq!(report.enqueued, 0);
    assert_eq!(report.uncertain_db, 0);
    assert!(
        candidate_row(&env.pool, &key).await.is_none(),
        "a key referenced before the under-lock recheck must never be enqueued"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn f_cross_tenant_reference_prevents_enqueue() {
    let env = setup().await;
    let content = format!("orphan-reconcile-f-{}", Uuid::new_v4()).into_bytes();
    let key = blob(&content);
    env.object_store
        .put(&key, content.clone().into())
        .await
        .expect("put blob");
    let tenant_b = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, storage_quota, tenant_id) VALUES ($1, $2, $3, 'test', 'Orphan Reconcile', 1000000, $4)",
    )
    .bind(user_b)
    .bind(format!("orphan-reconcile-f-{user_b}"))
    .bind(format!("orphan-reconcile-f-{user_b}@test.local"))
    .bind(tenant_b)
    .execute(&env.pool)
    .await
    .expect("create tenant-b user");
    // The reference lives entirely under tenant B; the producer has no
    // tenant context at all — reference counting is global.
    sqlx::query(
        "INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id, tenant_id) VALUES ($1, 'reconcile.bin', $2, $3, 'application/octet-stream', $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(format!("/orphan-reconcile-f-{}.bin", Uuid::new_v4()))
    .bind(content.len() as i64)
    .bind(digest(&key))
    .bind(&key)
    .bind(user_b)
    .bind(tenant_b)
    .execute(&env.pool)
    .await
    .expect("create cross-tenant file reference");

    let reports = reconcile_full_coverage(&env, &reconcile_config(500, Duration::ZERO)).await;
    let skipped_referenced: u64 = reports.iter().map(|report| report.skipped_referenced).sum();
    assert!(
        skipped_referenced >= 1,
        "a cross-tenant reference must still protect the blob, got {reports:?}"
    );
    assert!(
        candidate_row(&env.pool, &key).await.is_none(),
        "a cross-tenant referenced blob must never get a queue row"
    );
    assert_eq!(count_reason_rows_for_key(&env.pool, &key).await, 0);
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn g_vault_mail_version_and_replication_references_prevent_enqueue() {
    let env = setup().await;
    let user_id = insert_user(&env.pool, "g").await;

    // The key must be the sha256 of the exact bytes written: ObjectStore::put
    // verifies content against the content-addressed key. Every listed blob
    // is real; each gets exactly one durable reference of a different kind.
    // Reference kinds mirror the scanner suite's insertion idioms so both
    // suites prove the same global counting contract.
    let contents: Vec<Vec<u8>> = ["vault", "message", "version", "replication"]
        .iter()
        .map(|kind| format!("orphan-reconcile-g-{kind}-{}", Uuid::new_v4()).into_bytes())
        .collect();
    let keys: Vec<String> = contents.iter().map(|content| blob(content)).collect();
    for (content, key) in contents.into_iter().zip(&keys) {
        env.object_store
            .put(key, content.into())
            .await
            .expect("put blob");
    }
    let [vault_key, message_key, version_key, replication_key] =
        <[String; 4]>::try_from(keys).expect("four keys");

    // vault_files: canonical keys satisfy 'blobs/' || sha256.
    let vault_id = Uuid::new_v4();
    sqlx::query("INSERT INTO vaults (id, owner_user_id, name, adapter) VALUES ($1, $2, 'reconcile vault', 'obsidian')")
        .bind(vault_id)
        .bind(user_id)
        .execute(&env.pool)
        .await
        .expect("create vault");
    sqlx::query("INSERT INTO vault_files (id, vault_id, relative_path, sha256, size, server_rev) VALUES ($1, $2, 'reconcile.bin', $3, 1, 1)")
        .bind(Uuid::new_v4())
        .bind(vault_id)
        .bind(digest(&vault_key))
        .execute(&env.pool)
        .await
        .expect("create vault reference");

    // mail_messages.blob_key.
    sqlx::query(
        "INSERT INTO mail_messages (id, owner_id, imported_by, blob_key) VALUES ($1, $2, $2, $3)",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(&message_key)
    .execute(&env.pool)
    .await
    .expect("create mail message reference");

    // file_versions: retained non-current version of a live file.
    let current_key = blob(format!("orphan-reconcile-g-current-{}", Uuid::new_v4()).as_bytes());
    let file_id = insert_file_reference(&env.pool, user_id, &current_key, 1).await;
    let version_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO file_versions (id, file_id, version_number, content_hash, storage_key, size, created_by) VALUES ($1, $2, 2, $3, $4, 1, $5)",
    )
    .bind(version_id)
    .bind(file_id)
    .bind(digest(&version_key))
    .bind(&version_key)
    .bind(user_id)
    .execute(&env.pool)
    .await
    .expect("create retained version reference");

    // replication_jobs with an active status.
    sqlx::query(
        "INSERT INTO replication_jobs (id, file_id, file_version_id, storage_key, status) VALUES ($1, $2, $3, $4, 'queued')",
    )
    .bind(Uuid::new_v4())
    .bind(file_id)
    .bind(version_id)
    .bind(&replication_key)
    .execute(&env.pool)
    .await
    .expect("create replication job reference");

    let reports = reconcile_full_coverage(&env, &reconcile_config(500, Duration::ZERO)).await;
    let skipped_referenced: u64 = reports.iter().map(|report| report.skipped_referenced).sum();
    assert!(
        skipped_referenced >= 4,
        "all four reference kinds must protect their blobs, got {reports:?}"
    );
    for key in [&vault_key, &message_key, &version_key, &replication_key] {
        assert!(
            candidate_row(&env.pool, key).await.is_none(),
            "blob {key} must never get a queue row"
        );
        assert_eq!(
            count_reason_rows_for_key(&env.pool, key).await,
            0,
            "blob {key} must never be recorded with the producer reason"
        );
    }
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn i_existing_processing_candidate_keeps_lease_attempts_and_schedule() {
    let env = setup().await;
    let key = blob(format!("orphan-reconcile-i-{}", Uuid::new_v4()).as_bytes());
    sqlx::query(
        r#"
        INSERT INTO object_gc_queue
            (object_key, reason, state, attempt_count,
             locked_at, locked_by, not_before, created_at, updated_at)
        VALUES ($1, 'object_gc_orphan_reconcile_test:processing', 'processing', 3,
                NOW(), 'worker-x', NOW() + INTERVAL '2 hours', NOW(), NOW())
        "#,
    )
    .bind(&key)
    .execute(&env.pool)
    .await
    .expect("insert processing candidate");
    let before = candidate_row(&env.pool, &key)
        .await
        .expect("candidate exists before");

    let inspector = FakeInspector::new(vec![vec![key.clone()]]);
    let report = reconcile_orphan_blobs(
        &reconcile_config(10, Duration::ZERO),
        &inspector,
        env.object_store.as_ref() as &dyn BlobLockProvider,
        &env.metadata,
        None,
    )
    .await
    .expect("reconcile over a processing row");

    assert_eq!(
        report.enqueued, 0,
        "an existing row must never be duplicated"
    );
    assert_eq!(
        report.skipped_existing_queue_rows, 1,
        "a processing row is an ordinary existing row, not an operator hold"
    );
    assert_eq!(report.skipped_operator_holds, 0);
    assert_candidate_row_unchanged(
        &before,
        &candidate_row(&env.pool, &key)
            .await
            .expect("candidate exists after"),
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn j_existing_retry_candidate_keeps_not_before_floor() {
    let env = setup().await;
    let key = blob(format!("orphan-reconcile-j-{}", Uuid::new_v4()).as_bytes());
    sqlx::query(
        r#"
        INSERT INTO object_gc_queue
            (object_key, reason, state, attempt_count, last_error,
             not_before, created_at, updated_at)
        VALUES ($1, 'object_gc_orphan_reconcile_test:retry', 'retry', 2, 'boom',
                NOW() + INTERVAL '5 hours', NOW(), NOW())
        "#,
    )
    .bind(&key)
    .execute(&env.pool)
    .await
    .expect("insert retry candidate");
    let before = candidate_row(&env.pool, &key)
        .await
        .expect("candidate exists before");

    let inspector = FakeInspector::new(vec![vec![key.clone()]]);
    let report = reconcile_orphan_blobs(
        &reconcile_config(10, Duration::ZERO),
        &inspector,
        env.object_store.as_ref() as &dyn BlobLockProvider,
        &env.metadata,
        None,
    )
    .await
    .expect("reconcile over a retry row");

    assert_eq!(report.enqueued, 0);
    assert_eq!(report.skipped_existing_queue_rows, 1);
    assert_eq!(report.skipped_operator_holds, 0);
    assert_candidate_row_unchanged(
        &before,
        &candidate_row(&env.pool, &key)
            .await
            .expect("candidate exists after"),
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn k_terminal_and_special_states_are_not_reset() {
    let env = setup().await;
    let mut keys = Vec::new();
    for state in ["referenced", "deleted", "missing", "invalid_key"] {
        let key = blob(format!("orphan-reconcile-k-{state}-{}", Uuid::new_v4()).as_bytes());
        sqlx::query(
            r#"
            INSERT INTO object_gc_queue
                (object_key, reason, state, attempt_count, completed_at,
                 not_before, created_at, updated_at)
            VALUES ($1, 'object_gc_orphan_reconcile_test:terminal', $2, 1, NOW(),
                    NOW() + INTERVAL '1 hour', NOW(), NOW())
            "#,
        )
        .bind(&key)
        .bind(state)
        .execute(&env.pool)
        .await
        .expect("insert terminal candidate");
        keys.push((key, state.to_string()));
    }

    let inspector = FakeInspector::new(vec![keys.iter().map(|(key, _)| key.clone()).collect()]);
    let report = reconcile_orphan_blobs(
        &reconcile_config(10, Duration::ZERO),
        &inspector,
        env.object_store.as_ref() as &dyn BlobLockProvider,
        &env.metadata,
        None,
    )
    .await
    .expect("reconcile over terminal rows");

    assert_eq!(
        report.enqueued, 0,
        "terminal rows must never be re-enqueued"
    );
    assert_eq!(report.skipped_existing_queue_rows, 4);
    assert_eq!(report.skipped_operator_holds, 0);
    for (key, state) in &keys {
        let (row_state, completed_at): (String, Option<DateTime<Utc>>) =
            sqlx::query_as("SELECT state, completed_at FROM object_gc_queue WHERE object_key = $1")
                .bind(key)
                .fetch_one(&env.pool)
                .await
                .expect("terminal candidate row");
        assert_eq!(&row_state, state, "state must not change");
        assert!(
            completed_at.is_some(),
            "completed_at must survive reconciliation for {state}"
        );
        assert_eq!(count_reason_rows_for_key(&env.pool, key).await, 0);
    }
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn l_concurrent_reconcilers_elect_single_producer() {
    let env = setup().await;
    // A dedicated bucket keeps the run hermetic: both invocations must list
    // exactly our one aged (min_age zero), unreferenced blob.
    let bucket = format!("rustshare-test-conc-{}", Uuid::new_v4());
    let store = unique_object_store(&env, &bucket).await;
    let content = format!("orphan-reconcile-l-{}", Uuid::new_v4()).into_bytes();
    let key = blob(&content);
    store.put(&key, content.into()).await.expect("put blob");

    let config = reconcile_config(10, Duration::ZERO);
    let (left, right) = tokio::join!(
        reconcile_orphan_blobs(
            &config,
            store.as_ref() as &dyn BlobInspector,
            store.as_ref() as &dyn BlobLockProvider,
            &env.metadata,
            None,
        ),
        reconcile_orphan_blobs(
            &config,
            store.as_ref() as &dyn BlobInspector,
            store.as_ref() as &dyn BlobLockProvider,
            &env.metadata,
            None,
        ),
    );
    let left = left.expect("left reconcile invocation");
    let right = right.expect("right reconcile invocation");

    assert_eq!(
        left.enqueued + right.enqueued,
        1,
        "exactly one concurrent reconciler may insert the candidate \
         (left: {left:?}, right: {right:?})"
    );
    assert_eq!(
        left.skipped_existing_queue_rows + right.skipped_existing_queue_rows,
        1,
        "the loser must observe the winner's row as an existing queue row"
    );
    assert_eq!(left.uncertain_db + right.uncertain_db, 0);
    let row = candidate_row(&env.pool, &key)
        .await
        .expect("exactly one queue row exists for the key");
    assert_eq!(row.state, "pending");
    assert_eq!(row.attempt_count, 0);
    assert_eq!(row.reason, ORPHAN_RECONCILE_REASON);

    // Best-effort cleanup of the scratch bucket.
    let _ = store.delete(&key).await;
    delete_bucket_best_effort(&bucket).await;
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn m_database_outage_produces_no_candidate() {
    let env = setup().await;
    let content = format!("orphan-reconcile-m-{}", Uuid::new_v4()).into_bytes();
    let key = blob(&content);
    env.object_store
        .put(&key, content.into())
        .await
        .expect("put blob");

    // A metadata store whose pool is closed: every per-key metadata probe
    // fails immediately at checkout, modeling a database outage after
    // startup. (A connect_lazy pool against a dead port depends on
    // driver-specific refused-connect timing — it can block checkout for a
    // long retry window — so the suite uses the closed-pool idiom, same as
    // the scanner suite's outage test. No sleeps involved.)
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    let dead_pool = PgPool::connect(&database_url)
        .await
        .expect("connect disposable pool");
    let dead_metadata = MetadataStore::new(dead_pool.clone());
    dead_pool.close().await;

    let report = reconcile_orphan_blobs(
        &reconcile_config(100, Duration::ZERO),
        env.object_store.as_ref() as &dyn BlobInspector,
        &fake_locks(&env),
        &dead_metadata,
        None,
    )
    .await
    .expect("reconcile survives a metadata outage");

    assert!(
        report.uncertain_db > 0,
        "every probed key must degrade to uncertain_db, got {report:?}"
    );
    assert!(!report.db_reachable);
    assert_eq!(
        report.enqueued, 0,
        "an outage must never produce candidates"
    );
    assert!(
        candidate_row(&env.pool, &key).await.is_none(),
        "zero queue rows may exist for the listed key"
    );
    assert_eq!(count_reason_rows_for_key(&env.pool, &key).await, 0);
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn m2_reference_check_error_never_enqueues() {
    let env = setup().await;
    // Fail fast: the denied-role pool downgrades via SET ROLE, which only the
    // connecting superuser may do. Without this the pool would retry the
    // failing after_connect with backoff until its acquire timeout.
    let is_superuser: bool = sqlx::query_scalar("SELECT current_setting('is_superuser')::bool")
        .fetch_one(&env.pool)
        .await
        .expect("read is_superuser");
    assert!(
        is_superuser,
        "m2 requires a superuser DATABASE_URL (SET ROLE downgrade)"
    );

    // A dedicated bucket keeps the run hermetic: the denied role sees exactly
    // our one aged (min_age zero), unreferenced blob.
    let bucket = format!("rustshare-test-refdenied-{}", Uuid::new_v4());
    let store = unique_object_store(&env, &bucket).await;
    let content = format!("orphan-reconcile-m2-{}", Uuid::new_v4()).into_bytes();
    let key = blob(&content);
    store.put(&key, content.into()).await.expect("put blob");

    // A dedicated NOLOGIN role whose only privilege gap is SELECT on
    // mail_attachments, one of the seven tables count_blob_references reads:
    // the reference query fails with a permission error while the enqueue
    // path (INSERT and SELECT on object_gc_queue) stays fully functional.
    // This models a partial metadata outage — the queue writable, the
    // reference snapshot unreadable — which a uniformly closed pool cannot
    // model. The table is chosen from the non-RLS reference tables so a
    // missing grant is a hard error, not a silently empty RLS-filtered count.
    // The pool below connects as the test superuser and every physical
    // connection runs `SET ROLE` to the restricted role via after_connect, so
    // no credential for the role ever exists: NOLOGIN roles cannot
    // authenticate at all, and a superuser may SET ROLE to any role without
    // membership or a password. An after_connect error hard-closes that
    // connection and is never handed out by the pool, so a connection can
    // never silently skip the SET ROLE.
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    let database_name = database_url
        .rsplit('/')
        .next()
        .expect("DATABASE_URL database");
    let database_name = database_name.replace('"', "\"\"");
    // Hex-only identifiers keep the interpolated DDL free of quoting hazards.
    let role = format!("ogc_ref_denied_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE ROLE {role} NOLOGIN"))
        .execute(&env.pool)
        .await
        .expect("create denied role");
    sqlx::query(&format!(
        "GRANT CONNECT ON DATABASE \"{database_name}\" TO {role}"
    ))
    .execute(&env.pool)
    .await
    .expect("grant connect");
    sqlx::query(&format!("GRANT USAGE ON SCHEMA public TO {role}"))
        .execute(&env.pool)
        .await
        .expect("grant schema usage");
    sqlx::query(&format!(
        "GRANT SELECT ON files, file_versions, vault_files, mail_messages, \
         mail_message_parts, replication_jobs TO {role}"
    ))
    .execute(&env.pool)
    .await
    .expect("grant reference-table selects");
    sqlx::query(&format!(
        "GRANT INSERT, SELECT ON object_gc_queue TO {role}"
    ))
    .execute(&env.pool)
    .await
    .expect("grant queue writes");

    let denied_pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(set_role_on_connect(role.clone()))
        .connect(&database_url)
        .await
        .expect("connect with SET ROLE pool");
    let denied_metadata = MetadataStore::new(denied_pool.clone());

    // Collect every outcome first, clean up, and only then assert: a failed
    // assertion must not leak the role, the pool, or the scratch bucket.
    // The permission gap must be real before it can mean anything in the
    // report: the reference query errors, and the error is a permission
    // denial (not a transport artifact).
    let sanity_error = denied_metadata.count_blob_references(&key).await.err();
    let reconcile_result = reconcile_orphan_blobs(
        &reconcile_config(10, Duration::ZERO),
        store.as_ref() as &dyn BlobInspector,
        &fake_locks(&env),
        &denied_metadata,
        None,
    )
    .await;
    let candidate = candidate_row(&env.pool, &key).await;
    let reason_rows = count_reason_rows_for_key(&env.pool, &key).await;

    // Cleanup always runs before the assertions above can fail.
    denied_pool.close().await;
    let _ = sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&env.pool)
        .await;
    sqlx::query(&format!("DROP ROLE IF EXISTS {role}"))
        .execute(&env.pool)
        .await
        .expect("drop denied role");
    // Best-effort cleanup of the scratch bucket.
    let _ = store.delete(&key).await;
    delete_bucket_best_effort(&bucket).await;

    let error = sanity_error.expect("SELECT on mail_attachments must be denied for the test role");
    assert!(
        error
            .to_string()
            .to_lowercase()
            .contains("permission denied"),
        "expected a permission-denied error, got {error}"
    );

    let report = reconcile_result.expect("reconcile survives a reference-check outage");
    assert_eq!(
        report.enqueued, 0,
        "a reference-check error must never be read as zero references"
    );
    assert_eq!(
        report.objects_listed, 1,
        "the dedicated bucket must list exactly our blob"
    );
    assert!(
        report.uncertain_db >= 1,
        "a reference-check failure must degrade to uncertain_db, got {report:?}"
    );
    assert!(!report.db_reachable);
    assert!(
        candidate.is_none(),
        "a blob whose references cannot be read must never get a queue row"
    );
    assert_eq!(
        reason_rows, 0,
        "the producer must never record its own reason for an unreadable key"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage with bucket-versioning support"]
async fn p2_versioned_bucket_blocks_reconciliation_real_store() {
    let env = setup().await;
    // Dedicated bucket: enabling versioning must not disturb other suites
    // sharing the disposable bucket.
    let bucket = format!("rustshare-test-ver-{}", Uuid::new_v4());
    let store = unique_object_store(&env, &bucket).await;
    let reason_rows_before = count_reason_rows(&env.pool).await;

    let client = raw_s3_client().await;
    let enable = client
        .put_bucket_versioning()
        .bucket(&bucket)
        .versioning_configuration(
            aws_sdk_s3::types::VersioningConfiguration::builder()
                .status(aws_sdk_s3::types::BucketVersioningStatus::Enabled)
                .build(),
        )
        .send()
        .await;

    match enable {
        Ok(_) => {
            let state = store
                .bucket_versioning()
                .await
                .expect("probe versioning on the real store");
            assert_eq!(
                state,
                BucketVersioningState::Enabled,
                "this RustFS image supports bucket versioning; the probe must observe it"
            );
            let error = reconcile_orphan_blobs(
                &reconcile_config(10, Duration::ZERO),
                store.as_ref() as &dyn BlobInspector,
                store.as_ref() as &dyn BlobLockProvider,
                &env.metadata,
                None,
            )
            .await
            .unwrap_err_or_report("a versioned bucket must block reconciliation");
            match error {
                ReconcileError::VersioningUnsafe(observed) => assert_eq!(
                    observed,
                    BucketVersioningState::Enabled,
                    "the gate must report the observed unsafe state"
                ),
                other => panic!("expected VersioningUnsafe(Enabled), got {other:?}"),
            }
            assert_eq!(
                count_reason_rows(&env.pool).await,
                reason_rows_before,
                "a blocked reconciler must never enqueue"
            );
        }
        Err(put_error) => {
            // The image rejected PutBucketVersioning (e.g. NotImplemented):
            // the Enabled arm is covered only by the versioning-gate fakes and
            // the SDK mapping unit tests in the storage crate. Prove here only
            // that the real probe fails closed — it must not invent an Enabled
            // state the store refused to set.
            eprintln!(
                "rustfs rejected PutBucketVersioning ({put_error}); \
                 skipping the real Enabled gate (fakes cover it)"
            );
            if let Ok(state) = store.bucket_versioning().await {
                assert_ne!(
                    state,
                    BucketVersioningState::Enabled,
                    "the probe must not report Enabled when the store rejected enabling"
                );
            }
        }
    }

    // Best-effort cleanup of the scratch bucket: empty it first so the
    // best-effort bucket delete can actually succeed.
    empty_bucket_best_effort(&bucket).await;
    delete_bucket_best_effort(&bucket).await;
}

/// Convenience for the versioning-gate tests: a reconcile that must fail.
trait ExpectReconcileError {
    fn unwrap_err_or_report(self, message: &str) -> ReconcileError;
}

impl ExpectReconcileError for Result<OrphanReconcileReport, ReconcileError> {
    fn unwrap_err_or_report(self, message: &str) -> ReconcileError {
        match self {
            Err(error) => error,
            Ok(report) => panic!("{message}, got report {report:?}"),
        }
    }
}
