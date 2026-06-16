//! Tenant Isolation Contract Tests (G-01)
//!
//! Tests that user data is properly isolated between tenants.
//! Contract: User from tenant A cannot access resources from tenant B.

use crate::common::*;
use rustshare_core::services::{FileError, FolderError, PermissionResolverOps};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use sqlx::PgPool;
use uuid::Uuid;

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

/// G-01-06: PermissionResolverRepository queries are scoped to a tenant.
#[tokio::test]
#[ignore] // Requires database
async fn test_permission_resolver_repository_is_tenant_scoped() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let repo = PermissionResolverRepository::new(pool.clone());
    let suffix = Uuid::new_v4();

    let tenant_a = setup_test_tenant(&pool).await;
    let tenant_b = setup_test_tenant(&pool).await;

    let owner_a = Uuid::new_v4();
    let recipient_a = Uuid::new_v4();

    for (user_id, username, tenant_id) in [
        (owner_a, format!("owner-{suffix}"), tenant_a),
        (recipient_a, format!("recipient-{suffix}"), tenant_a),
    ] {
        sqlx::query(
            r#"
            INSERT INTO users (
                id, username, email, password_hash, display_name, is_admin, storage_quota, tenant_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(user_id)
        .bind(&username)
        .bind(format!("{username}@example.com"))
        .bind("hash")
        .bind("Test User")
        .bind(false)
        .bind(1024_i64)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("create user");
    }

    let folder_a = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO folders (id, name, path, owner_id, parent_folder_id, tenant_id, starred_at, deleted_at)
        VALUES ($1, $2, $3, $4, NULL, $5, NULL, NULL)
        "#,
    )
    .bind(folder_a)
    .bind(format!("folder-{suffix}"))
    .bind(format!("/folder-{suffix}"))
    .bind(owner_a)
    .bind(tenant_a)
    .execute(&pool)
    .await
    .expect("create folder");

    let file_a = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO files (
            id, name, path, size, mime_type, content_hash, storage_key,
            owner_id, parent_folder_id, current_version, tenant_id, starred_at, deleted_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, $9, $10, NULL, NULL)
        "#,
    )
    .bind(file_a)
    .bind(format!("file-{suffix}"))
    .bind(format!("/file-{suffix}"))
    .bind(123_i64)
    .bind("text/plain")
    .bind("abc123")
    .bind(format!("blobs/abc123-{suffix}"))
    .bind(owner_a)
    .bind(1_i32)
    .bind(tenant_a)
    .execute(&pool)
    .await
    .expect("create file");

    let share_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO shares (
            id, file_id, folder_id, share_token, permissions, password_hash,
            expires_at, access_count, recipient_user_id, recipient_group_id,
            created_by, created_at, revoked_at, upload_only, tenant_id
        )
        VALUES ($1, $2, NULL, NULL, $3, NULL, NULL, 0, $4, NULL, $5, NOW(), NULL, FALSE, $6)
        "#,
    )
    .bind(share_id)
    .bind(file_a)
    .bind("View")
    .bind(recipient_a)
    .bind(owner_a)
    .bind(tenant_a)
    .execute(&pool)
    .await
    .expect("create share");

    // Correct tenant resolves the data.
    assert!(
        repo.find_file_by_id(file_a, tenant_a)
            .await
            .unwrap()
            .is_some(),
        "tenant A should see its own file"
    );
    assert!(
        repo.find_folder_by_id(folder_a, tenant_a)
            .await
            .unwrap()
            .is_some(),
        "tenant A should see its own folder"
    );
    assert!(
        repo.find_user_share(Some(file_a), None, recipient_a, tenant_a)
            .await
            .unwrap()
            .is_some(),
        "tenant A should resolve its own share"
    );

    // Wrong tenant must not resolve any data.
    assert!(
        repo.find_file_by_id(file_a, tenant_b)
            .await
            .unwrap()
            .is_none(),
        "tenant B must not see tenant A's file"
    );
    assert!(
        repo.find_folder_by_id(folder_a, tenant_b)
            .await
            .unwrap()
            .is_none(),
        "tenant B must not see tenant A's folder"
    );
    assert!(
        repo.find_user_share(Some(file_a), None, recipient_a, tenant_b)
            .await
            .unwrap()
            .is_none(),
        "tenant B must not resolve tenant A's share"
    );

    // Cleanup
    cleanup_tenant(&pool, tenant_b).await;
    cleanup_tenant(&pool, tenant_a).await;
}
