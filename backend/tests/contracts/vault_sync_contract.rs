//! Contract tests for Vault Sync API
//!
//! These tests require a running database and S3-compatible storage.
//! Run with: cargo test --test contracts vault_sync -- --ignored

use bytes::Bytes;
use chrono::Utc;
use rustshare_core::domain::{
    CreateVaultRequest, DeleteVaultFileRequest, RenameVaultFileRequest, UploadVaultFileRequest,
    VaultAdapter, VaultDevice,
};
use rustshare_core::services::{ObjectStoreOps, VaultStore, VaultSyncService};
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

async fn setup_test_env() -> (
    PgPool,
    Arc<EventStore>,
    Arc<MetadataStore>,
    Arc<ObjectStore>,
) {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    let pool = PgPool::connect(&database_url)
        .await
        .expect("DB connect failed");
    let event_store = Arc::new(EventStore::new(pool.clone()));
    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));
    let object_store = Arc::new(
        ObjectStore::new(
            std::env::var("RUSTFS_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:9000".to_string()),
            std::env::var("RUSTFS_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            std::env::var("RUSTFS_BUCKET").unwrap_or_else(|_| "rustshare-data".to_string()),
        )
        .await
        .expect("S3 connect failed"),
    );
    (pool, event_store, metadata_store, object_store)
}

async fn create_test_user(
    metadata_store: &MetadataStore,
    username: &str,
    tenant_id: Uuid,
) -> rustshare_core::domain::User {
    let unique = format!("{}-{}", username, Uuid::new_v4());
    let user = rustshare_core::domain::User::new(
        unique.clone(),
        format!("{} Display", unique),
        "test_password_hash".to_string(),
        format!("{}@test.local", unique),
        false,
        10_737_418_240, // 10GB
        tenant_id,
    );

    metadata_store
        .create_user(&user)
        .await
        .expect("Failed to create test user");

    user
}

async fn create_test_device<S, O>(
    service: &VaultSyncService<S, O>,
    user_id: Uuid,
    tenant_id: Uuid,
) -> rustshare_core::domain::VaultDevice
where
    S: VaultStore,
    O: ObjectStoreOps,
{
    let device = rustshare_core::domain::VaultDevice {
        id: Uuid::new_v4(),
        tenant_id,
        user_id,
        vault_id: None,
        device_name: "Test Device".to_string(),
        client_type: "obsidian".to_string(),
        client_version: Some("1.0.0".to_string()),
        last_sync_rev: None,
        revoked_at: None,
        created_at: Utc::now(),
        last_seen_at: Utc::now(),
    };
    service
        .register_device(device.clone(), user_id)
        .await
        .expect("Failed to register test device");
    device
}

async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
    sqlx::query(
        "DELETE FROM vault_files WHERE tenant_id IN (SELECT tenant_id FROM users WHERE id = $1)",
    )
    .bind(user_id)
    .execute(pool)
    .await
    .ok();
    sqlx::query("DELETE FROM vaults WHERE owner_user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM vault_devices WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM files WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM folders WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
}

