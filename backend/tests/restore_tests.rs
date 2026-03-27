//! Restore Independence Contract Tests
//!
//! Tests that verify:
//! - Export produces complete state
//! - Restore works without central DB
//! - Shared-with-me is restored from received shares
//! - Favourites are restored from indexes

use crate::*;

/// Test RI-01: Export produces complete state
///
/// Verify export includes all user state
#[tokio::test]
async fn test_ri_01_export_produces_complete_state() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    // Create various resources
    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "file.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    let folder = ctx
        .folder_service()
        .create_folder(user_id, "Folder".to_string(), None)
        .await
        .unwrap();

    ctx.favourite_service()
        .add_favourite(user_id, file.id, FavouriteResourceType::OwnedFile)
        .await
        .unwrap();

    let export = ctx.export_user_bucket(user_id).await.unwrap();

    // Verify manifest
    assert_eq!(export.manifest.user_id, user_id);
    assert!(export.manifest.schema_versions.contains_key("FileDocument"));

    // Verify objects include all resources
    assert!(
        export.objects.iter().any(|o| o.key.contains(&file.id.to_string())),
        "Export should include file"
    );
    assert!(
        export.objects.iter().any(|o| o.key.contains(&folder.id.to_string())),
        "Export should include folder"
    );
    assert!(
        export.objects.iter().any(|o| o.key.contains("favourites")),
        "Export should include favourites"
    );

    // Verify all objects can be parsed
    for obj in &export.objects {
        if obj.key.ends_with(".json") {
            let _json: serde_json::Value = serde_json::from_slice(&obj.data)
                .expect(&format!("Should be valid JSON: {}", obj.key));
        }
    }
}

/// Test RI-02: Restore without central DB
///
/// Verify restore works without central metadata repository
#[tokio::test]
async fn test_ri_02_restore_without_central_db() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "restore.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    let export = ctx.export_user_bucket(user_id).await.unwrap();

    // Delete bucket and restore
    ctx.delete_user_bucket(user_id).await.unwrap();
    ctx.restore_user_bucket(user_id, &export).await.unwrap();

    // File accessible after restore
    let restored = ctx.file_service().get_file(user_id, file.id).await;
    assert!(restored.is_ok(), "File should be accessible after restore");
    assert_eq!(restored.unwrap().name, "restore.txt");
}

/// Test RI-03: Shared with me restored
///
/// Verify received shares are restored
#[tokio::test]
async fn test_ri_03_shared_with_me_restored() {
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
        .await
        .unwrap();

    // Export recipient's bucket
    let export = ctx.export_user_bucket(recipient_id).await.unwrap();

    // Delete and restore
    ctx.delete_user_bucket(recipient_id).await.unwrap();
    ctx.restore_user_bucket(recipient_id, &export).await.unwrap();

    // Shared with me restored
    let shares = ctx
        .share_service()
        .list_inbound_shares(recipient_id)
        .await
        .unwrap();
    assert!(
        shares.iter().any(|s| s.share_id == share.share_id),
        "Received shares should be restored"
    );
}

/// Test RI-04: Favourites restored from indexes
///
/// Verify favourites are restored from indexes
#[tokio::test]
async fn test_ri_04_favourites_restored_from_indexes() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "fav.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    ctx.favourite_service()
        .add_favourite(user_id, file.id, FavouriteResourceType::OwnedFile)
        .await
        .unwrap();

    let export = ctx.export_user_bucket(user_id).await.unwrap();

    ctx.delete_user_bucket(user_id).await.unwrap();
    ctx.restore_user_bucket(user_id, &export).await.unwrap();

    let favourites = ctx.favourite_service().list_favourites(user_id).await.unwrap();
    assert_eq!(favourites.len(), 1);
    assert_eq!(favourites[0].resource_id, file.id);
}

/// Test RI-05: Folder hierarchy restored
///
/// Verify folder hierarchy is restored correctly
#[tokio::test]
async fn test_ri_05_folder_hierarchy_restored() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    // Create folder hierarchy
    let root = ctx
        .folder_service()
        .create_folder(user_id, "Root".to_string(), None)
        .await
        .unwrap();

    let child = ctx
        .folder_service()
        .create_folder(user_id, "Child".to_string(), Some(root.id))
        .await
        .unwrap();

    let grandchild = ctx
        .folder_service()
        .create_folder(user_id, "Grandchild".to_string(), Some(child.id))
        .await
        .unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "nested.txt".to_string(),
            Some(grandchild.id),
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    let export = ctx.export_user_bucket(user_id).await.unwrap();

    ctx.delete_user_bucket(user_id).await.unwrap();
    ctx.restore_user_bucket(user_id, &export).await.unwrap();

    // Verify all folders restored
    let root_restored = ctx.folder_service().get_folder(user_id, root.id).await;
    assert!(root_restored.is_ok(), "Root folder should be restored");

    let child_restored = ctx.folder_service().get_folder(user_id, child.id).await;
    assert!(child_restored.is_ok(), "Child folder should be restored");
    assert_eq!(child_restored.unwrap().parent_folder_id, Some(root.id));

    let grandchild_restored = ctx.folder_service().get_folder(user_id, grandchild.id).await;
    assert!(grandchild_restored.is_ok(), "Grandchild folder should be restored");
    assert_eq!(grandchild_restored.unwrap().parent_folder_id, Some(child.id));

    // Verify file restored in correct location
    let file_restored = ctx.file_service().get_file(user_id, file.id).await;
    assert!(file_restored.is_ok());
    assert_eq!(file_restored.unwrap().parent_folder_id, Some(grandchild.id));
}

/// Test RI-06: Version history restored
///
/// Verify file version history is restored
#[tokio::test]
async fn test_ri_06_version_history_restored() {
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

    // Create multiple versions
    ctx.file_service()
        .update_file(user_id, file.id, 1, Bytes::from("v2"))
        .await
        .unwrap();

    ctx.file_service()
        .update_file(user_id, file.id, 2, Bytes::from("v3"))
        .await
        .unwrap();

    let export = ctx.export_user_bucket(user_id).await.unwrap();

    ctx.delete_user_bucket(user_id).await.unwrap();
    ctx.restore_user_bucket(user_id, &export).await.unwrap();

    // Verify versions restored
    let versions = ctx
        .file_service()
        .list_versions(user_id, file.id)
        .await
        .unwrap();
    assert_eq!(versions.len(), 3, "All versions should be restored");
}
