//! Contract tests for Legacy Module Root Policy (Step 5)
//!
//! Run with: cargo test --test module_service_test -- --ignored

use rustshare_core::domain::User;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::services::module_service::ModuleService;
use rustshare_storage::{EventStore, MetadataStore};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

async fn setup_test_env() -> (PgPool, Arc<EventStore>, Arc<MetadataStore>) {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let event_store = Arc::new(EventStore::new(pool.clone()));
    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));

    (pool, event_store, metadata_store)
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
        true, // admin so ensure_default_modules can find an admin
        10_737_418_240,
        tenant_id,
    );

    metadata_store
        .create_user(&user)
        .await
        .expect("Failed to create test user");

    user
}

async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
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

async fn cleanup_modules_and_folders(pool: &PgPool, tenant_id: Uuid, user_id: Uuid) {
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

    sqlx::query("DELETE FROM modules WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .ok();
}

#[tokio::test]
#[ignore] // Requires database
async fn contract_ensure_default_modules_creates_canonical_roots() {
    let (pool, event_store, metadata_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "module_root_user_1", tenant_id).await;

    let folder_service = Arc::new(create_folder_service(
        event_store,
        metadata_store.clone(),
        &pool,
    ));
    let module_service = ModuleService::new(folder_service, metadata_store.clone());

    module_service
        .ensure_default_modules(tenant_id)
        .await
        .expect("ensure_default_modules should succeed");

    let modules = module_service
        .list_modules(tenant_id)
        .await
        .expect("list_modules should succeed");

    let expected = [
        ("notes", "/Workspace/Notes"),
        ("meetings", "/Workspace/Meetings"),
        ("standups", "/Workspace/Standups"),
        ("kanban", "/Workspace/Kanban"),
        ("decisions", "/Workspace/Decisions"),
        ("brainstorming", "/Workspace/Brainstorming"),
        ("shares", "/Workspace/Shares"),
    ];

    for (key, expected_root) in expected {
        let module = modules
            .iter()
            .find(|m| m.module_key == key)
            .unwrap_or_else(|| panic!("module {} should exist", key));
        assert_eq!(
            module.root_path, expected_root,
            "module {} must use canonical workspace root",
            key
        );
    }

    cleanup_modules_and_folders(&pool, tenant_id, user.id).await;
    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database
async fn contract_ensure_default_modules_is_idempotent_no_duplicate_roots() {
    let (pool, event_store, metadata_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "module_root_user_2", tenant_id).await;

    let folder_service = Arc::new(create_folder_service(
        event_store,
        metadata_store.clone(),
        &pool,
    ));
    let module_service = ModuleService::new(folder_service, metadata_store.clone());

    // First call
    module_service
        .ensure_default_modules(tenant_id)
        .await
        .expect("first ensure_default_modules should succeed");

    // Count folders under Workspace after first call
    let workspace_folder = sqlx::query!(
        "SELECT id FROM folders WHERE name = 'Workspace' AND tenant_id = $1 AND owner_id = $2",
        tenant_id,
        user.id
    )
    .fetch_one(pool.as_ref())
    .await
    .expect("Workspace folder should exist");

    let first_count = sqlx::query!(
        "SELECT COUNT(*) as count FROM folders WHERE parent_folder_id = $1",
        workspace_folder.id
    )
    .fetch_one(pool.as_ref())
    .await
    .unwrap()
    .count
    .unwrap_or(0);

    // Second call
    module_service
        .ensure_default_modules(tenant_id)
        .await
        .expect("second ensure_default_modules should succeed");

    let second_count = sqlx::query!(
        "SELECT COUNT(*) as count FROM folders WHERE parent_folder_id = $1",
        workspace_folder.id
    )
    .fetch_one(pool.as_ref())
    .await
    .unwrap()
    .count
    .unwrap_or(0);

    assert_eq!(
        first_count, second_count,
        "ensure_default_modules must not create duplicate module roots"
    );

    cleanup_modules_and_folders(&pool, tenant_id, user.id).await;
    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database
async fn contract_enable_module_ensures_canonical_root_without_duplicates() {
    let (pool, event_store, metadata_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "module_root_user_3", tenant_id).await;

    let folder_service = Arc::new(create_folder_service(
        event_store,
        metadata_store.clone(),
        &pool,
    ));
    let module_service = ModuleService::new(folder_service, metadata_store.clone());

    module_service
        .ensure_default_modules(tenant_id)
        .await
        .unwrap();

    // Enable a module that starts disabled (e.g., kanban)
    module_service
        .enable_module("kanban", user.id, tenant_id)
        .await
        .expect("enable_module should succeed");

    // Enable again should be idempotent
    module_service
        .enable_module("kanban", user.id, tenant_id)
        .await
        .expect("re-enable_module should succeed");

    // Verify only one Kanban folder exists under Workspace
    let workspace_folder = sqlx::query!(
        "SELECT id FROM folders WHERE name = 'Workspace' AND tenant_id = $1 AND owner_id = $2",
        tenant_id,
        user.id
    )
    .fetch_one(pool.as_ref())
    .await
    .expect("Workspace folder should exist");

    let kanban_folders = sqlx::query!(
        "SELECT id FROM folders WHERE parent_folder_id = $1 AND name = 'Kanban'",
        workspace_folder.id
    )
    .fetch_all(pool.as_ref())
    .await
    .unwrap();

    assert_eq!(
        kanban_folders.len(),
        1,
        "only one canonical Kanban root should exist, found {}",
        kanban_folders.len()
    );

    cleanup_modules_and_folders(&pool, tenant_id, user.id).await;
    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database
async fn contract_module_summary_reads_from_canonical_root() {
    let (pool, event_store, metadata_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "module_root_user_4", tenant_id).await;

    let folder_service = Arc::new(create_folder_service(
        event_store,
        metadata_store.clone(),
        &pool,
    ));
    let module_service = ModuleService::new(folder_service, metadata_store.clone());

    module_service
        .ensure_default_modules(tenant_id)
        .await
        .unwrap();

    // Enable notes so we can get a summary
    module_service
        .enable_module("notes", user.id, tenant_id)
        .await
        .unwrap();

    let summary = module_service
        .get_module_summary("notes", tenant_id, user.id)
        .await
        .expect("get_module_summary should succeed");

    assert_eq!(summary.module_key, "notes");
    // With no notes created yet, total_items should be 0
    assert_eq!(summary.total_items, 0);

    cleanup_modules_and_folders(&pool, tenant_id, user.id).await;
    cleanup_user(&pool, user.id).await;
}
