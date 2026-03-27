//! Folder Lifecycle Contract Tests
//!
//! Tests that verify:
//! - Folder creation creates correct documents
//! - Folder hierarchy is maintained
//! - Delete creates tombstone
//! - Restore works from tombstone

use crate::*;

/// Test FO-01: Create folder creates documents
///
/// Verify folder creation creates FolderDocument and updates indexes
#[tokio::test]
async fn test_fo_01_create_folder_creates_documents() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let folder = ctx
        .folder_service()
        .create_folder(user_id, "TestFolder".to_string(), None)
        .await
        .unwrap();

    // FolderDocument exists
    let folder_key = format!("owned/folders/{}.json", folder.id);
    let folder_data = ctx
        .user_buckets
        .get_object(user_id, &folder_key)
        .await
        .unwrap();
    assert!(folder_data.is_some(), "FolderDocument should exist");

    let doc: FolderDocument = serde_json::from_slice(&folder_data.unwrap()).unwrap();
    assert_eq!(doc.id, folder.id);
    assert_eq!(doc.name, "TestFolder");
    assert_eq!(doc.owner_id, user_id);
    assert!(doc.parent_folder_id.is_none()); // Root folder

    // Note: Event documents are not currently created by services
    // This is a known limitation in the current implementation
    
    // Roots index updated
    let objects = ctx.list_bucket_objects(user_id).await.unwrap();
    assert!(
        objects.iter().any(|k| k.contains("roots")),
        "UserRootsIndex should be updated. Objects: {:?}",
        objects
    );
}

/// Test FO-02: Folder hierarchy maintained
///
/// Verify parent-child relationships are preserved
#[tokio::test]
async fn test_fo_02_folder_hierarchy_maintained() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let parent = ctx
        .folder_service()
        .create_folder(user_id, "Parent".to_string(), None)
        .await
        .unwrap();

    let child = ctx
        .folder_service()
        .create_folder(user_id, "Child".to_string(), Some(parent.id))
        .await
        .unwrap();

    let grandchild = ctx
        .folder_service()
        .create_folder(user_id, "Grandchild".to_string(), Some(child.id))
        .await
        .unwrap();

    // Verify parent relationships
    assert!(parent.parent_folder_id.is_none());
    assert_eq!(child.parent_folder_id, Some(parent.id));
    assert_eq!(grandchild.parent_folder_id, Some(child.id));

    // Verify paths
    assert_eq!(parent.path, "/Parent");
    assert_eq!(child.path, "/Parent/Child");
    assert_eq!(grandchild.path, "/Parent/Child/Grandchild");
}

/// Test FO-03: Rename folder updates path
///
/// Verify folder rename updates the path
#[tokio::test]
async fn test_fo_03_rename_folder_updates_path() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let folder = ctx
        .folder_service()
        .create_folder(user_id, "OldName".to_string(), None)
        .await
        .unwrap();

    assert_eq!(folder.path, "/OldName");

    let renamed = ctx
        .folder_service()
        .rename_folder(user_id, folder.id, "NewName".to_string())
        .await
        .unwrap();

    assert_eq!(renamed.name, "NewName");
    assert_eq!(renamed.path, "/NewName");
}

/// Test FO-04: Move folder updates parent
///
/// Verify folder move updates parent and path
#[tokio::test]
async fn test_fo_04_move_folder_updates_parent() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let source = ctx
        .folder_service()
        .create_folder(user_id, "Source".to_string(), None)
        .await
        .unwrap();

    let target = ctx
        .folder_service()
        .create_folder(user_id, "Target".to_string(), None)
        .await
        .unwrap();

    let folder = ctx
        .folder_service()
        .create_folder(user_id, "Movable".to_string(), Some(source.id))
        .await
        .unwrap();

    assert_eq!(folder.parent_folder_id, Some(source.id));
    assert_eq!(folder.path, "/Source/Movable");

    let moved = ctx
        .folder_service()
        .move_folder(user_id, folder.id, Some(target.id))
        .await
        .unwrap();

    assert_eq!(moved.parent_folder_id, Some(target.id));
    assert_eq!(moved.path, "/Target/Movable");
}

