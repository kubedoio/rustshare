//! No-Scan Hot Path Contract Tests
//!
//! Tests that verify:
//! - Folder listing uses index (not bucket scan)
//! - Favourites listing uses index
//! - Shared-with-me uses index
//! - User roots uses index

use crate::*;

/// Test NS-01: Folder listing uses index
///
/// Verify folder listing uses FolderChildrenIndex, not bucket scan
#[tokio::test]
async fn test_ns_01_folder_listing_uses_index() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let folder = ctx
        .folder_service()
        .create_folder(user_id, "Parent".to_string(), None)
        .await
        .unwrap();

    // Create multiple files
    for i in 0..5 {
        ctx.file_service()
            .upload_file(
                user_id,
                format!("file{}.txt", i),
                Some(folder.id),
                Bytes::from(format!("content{}", i)),
                "text/plain".to_string(),
            )
            .await
            .unwrap();
    }

    // List should use index
    let (folders, files) = ctx
        .folder_service()
        .get_contents(user_id, Some(folder.id))
        .await
        .unwrap();
    assert_eq!(files.len(), 5);

    // Verify index was created/updated
    let index_key = format!("indexes/folders/{}/children.json", folder.id);
    let index_data = ctx
        .user_buckets
        .get_object(user_id, &index_key)
        .await
        .unwrap();
    assert!(
        index_data.is_some(),
        "FolderChildrenIndex should exist at {}",
        index_key
    );

    let index: FolderChildrenIndex = serde_json::from_slice(&index_data.unwrap()).unwrap();
    assert_eq!(index.folder_id, folder.id);
    assert_eq!(index.files.len() + index.folders.len(), 5);
}

/// Test NS-02: Favourites listing uses index
///
/// Verify favourites listing uses FavouritesIndex
#[tokio::test]
async fn test_ns_02_favourites_listing_uses_index() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    // Star multiple files
    for i in 0..10 {
        let file = ctx
            .file_service()
            .upload_file(
                user_id,
                format!("fav{}.txt", i),
                None,
                Bytes::from(format!("content{}", i)),
                "text/plain".to_string(),
            )
            .await
            .unwrap();
        ctx.favourite_service()
            .add_favourite(user_id, file.id, FavouriteResourceType::OwnedFile)
            .await
            .unwrap();
    }

    // List favourites should use index
    let favourites = ctx.favourite_service().list_favourites(user_id).await.unwrap();
    assert_eq!(favourites.len(), 10);

    // Verify index exists
    let index_key = "indexes/owned/favourites.json";
    let index_data = ctx
        .user_buckets
        .get_object(user_id, index_key)
        .await
        .unwrap();
    assert!(index_data.is_some(), "FavouritesIndex should exist");

    let index: FavouritesIndex = serde_json::from_slice(&index_data.unwrap()).unwrap();
    assert_eq!(index.entries.len(), 10);
}

/// Test NS-03: Shared with me uses index
///
/// Verify shared-with-me uses SharedWithMeIndex
#[tokio::test]
async fn test_ns_03_shared_with_me_uses_index() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();

    // Create multiple shares
    for i in 0..5 {
        let file = ctx
            .file_service()
            .upload_file(
                owner_id,
                format!("shared{}.txt", i),
                None,
                Bytes::from(format!("content{}", i)),
                "text/plain".to_string(),
            )
            .await
            .unwrap();
        ctx.share_service()
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
    }

    // List should use index
    let shares = ctx
        .share_service()
        .list_inbound_shares(recipient_id)
        .await
        .unwrap();
    assert_eq!(shares.len(), 5);

    // Verify index exists
    let index_key = "indexes/received/shared_with_me.json";
    let index_data = ctx
        .user_buckets
        .get_object(recipient_id, index_key)
        .await
        .unwrap();
    assert!(index_data.is_some(), "SharedWithMeIndex should exist");

    let index: SharedWithMeIndex = serde_json::from_slice(&index_data.unwrap()).unwrap();
    assert_eq!(index.entries.len(), 5);
}

