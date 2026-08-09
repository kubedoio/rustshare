//! Integration tests: shared-admin subtree deletion (S2).
//!
//! A shared Admin recipient must be able to delete a shared folder including
//! its whole (possibly mixed-ownership) subtree; recipients without Admin must
//! be denied. Regression: delete_folder used owner-filtered descendant and
//! delete queries, so a shared Admin recipient silently no-op'd on
//! mixed-ownership trees.
//!
//! Run with: cargo test --test shared_admin_delete_test -- --ignored
//! (requires DATABASE_URL and S3-compatible object storage, as CI provides).

use bytes::Bytes;
use rustshare_core::domain::{Share, SharePermissions, User};
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderError, FolderService, PermissionResolver};
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

async fn cleanup(
    pool: &PgPool,
    owner_id: Uuid,
    admin_recipient_id: Uuid,
    view_recipient_id: Uuid,
    tenant_id: Uuid,
) {
    sqlx::query("DELETE FROM shares WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM files WHERE owner_id = $1 OR owner_id = $2 OR owner_id = $3")
        .bind(owner_id)
        .bind(admin_recipient_id)
        .bind(view_recipient_id)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM folders WHERE owner_id = $1 OR owner_id = $2 OR owner_id = $3")
        .bind(owner_id)
        .bind(admin_recipient_id)
        .bind(view_recipient_id)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2 OR id = $3")
        .bind(owner_id)
        .bind(admin_recipient_id)
        .bind(view_recipient_id)
        .execute(pool)
        .await
        .ok();
}

/// A shared Admin recipient can delete a mixed-ownership subtree; a non-Admin
/// recipient is denied.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn shared_admin_recipient_can_delete_mixed_ownership_subtree() {
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
    let owner = create_test_user(&metadata_store, &format!("d_owner_{test_id}"), tenant_id).await;
    let admin = create_test_user(&metadata_store, &format!("d_admin_{test_id}"), tenant_id).await;
    let viewer = create_test_user(&metadata_store, &format!("d_viewer_{test_id}"), tenant_id).await;

    // Mixed-ownership tree under /shared:
    //   /shared (owner)
    //     /shared/sub (owner)
    //     /shared/rec (admin recipient — different owner)
    //     /shared/doc.txt (owner)
    let shared = folder_service
        .create_folder("shared".into(), None, owner.id, tenant_id)
        .await
        .expect("owner creates shared folder");
    let sub = folder_service
        .create_folder("sub".into(), Some(shared.id), owner.id, tenant_id)
        .await
        .expect("owner creates sub folder");
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

    share_folder_to_user(
        &metadata_store,
        shared.id,
        owner.id,
        admin.id,
        SharePermissions::Admin,
        tenant_id,
    )
    .await;
    share_folder_to_user(
        &metadata_store,
        shared.id,
        owner.id,
        viewer.id,
        SharePermissions::View,
        tenant_id,
    )
    .await;

    // The Admin recipient creates their own folder inside the shared tree, so
    // the subtree has mixed ownership.
    let rec = folder_service
        .create_folder("rec".into(), Some(shared.id), admin.id, tenant_id)
        .await
        .expect("admin recipient creates folder under shared");

    // Regression: without the unchecked subtree walk/delete, this either
    // no-op'd silently or only deleted the caller's own folders.
    folder_service
        .delete_folder(shared.id, admin.id)
        .await
        .expect("shared Admin recipient can delete the whole shared subtree");

    // The entire mixed-ownership subtree is soft-deleted.
    for (label, id) in [("shared", shared.id), ("sub", sub.id), ("rec", rec.id)] {
        let deleted: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT deleted_at FROM folders WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("folder row exists");
        assert!(
            deleted.is_some(),
            "folder {label} must be soft-deleted after shared-admin delete"
        );
    }
    let file_deleted: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM files WHERE id = $1")
            .bind(file.id)
            .fetch_one(&pool)
            .await
            .expect("file row exists");
    assert!(
        file_deleted.is_some(),
        "file must be soft-deleted after shared-admin delete"
    );

    cleanup(&pool, owner.id, admin.id, viewer.id, tenant_id).await;
}

/// A non-Admin shared recipient is denied deletion of the shared folder.
#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn non_admin_recipient_cannot_delete_shared_folder() {
    let (pool, event_store, metadata_store, _object_store) = setup_test_env().await;
    let folder_service = create_folder_service(event_store, metadata_store.clone(), &pool);

    let tenant_id = Uuid::new_v4();
    let test_id = Uuid::new_v4().simple();
    let owner = create_test_user(&metadata_store, &format!("n_owner_{test_id}"), tenant_id).await;
    let viewer = create_test_user(&metadata_store, &format!("n_viewer_{test_id}"), tenant_id).await;

    let shared = folder_service
        .create_folder("shared".into(), None, owner.id, tenant_id)
        .await
        .expect("owner creates shared folder");

    share_folder_to_user(
        &metadata_store,
        shared.id,
        owner.id,
        viewer.id,
        SharePermissions::View,
        tenant_id,
    )
    .await;

    let denied = folder_service.delete_folder(shared.id, viewer.id).await;
    assert!(
        matches!(denied, Err(FolderError::PermissionDenied { .. })),
        "non-Admin recipient must be denied deletion, got: {denied:?}"
    );

    cleanup(&pool, owner.id, viewer.id, Uuid::new_v4(), tenant_id).await;
}
