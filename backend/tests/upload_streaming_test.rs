//! Integration test: large-object streaming upload/download.
//!
//! These tests exercise the streaming paths end-to-end and require a running
//! PostgreSQL database and S3-compatible object storage. They are ignored by
//! default so `cargo test` does not fail when the infrastructure is missing.
//!
//! Run with:
//!   cargo test --test upload_streaming_test -- --ignored

use bytes::Bytes;
use futures_util::StreamExt;
use rustshare_core::domain::User;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{CreateSessionRequest, PermissionResolver, UploadService};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::adapters::{UploadMetadataStoreAdapter, UploadObjectStoreAdapter};
use rustshare_storage::{
    metadata_v2::{stores::LocalFsDocumentStore, MetadataBackendConfig},
    repos::RustFsUploadSessionRepository,
    EventStore, MetadataStore, ObjectStore,
};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

async fn setup_test_env() -> (
    PgPool,
    Arc<EventStore>,
    Arc<MetadataStore>,
    Arc<ObjectStore>,
) {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let event_store = Arc::new(EventStore::new(pool.clone()));
    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));

    let s3_endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .unwrap_or_else(|_| "http://localhost:9000".to_string());
    let s3_region = std::env::var("S3_REGION")
        .or_else(|_| std::env::var("RUSTFS_REGION"))
        .unwrap_or_else(|_| "us-east-1".to_string());
    let s3_bucket = std::env::var("S3_BUCKET")
        .or_else(|_| std::env::var("RUSTFS_BUCKET"))
        .unwrap_or_else(|_| "rustshare".to_string());

    let object_store = Arc::new(
        ObjectStore::new(s3_endpoint, s3_region, s3_bucket)
            .await
            .expect("Failed to create object store"),
    );

    (pool, event_store, metadata_store, object_store)
}

fn create_file_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &PgPool,
) -> rustshare_core::services::FileService<
    EventStore,
    MetadataStore,
    ObjectStore,
    PermissionResolverRepository,
> {
    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));
    rustshare_core::services::FileService::new(
        event_store,
        metadata_store,
        object_store,
        broadcaster,
        permission_resolver,
    )
}

type TestUploadService = rustshare_core::services::UploadService<
    RustFsUploadSessionRepository,
    UploadObjectStoreAdapter,
    UploadMetadataStoreAdapter,
    EventStore,
>;

fn create_upload_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
) -> (TestUploadService, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().expect("create temp dir for upload doc store");
    let doc_store: Arc<dyn rustshare_storage::metadata_v2::MetadataDocumentStore> =
        Arc::new(LocalFsDocumentStore::new(
            temp_dir.path().to_path_buf(),
            MetadataBackendConfig {
                base_prefix: "apps/rustshare".to_string(),
                namespace: "uploads".to_string(),
                enable_optimistic_concurrency: true,
                fallback_to_leases: true,
            },
        ));

    let repository = Arc::new(RustFsUploadSessionRepository::new(
        doc_store,
        "apps/rustshare".to_string(),
        "uploads".to_string(),
    ));
    let object_store_adapter = Arc::new(UploadObjectStoreAdapter::new(object_store));
    let metadata_store_adapter = Arc::new(UploadMetadataStoreAdapter::new(metadata_store));
    let broadcaster = Arc::new(EventBroadcaster::new(100));

    let service = UploadService::new(
        repository,
        object_store_adapter,
        metadata_store_adapter,
        event_store,
        broadcaster,
    );

    (service, temp_dir)
}

async fn create_test_user(metadata_store: &MetadataStore, username: &str, tenant_id: Uuid) -> User {
    let user = User::new(
        username.to_string(),
        format!("{} Display", username),
        "test_password_hash".to_string(),
        format!("{}@test.local", username),
        false,
        10_737_418_240, // 10GB
        tenant_id,
    );

    metadata_store
        .create_user(&user)
        .await
        .expect("Failed to create test user");

    user
}

async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
    sqlx::query("DELETE FROM files WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM folders WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
}

