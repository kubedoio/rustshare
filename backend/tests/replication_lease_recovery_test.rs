//! Lease recovery and fencing contracts for replication jobs (post-fix).
//!
//! The replication worker leases `replication_jobs` rows before copying
//! content to remote targets. Two production hazards are pinned here:
//!
//! * **Stale leases** — a worker that crashes (or stalls past the lease
//!   timeout) while a job is `syncing` previously stranded the job forever:
//!   the old lease predicate only considered `queued`/`retrying` rows. The
//!   leaser now also reclaims `syncing` rows whose `leased_at` is older than
//!   the lease timeout, with no other intervention.
//! * **Lost-lease writes** — every job mutation used to be an unfenced
//!   `UPDATE ... WHERE id = $1`, so a stale worker could complete, retry or
//!   fail a job that a replacement worker already reclaimed, and clobber the
//!   file version's replication state along the way. Job transitions are now
//!   compare-and-swap guarded by `(id, lease_token, status = 'syncing')` and
//!   version-state transitions by the current `replication_state`.
//!
//! Invariants proven here:
//!
//! * **A** — a job whose worker crashed after leasing (expired `leased_at`)
//!   is reclaimed by a plain `lease_replication_jobs` call; the row comes back
//!   `syncing` with a FRESH per-job token (recovery rotates the token) and an
//!   incremented `attempt_count`.
//! * **B** — after a reclaim, `mark_replication_job_completed` with the
//!   obsolete token returns `Ok(false)` and changes nothing; the current
//!   token returns `Ok(true)` and completes the job.
//! * **C** — the same stale-token rejection holds for
//!   `mark_replication_job_retrying` and `mark_replication_job_failed`.
//! * **D** — an expired lease is reclaimed exactly once under sequential
//!   leasers: the post-claim predicate excludes the freshly reclaimed row,
//!   so an immediate second lease gets nothing. (Mutual exclusion between
//!   truly concurrent leasers is `FOR UPDATE SKIP LOCKED`, not this test.)
//! * **E** — a healthy (unexpired) `syncing` job is never stolen.
//! * **F** — `renew_replication_job_lease` with the current token refreshes
//!   `leased_at` (a subsequent lease cannot steal the job); a stale token
//!   returns `Ok(false)`.
//! * **G** — reclaim is selective: an expired `syncing` job is reclaimed with
//!   no other action, while a healthy `syncing` peer and a malformed `syncing`
//!   row with NULL `leased_at` (unknown lease age — deliberately NOT eligible)
//!   are left untouched.
//! * **H** — retry exhaustion does not block crash recovery: a stale `syncing`
//!   job already at `max_attempts` is still reclaimable (store-level). Every
//!   post-claim path is terminal: the worker's `attempt_count < max_attempts`
//!   gate routes an exhausted job to `failed`, and a version-guard conflict
//!   completes the job via the fenced mark (see J) — neither loops.
//! * **I** — intentionally omitted: the zero-target contract is covered by
//!   `replication_intent_atomicity_test` (upload side) and the store-level
//!   fenced completion is covered by B/J.
//! * **J** — with two jobs for one version, a reclaimed-and-completed first
//!   job sets the version `fully_replicated`; a stale second job can no longer
//!   clobber it (`update_file_version_replication_state_if_current` with
//!   `&[Syncing]` returns `Ok(false)`). The worker then finishes ITS OWN job
//!   terminally via the fenced mark — the guard rejection proves the version
//!   is already replicated — so the job cannot cycle
//!   reclaim → renew → guard-reject forever; the store-level half of that
//!   path is exercised here because the worker tick has no test seam.
//! * **K** — crash during remote completion: pre-crash attempt records stay,
//!   stale-token fenced writes are rejected, and the recovery lease completes
//!   the job normally.
//! * **L** — batch freshness: renewing a late-batch job before processing
//!   moves `leased_at` forward so a second worker cannot steal it.
//!
//! Time is controlled deterministically: `leased_at` / `next_attempt_at` are
//! manipulated with SQL interval arithmetic on the pool; the only sleeps are
//! trivial DB-clock awaits, and none are used for lease timing. No worker
//! loop runs in this suite; there is no worker-level test seam, so the worker
//! tick itself is not exercised.
//!
//! Run against the disposable local stack (never the developer database):
//!
//! ```text
//! DATABASE_URL=postgres://rustshare:<pw>@127.0.0.1:15432/rustshare_test \
//! S3_ENDPOINT=http://127.0.0.1:19000 S3_BUCKET=rustshare-test-integration \
//! AWS_ACCESS_KEY_ID=<rustfs-root-user> AWS_SECRET_ACCESS_KEY=<rustfs-root-password> \
//! RUSTSHARE_TEST_DISPOSABLE_DB=1 RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1 \
//!   cargo test -p rustshare-server --test replication_lease_recovery_test \
//!     -- --ignored --test-threads=1
//! ```

mod contracts;

use contracts::common::{cleanup_tenant, setup_test_env, TestContext};

