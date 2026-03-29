//! Search Authorization Contract Tests (Q-01 and Q-02)
//!
//! Tests search functionality with proper authorization:
//! - Q-01: Search returns files user has access to
//! - Q-02: Search excludes files user cannot access
//! - Q-03: Deleted files do not appear in search

use crate::contracts::common::*;

/// Q-01-01: Search returns files user has access to (own files)
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_search_returns_own_files() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "search_user", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Create files with distinct names
    let file1 = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "project_report.txt",
        b"Project report content",
    )
    .await;

    let file2 = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "project_plan.txt",
        b"Project plan content",
    )
    .await;

    // List files (simulating search for user's files)
    let files = ctx
        .metadata_store
        .list_files(None, user.id, tenant_id)
        .await
        .expect("Failed to list files");

    // Should find both files
    assert_eq!(files.len(), 2, "Should find both files");

    let file_names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    assert!(file_names.contains(&"project_report.txt"));
    assert!(file_names.contains(&"project_plan.txt"));

    // All returned files should be owned by the user
    for file in &files {
        assert_eq!(
            file.owner_id, user.id,
            "Search should only return user's files"
        );
    }

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// Q-02-01: Search excludes files user cannot access
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_search_excludes_other_users_files() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create two users
    let user_a = create_test_user(&ctx.metadata_store, "search_user_a", tenant_id).await;
    let user_b = create_test_user(&ctx.metadata_store, "search_user_b", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // User A creates files
    let _file_a1 = create_test_file(
        &file_service,
        user_a.id,
        tenant_id,
        None,
        "confidential_a.txt",
        b"User A confidential",
    )
    .await;

    let _file_a2 = create_test_file(
        &file_service,
        user_a.id,
        tenant_id,
        None,
        "secret_a.txt",
        b"User A secret",
    )
    .await;

    // User B creates a file
    let _file_b1 = create_test_file(
        &file_service,
        user_b.id,
        tenant_id,
        None,
        "document_b.txt",
        b"User B document",
    )
    .await;

    // User A lists files
    let files_a = ctx
        .metadata_store
        .list_files(None, user_a.id, tenant_id)
        .await
        .expect("Failed to list files");

    // User A should only see their own files
    assert_eq!(files_a.len(), 2, "User A should see only their 2 files");
    for file in &files_a {
        assert_eq!(
            file.owner_id, user_a.id,
            "User A should not see User B's files"
        );
    }

    // User B lists files
    let files_b = ctx
        .metadata_store
        .list_files(None, user_b.id, tenant_id)
        .await
        .expect("Failed to list files");

    // User B should only see their own file
    assert_eq!(files_b.len(), 1, "User B should see only their 1 file");
    assert_eq!(files_b[0].owner_id, user_b.id);

    // Cleanup
    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// Q-02-02: Search across tenants is isolated
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_search_is_tenant_isolated() {
    let ctx = setup_test_env().await;

    // Setup two tenants
    let tenant_a = setup_test_tenant(&ctx.pool).await;
    let tenant_b = setup_test_tenant(&ctx.pool).await;

    // Create users in each tenant
    let user_a = create_test_user(&ctx.metadata_store, "tenant_a_searcher", tenant_a).await;
    let user_b = create_test_user(&ctx.metadata_store, "tenant_b_searcher", tenant_b).await;

    // Create file service
    let file_service = ctx.file_service();

    // Create files in each tenant
    let _file_a = create_test_file(
        &file_service,
        user_a.id,
        tenant_a,
        None,
        "tenant_a_doc.txt",
        b"Tenant A document",
    )
    .await;

    let _file_b = create_test_file(
        &file_service,
        user_b.id,
        tenant_b,
        None,
        "tenant_b_doc.txt",
        b"Tenant B document",
    )
    .await;

    // Search in tenant A
    let files_a = ctx
        .metadata_store
        .list_files(None, user_a.id, tenant_a)
        .await
        .expect("Failed to list files");

    // Should only find tenant A files
    for file in &files_a {
        assert_eq!(
            file.tenant_id, tenant_a,
            "Search in tenant A should not return tenant B files"
        );
    }

    // Search in tenant B
    let files_b = ctx
        .metadata_store
        .list_files(None, user_b.id, tenant_b)
        .await
        .expect("Failed to list files");

    // Should only find tenant B files
    for file in &files_b {
        assert_eq!(
            file.tenant_id, tenant_b,
            "Search in tenant B should not return tenant A files"
        );
    }

    // Cleanup
    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, tenant_a).await;
    cleanup_tenant(&ctx.pool, tenant_b).await;
}

