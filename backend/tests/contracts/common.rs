//! Common helpers for contract tests
//!
//! Provides utilities for setting up test tenants, users, files, folders, and shares.

use bytes::Bytes;
use rustshare_core::domain::{File, Folder, Share, SharePermissions, User};
use rustshare_core::services::PermissionResolver;
use rustshare_core::services::{
    FileService, FolderService, JwtOps, ShareNotificationRepo, ShareService,
};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::services::note_service::NoteService;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Mock notification repo for testing
pub struct MockNotificationRepo;

#[async_trait::async_trait]
impl ShareNotificationRepo for MockNotificationRepo {
    async fn was_notified(
        &self,
        _user_id: rustshare_core::domain::UserId,
        _share_id: uuid::Uuid,
    ) -> Result<bool, sqlx::Error> {
        Ok(false)
    }

    async fn record_notification(
        &self,
        _user_id: rustshare_core::domain::UserId,
        _share_id: uuid::Uuid,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }
}

/// Test context holding all necessary services and stores
pub struct TestContext {
    pub pool: PgPool,
    pub event_store: Arc<EventStore>,
    pub metadata_store: Arc<MetadataStore>,
    pub object_store: Arc<ObjectStore>,
    pub broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
}

impl TestContext {
    /// Create a new FileService instance
    pub fn file_service(
        &self,
    ) -> FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository> {
        let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
            PermissionResolverRepository::new(self.pool.clone()),
        )));
        FileService::new(
            self.event_store.clone(),
            self.metadata_store.clone(),
            self.object_store.clone(),
            self.broadcaster.clone(),
            permission_resolver,
        )
    }

    /// Create a new FolderService instance
    pub fn folder_service(
        &self,
    ) -> FolderService<EventStore, MetadataStore, PermissionResolverRepository> {
        let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
            PermissionResolverRepository::new(self.pool.clone()),
        )));
        FolderService::new(
            self.event_store.clone(),
            self.metadata_store.clone(),
            self.broadcaster.clone(),
            permission_resolver,
        )
    }

    /// Create a new NoteService instance
    pub fn note_service(&self) -> NoteService {
        let file_service = Arc::new(self.file_service());
        let folder_service = Arc::new(self.folder_service());
        NoteService::new(
            file_service,
            folder_service,
            self.metadata_store.clone(),
            self.object_store.clone(),
        )
    }
}

/// Setup test environment with database and S3 connections
pub async fn setup_test_env() -> TestContext {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let event_store = Arc::new(EventStore::new(pool.clone()));
    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));
    let broadcaster = Arc::new(rustshare_core::events::EventBroadcaster::new(100));

    let s3_endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .unwrap_or_else(|_| "http://localhost:9000".to_string());
    let s3_region = std::env::var("S3_REGION")
        .or_else(|_| std::env::var("RUSTFS_REGION"))
        .unwrap_or_else(|_| "us-east-1".to_string());
    let s3_bucket = std::env::var("S3_BUCKET")
        .or_else(|_| std::env::var("RUSTFS_BUCKET"))
        .unwrap_or_else(|_| "rustshare".to_string());

    let object_store = Arc::new(
        ObjectStore::new(s3_endpoint, s3_region, s3_bucket)
            .await
            .expect("Failed to create object store"),
    );

    TestContext {
        pool,
        event_store,
        metadata_store,
        object_store,
        broadcaster,
    }
}

