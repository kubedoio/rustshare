//! Integration test: Conflict Detection Flow (Task 26)
//!
//! Tests optimistic locking for concurrent updates:
//! 1. Upload file (v1)
//! 2. Two concurrent update attempts
//! 3. One succeeds (v2)
//! 4. Other gets 409 Conflict error
//!
//! These tests require a running database and S3-compatible storage.
//! Run with: cargo test --test conflict_detection -- --ignored

use bytes::Bytes;
use rustshare_core::domain::User;
use rustshare_core::services::{FileError, FileService};
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
async fn test_optimistic_locking_conflict() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "conflict_user", tenant_id).await;

    // Create FileService
    let file_service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
    );

    // Step 1: Upload file (v1)
    let initial_content = Bytes::from("Initial version 1 content");
    let file_name = "versioned-doc.txt".to_string();

    let file_v1 = file_service
        .upload_file(
            user.id,
            file_name.clone(),
            None,
            initial_content.clone(),
            "text/plain".to_string(),
        )
        .await
        .expect("Failed to upload initial file");

    assert_eq!(file_v1.current_version, 1);

    // Step 2: First update succeeds (v1 -> v2)
    let update1_content = Bytes::from("Updated content - attempt 1");

    let file_v2 = file_service
        .update_file(
            file_v1.id,
            user.id,
            Some(1), // Expected version v1
            update1_content.clone(),
            None,
        )
        .await
        .expect("First update should succeed");

    assert_eq!(file_v2.current_version, 2);
    assert_ne!(file_v2.content_hash, file_v1.content_hash);

    // Step 3: Second update with stale version fails (409 Conflict)
    let update2_content = Bytes::from("Updated content - attempt 2 (should fail)");

    let result = file_service
        .update_file(
            file_v1.id,
            user.id,
            Some(1), // Stale version - still expecting v1 but file is now v2
            update2_content.clone(),
            None,
        )
        .await;

    // Should get version conflict error
    assert!(result.is_err(), "Second update with stale version should fail");

    match result {
        Err(FileError::VersionConflict { expected, actual }) => {
            assert_eq!(expected, 1, "Expected version should be 1 (stale)");
            assert_eq!(actual, 2, "Actual version should be 2 (current)");
        }
        other => panic!("Expected VersionConflict error, got: {:?}", other),
    }

    // Step 4: Verify file is still at v2 with first update's content
    let final_file = file_service
        .get_file(file_v1.id, user.id)
        .await
        .expect("Failed to get final file");

    assert_eq!(final_file.current_version, 2);
    assert_eq!(final_file.content_hash, file_v2.content_hash);

    // Cleanup
    file_service
        .delete_file(file_v1.id, user.id)
        .await
        .expect("Failed to delete file");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_successful_sequential_updates() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "sequential_user", tenant_id).await;

    // Create FileService
    let file_service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
    );

    // Upload initial file
    let v1_content = Bytes::from("Version 1");
    let file = file_service
        .upload_file(
            user.id,
            "sequential.txt".to_string(),
            None,
            v1_content,
            "text/plain".to_string(),
        )
        .await
        .expect("Failed to upload file");

    assert_eq!(file.current_version, 1);

    // Update to v2 with correct expected version
    let v2_content = Bytes::from("Version 2");
    let file_v2 = file_service
        .update_file(file.id, user.id, Some(1), v2_content, None)
        .await
        .expect("Failed to update to v2");

    assert_eq!(file_v2.current_version, 2);

    // Update to v3 with correct expected version
    let v3_content = Bytes::from("Version 3");
    let file_v3 = file_service
        .update_file(file.id, user.id, Some(2), v3_content, None)
        .await
        .expect("Failed to update to v3");

    assert_eq!(file_v3.current_version, 3);

    // Verify version history
    let versions = file_service
        .list_file_versions(file.id, user.id)
        .await
        .expect("Failed to list versions");

    assert_eq!(versions.len(), 3);
    assert_eq!(versions[0].version_number, 3); // Newest first
    assert_eq!(versions[1].version_number, 2);
    assert_eq!(versions[2].version_number, 1);

    // Cleanup
    file_service
        .delete_file(file.id, user.id)
        .await
        .expect("Failed to delete file");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_update_without_version_check() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "noversion_user", tenant_id).await;

    // Create FileService
    let file_service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
    );

    // Upload initial file
    let v1_content = Bytes::from("Initial content");
    let file = file_service
        .upload_file(
            user.id,
            "nocheck.txt".to_string(),
            None,
            v1_content,
            "text/plain".to_string(),
        )
        .await
        .expect("Failed to upload file");

    // Update without version check (None) - should always succeed
    let v2_content = Bytes::from("Updated content without version check");
    let file_v2 = file_service
        .update_file(file.id, user.id, None, v2_content.clone(), None)
        .await
        .expect("Update without version check should succeed");

    assert_eq!(file_v2.current_version, 2);

    // Another update without version check - should also succeed
    let v3_content = Bytes::from("Another update without version check");
    let file_v3 = file_service
        .update_file(file.id, user.id, None, v3_content, None)
        .await
        .expect("Second update without version check should succeed");

    assert_eq!(file_v3.current_version, 3);

    // Cleanup
    file_service
        .delete_file(file.id, user.id)
        .await
        .expect("Failed to delete file");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_multiple_conflict_scenarios() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "multiconflict_user", tenant_id).await;

    // Create FileService
    let file_service = FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
    );

    // Upload initial file (v1)
    let file = file_service
        .upload_file(
            user.id,
            "conflicts.txt".to_string(),
            None,
            Bytes::from("v1"),
            "text/plain".to_string(),
        )
        .await
        .expect("Failed to upload file");

    // Successful update to v2
    let _ = file_service
        .update_file(file.id, user.id, Some(1), Bytes::from("v2"), None)
        .await
        .expect("Update to v2 should succeed");

    // Successful update to v3
    let _ = file_service
        .update_file(file.id, user.id, Some(2), Bytes::from("v3"), None)
        .await
        .expect("Update to v3 should succeed");

    // Try to update with v1 - should fail (too stale)
    let result = file_service
        .update_file(file.id, user.id, Some(1), Bytes::from("should fail"), None)
        .await;

    assert!(result.is_err(), "Update with v1 should fail when current is v3");
    match result {
        Err(FileError::VersionConflict { expected, actual }) => {
            assert_eq!(expected, 1);
            assert_eq!(actual, 3);
        }
        _ => panic!("Expected VersionConflict"),
    }

    // Try to update with v2 - should also fail
    let result = file_service
        .update_file(file.id, user.id, Some(2), Bytes::from("should also fail"), None)
        .await;

    assert!(result.is_err(), "Update with v2 should fail when current is v3");
    match result {
        Err(FileError::VersionConflict { expected, actual }) => {
            assert_eq!(expected, 2);
            assert_eq!(actual, 3);
        }
        _ => panic!("Expected VersionConflict"),
    }

    // Update with correct version v3 - should succeed
    let file_v4 = file_service
        .update_file(file.id, user.id, Some(3), Bytes::from("v4"), None)
        .await
        .expect("Update with correct version should succeed");

    assert_eq!(file_v4.current_version, 4);

    // Cleanup
    file_service
        .delete_file(file.id, user.id)
        .await
        .expect("Failed to delete file");

    cleanup_user(&pool, user.id).await;
}
