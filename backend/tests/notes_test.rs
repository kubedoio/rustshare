//! Contract tests for RustShare Notes MVP-1
//!
//! Run with: cargo test --test notes_test -- --ignored

use bytes::Bytes;
use rustshare_core::domain::User;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::services::module_service::ModuleService;
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
        .save_note(note.id, user.id, "new content".to_string(), None, None)
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
        .save_note(note_a.id, user.id, "updated".to_string(), None, None)
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


#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_create_note_creates_bundle_structure() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_bundle_user_1", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Bundle Structure Test".to_string()),
            None,
            Some("# Hello".to_string()),
        )
        .await
        .unwrap();

    // The returned note name should be note.md
    assert_eq!(note.name, "note.md");
    let bundle_folder_id = note.parent_folder_id.expect("should have parent folder");

    // Verify subfolders exist
    let subfolders = metadata_store
        .list_folders(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    let subfolder_names: Vec<&str> = subfolders.iter().map(|f| f.name.as_str()).collect();
    assert!(subfolder_names.contains(&"attachments"));
    assert!(subfolder_names.contains(&"drawings"));
    assert!(subfolder_names.contains(&"exports"));
    assert!(subfolder_names.contains(&"_rustshare"));

    // Verify note.md exists inside bundle
    let files = metadata_store
        .list_files(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    let file_names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    assert!(file_names.contains(&"note.md"));

    // Verify manifest.json inside _rustshare
    let rustshare_folder = subfolders.iter().find(|f| f.name == "_rustshare").unwrap();
    let manifest_files = metadata_store
        .list_files(Some(rustshare_folder.id), user.id, tenant_id)
        .await
        .unwrap();
    let manifest_names: Vec<&str> = manifest_files.iter().map(|f| f.name.as_str()).collect();
    assert!(manifest_names.contains(&"manifest.json"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_save_note_renames_bundle_folder_on_h1_change() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_bundle_user_2", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Original Title".to_string()),
            None,
            Some("original body".to_string()),
        )
        .await
        .unwrap();

    let bundle_folder_id = note.parent_folder_id.expect("should have parent folder");

    let saved = service
        .save_note(note.id, user.id, "# New Title\n\nbody".to_string(), None, None)
        .await
        .unwrap();

    // File name should still be note.md
    assert_eq!(saved.name, "note.md");

    // Bundle folder should be renamed to the new H1 title
    let bundle_folder = metadata_store
        .find_folder_by_id(bundle_folder_id, user.id)
        .await
        .unwrap()
        .expect("bundle folder should exist");
    assert_eq!(bundle_folder.name, "New Title");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_delete_note_deletes_entire_bundle() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_bundle_user_3", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Delete Bundle Test".to_string()),
            None,
            None,
        )
        .await
        .unwrap();

    let bundle_folder_id = note.parent_folder_id.expect("should have parent folder");

    service.delete_note(note.id, user.id).await.unwrap();

    // Bundle folder should be gone
    let folder = metadata_store
        .find_folder_by_id(bundle_folder_id, user.id)
        .await
        .unwrap();
    assert!(folder.is_none(), "bundle folder should be deleted");

    // No files should remain inside the bundle
    let files = metadata_store
        .list_files(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    assert!(files.is_empty(), "bundle should contain no files");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_list_notes_includes_bundle_counts() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_bundle_user_4", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service = create_file_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Counts Test".to_string()),
            None,
            None,
        )
        .await
        .unwrap();

    let bundle_folder_id = note.parent_folder_id.expect("should have parent folder");

    let subfolders = metadata_store
        .list_folders(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    let attachments_folder = subfolders
        .iter()
        .find(|f| f.name == "attachments")
        .expect("attachments folder should exist");
    let drawings_folder = subfolders
        .iter()
        .find(|f| f.name == "drawings")
        .expect("drawings folder should exist");

    // Upload two attachments
    file_service
        .upload_file(
            user.id,
            "attach1.txt".to_string(),
            Some(attachments_folder.id),
            Bytes::from("a"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .unwrap();
    file_service
        .upload_file(
            user.id,
            "attach2.txt".to_string(),
            Some(attachments_folder.id),
            Bytes::from("b"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    // Upload one drawing
    file_service
        .upload_file(
            user.id,
            "drawing1.svg".to_string(),
            Some(drawings_folder.id),
            Bytes::from("<svg/>"),
            "image/svg+xml".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    let notes = service
        .list_notes(user.id, tenant_id, Some(10))
        .await
        .unwrap();

    let found = notes
        .iter()
        .find(|n| n.id == note.id)
        .expect("note should be in list");
    assert_eq!(found.attachment_count, 2);
    assert_eq!(found.drawing_count, 1);
    assert_eq!(found.export_count, 0);

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_standalone_md_still_works() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_bundle_user_5", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service = create_file_service(event_store, metadata_store.clone(), object_store, &pool);

    // Create a bundle note first to ensure /Workspace/Notes exists
    let setup_note = service
        .create_note(user.id, tenant_id, Some("Setup".to_string()), None, None)
        .await
        .unwrap();

    let bundle_folder = metadata_store
        .find_folder_by_id(setup_note.parent_folder_id.unwrap(), user.id)
        .await
        .unwrap()
        .unwrap();
    let notes_folder = metadata_store
        .find_folder_by_id(bundle_folder.parent_folder_id.unwrap(), user.id)
        .await
        .unwrap()
        .unwrap();

    // Create a standalone markdown file directly in Notes folder
    let standalone = file_service
        .upload_file(
            user.id,
            "standalone.md".to_string(),
            Some(notes_folder.id),
            Bytes::from("standalone content"),
            "text/markdown".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    // list_notes should include it with zero counts
    let notes = service
        .list_notes(user.id, tenant_id, Some(10))
        .await
        .unwrap();
    let found = notes
        .iter()
        .find(|n| n.id == standalone.id)
        .expect("standalone should appear in list");
    assert_eq!(found.name, "standalone.md");
    assert_eq!(found.attachment_count, 0);
    assert_eq!(found.drawing_count, 0);
    assert_eq!(found.export_count, 0);

    // save_note should work (plain content update, no H1 rename)
    let saved = service
        .save_note(standalone.id, user.id, "updated standalone".to_string(), None, None)
        .await
        .unwrap();
    assert_eq!(saved.content, "updated standalone");

    // delete_note should work (file deleted, no folder cascade)
    service.delete_note(standalone.id, user.id).await.unwrap();

    let deleted = metadata_store
        .find_file_by_id(standalone.id, user.id)
        .await
        .unwrap();
    assert!(deleted.is_none(), "standalone file should be deleted");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_recent_activity_shows_bundle_title() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_bundle_user_6", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let folder_service = Arc::new(create_folder_service(
        event_store,
        metadata_store.clone(),
        &pool,
    ));
    let module_service = ModuleService::new(folder_service, metadata_store.clone());

    // Ensure default modules exist for this tenant
    module_service.ensure_default_modules(tenant_id).await.unwrap();

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Activity Test Note".to_string()),
            None,
            Some("activity content".to_string()),
        )
        .await
        .unwrap();

    let summary = module_service
        .get_module_summary("notes", tenant_id, user.id)
        .await
        .unwrap();

    let names: Vec<&str> = summary.recent_items.iter().map(|i| i.name.as_str()).collect();
    assert!(
        names.iter().any(|n| *n == "Activity Test Note"),
        "recent items should show bundle title, got: {:?}",
        names
    );
    assert!(
        !names.iter().any(|n| *n == "note.md"),
        "recent items should not show raw note.md name, got: {:?}",
        names
    );

    cleanup_user(&pool, user.id).await;
}