/// VS-01: Create and retrieve a vault
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_create_and_get_vault() {
    let (pool, _event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "vault_user", tenant_id).await;

    let service = VaultSyncService::new(metadata_store.clone(), object_store.clone());
    let vault = service
        .create_vault(
            CreateVaultRequest {
                name: "Test Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: "test-device".to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    assert_eq!(vault.name, "Test Vault");
    assert_eq!(vault.adapter, VaultAdapter::ObsidianVault);
    assert_eq!(vault.server_rev, 0);

    let retrieved = service
        .get_vault(vault.id, tenant_id, user.id)
        .await
        .expect("Failed to get vault");
    assert_eq!(retrieved.id, vault.id);
    assert_eq!(retrieved.name, "Test Vault");
    assert_eq!(retrieved.server_rev, 0);

    cleanup_user(&pool, user.id).await;
}

/// VS-02: Upload a file and download it back
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_and_download_file() {
    let (pool, _event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "upload_user", tenant_id).await;

    let service = VaultSyncService::new(metadata_store.clone(), object_store.clone());
    let device = create_test_device(&service, user.id, tenant_id).await;
    let vault = service
        .create_vault(
            CreateVaultRequest {
                name: "Upload Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content = Bytes::from("Hello, vault sync!");
    let sha256 = hex::encode(Sha256::digest(&content));

    let file = service
        .upload_file(
            UploadVaultFileRequest {
                vault_id: vault.id,
                relative_path: "notes/hello.md".to_string(),
                content_type: Some("text/markdown".to_string()),
                sha256: sha256.clone(),
                size: content.len() as i64,
                base_server_rev: 0,
                device_id: device.id.to_string(),
                content: content.clone(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to upload file");

    assert_eq!(file.relative_path, "notes/hello.md");
    assert_eq!(file.server_rev, 1);

    let (downloaded, _content_type) = service
        .download_file(vault.id, "notes/hello.md", tenant_id, user.id)
        .await
        .expect("Failed to download file");
    assert_eq!(downloaded, content);

    cleanup_user(&pool, user.id).await;
}

/// VS-03: Concurrent upload conflict detection
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_conflict() {
    let (pool, _event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "conflict_user", tenant_id).await;

    let service = VaultSyncService::new(metadata_store.clone(), object_store.clone());
    let device = create_test_device(&service, user.id, tenant_id).await;
    let vault = service
        .create_vault(
            CreateVaultRequest {
                name: "Conflict Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content1 = Bytes::from("First version");
    let sha1 = hex::encode(Sha256::digest(&content1));

    service
        .upload_file(
            UploadVaultFileRequest {
                vault_id: vault.id,
                relative_path: "notes/conflict.md".to_string(),
                content_type: Some("text/markdown".to_string()),
                sha256: sha1,
                size: content1.len() as i64,
                base_server_rev: 0,
                device_id: device.id.to_string(),
                content: content1,
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to upload first version");

    let content2 = Bytes::from("Second version attempt");
    let sha2 = hex::encode(Sha256::digest(&content2));

    let result = service
        .upload_file(
            UploadVaultFileRequest {
                vault_id: vault.id,
                relative_path: "notes/conflict.md".to_string(),
                content_type: Some("text/markdown".to_string()),
                sha256: sha2,
                size: content2.len() as i64,
                base_server_rev: 0, // stale: file is now at rev 1
                device_id: device.id.to_string(),
                content: content2,
            },
            tenant_id,
            user.id,
        )
        .await;

    assert!(
        matches!(
            result,
            Err(rustshare_core::services::VaultSyncError::Conflict { .. })
        ),
        "Expected conflict error for stale base_server_rev"
    );

    cleanup_user(&pool, user.id).await;
}

/// VS-03b: Upload a new file with base_server_rev == 1 succeeds
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_new_file_with_base_rev_one() {
    let (pool, _event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "base_rev_one_user", tenant_id).await;

    let service = VaultSyncService::new(metadata_store.clone(), object_store.clone());
    let device = create_test_device(&service, user.id, tenant_id).await;
    let vault = service
        .create_vault(
            CreateVaultRequest {
                name: "BaseRevOne Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content = Bytes::from("New file with base rev 1");
    let sha256 = hex::encode(Sha256::digest(&content));

    let file = service
        .upload_file(
            UploadVaultFileRequest {
                vault_id: vault.id,
                relative_path: "notes/base_rev_one.md".to_string(),
                content_type: Some("text/markdown".to_string()),
                sha256,
                size: content.len() as i64,
                base_server_rev: 1,
                device_id: device.id.to_string(),
                content,
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to upload file with base_server_rev 1");

    assert_eq!(file.relative_path, "notes/base_rev_one.md");
    assert_eq!(file.server_rev, 1);

    cleanup_user(&pool, user.id).await;
}

/// VS-04: Tombstone a file and verify manifest
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_tombstone_and_manifest() {
    let (pool, _event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "tombstone_user", tenant_id).await;

    let service = VaultSyncService::new(metadata_store.clone(), object_store.clone());
    let device = create_test_device(&service, user.id, tenant_id).await;
    let vault = service
        .create_vault(
            CreateVaultRequest {
                name: "Tombstone Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content = Bytes::from("File to be deleted");
    let sha256 = hex::encode(Sha256::digest(&content));

    let file = service
        .upload_file(
            UploadVaultFileRequest {
                vault_id: vault.id,
                relative_path: "notes/delete_me.md".to_string(),
                content_type: Some("text/markdown".to_string()),
                sha256,
                size: content.len() as i64,
                base_server_rev: 0,
                device_id: device.id.to_string(),
                content,
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to upload file");

    let deleted = service
        .delete_file(
            DeleteVaultFileRequest {
                vault_id: vault.id,
                relative_path: "notes/delete_me.md".to_string(),
                base_server_rev: file.server_rev,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to delete file");

    assert!(deleted.deleted);

    let manifest = service
        .get_manifest(vault.id, tenant_id, user.id)
        .await
        .expect("Failed to get manifest")
        .manifest;

    let entry = manifest
        .files
        .iter()
        .find(|e| e.path == "notes/delete_me.md")
        .expect("File should appear in manifest");
    assert!(entry.deleted);

    cleanup_user(&pool, user.id).await;
}

/// VS-05: Rename a file within a vault
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_rename_file() {
    let (pool, _event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "rename_user", tenant_id).await;

    let service = VaultSyncService::new(metadata_store.clone(), object_store.clone());
    let device = create_test_device(&service, user.id, tenant_id).await;
    let vault = service
        .create_vault(
            CreateVaultRequest {
                name: "Rename Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content = Bytes::from("File to be renamed");
    let sha256 = hex::encode(Sha256::digest(&content));

    let file = service
        .upload_file(
            UploadVaultFileRequest {
                vault_id: vault.id,
                relative_path: "notes/old_name.md".to_string(),
                content_type: Some("text/markdown".to_string()),
                sha256,
                size: content.len() as i64,
                base_server_rev: 0,
                device_id: device.id.to_string(),
                content: content.clone(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to upload file");

    service
        .rename_file(
            RenameVaultFileRequest {
                vault_id: vault.id,
                old_path: "notes/old_name.md".to_string(),
                new_path: "notes/new_name.md".to_string(),
                base_server_rev: file.server_rev,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to rename file");

    // Old path should not be downloadable
    let old_result = service
        .download_file(vault.id, "notes/old_name.md", tenant_id, user.id)
        .await;
    assert!(
        old_result.is_err(),
        "Old path should not exist after rename"
    );

    // New path should exist and be downloadable
    let new_result = service
        .download_file(vault.id, "notes/new_name.md", tenant_id, user.id)
        .await;
    assert!(new_result.is_ok(), "New path should exist after rename");

    // Content should be preserved
    let (new_content, _content_type) = service
        .download_file(vault.id, "notes/new_name.md", tenant_id, user.id)
        .await
        .unwrap();
    assert_eq!(new_content, content);

    cleanup_user(&pool, user.id).await;
}

/// VS-06: Manifest includes all files with correct revisions
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_manifest_includes_all_files() {
    let (pool, _event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "manifest_user", tenant_id).await;

    let service = VaultSyncService::new(metadata_store.clone(), object_store.clone());
    let device = create_test_device(&service, user.id, tenant_id).await;
    let vault = service
        .create_vault(
            CreateVaultRequest {
                name: "Manifest Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content1 = Bytes::from("File one");
    let sha1 = hex::encode(Sha256::digest(&content1));
    service
        .upload_file(
            UploadVaultFileRequest {
                vault_id: vault.id,
                relative_path: "notes/one.md".to_string(),
                content_type: Some("text/markdown".to_string()),
                sha256: sha1,
                size: content1.len() as i64,
                base_server_rev: 0,
                device_id: device.id.to_string(),
                content: content1,
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to upload file one");

    let content2 = Bytes::from("File two");
    let sha2 = hex::encode(Sha256::digest(&content2));
    service
        .upload_file(
            UploadVaultFileRequest {
                vault_id: vault.id,
                relative_path: "notes/two.md".to_string(),
                content_type: Some("text/markdown".to_string()),
                sha256: sha2,
                size: content2.len() as i64,
                base_server_rev: 0,
                device_id: device.id.to_string(),
                content: content2,
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to upload file two");

    let manifest = service
        .get_manifest(vault.id, tenant_id, user.id)
        .await
        .expect("Failed to get manifest")
        .manifest;

    assert_eq!(manifest.files.len(), 2);

    let entry_one = manifest
        .files
        .iter()
        .find(|e| e.path == "notes/one.md")
        .expect("one.md should be in manifest");
    assert_eq!(entry_one.server_rev, 1);

    let entry_two = manifest
        .files
        .iter()
        .find(|e| e.path == "notes/two.md")
        .expect("two.md should be in manifest");
    assert_eq!(entry_two.server_rev, 2);

    cleanup_user(&pool, user.id).await;
}

/// VS-07: Device registration, retrieval, and revocation
#[tokio::test]
#[ignore] // Requires database and S3
async fn test_device_registration() {
    let (pool, _event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "device_user", tenant_id).await;

    let service = VaultSyncService::new(metadata_store.clone(), object_store.clone());
    let vault = service
        .create_vault(
            CreateVaultRequest {
                name: "Device Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: "test-device".to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let device = VaultDevice {
        id: Uuid::new_v4(),
        tenant_id,
        user_id: user.id,
        vault_id: Some(vault.id),
        device_name: "Test Device".to_string(),
        client_type: "obsidian".to_string(),
        client_version: Some("1.0.0".to_string()),
        last_sync_rev: None,
        revoked_at: None,
        created_at: Utc::now(),
        last_seen_at: Utc::now(),
    };

    let registered = service
        .register_device(device.clone(), user.id)
        .await
        .expect("Failed to register device");
    assert_eq!(registered.device_name, "Test Device");

    // Verify device can be retrieved from store
    let retrieved = metadata_store
        .get_vault_device(&registered.id.to_string(), tenant_id)
        .await
        .expect("Failed to get device")
        .expect("Device not found");
    assert_eq!(retrieved.id, registered.id);
    assert!(retrieved.revoked_at.is_none());

    // Revoke device
    service
        .revoke_device(registered.id, tenant_id, user.id)
        .await
        .expect("Failed to revoke device");

    // Verify device is revoked
    let after_revoke = metadata_store
        .get_vault_device(&registered.id.to_string(), tenant_id)
        .await
        .expect("Failed to get device after revoke")
        .expect("Device not found after revoke");
    assert!(
        after_revoke.revoked_at.is_some(),
        "Device should be revoked"
    );

    cleanup_user(&pool, user.id).await;
}
