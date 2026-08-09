//! Integration tests: shared-folder recipient operations.
//!
//! Regression coverage for shared-folder recipient support:
//!
//! 1. A shared Edit recipient can create folders under, rename folders inside,
//!    and move folders within a shared folder — previously the owner-filtered
//!    lookups returned NotFound / denied the operation.
//! 2. Moving requires Edit on the target parent; targets the recipient cannot
//!    write to are rejected (matches create_folder / upload_file semantics).
//! 3. A shared Edit recipient can move files into shared folders and can list
//!    and restore file versions (previously versions were owner-filtered, so
//!    recipients saw an empty history and could not restore).
//!
//! Run with: cargo test --test shared_recipient_operations_test -- --ignored
//! (requires DATABASE_URL and S3-compatible object storage, as CI provides).

use bytes::Bytes;
use rustshare_core::domain::{Share, SharePermissions, User};
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{
    FileError, FileService, FolderError, FolderService, PermissionResolver,
};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore, ObjectStoreOptions};
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
        .expect("Failed to connect to database");

    let event_store = Arc::new(EventStore::new(pool.clone()));
    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));

    let s3_endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .unwrap_or_else(|_| "http://localhost:9000".to_string());
    let s3_region = std::env::var("S3_REGION")
        .or_else(|_| std::env::var("RUSTFS_REGION"))
        .unwrap_or_else(|_| "us-east-1".to_string());
    let s3_bucket = std::env::var("S3_BUCKET")
        .or_else(|_| std::env::var("RUSTFS_BUCKET"))
        .unwrap_or_else(|_| "rustshare-files".to_string());

    let object_store = Arc::new(
        ObjectStore::new_with_options(
            s3_endpoint,
            s3_region,
            s3_bucket,
            ObjectStoreOptions {
                auto_create_bucket: true,
            },
        )
        .await
        .expect("Failed to create object store")
        .with_blob_lock_pool(pool.clone()),
    );

    (pool, event_store, metadata_store, object_store)
}

fn create_file_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &PgPool,
) -> FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository> {
    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));
    FileService::new(
        event_store,
        metadata_store,
        object_store,
        broadcaster,
        permission_resolver,
    )
}

fn create_folder_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    pool: &PgPool,
) -> FolderService<EventStore, MetadataStore, PermissionResolverRepository> {
    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));
    FolderService::new(
        event_store,
        metadata_store,
        broadcaster,
        permission_resolver,
    )
}

async fn create_test_user(metadata_store: &MetadataStore, username: &str, tenant_id: Uuid) -> User {
    let user = User::new(
        username.to_string(),
        format!("{} Display", username),
        "test_password_hash".to_string(),
        format!("{}@test.local", username),
        false,
        10_737_418_240,
        tenant_id,
    );

    metadata_store
        .create_user(&user)
        .await
        .expect("Failed to create test user");

    user
}

/// Create a user-to-user share of a folder (recipient gets `permissions`),
/// mirroring what the share repository's `create_user_share` persists.
async fn share_folder_to_user(
    metadata_store: &MetadataStore,
    folder_id: Uuid,
    owner_id: Uuid,
    recipient_id: Uuid,
    permissions: SharePermissions,
    tenant_id: Uuid,
) {
    let share = Share {
        id: Uuid::new_v4(),
        file_id: None,
        folder_id: Some(folder_id),
        share_token: None,
        permissions,
        password_hash: None,
        expires_at: None,
        upload_only: false,
        access_count: 0,
        recipient_user_id: Some(recipient_id),
        recipient_group_id: None,
        created_by: owner_id,
        created_at: chrono::Utc::now(),
        revoked_at: None,
        tenant_id,
    };
    metadata_store
        .create_share(&share)
        .await
        .expect("Failed to create user share");
}

