//! File Versioning Contract Tests (F-01 through F-04)
//!
//! Tests file versioning functionality:
//! - F-01: File create creates exactly one version
//! - F-02: File replace creates new version, keeps old
//! - F-03: File restore returns correct version
//! - F-04: Version history is immutable

use crate::common::*;

/// F-01: File create creates exactly one version
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_file_create_creates_exactly_one_version() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "version_user", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Upload a new file
    let file = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "versioned_doc.txt",
        b"Initial content",
    )
    .await;

    // Verify file has version 1
    assert_eq!(file.current_version, 1, "New file should have version 1");

    // List versions
    let versions = file_service
        .list_versions(file.id, user.id)
        .await
        .expect("Failed to list versions");

    // Should have exactly one version
    assert_eq!(
        versions.len(),
        1,
        "New file should have exactly one version"
    );
    assert_eq!(versions[0].version_number, 1);

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// F-02: File replace creates new version, keeps old
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_file_replace_creates_new_version_keeps_old() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "update_user", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Upload initial file (v1)
    let file = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "updatable_doc.txt",
        b"Version 1 content",
    )
    .await;

    let v1_hash = file.content_hash.clone();

    // Update file to create v2
    let file = file_service
        .update_file(
            file.id,
            user.id,
            1, // expected version
            bytes::Bytes::from("Version 2 content"),
        )
        .await
        .expect("Failed to update file");

    assert_eq!(file.current_version, 2);
    let v2_hash = file.content_hash.clone();
    assert_ne!(v2_hash, v1_hash, "Content hash should change");

    // Update file to create v3
    let file = file_service
        .update_file(
            file.id,
            user.id,
            2, // expected version
            bytes::Bytes::from("Version 3 content"),
        )
        .await
        .expect("Failed to update file");

    assert_eq!(file.current_version, 3);
    let v3_hash = file.content_hash;
    assert_ne!(v3_hash, v2_hash, "Content hash should change");

    // List all versions
    let versions = file_service
        .list_versions(file.id, user.id)
        .await
        .expect("Failed to list versions");

    // Should have 3 versions
    assert_eq!(versions.len(), 3, "Should have 3 versions");

    // Verify version numbers (newest first)
    assert_eq!(versions[0].version_number, 3);
    assert_eq!(versions[1].version_number, 2);
    assert_eq!(versions[2].version_number, 1);

    // Verify content hashes are preserved
    assert_eq!(versions[0].content_hash, v3_hash);
    assert_eq!(versions[1].content_hash, v2_hash);
    assert_eq!(versions[2].content_hash, v1_hash);

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// F-03: File restore returns correct version
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_file_restore_returns_correct_version() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "restore_user", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Upload initial file (v1)
    let file = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "restorable_doc.txt",
        b"Original content",
    )
    .await;

    let v1_hash = file.content_hash.clone();

    // Update to v2
    let file = file_service
        .update_file(file.id, user.id, 1, bytes::Bytes::from("Modified content"))
        .await
        .expect("Failed to update file");

    assert_eq!(file.current_version, 2);

    // Update to v3
    let file = file_service
        .update_file(
            file.id,
            user.id,
            2,
            bytes::Bytes::from("Further modified content"),
        )
        .await
        .expect("Failed to update file");

    assert_eq!(file.current_version, 3);

    // Restore to v1 (creates v4 with v1's content)
    let restored_file = file_service
        .restore_version(file.id, 1, user.id)
        .await
        .expect("Failed to restore version");

    // Verify restored file
    assert_eq!(
        restored_file.current_version, 4,
        "Restore should create new version"
    );
    assert_eq!(
        restored_file.content_hash, v1_hash,
        "Restored file should have v1's content hash"
    );

    // List all versions
    let versions = file_service
        .list_versions(file.id, user.id)
        .await
        .expect("Failed to list versions");

    // Should have 4 versions
    assert_eq!(versions.len(), 4, "Should have 4 versions after restore");

    // Verify v4 has same content as v1
    assert_eq!(versions[0].content_hash, v1_hash);
    assert_eq!(versions[0].version_number, 4);

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// F-04: Version history is immutable
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_version_history_is_immutable() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "immutable_user", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Upload initial file (v1)
    let file = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "immutable_doc.txt",
        b"Immutable content",
    )
    .await;

    let v1_hash = file.content_hash.clone();

    // Update to v2
    let file = file_service
        .update_file(file.id, user.id, 1, bytes::Bytes::from("Modified content"))
        .await
        .expect("Failed to update file");

    let v2_hash = file.content_hash.clone();

    // Get versions
    let versions = file_service
        .list_versions(file.id, user.id)
        .await
        .expect("Failed to list versions");

    // Verify v1 hash has not changed
    let v1_version = versions
        .iter()
        .find(|v| v.version_number == 1)
        .expect("v1 should exist");
    assert_eq!(
        v1_version.content_hash, v1_hash,
        "v1 hash should be unchanged"
    );

    // Verify v2 hash
    let v2_version = versions
        .iter()
        .find(|v| v.version_number == 2)
        .expect("v2 should exist");
    assert_eq!(v2_version.content_hash, v2_hash);

    // Version IDs should be unique and immutable
    assert_ne!(
        v1_version.id, v2_version.id,
        "Each version should have unique ID"
    );

    // Restore to v1 (creates v3)
    let restored_file = file_service
        .restore_version(file.id, 1, user.id)
        .await
        .expect("Failed to restore");

    assert_eq!(restored_file.current_version, 3);

    // Original v1 should still exist unchanged
    let versions = file_service
        .list_versions(file.id, user.id)
        .await
        .expect("Failed to list versions");

    let v1_still = versions
        .iter()
        .find(|v| v.version_number == 1)
        .expect("v1 should still exist");
    assert_eq!(v1_still.content_hash, v1_hash, "v1 should be immutable");

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// Additional: Concurrent updates with optimistic locking
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_concurrent_update_optimistic_locking() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "concurrent_user", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Upload initial file
    let file = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "concurrent_doc.txt",
        b"Initial content",
    )
    .await;

    // Try to update with wrong expected version
    let result = file_service
        .update_file(
            file.id,
            user.id,
            5, // Wrong expected version (should be 1)
            bytes::Bytes::from("New content"),
        )
        .await;

    assert!(
        matches!(
            result,
            Err(rustshare_core::services::FileError::VersionConflict { .. })
        ),
        "Update with wrong version should fail with VersionConflict"
    );

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}