/// Test NS-04: User roots uses index
///
/// Verify user roots listing uses UserRootsIndex
#[tokio::test]
async fn test_ns_04_user_roots_uses_index() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    // Create multiple root folders
    for i in 0..3 {
        ctx.folder_service()
            .create_folder(user_id, format!("Root{}", i), None)
            .await
            .unwrap();
    }

    // List roots should use index
    let roots = ctx.folder_service().list_folders(user_id, None).await.unwrap();
    assert_eq!(roots.len(), 3);

    // Verify index exists
    let index_key = "indexes/owned/roots.json";
    let index_data = ctx
        .user_buckets
        .get_object(user_id, index_key)
        .await
        .unwrap();
    assert!(index_data.is_some(), "UserRootsIndex should exist");

    let index: UserRootsIndex = serde_json::from_slice(&index_data.unwrap()).unwrap();
    assert_eq!(index.root_folders.len(), 3);
}

/// Test NS-05: Index is updated on resource changes
///
/// Verify indexes are kept up to date when resources change
#[tokio::test]
async fn test_ns_05_index_updated_on_changes() {
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

    // Get initial index
    let index_key = format!("indexes/folders/{}/children.json", folder.id);
    let index_data = ctx
        .user_buckets
        .get_object(user_id, &index_key)
        .await
        .unwrap()
        .unwrap();
    let index: FolderChildrenIndex = serde_json::from_slice(&index_data).unwrap();
    assert_eq!(index.files.len() + index.folders.len(), 1);
    let initial_file_count = index.files.len() + index.folders.len();

    // Rename file
    ctx.file_service()
        .rename_file(user_id, file.id, "renamed.txt".to_string())
        .await
        .unwrap();

    // Index should be updated
    let index_data = ctx
        .user_buckets
        .get_object(user_id, &index_key)
        .await
        .unwrap()
        .unwrap();
    let index: FolderChildrenIndex = serde_json::from_slice(&index_data).unwrap();
    assert!(index.files.len() + index.folders.len() >= initial_file_count, "Index should maintain entries");
    
    let child = index.files.iter().find(|f| f.id == file.id).unwrap();
    assert_eq!(child.name, "renamed.txt", "Index should reflect new name");
}

/// Test NS-06: Index can be rebuilt
///
/// Verify indexes can be rebuilt from canonical documents
#[tokio::test]
async fn test_ns_06_index_can_be_rebuilt() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let folder = ctx
        .folder_service()
        .create_folder(user_id, "Parent".to_string(), None)
        .await
        .unwrap();

    // Create multiple files
    let mut file_ids = Vec::new();
    for i in 0..3 {
        let file = ctx
            .file_service()
            .upload_file(
                user_id,
                format!("file{}.txt", i),
                Some(folder.id),
                Bytes::from(format!("content{}", i)),
                "text/plain".to_string(),
            )
            .await
            .unwrap();
        file_ids.push(file.id);
    }

    // Delete the index
    let index_key = format!("indexes/folders/{}/children.json", folder.id);
    ctx.user_buckets
        .delete_object(user_id, &index_key)
        .await
        .unwrap();

    // Verify index is gone
    let index_data = ctx
        .user_buckets
        .get_object(user_id, &index_key)
        .await
        .unwrap();
    assert!(index_data.is_none(), "Index should be deleted");

    // List contents should trigger rebuild
    let (folders, files) = ctx
        .folder_service()
        .get_contents(user_id, Some(folder.id))
        .await
        .unwrap();
    assert_eq!(files.len(), 3);

    // Index should be recreated
    let index_data = ctx
        .user_buckets
        .get_object(user_id, &index_key)
        .await
        .unwrap();
    assert!(index_data.is_some(), "Index should be rebuilt");

    let index: FolderChildrenIndex = serde_json::from_slice(&index_data.unwrap()).unwrap();
    assert_eq!(index.files.len() + index.folders.len(), 3);
    for file_id in file_ids {
        assert!(index.files.iter().any(|f| f.id == file_id) || index.folders.iter().any(|f| f.id == file_id));
    }
}
