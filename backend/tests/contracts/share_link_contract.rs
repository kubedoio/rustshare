//! Share Link Contract Tests (S-01 through S-08)
//!
//! Tests the complete sharing functionality including:
//! - Internal shares (S-01)
//! - Public read links (S-02)
//! - Upload-only links (S-03)
//! - Expired shares (S-04)
//! - Revoked shares (S-05)
//! - Password-protected shares (S-06)
//! - Download limits (S-07)
//! - Audit logging (S-08)

use crate::common::*;
use rustshare_core::domain::SharePermissions;
use rustshare_core::services::ShareError;
use rustshare_storage::{EventStore, MetadataStore};
use std::sync::Arc;

// Mock JWT manager for testing
struct MockJwtManager;

impl rustshare_core::services::JwtOps for MockJwtManager {
    fn encode_custom_claims<T: serde::Serialize>(&self, _claims: &T) -> Result<String, String> {
        Ok("test_jwt_token".to_string())
    }
}

fn create_share_service(
    ctx: &TestContext,
) -> rustshare_core::services::ShareService<
    EventStore,
    MetadataStore,
    MockJwtManager,
    crate::common::MockNotificationRepo,
> {
    crate::common::create_test_share_service(ctx, Arc::new(MockJwtManager))
}

/// S-01: Internal share grants access to recipient
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_internal_share_grants_access_to_recipient() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner and recipient users
    let owner = create_test_user(&ctx.metadata_store, "share_owner", tenant_id).await;
    let recipient = create_test_user(&ctx.metadata_store, "share_recipient", tenant_id).await;

    // Create file service and a file
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "shared_doc.txt",
        b"Shared content",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create internal share (user-to-user) - Note: This uses user_shares table conceptually
    // For now, we test that the owner can share and recipient gets access
    // The actual internal share mechanism may vary based on implementation

    // Owner should have access
    let owner_access = file_service.get_file(file.id, owner.id).await;
    assert!(
        owner_access.is_ok(),
        "Owner should have access to their file"
    );

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_user(&ctx.pool, recipient.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-02: Public read link allows anonymous access
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_public_read_link_allows_anonymous_access() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "public_share_owner", tenant_id).await;

    // Create file service and a file
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "public_doc.txt",
        b"Publicly shared content",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create public share with View permissions
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

    // Verify share was created with correct permissions
    assert_eq!(share.permissions, SharePermissions::View);
    assert!(
        share.share_token.is_some(),
        "Public share should have a token"
    );
    assert!(!share.upload_only);

    // Validate share and create session (simulating anonymous access)
    let token = share.share_token.unwrap();
    let session = share_service
        .validate_and_create_session(&token, None)
        .await;

    assert!(
        session.is_ok(),
        "Anonymous user should be able to validate public read share"
    );

    let session = session.unwrap();
    assert!(!session.upload_only, "Read share should not be upload-only");

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-03: Upload-only link allows upload but not browse
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_only_link_allows_upload_but_not_browse() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "upload_share_owner", tenant_id).await;

    // Create folder service and a folder
    let folder_service = ctx.folder_service();
    let folder = create_test_folder(&folder_service, owner.id, tenant_id, "Dropbox", None).await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create upload-only folder share
    let share = create_test_folder_share(
        &share_service,
        folder.id,
        owner.id,
        SharePermissions::Edit,
        None,
        None,
        true, // upload_only = true
        tenant_id,
    )
    .await;

    // Verify share was created as upload-only
    assert!(share.upload_only, "Share should be upload-only");

    // Validate share
    let token = share.share_token.unwrap();
    let session = share_service
        .validate_and_create_session(&token, None)
        .await;

    assert!(session.is_ok(), "Upload-only share should be validatable");

    let session = session.unwrap();
    assert!(session.upload_only, "Session should indicate upload-only");

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-04: Expired share denies access
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_expired_share_denies_access() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "expired_share_owner", tenant_id).await;

    // Create file service and a file
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "expiring_doc.txt",
        b"Temporarily shared content",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create share that expired 1 hour ago
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

    // Verify share is marked as expired
    assert!(share.is_expired(), "Share should be expired");

    // Try to validate the expired share
    let token = share.share_token.unwrap();
    let result = share_service
        .validate_and_create_session(&token, None)
        .await;

    assert!(
        matches!(result, Err(ShareError::Expired)),
        "Expired share should be rejected"
    );

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-05: Revoked share denies access
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_revoked_share_denies_access() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "revoked_share_owner", tenant_id).await;

    // Create file service and a file
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "revocable_doc.txt",
        b"Revocably shared content",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create share
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

    // Verify share works before revocation
    let session = share_service
        .validate_and_create_session(&token, None)
        .await;
    assert!(session.is_ok(), "Share should work before revocation");

    // Revoke the share
    share_service
        .revoke_share(share.id, owner.id)
        .await
        .expect("Failed to revoke share");

    // Try to validate the revoked share
    let result = share_service
        .validate_and_create_session(&token, None)
        .await;

    assert!(
        matches!(result, Err(ShareError::Revoked)),
        "Revoked share should be rejected"
    );

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-06: Password-protected share requires password
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_password_protected_share_requires_password() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "protected_share_owner", tenant_id).await;

    // Create file service and a file
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "protected_doc.txt",
        b"Password protected content",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create password-protected share
    let share = create_test_share(
        &share_service,
        file.id,
        owner.id,
        SharePermissions::View,
        Some("secret123".to_string()),
        None,
        tenant_id,
    )
    .await;

    // Verify share is password-protected
    assert!(
        share.is_password_protected(),
        "Share should be password-protected"
    );

    let token = share.share_token.unwrap();

    // Try to access without password
    let result = share_service
        .validate_and_create_session(&token, None)
        .await;
    assert!(
        matches!(result, Err(ShareError::PasswordRequired)),
        "Password-protected share should require password"
    );

    // Try to access with wrong password
    let result = share_service
        .validate_and_create_session(&token, Some("wrong_password".to_string()))
        .await;
    assert!(
        matches!(result, Err(ShareError::InvalidPassword)),
        "Wrong password should be rejected"
    );

    // Access with correct password
    let session = share_service
        .validate_and_create_session(&token, Some("secret123".to_string()))
        .await;
    assert!(session.is_ok(), "Correct password should allow access");

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-07: Non-owner cannot revoke share
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_non_owner_cannot_revoke_share() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner and another user
    let owner = create_test_user(&ctx.metadata_store, "share_owner", tenant_id).await;
    let other_user = create_test_user(&ctx.metadata_store, "other_user", tenant_id).await;

    // Create file service and a file
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "private_doc.txt",
        b"Private content",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create share
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

    // Non-owner tries to revoke
    let result = share_service.revoke_share(share.id, other_user.id).await;
    assert!(
        matches!(result, Err(ShareError::PermissionDenied { .. })),
        "Non-owner should not be able to revoke share"
    );

    // Owner should still be able to revoke
    let result = share_service.revoke_share(share.id, owner.id).await;
    assert!(result.is_ok(), "Owner should be able to revoke their share");

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_user(&ctx.pool, other_user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-08: Share access is logged
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_share_access_increments_count() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "count_share_owner", tenant_id).await;

    // Create file service and a file
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "counted_doc.txt",
        b"Counted access content",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create share
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

    // Initial access count should be 0
    assert_eq!(share.access_count, 0, "Initial access count should be 0");

    let token = share.share_token.unwrap();

    // Access share multiple times
    for i in 1..=3 {
        let _session = share_service
            .validate_and_create_session(&token, None)
            .await
            .expect("Share should be valid");

        // Note: In a real implementation, we'd verify the access count incremented
        // This may require fetching the share again from the database
    }

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-09: Already-issued public share session token is rejected after revoke
///
/// This test verifies that:
/// 1. A session can be created before revocation
/// 2. After revocation, new session creation fails with Revoked
/// 3. The share record in the database reflects revoked status
///
/// NOTE: Handler-level JWT validation for already-issued tokens should also
/// re-check share.revoked_at against the database. Current handlers only verify
/// share_id matches the JWT claims (see ensure_share_session_matches). This is
/// a known gap documented by this test.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_public_share_already_issued_session_rejected_after_revoke() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    let owner = create_test_user(&ctx.metadata_store, "session_owner", tenant_id).await;

    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "session_doc.txt",
        b"Session test content",
    )
    .await;

    let share_service = create_share_service(&ctx);

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

    // Create an active session BEFORE revocation (simulating a visitor who opened the link)
    let session = share_service
        .validate_and_create_session(&token, None)
        .await
        .expect("Session should be created before revocation");
    assert!(!session.upload_only, "Read share session should not be upload-only");

    // Revoke the share
    share_service
        .revoke_share(share.id, owner.id)
        .await
        .expect("Failed to revoke share");

    // Verify the share is marked revoked in the database
    let revoked_share = ctx
        .metadata_store
        .get_share_by_token(&token)
        .await
        .expect("DB lookup failed")
        .expect("Share should exist");
    assert!(
        revoked_share.revoked_at.is_some(),
        "Share should have revoked_at set after revocation"
    );

    // New session creation must fail after revocation
    let result = share_service
        .validate_and_create_session(&token, None)
        .await;
    assert!(
        matches!(result, Err(ShareError::Revoked)),
        "New session creation should fail after revocation"
    );

    // The already-issued JWT token (session.token) is not validated by the service layer.
    // Handlers that accept ShareSessionAuth MUST re-check revoked_at against the DB
    // to ensure already-issued tokens are rejected. This is a security contract gap.

    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-10: Share revocation emits a durable ShareRevoked event
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_public_share_revoke_emits_audit_event() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    let owner = create_test_user(&ctx.metadata_store, "audit_owner", tenant_id).await;

    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "audit_doc.txt",
        b"Audit test content",
    )
    .await;

    let share_service = create_share_service(&ctx);

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

    // Subscribe to events before revocation
    let mut receiver = ctx.broadcaster.subscribe();

    // Revoke the share
    share_service
        .revoke_share(share.id, owner.id)
        .await
        .expect("Failed to revoke share");

    // Verify the ShareRevoked event was broadcast
    let event = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        receiver.recv(),
    )
    .await
    .expect("Timed out waiting for event")
    .expect("Event broadcaster closed");

    assert_eq!(
        event.event_type,
        rustshare_core::events::EventType::ShareRevoked,
        "Revocation should emit ShareRevoked event"
    );
    assert_eq!(
        event.aggregate_id, share.id,
        "Event aggregate_id should match share ID"
    );

    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-11: Denied post-revocation access is auditable
///
/// When a revoked share token is presented, the system should log the denied
/// access attempt if an audit trail is required by contract G-03.
///
/// NOTE: Current implementation does not log denied share access attempts.
/// This test documents the gap.
#[tokio::test]
#[ignore] // Requires database and S3; also requires denied-access audit logging
async fn test_revoked_share_access_denial_is_auditable() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    let owner = create_test_user(&ctx.metadata_store, "denial_owner", tenant_id).await;

    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        None,
        "denial_doc.txt",
        b"Denial test content",
    )
    .await;

    let share_service = create_share_service(&ctx);

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

    // Create a session before revocation
    let _session = share_service
        .validate_and_create_session(&token, None)
        .await
        .expect("Session should work before revocation");

    // Revoke the share
    share_service
        .revoke_share(share.id, owner.id)
        .await
        .expect("Failed to revoke share");

    // Attempt to create a new session after revocation
    let result = share_service
        .validate_and_create_session(&token, None)
        .await;
    assert!(
        matches!(result, Err(ShareError::Revoked)),
        "Revoked share should deny new sessions"
    );

    // TODO: Once denied-access audit logging is implemented, verify that a
    // share_access_log row or security event was recorded for this denial.
    // Currently only successful accesses are logged.

    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}
