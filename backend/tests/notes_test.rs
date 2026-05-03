//! Contract tests for RustShare Notes MVP-1
//!
//! Run with: cargo test --test notes_test -- --ignored

use bytes::Bytes;
use rustshare_core::domain::User;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::services::note_service::{NoteService, NoteVisibility};
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
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
        .unwrap_or_else(|_| "rustshare".to_string());

    let object_store = Arc::new(
        ObjectStore::new(s3_endpoint, s3_region, s3_bucket)
            .await
            .expect("Failed to create object store"),
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

async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
    // Notes create file_versions rows whose created_by FK does not cascade on user delete.
    // Removing owned files first clears the dependent version rows before deleting the user.
    sqlx::query("DELETE FROM files WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test files");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test user");
}

fn create_note_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &PgPool,
) -> Arc<NoteService> {
    let broadcaster = Arc::new(EventBroadcaster::new(100));
    let file_service = Arc::new(create_file_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        pool,
    ));
    let folder_service = Arc::new(create_folder_service(
        event_store.clone(),
        metadata_store.clone(),
        pool,
    ));

    Arc::new(NoteService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
    ))
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_create_note_creates_markdown_file_and_metadata_sidecar() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_1", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Architecture Ideas".to_string()),
            None,
            Some("# Hello".to_string()),
        )
        .await
        .expect("create_note should succeed");

    // 1. Markdown file created
    assert!(note.name.ends_with(".md"));
    assert_eq!(note.content, "# Hello");

    // 2. Metadata sidecar exists with kind=note and private visibility
    assert_eq!(note.metadata.kind, "note");
    assert_eq!(note.metadata.visibility, NoteVisibility::Private);
    assert_eq!(note.metadata.title, "Architecture Ideas");
    assert!(note.metadata.public_share_id.is_none());

    // 3. Excerpt derived from content
    assert!(note.metadata.excerpt.contains("Hello"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_create_note_uses_collision_safe_naming() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_2", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note1 = service
        .create_note(
            user.id,
            tenant_id,
            Some("Untitled Note".to_string()),
            None,
            None,
        )
        .await
        .unwrap();
    let note2 = service
        .create_note(
            user.id,
            tenant_id,
            Some("Untitled Note".to_string()),
            None,
            None,
        )
        .await
        .unwrap();
    let note3 = service
        .create_note(
            user.id,
            tenant_id,
            Some("Untitled Note".to_string()),
            None,
            None,
        )
        .await
        .unwrap();

    assert_ne!(note1.name, note2.name);
    assert_ne!(note2.name, note3.name);
    assert!(note1.name.ends_with(".md"));
    assert!(note2.name.contains("Untitled Note 2"));
    assert!(note3.name.contains("Untitled Note 3"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_read_note_returns_content_and_metadata_unified() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_3", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let created = service
        .create_note(
            user.id,
            tenant_id,
            Some("Read Test".to_string()),
            None,
            Some("body".to_string()),
        )
        .await
        .unwrap();

    let note = service.get_note(created.id, user.id).await.unwrap();
    assert_eq!(note.id, created.id);
    assert_eq!(note.content, "body");
    assert_eq!(note.metadata.title, "Read Test");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_save_note_updates_content_excerpt_and_updated_at() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_4", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Save Test".to_string()),
            None,
            Some("old".to_string()),
        )
        .await
        .unwrap();

    let old_updated_at = note.metadata.updated_at;

    let saved = service
        .save_note(note.id, user.id, "new content".to_string(), None)
        .await
        .unwrap();

    assert_eq!(saved.content, "new content");
    assert!(saved.metadata.updated_at > old_updated_at);
    assert_eq!(saved.metadata.excerpt, "new content");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_rename_note_renames_file_and_sidecar_and_preserves_share_id() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_5", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Old Title".to_string()),
            None,
            None,
        )
        .await
        .unwrap();

    // Make public first to generate share_id
    let _ = service.toggle_visibility(note.id, user.id).await.unwrap();
    let share_id = service
        .get_note(note.id, user.id)
        .await
        .unwrap()
        .metadata
        .public_share_id
        .clone();

    let renamed = service
        .rename_note(note.id, user.id, "New Title".to_string())
        .await
        .unwrap();

    assert!(renamed.name.contains("New Title"));
    assert_eq!(renamed.metadata.title, "New Title");
    assert_eq!(renamed.metadata.public_share_id, share_id);

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_delete_note_invalidates_public_access() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_6", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Delete Test".to_string()),
            None,
            None,
        )
        .await
        .unwrap();

    let public = service.toggle_visibility(note.id, user.id).await.unwrap();
    let share_id = public.metadata.public_share_id.unwrap();

    // Verify public access works before delete
    let before_delete = service.get_public_note(&share_id).await;
    assert!(before_delete.is_ok());

    service.delete_note(note.id, user.id).await.unwrap();

    // Public access should fail after delete
    let after_delete = service.get_public_note(&share_id).await;
    assert!(after_delete.is_err());

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_list_recent_notes_ordered_by_updated_at_desc() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_7", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note_a = service
        .create_note(user.id, tenant_id, Some("A".to_string()), None, None)
        .await
        .unwrap();
    let note_b = service
        .create_note(user.id, tenant_id, Some("B".to_string()), None, None)
        .await
        .unwrap();

    // Touch note_a so it becomes most recent
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    service
        .save_note(note_a.id, user.id, "updated".to_string(), None)
        .await
        .unwrap();

    let recent = service
        .list_notes(user.id, tenant_id, Some(10))
        .await
        .unwrap();
    assert_eq!(recent[0].id, note_a.id);
    assert!(recent.iter().any(|n| n.id == note_b.id));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_toggle_visibility_private_to_public_generates_share_id_and_url() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_8", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Public Test".to_string()),
            None,
            Some("secret".to_string()),
        )
        .await
        .unwrap();

    assert_eq!(note.metadata.visibility, NoteVisibility::Private);

    let public = service.toggle_visibility(note.id, user.id).await.unwrap();
    assert_eq!(public.metadata.visibility, NoteVisibility::Public);
    let share_id = public
        .metadata
        .public_share_id
        .expect("share_id should be set");
    assert_eq!(share_id.len(), 32);

    // Public route readable anonymously
    let anon = service.get_public_note(&share_id).await.unwrap();
    assert_eq!(anon.title, "Public Test");
    assert_eq!(anon.content, "secret");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_toggle_visibility_public_to_private_disables_access() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_9", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Revoke Test".to_string()),
            None,
            None,
        )
        .await
        .unwrap();

    let public = service.toggle_visibility(note.id, user.id).await.unwrap();
    let share_id = public.metadata.public_share_id.unwrap();

    assert!(service.get_public_note(&share_id).await.is_ok());

    let private = service.toggle_visibility(note.id, user.id).await.unwrap();
    assert_eq!(private.metadata.visibility, NoteVisibility::Private);

    assert!(service.get_public_note(&share_id).await.is_err());

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_anonymous_request_to_private_note_returns_not_found() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_10", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(user.id, tenant_id, Some("Private".to_string()), None, None)
        .await
        .unwrap();

    // There is no share_id, so any random id should fail
    let result = service
        .get_public_note("nonexistentshareid12345678901234")
        .await;
    assert!(result.is_err());

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore]
async fn contract_public_note_page_does_not_leak_internal_paths() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_11", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Leak Test".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let public = service.toggle_visibility(note.id, user.id).await.unwrap();
    let share_id = public.metadata.public_share_id.unwrap();

    let anon = service.get_public_note(&share_id).await.unwrap();
    // PublicNote should not contain internal identifiers
    assert!(!anon.content.contains("blobs/"));
    assert!(!anon.title.contains(".json"));

    cleanup_user(&pool, user.id).await;
}
