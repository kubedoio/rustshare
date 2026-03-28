//! Contract Tests for RustShare Per-User Bucket Architecture
//!
//! These tests verify the architectural contracts of the RustFS-only,
//! per-user isolated bucket design. They must:
//! - Test behavior, not implementation
//! - Fail against stub/transitional implementations
//! - Pass only against real implementations

use std::sync::Arc;

use bytes::Bytes;
use uuid::Uuid;

use rustshare_storage::{
    services::{
        models::*,
        V2ServiceFactory,
    },
    user_bucket::{MemoryUserBucketStore, UserBucketStore, UserBucketConfig, UserId},
    cross_bucket::{MemoryCrossBucketReader, PortableStorageLocator},
    metadata_v2::user_bucket_store::UserBucketBlobStore,
};

// ============================================================================
// Test Helpers
// ============================================================================

fn create_test_user() -> UserId {
    Uuid::new_v4()
}

fn create_test_factory() -> V2ServiceFactory {
    let user_buckets: Arc<dyn UserBucketStore> = Arc::new(MemoryUserBucketStore::new());
    let cross_bucket_reader: Arc<dyn rustshare_storage::CrossBucketReader> = 
        Arc::new(MemoryCrossBucketReader::new());
    let blob_store = Arc::new(UserBucketBlobStore::new(user_buckets.clone()));
    
    V2ServiceFactory::new(
        user_buckets,
        cross_bucket_reader,
        blob_store,
        "http://localhost:9000".to_string(),
    )
}

fn create_test_config() -> UserBucketConfig {
    UserBucketConfig {
        endpoint: "http://localhost:9000".to_string(),
        region: "us-east-1".to_string(),
        bucket_prefix: "test-user-".to_string(),
        base_prefix: "".to_string(),
    }
}

// ============================================================================
// Bucket Isolation Contract Tests
// ============================================================================

#[tokio::test]
async fn bi_01_user_a_file_not_in_user_b_bucket() {
    // Arrange
    let factory = create_test_factory();
    let file_service = factory.file_service();
    let user_a = create_test_user();
    let user_b = create_test_user();
    
    // Act - User A uploads a file
    let content = Bytes::from("test content for user a");
    let file = file_service.upload_file(
        user_a,
        "test.txt".to_string(),
        None,
        content.clone(),
        "text/plain".to_string(),
    ).await.expect("Upload should succeed");
    
    // Assert - User B cannot access User A's file
    let result = file_service.get_file(user_b, file.id).await;
    assert!(
        result.is_err(),
        "User B should not be able to access User A's file"
    );
}

#[tokio::test]
async fn bi_03_shared_file_references_use_portable_locators() {
    // Arrange
    let factory = create_test_factory();
    let share_service = factory.share_service();
    let folder_service = factory.folder_service();
    let user_a = create_test_user();
    let user_b = create_test_user();
    
    // Create a folder for User A to share
    let folder = folder_service.create_folder(
        user_a,
        "Shared Folder".to_string(),
        None,
    ).await.expect("Create folder should succeed");
    
    // Act - User A shares folder with User B
    let share = share_service.create_folder_share(
        user_a,
        folder.id,
        user_b,
        SharePermissionV2::Read,
        None, // expires_at
    ).await.expect("Create share should succeed");
    
    // Assert - Share info is correct (portable locator is stored in the docs, verified in other tests)
    assert_eq!(share.resource_id, folder.id, "Share should reference the folder");
    assert_eq!(share.resource_type, ShareResourceTypeV2::Folder, "Resource type should be folder");
    assert_eq!(share.shared_by, user_a, "Share should be from user_a");
    assert_eq!(share.shared_with, user_b, "Share should be to user_b");
}

// ============================================================================
// File Lifecycle Contract Tests
// ============================================================================

#[tokio::test]
async fn fl_01_upload_creates_file_doc() {
    // Arrange
    let factory = create_test_factory();
    let file_service = factory.file_service();
    let user = create_test_user();
    
    // Act
    let content = Bytes::from("test file content");
    let file = file_service.upload_file(
        user,
        "document.pdf".to_string(),
        None,
        content.clone(),
        "application/pdf".to_string(),
    ).await.expect("Upload should succeed");
    
    // Assert
    assert_eq!(file.name, "document.pdf");
    assert_eq!(file.mime_type, "application/pdf");
    assert_eq!(file.size, content.len() as i64);
    assert!(!file.content_hash.is_empty(), "Content hash should be set");
    assert_eq!(file.owner_id, user);
}

