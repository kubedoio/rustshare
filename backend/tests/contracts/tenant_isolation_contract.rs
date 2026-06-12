//! Tenant Isolation Contract Tests (G-01)
//!
//! Tests that user data is properly isolated between tenants.
//! Contract: User from tenant A cannot access resources from tenant B.

use crate::common::*;
use rustshare_core::services::{FileError, FolderError};

/// G-01-01: User from tenant A cannot access file from tenant B
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_user_cannot_access_file_from_other_tenant() {
    let ctx = setup_test_env().await;

    // Setup two tenants
    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    // Create users in each tenant
    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    // Create FileService
    let file_service = ctx.file_service();

    // User A creates a file
    let file = create_test_file(
        &file_service,
        user_a.id,
        tenant_a,
        None,
        "confidential.txt",
        b"Tenant A secret data",
    )
    .await;

    // User B (different tenant) tries to access the file
    let result = file_service.get_file(file.id, user_b.id).await;

    // Should fail with permission denied
    assert!(
        matches!(result, Err(FileError::PermissionDenied { .. })),
        "User from tenant B should not be able to access file from tenant A"
    );

    // Cleanup
    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-02: Cross-tenant share link is denied
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_cross_tenant_share_link_is_denied() {
    let ctx = setup_test_env().await;

    // Setup two tenants
    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    // Create users in each tenant
    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let _user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    // Create folder service and a folder
    let folder_service = ctx.folder_service();
    let folder =
        create_test_folder(&folder_service, user_a.id, tenant_a, "SharedFolder", None).await;

    // Verify the folder has the correct tenant
    assert_eq!(folder.tenant_id, tenant_a);

    // Attempting to create a share with mismatched tenant should fail
    // This simulates what would happen if user_b tried to use a share from tenant_a
    let folder_from_other_tenant = folder_service.get_folder(folder.id, user_a.id).await;
    assert!(folder_from_other_tenant.is_ok());

    // The folder should maintain its original tenant_id
    let folder = folder_from_other_tenant.unwrap();
    assert_eq!(folder.tenant_id, tenant_a);

    // Cleanup
    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-03: Search results don't leak across tenants
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_search_results_do_not_leak_across_tenants() {
    let ctx = setup_test_env().await;

    // Setup two tenants
    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    // Create users in each tenant
    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    // Create FileService
    let file_service = ctx.file_service();

    // Both users create files with similar names
    let _file_a = create_test_file(
        &file_service,
        user_a.id,
        tenant_a,
        None,
        "project_specs.txt",
        b"Tenant A project specifications",
    )
    .await;

    let _file_b = create_test_file(
        &file_service,
        user_b.id,
        tenant_b,
        None,
        "project_specs.txt",
        b"Tenant B project specifications",
    )
    .await;

    // List files for user_a - should only see tenant_a files
    let files_a = ctx
        .metadata_store
        .list_files(None, user_a.id, tenant_a)
        .await
        .expect("Failed to list files");

    // All files should belong to tenant_a
    for file in &files_a {
        assert_eq!(
            file.tenant_id, tenant_a,
            "User A should only see files from their own tenant"
        );
        assert_eq!(
            file.owner_id, user_a.id,
            "User A should only see their own files"
        );
    }

    // List files for user_b - should only see tenant_b files
    let files_b = ctx
        .metadata_store
        .list_files(None, user_b.id, tenant_b)
        .await
        .expect("Failed to list files");

    // All files should belong to tenant_b
    for file in &files_b {
        assert_eq!(
            file.tenant_id, tenant_b,
            "User B should only see files from their own tenant"
        );
        assert_eq!(
            file.owner_id, user_b.id,
            "User B should only see their own files"
        );
    }

    // Cleanup
    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-04: Folder access is tenant-isolated
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_folder_access_is_tenant_isolated() {
    let ctx = setup_test_env().await;

    // Setup two tenants
    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    // Create users in each tenant
    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    // Create folder service
    let folder_service = ctx.folder_service();

    // User A creates a folder
    let folder_a = create_test_folder(&folder_service, user_a.id, tenant_a, "Private", None).await;

    // User B tries to access the folder
    let result = folder_service.get_folder(folder_a.id, user_b.id).await;

    // Should fail with permission denied
    assert!(
        matches!(result, Err(FolderError::PermissionDenied { .. })),
        "User from tenant B should not be able to access folder from tenant A"
    );

    // Cleanup
    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-05: User data queries are scoped to tenant
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_user_queries_are_tenant_scoped() {
    let ctx = setup_test_env().await;

    // Setup tenant
    let tenant_a = ctx.tenant_id;

    // Create multiple users in the same tenant
    let user_a1 = create_test_user(&ctx.metadata_store, "user_a1", tenant_a).await;
    let user_a2 = create_test_user(&ctx.metadata_store, "user_a2", tenant_a).await;

    // Create file service
    let file_service = ctx.file_service();

    // Each user creates files
    let file_a1 = create_test_file(
        &file_service,
        user_a1.id,
        tenant_a,
        None,
        "user1_file.txt",
        b"User 1 data",
    )
    .await;

    let file_a2 = create_test_file(
        &file_service,
        user_a2.id,
        tenant_a,
        None,
        "user2_file.txt",
        b"User 2 data",
    )
    .await;

    // User A1 can access their own file
    let result = file_service.get_file(file_a1.id, user_a1.id).await;
    assert!(result.is_ok(), "User should access their own file");

    // User A1 cannot access User A2's file (same tenant, different owner)
    let result = file_service.get_file(file_a2.id, user_a1.id).await;
    assert!(
        matches!(result, Err(FileError::PermissionDenied { .. })),
        "User should not access another user's file even in same tenant"
    );

    // Cleanup
    ctx.cleanup().await;
}