use std::sync::LazyLock;
use std::time::Duration;

use chrono::Utc;
use rustshare_core::domain::{ReplicationJob, ReplicationJobStatus, ReplicationState};
use rustshare_storage::ReplicationAttemptRecord;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Lease fixtures are process-global, so every test serializes on this guard
/// (same convention as the intent-atomicity suite).
static SERIAL: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

const LEASE_TIMEOUT_SECS: i64 = 300;

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

fn blob_key(content: &[u8]) -> String {
    format!("blobs/{}", hex::encode(Sha256::digest(content)))
}

/// Seed exactly one enabled replication target. `replication_targets` is
/// global (no tenant column): the caller MUST remove the row again (see
/// [`FixtureCleanupGuard`]).
async fn seed_enabled_replication_target(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO replication_targets (id, name, destination_type, endpoint, enabled) \
         VALUES ($1, $2, 's3', 'http://127.0.0.1:1', TRUE)",
    )
    .bind(id)
    .bind(format!("lease-recovery-target-{id}"))
    .execute(pool)
    .await
    .expect("seed enabled replication target");
    id
}

/// Seed a `files` + `file_versions` pair directly (no upload) so the
/// replication job foreign keys are satisfied. Returns
/// `(file_id, version_id, storage_key)`.
async fn seed_file_version(ctx: &TestContext, owner_id: Uuid, tag: Uuid) -> (Uuid, Uuid, String) {
    let file_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();
    let content = format!("lease-recovery-{tag}").into_bytes();
    let storage_key = blob_key(&content);
    let content_hash = hex::encode(Sha256::digest(&content));

    sqlx::query(
        "INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id, tenant_id) \
         VALUES ($1, $2, $3, $4, 'text/plain', $5, $6, $7, $8)",
    )
    .bind(file_id)
    .bind(format!("lease-{tag}.txt"))
    .bind(format!("/lease-{tag}.txt"))
    .bind(content.len() as i64)
    .bind(&content_hash)
    .bind(&storage_key)
    .bind(owner_id)
    .bind(ctx.tenant_id)
    .execute(&ctx.pool)
    .await
    .expect("seed files row");

    sqlx::query(
        "INSERT INTO file_versions (id, file_id, version_number, content_hash, storage_key, size, created_by, tenant_id, replication_state) \
         VALUES ($1, $2, 1, $3, $4, $5, $6, $7, 'queued')",
    )
    .bind(version_id)
    .bind(file_id)
    .bind(&content_hash)
    .bind(&storage_key)
    .bind(content.len() as i64)
    .bind(owner_id)
    .bind(ctx.tenant_id)
    .execute(&ctx.pool)
    .await
    .expect("seed file_versions row");

    (file_id, version_id, storage_key)
}

/// Queue one replication job for the seeded version via the store API.
async fn create_queued_job(
    ctx: &TestContext,
    file_id: Uuid,
    version_id: Uuid,
    storage_key: &str,
) -> ReplicationJob {
    let job = ReplicationJob::new(file_id, version_id, storage_key.to_string());
    ctx.metadata_store
        .create_replication_job(&job)
        .await
        .expect("create replication job");
    job
}

async fn lease_due_jobs(ctx: &TestContext) -> Vec<ReplicationJob> {
    ctx.metadata_store
        .lease_replication_jobs(10, LEASE_TIMEOUT_SECS)
        .await
        .expect("lease replication jobs")
}

fn find_job(jobs: &[ReplicationJob], job_id: Uuid) -> Option<&ReplicationJob> {
    jobs.iter().find(|job| job.id == job_id)
}

/// Make a leased job eligible for reclaim regardless of the lease timeout.
async fn force_lease_expiry(pool: &PgPool, job_id: Uuid) {
    sqlx::query(
        "UPDATE replication_jobs SET leased_at = NOW() - interval '10 minutes' WHERE id = $1",
    )
    .bind(job_id)
    .execute(pool)
    .await
    .expect("force lease expiry");
}

