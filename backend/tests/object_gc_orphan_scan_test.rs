//! Read-only orphan-discovery dry-run scanner contract tests.
//!
//! These tests pin the Phase 3.4A discovery surface: classification of listed
//! blob keys against the global reference summary and the durable GC queue,
//! with no deletion and no enqueueing. Object-store listing is exercised
//! against real RustFS in the tests that need it; everything else uses a
//! deterministic fake lister so the scenarios stay hermetic.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use chrono::Utc;
use rustshare_server::object_gc_scan::{
    scan_orphan_blobs_dry_run, BlobListPage, BlobLister, Classification, KeyScanOutcome,
    OrphanScanConfig, OrphanScanReport, ScanError,
};
use rustshare_storage::{MetadataStore, ObjectStore, ObjectStoreOptions};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
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
    sqlx::query("DELETE FROM object_gc_queue WHERE reason LIKE 'object_gc_orphan_scan_test:%'")
        .execute(&pool)
        .await
        .expect("clean prior test candidates");

    let endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .unwrap_or_else(|_| "http://localhost:9000".to_string());
    let region = std::env::var("S3_REGION")
        .or_else(|_| std::env::var("RUSTFS_REGION"))
        .unwrap_or_else(|_| "us-east-1".to_string());
    let bucket = std::env::var("S3_BUCKET")
        .or_else(|_| std::env::var("RUSTFS_BUCKET"))
        .unwrap_or_else(|_| "rustshare-test".to_string());
    let object_store = ObjectStore::new_with_options(
        endpoint,
        region,
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

fn digest(key: &str) -> &str {
    key.strip_prefix("blobs/").expect("blob prefix")
}

fn config(max_keys: u32, min_age: Duration) -> OrphanScanConfig {
    OrphanScanConfig::new(max_keys, min_age).expect("valid scan config")
}

fn outcomes_by_key(report: &OrphanScanReport) -> BTreeMap<String, Classification> {
    report
        .outcomes
        .iter()
        .map(|outcome| (outcome.key.clone(), outcome.classification))
        .collect()
}

/// A lister that serves canned pages selected by opaque continuation tokens,
/// records the tokens it was called with, and can fail one page on demand.
/// With `empty_tokened_pages` it instead models a churning bucket: every page
/// is empty but always carries a continuation token.
struct FakeLister {
    pages: Vec<Vec<String>>,
    fail_on_page: Option<usize>,
    empty_tokened_pages: bool,
    calls: Arc<Mutex<Vec<Option<String>>>>,
}

impl FakeLister {
    fn new(pages: Vec<Vec<String>>) -> Self {
        Self {
            pages,
            fail_on_page: None,
            empty_tokened_pages: false,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn fail_on_page(mut self, page: usize) -> Self {
        self.fail_on_page = Some(page);
        self
    }

    fn with_empty_tokened_pages(mut self) -> Self {
        self.empty_tokened_pages = true;
        self
    }

    fn called_tokens(&self) -> Vec<Option<String>> {
        self.calls.lock().expect("calls lock").clone()
    }

    fn page_index(token: Option<&str>) -> usize {
        match token {
            None => 0,
            Some(token) => token
                .strip_prefix("page-")
                .and_then(|index| index.parse::<usize>().ok())
                .expect("unknown continuation token"),
        }
    }
}

impl BlobLister for FakeLister {
    fn list_page<'a>(
        &'a self,
        continuation: Option<&'a str>,
        max_keys: u32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BlobListPage, ScanError>> + Send + 'a>,
    > {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("calls lock")
                .push(continuation.map(str::to_string));
            let index = Self::page_index(continuation);
            if self.fail_on_page == Some(index) {
                return Err(ScanError::List("synthetic list failure".to_string()));
            }
            if self.empty_tokened_pages {
                return Ok(BlobListPage {
                    blobs: Vec::new(),
                    next_continuation: Some(format!("page-{}", index + 1)),
                });
            }
            let keys: Vec<String> = self
                .pages
                .get(index)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .take(max_keys as usize)
                .collect();
            let last_modified = Utc::now() - chrono::Duration::hours(48);
            Ok(BlobListPage {
                blobs: keys
                    .into_iter()
                    .map(|key| rustshare_server::object_gc_scan::ListedBlob { key, last_modified })
                    .collect(),
                next_continuation: (index + 1 < self.pages.len())
                    .then(|| format!("page-{}", index + 1)),
            })
        })
    }
}

fn single_key_lister(key: &str) -> FakeLister {
    FakeLister::new(vec![vec![key.to_string()]])
}

/// Page through a real-bucket scan until `key` has been classified. The
/// disposable bucket is shared with other suites, so a single bounded
/// invocation may not reach our key.
async fn scan_finding_key(
    lister: &dyn BlobLister,
    metadata: &MetadataStore,
    config: &OrphanScanConfig,
    key: &str,
) -> Classification {
    let mut token = None;
    for _ in 0..20 {
        let report = scan_orphan_blobs_dry_run(lister, metadata, config, token.as_deref())
            .await
            .expect("dry-run scan");
        if let Some(outcome) = report.outcomes.iter().find(|outcome| outcome.key == key) {
            return outcome.classification;
        }
        match report.next_continuation_token {
            Some(next) => token = Some(next),
            None => panic!("key {key} was never classified after a full bucket scan"),
        }
    }
    panic!("key {key} was not classified within 20 scan invocations");
}

/// Page through every listed key and concatenate the per-key outcomes.
async fn scan_all(
    lister: &dyn BlobLister,
    metadata: &MetadataStore,
    config: &OrphanScanConfig,
) -> Vec<KeyScanOutcome> {
    let mut outcomes = Vec::new();
    let mut token = None;
    loop {
        let report = scan_orphan_blobs_dry_run(lister, metadata, config, token.as_deref())
            .await
            .expect("dry-run scan");
        outcomes.extend(report.outcomes);
        match report.next_continuation_token {
            Some(next) => token = Some(next),
            None => return outcomes,
        }
    }
}

async fn insert_user(pool: &PgPool, suffix: &str) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, storage_quota) VALUES ($1, $2, $3, 'test', 'Orphan Scan', 1000000)",
    )
    .bind(user_id)
    .bind(format!("orphan-scan-{suffix}-{user_id}"))
    .bind(format!("orphan-scan-{suffix}-{user_id}@test.local"))
    .execute(pool)
    .await
    .expect("create user");
    user_id
}

