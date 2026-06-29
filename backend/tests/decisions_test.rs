//! Contract tests for RustShare Decision Records
//!
//! Run with: cargo test --test decisions_test -- --ignored

use rustshare_core::domain::User;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_server::services::decision_service::{DecisionError, DecisionService};
use rustshare_server::services::note_service::NoteService;
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

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup test user");
}

fn create_decision_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &PgPool,
) -> Arc<DecisionService> {
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

    Arc::new(DecisionService::new(
        file_service,
        folder_service,
        metadata_store,
        object_store,
    ))
}

fn create_note_service(
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    pool: &PgPool,
) -> Arc<NoteService> {
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
        pool.clone(),
    ))
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn contract_create_decision_creates_file_in_decisions_folder() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "decision_contract_user_1", tenant_id).await;
    let service = create_decision_service(event_store, metadata_store.clone(), object_store, &pool);

    let decision = service
        .create_decision(
            user.id,
            tenant_id,
            "Use Rust for backend".to_string(),
            "General".to_string(),
            "# Decision\n\nWe will use Rust.".to_string(),
        )
        .await
        .expect("create_decision should succeed");

    // 1. File is a markdown file
    assert!(decision.name.ends_with(".md"));
    // 2. File path is under canonical /Workspace/Decisions folder
    assert!(
        decision.path.starts_with("/Workspace/Decisions/"),
        "Decision should be in canonical /Workspace/Decisions folder, got path: {}",
        decision.path
    );
    // 3. Metadata sidecar exists with kind=decision
    assert_eq!(decision.metadata.kind, "decision");
    assert_eq!(decision.metadata.title, "Use Rust for backend");
    assert_eq!(decision.metadata.category, "General");
    // 4. Filename has DEC- prefix
    assert!(
        decision.name.starts_with("DEC-"),
        "Decision filename should start with DEC-, got: {}",
        decision.name
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_decision_does_not_appear_in_notes_list() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "decision_contract_user_2", tenant_id).await;
    let decision_service = create_decision_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let note_service =
        create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    // Create a decision
    let decision = decision_service
        .create_decision(
            user.id,
            tenant_id,
            "Should not appear in notes".to_string(),
            "General".to_string(),
            "# Decision content".to_string(),
        )
        .await
        .unwrap();

    // Create a note
    let note = note_service
        .create_note(
            user.id,
            tenant_id,
            Some("Real note".to_string()),
            None,
            Some("# Note content".to_string()),
        )
        .await
        .unwrap();

    // List notes — should NOT contain the decision
    let notes = note_service
        .list_notes(user.id, tenant_id, 50, 0)
        .await
        .unwrap();

    assert!(
        notes.iter().any(|n| n.id == note.id),
        "Notes list should contain the real note"
    );
    assert!(
        !notes.iter().any(|n| n.id == decision.id),
        "Notes list should NOT contain the decision, but it did"
    );

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_rename_decision_updates_filename_and_metadata() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "decision_contract_user_3", tenant_id).await;
    let service = create_decision_service(event_store, metadata_store.clone(), object_store, &pool);

    let decision = service
        .create_decision(
            user.id,
            tenant_id,
            "Original Title".to_string(),
            "General".to_string(),
            "# Decision".to_string(),
        )
        .await
        .unwrap();

    let original_name = decision.name;
    let original_prefix = original_name
        .split('-')
        .take(2)
        .collect::<Vec<_>>()
        .join("-");

    // Rename the decision
    let renamed = service
        .rename_decision(decision.id, user.id, tenant_id, "Updated Title".to_string())
        .await
        .unwrap();

    // 1. Name should still have DEC- prefix and .md suffix
    assert!(
        renamed.name.starts_with(&original_prefix),
        "DEC-ID prefix should be preserved"
    );
    assert!(renamed.name.ends_with(".md"));
    // 2. Title should be updated
    assert_eq!(renamed.metadata.title, "Updated Title");
    // 3. Name should contain the new slug
    assert!(
        renamed.name.contains("updated-title") || renamed.name.contains("updated-title"),
        "Filename should contain new slug, got: {}",
        renamed.name
    );
    // 4. ID should be unchanged
    assert_eq!(renamed.id, decision.id);
    // 5. Content should be preserved
    assert_eq!(renamed.content, "# Decision");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_rename_decision_empty_title_fails() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "decision_contract_user_4", tenant_id).await;
    let service = create_decision_service(event_store, metadata_store.clone(), object_store, &pool);

    let decision = service
        .create_decision(
            user.id,
            tenant_id,
            "Some Title".to_string(),
            "General".to_string(),
            "# Decision".to_string(),
        )
        .await
        .unwrap();

    // Empty title should fail
    let result = service
        .rename_decision(decision.id, user.id, tenant_id, "   ".to_string())
        .await;
    assert!(result.is_err(), "Renaming with empty title should fail");

    // Empty string should also fail
    let result2 = service
        .rename_decision(decision.id, user.id, tenant_id, "".to_string())
        .await;
    assert!(result2.is_err(), "Renaming with empty string should fail");

    cleanup_user(&pool, user.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_list_decisions_only_returns_decisions() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&metadata_store, "decision_contract_user_5", tenant_id).await;
    let decision_service = create_decision_service(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        &pool,
    );
    let note_service =
        create_note_service(event_store, metadata_store.clone(), object_store, &pool);

    // Create a decision
    let decision = decision_service
        .create_decision(
            user.id,
            tenant_id,
            "Architecture Decision".to_string(),
            "General".to_string(),
            "# ADR".to_string(),
        )
        .await
        .unwrap();

    // Create a note (should not appear in decisions list)
    let _note = note_service
        .create_note(
            user.id,
            tenant_id,
            Some("Random note".to_string()),
            None,
            Some("# Note".to_string()),
        )
        .await
        .unwrap();

    // List decisions
    let decisions = decision_service
        .list_decisions(user.id, tenant_id, 1000, 0)
        .await
        .unwrap();

    assert!(
        decisions.iter().any(|d| d.id == decision.id),
        "Decisions list should contain the decision"
    );
    // Decision list filters by path, so notes should not appear anyway
    assert_eq!(
        decisions.len(),
        1,
        "Decisions list should contain exactly 1 decision"
    );

    cleanup_user(&pool, user.id).await;
}
// LB-02: Negative tenant/permission contract tests

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_get_decision_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "decision_user_a", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "decision_user_b", tenant_b).await;
    let service = create_decision_service(event_store, metadata_store.clone(), object_store, &pool);

    let decision = service
        .create_decision(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let result = service.get_decision(decision.id, user_b.id, tenant_b).await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Cross-tenant get_decision should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_update_decision_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "decision_user_a2", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "decision_user_b2", tenant_b).await;
    let service = create_decision_service(event_store, metadata_store.clone(), object_store, &pool);

    let decision = service
        .create_decision(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .update_decision(
            decision.id,
            user_b.id,
            tenant_b,
            Some("Hacked".to_string()),
            None,
            Some("evil".to_string()),
        )
        .await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Cross-tenant update_decision should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_rename_decision_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "decision_user_a3", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "decision_user_b3", tenant_b).await;
    let service = create_decision_service(event_store, metadata_store.clone(), object_store, &pool);

    let decision = service
        .create_decision(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .rename_decision(decision.id, user_b.id, tenant_b, "Hacked".to_string())
        .await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Cross-tenant rename_decision should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_cross_tenant_list_decisions_does_not_leak() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let user_a = create_test_user(&metadata_store, "decision_user_a4", tenant_a).await;
    let user_b = create_test_user(&metadata_store, "decision_user_b4", tenant_b).await;
    let service = create_decision_service(event_store, metadata_store.clone(), object_store, &pool);

    let _decision = service
        .create_decision(
            user_a.id,
            tenant_a,
            "Secret".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let list_b = service
        .list_decisions(user_b.id, tenant_b, 1000, 0)
        .await
        .unwrap();
    assert!(
        !list_b.iter().any(|d| d.metadata.title == "Secret"),
        "Cross-tenant list_decisions should not leak decisions"
    );

    cleanup_user(&pool, user_a.id).await;
    cleanup_user(&pool, user_b.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_get_decision_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "decision_owner", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "decision_other", tenant_id).await;
    let service = create_decision_service(event_store, metadata_store.clone(), object_store, &pool);

    let decision = service
        .create_decision(
            user_owner.id,
            tenant_id,
            "Private".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .get_decision(decision.id, user_other.id, tenant_id)
        .await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Same-tenant unauthorized get_decision should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_update_decision_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "decision_owner_update", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "decision_other_update", tenant_id).await;
    let service = create_decision_service(event_store, metadata_store.clone(), object_store, &pool);

    let decision = service
        .create_decision(
            user_owner.id,
            tenant_id,
            "Private".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .update_decision(
            decision.id,
            user_other.id,
            tenant_id,
            Some("Hacked".to_string()),
            None,
            Some("evil".to_string()),
        )
        .await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Same-tenant unauthorized update_decision should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}

#[tokio::test]
#[ignore = "Requires database and S3"]
async fn contract_same_tenant_unauthorized_rename_decision_denied() {
    let (pool, event_store, metadata_store, object_store) = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user_owner = create_test_user(&metadata_store, "decision_owner_rename", tenant_id).await;
    let user_other = create_test_user(&metadata_store, "decision_other_rename", tenant_id).await;
    let service = create_decision_service(event_store, metadata_store.clone(), object_store, &pool);

    let decision = service
        .create_decision(
            user_owner.id,
            tenant_id,
            "Private".to_string(),
            "Exec".to_string(),
            "content".to_string(),
        )
        .await
        .unwrap();

    let result = service
        .rename_decision(decision.id, user_other.id, tenant_id, "Hacked".to_string())
        .await;
    assert!(
        matches!(result, Err(DecisionError::PermissionDenied)),
        "Same-tenant unauthorized rename_decision should be denied, got {:?}",
        result
    );

    cleanup_user(&pool, user_owner.id).await;
    cleanup_user(&pool, user_other.id).await;
}
