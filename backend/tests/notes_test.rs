//! Contract tests for RustShare Notes MVP-1
//!
//! Run with: cargo test --test notes_test -- --ignored

use bytes::Bytes;
use rustshare_core::domain::User;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::services::module_service::ModuleService;
use rustshare_server::services::note_service::{
    NoteConflictResolution, NoteError, NoteService, NoteVisibility,
};
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
        ObjectStore::new_with_options(
            s3_endpoint,
            s3_region,
            s3_bucket,
            rustshare_storage::ObjectStoreOptions {
                auto_create_bucket: true,
            },
        )
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
    let _broadcaster = Arc::new(EventBroadcaster::new(100));
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

    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));

    Arc::new(NoteService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
        permission_resolver,
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
    assert!(note.content.starts_with("---\n"));
    assert!(note.content.contains("type: Note"));
    assert!(note.content.contains("rustshare:\n"));
    assert!(note.content.contains("id: "));
    assert!(note.content.contains("# Architecture Ideas"));

    // 2. Metadata sidecar exists with kind=note and private visibility
    assert_eq!(note.metadata.kind, "note");
    assert_eq!(note.metadata.visibility, NoteVisibility::Private);
    assert_eq!(note.metadata.title, "Architecture Ideas");
    assert!(note.metadata.public_share_id.is_none());

    // 3. OKF identity
    let okf_id = note.metadata.okf_id.expect("okf_id should be set");
    assert!(!okf_id.is_nil());
    assert_eq!(note.okf_id, Some(okf_id));
    assert!(note.metadata.acl_hash.is_some());
    assert_eq!(note.metadata.acl_version, Some(1));

    // 4. Excerpt derived from body
    assert!(note.metadata.excerpt.contains("Architecture Ideas"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
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

    // For folder-backed notes, collision-safe naming applies to bundle folders,
    // not the file (which is always note.md). Verify via parent folder names.
    let folder1 = metadata_store
        .find_folder_by_id(note1.parent_folder_id.unwrap(), user.id)
        .await
        .unwrap()
        .unwrap();
    let folder2 = metadata_store
        .find_folder_by_id(note2.parent_folder_id.unwrap(), user.id)
        .await
        .unwrap()
        .unwrap();
    let folder3 = metadata_store
        .find_folder_by_id(note3.parent_folder_id.unwrap(), user.id)
        .await
        .unwrap()
        .unwrap();

    assert_ne!(folder1.name, folder2.name);
    assert_ne!(folder2.name, folder3.name);
    assert!(note1.name.ends_with(".md"));
    assert!(folder2.name.contains("Untitled Note 2"));
    assert!(folder3.name.contains("Untitled Note 3"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
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

    let note = service
        .get_note(created.id, user.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(note.id, created.id);
    assert!(note.content.starts_with("---\n"));
    assert!(note.content.contains("body"));
    assert!(note.content.contains("title: Read Test"));
    assert_eq!(note.metadata.title, "Read Test");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
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
        .save_note(
            note.id,
            user.id,
            tenant_id,
            "new content".to_string(),
            None,
            None,
        )
        .await
        .unwrap();

    assert!(saved.content.starts_with("---\n"));
    assert!(saved.content.contains("new content"));
    assert!(saved.metadata.updated_at > old_updated_at);
    assert_eq!(saved.metadata.excerpt, "new content");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
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
    let _ = service
        .toggle_visibility(note.id, user.id, tenant_id)
        .await
        .unwrap();
    let share_id = service
        .get_note(note.id, user.id, tenant_id)
        .await
        .unwrap()
        .metadata
        .public_share_id
        .clone();

    let renamed = service
        .rename_note(note.id, user.id, tenant_id, "New Title".to_string())
        .await
        .unwrap();

    // For folder-backed notes, the file name stays note.md; the parent folder is renamed
    let renamed_folder = metadata_store
        .find_folder_by_id(renamed.parent_folder_id.unwrap(), user.id)
        .await
        .unwrap()
        .unwrap();
    assert!(renamed_folder.name.contains("New Title"));
    assert_eq!(renamed.metadata.title, "New Title");
    assert_eq!(renamed.metadata.public_share_id, share_id);

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
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

    let public = service
        .toggle_visibility(note.id, user.id, tenant_id)
        .await
        .unwrap();
    let share_id = public.metadata.public_share_id.unwrap();

    // Verify public access works before delete
    let before_delete = service.get_public_note(&share_id).await;
    assert!(before_delete.is_ok());

    service
        .delete_note(note.id, user.id, tenant_id)
        .await
        .unwrap();

    // Public access should fail after delete
    let after_delete = service.get_public_note(&share_id).await;
    assert!(after_delete.is_err());

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
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
        .save_note(
            note_a.id,
            user.id,
            tenant_id,
            "updated".to_string(),
            None,
            None,
        )
        .await
        .unwrap();

    let recent = service.list_notes(user.id, tenant_id, 10, 0).await.unwrap();
    assert_eq!(recent[0].id, note_a.id);
    assert!(recent.iter().any(|n| n.id == note_b.id));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
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

    let public = service
        .toggle_visibility(note.id, user.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(public.metadata.visibility, NoteVisibility::Public);
    let share_id = public
        .metadata
        .public_share_id
        .expect("share_id should be set");
    assert_eq!(share_id.len(), 32);

    // Public route readable anonymously
    let anon = service.get_public_note(&share_id).await.unwrap();
    assert_eq!(anon.title, "Public Test");
    assert!(anon.content.contains("secret"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
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

    let public = service
        .toggle_visibility(note.id, user.id, tenant_id)
        .await
        .unwrap();
    let share_id = public.metadata.public_share_id.unwrap();

    assert!(service.get_public_note(&share_id).await.is_ok());

    let private = service
        .toggle_visibility(note.id, user.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(private.metadata.visibility, NoteVisibility::Private);

    assert!(service.get_public_note(&share_id).await.is_err());

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_anonymous_request_to_private_note_returns_not_found() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_contract_user_10", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let _note = service
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
#[ignore = "Requires database and S3"]
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

    let public = service
        .toggle_visibility(note.id, user.id, tenant_id)
        .await
        .unwrap();
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
async fn contract_save_note_does_not_rename_bundle_folder_on_h1_change() {
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
    let original_folder_name = metadata_store
        .find_folder_by_id(bundle_folder_id, user.id)
        .await
        .unwrap()
        .expect("bundle folder should exist")
        .name;

    let saved = service
        .save_note(
            note.id,
            user.id,
            tenant_id,
            "# New Title\n\nbody".to_string(),
            None,
            None,
        )
        .await
        .unwrap();

    // File name should still be note.md
    assert_eq!(saved.name, "note.md");

    // ADR-0029: changing H1 must NOT rename the bundle folder.
    let bundle_folder = metadata_store
        .find_folder_by_id(bundle_folder_id, user.id)
        .await
        .unwrap()
        .expect("bundle folder should exist");
    assert_eq!(bundle_folder.name, original_folder_name);

    // Frontmatter title and bundle_name should still reflect the original title.
    assert!(saved.content.contains("title: Original Title"));
    assert!(saved.content.contains("bundle_name: Original Title"));

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

    service
        .delete_note(note.id, user.id, tenant_id)
        .await
        .unwrap();

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
    let file_service =
        create_file_service(event_store, metadata_store.clone(), object_store, &pool);

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

    let notes = service.list_notes(user.id, tenant_id, 10, 0).await.unwrap();

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
async fn contract_new_note_writes_to_canonical_workspace_path() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_canonical_user", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Canonical Path Test".to_string()),
            None,
            None,
        )
        .await
        .unwrap();

    let bundle_folder = metadata_store
        .find_folder_by_id(note.parent_folder_id.unwrap(), user.id)
        .await
        .unwrap()
        .unwrap();
    let notes_folder = metadata_store
        .find_folder_by_id(bundle_folder.parent_folder_id.unwrap(), user.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        notes_folder.path, "/Workspace/Notes",
        "new notes must be written to canonical /Workspace/Notes path"
    );
    assert!(
        note.path.starts_with("/Workspace/Notes/"),
        "note path must be under canonical workspace root, got: {}",
        note.path
    );

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
    let file_service =
        create_file_service(event_store, metadata_store.clone(), object_store, &pool);

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
    let notes = service.list_notes(user.id, tenant_id, 10, 0).await.unwrap();
    let found = notes
        .iter()
        .find(|n| n.id == standalone.id)
        .expect("standalone should appear in list");
    assert_eq!(found.name, "standalone.md");
    assert_eq!(found.attachment_count, 0);
    assert_eq!(found.drawing_count, 0);
    assert_eq!(found.export_count, 0);

    // save_note should work (plain content update, no H1 rename).
    // Legacy notes without frontmatter get OKF frontmatter merged in.
    let saved = service
        .save_note(
            standalone.id,
            user.id,
            tenant_id,
            "updated standalone".to_string(),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(saved.content.starts_with("---\n"));
    assert!(saved.content.contains("updated standalone"));
    assert!(saved.content.contains("type: Note"));

    // delete_note should work (file deleted, no folder cascade)
    service
        .delete_note(standalone.id, user.id, tenant_id)
        .await
        .unwrap();

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
    module_service
        .ensure_default_modules(tenant_id)
        .await
        .unwrap();

    let _note = service
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

    let names: Vec<&str> = summary
        .recent_items
        .iter()
        .map(|i| i.name.as_str())
        .collect();
    assert!(
        names.contains(&"Activity Test Note"),
        "recent items should show bundle title, got: {:?}",
        names
    );
    assert!(
        !names.contains(&"note.md"),
        "recent items should not show raw note.md name, got: {:?}",
        names
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_custom_workspace_and_folder_paths() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "custom_paths_user", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    // Customize the workspace and folder names
    let customized_service = (*service)
        .clone()
        .with_custom_paths("CustomWorkspace".to_string(), "CustomNotes".to_string());

    let note = customized_service
        .create_note(
            user.id,
            tenant_id,
            Some("Custom Note".to_string()),
            None,
            Some("custom note content".to_string()),
        )
        .await
        .unwrap();

    assert_eq!(customized_service.workspace_name, "CustomWorkspace");
    assert_eq!(customized_service.folder_name, "CustomNotes");
    assert!(note.path.starts_with("/CustomWorkspace/CustomNotes/"));

    let listed = customized_service
        .list_notes(user.id, tenant_id, 1000, 0)
        .await
        .unwrap();

    assert!(
        listed.iter().any(|n| n.id == note.id),
        "should find custom note in custom listed notes"
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_rename_note_preserves_okf_id_and_updates_frontmatter() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_rename_okf_user", tenant_id).await;
    let service = create_note_service(
        event_store,
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Original Title".to_string()),
            None,
            None,
        )
        .await
        .unwrap();

    let original_okf_id = note.okf_id.expect("okf_id should be set");
    let bundle_folder_id = note.parent_folder_id.expect("should have parent folder");

    let renamed = service
        .rename_note(note.id, user.id, tenant_id, "Renamed Title".to_string())
        .await
        .unwrap();

    // rustshare.id is preserved.
    assert_eq!(renamed.okf_id, Some(original_okf_id));
    assert_eq!(renamed.metadata.okf_id, Some(original_okf_id));

    // Parent bundle folder renamed.
    let bundle_folder = metadata_store
        .find_folder_by_id(bundle_folder_id, user.id)
        .await
        .unwrap()
        .expect("bundle folder should exist");
    assert!(bundle_folder.name.contains("Renamed Title"));

    // Sidecar title updated.
    assert_eq!(renamed.metadata.title, "Renamed Title");

    // note.md frontmatter updated.
    assert!(renamed.content.contains("title: Renamed Title"));
    assert!(renamed.content.contains("bundle_name: Renamed Title"));
    assert!(renamed.content.contains(&format!("id: {original_okf_id}")));

    // Manifest title updated.
    let rustshare_folder = metadata_store
        .list_folders(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap()
        .into_iter()
        .find(|f| f.name == "_rustshare")
        .expect("_rustshare folder should exist");
    let manifest_file = metadata_store
        .list_files(Some(rustshare_folder.id), user.id, tenant_id)
        .await
        .unwrap()
        .into_iter()
        .find(|f| f.name == "manifest.json")
        .expect("manifest.json should exist");
    let manifest_bytes = object_store
        .get(&manifest_file.storage_key())
        .await
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["title"], "Renamed Title");
    assert_eq!(manifest["rustshare_id"], original_okf_id.to_string());

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_duplicate_note_generates_new_okf_id() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_dup_okf_user", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(user.id, tenant_id, Some("Original".to_string()), None, None)
        .await
        .unwrap();

    let original_okf_id = note.okf_id.expect("okf_id should be set");

    let duplicated = service
        .duplicate_note(note.id, user.id, tenant_id)
        .await
        .unwrap();

    assert_ne!(duplicated.id, note.id);
    let duplicated_okf_id = duplicated.okf_id.expect("duplicated okf_id should be set");
    assert!(!duplicated_okf_id.is_nil());
    assert_ne!(duplicated_okf_id, original_okf_id);
    assert!(duplicated
        .content
        .contains(&format!("id: {duplicated_okf_id}")));
    assert!(duplicated.content.contains("title: Original (copy)"));
    assert!(duplicated.content.contains("# Original (copy)"));

    cleanup_user(&pool, user.id).await;
}

// LB-02: Negative tenant/permission contract tests

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_get_note_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "note_user_a", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "note_user_b", tenant_b).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user_a.id,
            tenant_a,
            Some("Secret".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service.get_note(note.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Cross-tenant get_note should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_save_note_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "note_user_a2", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "note_user_b2", tenant_b).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user_a.id,
            tenant_a,
            Some("Secret".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service
        .save_note(
            note.id,
            user_b.id,
            tenant_b,
            "hacked".to_string(),
            None,
            None,
        )
        .await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Cross-tenant save_note should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_delete_note_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "note_user_a3", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "note_user_b3", tenant_b).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user_a.id,
            tenant_a,
            Some("Secret".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service.delete_note(note.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Cross-tenant delete_note should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_list_notes_does_not_leak() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "note_user_a4", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "note_user_b4", tenant_b).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let _note = service
        .create_note(
            user_a.id,
            tenant_a,
            Some("Secret".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let list_b = service
        .list_notes(user_b.id, tenant_b, 10, 0)
        .await
        .unwrap();
    assert!(
        !list_b.iter().any(|n| n.metadata.title == "Secret"),
        "Cross-tenant list_notes should not leak notes"
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_get_note_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "note_owner", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "note_other", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user_owner.id,
            tenant_id,
            Some("Private".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service.get_note(note.id, user_other.id, tenant_id).await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Same-tenant unauthorized get_note should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_private_note_not_accessible_without_share_id() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_share_user", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Private Note".to_string()),
            None,
            Some("secret".to_string()),
        )
        .await
        .unwrap();

    // Private note should not be accessible via get_public_note
    // We need the public_share_id to even try; since it's private, there's no share_id.
    // The test documents that there is no backdoor to access private note content.
    let result = service
        .get_public_note("nonexistentshareid12345678901234")
        .await;
    assert!(
        result.is_err(),
        "Random share_id should not access any note"
    );

    // Even the owner cannot access via public route without share_id
    assert!(note.metadata.public_share_id.is_none());

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_save_note_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "note_owner_save", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "note_other_save", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user_owner.id,
            tenant_id,
            Some("Private".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service
        .save_note(
            note.id,
            user_other.id,
            tenant_id,
            "hacked".to_string(),
            None,
            None,
        )
        .await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Same-tenant unauthorized save_note should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_delete_note_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "note_owner_del", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "note_other_del", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user_owner.id,
            tenant_id,
            Some("Private".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service.delete_note(note.id, user_other.id, tenant_id).await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Same-tenant unauthorized delete_note should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_rename_note_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "note_owner_rename", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "note_other_rename", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user_owner.id,
            tenant_id,
            Some("Private".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service
        .rename_note(note.id, user_other.id, tenant_id, "Hacked".to_string())
        .await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Same-tenant unauthorized rename_note should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_list_notes_does_not_leak() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "note_owner_list", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "note_other_list", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let _note = service
        .create_note(
            user_owner.id,
            tenant_id,
            Some("Private".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let list_other = service
        .list_notes(user_other.id, tenant_id, 10, 0)
        .await
        .unwrap();
    assert!(
        !list_other.iter().any(|n| n.metadata.title == "Private"),
        "Same-tenant unauthorized list_notes should not leak notes"
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_rename_note_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "note_user_a_rename", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "note_user_b_rename", tenant_b).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user_a.id,
            tenant_a,
            Some("Secret".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let result = service
        .rename_note(note.id, user_b.id, tenant_b, "Hacked".to_string())
        .await;
    assert!(
        matches!(result, Err(NoteError::PermissionDenied)),
        "Cross-tenant rename_note should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

// ============================================================================
// Step 11: Attachment Security and Portability Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_note_attachment_upload_rejects_dotdot_filename() {
    // FIXED: FileService::validate_file_name now rejects '..' substring.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_attach_dotdot", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service =
        create_file_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Attach Test".to_string()),
            None,
            None,
        )
        .await
        .unwrap();
    let bundle_folder_id = note.parent_folder_id.unwrap();
    let subfolders = metadata_store
        .list_folders(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    let attachments_folder = subfolders.iter().find(|f| f.name == "attachments").unwrap();

    let result = file_service
        .upload_file(
            user.id,
            "..secret.txt".to_string(),
            Some(attachments_folder.id),
            Bytes::from("test"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await;

    assert!(
        result.is_err(),
        "dotdot filename should be rejected: {:?}",
        result
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_note_attachment_upload_rejects_backslash_filename() {
    // FIXED: FileService::validate_file_name now rejects '\'.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_attach_backslash", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service =
        create_file_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Attach Test".to_string()),
            None,
            None,
        )
        .await
        .unwrap();
    let bundle_folder_id = note.parent_folder_id.unwrap();
    let subfolders = metadata_store
        .list_folders(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    let attachments_folder = subfolders.iter().find(|f| f.name == "attachments").unwrap();

    let result = file_service
        .upload_file(
            user.id,
            "secret\\file.txt".to_string(),
            Some(attachments_folder.id),
            Bytes::from("test"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await;

    assert!(
        result.is_err(),
        "backslash filename should be rejected: {:?}",
        result
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_note_attachment_upload_rejects_editor_json_filename() {
    // FIXED: FileService now rejects index.editor.json.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_attach_editor", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service =
        create_file_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Attach Test".to_string()),
            None,
            None,
        )
        .await
        .unwrap();
    let bundle_folder_id = note.parent_folder_id.unwrap();
    let subfolders = metadata_store
        .list_folders(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    let attachments_folder = subfolders.iter().find(|f| f.name == "attachments").unwrap();

    let result = file_service
        .upload_file(
            user.id,
            "index.editor.json".to_string(),
            Some(attachments_folder.id),
            Bytes::from("test"),
            "application/json".to_string(),
            tenant_id,
        )
        .await;

    assert!(
        result.is_err(),
        "index.editor.json should be rejected: {:?}",
        result
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_note_bundle_count_excludes_hidden_files() {
    // FIXED: count_bundle_contents now filters hidden metadata files.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_attach_hidden", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service = create_file_service(
        event_store,
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Hidden Count Test".to_string()),
            None,
            None,
        )
        .await
        .unwrap();
    let bundle_folder_id = note.parent_folder_id.unwrap();
    let subfolders = metadata_store
        .list_folders(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    let attachments_folder = subfolders.iter().find(|f| f.name == "attachments").unwrap();

    // Upload a regular file
    file_service
        .upload_file(
            user.id,
            "real.txt".to_string(),
            Some(attachments_folder.id),
            Bytes::from("real"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    // Inject a hidden metadata file directly to verify filtering in counts
    let hidden_file = rustshare_core::domain::File::new(
        ".rustshare.json".to_string(),
        format!(
            "{}/.rustshare.json",
            attachments_folder.path.trim_end_matches('/')
        ),
        "d41d8cd98f00b204e9800998ecf8427e".to_string(), // empty md5
        0,
        "application/json".to_string(),
        Some(attachments_folder.id),
        user.id,
        tenant_id,
    );
    metadata_store.create_file(&hidden_file).await.unwrap();

    let notes = service.list_notes(user.id, tenant_id, 10, 0).await.unwrap();
    let found = notes.iter().find(|n| n.id == note.id).unwrap();

    assert_eq!(
        found.attachment_count, 1,
        "hidden metadata file should not be counted"
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_public_note_excludes_attachment_metadata() {
    // PublicNote must not leak attachment metadata or internal paths.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_public_attach", tenant_id).await;
    let service = create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Public Attach".to_string()),
            None,
            Some("content".to_string()),
        )
        .await
        .unwrap();

    let public = service
        .toggle_visibility(note.id, user.id, tenant_id)
        .await
        .unwrap();
    let share_id = public.metadata.public_share_id.unwrap();

    let anon = service.get_public_note(&share_id).await.unwrap();
    // PublicNote has no attachments field, so no attachment metadata is exposed
    let json = serde_json::to_value(&anon).unwrap();
    assert!(
        json.get("attachments").is_none(),
        "PublicNote should not expose attachments"
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_note_attachment_duplicate_overwrites() {
    // CURRENT BEHAVIOR: FileService uploads to the same path overwrite existing content
    // (creating a new version) instead of rejecting or renaming. This test pins that behavior.
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_attach_dup", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service =
        create_file_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(user.id, tenant_id, Some("Dup Test".to_string()), None, None)
        .await
        .unwrap();
    let bundle_folder_id = note.parent_folder_id.unwrap();
    let subfolders = metadata_store
        .list_folders(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    let attachments_folder = subfolders.iter().find(|f| f.name == "attachments").unwrap();

    let first = file_service
        .upload_file(
            user.id,
            "dup.txt".to_string(),
            Some(attachments_folder.id),
            Bytes::from("first"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    let second = file_service
        .upload_file(
            user.id,
            "dup.txt".to_string(),
            Some(attachments_folder.id),
            Bytes::from("second"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    // Same file ID, overwritten content, version incremented
    assert_eq!(
        first.id, second.id,
        "Duplicate filename should overwrite existing file"
    );
    assert_eq!(second.current_version, first.current_version + 1);
    assert_eq!(second.size, 6); // "second" length

    cleanup_user(&pool, user.id).await;
}

// ============================================================================
// OKF-native reconciliation tests
// ============================================================================

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_reconcile_external_yaml_title_updates_manifest_and_folder() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_reconcile_yaml_user", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service = create_file_service(
        event_store,
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Original Title".to_string()),
            None,
            Some("body".to_string()),
        )
        .await
        .unwrap();

    let bundle_folder_id = note.parent_folder_id.unwrap();

    // Simulate external edit: rewrite note.md with a new title but same OKF id.
    let okf_id = note.okf_id.unwrap();
    let edited_doc = format!(
        r#"---
type: Note
title: YAML Edited Title
rustshare:
  id: {okf_id}
  module: notes
  bundle_name: YAML Edited Title
---
# YAML Edited Title

body"#
    );
    file_service
        .edit_file(note.id, user.id, Bytes::from(edited_doc), "overwrite", None)
        .await
        .unwrap();

    // Reconcile on read.
    let reconciled = service.get_note(note.id, user.id, tenant_id).await.unwrap();

    assert_eq!(reconciled.metadata.title, "YAML Edited Title");
    assert!(reconciled.conflict.is_none());
    assert!(reconciled.content.contains("title: YAML Edited Title"));

    let bundle_folder = metadata_store
        .find_folder_by_id(bundle_folder_id, user.id)
        .await
        .unwrap()
        .expect("bundle folder should exist");
    assert!(
        bundle_folder.name.contains("YAML Edited Title"),
        "folder should be renamed from YAML title, got: {}",
        bundle_folder.name
    );

    // Manifest title updated.
    let subfolders = metadata_store
        .list_folders(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    let rustshare_folder = subfolders.iter().find(|f| f.name == "_rustshare").unwrap();
    let manifest_files = metadata_store
        .list_files(Some(rustshare_folder.id), user.id, tenant_id)
        .await
        .unwrap();
    let manifest_file = manifest_files
        .into_iter()
        .find(|f| f.name == "manifest.json")
        .expect("manifest.json should exist");
    let manifest_bytes = object_store
        .get(&manifest_file.storage_key())
        .await
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["title"], "YAML Edited Title");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_reconcile_external_folder_rename_updates_yaml_and_manifest() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_reconcile_folder_user", tenant_id).await;
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

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Original Title".to_string()),
            None,
            Some("body".to_string()),
        )
        .await
        .unwrap();

    let bundle_folder_id = note.parent_folder_id.unwrap();

    // Simulate external folder rename.
    folder_service
        .rename_folder(
            bundle_folder_id,
            "Folder Renamed Externally".to_string(),
            user.id,
        )
        .await
        .unwrap();

    let reconciled = service.get_note(note.id, user.id, tenant_id).await.unwrap();

    assert_eq!(reconciled.metadata.title, "Folder Renamed Externally");
    assert!(reconciled.conflict.is_none());
    assert!(reconciled
        .content
        .contains("title: Folder Renamed Externally"));
    assert!(reconciled
        .content
        .contains("bundle_name: Folder Renamed Externally"));

    let subfolders = metadata_store
        .list_folders(Some(bundle_folder_id), user.id, tenant_id)
        .await
        .unwrap();
    let rustshare_folder = subfolders.iter().find(|f| f.name == "_rustshare").unwrap();
    let manifest_files = metadata_store
        .list_files(Some(rustshare_folder.id), user.id, tenant_id)
        .await
        .unwrap();
    let manifest_file = manifest_files
        .into_iter()
        .find(|f| f.name == "manifest.json")
        .expect("manifest.json should exist");
    let manifest_bytes = object_store
        .get(&manifest_file.storage_key())
        .await
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["title"], "Folder Renamed Externally");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_reconcile_both_title_and_folder_changed_creates_conflict() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_reconcile_conflict_user", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service = create_file_service(
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

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Original Title".to_string()),
            None,
            Some("body".to_string()),
        )
        .await
        .unwrap();

    let bundle_folder_id = note.parent_folder_id.unwrap();
    let okf_id = note.okf_id.unwrap();

    // External YAML title edit.
    let edited_doc = format!(
        r#"---
type: Note
title: YAML Title
rustshare:
  id: {okf_id}
  module: notes
  bundle_name: YAML Title
---
# YAML Title

body"#
    );
    file_service
        .edit_file(note.id, user.id, Bytes::from(edited_doc), "overwrite", None)
        .await
        .unwrap();

    // External folder rename to something different.
    folder_service
        .rename_folder(bundle_folder_id, "Folder Name".to_string(), user.id)
        .await
        .unwrap();

    let reconciled = service.get_note(note.id, user.id, tenant_id).await.unwrap();

    let conflict = reconciled
        .conflict
        .expect("should have a title_mismatch conflict");
    assert_eq!(conflict.kind, "title_mismatch");
    assert_eq!(conflict.yaml_title, Some("YAML Title".to_string()));
    assert_eq!(conflict.folder_name, Some("Folder Name".to_string()));

    // Neither YAML nor folder should be overwritten by the other.
    assert!(reconciled.content.contains("title: YAML Title"));
    let bundle_folder = metadata_store
        .find_folder_by_id(bundle_folder_id, user.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bundle_folder.name, "Folder Name");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_reconcile_missing_okf_id_generates_and_persists_id() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_reconcile_id_user", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service =
        create_file_service(event_store, metadata_store.clone(), object_store, &pool);

    // Create a setup note to ensure /Workspace/Notes exists.
    let setup = service
        .create_note(user.id, tenant_id, Some("Setup".to_string()), None, None)
        .await
        .unwrap();
    let bundle_folder = metadata_store
        .find_folder_by_id(setup.parent_folder_id.unwrap(), user.id)
        .await
        .unwrap()
        .unwrap();
    let notes_folder = metadata_store
        .find_folder_by_id(bundle_folder.parent_folder_id.unwrap(), user.id)
        .await
        .unwrap()
        .unwrap();

    // Upload a standalone markdown file without rustshare.id.
    let standalone = file_service
        .upload_file(
            user.id,
            "no-id.md".to_string(),
            Some(notes_folder.id),
            Bytes::from("---\ntype: Note\ntitle: No Id Note\n---\n# No Id Note\n"),
            "text/markdown".to_string(),
            tenant_id,
        )
        .await
        .unwrap();

    let reconciled = service
        .get_note(standalone.id, user.id, tenant_id)
        .await
        .unwrap();

    let okf_id = reconciled
        .okf_id
        .expect("missing okf_id should be generated");
    assert!(!okf_id.is_nil());
    assert!(reconciled.content.contains(&format!("id: {okf_id}")));

    // A sidecar should now exist with the generated id.
    assert_eq!(reconciled.metadata.okf_id, Some(okf_id));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_reconcile_preserves_unknown_frontmatter_fields() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_reconcile_preserve_user", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service =
        create_file_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Original Title".to_string()),
            None,
            Some("body".to_string()),
        )
        .await
        .unwrap();

    let okf_id = note.okf_id.unwrap();
    let edited_doc = format!(
        r#"---
type: Note
title: Original Title
custom_field: preserved_value
rustshare:
  id: {okf_id}
  module: notes
  custom_nested: also_preserved
---
body"#
    );
    file_service
        .edit_file(note.id, user.id, Bytes::from(edited_doc), "overwrite", None)
        .await
        .unwrap();

    let reconciled = service.get_note(note.id, user.id, tenant_id).await.unwrap();

    assert!(reconciled.content.contains("custom_field: preserved_value"));
    assert!(reconciled.content.contains("custom_nested: also_preserved"));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_reconcile_identity_mismatch_creates_conflict() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_reconcile_identity_user", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service =
        create_file_service(event_store, metadata_store.clone(), object_store, &pool);

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Original Title".to_string()),
            None,
            Some("body".to_string()),
        )
        .await
        .unwrap();

    let original_okf_id = note.okf_id.unwrap();
    let tampered_okf_id = Uuid::new_v4();
    assert_ne!(tampered_okf_id, original_okf_id);

    let edited_doc = format!(
        r#"---
type: Note
title: Original Title
rustshare:
  id: {tampered_okf_id}
  module: notes
---
body"#
    );
    file_service
        .edit_file(note.id, user.id, Bytes::from(edited_doc), "overwrite", None)
        .await
        .unwrap();

    let reconciled = service.get_note(note.id, user.id, tenant_id).await.unwrap();

    let conflict = reconciled
        .conflict
        .expect("should have an identity_mismatch conflict");
    assert_eq!(conflict.kind, "identity_mismatch");
    assert_eq!(conflict.yaml_id, Some(tampered_okf_id));
    assert_eq!(conflict.sidecar_id, Some(original_okf_id));

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_resolve_note_conflict_clears_conflict_and_persists_choice() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "note_resolve_conflict_user", tenant_id).await;
    let service = create_note_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let file_service = create_file_service(
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

    let note = service
        .create_note(
            user.id,
            tenant_id,
            Some("Original Title".to_string()),
            None,
            Some("body".to_string()),
        )
        .await
        .unwrap();

    let bundle_folder_id = note.parent_folder_id.unwrap();
    let okf_id = note.okf_id.unwrap();

    // Create a title/folder conflict.
    let edited_doc = format!(
        r#"---
type: Note
title: YAML Title
rustshare:
  id: {okf_id}
  module: notes
---
body"#
    );
    file_service
        .edit_file(note.id, user.id, Bytes::from(edited_doc), "overwrite", None)
        .await
        .unwrap();
    folder_service
        .rename_folder(bundle_folder_id, "Folder Title".to_string(), user.id)
        .await
        .unwrap();

    let conflicted = service.get_note(note.id, user.id, tenant_id).await.unwrap();
    assert!(conflicted.conflict.is_some());

    // Resolve with a custom title.
    let resolved = service
        .resolve_note_conflict(
            note.id,
            user.id,
            tenant_id,
            NoteConflictResolution::Custom("Resolved Title".to_string()),
        )
        .await
        .unwrap();

    assert_eq!(resolved.metadata.title, "Resolved Title");
    assert!(resolved.conflict.is_none());
    assert!(resolved.content.contains("title: Resolved Title"));
    assert!(resolved.content.contains("bundle_name: Resolved Title"));

    let bundle_folder = metadata_store
        .find_folder_by_id(bundle_folder_id, user.id)
        .await
        .unwrap()
        .unwrap();
    assert!(bundle_folder.name.contains("Resolved Title"));

    cleanup_user(&pool, user.id).await;
}
