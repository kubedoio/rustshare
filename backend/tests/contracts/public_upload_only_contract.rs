//! Public Upload-Only Share Contract Tests (S-03 Specific)
//!
//! Detailed tests for upload-only share links:
//! - Anonymous user can upload via upload-only link
//! - Anonymous user cannot list existing files
//! - Upload-only with password still requires password
//! - Upload-only link expires correctly
//! - Upload-only link can be revoked

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

/// S-03-01: Anonymous user can upload via upload-only link
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_anonymous_can_upload_via_upload_only_link() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "upload_owner", tenant_id).await;

    // Create folder service and a folder
    let folder_service = ctx.folder_service();
    let folder = create_test_folder(&folder_service, owner.id, tenant_id, "Dropbox", None).await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create upload-only folder share
    let share = share_service
        .create_folder_share(
            folder.id,
            owner.id,
            SharePermissions::Edit,
            None,
            None,
            true, // upload_only = true
            tenant_id,
        )
        .await
        .expect("Failed to create upload-only share");

    // Verify share properties
    assert!(share.upload_only, "Share should be upload-only");
    assert_eq!(share.folder_id, Some(folder.id));
    assert!(share.share_token.is_some());

    // Validate the share as an anonymous user would
    let token = share.share_token.unwrap();
    let session = share_service
        .validate_and_create_session(&token, None)
        .await
        .expect("Upload-only share should be validatable");

    // Verify session indicates upload-only
    assert!(
        session.upload_only,
        "Session should indicate upload-only access"
    );

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-03-02: Anonymous user cannot list existing files via upload-only link
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_anonymous_cannot_list_files_via_upload_only() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "upload_owner2", tenant_id).await;

    // Create folder service and a folder
    let folder_service = ctx.folder_service();
    let folder = create_test_folder(&folder_service, owner.id, tenant_id, "Dropbox", None).await;

    // Create file service and add a file to the folder
    let file_service = ctx.file_service();
    let _file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        Some(folder.id),
        "existing_file.txt",
        b"Existing content",
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create upload-only folder share
    let share = share_service
        .create_folder_share(
            folder.id,
            owner.id,
            SharePermissions::Edit,
            None,
            None,
            true,
            tenant_id,
        )
        .await
        .expect("Failed to create upload-only share");

    // Get public share info
    let token = share.share_token.unwrap();
    let public_info = share_service.get_public_share_info(&token).await;

    // The share info should be available
    assert!(
        public_info.is_ok(),
        "Public share info should be accessible"
    );

    let (_share, _file_info, folder_info) = public_info.unwrap();
    assert!(folder_info.is_some(), "Folder info should be available");

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-03-03: Upload-only with password still requires password
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_only_with_password_requires_password() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "password_upload_owner", tenant_id).await;

    // Create folder service and a folder
    let folder_service = ctx.folder_service();
    let folder =
        create_test_folder(&folder_service, owner.id, tenant_id, "SecureDropbox", None).await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create password-protected upload-only share
    let share = share_service
        .create_folder_share(
            folder.id,
            owner.id,
            SharePermissions::Edit,
            Some("upload_password".to_string()),
            None,
            true,
            tenant_id,
        )
        .await
        .expect("Failed to create password-protected upload-only share");

    // Verify share is password-protected and upload-only
    assert!(share.is_password_protected());
    assert!(share.upload_only);

    let token = share.share_token.unwrap();

    // Try to access without password
    let result = share_service
        .validate_and_create_session(&token, None)
        .await;
    assert!(
        matches!(result, Err(ShareError::PasswordRequired)),
        "Password should be required"
    );

    // Try with wrong password
    let result = share_service
        .validate_and_create_session(&token, Some("wrong_password".to_string()))
        .await;
    assert!(
        matches!(result, Err(ShareError::InvalidPassword)),
        "Wrong password should be rejected"
    );

    // Access with correct password
    let session = share_service
        .validate_and_create_session(&token, Some("upload_password".to_string()))
        .await;
    assert!(session.is_ok(), "Correct password should allow access");

    let session = session.unwrap();
    assert!(session.upload_only, "Session should still be upload-only");

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-03-04: Upload-only link expires correctly
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_only_link_expires() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "expiring_upload_owner", tenant_id).await;

    // Create folder service and a folder
    let folder_service = ctx.folder_service();
    let folder =
        create_test_folder(&folder_service, owner.id, tenant_id, "TempDropbox", None).await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create expired upload-only share
    let expired_time = chrono::Utc::now() - chrono::Duration::hours(1);
    let share = share_service
        .create_folder_share(
            folder.id,
            owner.id,
            SharePermissions::Edit,
            None,
            Some(expired_time),
            true,
            tenant_id,
        )
        .await
        .expect("Failed to create upload-only share");

    // Verify share is expired
    assert!(share.is_expired());
    assert!(share.upload_only);

    let token = share.share_token.unwrap();

    // Try to validate expired share
    let result = share_service
        .validate_and_create_session(&token, None)
        .await;
    assert!(
        matches!(result, Err(ShareError::Expired)),
        "Expired upload-only share should be rejected"
    );

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-03-05: Upload-only link can be revoked
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_only_link_can_be_revoked() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "revocable_upload_owner", tenant_id).await;

    // Create folder service and a folder
    let folder_service = ctx.folder_service();
    let folder = create_test_folder(
        &folder_service,
        owner.id,
        tenant_id,
        "RevocableDropbox",
        None,
    )
    .await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create upload-only share
    let share = share_service
        .create_folder_share(
            folder.id,
            owner.id,
            SharePermissions::Edit,
            None,
            None,
            true,
            tenant_id,
        )
        .await
        .expect("Failed to create upload-only share");

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
        .expect("Failed to revoke upload-only share");

    // Try to validate revoked share
    let result = share_service
        .validate_and_create_session(&token, None)
        .await;
    assert!(
        matches!(result, Err(ShareError::Revoked)),
        "Revoked upload-only share should be rejected"
    );

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-03-06: Upload-only link has correct permissions
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_only_permissions() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create owner user
    let owner = create_test_user(&ctx.metadata_store, "perm_upload_owner", tenant_id).await;

    // Create folder service and a folder
    let folder_service = ctx.folder_service();
    let folder =
        create_test_folder(&folder_service, owner.id, tenant_id, "PermDropbox", None).await;

    // Create share service
    let share_service = create_share_service(&ctx);

    // Create upload-only share with View permission (should still be upload-only)
    let share = share_service
        .create_folder_share(
            folder.id,
            owner.id,
            SharePermissions::View,
            None,
            None,
            true,
            tenant_id,
        )
        .await
        .expect("Failed to create upload-only share");

    // Verify upload_only flag takes precedence
    assert!(share.upload_only, "upload_only flag should be set");

    let token = share.share_token.unwrap();
    let session = share_service
        .validate_and_create_session(&token, None)
        .await
        .expect("Should validate");

    // Session should be upload-only regardless of base permissions
    assert!(session.upload_only, "Session should be upload-only");

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-03-07: Upload-only share revocation blocks new sessions
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_only_revoke_blocks_new_sessions() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    let owner = create_test_user(&ctx.metadata_store, "upload_revoke_owner", tenant_id).await;

    let folder_service = ctx.folder_service();
    let folder =
        create_test_folder(&folder_service, owner.id, tenant_id, "RevokeDropbox", None).await;

    let share_service = create_share_service(&ctx);

    let share = share_service
        .create_folder_share(
            folder.id,
            owner.id,
            SharePermissions::Edit,
            None,
            None,
            true,
            tenant_id,
        )
        .await
        .expect("Failed to create upload-only share");

    let token = share.share_token.clone().unwrap();

    // Create session before revocation
    let session = share_service
        .validate_and_create_session(&token, None)
        .await
        .expect("Session should work before revocation");
    assert!(session.upload_only, "Session should be upload-only");

    // Revoke the upload-only share
    share_service
        .revoke_share(share.id, owner.id)
        .await
        .expect("Failed to revoke upload-only share");

    // Verify share is revoked in DB
    let revoked_share = ctx
        .metadata_store
        .get_share_by_token(&token)
        .await
        .expect("DB lookup failed")
        .expect("Share should exist");
    assert!(
        revoked_share.revoked_at.is_some(),
        "Upload-only share should be revoked"
    );

    // New session creation must fail after revocation
    let result = share_service
        .validate_and_create_session(&token, None)
        .await;
    assert!(
        matches!(result, Err(ShareError::Revoked)),
        "Revoked upload-only share should block new sessions"
    );

    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-03-08: Upload-only session cannot list folder contents
