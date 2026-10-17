//! PostgreSQL and S3-compatible object GC contract tests.

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use bytes::Bytes;
use rustshare_server::object_gc::{tick, ObjectGcConfig};
use rustshare_storage::{
    metadata::ObjectGcCandidateRow, MetadataStore, ObjectStore, ObjectStoreOptions,
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

static TEST_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

async fn setup() -> (
    tokio::sync::OwnedMutexGuard<()>,
    PgPool,
    Arc<MetadataStore>,
    Arc<ObjectStore>,
) {
    let test_guard = TEST_LOCK
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
    sqlx::query("DELETE FROM object_gc_queue WHERE reason LIKE 'object_gc_test:%'")
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

    (
        test_guard,
        pool.clone(),
        Arc::new(MetadataStore::new(pool)),
        Arc::new(object_store),
    )
}

fn config(batch_size: i64) -> ObjectGcConfig {
    ObjectGcConfig {
        enabled: true,
        interval: Duration::from_secs(60),
        batch_size,
        grace_period_hours: 1,
        max_attempts: 5,
        lease_seconds: 60,
        max_backoff_seconds: 300,
    }
}

fn blob(content: &[u8]) -> String {
    format!("blobs/{}", hex::encode(Sha256::digest(content)))
}

async fn make_due(pool: &PgPool, key: &str) {
    sqlx::query(
        "UPDATE object_gc_queue SET not_before = '-infinity', last_seen_at = '-infinity', created_at = '-infinity' WHERE object_key = $1",
    )
    .bind(key)
    .execute(pool)
    .await
    .expect("make candidate due");
}

async fn delete_files_in_order(
    pool: PgPool,
    first_file_id: Uuid,
    second_file_id: Uuid,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("DELETE FROM files WHERE id = $1")
        .bind(first_file_id)
        .execute(&mut *transaction)
        .await?;
    // Deliberately keep the first queue key locked while the other transaction
    // reaches its first delete, forcing the opposite-order lock cycle.
    sqlx::query("SELECT pg_sleep(0.25)")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM files WHERE id = $1")
        .bind(second_file_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn concurrent_file_deletes_enqueue_shared_keys_without_deadlock() {
    let (_test_guard, pool, _metadata, _) = setup().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();
    let left_first = Uuid::new_v4();
    let left_second = Uuid::new_v4();
    let right_first = Uuid::new_v4();
    let right_second = Uuid::new_v4();
    let key_a = format!("blobs/{0}{0}", Uuid::new_v4().simple());
    let key_b = format!("blobs/{0}{0}", Uuid::new_v4().simple());

    for (user_id, tenant_id, suffix) in [(user_a, tenant_a, "left"), (user_b, tenant_b, "right")] {
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, display_name, storage_quota, tenant_id) VALUES ($1, $2, $3, 'test', $4, 1000000, $5)",
        )
        .bind(user_id)
        .bind(format!("object-gc-deadlock-{suffix}-{user_id}"))
        .bind(format!("object-gc-deadlock-{suffix}-{user_id}@test.local"))
        .bind(format!("Object GC {suffix}"))
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("create deadlock regression user");
    }

    for (file_id, user_id, tenant_id, key, name) in [
        (left_first, user_a, tenant_a, &key_a, "left-a"),
        (left_second, user_a, tenant_a, &key_b, "left-b"),
        (right_first, user_b, tenant_b, &key_b, "right-b"),
        (right_second, user_b, tenant_b, &key_a, "right-a"),
    ] {
        sqlx::query(
            "INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id, tenant_id) VALUES ($1, $2, $3, 1, 'text/plain', $4, $5, $6, $7)",
        )
        .bind(file_id)
        .bind(name)
        .bind(format!("/{name}"))
        .bind(key.strip_prefix("blobs/").expect("blob key"))
        .bind(key)
        .bind(user_id)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("create deadlock regression file");
    }

    let deletes = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            delete_files_in_order(pool.clone(), left_first, left_second),
            delete_files_in_order(pool.clone(), right_first, right_second),
        )
    })
    .await
    .expect("concurrent file deletes did not complete");
    assert!(deletes.0.is_ok(), "left delete failed: {:?}", deletes.0);
    assert!(deletes.1.is_ok(), "right delete failed: {:?}", deletes.1);

    let queued: Vec<(String, String, String, bool, bool)> = sqlx::query_as(
        "SELECT object_key, reason, state, operator_hold, not_before <= NOW() FROM object_gc_queue WHERE object_key = ANY($1) ORDER BY object_key",
    )
    .bind(vec![key_a.clone(), key_b.clone()])
    .fetch_all(&pool)
    .await
    .expect("read concurrent delete queue entries");
    assert_eq!(queued.len(), 2);
    assert!(queued
        .iter()
        .all(|(object_key, reason, state, operator_hold, is_due)| {
            [key_a.as_str(), key_b.as_str()].contains(&object_key.as_str())
                && reason == "reference_replaced"
                && state == "pending"
                && !operator_hold
                && *is_due
        }));

    sqlx::query("DELETE FROM users WHERE id = ANY($1)")
        .bind(vec![user_a, user_b])
        .execute(&pool)
        .await
        .expect("cleanup deadlock regression users");
    sqlx::query("DELETE FROM object_gc_queue WHERE object_key = ANY($1)")
        .bind(vec![key_a, key_b])
        .execute(&pool)
        .await
        .expect("cleanup deadlock regression queue entries");
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn unreferenced_blob_is_deleted_after_two_reference_checks() {
    let (_test_guard, pool, metadata, objects) = setup().await;
    let content = Bytes::from(format!("object-gc-{}", Uuid::new_v4()));
    let key = blob(&content);
    objects.put(&key, content).await.expect("put blob");
    metadata
        .enqueue_object_gc_candidate(&key, "object_gc_test:delete", 1)
        .await
        .expect("enqueue candidate");
    make_due(&pool, &key).await;

    tick(&metadata, &objects, &config(1))
        .await
        .expect("GC tick");

    assert!(!objects.exists(&key).await.expect("check blob"));
    let state: String =
        sqlx::query_scalar("SELECT state FROM object_gc_queue WHERE object_key = $1")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .expect("candidate state");
    assert_eq!(state, "deleted");
}

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn duplicate_candidates_coalesce_without_shortening_delay() {
    let (_test_guard, pool, metadata, _) = setup().await;
    let key = blob(Uuid::new_v4().as_bytes());
    metadata
        .enqueue_object_gc_candidate(&key, "object_gc_test:coalesce-long", 48)
        .await
        .expect("first enqueue");
    metadata
        .enqueue_object_gc_candidate(&key, "object_gc_test:coalesce-short", 1)
        .await
        .expect("second enqueue");

    let (count, hours): (i64, f64) = sqlx::query_as(
        "SELECT COUNT(*)::BIGINT, (EXTRACT(EPOCH FROM MAX(not_before) - NOW()) / 3600)::DOUBLE PRECISION FROM object_gc_queue WHERE object_key = $1",
    )
    .bind(&key)
    .fetch_one(&pool)
    .await
    .expect("candidate row");
    assert_eq!(count, 1);
    assert!(hours > 47.0, "delay was shortened to {hours} hours");
}

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn concurrent_workers_lease_candidate_once() {
    let (_test_guard, pool, metadata, _) = setup().await;
    let key = blob(Uuid::new_v4().as_bytes());
    metadata
        .enqueue_object_gc_candidate(&key, "object_gc_test:concurrency", 1)
        .await
        .expect("enqueue candidate");
    make_due(&pool, &key).await;

    let (left, right) = tokio::join!(
        metadata.lease_object_gc_candidates(1, 60, "worker-left", 1),
        metadata.lease_object_gc_candidates(1, 60, "worker-right", 1),
    );
    let leased = left
        .expect("left lease")
        .into_iter()
        .chain(right.expect("right lease"))
        .filter(|candidate| candidate.object_key == key)
        .count();
    assert_eq!(leased, 1);
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn global_references_protect_shared_blob() {
    let (_test_guard, pool, metadata, objects) = setup().await;
    let content = Bytes::from(format!("shared-object-gc-{}", Uuid::new_v4()));
    let key = blob(&content);
    let digest = key.strip_prefix("blobs/").expect("blob prefix");
    let suffix = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();
    let vault_id = Uuid::new_v4();
    let message_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, storage_quota) VALUES ($1, $2, $3, 'test', 'Object GC', 1000000)",
    )
    .bind(user_id)
    .bind(format!("object-gc-{suffix}"))
    .bind(format!("object-gc-{suffix}@test.local"))
    .execute(&pool)
    .await
    .expect("create user");
    sqlx::query(
        "INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id) VALUES ($1, 'shared.bin', $2, $3, 'application/octet-stream', $4, $5, $6)",
    )
    .bind(file_id)
    .bind(format!("/shared-{suffix}.bin"))
    .bind(content.len() as i64)
    .bind(digest)
    .bind(&key)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("create file reference");
    sqlx::query(
        "INSERT INTO file_versions (id, file_id, version_number, content_hash, storage_key, size, created_by) VALUES ($1, $2, 1, $3, $4, $5, $6)",
    )
    .bind(version_id)
    .bind(file_id)
    .bind(digest)
    .bind(&key)
    .bind(content.len() as i64)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("create version reference");
    sqlx::query(
        "INSERT INTO vaults (id, owner_user_id, name, adapter) VALUES ($1, $2, 'GC vault', 'obsidian')",
    )
    .bind(vault_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("create vault");
    sqlx::query(
        "INSERT INTO vault_files (id, vault_id, relative_path, sha256, size, server_rev) VALUES ($1, $2, 'shared.bin', $3, $4, 1)",
    )
    .bind(Uuid::new_v4())
    .bind(vault_id)
    .bind(digest)
    .bind(content.len() as i64)
    .execute(&pool)
    .await
    .expect("create vault reference");
    sqlx::query(
        "INSERT INTO mail_messages (id, owner_id, imported_by, blob_key) VALUES ($1, $2, $2, $3)",
    )
    .bind(message_id)
    .bind(user_id)
    .bind(&key)
    .execute(&pool)
    .await
    .expect("create mail reference");
    sqlx::query(
        "INSERT INTO mail_message_parts (message_id, part_index, content_type, blob_key) VALUES ($1, 0, 'text/plain', $2)",
    )
    .bind(message_id)
    .bind(&key)
    .execute(&pool)
    .await
    .expect("create mail part reference");
    sqlx::query(
        "INSERT INTO mail_attachments (message_id, filename, blob_key) VALUES ($1, 'shared.bin', $2)",
    )
    .bind(message_id)
    .bind(&key)
    .execute(&pool)
    .await
    .expect("create attachment reference");
    sqlx::query(
        "INSERT INTO replication_jobs (id, file_id, file_version_id, storage_key, status) VALUES ($1, $2, $3, $4, 'retrying')",
    )
    .bind(Uuid::new_v4())
    .bind(file_id)
    .bind(version_id)
    .bind(&key)
    .execute(&pool)
    .await
    .expect("create replication reference");

    let summary = metadata
        .count_blob_references(&key)
        .await
        .expect("count global references");
    assert_eq!(summary.total(), 7);

    objects.put(&key, content).await.expect("put shared blob");
    metadata
        .enqueue_object_gc_candidate(&key, "object_gc_test:referenced", 1)
        .await
        .expect("enqueue candidate");
    make_due(&pool, &key).await;
    tick(&metadata, &objects, &config(1))
        .await
        .expect("GC tick");

    assert!(objects.exists(&key).await.expect("shared blob remains"));
    let state: String =
        sqlx::query_scalar("SELECT state FROM object_gc_queue WHERE object_key = $1")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .expect("candidate state");
    assert_eq!(state, "referenced");

    sqlx::query("DELETE FROM files WHERE id = $1")
        .bind(file_id)
        .execute(&pool)
        .await
        .expect("cleanup file");
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("cleanup user");
    objects.delete(&key).await.expect("cleanup blob");
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn missing_object_completes_idempotently() {
    let (_test_guard, pool, metadata, objects) = setup().await;
    let key = blob(Uuid::new_v4().as_bytes());
    metadata
        .enqueue_object_gc_candidate(&key, "object_gc_test:missing", 1)
        .await
        .expect("enqueue candidate");
    make_due(&pool, &key).await;

    tick(&metadata, &objects, &config(1))
        .await
        .expect("GC tick");

    let state: String =
        sqlx::query_scalar("SELECT state FROM object_gc_queue WHERE object_key = $1")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .expect("candidate state");
    assert_eq!(state, "missing");
}

#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn invalid_key_is_never_deleted() {
    let (_test_guard, pool, metadata, objects) = setup().await;
    let key = format!("blobs/invalid-{}", Uuid::new_v4());
    objects
        .put(&key, Bytes::from_static(b"must remain"))
        .await
        .expect("put invalid-key sentinel");
    metadata
        .enqueue_object_gc_candidate(&key, "object_gc_test:invalid", 1)
        .await
        .expect("enqueue candidate");
    make_due(&pool, &key).await;

    tick(&metadata, &objects, &config(1))
        .await
        .expect("GC tick");

    assert!(objects.exists(&key).await.expect("sentinel remains"));
    let state: String =
        sqlx::query_scalar("SELECT state FROM object_gc_queue WHERE object_key = $1")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .expect("candidate state");
    assert_eq!(state, "invalid_key");
    objects.delete(&key).await.expect("cleanup sentinel");
}

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn stale_processing_lease_is_reclaimed_after_restart() {
    let (_test_guard, pool, metadata, _) = setup().await;
    let key = blob(Uuid::new_v4().as_bytes());
    metadata
        .enqueue_object_gc_candidate(&key, "object_gc_test:restart", 1)
        .await
        .expect("enqueue candidate");
    make_due(&pool, &key).await;
    let first = metadata
        .lease_object_gc_candidates(1, 60, "crashed-worker", 1)
        .await
        .expect("first lease");
    assert_eq!(first[0].object_key, key);
    sqlx::query(
        "UPDATE object_gc_queue SET locked_at = NOW() - INTERVAL '2 minutes' WHERE object_key = $1",
    )
    .bind(&key)
    .execute(&pool)
    .await
    .expect("expire lease");

    let reclaimed = metadata
        .lease_object_gc_candidates(1, 60, "replacement-worker", 1)
        .await
        .expect("reclaim lease");
    assert_eq!(reclaimed[0].object_key, key);
    assert_eq!(
        reclaimed[0].locked_by.as_deref(),
        Some("replacement-worker")
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn exhausted_candidate_moves_to_operator_hold() {
    let (_test_guard, pool, metadata, _) = setup().await;
    let key = blob(Uuid::new_v4().as_bytes());
    metadata
        .enqueue_object_gc_candidate(&key, "object_gc_test:attempt-cap", 1)
        .await
        .expect("enqueue candidate");
    make_due(&pool, &key).await;

    let leased = metadata
        .lease_object_gc_candidates(1, 60, "exhausting-worker", 1)
        .await
        .expect("lease candidate");
    assert_eq!(leased[0].attempt_count, 1);
    assert!(metadata
        .hold_object_gc_candidate(&key, "exhausting-worker", "permanent test failure")
        .await
        .expect("hold candidate"));

    let leased_again = metadata
        .lease_object_gc_candidates(1, 60, "later-worker", 1)
        .await
        .expect("lease after hold");
    assert!(leased_again
        .iter()
        .all(|candidate| candidate.object_key != key));
    let (state, operator_hold, error): (String, bool, Option<String>) = sqlx::query_as(
        "SELECT state, operator_hold, last_error FROM object_gc_queue WHERE object_key = $1",
    )
    .bind(&key)
    .fetch_one(&pool)
    .await
    .expect("held candidate state");
    assert_eq!(state, "operator_hold");
    assert!(operator_hold);
    assert_eq!(error.as_deref(), Some("permanent test failure"));
}

/// Characterization proof for the existing `enqueue_object_gc_candidate`
/// reset-on-conflict behavior: re-enqueueing an operator-held candidate
/// clears the hold, resets the attempt history, and clears any lease.
///
/// This documents why the orphan-reconciliation producer must NOT reuse that
/// function: it would silently release operator holds and worker leases. The
/// producer must use `enqueue_object_gc_candidate_if_absent` instead, which
/// leaves existing rows completely untouched.
#[allow(clippy::type_complexity)]
#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn characterization_enqueue_resets_operator_held_row() {
    let (_test_guard, pool, metadata, _objects) = setup().await;
    let key = format!("blobs/{}", Uuid::new_v4());

    // Simulate a candidate that failed repeatedly and was operator-held with
    // an active lease, a completed run, and a future schedule.
    sqlx::query(
        r#"
        INSERT INTO object_gc_queue
            (object_key, reason, state, attempt_count, last_attempt_at, last_error,
             operator_hold, locked_at, locked_by, completed_at,
             not_before, created_at, updated_at)
        VALUES ($1, 'object_gc_test:characterization', 'operator_hold', 5,
                NOW(), 'permanent test failure',
                true, NOW(), 'held-worker', NOW(),
                NOW() + INTERVAL '30 hours', NOW(), NOW())
        "#,
    )
    .bind(&key)
    .execute(&pool)
    .await
    .expect("insert operator-held candidate");

    metadata
        .enqueue_object_gc_candidate(&key, "object_gc_test:characterization", 1)
        .await
        .expect("re-enqueue candidate");

    let (state, attempt_count, operator_hold, locked_at, locked_by, completed_at, last_attempt_at): (
        String,
        i32,
        bool,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT state, attempt_count, operator_hold, locked_at, locked_by, completed_at, last_attempt_at FROM object_gc_queue WHERE object_key = $1",
    )
    .bind(&key)
    .fetch_one(&pool)
    .await
    .expect("candidate row");

    assert_eq!(state, "pending", "state resets on conflict");
    assert_eq!(attempt_count, 0, "attempt history resets on conflict");
    assert!(!operator_hold, "operator hold clears on conflict");
    assert!(locked_at.is_none(), "lease clears on conflict");
    assert!(locked_by.is_none(), "lease owner clears on conflict");
    assert!(completed_at.is_none(), "completion clears on conflict");
    assert!(last_attempt_at.is_none(), "last attempt clears on conflict");
}

/// Re-enqueueing a candidate that previously exhausted retries (and was
/// operator-held) must reset its attempt history so it starts a fresh grace
/// period instead of being re-held after a single transient failure.
#[allow(clippy::type_complexity)]
#[tokio::test]
#[ignore = "Requires PostgreSQL and S3-compatible object storage"]
async fn reenqueue_resets_failed_candidate_attempt_history() {
    let (_test_guard, pool, metadata, _objects) = setup().await;
    let key = format!("blobs/{}", Uuid::new_v4());

    // Simulate a candidate that failed repeatedly and was operator-held.
    sqlx::query(
        r#"
        INSERT INTO object_gc_queue
            (object_key, reason, state, attempt_count, last_attempt_at, operator_hold,
             locked_at, locked_by, completed_at, not_before, created_at, updated_at)
        VALUES ($1, 'object_gc_test:reenqueue', 'operator_hold', 5, NOW(), true,
                NOW(), 'exhausting-worker', NOW(), NOW(), NOW(), NOW())
        "#,
    )
    .bind(&key)
    .execute(&pool)
    .await
    .expect("insert held candidate");

    metadata
        .enqueue_object_gc_candidate(&key, "object_gc_test:reenqueue", 1)
        .await
        .expect("re-enqueue candidate");

    let (state, attempt_count, operator_hold, last_attempt_at, locked_at, completed_at): (
        String,
        i32,
        bool,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT state, attempt_count, operator_hold, last_attempt_at, locked_at, completed_at FROM object_gc_queue WHERE object_key = $1",
    )
    .bind(&key)
    .fetch_one(&pool)
    .await
    .expect("candidate row");

    assert_eq!(state, "pending");
    assert_eq!(attempt_count, 0, "attempt history must reset on re-enqueue");
    assert!(!operator_hold, "operator hold must clear on re-enqueue");
    assert!(last_attempt_at.is_none());
    assert!(locked_at.is_none());
    assert!(completed_at.is_none());
}

fn assert_candidate_row_unchanged(before: &ObjectGcCandidateRow, after: &ObjectGcCandidateRow) {
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

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn enqueue_if_absent_inserts_pending_candidate() {
    let (_test_guard, pool, metadata, _objects) = setup().await;
    let key = format!("blobs/{}", Uuid::new_v4());

    let inserted = metadata
        .enqueue_object_gc_candidate_if_absent(&key, "object_gc_test:if-absent-insert", 24)
        .await
        .expect("insert absent candidate");
    assert!(inserted, "absent key must report a fresh insert");

    let (state, reason, operator_hold, attempt_count, grace_seconds): (
        String,
        String,
        bool,
        i32,
        f64,
    ) = sqlx::query_as(
        "SELECT state, reason, operator_hold, attempt_count, EXTRACT(EPOCH FROM not_before - NOW())::DOUBLE PRECISION FROM object_gc_queue WHERE object_key = $1",
    )
    .bind(&key)
    .fetch_one(&pool)
    .await
    .expect("candidate row");
    assert_eq!(state, "pending");
    assert_eq!(reason, "object_gc_test:if-absent-insert");
    assert!(!operator_hold);
    assert_eq!(attempt_count, 0);
    assert!(
        grace_seconds > 24.0 * 3600.0 - 5.0 && grace_seconds < 24.0 * 3600.0 + 5.0,
        "not_before must be approximately now + 24h, got {grace_seconds}s"
    );
}

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn enqueue_if_absent_leaves_operator_held_row_untouched() {
    let (_test_guard, pool, metadata, _objects) = setup().await;
    let key = format!("blobs/{}", Uuid::new_v4());
    sqlx::query(
        r#"
        INSERT INTO object_gc_queue
            (object_key, reason, state, attempt_count, last_attempt_at, last_error,
             operator_hold, locked_at, locked_by, completed_at,
             not_before, created_at, updated_at)
        VALUES ($1, 'object_gc_test:if-absent-held', 'operator_hold', 5,
                NOW(), 'permanent test failure',
                true, NOW(), 'held-worker', NOW(),
                NOW() + INTERVAL '30 hours', NOW(), NOW())
        "#,
    )
    .bind(&key)
    .execute(&pool)
    .await
    .expect("insert operator-held candidate");
    let before = metadata
        .get_object_gc_candidate(&key)
        .await
        .expect("read candidate before")
        .expect("candidate exists before");

    let inserted = metadata
        .enqueue_object_gc_candidate_if_absent(&key, "object_gc_test:if-absent-held", 1)
        .await
        .expect("attempt insert over held row");
    assert!(!inserted, "held key must not report a fresh insert");

    let after = metadata
        .get_object_gc_candidate(&key)
        .await
        .expect("read candidate after")
        .expect("candidate exists after");
    assert_candidate_row_unchanged(&before, &after);
}

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn enqueue_if_absent_leaves_processing_lease_untouched() {
    let (_test_guard, pool, metadata, _objects) = setup().await;
    let key = format!("blobs/{}", Uuid::new_v4());
    sqlx::query(
        r#"
        INSERT INTO object_gc_queue
            (object_key, reason, state, attempt_count,
             locked_at, locked_by, not_before, created_at, updated_at)
        VALUES ($1, 'object_gc_test:if-absent-processing', 'processing', 3,
                NOW(), 'worker-x', NOW() + INTERVAL '2 hours', NOW(), NOW())
        "#,
    )
    .bind(&key)
    .execute(&pool)
    .await
    .expect("insert processing candidate");
    let before = metadata
        .get_object_gc_candidate(&key)
        .await
        .expect("read candidate before")
        .expect("candidate exists before");

    let inserted = metadata
        .enqueue_object_gc_candidate_if_absent(&key, "object_gc_test:if-absent-processing", 48)
        .await
        .expect("attempt insert over leased row");
    assert!(!inserted, "leased key must not report a fresh insert");

    let after = metadata
        .get_object_gc_candidate(&key)
        .await
        .expect("read candidate after")
        .expect("candidate exists after");
    assert_candidate_row_unchanged(&before, &after);
}

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn enqueue_if_absent_preserves_retry_backoff_floor() {
    let (_test_guard, pool, metadata, _objects) = setup().await;
    let key = format!("blobs/{}", Uuid::new_v4());
    sqlx::query(
        r#"
        INSERT INTO object_gc_queue
            (object_key, reason, state, attempt_count, last_error,
             not_before, created_at, updated_at)
        VALUES ($1, 'object_gc_test:if-absent-retry', 'retry', 2, 'boom',
                NOW() + INTERVAL '5 hours', NOW(), NOW())
        "#,
    )
    .bind(&key)
    .execute(&pool)
    .await
    .expect("insert retry candidate");
    let before = metadata
        .get_object_gc_candidate(&key)
        .await
        .expect("read candidate before")
        .expect("candidate exists before");

    let inserted = metadata
        .enqueue_object_gc_candidate_if_absent(&key, "object_gc_test:if-absent-retry", 1)
        .await
        .expect("attempt insert over retry row");
    assert!(!inserted, "retry key must not report a fresh insert");

    let after = metadata
        .get_object_gc_candidate(&key)
        .await
        .expect("read candidate after")
        .expect("candidate exists after");
    assert_candidate_row_unchanged(&before, &after);
}

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn enqueue_if_absent_leaves_terminal_states_untouched() {
    let (_test_guard, pool, metadata, _objects) = setup().await;
    for state in ["referenced", "deleted", "missing", "invalid_key"] {
        let key = format!("blobs/{}", Uuid::new_v4());
        sqlx::query(
            r#"
            INSERT INTO object_gc_queue
                (object_key, reason, state, attempt_count, completed_at,
                 not_before, created_at, updated_at)
            VALUES ($1, 'object_gc_test:if-absent-terminal', $2, 1, NOW(),
                    NOW() + INTERVAL '1 hour', NOW(), NOW())
            "#,
        )
        .bind(&key)
        .bind(state)
        .execute(&pool)
        .await
        .expect("insert terminal candidate");

        let inserted = metadata
            .enqueue_object_gc_candidate_if_absent(&key, "object_gc_test:if-absent-terminal", 1)
            .await
            .expect("attempt insert over terminal row");
        assert!(!inserted, "{state} key must not report a fresh insert");

        let (row_state, completed_at): (String, Option<chrono::DateTime<chrono::Utc>>) =
            sqlx::query_as("SELECT state, completed_at FROM object_gc_queue WHERE object_key = $1")
                .bind(&key)
                .fetch_one(&pool)
                .await
                .expect("terminal candidate row");
        assert_eq!(row_state, state, "state must not change");
        assert!(
            completed_at.is_some(),
            "completed_at must not change for {state}"
        );
    }
}

#[tokio::test]
#[ignore = "Requires PostgreSQL"]
async fn enqueue_if_absent_concurrent_inserts_elect_single_winner() {
    let (_test_guard, pool, metadata, _objects) = setup().await;
    let key = format!("blobs/{}", Uuid::new_v4());

    let left = {
        let metadata = metadata.clone();
        let key = key.clone();
        tokio::spawn(async move {
            metadata
                .enqueue_object_gc_candidate_if_absent(
                    &key,
                    "object_gc_test:if-absent-concurrent",
                    1,
                )
                .await
                .expect("left insert attempt")
        })
    };
    let right = {
        let metadata = metadata.clone();
        let key = key.clone();
        tokio::spawn(async move {
            metadata
                .enqueue_object_gc_candidate_if_absent(
                    &key,
                    "object_gc_test:if-absent-concurrent",
                    1,
                )
                .await
                .expect("right insert attempt")
        })
    };
    let outcomes = tokio::join!(left, right);
    let winners = [
        outcomes.0.expect("left task"),
        outcomes.1.expect("right task"),
    ]
    .iter()
    .filter(|outcome| **outcome)
    .count();
    assert_eq!(winners, 1, "exactly one racing insert may win");

    let (count, state, attempt_count): (i64, String, i32) = sqlx::query_as(
        "SELECT COUNT(*)::BIGINT, state, attempt_count FROM object_gc_queue WHERE object_key = $1 GROUP BY state, attempt_count",
    )
    .bind(&key)
    .fetch_one(&pool)
    .await
    .expect("candidate row");
    assert_eq!(count, 1, "exactly one queue row may exist");
    assert_eq!(state, "pending");
    assert_eq!(attempt_count, 0);
}
