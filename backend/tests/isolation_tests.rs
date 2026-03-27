//! User Bucket Isolation Contract Tests
//!
//! Tests that verify:
//! - User data is isolated to their own bucket
//! - Recipient-side state is in recipient's bucket
//! - Favourites are user-specific
//! - No central database is required

use crate::*;

/// Test UB-01: Create file writes to correct user bucket only
///
/// Verify that creating a file writes canonical documents only to the owner's bucket
/// and not to any other user's bucket.
#[tokio::test]
async fn test_ub_01_create_file_writes_to_owner_bucket_only() {
    // Arrange
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let other_user_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(other_user_id).await.unwrap();

    // Act
    let file = ctx
        .file_service()
        .upload_file(
            owner_id,
            "test.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await;

    // Assert - file created successfully
    assert!(file.is_ok(), "File upload failed: {:?}", file.err());
    let file = file.unwrap();

    // File document exists in owner's bucket
    let owner_objects = ctx.list_bucket_objects(owner_id).await.unwrap();
    assert!(
        owner_objects.iter().any(|k| k.contains(&file.id.to_string())),
        "File document should exist in owner's bucket. Objects: {:?}",
        owner_objects
    );

    // File document does NOT exist in other user's bucket
    let other_objects = ctx.list_bucket_objects(other_user_id).await.unwrap();
    assert!(
        !other_objects.iter().any(|k| k.contains(&file.id.to_string())),
        "File document should NOT exist in other user's bucket"
    );

    // Verify file is in the correct path in owner's bucket
    let file_key = format!("owned/files/{}.json", file.id);
    let file_data = ctx
        .user_buckets
        .get_object(owner_id, &file_key)
        .await
        .unwrap();
    assert!(
        file_data.is_some(),
        "File should be stored at {}",
        file_key
    );
}

/// Test UB-02: Recipient share reference in recipient bucket
///
/// Verify that when a share is created, the recipient gets a reference
/// in their own bucket, and the owner has the outbound share document.
#[tokio::test]
async fn test_ub_02_recipient_share_reference_in_recipient_bucket() {
    // Arrange
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            owner_id,
            "shared.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // Act
    let share = ctx
        .share_service()
        .create_share(
            owner_id,
            recipient_id,
            file.id,
            ShareResourceTypeV2::File,
            SharePermissionV2::Read,
            None,
        )
        .await;

    // Assert - share created successfully
    assert!(share.is_ok(), "Share creation failed: {:?}", share.err());
    let share = share.unwrap();

    // Outbound share in owner's bucket
    let owner_objects = ctx.list_bucket_objects(owner_id).await.unwrap();
    assert!(
        owner_objects
            .iter()
            .any(|k| k.contains(&format!("shares/outbound/{}", share.share_id))),
        "Outbound share document should exist in owner's bucket. Objects: {:?}",
        owner_objects
    );

    // Received share reference in recipient's bucket
    let recipient_objects = ctx.list_bucket_objects(recipient_id).await.unwrap();
    assert!(
        recipient_objects
            .iter()
            .any(|k| k.contains(&format!("received/shares/{}", share.share_id))),
        "Received share reference should exist in recipient's bucket. Objects: {:?}",
        recipient_objects
    );

    // Verify the documents exist at the expected paths
    let outbound_key = format!("owned/shares/outbound/{}.json", share.share_id);
    let received_key = format!("received/shares/{}.json", share.share_id);

    let outbound_data = ctx
        .user_buckets
        .get_object(owner_id, &outbound_key)
        .await
        .unwrap();
    assert!(
        outbound_data.is_some(),
        "Outbound share should be at {}",
        outbound_key
    );

    let received_data = ctx
        .user_buckets
        .get_object(recipient_id, &received_key)
        .await
        .unwrap();
    assert!(
        received_data.is_some(),
        "Received share should be at {}",
        received_key
    );

    // Verify the received share has a valid locator
    let received_doc: ReceivedShareReference =
        serde_json::from_slice(&received_data.unwrap()).unwrap();
    assert_eq!(received_doc.share_id, share.share_id);
    assert_eq!(received_doc.owner_user_id, owner_id);
    assert!(
        received_doc.resource_locator.bucket.contains(&owner_id.to_string()),
        "Locator should point to owner's bucket"
    );
}

/// Test UB-03: Favourites in user's bucket only
///
/// Verify that starring a file only writes to the starring user's bucket
/// and not to the owner's bucket (for shared files).
#[tokio::test]
async fn test_ub_03_favourites_in_user_bucket_only() {
    // Arrange
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let starrer_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(starrer_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            owner_id,
            "starred.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // Create share so starrer can see the file
    let share = ctx
        .share_service()
        .create_share(
            owner_id,
            starrer_id,
            file.id,
            ShareResourceTypeV2::File,
            SharePermissionV2::Read,
            None,
        )
        .await
        .unwrap();

    // Act - favourite the shared file (resource_id is the file id, not share id)
    let result = ctx
        .favourite_service()
        .add_favourite(starrer_id, file.id, FavouriteResourceType::ReceivedFile)
        .await;

    // Assert
    assert!(result.is_ok(), "Starring failed: {:?}", result.err());

    // Favourites index in starrer's bucket
    let starrer_objects = ctx.list_bucket_objects(starrer_id).await.unwrap();
    assert!(
        starrer_objects.iter().any(|k| k.contains("favourites")),
        "Favourites index should exist in starrer's bucket. Objects: {:?}",
        starrer_objects
    );

    // NO favourites in owner's bucket
    let owner_objects = ctx.list_bucket_objects(owner_id).await.unwrap();
    assert!(
        !owner_objects.iter().any(|k| k.contains("favourites")),
        "Favourites should NOT exist in owner's bucket"
    );

    // Verify the file document in owner's bucket is unchanged (no favourites field)
    let file_key = format!("owned/files/{}.json", file.id);
    let file_data = ctx
        .user_buckets
        .get_object(owner_id, &file_key)
        .await
        .unwrap()
        .expect("File should exist");
    let file_doc: FileDocument = serde_json::from_slice(&file_data).unwrap();

    // File document should not have any favourites-related fields
    // (verified by the FileDocument schema not having such fields)
    assert_eq!(file_doc.version_number, 1, "File version should not change when starred by recipient");
}

/// Test UB-04: No central database required
///
/// Verify that operations work without PostgreSQL connection.
/// This test uses only object-store-based services.
#[tokio::test]
async fn test_ub_04_no_central_database_required() {
    // This test context has no PostgreSQL
    let ctx = TestContext::new_without_postgres().await;

    // Should be able to perform all core operations
    let user_id = Uuid::new_v4();
    let result = ctx.create_user(user_id).await;
    assert!(result.is_ok(), "User creation should work without PostgreSQL");

    let file_result = ctx
        .file_service()
        .upload_file(
            user_id,
            "test.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await;

    assert!(
        file_result.is_ok(),
        "File upload should work without PostgreSQL: {:?}",
        file_result.err()
    );
    let file = file_result.unwrap();
    assert_eq!(file.owner_id, user_id);

    // Verify we can read the file back
    let retrieved = ctx.file_service().get_file(user_id, file.id).await;
    assert!(
        retrieved.is_ok(),
        "File retrieval should work without PostgreSQL"
    );
    assert_eq!(retrieved.unwrap().name, "test.txt");
}

/// Test UB-05: File content blob isolation
///
/// Verify that while content blobs may be in a shared bucket for deduplication,
/// the file metadata (including content reference) is isolated to the owner's bucket.
#[tokio::test]
async fn test_ub_05_file_content_reference_isolation() {
    // Arrange
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let other_user_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(other_user_id).await.unwrap();

    let content = Bytes::from("shared content for deduplication test");

    // Act - owner uploads file
    let file = ctx
        .file_service()
        .upload_file(
            owner_id,
            "owner_file.txt".to_string(),
            None,
            content.clone(),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // Other user uploads same content
    let other_file = ctx
        .file_service()
        .upload_file(
            other_user_id,
            "other_file.txt".to_string(),
            None,
            content.clone(),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // Assert - both files have same content hash but different metadata
    assert_eq!(
        file.content_hash, other_file.content_hash,
        "Same content should have same checksum"
    );

    // Each file's metadata is in its owner's bucket only
    let owner_file_key = format!("owned/files/{}.json", file.id);
    let other_file_key = format!("owned/files/{}.json", other_file.id);

    // Owner can access their file metadata
    assert!(ctx
        .user_buckets
        .get_object(owner_id, &owner_file_key)
        .await
        .unwrap()
        .is_some());

    // Owner cannot access other user's file metadata (no cross-bucket access)
    assert!(ctx
        .user_buckets
        .get_object(owner_id, &other_file_key)
        .await
        .unwrap()
        .is_none());
}

/// Test UB-06: Folder hierarchy isolation
///
/// Verify that folder hierarchies are isolated to their owner's bucket.
#[tokio::test]
async fn test_ub_06_folder_hierarchy_isolation() {
    // Arrange
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let other_user_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(other_user_id).await.unwrap();

    // Act - create folder hierarchy
    let root = ctx
        .folder_service()
        .create_folder(owner_id, "Root".to_string(), None)
        .await
        .unwrap();

    let child = ctx
        .folder_service()
        .create_folder(owner_id, "Child".to_string(), Some(root.id))
        .await
        .unwrap();

    let grandchild = ctx
        .folder_service()
        .create_folder(owner_id, "Grandchild".to_string(), Some(child.id))
        .await
        .unwrap();

    // Assert - all folders in owner's bucket
    let owner_objects = ctx.list_bucket_objects(owner_id).await.unwrap();
    assert!(owner_objects
        .iter()
        .any(|k| k.contains(&root.id.to_string())));
    assert!(owner_objects
        .iter()
        .any(|k| k.contains(&child.id.to_string())));
    assert!(owner_objects
        .iter()
        .any(|k| k.contains(&grandchild.id.to_string())));

    // No folders in other user's bucket
    let other_objects = ctx.list_bucket_objects(other_user_id).await.unwrap();
    assert!(!other_objects
        .iter()
        .any(|k| k.contains(&root.id.to_string())));
    assert!(!other_objects
        .iter()
        .any(|k| k.contains(&child.id.to_string())));
    assert!(!other_objects
        .iter()
        .any(|k| k.contains(&grandchild.id.to_string())));
}