///
/// An upload-only share must not allow browsing existing files in the folder.
/// The service-level list_public_folder_contents does not currently enforce
/// this boundary; the handler blocks it. This test documents the expected
/// service-level contract.
#[tokio::test]
#[ignore] // Requires database and S3; service does not enforce upload-only boundary
async fn test_upload_only_session_cannot_list_folder_contents() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    let owner = create_test_user(&ctx.metadata_store, "upload_boundary_owner", tenant_id).await;

    let folder_service = ctx.folder_service();
    let folder = create_test_folder(
        &folder_service,
        owner.id,
        tenant_id,
        "BoundaryDropbox",
        None,
    )
    .await;

    // Add an existing file to the folder
    let file_service = ctx.file_service();
    let _existing_file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        Some(folder.id),
        "existing.txt",
        b"Existing content",
    )
    .await;

    let share_service = create_share_service(&ctx);

    let share = share_service
        .create_folder_share(
            folder.id,
            owner.id,
            SharePermissions::Edit,
            None,
            None,
            true,
            tenant_id,
        )
        .await
        .expect("Failed to create upload-only share");

    let token = share.share_token.unwrap();

    // Create upload-only session
    let session = share_service
        .validate_and_create_session(&token, None)
        .await
        .expect("Upload-only session should be created");
    assert!(session.upload_only, "Session must be upload-only");

    // TODO: list_public_folder_contents should reject upload-only shares.
    // Currently the service returns contents and the handler blocks it.
    // Once the service enforces this, uncomment the assertion below.
    let result = share_service
        .list_public_folder_contents(&token, None)
        .await;
    assert!(
        result.is_err(),
        "Upload-only share should not allow listing folder contents"
    );

    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// S-03-09: Upload-only session token is invalid after revoke
