//! File Lifecycle Contract Tests
//!
//! Tests that verify:
//! - File upload creates all required documents
//! - File identity is stable across operations
//! - Delete creates tombstone
//! - Restore works from tombstone
//! - Version history is preserved

use crate::*;

/// Test FL-01: Upload creates all required documents
///
/// Verify upload creates FileDocument, VersionDocument, and FolderChildrenIndex
#[tokio::test]
async fn test_fl_01_upload_creates_all_required_documents() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "test.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // FileDocument exists
    let file_key = format!("owned/files/{}.json", file.id);
    let file_data = ctx
        .user_buckets
        .get_object(user_id, &file_key)
        .await
        .unwrap();
    assert!(file_data.is_some(), "FileDocument should exist");

    let doc: FileDocument = serde_json::from_slice(&file_data.unwrap()).unwrap();
    assert_eq!(doc.id, file.id);
    assert_eq!(doc.name, "test.txt");
    assert_eq!(doc.owner_id, user_id);

    // VersionDocument exists - find it by listing the file_versions directory
    let versions_prefix = format!("owned/file_versions/{}/", file.id);
    let version_objects = ctx
        .user_buckets
        .list_objects(user_id, &versions_prefix)
        .await
        .unwrap();
    assert!(!version_objects.is_empty(), "FileVersionDocument should exist");

    let version_data = ctx
        .user_buckets
        .get_object(user_id, &version_objects[0])
        .await
        .unwrap()
        .expect("Version should exist");
    let version_doc: FileVersionDocument = serde_json::from_slice(&version_data).unwrap();
    assert_eq!(version_doc.file_id, file.id);
    assert_eq!(version_doc.version_number, 1);

    // Note: Event documents are not currently created by services
    // This is a known limitation in the current implementation
    
    // Root files don't have a FolderChildrenIndex (only files in folders do)
    // The UserRootsIndex tracks root files instead
    let objects = ctx.list_bucket_objects(user_id).await.unwrap();
    assert!(
        objects.iter().any(|k| k.contains("indexes/owned/roots")),
        "UserRootsIndex should be created/updated. Objects: {:?}",
        objects
    );
}

/// Test FL-02: File identity is stable
///
/// Verify file ID doesn't change during rename/move operations
#[tokio::test]
async fn test_fl_02_file_identity_stable() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "original.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();
    let original_id = file.id;

    // Rename
    let renamed = ctx
        .file_service()
        .rename_file(user_id, file.id, "renamed.txt".to_string())
        .await
        .unwrap();
    assert_eq!(renamed.id, original_id, "ID should not change after rename");
    assert_eq!(renamed.name, "renamed.txt");

    // Create folder and move
    let folder = ctx
        .folder_service()
        .create_folder(user_id, "Folder".to_string(), None)
        .await
        .unwrap();
    let moved = ctx
        .file_service()
        .move_file(user_id, file.id, Some(folder.id))
        .await
        .unwrap();
    assert_eq!(moved.id, original_id, "ID should not change after move");
    assert_eq!(moved.parent_folder_id, Some(folder.id));

    // Verify same file can still be retrieved by original ID
    let retrieved = ctx.file_service().get_file(user_id, original_id).await.unwrap();
    assert_eq!(retrieved.id, original_id);
}

/// Test FL-03: Delete creates tombstone
///
/// Verify delete creates TombstoneDocument and marks file as deleted
#[tokio::test]
async fn test_fl_03_delete_creates_tombstone() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "delete_me.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    ctx.file_service().delete_file(user_id, file.id).await.unwrap();

    // Tombstone exists
    let tombstone_key = format!("owned/tombstones/files/{}.json", file.id);
    let tombstone = ctx
        .user_buckets
        .get_object(user_id, &tombstone_key)
        .await
        .unwrap();
    assert!(tombstone.is_some(), "TombstoneDocument should exist");

    let tombstone_doc: TombstoneDocument = serde_json::from_slice(&tombstone.unwrap()).unwrap();
    assert_eq!(tombstone_doc.resource_id, file.id);
    assert!(matches!(tombstone_doc.resource_type, TombstoneResourceType::File));
    assert_eq!(tombstone_doc.deleted_by, user_id);

    // File document still exists but marked deleted
    let file_key = format!("owned/files/{}.json", file.id);
    let file_data = ctx
        .user_buckets
        .get_object(user_id, &file_key)
        .await
        .unwrap();
    assert!(file_data.is_some(), "FileDocument should still exist");

    let doc: FileDocument = serde_json::from_slice(&file_data.unwrap()).unwrap();
    assert!(doc.deleted, "File should be marked as deleted");

    // File should not be retrievable through normal get
    let result = ctx.file_service().get_file(user_id, file.id).await;
    assert!(result.is_err(), "Deleted file should not be accessible");
}

/// Test FL-04: Restore from tombstone
///
/// Verify restore recreates file from tombstone
#[tokio::test]
async fn test_fl_04_restore_from_tombstone() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "restore_me.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();
    let original_id = file.id;
    let original_path = file.path.clone();

    ctx.file_service().delete_file(user_id, file.id).await.unwrap();

    let restored = ctx
        .file_service()
        .restore_file(user_id, file.id)
        .await
        .unwrap();

    assert_eq!(restored.id, original_id, "ID should be preserved");
    assert!(!restored.deleted, "File should not be deleted after restore");
    assert_eq!(restored.path, original_path, "Path should be restored");

    // File should be accessible again
    let retrieved = ctx.file_service().get_file(user_id, file.id).await;
    assert!(retrieved.is_ok(), "Restored file should be accessible");

    // Tombstone should be removed or marked
    let tombstone_key = format!("owned/tombstones/files/{}.json", file.id);
    let tombstone = ctx
        .user_buckets
        .get_object(user_id, &tombstone_key)
        .await
        .unwrap();
    assert!(
        tombstone.is_none() || is_restored_tombstone(&tombstone.unwrap()),
        "Tombstone should be removed or marked as restored"
    );
}

