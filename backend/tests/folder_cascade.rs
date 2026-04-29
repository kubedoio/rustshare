//! Integration test: Folder Cascade Delete (Task 27)
//!
//! Tests cascade deletion of folder hierarchies:
//! 1. Create folder hierarchy: Root → Docs → Work → Projects
//! 2. Add files to various folders
//! 3. Delete "Docs" folder
//! 4. Verify all descendants (Work, Projects) and their files are deleted
//!
//! These tests require a running database and S3-compatible storage.
//! Run with: cargo test --test folder_cascade -- --ignored

use bytes::Bytes;
use rustshare_core::domain::User;
use rustshare_core::services::{FileService, FolderService};
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use sqlx::PgPool;
use std::sync::Arc;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::PermissionResolver;
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use uuid::Uuid;

/// Setup test environment with database and S3 connections
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

/// Create a test user in the database
async fn create_test_user(metadata_store: &MetadataStore, username: &str, tenant_id: Uuid) -> User {
    let user = User::new(
        username.to_string(),
        format!("{} Display", username),
        "test_password_hash".to_string(),
        format!("{}@test.local", username),
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

/// Cleanup test user from database
async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test user");
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_folder_cascade_delete() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "cascade_user", tenant_id).await;

    // Create services
    let folder_service = FolderService::new(event_store.clone(), metadata_store.clone());
    let file_service = create_file_service(event_store.clone(), metadata_store.clone(), object_store.clone(), &pool);

    // Step 1: Create folder hierarchy: Root → Docs → Work → Projects
    let root = folder_service
        .create_folder("Root".to_string(), None, user.id, tenant_id)
        .await
        .expect("Failed to create Root folder");

    let docs = folder_service
        .create_folder("Docs".to_string(), Some(root.id), user.id, tenant_id)
        .await
        .expect("Failed to create Docs folder");

    let work = folder_service
        .create_folder("Work".to_string(), Some(docs.id), user.id, tenant_id)
        .await
        .expect("Failed to create Work folder");

    let projects = folder_service
        .create_folder("Projects".to_string(), Some(work.id), user.id, tenant_id)
        .await
        .expect("Failed to create Projects folder");

    // Step 2: Add files to various folders
    let file_in_root = file_service
        .upload_file(
            user.id,
            "root-file.txt".to_string(),
            Some(root.id),
            Bytes::from("File in root"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file to root");

    let file_in_docs = file_service
        .upload_file(
            user.id,
            "docs-file.txt".to_string(),
            Some(docs.id),
            Bytes::from("File in docs"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file to docs");

    let file_in_work = file_service
        .upload_file(
            user.id,
            "work-file.txt".to_string(),
            Some(work.id),
            Bytes::from("File in work"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file to work");

    let file_in_projects = file_service
        .upload_file(
            user.id,
            "project-file.txt".to_string(),
            Some(projects.id),
            Bytes::from("File in projects"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file to projects");

    // Verify all folders and files exist
    assert!(folder_service.get_folder(root.id, user.id).await.is_ok());
    assert!(folder_service.get_folder(docs.id, user.id).await.is_ok());
    assert!(folder_service.get_folder(work.id, user.id).await.is_ok());
    assert!(folder_service.get_folder(projects.id, user.id).await.is_ok());

    assert!(file_service.get_file(file_in_root.id, user.id).await.is_ok());
    assert!(file_service.get_file(file_in_docs.id, user.id).await.is_ok());
    assert!(file_service.get_file(file_in_work.id, user.id).await.is_ok());
    assert!(file_service.get_file(file_in_projects.id, user.id).await.is_ok());

    // Step 3: Delete "Docs" folder (cascade should delete Work, Projects, and all their files)
    folder_service
        .delete_folder(docs.id, user.id)
        .await
        .expect("Failed to delete Docs folder");

    // Step 4: Verify cascade deletion

    // Root and its file should still exist
    assert!(
        folder_service.get_folder(root.id, user.id).await.is_ok(),
        "Root folder should still exist"
    );
    assert!(
        file_service.get_file(file_in_root.id, user.id).await.is_ok(),
        "File in root should still exist"
    );

    // Docs and all descendants should be deleted
    assert!(
        folder_service.get_folder(docs.id, user.id).await.is_err(),
        "Docs folder should be deleted"
    );
    assert!(
        folder_service.get_folder(work.id, user.id).await.is_err(),
        "Work folder should be deleted (cascade)"
    );
    assert!(
        folder_service.get_folder(projects.id, user.id).await.is_err(),
        "Projects folder should be deleted (cascade)"
    );

    // Files in deleted folders should also be deleted
    assert!(
        file_service.get_file(file_in_docs.id, user.id).await.is_err(),
        "File in docs should be deleted"
    );
    assert!(
        file_service.get_file(file_in_work.id, user.id).await.is_err(),
        "File in work should be deleted (cascade)"
    );
    assert!(
        file_service.get_file(file_in_projects.id, user.id).await.is_err(),
        "File in projects should be deleted (cascade)"
    );

    // Cleanup remaining resources
    file_service
        .delete_file(file_in_root.id, user.id)
        .await
        .expect("Failed to delete file in root");
    folder_service
        .delete_folder(root.id, user.id)
        .await
        .expect("Failed to delete root folder");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_deep_hierarchy_cascade_delete() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "deep_cascade_user", tenant_id).await;

    // Create services
    let folder_service = FolderService::new(event_store.clone(), metadata_store.clone());
    let file_service = create_file_service(event_store.clone(), metadata_store.clone(), object_store.clone(), &pool);

    // Create a deeper hierarchy: Root → L1 → L2 → L3 → L4 → L5
    let root = folder_service
        .create_folder("Root".to_string(), None, user.id, tenant_id)
        .await
        .expect("Failed to create root");

    let mut parent_id = root.id;
    let mut folder_ids = vec![root.id];

    for i in 1..=5 {
        let folder = folder_service
            .create_folder(format!("Level{}", i), Some(parent_id), user.id, tenant_id)
            .await
            .expect(&format!("Failed to create level {} folder", i));

        folder_ids.push(folder.id);
        parent_id = folder.id;

        // Add a file to each level
        file_service
            .upload_file(
                user.id,
                format!("file-level{}.txt", i),
                Some(folder.id),
                Bytes::from(format!("Content at level {}", i)),
                "text/plain".to_string(),
            tenant_id,
            )
            .await
            .expect(&format!("Failed to upload file to level {}", i));
    }

    // Delete the second level (Level1)
    let level1_id = folder_ids[1];
    folder_service
        .delete_folder(level1_id, user.id)
        .await
        .expect("Failed to delete Level1");

    // Root should still exist
    assert!(
        folder_service.get_folder(root.id, user.id).await.is_ok(),
        "Root should still exist"
    );

    // All other levels should be deleted
    for i in 1..=5 {
        let folder_id = folder_ids[i];
        assert!(
            folder_service.get_folder(folder_id, user.id).await.is_err(),
            "Level{} should be deleted", i
        );
    }

    // Cleanup root
    folder_service
        .delete_folder(root.id, user.id)
        .await
        .expect("Failed to delete root");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_cascade_delete_with_siblings() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "siblings_cascade_user", tenant_id).await;

    // Create services
    let folder_service = FolderService::new(event_store.clone(), metadata_store.clone());

    // Create hierarchy with siblings:
    //   Root
    //   ├── FolderA
    //   │   ├── ChildA1
    //   │   └── ChildA2
    //   └── FolderB
    //       ├── ChildB1
    //       └── ChildB2

    let root = folder_service
        .create_folder("Root".to_string(), None, user.id, tenant_id)
        .await
        .expect("Failed to create root");

    let folder_a = folder_service
        .create_folder("FolderA".to_string(), Some(root.id), user.id, tenant_id)
        .await
        .expect("Failed to create FolderA");

    let child_a1 = folder_service
        .create_folder("ChildA1".to_string(), Some(folder_a.id), user.id, tenant_id)
        .await
        .expect("Failed to create ChildA1");

    let child_a2 = folder_service
        .create_folder("ChildA2".to_string(), Some(folder_a.id), user.id, tenant_id)
        .await
        .expect("Failed to create ChildA2");

    let folder_b = folder_service
        .create_folder("FolderB".to_string(), Some(root.id), user.id, tenant_id)
        .await
        .expect("Failed to create FolderB");

    let child_b1 = folder_service
        .create_folder("ChildB1".to_string(), Some(folder_b.id), user.id, tenant_id)
        .await
        .expect("Failed to create ChildB1");

    let child_b2 = folder_service
        .create_folder("ChildB2".to_string(), Some(folder_b.id), user.id, tenant_id)
        .await
        .expect("Failed to create ChildB2");

    // Delete FolderA (should delete ChildA1 and ChildA2 but not FolderB or its children)
    folder_service
        .delete_folder(folder_a.id, user.id)
        .await
        .expect("Failed to delete FolderA");

    // Verify FolderA and its children are deleted
    assert!(
        folder_service.get_folder(folder_a.id, user.id).await.is_err(),
        "FolderA should be deleted"
    );
    assert!(
        folder_service.get_folder(child_a1.id, user.id).await.is_err(),
        "ChildA1 should be deleted"
    );
    assert!(
        folder_service.get_folder(child_a2.id, user.id).await.is_err(),
        "ChildA2 should be deleted"
    );

    // Verify FolderB and its children still exist
    assert!(
        folder_service.get_folder(root.id, user.id).await.is_ok(),
        "Root should still exist"
    );
    assert!(
        folder_service.get_folder(folder_b.id, user.id).await.is_ok(),
        "FolderB should still exist"
    );
    assert!(
        folder_service.get_folder(child_b1.id, user.id).await.is_ok(),
        "ChildB1 should still exist"
    );
    assert!(
        folder_service.get_folder(child_b2.id, user.id).await.is_ok(),
        "ChildB2 should still exist"
    );

    // Cleanup
    folder_service
        .delete_folder(folder_b.id, user.id)
        .await
        .expect("Failed to delete FolderB");
    folder_service
        .delete_folder(root.id, user.id)
        .await
        .expect("Failed to delete root");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_leaf_folder_delete() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;

    // Create test user
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "leaf_delete_user", tenant_id).await;

    // Create services
    let folder_service = FolderService::new(event_store.clone(), metadata_store.clone());
    let file_service = create_file_service(event_store.clone(), metadata_store.clone(), object_store.clone(), &pool);

    // Create simple hierarchy: Root → Parent → Leaf
    let root = folder_service
        .create_folder("Root".to_string(), None, user.id, tenant_id)
        .await
        .expect("Failed to create root");

    let parent = folder_service
        .create_folder("Parent".to_string(), Some(root.id), user.id, tenant_id)
        .await
        .expect("Failed to create parent");

    let leaf = folder_service
        .create_folder("Leaf".to_string(), Some(parent.id), user.id, tenant_id)
        .await
        .expect("Failed to create leaf");

    // Add file to leaf
    let file_in_leaf = file_service
        .upload_file(
            user.id,
            "leaf-file.txt".to_string(),
            Some(leaf.id),
            Bytes::from("Leaf content"),
            "text/plain".to_string(),
            tenant_id,
        )
        .await
        .expect("Failed to upload file to leaf");

    // Delete leaf folder
    folder_service
        .delete_folder(leaf.id, user.id)
        .await
        .expect("Failed to delete leaf");

    // Verify only leaf and its file are deleted
    assert!(
        folder_service.get_folder(leaf.id, user.id).await.is_err(),
        "Leaf should be deleted"
    );
    assert!(
        file_service.get_file(file_in_leaf.id, user.id).await.is_err(),
        "File in leaf should be deleted"
    );

    // Parent and root should still exist
    assert!(
        folder_service.get_folder(parent.id, user.id).await.is_ok(),
        "Parent should still exist"
    );
    assert!(
        folder_service.get_folder(root.id, user.id).await.is_ok(),
        "Root should still exist"
    );

    // Cleanup
    folder_service
        .delete_folder(parent.id, user.id)
        .await
        .expect("Failed to delete parent");
    folder_service
        .delete_folder(root.id, user.id)
        .await
        .expect("Failed to delete root");

    cleanup_user(&pool, user.id).await;
}
