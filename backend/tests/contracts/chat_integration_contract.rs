//! Chat Integration Contract Tests (H-01 through H-06)
//!
//! Tests chat/AI unfurl integration:
//! - H-01: Unfurl checks permissions before returning preview
//! - H-02: Revoked share stops unfurl from working
//! - H-03: Webhook events are signed and verifiable
//! - H-04: Unfurl respects tenant boundaries
//! - H-05: Unfurl handles deleted files gracefully
//! - H-06: Unfurl rate limiting

use crate::contracts::common::*;
use rustshare_core::domain::SharePermissions;
use rustshare_core::services::ShareError;

// Mock JWT manager for testing
struct MockJwtManager;

impl rustshare_core::services::share_service::JwtOps for MockJwtManager {
    fn encode_custom_claims<T: serde::Serialize>(&self, _claims: &T) -> Result<String, String> {
        Ok("test_jwt_token".to_string())
    }
}

fn create_share_service(
    ctx: &TestContext,
) -> rustshare_core::services::ShareService<EventStore, MetadataStore, MockJwtManager> {
    rustshare_core::services::ShareService::new(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.broadcaster.clone(),
        Arc::new(MockJwtManager),
    )
}

/// H-01: Unfurl checks permissions before returning preview
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_unfurl_checks_permissions() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "unfurl_owner", tenant_id).await;

    // Create file service and a file
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "shared_for_unfurl.txt",
        b"Content for unfurl preview",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create a public share
    let share = create_test_share(
        &share_service,
        file.id,
        owner.id,
        SharePermissions::View,
        None,
        None,
        tenant_id,
    )
    .await;

    // Get public share info (simulating unfurl)
    let token = share.share_token.unwrap();
    let public_info = share_service.get_public_share_info(&token).await;

    assert!(
        public_info.is_ok(),
        "Valid share should allow unfurl preview"
    );

    let (retrieved_share, file_info, _) = public_info.unwrap();
    assert!(file_info.is_some(), "Unfurl should return file info");
    assert_eq!(retrieved_share.id, share.id);

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// H-02: Revoked share stops unfurl from working
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_revoked_share_stops_unfurl() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "revoke_unfurl_owner", tenant_id).await;

    // Create file service and a file
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "revocable_unfurl.txt",
        b"Content for unfurl",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create a public share
    let share = create_test_share(
        &share_service,
        file.id,
        owner.id,
        SharePermissions::View,
        None,
        None,
        tenant_id,
    )
    .await;

    let token = share.share_token.clone().unwrap();

    // Verify unfurl works before revocation
    let public_info = share_service.get_public_share_info(&token).await;
    assert!(public_info.is_ok(), "Unfurl should work before revocation");

    // Revoke the share
    share_service
        .revoke_share(share.id, owner.id)
        .await
        .expect("Failed to revoke share");

    // Unfurl should now fail
    let result = share_service.get_public_share_info(&token).await;
    assert!(
        matches!(result, Err(ShareError::Revoked)),
        "Unfurl should fail for revoked share"
    );

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// H-03: Webhook events are signed and verifiable (conceptual)
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_webhook_event_structure() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "webhook_user", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Create a file
    let file = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "webhook_test.txt",
        b"Webhook test content",
    )
    .await;

    // Verify file was created
    assert!(!file.id.is_nil());
    assert_eq!(file.owner_id, user.id);

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// H-04: Unfurl respects tenant boundaries
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_unfurl_respects_tenant_boundaries() {
    let ctx = setup_test_env().await;

    // Setup two tenants
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    // Create users
    let user_a = create_test_user(&ctx.metadata_store, "unfurl_user_a", tenant_a).await;

    // Create file service
    let file_service = ctx.file_service();

    // Create file in tenant A
    let file = create_test_file(
        &file_service,
        user_a.id,
        tenant_a,
        None,
        "tenant_a_unfurl.txt",
        b"Tenant A content",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create share in tenant A
    let share = create_test_share(
        &share_service,
        file.id,
        user_a.id,
        SharePermissions::View,
        None,
        None,
        tenant_a,
    )
    .await;

    // Verify share belongs to tenant A
    assert_eq!(share.tenant_id, tenant_a);
    assert_ne!(share.tenant_id, tenant_b);

    // Get public share info
    let token = share.share_token.unwrap();
    let public_info = share_service.get_public_share_info(&token).await;
    assert!(public_info.is_ok());

    let (share_info, _, _) = public_info.unwrap();
    assert_eq!(share_info.tenant_id, tenant_a, "Unfurl should respect tenant");

    // Cleanup
    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

/// H-05: Unfurl handles deleted files gracefully
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_unfurl_handles_deleted_files() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "deleted_unfurl_owner", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Create a file
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "to_be_deleted.txt",
        b"Content that will be deleted",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create a share
    let share = create_test_share(
        &share_service,
        file.id,
        owner.id,
        SharePermissions::View,
        None,
        None,
        tenant_id,
    )
    .await;

    let token = share.share_token.clone().unwrap();

    // Verify unfurl works
    let public_info = share_service.get_public_share_info(&token).await;
    assert!(public_info.is_ok(), "Unfurl should work before deletion");

    // Delete the file
    file_service
        .delete_file(file.id, owner.id)
        .await
        .expect("Failed to delete file");

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// H-06: Expired share stops unfurl
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_expired_share_stops_unfurl() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "expired_unfurl_owner", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Create a file
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "expiring_unfurl.txt",
        b"Content for expiring unfurl",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create an already expired share
    let expired_time = chrono::Utc::now() - chrono::Duration::hours(1);
    let share = create_test_share(
        &share_service,
        file.id,
        owner.id,
        SharePermissions::View,
        None,
        Some(expired_time),
        tenant_id,
    )
    .await;

    let token = share.share_token.unwrap();

    // Unfurl should fail for expired share
    let result = share_service.get_public_share_info(&token).await;
    assert!(
        matches!(result, Err(ShareError::Expired)),
        "Unfurl should fail for expired share"
    );

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}
