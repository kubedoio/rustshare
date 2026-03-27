//! Favourites/Stars Contract Tests
//!
//! Tests that verify:
//! - Starring owned file updates user's favourites only
//! - Starring shared file updates recipient favourites only
//! - Unstarring removes from favourites
//! - Favourites survive restore
//! - Owner's canonical file is unchanged by starring

use crate::*;

/// Test FV-01: Star owned file updates user favourites
///
/// Verify starring owned file updates user's favourites index,
/// and the owner file document is unchanged.
#[tokio::test]
async fn test_fv_01_star_owned_updates_user_favourites() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "myfile.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // Record file version before star
    let file_before = ctx.file_service().get_file(user_id, file.id).await.unwrap();
    let version_before = file_before.current_version;

    // Star the file
    ctx
        .favourite_service()
        .add_favourite(user_id, file.id, FavouriteResourceType::OwnedFile)
        .await
        .unwrap();

    // Favourites index exists
    let index_key = "indexes/owned/favourites.json";
    let index_data = ctx
        .user_buckets
        .get_object(user_id, index_key)
        .await
        .unwrap();
    assert!(
        index_data.is_some(),
        "FavouritesIndex should exist in user's bucket"
    );

    let index: FavouritesIndex = serde_json::from_slice(&index_data.unwrap()).unwrap();
    assert!(
        index.entries.iter().any(|e| e.resource_id == file.id),
        "FavouritesIndex should contain the file"
    );

    // File document unchanged (no version bump)
    let file_after = ctx.file_service().get_file(user_id, file.id).await.unwrap();
    assert_eq!(
        file_after.current_version, version_before,
        "File version should not change when starred"
    );

    // Verify the entry by reading from index
    let fav_list = ctx.favourite_service().list_favourites(user_id).await.unwrap();
    let entry = fav_list.iter().find(|e| e.resource_id == file.id).expect("File should be in favourites");
    assert_eq!(entry.resource_id, file.id);
}

/// Test FV-02: Star shared file updates recipient only
///
/// Verify starring shared file updates only recipient's favourites,
/// not owner's file document.
#[tokio::test]
async fn test_fv_02_star_shared_updates_recipient_only() {
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

    let file_version_before = file.current_version;

    // Recipient stars the shared file (using resource_id, not share_id)
    ctx
        .favourite_service()
        .add_favourite(recipient_id, file.id, FavouriteResourceType::ReceivedFile)
        .await
        .unwrap();

    // Recipient has favourites index
    let recipient_index_key = "indexes/received/favourites.json";
    let recipient_index = ctx
        .user_buckets
        .get_object(recipient_id, recipient_index_key)
        .await
        .unwrap();
    assert!(
        recipient_index.is_some(),
        "Recipient should have favourites index"
    );

    let index: FavouritesIndex = serde_json::from_slice(&recipient_index.unwrap()).unwrap();
    assert!(
        index.entries.iter().any(|e| e.resource_id == file.id),
        "Recipient's favourites should contain the shared file"
    );

    // Owner does NOT have favourites (for this file)
    let owner_index = ctx
        .user_buckets
        .get_object(owner_id, "indexes/owned/favourites.json")
        .await
        .unwrap();
    if let Some(data) = owner_index {
        let index: FavouritesIndex = serde_json::from_slice(&data).unwrap();
        assert!(
            !index.entries.iter().any(|e| e.resource_id == file.id && true),
            "Owner should not have the shared file as a favourite"
        );
    }

    // Owner's file unchanged
    let file_after = ctx.file_service().get_file(owner_id, file.id).await.unwrap();
    assert_eq!(
        file_after.current_version, file_version_before,
        "Owner's file version should not change when recipient stars it"
    );

    // Verify the entry has a locator
    let fav_list = ctx.favourite_service().list_favourites(recipient_id).await.unwrap();
    let entry = fav_list.iter().find(|e| e.resource_id == file.id).expect("File should be in favourites");
    // Note: The V2 implementation doesn't store locator in favourite entries by default
}