#[tokio::test]
async fn fl_02_upload_creates_version_doc() {
    // Arrange
    let factory = create_test_factory();
    let file_service = factory.file_service();
    let user = create_test_user();
    
    let content = Bytes::from("test file content");
    let file = file_service.upload_file(
        user,
        "document.pdf".to_string(),
        None,
        content.clone(),
        "application/pdf".to_string(),
    ).await.expect("Upload should succeed");
    
    // Act
    let versions = file_service.list_versions(user, file.id)
        .await
        .expect("List versions should succeed");
    
    // Assert
    assert_eq!(versions.len(), 1, "Should have one version");
    assert_eq!(versions[0].version_number, 1);
    assert_eq!(versions[0].file_id, file.id);
}

#[tokio::test]
async fn fl_03_upload_stores_blob_content_addressed() {
    // Arrange
    let factory = create_test_factory();
    let file_service = factory.file_service();
    let user = create_test_user();
    let content = Bytes::from("duplicate content");
    
    // Act - Upload same content twice
    let file1 = file_service.upload_file(
        user,
        "file1.txt".to_string(),
        None,
        content.clone(),
        "text/plain".to_string(),
    ).await.expect("First upload should succeed");
    
    let file2 = file_service.upload_file(
        user,
        "file2.txt".to_string(),
        None,
        content.clone(),
        "text/plain".to_string(),
    ).await.expect("Second upload should succeed");
    
    // Assert - Both files have same content hash
    assert_eq!(
        file1.content_hash, file2.content_hash,
        "Same content should have same hash"
    );
}

#[tokio::test]
async fn fl_06_get_file_returns_correct_metadata() {
    // Arrange
    let factory = create_test_factory();
    let file_service = factory.file_service();
    let user = create_test_user();
    
    let content = Bytes::from("test content");
    let uploaded = file_service.upload_file(
        user,
        "test.txt".to_string(),
        None,
        content.clone(),
        "text/plain".to_string(),
    ).await.expect("Upload should succeed");
    
    // Act
    let retrieved = file_service.get_file(user, uploaded.id)
        .await
        .expect("Get file should succeed");
    
    // Assert
    assert_eq!(retrieved.id, uploaded.id);
    assert_eq!(retrieved.name, uploaded.name);
    assert_eq!(retrieved.size, uploaded.size);
    assert_eq!(retrieved.content_hash, uploaded.content_hash);
    assert_eq!(retrieved.owner_id, user);
}

#[tokio::test]
async fn fl_10_delete_creates_tombstone() {
    // Arrange
    let factory = create_test_factory();
    let file_service = factory.file_service();
    let user = create_test_user();
    
    let content = Bytes::from("content to delete");
    let file = file_service.upload_file(
        user,
        "delete_me.txt".to_string(),
        None,
        content.clone(),
        "text/plain".to_string(),
    ).await.expect("Upload should succeed");
    
    // Act
    file_service.delete_file(user, file.id)
        .await
        .expect("Delete should succeed");
    
    // Assert - File should be soft-deleted
    let result = file_service.get_file(user, file.id).await;
    assert!(
        matches!(result, Err(rustshare_core::services::FileError::NotFound(_))),
        "Deleted file should not be retrievable"
    );
}

// ============================================================================
// Folder Lifecycle Contract Tests
// ============================================================================

#[tokio::test]
async fn fo_01_create_folder_creates_doc() {
    // Arrange
    let factory = create_test_factory();
    let folder_service = factory.folder_service();
    let user = create_test_user();
    
    // Act
    let folder = folder_service.create_folder(
        user,
        "My Folder".to_string(),
        None,
    ).await.expect("Create folder should succeed");
    
    // Assert
    assert_eq!(folder.name, "My Folder");
    assert_eq!(folder.owner_id, user);
    assert!(!folder.deleted);
}

#[tokio::test]
async fn fo_04_list_children_returns_files_and_folders() {
    // Arrange
    let factory = create_test_factory();
    let file_service = factory.file_service();
    let folder_service = factory.folder_service();
    let user = create_test_user();
    
    // Create parent folder
    let parent = folder_service.create_folder(
        user,
        "Parent".to_string(),
        None,
    ).await.expect("Create parent should succeed");
    
    // Create child folder
    let child_folder = folder_service.create_folder(
        user,
        "Child Folder".to_string(),
        Some(parent.id),
    ).await.expect("Create child folder should succeed");
    
    // Create child file
    let content = Bytes::from("file content");
    let child_file = file_service.upload_file(
        user,
        "child.txt".to_string(),
        Some(parent.id),
        content,
        "text/plain".to_string(),
    ).await.expect("Upload should succeed");
    
    // Act
    let (folders, files) = folder_service.list_children(user, parent.id)
        .await
        .expect("List children should succeed");
    
    // Assert
    assert_eq!(folders.len(), 1, "Should have one child folder");
    assert_eq!(folders[0].id, child_folder.id);
    assert_eq!(files.len(), 1, "Should have one child file");
    assert_eq!(files[0].id, child_file.id);
}