/// Q-03-01: Deleted files do not appear in search
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_deleted_files_not_in_search() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "delete_search_user", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Create files
    let file1 = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "keep_me.txt",
        b"Keep this file",
    )
    .await;

    let file2 = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "delete_me.txt",
        b"Delete this file",
    )
    .await;

    // Verify both files exist
    let files_before = ctx
        .metadata_store
        .list_files(None, user.id, tenant_id)
        .await
        .expect("Failed to list files");
    assert_eq!(files_before.len(), 2);

    // Delete file2
    file_service
        .delete_file(file2.id, user.id)
        .await
        .expect("Failed to delete file");

    // List files again
    let files_after = ctx
        .metadata_store
        .list_files(None, user.id, tenant_id)
        .await
        .expect("Failed to list files");

    // Should only have file1
    assert_eq!(files_after.len(), 1);
    assert_eq!(files_after[0].id, file1.id);
    assert_eq!(files_after[0].name, "keep_me.txt");

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// Q-01-02: Search in folders returns folder contents
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_search_in_folders() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "folder_search_user", tenant_id).await;

    // Create folder service and folders
    let folder_service = ctx.folder_service();
    let folder1 = create_test_folder(&folder_service, user.id, tenant_id, "Work", None).await;
    let folder2 = create_test_folder(&folder_service, user.id, tenant_id, "Personal", None).await;

    // Create file service
    let file_service = ctx.file_service();

    // Create files in different folders
    let _file1 = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        Some(folder1.id),
        "work_doc.txt",
        b"Work document",
    )
    .await;

    let _file2 = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        Some(folder2.id),
        "personal_doc.txt",
        b"Personal document",
    )
    .await;

    let _file3 = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "root_doc.txt",
        b"Root document",
    )
    .await;

    // List files in folder1
    let work_files = ctx
        .metadata_store
        .list_files(Some(folder1.id), user.id, tenant_id)
        .await
        .expect("Failed to list files");

    assert_eq!(work_files.len(), 1);
    assert_eq!(work_files[0].name, "work_doc.txt");

    // List files in folder2
    let personal_files = ctx
        .metadata_store
        .list_files(Some(folder2.id), user.id, tenant_id)
        .await
        .expect("Failed to list files");

    assert_eq!(personal_files.len(), 1);
    assert_eq!(personal_files[0].name, "personal_doc.txt");

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// Q-02-03: Search respects folder permissions
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_search_respects_folder_permissions() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create users
    let owner = create_test_user(&ctx.metadata_store, "folder_owner", tenant_id).await;
    let other = create_test_user(&ctx.metadata_store, "folder_other", tenant_id).await;

    // Create folder service
    let folder_service = ctx.folder_service();
    let folder = create_test_folder(&folder_service, owner.id, tenant_id, "Private", None).await;

    // Create file service
    let file_service = ctx.file_service();

    // Create file in folder
    let _file = create_test_file(
        &file_service,
        owner.id,
        tenant_id,
        Some(folder.id),
        "private_file.txt",
        b"Private content",
    )
    .await;

    // Owner can list files in their folder
    let owner_files = ctx
        .metadata_store
        .list_files(Some(folder.id), owner.id, tenant_id)
        .await
        .expect("Failed to list files");

    assert_eq!(owner_files.len(), 1);

    // Other user cannot access owner's folder
    let folder_access = folder_service.get_folder(folder.id, other.id).await;
    assert!(
        folder_access.is_err(),
        "Other user should not access owner's folder"
    );

    // Cleanup
    cleanup_user(&ctx.pool, owner.id).await;
    cleanup_user(&ctx.pool, other.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}
