//! Contract tests for RustShare Meeting Notes module.
//!
//! Run with: cargo test --test meetings_test -- --ignored

use rustshare_core::domain::User;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::services::meeting_service::{MeetingError, MeetingService};
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
    sqlx::query("DELETE FROM files WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test files");

    sqlx::query("DELETE FROM folders WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test folders");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test user");
}

fn create_meeting_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &PgPool,
) -> Arc<MeetingService> {
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

    Arc::new(MeetingService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
    ))
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_create_meeting_creates_folder_structure_and_metadata() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "meeting_contract_user_1", tenant_id).await;
    let service = create_meeting_service(event_store, metadata_store.clone(), object_store, &pool);

    let meeting = service
        .create_meeting(
            user.id,
            tenant_id,
            "Sprint Planning".to_string(),
            "Engineering".to_string(),
            chrono::Utc::now(),
            "# Agenda\n\n- Review backlog".to_string(),
        )
        .await
        .expect("create_meeting should succeed");

    assert!(
        meeting.path.starts_with("/Workspace/Meetings/"),
        "Meeting should be in canonical /Workspace/Meetings folder, got path: {}",
        meeting.path
    );
    assert_eq!(meeting.metadata.title, "Sprint Planning");
    assert_eq!(meeting.metadata.team, "Engineering");
    assert_eq!(meeting.metadata.kind, "meeting");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_get_meeting_returns_content_and_metadata() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "meeting_contract_user_2", tenant_id).await;
    let service = create_meeting_service(event_store, metadata_store.clone(), object_store, &pool);

    let created = service
        .create_meeting(
            user.id,
            tenant_id,
            "Standup".to_string(),
            "Team A".to_string(),
            chrono::Utc::now(),
            "Notes".to_string(),
        )
        .await
        .unwrap();

    let meeting = service
        .get_meeting(created.id, user.id, tenant_id)
        .await
        .unwrap();
    assert_eq!(meeting.id, created.id);
    assert_eq!(meeting.content, "Notes");
    assert_eq!(meeting.metadata.title, "Standup");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_update_meeting_updates_content_and_metadata() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "meeting_contract_user_3", tenant_id).await;
    let service = create_meeting_service(event_store, metadata_store.clone(), object_store, &pool);

    let meeting = service
        .create_meeting(
            user.id,
            tenant_id,
            "Retro".to_string(),
            "Team B".to_string(),
            chrono::Utc::now(),
            "Old".to_string(),
        )
        .await
        .unwrap();

    let updated = service
        .update_meeting(
            meeting.id,
            user.id,
            tenant_id,
            Some("Retro Renamed".to_string()),
            Some("New content".to_string()),
            Some(vec!["alice".to_string()]),
        )
        .await
        .unwrap();

    assert_eq!(updated.metadata.title, "Retro Renamed");
    assert_eq!(updated.content, "New content");
    assert!(updated.metadata.attendees.contains(&"alice".to_string()));

    cleanup_user(&pool, user.id).await;
}

// LB-02: Negative tenant/permission contract tests

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_get_meeting_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "meeting_user_a", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "meeting_user_b", tenant_b).await;
    let service = create_meeting_service(event_store, metadata_store.clone(), object_store, &pool);

    let meeting = service
        .create_meeting(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            chrono::Utc::now(),
            "confidential".to_string(),
        )
        .await
        .unwrap();

    let result = service.get_meeting(meeting.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(MeetingError::PermissionDenied)),
        "Cross-tenant get_meeting should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_update_meeting_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "meeting_user_a2", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "meeting_user_b2", tenant_b).await;
    let service = create_meeting_service(event_store, metadata_store.clone(), object_store, &pool);

    let meeting = service
        .create_meeting(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            chrono::Utc::now(),
            "confidential".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .update_meeting(
            meeting.id,
            user_b.id,
            tenant_b,
            Some("Hacked".to_string()),
            Some("evil".to_string()),
            None,
        )
        .await;
    assert!(
        matches!(result, Err(MeetingError::PermissionDenied)),
        "Cross-tenant update_meeting should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_list_meetings_does_not_leak() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "meeting_user_a3", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "meeting_user_b3", tenant_b).await;
    let service = create_meeting_service(event_store, metadata_store.clone(), object_store, &pool);

    let _meeting = service
        .create_meeting(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            chrono::Utc::now(),
            "confidential".to_string(),
        )
        .await
        .unwrap();

    let list_b = service
        .list_meetings(user_b.id, tenant_b, 1000, 0)
        .await
        .unwrap();
    assert!(
        !list_b.iter().any(|m| m.metadata.title == "Secret"),
        "Cross-tenant list_meetings should not leak meetings"
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_get_meeting_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "meeting_owner", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "meeting_other", tenant_id).await;
    let service = create_meeting_service(event_store, metadata_store.clone(), object_store, &pool);

    let meeting = service
        .create_meeting(
            user_owner.id,
            tenant_id,
            "Private".to_string(),
            "Exec".to_string(),
            chrono::Utc::now(),
            "notes".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .get_meeting(meeting.id, user_other.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(MeetingError::PermissionDenied)),
        "Same-tenant unauthorized get_meeting should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_update_meeting_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "meeting_owner_update", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "meeting_other_update", tenant_id).await;
    let service = create_meeting_service(event_store, metadata_store.clone(), object_store, &pool);

    let meeting = service
        .create_meeting(
            user_owner.id,
            tenant_id,
            "Private".to_string(),
            "Exec".to_string(),
            chrono::Utc::now(),
            "notes".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .update_meeting(
            meeting.id,
            user_other.id,
            tenant_id,
            Some("Hacked".to_string()),
            Some("evil".to_string()),
            None,
        )
        .await;
    assert!(
        matches!(result, Err(MeetingError::PermissionDenied)),
        "Same-tenant unauthorized update_meeting should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}