/// Create a test tenant and return its ID
pub async fn setup_test_tenant(pool: &PgPool) -> Uuid {
    let tenant_id = Uuid::new_v4();
    let tenant_name = format!("Test Tenant {}", tenant_id);

    sqlx::query(
        r#"
        INSERT INTO tenants (id, name, created_at, updated_at)
        VALUES ($1, $2, NOW(), NOW())
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(tenant_id)
    .bind(tenant_name)
    .execute(pool)
    .await
    .expect("Failed to create test tenant");

    tenant_id
}

/// Create a test user in the specified tenant
pub async fn create_test_user(
    metadata_store: &MetadataStore,
    username: &str,
    tenant_id: Uuid,
) -> User {
    let unique_username = format!("{}_{}", username, Uuid::new_v4());
    let user = User::new(
        unique_username.clone(),
        format!("{} Display", username),
        "test_password_hash".to_string(),
        format!("{}@test.local", unique_username),
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

/// Create a test folder
pub async fn create_test_folder(
    folder_service: &FolderService<EventStore, MetadataStore, PermissionResolverRepository>,
    owner_id: Uuid,
    tenant_id: Uuid,
    name: &str,
    parent_id: Option<Uuid>,
) -> Folder {
    folder_service
        .create_folder(name.to_string(), parent_id, owner_id, tenant_id)
        .await
        .expect("Failed to create test folder")
}

/// Create a test file with content
pub async fn create_test_file(
    file_service: &FileService<
        EventStore,
        MetadataStore,
        ObjectStore,
        PermissionResolverRepository,
    >,
    owner_id: Uuid,
    tenant_id: Uuid,
    folder_id: Option<Uuid>,
    name: &str,
    content: &[u8],
) -> File {
    file_service
        .upload_file(
            owner_id,
            name.to_string(),
            folder_id,
            Bytes::from(content.to_vec()),
            "application/octet-stream".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to create test file")
}

/// Create a test share service
pub fn create_test_share_service<J: JwtOps>(
    ctx: &TestContext,
    jwt_manager: Arc<J>,
) -> ShareService<EventStore, MetadataStore, J, MockNotificationRepo> {
    ShareService::new(
        ctx.event_store.clone(),
        ctx.metadata_store.clone(),
        ctx.broadcaster.clone(),
        jwt_manager,
        Arc::new(MockNotificationRepo),
    )
}

/// Create a test share for a file
pub async fn create_test_share<J: JwtOps>(
    share_service: &ShareService<EventStore, MetadataStore, J, MockNotificationRepo>,
    file_id: Uuid,
    user_id: Uuid,
    permissions: SharePermissions,
    password: Option<String>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    tenant_id: Uuid,
) -> Share {
    share_service
        .create_share(
            file_id,
            user_id,
            permissions,
            password,
            expires_at,
            tenant_id,
        )
        .await
        .expect("Failed to create test share")
}

/// Create a test folder share
pub async fn create_test_folder_share<J: JwtOps>(
    share_service: &ShareService<EventStore, MetadataStore, J, MockNotificationRepo>,
    folder_id: Uuid,
    user_id: Uuid,
    permissions: SharePermissions,
    password: Option<String>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    upload_only: bool,
    tenant_id: Uuid,
) -> Share {
    share_service
        .create_folder_share(
            folder_id,
            user_id,
            permissions,
            password,
            expires_at,
            upload_only,
            tenant_id,
        )
        .await
        .expect("Failed to create test folder share")
}

/// Cleanup a test user and all associated data
pub async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
    // Delete user's files first (cascade will handle versions)
    sqlx::query("DELETE FROM files WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();

    // Delete user's folders
    sqlx::query("DELETE FROM folders WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();

    // Delete the user
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
}

/// Create a test group and return its ID
pub async fn create_test_group(
    pool: &PgPool,
    name: &str,
    tenant_id: Uuid,
    created_by: Uuid,
) -> Uuid {
    let group_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO user_groups (id, name, tenant_id, created_by)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(group_id)
    .bind(name)
    .bind(tenant_id)
    .bind(created_by)
    .execute(pool)
    .await
    .expect("Failed to create test group");

    group_id
}

/// Add a user to a group
pub async fn add_user_to_group(pool: &PgPool, group_id: Uuid, user_id: Uuid) {
    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
        .bind(group_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to add user to group");
}

/// Remove a user from a group
pub async fn remove_user_from_group(pool: &PgPool, group_id: Uuid, user_id: Uuid) {
    sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
        .bind(group_id)
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to remove user from group");
}

/// Cleanup a tenant and all its data
pub async fn cleanup_tenant(pool: &PgPool, tenant_id: Uuid) {
    // Delete all users in the tenant (this will cascade)
    sqlx::query("DELETE FROM users WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .ok();

    // Delete the tenant
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .ok();
}