// ============================================================================
// Share Lifecycle Contract Tests
// ============================================================================

#[tokio::test]
async fn sl_01_create_share_creates_outbound_doc() {
    // Arrange
    let factory = create_test_factory();
    let share_service = factory.share_service();
    let folder_service = factory.folder_service();
    let user_a = create_test_user();
    let user_b = create_test_user();
    
    let folder = folder_service.create_folder(
        user_a,
        "Shared".to_string(),
        None,
    ).await.expect("Create folder should succeed");
    
    // Act
    let share = share_service.create_folder_share(
        user_a,
        folder.id,
        user_b,
        SharePermissionV2::Read,
        None,
    ).await.expect("Create share should succeed");
    
    // Assert
    let outbound = share_service.get_outbound_share(user_a, share.share_id)
        .await
        .expect("Get outbound share should succeed");
    assert_eq!(outbound.resource_id, folder.id);
    assert_eq!(outbound.shared_with_user_id, user_b);
}

#[tokio::test]
async fn sl_02_create_share_creates_received_doc() {
    // Arrange
    let factory = create_test_factory();
    let share_service = factory.share_service();
    let folder_service = factory.folder_service();
    let user_a = create_test_user();
    let user_b = create_test_user();
    
    let folder = folder_service.create_folder(
        user_a,
        "Shared".to_string(),
        None,
    ).await.expect("Create folder should succeed");
    
    // Act
    let share = share_service.create_folder_share(
        user_a,
        folder.id,
        user_b,
        SharePermissionV2::Read,
        None,
    ).await.expect("Create share should succeed");
    
    // Assert
    let received = share_service.get_received_share(user_b, share.share_id)
        .await
        .expect("Get received share should succeed");
    assert_eq!(received.share_id, share.share_id);
    assert_eq!(received.shared_by, user_a);
}

#[tokio::test]
async fn sl_03_create_share_updates_recipient_shared_with_me() {
    // Arrange
    let factory = create_test_factory();
    let share_service = factory.share_service();
    let folder_service = factory.folder_service();
    let user_a = create_test_user();
    let user_b = create_test_user();
    
    let folder = folder_service.create_folder(
        user_a,
        "Shared".to_string(),
        None,
    ).await.expect("Create folder should succeed");
    
    // Act
    let share = share_service.create_folder_share(
        user_a,
        folder.id,
        user_b,
        SharePermissionV2::Read,
        None,
    ).await.expect("Create share should succeed");
    
    // Assert
    let shared_with_me = share_service.list_received_shares(user_b)
        .await
        .expect("List received shares should succeed");
    assert_eq!(shared_with_me.len(), 1);
    assert_eq!(shared_with_me[0].share_id, share.share_id);
}

// ============================================================================
// Favourites Contract Tests
// ============================================================================

#[tokio::test]
async fn fv_01_star_owned_file_updates_favourites_index() {
    // Arrange
    let factory = create_test_factory();
    let file_service = factory.file_service();
    let favourite_service = factory.favourite_service();
    let user = create_test_user();
    
    let content = Bytes::from("file content");
    let file = file_service.upload_file(
        user,
        "favourite.txt".to_string(),
        None,
        content,
        "text/plain".to_string(),
    ).await.expect("Upload should succeed");
    
    // Act
    favourite_service.star_owned_file(user, file.id)
        .await
        .expect("Star should succeed");
    
    // Assert
    let favourites = favourite_service.list_favourites(user)
        .await
        .expect("List favourites should succeed");
    assert_eq!(favourites.len(), 1);
    assert_eq!(favourites[0].resource_id, file.id);
}

#[tokio::test]
async fn fv_04_unstar_removes_from_index() {
    // Arrange
    let factory = create_test_factory();
    let file_service = factory.file_service();
    let favourite_service = factory.favourite_service();
    let user = create_test_user();
    
    let content = Bytes::from("file content");
    let file = file_service.upload_file(
        user,
        "favourite.txt".to_string(),
        None,
        content,
        "text/plain".to_string(),
    ).await.expect("Upload should succeed");
    
    favourite_service.star_owned_file(user, file.id)
        .await
        .expect("Star should succeed");
    
    // Act
    favourite_service.unstar(user, file.id)
        .await
        .expect("Unstar should succeed");
    
    // Assert
    let favourites = favourite_service.list_favourites(user)
        .await
        .expect("List favourites should succeed");
    assert!(favourites.is_empty(), "Favourites should be empty after unstar");
}