async fn insert_file_reference(pool: &PgPool, user_id: Uuid, key: &str, size: i64) -> Uuid {
    let file_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id) VALUES ($1, 'scan.bin', $2, $3, 'application/octet-stream', $4, $5, $6)",
    )
    .bind(file_id)
    .bind(format!("/orphan-scan-{}.bin", Uuid::new_v4()))
    .bind(size)
    .bind(digest(key))
    .bind(key)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("create file reference");
    file_id
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn a_never_referenced_blob_classifies_potential_orphan_via_real_listing() {
    let env = setup().await;
    let content = format!("orphan-scan-a-{}", Uuid::new_v4()).into_bytes();
    let key = blob(&content);
    env.object_store
        .put(&key, content.into())
        .await
        .expect("put orphan blob");

    // min_object_age of zero: the freshly written blob is old enough to be
    // considered (age is a consideration floor, not a safety proof).
    let classification = scan_finding_key(
        env.object_store.as_ref() as &dyn BlobLister,
        &env.metadata,
        &config(1000, Duration::ZERO),
        &key,
    )
    .await;
    assert_eq!(
        classification,
        Classification::PotentialOrphan,
        "a listed, never-referenced, age-eligible blob must be a potential orphan"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn b_file_reference_classifies_referenced() {
    let env = setup().await;
    let key = blob(format!("orphan-scan-b-{}", Uuid::new_v4()).as_bytes());
    let user_id = insert_user(&env.pool, "b").await;
    insert_file_reference(&env.pool, user_id, &key, 1).await;

    let report = scan_orphan_blobs_dry_run(
        &single_key_lister(&key),
        &env.metadata,
        &config(10, Duration::ZERO),
        None,
    )
    .await
    .expect("dry-run scan");

    assert_eq!(
        outcomes_by_key(&report).get(&key),
        Some(&Classification::Referenced),
        "a blob referenced by a live file row must classify as referenced"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn c_retained_non_current_file_version_classifies_referenced() {
    let env = setup().await;
    let current_key = blob(format!("orphan-scan-c-current-{}", Uuid::new_v4()).as_bytes());
    let old_key = blob(format!("orphan-scan-c-old-{}", Uuid::new_v4()).as_bytes());
    let user_id = insert_user(&env.pool, "c").await;
    let file_id = insert_file_reference(&env.pool, user_id, &current_key, 1).await;
    let version_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO file_versions (id, file_id, version_number, content_hash, storage_key, size, created_by) VALUES ($1, $2, 2, $3, $4, 1, $5)",
    )
    .bind(version_id)
    .bind(file_id)
    .bind(digest(&old_key))
    .bind(&old_key)
    .bind(user_id)
    .execute(&env.pool)
    .await
    .expect("create retained non-current version reference");

    let report = scan_orphan_blobs_dry_run(
        &single_key_lister(&old_key),
        &env.metadata,
        &config(10, Duration::ZERO),
        None,
    )
    .await
    .expect("dry-run scan");

    assert_eq!(
        outcomes_by_key(&report).get(&old_key),
        Some(&Classification::Referenced),
        "a retained non-current file_version row must protect its blob"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn d_vault_and_mail_references_classify_referenced() {
    let env = setup().await;
    let user_id = insert_user(&env.pool, "d").await;

    let vault_key = blob(format!("orphan-scan-d-vault-{}", Uuid::new_v4()).as_bytes());
    let message_key = blob(format!("orphan-scan-d-message-{}", Uuid::new_v4()).as_bytes());
    let attachment_key = blob(format!("orphan-scan-d-attachment-{}", Uuid::new_v4()).as_bytes());

    let vault_id = Uuid::new_v4();
    sqlx::query("INSERT INTO vaults (id, owner_user_id, name, adapter) VALUES ($1, $2, 'scan vault', 'obsidian')")
        .bind(vault_id)
        .bind(user_id)
        .execute(&env.pool)
        .await
        .expect("create vault");
    sqlx::query("INSERT INTO vault_files (id, vault_id, relative_path, sha256, size, server_rev) VALUES ($1, $2, 'scan.bin', $3, 1, 1)")
        .bind(Uuid::new_v4())
        .bind(vault_id)
        .bind(digest(&vault_key))
        .execute(&env.pool)
        .await
        .expect("create vault reference");

    let message_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO mail_messages (id, owner_id, imported_by, blob_key) VALUES ($1, $2, $2, $3)",
    )
    .bind(message_id)
    .bind(user_id)
    .bind(&message_key)
    .execute(&env.pool)
    .await
    .expect("create mail message reference");
    sqlx::query(
        "INSERT INTO mail_attachments (message_id, filename, blob_key) VALUES ($1, 'scan.bin', $2)",
    )
    .bind(message_id)
    .bind(&attachment_key)
    .execute(&env.pool)
    .await
    .expect("create mail attachment reference");

    let keys = vec![
        vault_key.clone(),
        message_key.clone(),
        attachment_key.clone(),
    ];
    let lister = FakeLister::new(vec![keys.clone()]);
    let report =
        scan_orphan_blobs_dry_run(&lister, &env.metadata, &config(10, Duration::ZERO), None)
            .await
            .expect("dry-run scan");

    let outcomes = outcomes_by_key(&report);
    for key in keys {
        assert_eq!(
            outcomes.get(&key),
            Some(&Classification::Referenced),
            "vault and mail references must protect a blob"
        );
    }
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn e_replication_job_states_classify_per_contract() {
    let env = setup().await;
    let user_id = insert_user(&env.pool, "e").await;
    let current_key = blob(format!("orphan-scan-e-holder-{}", Uuid::new_v4()).as_bytes());
    let file_id = insert_file_reference(&env.pool, user_id, &current_key, 1).await;
    let version_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO file_versions (id, file_id, version_number, content_hash, storage_key, size, created_by) VALUES ($1, $2, 1, $3, $4, 1, $5)",
    )
    .bind(version_id)
    .bind(file_id)
    .bind(digest(&current_key))
    .bind(&current_key)
    .bind(user_id)
    .execute(&env.pool)
    .await
    .expect("create version");

    let queued_key = blob(format!("orphan-scan-e-queued-{}", Uuid::new_v4()).as_bytes());
    let completed_key = blob(format!("orphan-scan-e-completed-{}", Uuid::new_v4()).as_bytes());
    for (key, status) in [(&queued_key, "queued"), (&completed_key, "completed")] {
        sqlx::query(
            "INSERT INTO replication_jobs (id, file_id, file_version_id, storage_key, status) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::new_v4())
        .bind(file_id)
        .bind(version_id)
        .bind(key)
        .bind(status)
        .execute(&env.pool)
        .await
        .expect("create replication job");
    }

    let keys = vec![queued_key.clone(), completed_key.clone()];
    let lister = FakeLister::new(vec![keys]);
    let report =
        scan_orphan_blobs_dry_run(&lister, &env.metadata, &config(10, Duration::ZERO), None)
            .await
            .expect("dry-run scan");
    let outcomes = outcomes_by_key(&report);

    assert_eq!(
        outcomes.get(&queued_key),
        Some(&Classification::Referenced),
        "a queued replication job must protect its blob"
    );
    assert_eq!(
        outcomes.get(&completed_key),
        Some(&Classification::PotentialOrphan),
        "a completed replication job is not a durable reference"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn f_reference_from_another_tenant_still_counts() {
    let env = setup().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    let key = blob(format!("orphan-scan-f-{}", Uuid::new_v4()).as_bytes());

    for (user_id, tenant_id, suffix) in [(user_a, tenant_a, "a"), (user_b, tenant_b, "b")] {
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, display_name, storage_quota, tenant_id) VALUES ($1, $2, $3, 'test', 'Orphan Scan', 1000000, $4)",
        )
        .bind(user_id)
        .bind(format!("orphan-scan-f-{suffix}-{user_id}"))
        .bind(format!("orphan-scan-f-{suffix}-{user_id}@test.local"))
        .bind(tenant_id)
        .execute(&env.pool)
        .await
        .expect("create tenant user");
    }
    // The reference is owned by tenant B; the scan has no tenant predicate.
    sqlx::query(
        "INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id, tenant_id) VALUES ($1, 'scan.bin', $2, 1, 'application/octet-stream', $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(format!("/orphan-scan-f-{}.bin", Uuid::new_v4()))
    .bind(digest(&key))
    .bind(&key)
    .bind(user_b)
    .bind(tenant_b)
    .execute(&env.pool)
    .await
    .expect("create cross-tenant file reference");

    let report = scan_orphan_blobs_dry_run(
        &single_key_lister(&key),
        &env.metadata,
        &config(10, Duration::ZERO),
        None,
    )
    .await
    .expect("dry-run scan");

    assert_eq!(
        outcomes_by_key(&report).get(&key),
        Some(&Classification::Referenced),
        "reference counting is global; another tenant's row must protect the blob"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn g_younger_than_min_age_classifies_recent() {
    let env = setup().await;
    let content = format!("orphan-scan-g-{}", Uuid::new_v4()).into_bytes();
    let key = blob(&content);
    env.object_store
        .put(&key, content.into())
        .await
        .expect("put fresh blob");

    let classification = scan_finding_key(
        env.object_store.as_ref() as &dyn BlobLister,
        &env.metadata,
        &config(1000, Duration::from_secs(48 * 3600)),
        &key,
    )
    .await;
    assert_eq!(
        classification,
        Classification::Recent,
        "a blob younger than min_object_age must never be considered for orphanhood"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn h_queue_states_classify_queued_variants() {
    let env = setup().await;
    let states = [
        "pending",
        "retry",
        "processing",
        "referenced",
        "operator_hold",
    ];
    let mut keys = Vec::new();
    for state in states {
        let key = blob(format!("orphan-scan-h-{state}-{}", Uuid::new_v4()).as_bytes());
        env.metadata
            .enqueue_object_gc_candidate(&key, "object_gc_orphan_scan_test:h", 1)
            .await
            .expect("enqueue candidate");
        sqlx::query("UPDATE object_gc_queue SET state = $2 WHERE object_key = $1")
            .bind(&key)
            .bind(state)
            .execute(&env.pool)
            .await
            .expect("set queue state");
        keys.push((key, state));
    }

    let lister = FakeLister::new(vec![keys.iter().map(|(key, _)| key.clone()).collect()]);
    let report =
        scan_orphan_blobs_dry_run(&lister, &env.metadata, &config(10, Duration::ZERO), None)
            .await
            .expect("dry-run scan");
    let outcomes = outcomes_by_key(&report);

    for (key, state) in keys {
        let expected = match state {
            "pending" => Classification::QueuedPending,
            "retry" => Classification::QueuedRetry,
            "processing" => Classification::QueuedProcessing,
            "referenced" => Classification::QueuedReferenced,
            "operator_hold" => Classification::QueuedOperatorHold,
            other => panic!("unmapped state {other}"),
        };
        assert_eq!(
            outcomes.get(&key),
            Some(&expected),
            "state {state} must classify as its queued_<state> variant"
        );
    }
    assert_eq!(report.queued_pending_count, 1);
    assert_eq!(report.queued_retry_count, 1);
    assert_eq!(report.queued_processing_count, 1);
    assert_eq!(report.queued_referenced_count, 1);
    assert_eq!(report.queued_operator_hold_count, 1);
    assert_eq!(report.potential_orphan_count, 0);

    for (key, _) in outcomes {
        sqlx::query("DELETE FROM object_gc_queue WHERE object_key = $1")
            .bind(key)
            .execute(&env.pool)
            .await
            .expect("clean up seeded queue row");
    }
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn i_non_canonical_keys_are_excluded_not_orphans() {
    let env = setup().await;
    let legacy_upper = format!("blobs/{}", "A".repeat(64));
    let invalid = vec![
        "blobs/not-hex".to_string(),
        "thumbnails/x".to_string(),
        format!("blobs/{}", "b".repeat(63)),
        format!("BLOBS/{}", "c".repeat(64)),
    ];
    let mut page = invalid.clone();
    page.push(legacy_upper.clone());
    let lister = FakeLister::new(vec![page]);

    let report =
        scan_orphan_blobs_dry_run(&lister, &env.metadata, &config(10, Duration::ZERO), None)
            .await
            .expect("dry-run scan");
    let outcomes = outcomes_by_key(&report);

    for key in invalid {
        assert_eq!(
            outcomes.get(&key),
            Some(&Classification::ExcludedInvalidKey),
            "key {key} must be excluded, never counted as a potential orphan"
        );
    }
    assert_eq!(report.excluded_invalid_key_count, 4);
    // Positive control: legacy uppercase hex is a valid content-addressed key.
    assert_eq!(
        outcomes.get(&legacy_upper),
        Some(&Classification::PotentialOrphan)
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn j_pagination_resumes_and_covers_without_skips() {
    let env = setup().await;
    let all_keys: Vec<String> = (0..2500)
        .map(|index| format!("blobs/{:064x}", index))
        .collect();
    let pages: Vec<Vec<String>> = all_keys.chunks(1000).map(|chunk| chunk.to_vec()).collect();
    let lister = FakeLister::new(pages);

    let first =
        scan_orphan_blobs_dry_run(&lister, &env.metadata, &config(1000, Duration::ZERO), None)
            .await
            .expect("first invocation");
    assert_eq!(first.scanned_count, 1000);
    let calls_after_first = lister.called_tokens().len();
    assert!(
        calls_after_first <= 1000,
        "page-call cap must bound listing calls per invocation"
    );
    let resume_token = first
        .next_continuation_token
        .clone()
        .expect("budget-consumed invocation must return a resume token");

    let second = scan_orphan_blobs_dry_run(
        &lister,
        &env.metadata,
        &config(1000, Duration::ZERO),
        Some(&resume_token),
    )
    .await
    .expect("resumed invocation");
    assert_eq!(second.scanned_count, 1000);
    let final_token = second
        .next_continuation_token
        .clone()
        .expect("budget-consumed invocation must return a resume token");

    let third = scan_orphan_blobs_dry_run(
        &lister,
        &env.metadata,
        &config(1000, Duration::ZERO),
        Some(&final_token),
    )
    .await
    .expect("final invocation");
    assert_eq!(third.scanned_count, 500);
    assert_eq!(third.next_continuation_token, None);
    assert_eq!(
        lister.called_tokens(),
        vec![None, Some("page-1".to_string()), Some("page-2".to_string())],
        "the resume token must drive the next listing page exactly"
    );
    assert!(
        lister.called_tokens().len() - calls_after_first <= 1000,
        "page-call cap must bound listing calls per invocation"
    );

    let mut covered: std::collections::BTreeSet<String> = first
        .outcomes
        .iter()
        .chain(second.outcomes.iter())
        .chain(third.outcomes.iter())
        .map(|outcome| outcome.key.clone())
        .collect();
    assert_eq!(
        covered.len(),
        2500,
        "no key may be skipped or double-counted across invocations"
    );
    covered.extend(all_keys.iter().cloned());
    assert_eq!(
        covered.len(),
        2500,
        "resumed scans must cover exactly the listed keys"
    );
    for key in &all_keys {
        assert!(covered.contains(key));
    }

    // Repeated full scans are stable.
    let full_config = config(1000, Duration::ZERO);
    let run_a = scan_all(&lister, &env.metadata, &full_config).await;
    let run_b = scan_all(&lister, &env.metadata, &full_config).await;
    assert_eq!(run_a, run_b);
    assert_eq!(run_a.len(), 2500);
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn j_empty_pages_stop_at_page_cap() {
    let env = setup().await;
    // A churning bucket can answer listing pages that are empty but still
    // carry a continuation token; the page-call budget must bound the
    // invocation and stay resumable instead of following tokens forever.
    let lister = FakeLister::new(Vec::new()).with_empty_tokened_pages();
    let budget = 500;
    let page_cap = 500; // max(config.max_keys_per_invocation, 100)

    let report = scan_orphan_blobs_dry_run(
        &lister,
        &env.metadata,
        &config(budget, Duration::ZERO),
        None,
    )
    .await
    .expect("scan over empty-tokened pages must terminate normally");

    assert_eq!(report.scanned_count, 0);
    assert_eq!(report.potential_orphan_count, 0);
    let resume_token = report
        .next_continuation_token
        .clone()
        .expect("page-cap stop must return a resumable token");
    assert_eq!(
        lister.called_tokens().len(),
        page_cap,
        "invocation must stop at exactly the page-call cap"
    );

    // Resuming from the token terminates the same way instead of hanging.
    let resumed = scan_orphan_blobs_dry_run(
        &lister,
        &env.metadata,
        &config(budget, Duration::ZERO),
        Some(&resume_token),
    )
    .await
    .expect("resumed scan over empty-tokened pages must also terminate");
    assert_eq!(resumed.scanned_count, 0);
    assert!(resumed.next_continuation_token.is_some());
    assert_eq!(
        lister.called_tokens().len(),
        2 * page_cap,
        "each invocation is bounded by the same page-call cap"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn k_list_failure_aborts_and_db_outage_marks_uncertainty() {
    let env = setup().await;

    // A hard list failure aborts with Err(ScanError::List); nothing partial.
    let failing = FakeLister::new(vec![
        vec![format!("blobs/{:064x}", 1)],
        vec![format!("blobs/{:064x}", 2)],
    ])
    .fail_on_page(1);
    let error =
        scan_orphan_blobs_dry_run(&failing, &env.metadata, &config(10, Duration::ZERO), None)
            .await
            .expect_err("list failure must abort the invocation");
    match error {
        ScanError::List(message) => {
            assert!(message.contains("synthetic list failure"));
            assert!(message.len() <= 500, "error text must be bounded");
        }
        other => panic!("expected ScanError::List, got {other:?}"),
    }

    // A metadata outage must degrade to uncertain_db_error, never zero references.
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    let dead_pool = PgPool::connect(&database_url)
        .await
        .expect("connect disposable pool");
    let dead_metadata = MetadataStore::new(dead_pool.clone());
    dead_pool.close().await;

    let keys = vec![
        format!("blobs/{:064x}", 10),
        format!("blobs/{:064x}", 11),
        format!("blobs/{:064x}", 12),
    ];
    let lister = FakeLister::new(vec![keys]);
    let report =
        scan_orphan_blobs_dry_run(&lister, &dead_metadata, &config(10, Duration::ZERO), None)
            .await
            .expect("partial report survives a database outage");
    assert!(!report.db_reachable);
    assert_eq!(report.uncertain_db_error_count, 3);
    assert_eq!(report.uncertain_count, 3);
    assert_eq!(report.potential_orphan_count, 0);
    assert!(!report.uncertain_reasons.is_empty());
    for reason in &report.uncertain_reasons {
        assert!(reason.len() <= 500, "uncertain reason must be bounded");
    }
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn l_scan_is_read_only() {
    let env = setup().await;
    let content = format!("orphan-scan-l-{}", Uuid::new_v4()).into_bytes();
    let key = blob(&content);
    env.object_store
        .put(&key, content.clone().into())
        .await
        .expect("put blob");
    let user_id = insert_user(&env.pool, "l").await;
    insert_file_reference(&env.pool, user_id, &key, content.len() as i64).await;

    async fn table_count(pool: &PgPool, table: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(pool)
            .await
            .expect("count table")
    }

    let snapshot = (
        table_count(&env.pool, "files").await,
        table_count(&env.pool, "file_versions").await,
        table_count(&env.pool, "object_gc_queue").await,
        env.object_store
            .get(&key)
            .await
            .expect("read blob before scan"),
    );

    for _ in 0..2 {
        scan_finding_key(
            env.object_store.as_ref() as &dyn BlobLister,
            &env.metadata,
            &config(1000, Duration::ZERO),
            &key,
        )
        .await;
    }

    let after = (
        table_count(&env.pool, "files").await,
        table_count(&env.pool, "file_versions").await,
        table_count(&env.pool, "object_gc_queue").await,
        env.object_store
            .get(&key)
            .await
            .expect("read blob after scan"),
    );
    assert_eq!(
        snapshot, after,
        "a dry-run scan must not mutate database rows or objects"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn m_classification_is_a_point_in_time_observation() {
    let env = setup().await;
    let content = format!("orphan-scan-m-{}", Uuid::new_v4()).into_bytes();
    let key = blob(&content);
    env.object_store
        .put(&key, content.into())
        .await
        .expect("put blob");

    let lister = single_key_lister(&key);
    let first =
        scan_orphan_blobs_dry_run(&lister, &env.metadata, &config(10, Duration::ZERO), None)
            .await
            .expect("first scan");
    assert_eq!(
        outcomes_by_key(&first).get(&key),
        Some(&Classification::PotentialOrphan)
    );

    // A durable reference appears between inspections; the same key must flip.
    let user_id = insert_user(&env.pool, "m").await;
    insert_file_reference(&env.pool, user_id, &key, 1).await;

    let second =
        scan_orphan_blobs_dry_run(&lister, &env.metadata, &config(10, Duration::ZERO), None)
            .await
            .expect("second scan");
    assert_eq!(
        outcomes_by_key(&second).get(&key),
        Some(&Classification::Referenced),
        "potential_orphan is only zero references observed at inspection time; \
         it is not deletion-safe"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn n_report_carries_counts_only_and_lister_seam_is_list_only() {
    let env = setup().await;
    let key = blob(format!("orphan-scan-n-{}", Uuid::new_v4()).as_bytes());
    let lister = single_key_lister(&key);
    let report =
        scan_orphan_blobs_dry_run(&lister, &env.metadata, &config(10, Duration::ZERO), None)
            .await
            .expect("dry-run scan");

    // Per-key detail exists internally for assertions...
    assert_eq!(
        report.outcomes,
        vec![KeyScanOutcome {
            key: key.clone(),
            classification: Classification::PotentialOrphan,
        }]
    );
    // ...but never crosses the HTTP boundary: aggregate counts only, no
    // key-identifying fields, no sizes, no last-modified, no contents.
    let json = serde_json::to_value(&report).expect("serialize report");
    for field in [
        "outcomes",
        "size",
        "sizes",
        "last_modified",
        "content",
        "contents",
        "first_key_scanned",
        "last_key_scanned",
    ] {
        assert!(json.get(field).is_none(), "report must not contain {field}");
    }
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["potential_orphan_count"], 1);
    assert_eq!(json["scanned_count"], 1);

    // By-construction guarantee: the scan seam accepts &dyn BlobLister only;
    // ObjectStore's put/delete methods are not reachable through the trait, so
    // a scan cannot mutate object storage (asserted by usage, compile-time).
    let list_only: &dyn BlobLister = env.object_store.as_ref();
    let _ =
        scan_orphan_blobs_dry_run(list_only, &env.metadata, &config(1, Duration::ZERO), None).await;
}
