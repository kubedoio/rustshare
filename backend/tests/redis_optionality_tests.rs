//! Redis Optionality Contract Tests
//!
//! Tests that verify:
//! - Core flows work without Redis
//! - Distributed coordination requires Redis
//! - Redis loss does not destroy durable truth

use crate::*;

/// Test RO-01: Core flows without Redis
///
/// Verify file CRUD and sharing work without Redis
#[tokio::test]
async fn test_ro_01_core_flows_without_redis() {
    let ctx = TestContext::new_without_redis().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    // File CRUD
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

    let renamed = ctx
        .file_service()
        .rename_file(user_id, file.id, "renamed.txt".to_string())
        .await
        .unwrap();
    assert_eq!(renamed.name, "renamed.txt");

    ctx.file_service().delete_file(user_id, file.id).await.unwrap();

    // Sharing
    let file2 = ctx
        .file_service()
        .upload_file(
            user_id,
            "share.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();
    let recipient_id = Uuid::new_v4();
    ctx.create_user(recipient_id).await.unwrap();

    ctx.share_service()
        .create_share(
            user_id,
            recipient_id,
            file2.id,
            ShareResourceTypeV2::File,
            SharePermissionV2::Read,
            None,
        )
        .await
        .unwrap();

    // All operations succeeded without Redis
}

/// Test RO-02: Coordination requires Redis
///
/// Verify distributed coordination features require Redis
#[tokio::test]
async fn test_ro_02_coordination_requires_redis() {
    let ctx = TestContext::new_without_redis().await;

    // Attempting to use coordination without Redis should fail gracefully
    let result = ctx.coordination.acquire_lease("test", 30).await;
    
    // Should either fail or return a dummy lease
    if let Ok(lease) = result {
        assert!(
            lease.is_dummy_lease(),
            "Without Redis, should get a dummy lease"
        );
    }
}

/// Test RO-03: Redis loss no data loss
///
/// Verify Redis loss does not destroy durable truth
#[tokio::test]
async fn test_ro_03_redis_loss_no_data_loss() {
    let ctx = TestContext::new().await;
    let user_id = Uuid::new_v4();
    ctx.create_user(user_id).await.unwrap();

    // Create file with Redis available
    let file = ctx
        .file_service()
        .upload_file(
            user_id,
            "durable.txt".to_string(),
            None,
            Bytes::from("content"),
            "text/plain".to_string(),
        )
        .await
        .unwrap();

    // "Lose" Redis (simulated)
    ctx.simulate_redis_loss().await;

    // File still accessible from bucket
    let retrieved = ctx.file_service().get_file(user_id, file.id).await;
    assert!(
        retrieved.is_ok(),
        "File should still be accessible after Redis loss"
    );
    assert_eq!(retrieved.unwrap().id, file.id);
}

/// Test RO-04: Favourites work without Redis
///
/// Verify favourites work without Redis coordination
#[tokio::test]
async fn test_ro_04_favourites_work_without_redis() {
    let ctx = TestContext::new_without_redis().await;
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

    // Star without Redis
    let entry = ctx
        .favourite_service()
        .add_favourite(user_id, file.id, FavouriteResourceType::OwnedFile)
        .await
        .unwrap();

    // Verify in bucket
    let index_key = "indexes/owned/favourites.json";
    let index_data = ctx
        .user_buckets
        .get_object(user_id, index_key)
        .await
        .unwrap();
    assert!(
        index_data.is_some(),
        "Favourites should be stored without Redis"
    );

    // Unstar without Redis
    ctx.favourite_service()
        .remove_favourite(user_id, file.id)
        .await
        .unwrap();
}

/// Test RO-05: Share operations without Redis
///
/// Verify sharing works without Redis
#[tokio::test]
async fn test_ro_05_share_operations_without_redis() {
    let ctx = TestContext::new_without_redis().await;
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

    // Create share without Redis
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

    // Verify in buckets
    let outbound_key = format!("owned/shares/outbound/{}.json", share.share_id);
    let outbound = ctx
        .user_buckets
        .get_object(owner_id, &outbound_key)
        .await
        .unwrap();
    assert!(outbound.is_some(), "Share should be created without Redis");

    let received_key = format!("received/shares/{}.json", share.share_id);
    let received = ctx
        .user_buckets
        .get_object(recipient_id, &received_key)
        .await
        .unwrap();
    assert!(
        received.is_some(),
        "Received share should be created without Redis"
    );
}
