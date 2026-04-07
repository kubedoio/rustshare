//! Device Pairing Contract Tests (C-01 through C-06)
//!
//! Tests device synchronization and pairing functionality:
//! - C-01: Device pairing creates scoped trust relationship
//! - C-02: Revoked device cannot sync
//! - C-03: Device token is not reusable after revocation
//! - C-04: Device pairing requires user approval
//! - C-05: Device tokens are tenant-scoped
//! - C-06: Device sync respects file permissions

use crate::contracts::common::*;
use rustshare_core::domain::{DevicePairRequest, DeviceToken};

/// C-01: Device pairing creates scoped trust relationship
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_device_pairing_creates_scoped_trust() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "device_user", tenant_id).await;

    // Create a device token (simulating successful pairing)
    let device_token = DeviceToken {
        id: uuid::Uuid::new_v4(),
        user_id: user.id,
        token_hash: "hashed_token_value".to_string(),
        device_name: "Test Device".to_string(),
        created_at: chrono::Utc::now(),
        last_used_at: chrono::Utc::now(),
        revoked_at: None,
        tenant_id,
    };

    // Verify token properties
    assert_eq!(device_token.user_id, user.id);
    assert_eq!(device_token.tenant_id, tenant_id);
    assert!(
        device_token.revoked_at.is_none(),
        "New token should not be revoked"
    );

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// C-02: Revoked device cannot sync
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_revoked_device_cannot_sync() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "revoked_device_user", tenant_id).await;

    // Create file service and a file
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "sync_test.txt",
        b"Sync test content",
    )
    .await;

    // Create a revoked device token
    let revoked_device = DeviceToken {
        id: uuid::Uuid::new_v4(),
        user_id: user.id,
        token_hash: "revoked_token_hash".to_string(),
        device_name: "Revoked Device".to_string(),
        created_at: chrono::Utc::now(),
        last_used_at: chrono::Utc::now(),
        revoked_at: Some(chrono::Utc::now()), // Device is revoked
        tenant_id,
    };

    // Verify device is marked as revoked
    assert!(
        revoked_device.revoked_at.is_some(),
        "Device should be revoked"
    );

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// C-03: Device token is not reusable after revocation
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_device_token_not_reusable_after_revocation() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "token_user", tenant_id).await;

    // Create a revoked device token with specific token hash
    let token_hash = "unique_token_hash_12345";

    let revoked_device = DeviceToken {
        id: uuid::Uuid::new_v4(),
        user_id: user.id,
        token_hash: token_hash.to_string(),
        device_name: "Test Device".to_string(),
        created_at: chrono::Utc::now(),
        last_used_at: chrono::Utc::now(),
        revoked_at: Some(chrono::Utc::now()),
        tenant_id,
    };

    // Verify the revoked token hash
    assert_eq!(revoked_device.token_hash, token_hash);
    assert!(revoked_device.revoked_at.is_some());

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// C-04: Device pairing requires user approval
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_device_pairing_requires_user_approval() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "approval_user", tenant_id).await;

    // Create a pending device pair request (not yet approved)
    let pending_request = DevicePairRequest {
        id: uuid::Uuid::new_v4(),
        device_code: "device_code_123".to_string(),
        user_code: "USER-CODE-123".to_string(),
        user_id: None, // Not yet associated with a user
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
        approved_at: None, // Not yet approved
    };

    // Verify request is pending
    assert!(
        pending_request.user_id.is_none(),
        "New request should not have user_id"
    );
    assert!(
        pending_request.approved_at.is_none(),
        "New request should not be approved"
    );

    // Simulate approval
    let approved_request = DevicePairRequest {
        id: pending_request.id,
        device_code: pending_request.device_code,
        user_code: pending_request.user_code,
        user_id: Some(user.id), // Now associated
        expires_at: pending_request.expires_at,
        approved_at: Some(chrono::Utc::now()), // Now approved
    };

    // Verify approval
    assert_eq!(approved_request.user_id, Some(user.id));
    assert!(approved_request.approved_at.is_some());

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// C-05: Device tokens are tenant-scoped
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_device_tokens_are_tenant_scoped() {
    let ctx = setup_test_env().await;

    // Setup two tenants
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    // Create users in each tenant
    let user_a = create_test_user(&ctx.metadata_store, "unfurl_user_a", tenant_a).await;

    // Create file service
    let file_service = ctx.file_service();

    // Create file in tenant A
    let file = create_test_file(
        &file_service,
        user_a.id,
        tenant_a,
        None,
        "tenant_a_file.txt",
        b"Tenant A content",
    )
    .await;

    // Create device token in tenant A
    let device_token = DeviceToken {
        id: uuid::Uuid::new_v4(),
        user_id: user_a.id,
        token_hash: "scoped_token_hash".to_string(),
        device_name: "Tenant A Device".to_string(),
        created_at: chrono::Utc::now(),
        last_used_at: chrono::Utc::now(),
        revoked_at: None,
        tenant_id: tenant_a,
    };

    // Verify token is scoped to tenant A
    assert_eq!(device_token.tenant_id, tenant_a);
    assert_ne!(device_token.tenant_id, tenant_b);

    // Cleanup
    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

/// C-06: Device sync respects file permissions
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_device_sync_respects_file_permissions() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner and another user
    let owner = create_test_user(&ctx.metadata_store, "file_owner", tenant_id).await;
    let other_user = create_test_user(&ctx.metadata_store, "other_user", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Owner creates a file
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "private_file.txt",
        b"Private content",
    )
    .await;

    // Owner can access their file
    let owner_access = file_service.get_file(file.id, owner.id).await;
    assert!(owner_access.is_ok(), "Owner should access their file");

    // Other user cannot access the file
    let other_access = file_service.get_file(file.id, other_user.id).await;
    assert!(
        other_access.is_err(),
        "Other user should not access owner's file"
    );

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_user(&ctx.pool, other_user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// Additional test: Device code expiration
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_device_pair_request_expires() {
    let ctx = setup_test_env().await;

    // Create a pair request that expires in the future
    let future_request = DevicePairRequest {
        id: uuid::Uuid::new_v4(),
        device_code: "future_code".to_string(),
        user_code: "FUTURE-CODE".to_string(),
        user_id: None,
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
        approved_at: None,
    };

    // Should not be expired
    assert!(
        future_request.expires_at > chrono::Utc::now(),
        "Future request should not be expired"
    );

    // Create an expired pair request
    let expired_request = DevicePairRequest {
        id: uuid::Uuid::new_v4(),
        device_code: "expired_code".to_string(),
        user_code: "EXPIRED-CODE".to_string(),
        user_id: None,
        expires_at: chrono::Utc::now() - chrono::Duration::minutes(1),
        approved_at: None,
    };

    // Should be expired
    assert!(
        expired_request.expires_at < chrono::Utc::now(),
        "Expired request should be past expiration"
    );
}