/// Test FL-05: Version history preserved
///
/// Verify all versions are preserved and accessible
#[tokio::test]
async fn test_fl_05_version_history_preserved() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "versioned.txt".to_string(),
            None,
            Bytes::from("v1"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // Update twice
    ctx.file_service()
        .update_file(user_id, file.id, 1, Bytes::from("v2"))
        .await
        .unwrap();
    ctx.file_service()
        .update_file(user_id, file.id, 2, Bytes::from("v3"))
        .await
        .unwrap();

    // List versions
    let versions = ctx
        .file_service()
        .list_versions(user_id, file.id)
        .await
        .unwrap();
    assert_eq!(versions.len(), 3, "Should have 3 versions");
    assert!(
        versions.iter().any(|v| v.version_number == 1),
        "Should have version 1"
    );
    assert!(
        versions.iter().any(|v| v.version_number == 2),
        "Should have version 2"
    );
    assert!(
        versions.iter().any(|v| v.version_number == 3),
        "Should have version 3"
    );

    // Current file should be at version 3
    let current = ctx.file_service().get_file(user_id, file.id).await.unwrap();
    assert_eq!(current.current_version, 3);

    // All version documents should exist in bucket
    for version in &versions {
        let version_key = format!("owned/file_versions/{}/{}.json", file.id, version.id);
        let data = ctx
            .user_buckets
            .get_object(user_id, &version_key)
            .await
            .unwrap();
        assert!(
            data.is_some(),
            "Version {} document should exist",
            version.version_number
        );
    }
}

/// Test FL-06: File rename updates path correctly
///
/// Verify rename updates the path while preserving parent folder
#[tokio::test]
async fn test_fl_06_file_rename_updates_path() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    // Create folder and file inside it
    let folder = ctx
        .folder_service()
        .create_folder(user_id, "Documents".to_string(), None)
        .await
        .unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "oldname.txt".to_string(),
            Some(folder.id),
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(file.path, "/Documents/oldname.txt");

    // Rename
    let renamed = ctx
        .file_service()
        .rename_file(user_id, file.id, "newname.txt".to_string())
        .await
        .unwrap();

    assert_eq!(renamed.name, "newname.txt");
    assert_eq!(renamed.path, "/Documents/newname.txt");
    assert_eq!(renamed.parent_folder_id, Some(folder.id)); // Parent unchanged
}

/// Test FL-07: File move updates parent and path
///
/// Verify move updates both parent folder and path
#[tokio::test]
async fn test_fl_07_file_move_updates_parent_and_path() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let source_folder = ctx
        .folder_service()
        .create_folder(user_id, "Source".to_string(), None)
        .await
        .unwrap();

    let target_folder = ctx
        .folder_service()
        .create_folder(user_id, "Target".to_string(), None)
        .await
        .unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "movable.txt".to_string(),
            Some(source_folder.id),
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(file.parent_folder_id, Some(source_folder.id));
    assert_eq!(file.path, "/Source/movable.txt");

    // Move
    let moved = ctx
        .file_service()
        .move_file(user_id, file.id, Some(target_folder.id))
        .await
        .unwrap();

    assert_eq!(moved.parent_folder_id, Some(target_folder.id));
    assert_eq!(moved.path, "/Target/movable.txt");
}

/// Test FL-08: Folder children index updated on file operations
///
/// Verify folder children index is updated when files are added/removed
#[tokio::test]
async fn test_fl_08_folder_children_index_updated() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let folder = ctx
        .folder_service()
        .create_folder(user_id, "Parent".to_string(), None)
        .await
        .unwrap();

    // Create file
    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "child.txt".to_string(),
            Some(folder.id),
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // Verify index contains file
    let index_key = format!("indexes/folders/{}/children.json", folder.id);
    let index_data = ctx
        .user_buckets
        .get_object(user_id, &index_key)
        .await
        .unwrap()
        .expect("Index should exist");

    let index: FolderChildrenIndex = serde_json::from_slice(&index_data).unwrap();
    assert!(
        index.files.iter().any(|f| f.id == file.id && !f.deleted),
        "Index should contain the file"
    );

    // Delete file
    ctx.file_service().delete_file(user_id, file.id).await.unwrap();

    // Verify index updated
    let index_data = ctx
        .user_buckets
        .get_object(user_id, &index_key)
        .await
        .unwrap()
        .expect("Index should still exist");

    let index: FolderChildrenIndex = serde_json::from_slice(&index_data).unwrap();
    let file_entry = index.files.iter().find(|f| f.id == file.id);
    assert!(
        file_entry.is_some() && file_entry.unwrap().deleted,
        "File should be marked deleted in index"
    );
}

/// Helper function to check if a tombstone is marked as restored
fn is_restored_tombstone(data: &Bytes) -> bool {
    // Check if the tombstone JSON has a "restored_at" field or similar
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(data) {
        json.get("restored_at").is_some() || json.get("is_restored").and_then(|v| v.as_bool()).unwrap_or(false)
    } else {
        false
    }
}
