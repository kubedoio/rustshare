//! Integration test: Version Restore Flow (Task 28)
//!
//! Tests version history and restoration:
//! 1. Upload file (v1)
//! 2. Update file (v2)
//! 3. Update file (v3)
//! 4. Restore v1 → creates v4 with v1 content
//! 5. List versions shows all 4 versions
//!
//! These tests require a running database and S3-compatible storage.
//! Run with: cargo test --test version_restore -- --ignored

use bytes::Bytes;
use rustshare_core::domain::User;
use rustshare_core::services::FileService;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Setup test environment with database and S3 connections
async fn setup_test_env() -> (PgPool, Arc<EventStore>, Arc<MetadataStore>, Arc<ObjectStore>) {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let event_store = Arc::new(EventStore::new(pool.clone()));
    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));

    let s3_endpoint = std::env::var("S3_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:9000".to_string());
    let s3_region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    let s3_bucket = std::env::var("S3_BUCKET").unwrap_or_else(|_| "rustshare".to_string());

    let object_store = Arc::new(
        ObjectStore::new(s3_endpoint, s3_region, s3_bucket)
            .await
            .expect("Failed to create object store"),
    );

    (pool, event_store, metadata_store, object_store)
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
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test user");
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_version_restore_flow() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "version_restore_user", tenant_id).await;

    // Create FileService
    let file_service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
    );

    // Step 1: Upload file (v1)
    let v1_content = Bytes::from("Version 1 content - original");
    let file_v1 = file_service
        .upload_file(
            user.id,
            "versioned-doc.txt".to_string(),
            None,
            v1_content.clone(),
            "text/plain".to_string(),
        )
        .await
        .expect("Failed to upload v1");

    assert_eq!(file_v1.current_version, 1);
    let v1_hash = file_v1.content_hash.clone();

    // Step 2: Update file (v2)
    let v2_content = Bytes::from("Version 2 content - first update");
    let file_v2 = file_service
        .update_file(file_v1.id, user.id, Some(1), v2_content.clone(), None)
        .await
        .expect("Failed to update to v2");

    assert_eq!(file_v2.current_version, 2);
    let v2_hash = file_v2.content_hash.clone();
    assert_ne!(v2_hash, v1_hash);

    // Step 3: Update file (v3)
    let v3_content = Bytes::from("Version 3 content - second update");
    let file_v3 = file_service
        .update_file(file_v1.id, user.id, Some(2), v3_content.clone(), None)
        .await
        .expect("Failed to update to v3");

    assert_eq!(file_v3.current_version, 3);
    let v3_hash = file_v3.content_hash.clone();
    assert_ne!(v3_hash, v2_hash);
    assert_ne!(v3_hash, v1_hash);

    // Verify we have 3 versions
    let versions_before_restore = file_service
        .list_file_versions(file_v1.id, user.id)
        .await
        .expect("Failed to list versions");

    assert_eq!(versions_before_restore.len(), 3);
    assert_eq!(versions_before_restore[0].version_number, 3); // Newest first
    assert_eq!(versions_before_restore[1].version_number, 2);
    assert_eq!(versions_before_restore[2].version_number, 1);

    // Step 4: Restore v1 → creates v4 with v1 content
    let file_v4 = file_service
        .restore_file_version(file_v1.id, user.id, 1)
        .await
        .expect("Failed to restore v1");

    assert_eq!(file_v4.current_version, 4);
    assert_eq!(
        file_v4.content_hash, v1_hash,
        "v4 should have same content hash as v1"
    );

    // Step 5: List versions shows all 4 versions
    let versions_after_restore = file_service
        .list_file_versions(file_v1.id, user.id)
        .await
        .expect("Failed to list versions after restore");

    assert_eq!(versions_after_restore.len(), 4);
    assert_eq!(versions_after_restore[0].version_number, 4); // Newest
    assert_eq!(versions_after_restore[1].version_number, 3);
    assert_eq!(versions_after_restore[2].version_number, 2);
    assert_eq!(versions_after_restore[3].version_number, 1); // Oldest

    // Verify content hashes
    assert_eq!(versions_after_restore[0].content_hash, v1_hash); // v4 = v1 content
    assert_eq!(versions_after_restore[1].content_hash, v3_hash);
    assert_eq!(versions_after_restore[2].content_hash, v2_hash);
    assert_eq!(versions_after_restore[3].content_hash, v1_hash);

    // Verify file metadata shows current version as v4
    let current_file = file_service
        .get_file(file_v1.id, user.id)
        .await
        .expect("Failed to get current file");

    assert_eq!(current_file.current_version, 4);
    assert_eq!(current_file.content_hash, v1_hash);

    // Cleanup
    file_service
        .delete_file(file_v1.id, user.id)
        .await
        .expect("Failed to delete file");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_restore_multiple_versions() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "multi_restore_user", tenant_id).await;

    // Create FileService
    let file_service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
    );

    // Upload and create several versions
    let file = file_service
        .upload_file(
            user.id,
            "multi-restore.txt".to_string(),
            None,
            Bytes::from("v1"),
            "text/plain".to_string(),
        )
        .await
        .expect("Failed to upload v1");

    let v1_hash = file.content_hash.clone();

    // Create v2
    let file = file_service
        .update_file(file.id, user.id, None, Bytes::from("v2"), None)
        .await
        .expect("Failed to create v2");
    let v2_hash = file.content_hash.clone();

    // Create v3
    let file = file_service
        .update_file(file.id, user.id, None, Bytes::from("v3"), None)
        .await
        .expect("Failed to create v3");
    let v3_hash = file.content_hash.clone();

    // Restore v2 -> creates v4
    let file = file_service
        .restore_file_version(file.id, user.id, 2)
        .await
        .expect("Failed to restore v2");

    assert_eq!(file.current_version, 4);
    assert_eq!(file.content_hash, v2_hash);

    // Restore v1 -> creates v5
    let file = file_service
        .restore_file_version(file.id, user.id, 1)
        .await
        .expect("Failed to restore v1");

    assert_eq!(file.current_version, 5);
    assert_eq!(file.content_hash, v1_hash);

    // Restore v3 -> creates v6
    let file = file_service
        .restore_file_version(file.id, user.id, 3)
        .await
        .expect("Failed to restore v3");

    assert_eq!(file.current_version, 6);
    assert_eq!(file.content_hash, v3_hash);

    // Verify we have 6 versions total
    let versions = file_service
        .list_file_versions(file.id, user.id)
        .await
        .expect("Failed to list versions");

    assert_eq!(versions.len(), 6);

    // Verify version sequence
    assert_eq!(versions[0].version_number, 6);
    assert_eq!(versions[0].content_hash, v3_hash);

    assert_eq!(versions[1].version_number, 5);
    assert_eq!(versions[1].content_hash, v1_hash);

    assert_eq!(versions[2].version_number, 4);
    assert_eq!(versions[2].content_hash, v2_hash);

    assert_eq!(versions[3].version_number, 3);
    assert_eq!(versions[3].content_hash, v3_hash);

    assert_eq!(versions[4].version_number, 2);
    assert_eq!(versions[4].content_hash, v2_hash);

    assert_eq!(versions[5].version_number, 1);
    assert_eq!(versions[5].content_hash, v1_hash);

    // Cleanup
    file_service
        .delete_file(file.id, user.id)
        .await
        .expect("Failed to delete file");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_restore_same_version_multiple_times() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "same_restore_user", tenant_id).await;

    // Create FileService
    let file_service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
    );

    // Upload initial file
    let file = file_service
        .upload_file(
            user.id,
            "same-restore.txt".to_string(),
            None,
            Bytes::from("Original content"),
            "text/plain".to_string(),
        )
        .await
        .expect("Failed to upload file");

    let original_hash = file.content_hash.clone();

    // Create v2
    let _ = file_service
        .update_file(
            file.id,
            user.id,
            None,
            Bytes::from("Modified content"),
            None,
        )
        .await
        .expect("Failed to create v2");

    // Restore v1 -> creates v3
    let file_v3 = file_service
        .restore_file_version(file.id, user.id, 1)
        .await
        .expect("Failed to restore v1 first time");

    assert_eq!(file_v3.current_version, 3);
    assert_eq!(file_v3.content_hash, original_hash);

    // Restore v1 again -> creates v4
    let file_v4 = file_service
        .restore_file_version(file.id, user.id, 1)
        .await
        .expect("Failed to restore v1 second time");

    assert_eq!(file_v4.current_version, 4);
    assert_eq!(file_v4.content_hash, original_hash);

    // Restore v1 once more -> creates v5
    let file_v5 = file_service
        .restore_file_version(file.id, user.id, 1)
        .await
        .expect("Failed to restore v1 third time");

    assert_eq!(file_v5.current_version, 5);
    assert_eq!(file_v5.content_hash, original_hash);

    // Verify all 5 versions exist
    let versions = file_service
        .list_file_versions(file.id, user.id)
        .await
        .expect("Failed to list versions");

    assert_eq!(versions.len(), 5);

    // Cleanup
    file_service
        .delete_file(file.id, user.id)
        .await
        .expect("Failed to delete file");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_get_specific_version() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "get_version_user", tenant_id).await;

    // Create FileService
    let file_service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
    );

    // Create file with multiple versions
    let file = file_service
        .upload_file(
            user.id,
            "versioned.txt".to_string(),
            None,
            Bytes::from("Version 1"),
            "text/plain".to_string(),
        )
        .await
        .expect("Failed to upload file");

    let v1_hash = file.content_hash.clone();

    file_service
        .update_file(file.id, user.id, None, Bytes::from("Version 2"), None)
        .await
        .expect("Failed to create v2");

    file_service
        .update_file(file.id, user.id, None, Bytes::from("Version 3"), None)
        .await
        .expect("Failed to create v3");

    // Get specific versions
    let version1 = file_service
        .get_file_version(file.id, user.id, 1)
        .await
        .expect("Failed to get version 1");

    let version2 = file_service
        .get_file_version(file.id, user.id, 2)
        .await
        .expect("Failed to get version 2");

    let version3 = file_service
        .get_file_version(file.id, user.id, 3)
        .await
        .expect("Failed to get version 3");

    // Verify version numbers
    assert_eq!(version1.version_number, 1);
    assert_eq!(version2.version_number, 2);
    assert_eq!(version3.version_number, 3);

    // Verify they have different content hashes
    assert_ne!(version1.content_hash, version2.content_hash);
    assert_ne!(version2.content_hash, version3.content_hash);

    // Verify version 1 has the original hash
    assert_eq!(version1.content_hash, v1_hash);

    // Cleanup
    file_service
        .delete_file(file.id, user.id)
        .await
        .expect("Failed to delete file");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_restore_nonexistent_version() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "nonexistent_restore_user", tenant_id).await;

    // Create FileService
    let file_service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
    );

    // Upload file (only v1)
    let file = file_service
        .upload_file(
            user.id,
            "single-version.txt".to_string(),
            None,
            Bytes::from("Only version"),
            "text/plain".to_string(),
        )
        .await
        .expect("Failed to upload file");

    // Try to restore non-existent version 99
    let result = file_service.restore_file_version(file.id, user.id, 99).await;

    assert!(
        result.is_err(),
        "Restoring non-existent version should fail"
    );

    // Cleanup
    file_service
        .delete_file(file.id, user.id)
        .await
        .expect("Failed to delete file");

    cleanup_user(&pool, user.id).await;
}