/// Additional: Version ordering is maintained
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_version_ordering_is_sequential() {
    let ctx = setup_test_env().await;
    let tenant_id = setup_test_tenant(&ctx.pool).await;

    // Create user
    let user = create_test_user(&ctx.metadata_store, "ordering_user", tenant_id).await;

    // Create file service
    let file_service = ctx.file_service();

    // Upload initial file
    let file = create_test_file(
        &file_service,
        user.id,
        tenant_id,
        None,
        "ordered_doc.txt",
        b"v1 content",
    )
    .await;

    // Create multiple versions
    let mut current_version = 1;
    for i in 2..=5 {
        let file = file_service
            .update_file(
                file.id,
                user.id,
                current_version,
                bytes::Bytes::from(format!("v{} content", i)),
            )
            .await
            .expect("Failed to update");

        assert_eq!(file.current_version, i);
        current_version = i;
    }

    // List versions and verify ordering
    let versions = file_service
        .list_versions(file.id, user.id)
        .await
        .expect("Failed to list versions");

    assert_eq!(versions.len(), 5);

    // Versions should be ordered newest first
    for (i, version) in versions.iter().enumerate() {
        let expected_version = 5 - i; // 5, 4, 3, 2, 1
        assert_eq!(
            version.version_number, expected_version as i32,
            "Version ordering should be sequential"
        );
    }

    // Cleanup
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}