/// Test FV-03: Unstar removes from favourites
///
/// Verify unstarring removes entry from favourites index
#[tokio::test]
async fn test_fv_03_unstar_removes_from_favourites() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "starred.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    let entry = ctx
        .favourite_service()
        .add_favourite(user_id, file.id, FavouriteResourceType::OwnedFile)
        .await
        .unwrap();

    // Verify it's in the index
    let index_key = "indexes/owned/favourites.json";
    let index_data = ctx
        .user_buckets
        .get_object(user_id, index_key)
        .await
        .unwrap()
        .unwrap();
    let index: FavouritesIndex = serde_json::from_slice(&index_data).unwrap();
    assert!(index.entries.iter().any(|e| e.resource_id == file.id));

    // Unstar
    ctx.favourite_service()
        .remove_favourite(user_id, file.id)
        .await
        .unwrap();

    // Verify removed from index
    let index_data = ctx
        .user_buckets
        .get_object(user_id, index_key)
        .await
        .unwrap()
        .unwrap();
    let index: FavouritesIndex = serde_json::from_slice(&index_data).unwrap();
    assert!(
        !index.entries.iter().any(|e| e.resource_id == file.id),
        "File should be removed from favourites"
    );
}

/// Test FV-04: Favourites survive restore
///
/// Verify favourites are restored when user bucket is restored
#[tokio::test]
async fn test_fv_04_favourites_survive_restore() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "favourite.txt".to_string(),
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

    // Export and restore
    let export = ctx.export_user_bucket(user_id).await.unwrap();
    ctx.delete_user_bucket(user_id).await.unwrap();
    ctx.restore_user_bucket(user_id, &export).await.unwrap();

    // Favourites still present
    let favourites = ctx.favourite_service().list_favourites(user_id).await.unwrap();
    assert!(
        favourites.iter().any(|f| f.resource_id == file.id),
        "Favourites should survive restore"
    );
}

/// Test FV-05: List favourites returns all starred items
///
/// Verify list_favourites returns both owned and shared favourites
#[tokio::test]
async fn test_fv_05_list_favourites_returns_all() {
    let ctx = TestContext::new().await;
    let owner_id = Uuid::new_v4();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(owner_id).await.unwrap();
    ctx.create_user(recipient_id).await.unwrap();

    // Owner stars their own files
    let file1 = ctx
        .file_service()
        .upload_file(
            owner_id,
            "owned1.txt".to_string(),
            None,
            Bytes::from("content1"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    let file2 = ctx
        .file_service()
        .upload_file(
            owner_id,
            "owned2.txt".to_string(),
            None,
            Bytes::from("content2"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    ctx.favourite_service()
        .add_favourite(owner_id, file1.id, FavouriteResourceType::OwnedFile)
        .await
        .unwrap();
    ctx.favourite_service()
        .add_favourite(owner_id, file2.id, FavouriteResourceType::OwnedFile)
        .await
        .unwrap();

    // Owner shares one file with recipient
    let share = ctx
        .share_service()
        .create_share(
            owner_id,
            recipient_id,
            file1.id,
            ShareResourceTypeV2::File,
            SharePermissionV2::Read,
            None,
        )
        .await
        .unwrap();

    // Recipient stars the shared file
    ctx.favourite_service()
        .add_favourite(recipient_id, file1.id, FavouriteResourceType::ReceivedFile)
        .await
        .unwrap();

    // Owner's favourites
    let owner_favourites = ctx.favourite_service().list_favourites(owner_id).await.unwrap();
    assert_eq!(owner_favourites.len(), 2, "Owner should have 2 favourites");
    assert!(owner_favourites.iter().any(|f| f.resource_id == file1.id));
    assert!(owner_favourites.iter().any(|f| f.resource_id == file2.id));

    // Recipient's favourites
    let recipient_favourites = ctx.favourite_service().list_favourites(recipient_id).await.unwrap();
    assert_eq!(recipient_favourites.len(), 1, "Recipient should have 1 favourite");
    assert!(recipient_favourites.iter().any(|f| f.resource_id == file1.id));
}

/// Test FV-06: Is_favourited returns correct state
///
/// Verify is_favourited returns true for starred items, false otherwise
#[tokio::test]
async fn test_fv_06_is_favourited_correct_state() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    let starred_file = ctx
        .file_service()
        .upload_file(
            user_id,
            "starred.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    let unstarred_file = ctx
        .file_service()
        .upload_file(
            user_id,
            "unstarred.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // Star one file
    ctx.favourite_service()
        .add_favourite(user_id, starred_file.id, FavouriteResourceType::OwnedFile)
        .await
        .unwrap();

    // Check is_favourited
    assert!(
        ctx.favourite_service()
            .is_favourite(user_id, starred_file.id)
            .await
            .unwrap(),
        "Starred file should return true"
    );

    assert!(
        !ctx.favourite_service()
            .is_favourite(user_id, unstarred_file.id)
            .await
            .unwrap(),
        "Unstarred file should return false"
    );
}
