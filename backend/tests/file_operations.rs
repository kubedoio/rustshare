//! Integration test: File Upload/Download Flow (Task 25)
//!
//! Tests the complete end-to-end flow:
//! 1. Upload a file
//! 2. Get file metadata
//! 3. Verify stored content can be read through the object store
//! 4. Delete file
//!
//! These tests require a running database and S3-compatible storage.
//! Run with: cargo test --test file_operations -- --ignored

use bytes::Bytes;
use rustshare_core::domain::User;
use rustshare_core::events::{AggregateType, EventBroadcaster, EventType};
use rustshare_core::services::PermissionResolver;
use rustshare_core::services::{FileService, FolderService};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Setup test environment with database and S3 connections
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
) -> FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository> {
    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));
    FileService::new(
        event_store,
        metadata_store,
        object_store,
        broadcaster,
        permission_resolver,
    )
}

#[allow(dead_code)]
fn create_folder_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    pool: &PgPool,
) -> FolderService<EventStore, MetadataStore, PermissionResolverRepository> {
    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));
    FolderService::new(
        event_store,
        metadata_store,
        broadcaster,
        permission_resolver,
    )
}

/// Create a test user in the database
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

/// Cleanup test user from database
async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
    // Delete user's files first (cascade will handle versions)
    sqlx::query("DELETE FROM files WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();

    // Delete user's folders
    sqlx::query("DELETE FROM folders WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();

    // Delete the user
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test user");
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_file_upload_emits_audit_event() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "audit_upload_user", tenant_id).await;

    // Create FileService
    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    // Upload a file
    let file_content = Bytes::from("Audit test content for upload event");
    let uploaded_file = file_service
        .upload_file(
            user.id,
            "audit-upload.txt".to_string(),
            None,
            file_content.clone(),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file");

    // Verify that a durable FileUploaded event exists in the event store
    let events = event_store
        .get_events(uploaded_file.id, AggregateType::File)
        .await
        .expect("Failed to fetch events from event store");

    assert!(
        !events.is_empty(),
        "File upload must emit at least one audit event"
    );
    let upload_event = events
        .iter()
        .find(|e| e.event_type == EventType::FileUploaded);
    assert!(
        upload_event.is_some(),
        "Events must contain a FileUploaded audit event"
    );
    let event = upload_event.unwrap();
    assert_eq!(event.aggregate_id, uploaded_file.id);
    assert_eq!(event.aggregate_type, AggregateType::File);
    assert_eq!(event.user_id, user.id);

    // Cleanup
    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_file_delete_emits_audit_event() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "audit_delete_user", tenant_id).await;
    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    // Upload then delete
    let file_content = Bytes::from("Content to delete");
    let uploaded_file = file_service
        .upload_file(
            user.id,
            "audit-delete.txt".to_string(),
            None,
            file_content,
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file");

    file_service
        .delete_file(uploaded_file.id, user.id)
        .await
        .expect("Failed to delete file");

    // Verify FileDeleted event exists
    let events = event_store
        .get_events(uploaded_file.id, AggregateType::File)
        .await
        .expect("Failed to fetch events");

    let delete_event = events
        .iter()
        .find(|e| e.event_type == EventType::FileDeleted);
    assert!(
        delete_event.is_some(),
        "Events must contain a FileDeleted audit event"
    );
    let event = delete_event.unwrap();
    assert_eq!(event.aggregate_id, uploaded_file.id);
    assert_eq!(event.user_id, user.id);

    // Cleanup
    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_file_move_emits_audit_event() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "audit_move_user", tenant_id).await;

    // Create root folder and target folder
    let root_folder = rustshare_core::domain::Folder::new_root(user.id, tenant_id);
    metadata_store
        .create_folder(&root_folder)
        .await
        .expect("Failed to create root folder");

    let target_folder = rustshare_core::domain::Folder::new_child(
        "Target".to_string(),
        "/Target".to_string(),
        root_folder.id,
        user.id,
        tenant_id,
    );
    metadata_store
        .create_folder(&target_folder)
        .await
        .expect("Failed to create target folder");

    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    let file_content = Bytes::from("Content to move");
    let uploaded_file = file_service
        .upload_file(
            user.id,
            "audit-move.txt".to_string(),
            None,
            file_content,
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file");

    // Move the file
    let _moved_file = file_service
        .move_file(uploaded_file.id, Some(target_folder.id), user.id)
        .await
        .expect("Failed to move file");

    // Verify FileMoved event exists
    let events = event_store
        .get_events(uploaded_file.id, AggregateType::File)
        .await
        .expect("Failed to fetch events");

    let move_event = events.iter().find(|e| e.event_type == EventType::FileMoved);
    assert!(
        move_event.is_some(),
        "Events must contain a FileMoved audit event"
    );
    let event = move_event.unwrap();
    assert_eq!(event.aggregate_id, uploaded_file.id);
    assert_eq!(event.user_id, user.id);

    // Cleanup
    file_service
        .delete_file(uploaded_file.id, user.id)
        .await
        .ok();
    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(target_folder.id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(root_folder.id)
        .execute(&pool)
        .await
        .ok();
    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_file_upload_download_flow() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "fileops_user", tenant_id).await;

    // Create FileService
    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    // Step 1: Upload a file
    let file_content = Bytes::from("Hello, this is test file content!");
    let file_name = "test-document.txt".to_string();
    let mime_type = "text/plain".to_string();

    let uploaded_file = file_service
        .upload_file(
            user.id,
            file_name.clone(),
            None,
            file_content.clone(),
            mime_type.clone(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file");

    // Verify upload
    assert_eq!(uploaded_file.name, file_name);
    assert_eq!(uploaded_file.mime_type, mime_type);
    assert_eq!(uploaded_file.size, file_content.len() as i64);
    assert_eq!(uploaded_file.owner_id, user.id);
    assert_eq!(uploaded_file.current_version, 1);
    assert_eq!(uploaded_file.parent_folder_id, None);

    // Step 2: Get file metadata
    let retrieved_file = file_service
        .get_file(uploaded_file.id, user.id)
        .await
        .expect("Failed to get file metadata");

    assert_eq!(retrieved_file.id, uploaded_file.id);
    assert_eq!(retrieved_file.name, file_name);
    assert_eq!(retrieved_file.content_hash, uploaded_file.content_hash);
    assert_eq!(retrieved_file.size, file_content.len() as i64);

    // Step 3: Verify content can be read through the app-mediated object-store path.
    let downloaded = object_store
        .get(&uploaded_file.storage_key())
        .await
        .expect("Failed to read uploaded object");

    assert_eq!(downloaded, file_content);

    // Step 4: Delete file
    file_service
        .delete_file(uploaded_file.id, user.id)
        .await
        .expect("Failed to delete file");

    // Verify deletion - file should not be found
    let result = file_service.get_file(uploaded_file.id, user.id).await;
    assert!(result.is_err(), "File should not exist after deletion");

    // Cleanup
    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_file_upload_with_parent_folder() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "fileops_folder_user", tenant_id).await;

    // Need to create a root folder first
    let root_folder = rustshare_core::domain::Folder::new_root(user.id, tenant_id);
    metadata_store
        .create_folder(&root_folder)
        .await
        .expect("Failed to create root folder");

    let parent_folder = rustshare_core::domain::Folder::new_child(
        "Documents".to_string(),
        "/Documents".to_string(),
        root_folder.id,
        user.id,
        tenant_id,
    );

    metadata_store
        .create_folder(&parent_folder)
        .await
        .expect("Failed to create parent folder");

    // Create FileService
    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    // Upload file to parent folder
    let file_content = Bytes::from("Document content in folder");
    let file_name = "doc-in-folder.txt".to_string();

    let uploaded_file = file_service
        .upload_file(
            user.id,
            file_name.clone(),
            Some(parent_folder.id),
            file_content.clone(),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file to folder");

    // Verify file is in the correct folder
    assert_eq!(uploaded_file.parent_folder_id, Some(parent_folder.id));
    assert_eq!(uploaded_file.path, "/Documents/doc-in-folder.txt");

    // Get file metadata
    let retrieved_file = file_service
        .get_file(uploaded_file.id, user.id)
        .await
        .expect("Failed to get file");

    assert_eq!(retrieved_file.parent_folder_id, Some(parent_folder.id));

    // Cleanup
    file_service
        .delete_file(uploaded_file.id, user.id)
        .await
        .expect("Failed to delete file");

    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(parent_folder.id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup parent folder");

    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(root_folder.id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup root folder");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_file_deduplication() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "fileops_dedup_user", tenant_id).await;

    // Create FileService
    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    // Upload same content twice with different names
    let file_content = Bytes::from("Identical content for deduplication test");

    let file1 = file_service
        .upload_file(
            user.id,
            "file1.txt".to_string(),
            None,
            file_content.clone(),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload first file");

    let file2 = file_service
        .upload_file(
            user.id,
            "file2.txt".to_string(),
            None,
            file_content.clone(),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload second file");

    // Both files should have the same content hash (deduplication)
    assert_eq!(file1.content_hash, file2.content_hash);
    assert_ne!(file1.id, file2.id); // But different IDs
    assert_ne!(file1.name, file2.name); // And different names

    // Both should have the same storage key
    assert_eq!(file1.storage_key(), file2.storage_key());

    // Cleanup
    file_service
        .delete_file(file1.id, user.id)
        .await
        .expect("Failed to delete file1");
    file_service
        .delete_file(file2.id, user.id)
        .await
        .expect("Failed to delete file2");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_move_file_to_folder() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "fileops_move_user", tenant_id).await;

    // Create root folder
    let root_folder = rustshare_core::domain::Folder::new_root(user.id, tenant_id);
    metadata_store
        .create_folder(&root_folder)
        .await
        .expect("Failed to create root folder");

    // Create target folder
    let target_folder = rustshare_core::domain::Folder::new_child(
        "Target".to_string(),
        "/Target".to_string(),
        root_folder.id,
        user.id,
        tenant_id,
    );
    metadata_store
        .create_folder(&target_folder)
        .await
        .expect("Failed to create target folder");

    // Create FileService
    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    // Upload a file at root (no parent folder)
    let file_content = Bytes::from("File to be moved");
    let uploaded_file = file_service
        .upload_file(
            user.id,
            "moveme.txt".to_string(),
            None,
            file_content.clone(),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file");

    // Verify file is at root
    assert_eq!(uploaded_file.parent_folder_id, None);
    assert_eq!(uploaded_file.path, "/moveme.txt");

    // Move file to target folder
    let moved_file = file_service
        .move_file(uploaded_file.id, Some(target_folder.id), user.id)
        .await
        .expect("Failed to move file");

    // Verify file is now in target folder
    assert_eq!(moved_file.parent_folder_id, Some(target_folder.id));
    assert_eq!(moved_file.path, "/Target/moveme.txt");
    assert_eq!(moved_file.name, "moveme.txt"); // Name unchanged

    // Verify by fetching the file again
    let fetched_file = file_service
        .get_file(uploaded_file.id, user.id)
        .await
        .expect("Failed to get file");
    assert_eq!(fetched_file.parent_folder_id, Some(target_folder.id));
    assert_eq!(fetched_file.path, "/Target/moveme.txt");

    // Cleanup
    file_service
        .delete_file(uploaded_file.id, user.id)
        .await
        .expect("Failed to delete file");

    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(target_folder.id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup target folder");

    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(root_folder.id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup root folder");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_move_file_to_root() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "fileops_move_root_user", tenant_id).await;

    // Create root folder
    let root_folder = rustshare_core::domain::Folder::new_root(user.id, tenant_id);
    metadata_store
        .create_folder(&root_folder)
        .await
        .expect("Failed to create root folder");

    // Create source folder
    let source_folder = rustshare_core::domain::Folder::new_child(
        "Source".to_string(),
        "/Source".to_string(),
        root_folder.id,
        user.id,
        tenant_id,
    );
    metadata_store
        .create_folder(&source_folder)
        .await
        .expect("Failed to create source folder");

    // Create FileService
    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    // Upload a file in source folder
    let file_content = Bytes::from("File to move to root");
    let uploaded_file = file_service
        .upload_file(
            user.id,
            "moveme.txt".to_string(),
            Some(source_folder.id),
            file_content.clone(),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file");

    // Verify file is in source folder
    assert_eq!(uploaded_file.parent_folder_id, Some(source_folder.id));
    assert_eq!(uploaded_file.path, "/Source/moveme.txt");

    // Move file to root (None parent)
    let moved_file = file_service
        .move_file(uploaded_file.id, None, user.id)
        .await
        .expect("Failed to move file to root");

    // Verify file is now at root
    assert_eq!(moved_file.parent_folder_id, None);
    assert_eq!(moved_file.path, "/moveme.txt");

    // Cleanup
    file_service
        .delete_file(uploaded_file.id, user.id)
        .await
        .expect("Failed to delete file");

    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(source_folder.id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup source folder");

    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(root_folder.id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup root folder");

    cleanup_user(&pool, user.id).await;
}