async fn cleanup(pool: &PgPool, owner_id: Uuid, recipient_id: Uuid, tenant_id: Uuid) {
    sqlx::query("DELETE FROM shares WHERE tenant_id = $1 AND (created_by = $2 OR created_by = $3)")
        .bind(tenant_id)
        .bind(owner_id)
        .bind(recipient_id)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM files WHERE owner_id = $1 OR owner_id = $2")
        .bind(owner_id)
        .bind(recipient_id)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM folders WHERE owner_id = $1 OR owner_id = $2")
        .bind(owner_id)
        .bind(recipient_id)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
        .bind(owner_id)
        .bind(recipient_id)
        .execute(pool)
        .await
        .ok();
}

/// A shared Edit recipient can create folders under, rename folders inside, and
/// move folders within a shared folder; moving into a folder they cannot write
/// to is rejected.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn shared_edit_recipient_can_restructure_shared_folder() {
    let (pool, event_store, metadata_store, _object_store) = setup_test_env().await;
    let folder_service = create_folder_service(event_store.clone(), metadata_store.clone(), &pool);

    let tenant_id = Uuid::new_v4();
    let test_id = Uuid::new_v4().simple();
    let owner = create_test_user(&metadata_store, &format!("s1_owner_{test_id}"), tenant_id).await;
    let recipient = create_test_user(
        &metadata_store,
        &format!("s1_recipient_{test_id}"),
        tenant_id,
    )
    .await;

    // Owner's tree: /shared (with /shared/sub) and /private.
    let shared = folder_service
        .create_folder("shared".into(), None, owner.id, tenant_id)
        .await
        .expect("owner creates shared folder");
    let sub = folder_service
        .create_folder("sub".into(), Some(shared.id), owner.id, tenant_id)
        .await
        .expect("owner creates sub folder");
    let private = folder_service
        .create_folder("private".into(), None, owner.id, tenant_id)
        .await
        .expect("owner creates private folder");

    share_folder_to_user(
        &metadata_store,
        shared.id,
        owner.id,
        recipient.id,
        SharePermissions::Edit,
        tenant_id,
    )
    .await;

    // Recipient creates a folder under the shared folder. Regression: the
    // owner-filtered parent lookup used to return NotFound for the recipient.
    let rec1 = folder_service
        .create_folder("rec1".into(), Some(shared.id), recipient.id, tenant_id)
        .await
        .expect("shared Edit recipient can create a folder under the shared folder");

    // Recipient renames the owner's folder inside the shared folder.
    // Regression: the parent lookup for the new path used to fail.
    let renamed = folder_service
        .rename_folder(sub.id, "sub-renamed".into(), recipient.id)
        .await
        .expect("shared Edit recipient can rename a folder inside the shared folder");
    assert_eq!(renamed.name, "sub-renamed");

    // Recipient moves the renamed folder into the shared folder (Edit on the
    // target parent via the share).
    folder_service
        .move_folder(renamed.id, Some(shared.id), recipient.id)
        .await
        .expect("shared Edit recipient can move a folder into the shared folder");

    // Recipient moves a folder into the shared folder (Edit on target via share).
    folder_service
        .move_folder(rec1.id, Some(shared.id), recipient.id)
        .await
        .expect("shared Edit recipient can move their folder into the shared folder");

    // Moving into /private requires Edit on the target parent — the recipient
    // has no access there, so this must be denied.
    let denied = folder_service
        .move_folder(renamed.id, Some(private.id), recipient.id)
        .await;
    assert!(
        matches!(denied, Err(FolderError::PermissionDenied { .. })),
        "moving into a folder without Edit must be denied, got: {denied:?}"
    );

    // Creating under /private must also be denied.
    let denied_create = folder_service
        .create_folder("x".into(), Some(private.id), recipient.id, tenant_id)
        .await;
    assert!(
        matches!(denied_create, Err(FolderError::PermissionDenied { .. })),
        "creating under a folder without Edit must be denied, got: {denied_create:?}"
    );

    cleanup(&pool, owner.id, recipient.id, tenant_id).await;
}

