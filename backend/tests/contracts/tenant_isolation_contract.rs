//! Tenant Isolation Contract Tests (G-01)
//!
//! Tests that user data is properly isolated between tenants.
//! Contract: User from tenant A cannot access resources from tenant B.

use crate::common::*;
use rustshare_core::domain::SharePermissions;
use rustshare_core::services::{
    FileError, FolderError, NotificationError, PermissionResolverOps, Resource, ShareError,
};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Mock JWT manager used by the share service in contract tests.
struct MockJwtManager;

impl rustshare_core::services::JwtOps for MockJwtManager {
    fn encode_custom_claims<T: serde::Serialize>(&self, _claims: &T) -> Result<String, String> {
        Ok("test_jwt_token".to_string())
    }
}

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

    // User A creates a file in tenant A
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        user_a.id,
        tenant_a,
        None,
        "secret.txt",
        b"Tenant A secret",
    )
    .await;

    // User A creates a public share link for the file
    let share_service = create_test_share_service(&ctx, Arc::new(MockJwtManager));
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
    let token = share.share_token.expect("Public share should have a token");

    // Attempt to access the share link from tenant B. Public share resolution
    // must be tenant-scoped, so the token should not resolve outside tenant A.
    let result = share_service
        .validate_and_create_session(&token, None, Some(tenant_b))
        .await;

    assert!(
        matches!(result, Err(ShareError::ShareNotFoundByToken(_))),
        "Cross-tenant share link access should be denied; got {:?}",
        result
    );

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