async fn job_status_and_token(pool: &PgPool, job_id: Uuid) -> (String, Option<Uuid>) {
    let row = sqlx::query("SELECT status, lease_token FROM replication_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(pool)
        .await
        .expect("load replication job row");
    (row.get(0), row.get(1))
}

async fn leased_at_epoch(pool: &PgPool, job_id: Uuid) -> Option<f64> {
    sqlx::query_scalar::<_, Option<f64>>(
        "SELECT extract(epoch FROM leased_at)::float8 FROM replication_jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await
    .expect("load leased_at")
}

async fn version_state(pool: &PgPool, version_id: Uuid) -> String {
    sqlx::query_scalar::<_, String>("SELECT replication_state FROM file_versions WHERE id = $1")
        .bind(version_id)
        .fetch_one(pool)
        .await
        .expect("load version replication state")
}

/// Delete every fixture row this suite creates, for one tenant.
async fn cleanup_fixture(ctx: &TestContext, target_ids: &[Uuid]) {
    for target_id in target_ids {
        sqlx::query("DELETE FROM replication_targets WHERE id = $1")
            .bind(target_id)
            .execute(&ctx.pool)
            .await
            .ok();
    }
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

/// Panic-path cleanup for the seeded replication target (global, no tenant
/// column) and the tenant rows.
struct FixtureCleanupGuard {
    database_url: String,
    tenant_id: Uuid,
    target_ids: Vec<Uuid>,
    armed: bool,
}

impl FixtureCleanupGuard {
    fn new(ctx: &TestContext) -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://rustshare:changeme@localhost:5432/rustshare".to_string()
            }),
            tenant_id: ctx.tenant_id,
            target_ids: Vec::new(),
            armed: true,
        }
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
        let target_ids = std::mem::take(&mut self.target_ids);
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
                        for target_id in &target_ids {
                            sqlx::query("DELETE FROM replication_targets WHERE id = $1")
                                .bind(target_id)
                                .execute(&pool)
                                .await
                                .ok();
                        }
                        cleanup_tenant(&pool, tenant_id).await;
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
// A. crash-after-lease: expired syncing job is reclaimed with a fresh token
// ---------------------------------------------------------------------------

/// **Invariant A** — a job leased by a worker that then crashed (lease
/// expired) is picked up by a plain `lease_replication_jobs` call. The row
/// returns `syncing` with a fresh token (B != A — recovery rotates the
/// per-job token) and a monotonically incremented `attempt_count`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn a_crash_after_lease_reclaims_with_fresh_token() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    let owner = ctx.create_test_user("a_lease_recovery_owner").await;
    let tag = Uuid::new_v4();
    let (file_id, version_id, storage_key) = seed_file_version(&ctx, owner.id, tag).await;
    let job = create_queued_job(&ctx, file_id, version_id, &storage_key).await;

    let first = lease_due_jobs(&ctx).await;
    let first_job = find_job(&first, job.id).expect("fresh job must be leaseable");
    let token_a = first_job
        .lease_token
        .expect("a leased job carries its fencing token");
    assert_eq!(first_job.status, ReplicationJobStatus::Syncing);
    assert_eq!(first_job.attempt_count, 1);

    force_lease_expiry(&ctx.pool, job.id).await;

    let second = lease_due_jobs(&ctx).await;
    let second_job = find_job(&second, job.id).expect("expired syncing job must be reclaimed");
    let token_b = second_job
        .lease_token
        .expect("a reclaimed job carries its fencing token");
    assert_ne!(
        token_b, token_a,
        "recovery must rotate the lease token so the stale worker's fencing token dies"
    );
    assert_eq!(second_job.status, ReplicationJobStatus::Syncing);
    assert_eq!(second_job.attempt_count, 2);

    let stored = ctx
        .metadata_store
        .get_replication_job(job.id)
        .await
        .expect("load job")
        .expect("job must exist");
    assert_eq!(stored.status, ReplicationJobStatus::Syncing);
    assert_eq!(stored.lease_token, Some(token_b));

    cleanup_fixture(&ctx, &[]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// B. stale completion rejected after reclaim
// ---------------------------------------------------------------------------

/// **Invariant B** — after a reclaim, a completion attempt carrying the
/// obsolete token is a lease conflict: `mark_replication_job_completed`
/// returns `Ok(false)`, the job stays `syncing` under the new token, and only
/// the current token completes it (`Ok(true)` → `completed`, lease cleared).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn b_stale_completion_rejected_after_reclaim() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    let owner = ctx.create_test_user("b_stale_completion_owner").await;
    let tag = Uuid::new_v4();
    let (file_id, version_id, storage_key) = seed_file_version(&ctx, owner.id, tag).await;
    let job = create_queued_job(&ctx, file_id, version_id, &storage_key).await;

    let first = lease_due_jobs(&ctx).await;
    let token_a = find_job(&first, job.id).unwrap().lease_token.unwrap();
    force_lease_expiry(&ctx.pool, job.id).await;

    let second = lease_due_jobs(&ctx).await;
    let token_b = find_job(&second, job.id).unwrap().lease_token.unwrap();
    assert_ne!(token_b, token_a);

    let stale = ctx
        .metadata_store
        .mark_replication_job_completed(job.id, token_a)
        .await
        .expect("stale completion query must succeed");
    assert!(!stale, "a stale token must not complete a reclaimed job");
    let (status, token) = job_status_and_token(&ctx.pool, job.id).await;
    assert_eq!(
        status, "syncing",
        "the job must stay syncing after a lease conflict"
    );
    assert_eq!(token, Some(token_b));

    let current = ctx
        .metadata_store
        .mark_replication_job_completed(job.id, token_b)
        .await
        .expect("current completion query must succeed");
    assert!(current, "the current token must complete the job");
    let (status, token) = job_status_and_token(&ctx.pool, job.id).await;
    assert_eq!(status, "completed");
    assert_eq!(token, None, "completion must clear the lease");

    cleanup_fixture(&ctx, &[]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// C. stale retrying / failed transitions rejected after reclaim
// ---------------------------------------------------------------------------

/// **Invariant C** — the stale-token rejection holds for the retrying and
/// terminal-failure transitions as well: both return `Ok(false)` and change
/// nothing while the job belongs to the new token; each transition succeeds
/// with the token that actually owns the lease.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn c_stale_retrying_and_failed_rejected_after_reclaim() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    let owner = ctx.create_test_user("c_stale_transitions_owner").await;
    let tag = Uuid::new_v4();
    let (file_id, version_id, storage_key) = seed_file_version(&ctx, owner.id, tag).await;
    let job = create_queued_job(&ctx, file_id, version_id, &storage_key).await;

    let first = lease_due_jobs(&ctx).await;
    let token_a = find_job(&first, job.id).unwrap().lease_token.unwrap();
    force_lease_expiry(&ctx.pool, job.id).await;

    let second = lease_due_jobs(&ctx).await;
    let token_b = find_job(&second, job.id).unwrap().lease_token.unwrap();

    let retry_at = Utc::now() + chrono::Duration::seconds(60);
    assert!(!ctx
        .metadata_store
        .mark_replication_job_retrying(job.id, token_a, "stale boom", retry_at)
        .await
        .expect("stale retrying query must succeed"));
    assert!(!ctx
        .metadata_store
        .mark_replication_job_failed(job.id, token_a, "stale boom")
        .await
        .expect("stale failed query must succeed"));
    let (status, token) = job_status_and_token(&ctx.pool, job.id).await;
    assert_eq!(status, "syncing");
    assert_eq!(token, Some(token_b));

    // The owning token schedules the retry; the job becomes leaseable again
    // immediately (next_attempt_at due now).
    let due_now = Utc::now();
    assert!(ctx
        .metadata_store
        .mark_replication_job_retrying(job.id, token_b, "real boom", due_now)
        .await
        .expect("retrying with the current token must succeed"));
    let (status, token) = job_status_and_token(&ctx.pool, job.id).await;
    assert_eq!(status, "retrying");
    assert_eq!(token, None);

    // Re-claim the retrying job; the obsolete token B now fails as well.
    let third = lease_due_jobs(&ctx).await;
    let token_c = find_job(&third, job.id).unwrap().lease_token.unwrap();
    assert_ne!(token_c, token_b);
    assert!(!ctx
        .metadata_store
        .mark_replication_job_failed(job.id, token_b, "obsolete boom")
        .await
        .expect("obsolete failed query must succeed"));
    assert!(ctx
        .metadata_store
        .mark_replication_job_failed(job.id, token_c, "real boom")
        .await
        .expect("failed with the current token must succeed"));
    let (status, token) = job_status_and_token(&ctx.pool, job.id).await;
    assert_eq!(status, "failed");
    assert_eq!(token, None);

    cleanup_fixture(&ctx, &[]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// D. concurrent reclaim: exactly one leaser wins
// ---------------------------------------------------------------------------

/// **Invariant D** — an expired lease is reclaimed exactly once under
/// sequential leasers: the post-claim predicate excludes the freshly
/// reclaimed row, so a second immediate lease attempt gets nothing. True
/// mutual exclusion between concurrent leasers is provided by
/// `FOR UPDATE SKIP LOCKED` in the lease query; this test only pins the
/// sequential half.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn d_expired_lease_reclaimed_exactly_once() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    let owner = ctx.create_test_user("d_single_reclaim_owner").await;
    let tag = Uuid::new_v4();
    let (file_id, version_id, storage_key) = seed_file_version(&ctx, owner.id, tag).await;
    let job = create_queued_job(&ctx, file_id, version_id, &storage_key).await;

    let first = lease_due_jobs(&ctx).await;
    assert!(find_job(&first, job.id).is_some());
    force_lease_expiry(&ctx.pool, job.id).await;

    let second = lease_due_jobs(&ctx).await;
    let reclaimed = find_job(&second, job.id).expect("the first leaser must reclaim the job");
    let token_b = reclaimed.lease_token.unwrap();

    let third = lease_due_jobs(&ctx).await;
    assert!(
        find_job(&third, job.id).is_none(),
        "a second leaser must not get the freshly reclaimed job"
    );
    let (status, token) = job_status_and_token(&ctx.pool, job.id).await;
    assert_eq!(status, "syncing");
    assert_eq!(token, Some(token_b));

    cleanup_fixture(&ctx, &[]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// E. a healthy lease is never stolen
// ---------------------------------------------------------------------------

/// **Invariant E** — a `syncing` job whose lease has not expired is not
/// returned by the leaser at all, so a second worker cannot steal in-flight
/// work.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn e_healthy_lease_not_stolen() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    let owner = ctx.create_test_user("e_healthy_lease_owner").await;
    let tag = Uuid::new_v4();
    let (file_id, version_id, storage_key) = seed_file_version(&ctx, owner.id, tag).await;
    let job = create_queued_job(&ctx, file_id, version_id, &storage_key).await;

    let first = lease_due_jobs(&ctx).await;
    let token_a = find_job(&first, job.id).unwrap().lease_token.unwrap();

    let second = lease_due_jobs(&ctx).await;
    assert!(
        find_job(&second, job.id).is_none(),
        "an unexpired syncing job must not be stealable"
    );
    let (status, token) = job_status_and_token(&ctx.pool, job.id).await;
    assert_eq!(status, "syncing");
    assert_eq!(token, Some(token_a));

    cleanup_fixture(&ctx, &[]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// F. renewal refreshes the lease and detects lost leases
// ---------------------------------------------------------------------------

/// **Invariant F** — `renew_replication_job_lease` with the current token
/// bumps `leased_at`, so even a job whose lease already aged out (a late
/// batch peer) becomes un-stealable again; a stale token returns `Ok(false)`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn f_renewal_extends_lease_and_detects_lost_lease() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    let owner = ctx.create_test_user("f_renewal_owner").await;
    let tag = Uuid::new_v4();
    let (file_id, version_id, storage_key) = seed_file_version(&ctx, owner.id, tag).await;
    let job = create_queued_job(&ctx, file_id, version_id, &storage_key).await;

    let first = lease_due_jobs(&ctx).await;
    let token_a = find_job(&first, job.id).unwrap().lease_token.unwrap();

    // The batch waited: this job's lease is already expired by the time the
    // worker reaches it. Renewing must still succeed (token + status match)
    // and push leased_at forward so the job cannot be reclaimed mid-batch.
    force_lease_expiry(&ctx.pool, job.id).await;
    let before = leased_at_epoch(&ctx.pool, job.id)
        .await
        .expect("forced lease must still have a leased_at");

    assert!(ctx
        .metadata_store
        .renew_replication_job_lease(job.id, token_a)
        .await
        .expect("renewal query must succeed"));
    let after = leased_at_epoch(&ctx.pool, job.id)
        .await
        .expect("renewed lease must have a leased_at");
    assert!(
        after > before,
        "renewal must move leased_at forward (before={before}, after={after})"
    );

    let second = lease_due_jobs(&ctx).await;
    assert!(
        find_job(&second, job.id).is_none(),
        "a renewed job must not be reclaimable by a second worker"
    );

    // A stale token cannot renew: the lease belongs to the current holder.
    assert!(!ctx
        .metadata_store
        .renew_replication_job_lease(job.id, Uuid::new_v4())
        .await
        .expect("stale renewal query must succeed"));
    let (status, token) = job_status_and_token(&ctx.pool, job.id).await;
    assert_eq!(status, "syncing");
    assert_eq!(token, Some(token_a));

    cleanup_fixture(&ctx, &[]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// G. reclaim is selective (worker-restart recovery + malformed rows)
// ---------------------------------------------------------------------------

/// **Invariant G** — worker-restart recovery needs nothing but the lease
/// call: an expired `syncing` job is reclaimed while (a) a healthy `syncing`
/// peer is left alone and (b) a malformed `syncing` row with NULL `leased_at`
/// — unknown lease age, expiry unprovable — is deliberately NOT eligible and
/// is left untouched for manual/observability follow-up.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn g_expired_syncing_reclaimed_selectively() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    let owner = ctx.create_test_user("g_selective_reclaim_owner").await;
    let tag = Uuid::new_v4();

    // The expired job: crashed worker, nothing else to do but lease again.
    let (file_a, version_a, key_a) = seed_file_version(&ctx, owner.id, tag).await;
    let job_a = create_queued_job(&ctx, file_a, version_a, &key_a).await;

    // The malformed job: syncing with NULL leased_at (legacy/corrupt row).
    let (file_m, version_m, key_m) = seed_file_version(&ctx, owner.id, Uuid::new_v4()).await;
    let job_m = create_queued_job(&ctx, file_m, version_m, &key_m).await;

    // The healthy job: syncing, lease still fresh.
    let (file_h, version_h, key_h) = seed_file_version(&ctx, owner.id, Uuid::new_v4()).await;
    let job_h = create_queued_job(&ctx, file_h, version_h, &key_h).await;

    let first = lease_due_jobs(&ctx).await;
    let token_a1 = find_job(&first, job_a.id).unwrap().lease_token.unwrap();
    let token_m = find_job(&first, job_m.id).unwrap().lease_token.unwrap();
    find_job(&first, job_h.id).expect("healthy job must be leased too");

    force_lease_expiry(&ctx.pool, job_a.id).await;
    // Simulate the malformed legacy row: syncing but with no lease timestamp.
    sqlx::query("UPDATE replication_jobs SET leased_at = NULL WHERE id = $1")
        .bind(job_m.id)
        .execute(&ctx.pool)
        .await
        .expect("null out malformed job lease");

    let second = lease_due_jobs(&ctx).await;
    let reclaimed_a = find_job(&second, job_a.id)
        .expect("the expired syncing job must be reclaimed with no other action");
    let token_a2 = reclaimed_a.lease_token.unwrap();
    assert_ne!(token_a2, token_a1, "reclaim must rotate the token");
    assert_eq!(reclaimed_a.attempt_count, 2);
    assert!(
        find_job(&second, job_h.id).is_none(),
        "the healthy syncing peer must not be reclaimed"
    );
    assert!(
        find_job(&second, job_m.id).is_none(),
        "a syncing row with NULL leased_at must never be reclaimed"
    );

    // The malformed row is byte-for-byte untouched.
    let (status_m, token_m_after) = job_status_and_token(&ctx.pool, job_m.id).await;
    assert_eq!(status_m, "syncing");
    assert_eq!(token_m_after, Some(token_m));
    assert_eq!(
        leased_at_epoch(&ctx.pool, job_m.id).await,
        None,
        "the malformed row must keep its NULL leased_at"
    );

    cleanup_fixture(&ctx, &[]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// H. retry exhaustion does not block crash recovery
// ---------------------------------------------------------------------------

/// **Invariant H** — a stale `syncing` job that already reached `max_attempts`
/// is still reclaimable: recovery must not evade the retry gate, the gate
/// must simply see the incremented count. `attempt_count` is monotonic
/// (every claim, including recovery claims, increments it), so the worker's
/// post-claim `attempt_count < max_attempts` check routes such a job to
/// `failed`. Every post-claim path is terminal — exhausted jobs fail, and a
/// version-guard conflict completes the job via the fenced mark (see J) — so
/// no recovered job can loop. Store-level assertions only; the worker tick
/// has no testable seam.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn h_recovery_lease_counts_toward_retry_exhaustion() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    const MAX_ATTEMPTS: i32 = 8;

    let owner = ctx.create_test_user("h_retry_exhaustion_owner").await;
    let tag = Uuid::new_v4();
    let (file_id, version_id, storage_key) = seed_file_version(&ctx, owner.id, tag).await;
    let job = create_queued_job(&ctx, file_id, version_id, &storage_key).await;

    let first = lease_due_jobs(&ctx).await;
    assert!(find_job(&first, job.id).is_some());

    // Simulate a job that has burned all its attempts and then stalled.
    sqlx::query("UPDATE replication_jobs SET attempt_count = $2 WHERE id = $1")
        .bind(job.id)
        .bind(MAX_ATTEMPTS)
        .execute(&ctx.pool)
        .await
        .expect("set attempt count to max");
    force_lease_expiry(&ctx.pool, job.id).await;

    let second = lease_due_jobs(&ctx).await;
    let reclaimed = find_job(&second, job.id).expect(
        "a stale syncing job must be reclaimable even at max_attempts — recovery must not \
         evade the retry gate",
    );
    assert_eq!(
        reclaimed.attempt_count,
        MAX_ATTEMPTS + 1,
        "every claim (recovery included) increments attempt_count monotonically"
    );
    assert_eq!(reclaimed.status, ReplicationJobStatus::Syncing);
    assert!(
        reclaimed.lease_token.is_some(),
        "the recovered job carries a fresh fencing token"
    );

    cleanup_fixture(&ctx, &[]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// J. a stale job cannot clobber a fully replicated version
// ---------------------------------------------------------------------------

/// **Invariant J** — with two jobs pointing at one version, the first job is
/// reclaimed after its worker crashed and completes: the version becomes
/// `fully_replicated`. The second job's guarded version transition to
/// `failed` is then rejected (`Ok(false)`) and the version stays
/// `fully_replicated`. A worker hitting that rejection completes its own job
/// terminally through the fenced mark (the rejection proves the version is
/// already replicated); job2's lease is still valid here, so the completion
/// succeeds. This pins the store-level half of the worker's guard-conflict
/// path so the job cannot livelock reclaim → renew → guard-reject with a
/// growing `attempt_count`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn j_stale_job_cannot_clobber_fully_replicated_version() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    let owner = ctx.create_test_user("j_version_guard_owner").await;
    let tag = Uuid::new_v4();
    let (file_id, version_id, storage_key) = seed_file_version(&ctx, owner.id, tag).await;
    let job1 = create_queued_job(&ctx, file_id, version_id, &storage_key).await;
    let job2 = create_queued_job(&ctx, file_id, version_id, &storage_key).await;

    let first = lease_due_jobs(&ctx).await;
    assert!(find_job(&first, job1.id).is_some());
    let token_a2 = find_job(&first, job2.id)
        .expect("job2 must be leased")
        .lease_token
        .unwrap();

    // Worker proceeds on job1: the version moves to syncing (guard accepts
    // every not-yet-replicated state).
    assert!(ctx
        .metadata_store
        .update_file_version_replication_state_if_current(
            version_id,
            &[
                ReplicationState::Queued,
                ReplicationState::PrimaryWritten,
                ReplicationState::Syncing,
                ReplicationState::Degraded,
                ReplicationState::Failed,
            ],
            ReplicationState::Syncing,
        )
        .await
        .expect("version syncing transition must succeed"));
    assert_eq!(version_state(&ctx.pool, version_id).await, "syncing");

    // Job1's worker dies; a replacement reclaims just that job.
    force_lease_expiry(&ctx.pool, job1.id).await;
    let second = lease_due_jobs(&ctx).await;
    let reclaimed = find_job(&second, job1.id).expect("job1 must be reclaimable");
    let token_b = reclaimed.lease_token.unwrap();
    assert!(
        find_job(&second, job2.id).is_none(),
        "job2's healthy lease must survive job1's reclaim"
    );

    // The recovered job1 completes: job status and version state fence.
    assert!(ctx
        .metadata_store
        .mark_replication_job_completed(job1.id, token_b)
        .await
        .expect("completion with the recovery token must succeed"));
    assert!(ctx
        .metadata_store
        .update_file_version_replication_state_if_current(
            version_id,
            &[ReplicationState::Syncing],
            ReplicationState::FullyReplicated,
        )
        .await
        .expect("version completion transition must succeed"));
    assert_eq!(
        version_state(&ctx.pool, version_id).await,
        "fully_replicated"
    );

    // The stale job2 can no longer damage the version: the Syncing-only guard
    // no longer matches and the write is skipped.
    assert!(!ctx
        .metadata_store
        .update_file_version_replication_state_if_current(
            version_id,
            &[ReplicationState::Syncing],
            ReplicationState::Failed,
        )
        .await
        .expect("stale version transition query must succeed"));
    assert_eq!(
        version_state(&ctx.pool, version_id).await,
        "fully_replicated",
        "a stale job must not clobber the fully replicated version"
    );

    // Worker guard-conflict path: the rejection proves the version is already
    // replicated, so the worker finishes ITS OWN job terminally through the
    // fenced mark instead of cycling reclaim -> renew -> guard-reject forever.
    // job2 still legitimately holds token_a2, so the fenced completion must
    // succeed and must not touch the version state.
    assert!(ctx
        .metadata_store
        .mark_replication_job_completed(job2.id, token_a2)
        .await
        .expect("fenced completion after a version guard conflict must succeed"));
    let (status2, token2) = job_status_and_token(&ctx.pool, job2.id).await;
    assert_eq!(status2, "completed");
    assert_eq!(token2, None, "terminal completion must clear the lease");
    assert_eq!(
        version_state(&ctx.pool, version_id).await,
        "fully_replicated",
        "terminal completion after a guard conflict must not clobber the version"
    );

    cleanup_fixture(&ctx, &[]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// K. crash during remote completion: stale writes rejected, recovery succeeds
// ---------------------------------------------------------------------------

/// **Invariant K** — a worker that renews, starts the remote copy (an attempt
/// record is durable) and then crashes leaves the job reclaimable. Its
/// post-copy fenced writes with the stale token are all rejected; the
/// recovery lease completes the job and the version normally. (Attempt rows
/// are intentionally unfenced observability records — their presence must not
/// gate the fenced job/version transitions.)
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn k_crash_during_remote_completion_rejected_then_recovery_succeeds() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    let owner = ctx.create_test_user("k_crash_mid_copy_owner").await;
    let tag = Uuid::new_v4();
    let (file_id, version_id, storage_key) = seed_file_version(&ctx, owner.id, tag).await;
    let job = create_queued_job(&ctx, file_id, version_id, &storage_key).await;
    let target_id = seed_enabled_replication_target(&ctx.pool).await;
    cleanup_guard.track_target(target_id);

    let first = lease_due_jobs(&ctx).await;
    let job_row = find_job(&first, job.id).expect("job must be leaseable");
    let token_a = job_row.lease_token.unwrap();

    // The worker confirms ownership, starts the copy and records the attempt;
    // then it dies before completing the job.
    assert!(ctx
        .metadata_store
        .renew_replication_job_lease(job.id, token_a)
        .await
        .expect("renewal must succeed"));
    assert!(ctx
        .metadata_store
        .update_file_version_replication_state_if_current(
            version_id,
            &[
                ReplicationState::Queued,
                ReplicationState::PrimaryWritten,
                ReplicationState::Syncing,
                ReplicationState::Degraded,
                ReplicationState::Failed,
            ],
            ReplicationState::Syncing,
        )
        .await
        .expect("version syncing transition must succeed"));
    let started_at = Utc::now();
    ctx.metadata_store
        .create_replication_attempt(ReplicationAttemptRecord {
            job_id: job.id,
            target_id,
            attempt_number: job_row.attempt_count,
            status: "completed",
            error_message: None,
            started_at,
            completed_at: started_at,
        })
        .await
        .expect("record pre-crash attempt");

    // Crash: lease expires, a replacement worker reclaims the job.
    force_lease_expiry(&ctx.pool, job.id).await;
    let second = lease_due_jobs(&ctx).await;
    let token_b = find_job(&second, job.id)
        .expect("crash recovery must reclaim the job")
        .lease_token
        .unwrap();
    assert_ne!(token_b, token_a);

    // The dead worker's fenced writes are all rejected.
    assert!(!ctx
        .metadata_store
        .mark_replication_job_completed(job.id, token_a)
        .await
        .expect("stale completion query must succeed"));
    assert!(!ctx
        .metadata_store
        .mark_replication_job_retrying(job.id, token_a, "late completion", Utc::now(),)
        .await
        .expect("stale retrying query must succeed"));
    assert!(!ctx
        .metadata_store
        .mark_replication_job_failed(job.id, token_a, "late completion")
        .await
        .expect("stale failed query must succeed"));
    let (status, token) = job_status_and_token(&ctx.pool, job.id).await;
    assert_eq!(status, "syncing");
    assert_eq!(token, Some(token_b));

    // The recovery worker records its own attempt and completes normally.
    let recovered = ctx
        .metadata_store
        .get_replication_job(job.id)
        .await
        .expect("load job")
        .expect("job must exist");
    let completed_at = Utc::now();
    ctx.metadata_store
        .create_replication_attempt(ReplicationAttemptRecord {
            job_id: job.id,
            target_id,
            attempt_number: recovered.attempt_count,
            status: "completed",
            error_message: None,
            started_at: completed_at,
            completed_at,
        })
        .await
        .expect("record recovery attempt");
    assert!(ctx
        .metadata_store
        .mark_replication_job_completed(job.id, token_b)
        .await
        .expect("completion with the recovery token must succeed"));
    assert!(ctx
        .metadata_store
        .update_file_version_replication_state_if_current(
            version_id,
            &[ReplicationState::Syncing],
            ReplicationState::FullyReplicated,
        )
        .await
        .expect("version completion transition must succeed"));

    let (status, token) = job_status_and_token(&ctx.pool, job.id).await;
    assert_eq!(status, "completed");
    assert_eq!(token, None);
    assert_eq!(
        version_state(&ctx.pool, version_id).await,
        "fully_replicated"
    );
    let attempt_count: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM replication_attempts WHERE job_id = $1")
            .bind(job.id)
            .fetch_one(&ctx.pool)
            .await
            .expect("count attempts");
    assert_eq!(
        attempt_count, 2,
        "the pre-crash attempt and the recovery attempt must both be durable"
    );

    cleanup_fixture(&ctx, &[target_id]).await;
    cleanup_guard.disarm();
}

// ---------------------------------------------------------------------------
// L. renewal keeps a late-batch job from being stolen
// ---------------------------------------------------------------------------

/// **Invariant L** — batch freshness: two jobs leased in one call, the first
/// already aged past the timeout while the second is processed. Renewing the
/// late job before working it moves `leased_at` forward, so a competing
/// worker's lease attempt cannot steal it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn l_batch_renewal_keeps_late_batch_job_unstealable() {
    let _guard = SERIAL.lock().await;
    assert_disposable_environment();
    let ctx = setup_test_env().await;
    let mut cleanup_guard = FixtureCleanupGuard::new(&ctx);

    let owner = ctx.create_test_user("l_batch_renewal_owner").await;
    let tag = Uuid::new_v4();
    let (file1, version1, key1) = seed_file_version(&ctx, owner.id, tag).await;
    let job1 = create_queued_job(&ctx, file1, version1, &key1).await;
    let (file2, version2, key2) = seed_file_version(&ctx, owner.id, Uuid::new_v4()).await;
    let job2 = create_queued_job(&ctx, file2, version2, &key2).await;

    let first = lease_due_jobs(&ctx).await;
    let token1 = find_job(&first, job1.id).unwrap().lease_token.unwrap();
    find_job(&first, job2.id).expect("both batch peers must be leased");

    // Job2 is processed first; by the time the worker reaches job1 its lease
    // has aged out. The worker renews before processing.
    force_lease_expiry(&ctx.pool, job1.id).await;
    let before = leased_at_epoch(&ctx.pool, job1.id)
        .await
        .expect("forced lease must still have a leased_at");

    assert!(ctx
        .metadata_store
        .renew_replication_job_lease(job1.id, token1)
        .await
        .expect("renewal query must succeed"));
    let after = leased_at_epoch(&ctx.pool, job1.id)
        .await
        .expect("renewed lease must have a leased_at");
    assert!(
        after > before,
        "renewal must move leased_at forward (before={before}, after={after})"
    );

    // A second worker sweeping now gets nothing: job1 was just renewed and
    // job2's lease is still healthy.
    let second = lease_due_jobs(&ctx).await;
    assert!(
        find_job(&second, job1.id).is_none(),
        "the renewed late-batch job must not be stealable"
    );
    assert!(find_job(&second, job2.id).is_none());

    let (status, token) = job_status_and_token(&ctx.pool, job1.id).await;
    assert_eq!(status, "syncing");
    assert_eq!(token, Some(token1));

    cleanup_fixture(&ctx, &[]).await;
    cleanup_guard.disarm();
}