/// A shared Edit recipient can move files into shared folders; targets without
/// Edit access are rejected.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn shared_edit_recipient_can_move_files_into_shared_folder() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let folder_service = create_folder_service(event_store, metadata_store.clone(), &pool);

    let tenant_id = Uuid::new_v4();
    let test_id = Uuid::new_v4().simple();
    let owner = create_test_user(&metadata_store, &format!("s2_owner_{test_id}"), tenant_id).await;
    let recipient = create_test_user(
        &metadata_store,
        &format!("s2_recipient_{test_id}"),
        tenant_id,
    )
    .await;

    let shared = folder_service
        .create_folder("shared".into(), None, owner.id, tenant_id)
        .await
        .expect("owner creates shared folder");
    let private = folder_service
        .create_folder("private".into(), None, owner.id, tenant_id)
        .await
        .expect("owner creates private folder");

    share_folder_to_user(
        &metadata_store,
        shared.id,
        owner.id,
        recipient.id,
        SharePermissions::Edit,
        tenant_id,
    )
    .await;

    let file = file_service
        .upload_file(
            owner.id,
            "doc.txt".into(),
            Some(shared.id),
            Bytes::from("content"),
            "text/plain".into(),
            tenant_id,
        )
        .await
        .expect("owner uploads file into shared folder");

    // Recipient's own folder under the shared folder — a legal move target.
    let rec1 = folder_service
        .create_folder("rec1".into(), Some(shared.id), recipient.id, tenant_id)
        .await
        .expect("recipient creates folder under shared");

    // Regression: the owner-filtered target lookup used to deny this.
    let moved = file_service
        .move_file(file.id, Some(rec1.id), recipient.id)
        .await
        .expect("shared Edit recipient can move a file into a folder under the shared folder");
    assert_eq!(moved.parent_folder_id, Some(rec1.id));

    // Moving into /private requires Edit on the target parent — denied.
    let denied = file_service
        .move_file(file.id, Some(private.id), recipient.id)
        .await;
    assert!(
        matches!(denied, Err(FileError::PermissionDenied { .. })),
        "moving a file into a folder without Edit must be denied, got: {denied:?}"
    );

    cleanup(&pool, owner.id, recipient.id, tenant_id).await;
}

/// A shared recipient can list and restore file versions (previously the
/// version queries were owner-filtered, so recipients saw nothing).
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn shared_edit_recipient_can_list_and_restore_versions() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let file_service = create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let folder_service = create_folder_service(event_store, metadata_store.clone(), &pool);

    let tenant_id = Uuid::new_v4();
    let test_id = Uuid::new_v4().simple();
    let owner = create_test_user(&metadata_store, &format!("s3_owner_{test_id}"), tenant_id).await;
    let recipient = create_test_user(
        &metadata_store,
        &format!("s3_recipient_{test_id}"),
        tenant_id,
    )
    .await;

    let shared = folder_service
        .create_folder("shared".into(), None, owner.id, tenant_id)
        .await
        .expect("owner creates shared folder");

    share_folder_to_user(
        &metadata_store,
        shared.id,
        owner.id,
        recipient.id,
        SharePermissions::Edit,
        tenant_id,
    )
    .await;

    let file = file_service
        .upload_file(
            owner.id,
            "v.txt".into(),
            Some(shared.id),
            Bytes::from("v1"),
            "text/plain".into(),
            tenant_id,
        )
        .await
        .expect("owner uploads v1");
    file_service
        .update_file(file.id, owner.id, 1, Bytes::from("v2"))
        .await
        .expect("owner updates to v2");

    // Regression: list_file_versions was owner-filtered, so the recipient saw
    // an empty history.
    let versions = file_service
        .list_versions(file.id, recipient.id)
        .await
        .expect("shared recipient can list versions");
    assert_eq!(versions.len(), 2, "recipient must see both versions");

    // Regression: restore looked up the version by the caller id, so the
    // recipient got VersionNotFound.
    let restored = file_service
        .restore_version(file.id, 1, recipient.id)
        .await
        .expect("shared Edit recipient can restore a version");
    assert_eq!(restored.current_version, 3, "restore creates a new version");

    let versions_after = file_service
        .list_versions(file.id, recipient.id)
        .await
        .expect("shared recipient can list versions after restore");
    assert_eq!(versions_after.len(), 3);

    cleanup(&pool, owner.id, recipient.id, tenant_id).await;
}