/// G-01-07: User from tenant B cannot list files in tenant A
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_user_cannot_list_files_from_other_tenant() {
    let ctx = setup_test_env().await;

    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    let file_service = ctx.file_service();
    let file_a = create_test_file(
        &file_service,
        user_a.id,
        tenant_a,
        None,
        "confidential.txt",
        b"Tenant A data",
    )
    .await;

    let files_b = ctx
        .metadata_store
        .list_files(None, user_b.id, tenant_b)
        .await
        .expect("Failed to list files for tenant B");

    assert!(
        files_b.iter().all(|f| f.id != file_a.id),
        "User B should not see files from tenant A"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-08: User from tenant B cannot rename/update a file in tenant A
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_user_cannot_update_file_from_other_tenant() {
    let ctx = setup_test_env().await;

    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    let file_service = ctx.file_service();
    let file_a = create_test_file(
        &file_service,
        user_a.id,
        tenant_a,
        None,
        "confidential.txt",
        b"Tenant A data",
    )
    .await;

    let result = file_service
        .rename_file(file_a.id, "renamed_by_b.txt".to_string(), user_b.id)
        .await;

    assert!(
        matches!(result, Err(FileError::PermissionDenied { .. })),
        "User B should not be able to rename a file in tenant A"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-09: User from tenant B cannot delete a file in tenant A
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_user_cannot_delete_file_from_other_tenant() {
    let ctx = setup_test_env().await;

    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    let file_service = ctx.file_service();
    let file_a = create_test_file(
        &file_service,
        user_a.id,
        tenant_a,
        None,
        "confidential.txt",
        b"Tenant A data",
    )
    .await;

    let result = file_service.delete_file(file_a.id, user_b.id).await;

    assert!(
        matches!(result, Err(FileError::PermissionDenied { .. })),
        "User B should not be able to delete a file in tenant A"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-10: User from tenant B cannot list folders in tenant A
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_user_cannot_list_folders_from_other_tenant() {
    let ctx = setup_test_env().await;

    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    let folder_service = ctx.folder_service();
    let folder_a = create_test_folder(&folder_service, user_a.id, tenant_a, "Private", None).await;

    let folders_b = ctx
        .metadata_store
        .list_folders(None, user_b.id, tenant_b)
        .await
        .expect("Failed to list folders for tenant B");

    assert!(
        folders_b.iter().all(|f| f.id != folder_a.id),
        "User B should not see folders from tenant A"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-11: User from tenant B cannot rename a folder in tenant A
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_user_cannot_update_folder_from_other_tenant() {
    let ctx = setup_test_env().await;

    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    let folder_service = ctx.folder_service();
    let folder_a = create_test_folder(&folder_service, user_a.id, tenant_a, "Private", None).await;

    let result = folder_service
        .rename_folder(folder_a.id, "RenamedByB".to_string(), user_b.id)
        .await;

    assert!(
        matches!(result, Err(FolderError::PermissionDenied { .. })),
        "User B should not be able to rename a folder in tenant A"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-12: User from tenant B cannot delete a folder in tenant A
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_user_cannot_delete_folder_from_other_tenant() {
    let ctx = setup_test_env().await;

    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    let folder_service = ctx.folder_service();
    let folder_a = create_test_folder(&folder_service, user_a.id, tenant_a, "Private", None).await;

    let result = folder_service.delete_folder(folder_a.id, user_b.id).await;

    assert!(
        matches!(result, Err(FolderError::PermissionDenied { .. })),
        "User B should not be able to delete a folder in tenant A"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-13: Notifications do not leak across tenants
#[tokio::test]
#[ignore] // Requires database
async fn test_notifications_do_not_leak_across_tenants() {
    let ctx = setup_test_env().await;

    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    let notification_service = ctx.notification_service();
    let notification = create_test_notification(&notification_service, user_a.id, tenant_a).await;

    // User B listing notifications in tenant B must not see tenant A's notification.
    let notifications_b = notification_service
        .list_notifications(user_b.id, tenant_b, false, 100, 0)
        .await
        .expect("Failed to list notifications for tenant B");

    assert!(
        notifications_b.is_empty(),
        "User B should not see notifications from tenant A"
    );

    // Direct fetch by user B in tenant B should also fail to find it.
    let result = notification_service
        .get_notification(notification.id, user_b.id, tenant_b)
        .await;

    assert!(
        matches!(result, Err(NotificationError::NotFoundById(_))),
        "User B should not be able to retrieve a tenant A notification"
    );

    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

// ---------------------------------------------------------------------------
// AR-03: tenant-safe groups and sharing
// ---------------------------------------------------------------------------

/// G-01-14: Group-share creation with a group from another organization fails
/// closed and writes no share row. This is the service-level analogue of the
/// cross-organization add-member refusal: the grant (group membership) must
/// not bridge organizations.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_group_share_with_foreign_group_refused_no_share_row() {
    let ctx = setup_test_env().await;

    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    // Resource owned by tenant A's user.
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        user_a.id,
        tenant_a,
        None,
        "tenant_a_secret.txt",
        b"Tenant A secret",
    )
    .await;

    // Group that belongs to organization B only.
    let foreign_group_id =
        create_test_group(&ctx.pool, "foreign_org_group", tenant_b, user_b.id).await;

    let share_service = create_test_share_service(&ctx, Arc::new(MockJwtManager));
    let result = share_service
        .create_group_share(
            Resource::File(file.id),
            foreign_group_id,
            SharePermissions::View,
            user_a.id,
            tenant_a,
        )
        .await;

    assert!(
        matches!(result, Err(ShareError::CrossTenantSharingNotAllowed)),
        "Group-share creation with a foreign-organization group must be refused; got {:?}",
        result
    );

    let share_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shares WHERE file_id = $1")
        .bind(file.id)
        .fetch_one(&ctx.pool)
        .await
        .expect("Failed to count share rows");
    assert_eq!(
        share_count, 0,
        "A refused cross-organization group share must not write a share row"
    );

    // Cleanup
    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-15: Public-link creation (create_share) for a resource of another
/// organization is refused and writes no share row.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_public_link_for_foreign_resource_refused_no_share_row() {
    let ctx = setup_test_env().await;

    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let _user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    // Resource owned by tenant A's user, in organization A.
    let file_service = ctx.file_service();
    let file = create_test_file(
        &file_service,
        user_a.id,
        tenant_a,
        None,
        "tenant_a_only.txt",
        b"Tenant A only",
    )
    .await;

    // The owner requests a public link while presenting another organization:
    // the link targets a foreign-tenant resource and must fail closed.
    let share_service = create_test_share_service(&ctx, Arc::new(MockJwtManager));
    let result = share_service
        .create_share(
            file.id,
            user_a.id,
            SharePermissions::View,
            None,
            None,
            tenant_b,
        )
        .await;

    assert!(
        matches!(result, Err(ShareError::CrossTenantSharingNotAllowed)),
        "Public-link creation for a foreign-organization resource must be refused; got {:?}",
        result
    );

    let share_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shares WHERE file_id = $1")
        .bind(file.id)
        .fetch_one(&ctx.pool)
        .await
        .expect("Failed to count share rows");
    assert_eq!(
        share_count, 0,
        "A refused cross-organization public link must not write a share row"
    );

    // Cleanup
    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}

/// G-01-16: The group roster query must not disclose foreign-organization
/// principals. The query text below replicates the tenant-join form of the
/// roster query in `rustshare_server::handlers::groups::get_my_group` — the
/// contract suite cannot import the handler's private SQL, so the text is
/// kept in sync verbatim; a drift in the handler will fail the HTTP-level
/// regression tests in admin_groups_test.
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_group_roster_query_excludes_foreign_principals() {
    let ctx = setup_test_env().await;

    let tenant_a = ctx.tenant_id;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_user", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_user", tenant_b).await;

    let group_id = create_test_group(&ctx.pool, "roster_scope_group", tenant_a, user_a.id).await;
    add_user_to_group(&ctx.pool, group_id, user_a.id).await;
    // Seed the mismatched legacy membership row directly, as a pre-AR-03 row
    // would exist: the raw group_members table now holds 2 rows.
    add_user_to_group(&ctx.pool, group_id, user_b.id).await;

    let raw_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = $1")
            .bind(group_id)
            .fetch_one(&ctx.pool)
            .await
            .expect("Failed to count raw membership rows");
    assert_eq!(raw_count, 2, "fixture must seed both membership rows");

    let roster: Vec<(uuid::Uuid, String, String)> = sqlx::query_as(
        r#"
        SELECT u.id AS user_id, u.username, u.email
        FROM group_members m
        JOIN users u ON u.id = m.user_id
        WHERE m.group_id = $1
          AND u.tenant_id = $2
        ORDER BY m.added_at ASC
        "#,
    )
    .bind(group_id)
    .bind(tenant_a)
    .fetch_all(&ctx.pool)
    .await
    .expect("Failed to run the tenant-scoped roster query");

    let roster_ids: Vec<uuid::Uuid> = roster.iter().map(|row| row.0).collect();
    assert!(
        roster_ids.contains(&user_a.id),
        "roster must include the same-organization member"
    );
    assert!(
        !roster_ids.contains(&user_b.id),
        "roster must not disclose the foreign-organization principal"
    );
    assert_eq!(
        roster_ids.len(),
        1,
        "the tenant-scoped roster must return exactly one principal"
    );

    // Cleanup
    cleanup_tenant(&ctx.pool, tenant_b).await;
    ctx.cleanup().await;
}
