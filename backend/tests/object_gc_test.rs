//! PostgreSQL and S3-compatible object GC contract tests.

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use bytes::Bytes;
use rustshare_server::object_gc::{tick, ObjectGcConfig};
use rustshare_storage::{MetadataStore, ObjectStore, ObjectStoreOptions};
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