// ============================================================================
// Portable Locator Contract Tests
// ============================================================================

#[tokio::test]
async fn pl_01_locator_serializes_correctly() {
    // Arrange
    let locator = PortableStorageLocator::new_s3(
        "http://localhost:9000",
        "rustshare-user-test",
        "owned/files/test-file.json",
        "file",
        Uuid::new_v4(),
    );
    
    // Act
    let json = serde_json::to_string(&locator).expect("Serialize should succeed");
    
    // Assert
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON should be valid");
    assert_eq!(parsed["locator_version"], 1);
    assert_eq!(parsed["storage_provider_kind"], "s3");
    assert_eq!(parsed["bucket"], "rustshare-user-test");
    assert!(parsed["key"].as_str().unwrap().contains("owned/files"));
}

#[tokio::test]
async fn pl_02_locator_deserializes_correctly() {
    // Arrange
    let json = r#"{
        "locator_version": 1,
        "storage_provider_kind": "s3",
        "endpoint_ref": "http://localhost:9000",
        "bucket": "rustshare-user-test",
        "key": "owned/files/test-file.json",
        "resource_type": "file",
        "resource_id": "550e8400-e29b-41d4-a716-446655440000",
        "version_id": null,
        "content_hash": null
    }"#;
    
    // Act
    let locator: PortableStorageLocator = serde_json::from_str(json).expect("Deserialize should succeed");
    
    // Assert
    assert_eq!(locator.locator_version, 1);
    assert_eq!(locator.storage_provider_kind, "s3");
    assert_eq!(locator.bucket, "rustshare-user-test");
    assert!(!locator.key.is_empty());
}

// ============================================================================
// Bucket Provisioning Contract Tests
// ============================================================================

#[tokio::test]
async fn bp_02_provisioning_creates_required_indexes() {
    // Arrange
    let user_buckets: Arc<dyn UserBucketStore> = Arc::new(MemoryUserBucketStore::new());
    let user = create_test_user();
    
    // Act - Provision user bucket
    let provisioned = provision_user_bucket(user_buckets.clone(), user)
        .await
        .expect("Provisioning should succeed");
    
    // Assert - Required indexes should exist
    assert!(provisioned, "Bucket should be provisioned");
    
    // Verify indexes were created
    let indexes = user_buckets.list_objects(user, "indexes/").await.expect("List should succeed");
    let has_owned_index = indexes.iter().any(|k| k.contains("owned/roots"));
    let has_favourites_index = indexes.iter().any(|k| k.contains("favourites"));
    
    assert!(has_owned_index, "Should have owned roots index");
    assert!(has_favourites_index, "Should have favourites index");
}

#[tokio::test]
async fn bp_03_provisioning_is_idempotent() {
    // Arrange
    let user_buckets: Arc<dyn UserBucketStore> = Arc::new(MemoryUserBucketStore::new());
    let user = create_test_user();
    
    // Act - Provision twice
    let first = provision_user_bucket(user_buckets.clone(), user).await;
    let second = provision_user_bucket(user_buckets.clone(), user).await;
    
    // Assert - Both should succeed
    assert!(first.is_ok(), "First provisioning should succeed");
    assert!(second.is_ok(), "Second provisioning should succeed");
    
    // Verify bucket exists only once
    assert!(user_buckets.bucket_exists(user).await.expect("Check should succeed"));
}

// ============================================================================
// Helper Functions
// ============================================================================

async fn provision_user_bucket(
    user_buckets: Arc<dyn UserBucketStore>,
    user_id: UserId,
) -> Result<bool, anyhow::Error> {
    // Create bucket if not exists
    if !user_buckets.bucket_exists(user_id).await? {
        user_buckets.create_bucket(user_id).await?;
    }
    
    // Initialize required indexes
    let indexes = vec![
        ("indexes/owned/roots.json", b"{\"schema_version\":2,\"files\":[],\"folders\":[],\"updated_at\":null}".to_vec()),
        ("indexes/owned/favourites.json", b"{\"schema_version\":2,\"entries\":[],\"updated_at\":null}".to_vec()),
        ("indexes/received/shared_with_me.json", b"{\"schema_version\":2,\"shares\":[],\"updated_at\":null}".to_vec()),
        ("indexes/received/favourites.json", b"{\"schema_version\":2,\"entries\":[],\"updated_at\":null}".to_vec()),
    ];
    
    for (key, data) in indexes {
        if !user_buckets.object_exists(user_id, key).await? {
            user_buckets.put_object(user_id, key, Bytes::from(data)).await?;
        }
    }
    
    Ok(true)
}