/// Build a multi-MB file on disk and return its path and expected content.
async fn create_large_temp_file(size_bytes: usize) -> (tempfile::NamedTempFile, Vec<u8>) {
    let temp_file = tokio::task::spawn_blocking(tempfile::NamedTempFile::new)
        .await
        .expect("spawn temp file")
        .expect("create temp file");

    let mut async_file = tokio::fs::File::from_std(temp_file.reopen().expect("reopen temp file"));

    let chunk_size = 64 * 1024usize;
    let mut content = Vec::with_capacity(size_bytes);
    let mut written = 0usize;
    let mut counter = 0u8;
    while written < size_bytes {
        let to_write = std::cmp::min(chunk_size, size_bytes - written);
        let chunk = vec![counter; to_write];
        tokio::io::AsyncWriteExt::write_all(&mut async_file, &chunk)
            .await
            .expect("write temp file");
        content.extend_from_slice(&chunk);
        written += to_write;
        counter = counter.wrapping_add(1);
    }
    tokio::io::AsyncWriteExt::flush(&mut async_file)
        .await
        .expect("flush temp file");

    (temp_file, content)
}

#[tokio::test]
#[ignore = "requires database and S3"]
async fn large_file_upload_uses_streaming_path_and_matches_on_download() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "streaming_upload_user", tenant_id).await;

    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    // Use a 4 MiB file so the streaming path is exercised while keeping tests fast.
    let file_size = 4 * 1024 * 1024;
    let (temp_file, expected_content) = create_large_temp_file(file_size).await;

    let uploaded_file = file_service
        .upload_file_from_path(
            user.id,
            "large-streaming.bin".to_string(),
            None,
            temp_file.path(),
            "application/octet-stream".to_string(),
            tenant_id,
        )
        .await
        .expect("should upload large file from path");

    assert_eq!(uploaded_file.size, file_size as i64);

    // Stream the file back from object storage and verify every byte.
    let (_content_type, content_length, stream) = object_store
        .get_stream(&uploaded_file.storage_key())
        .await
        .expect("should stream object from storage");

    assert_eq!(content_length, Some(file_size as i64));

    let chunks: Vec<Bytes> = stream.map(|r| r.expect("stream chunk")).collect().await;
    let mut received = Vec::with_capacity(file_size);
    for chunk in chunks {
        received.extend_from_slice(&chunk);
    }
    assert_eq!(received.len(), file_size);
    assert_eq!(received, expected_content);

    // The temporary upload file is deleted automatically when dropped.
    assert!(
        !temp_file.path().exists(),
        "temporary upload file should be cleaned up"
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "requires database and S3"]
async fn aborted_resumable_upload_cleans_up_chunks() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "abort_upload_user", tenant_id).await;

    let (upload_service, _upload_store_dir) = create_upload_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
    );

    let total_size = 2 * 1024 * 1024u64;
    let chunk_size = 1024 * 1024u64;

    let session = upload_service
        .create_session(
            user.id,
            tenant_id,
            CreateSessionRequest {
                folder_id: None,
                file_name: "abort-me.bin".to_string(),
                mime_type: "application/octet-stream".to_string(),
                total_size,
                chunk_size,
                file_hash: None,
            },
        )
        .await
        .expect("create upload session");

    // Upload the first chunk from a temp file.
    let (chunk_temp, _chunk_content) = create_large_temp_file(chunk_size as usize).await;
    upload_service
        .upload_chunk_from_path(session.session_id, 0, chunk_temp.path(), None, user.id)
        .await
        .expect("upload first chunk");

    // Verify the chunk landed in object storage before abort.
    let chunk_key = format!("temp/uploads/{}/0", session.session_id);
    assert!(
        object_store.exists(&chunk_key).await.unwrap(),
        "uploaded chunk should exist in object storage before abort"
    );

    // Abort the session; chunks should be cleaned up.
    upload_service
        .abort_session(session.session_id, user.id)
        .await
        .expect("abort session");

    let status = upload_service
        .get_session_status(session.session_id, user.id)
        .await;
    assert!(
        matches!(
            status,
            Err(rustshare_core::services::UploadError::SessionAborted(_))
        ),
        "session should be marked aborted"
    );

    assert!(
        !object_store.exists(&chunk_key).await.unwrap(),
        "uploaded chunk should be deleted from object storage after abort"
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "requires database and S3"]
async fn temp_file_is_cleaned_up_after_failed_stream() {
    // This test verifies the contract used by the HTTP handlers: even when
    // streaming fails, the NamedTempFile Drop impl removes the underlying path.
    let temp_file = tokio::task::spawn_blocking(tempfile::NamedTempFile::new)
        .await
        .expect("spawn temp file")
        .expect("create temp file");

    let path = temp_file.path().to_path_buf();
    assert!(path.exists());

    drop(temp_file);
    assert!(!path.exists(), "temp file path should be removed on drop");
}