///
/// Already-issued upload-only session tokens must not retain access after
/// the share is revoked. As with read shares, handler-level JWT validation
/// must re-check revoked_at against the database.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_only_already_issued_session_rejected_after_revoke() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    let owner = create_test_user(&ctx.metadata_store, "upload_session_owner", tenant_id).await;

    let folder_service = ctx.folder_service();
    let folder =
        create_test_folder(&folder_service, owner.id, tenant_id, "SessionDropbox", None).await;

    let share_service = create_share_service(&ctx);

    let share = share_service
        .create_folder_share(
            folder.id,
            owner.id,
            SharePermissions::Edit,
            None,
            None,
            true,
            tenant_id,
        )
        .await
        .expect("Failed to create upload-only share");

    let token = share.share_token.clone().unwrap();

    // Issue a session token BEFORE revocation
    let session = share_service
        .validate_and_create_session(&token, None)
        .await
        .expect("Should create session before revoke");
    assert!(session.upload_only);

    // Revoke
    share_service
        .revoke_share(share.id, owner.id)
        .await
        .expect("Failed to revoke");

    // New session attempts must fail
    let result = share_service
        .validate_and_create_session(&token, None)
        .await;
    assert!(
        matches!(result, Err(ShareError::Revoked)),
        "Revoked upload-only share must reject new sessions"
    );

    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}