/// Test FO-05: Delete folder creates tombstone
///
/// Verify delete creates TombstoneDocument
#[tokio::test]
async fn test_fo_05_delete_folder_creates_tombstone() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let folder = ctx
        .folder_service()
        .create_folder(user_id, "DeleteMe".to_string(), None)
        .await
        .unwrap();

    ctx.folder_service()
        .delete_folder(user_id, folder.id)
        .await
        .unwrap();

    // Tombstone exists
    let tombstone_key = format!("owned/tombstones/folders/{}.json", folder.id);
    let tombstone = ctx
        .user_buckets
        .get_object(user_id, &tombstone_key)
        .await
        .unwrap();
    assert!(
        tombstone.is_some(),
        "TombstoneDocument should exist for folder"
    );

    let doc: TombstoneDocument = serde_json::from_slice(&tombstone.unwrap()).unwrap();
    assert_eq!(doc.resource_id, folder.id);
    assert!(matches!(doc.resource_type, TombstoneResourceType::Folder));

    // Folder marked deleted
    let folder_key = format!("owned/folders/{}.json", folder.id);
    let folder_data = ctx
        .user_buckets
        .get_object(user_id, &folder_key)
        .await
        .unwrap()
        .unwrap();
    let doc: FolderDocument = serde_json::from_slice(&folder_data).unwrap();
    assert!(doc.deleted);
}

/// Test FO-06: Restore folder from tombstone
///
/// Verify restore recreates folder from tombstone
#[tokio::test]
async fn test_fo_06_restore_folder_from_tombstone() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let folder = ctx
        .folder_service()
        .create_folder(user_id, "RestoreMe".to_string(), None)
        .await
        .unwrap();
    let original_id = folder.id;

    ctx.folder_service()
        .delete_folder(user_id, folder.id)
        .await
        .unwrap();

    let restored = ctx
        .folder_service()
        .restore_folder(user_id, folder.id)
        .await
        .unwrap();

    assert_eq!(restored.id, original_id);
    assert!(!restored.deleted);
    assert_eq!(restored.name, "RestoreMe");

    // Can access again
    let retrieved = ctx.folder_service().get_folder(user_id, folder.id).await;
    assert!(retrieved.is_ok());
}

/// Test FO-07: Nested folder contents move with parent
///
/// Verify that moving a folder also moves its contents
#[tokio::test]
async fn test_fo_07_nested_contents_move_with_parent() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let root = ctx
        .folder_service()
        .create_folder(user_id, "Root".to_string(), None)
        .await
        .unwrap();

    let target = ctx
        .folder_service()
        .create_folder(user_id, "Target".to_string(), None)
        .await
        .unwrap();

    let child_folder = ctx
        .folder_service()
        .create_folder(user_id, "ChildFolder".to_string(), Some(root.id))
        .await
        .unwrap();

    let child_file = ctx
        .file_service()
        .upload_file(
            user_id,
            "child.txt".to_string(),
            Some(child_folder.id),
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // Verify initial paths
    assert!(child_folder.path.starts_with("/Root/"));
    assert!(child_file.path.starts_with("/Root/"));

    // Move root folder
    let moved_root = ctx
        .folder_service()
        .move_folder(user_id, root.id, Some(target.id))
        .await
        .unwrap();

    // Verify root moved
    assert_eq!(moved_root.parent_folder_id, Some(target.id));

    // Child paths should be updated (either immediately or lazily)
    let child_folder_after = ctx
        .folder_service()
        .get_folder(user_id, child_folder.id)
        .await
        .unwrap();
    let child_file_after = ctx
        .file_service()
        .get_file(user_id, child_file.id)
        .await
        .unwrap();

    assert!(
        child_folder_after.path.starts_with("/Target/"),
        "Child folder path should be updated: {}",
        child_folder_after.path
    );
    assert!(
        child_file_after.path.starts_with("/Target/"),
        "Child file path should be updated: {}",
        child_file_after.path
    );
}
