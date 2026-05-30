//! Integration test: Operational readiness endpoint.
//!
//! Tests the `/health/ready` readiness probe.
//!
//! Non-ignored tests exercise the pure readiness evaluation logic without
//! infrastructure. Ignored tests validate the handler against real DB/S3.
//!
//! Run pure logic tests:
//!   cargo test --test readiness_test
//!
//! Run ignored infrastructure tests:
//!   cargo test --test readiness_test -- --ignored

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{
    FileService, FolderService, NotificationService,
    PermissionResolver, ShareService, ThumbnailService,
};
use rustshare_infrastructure::repositories::{
    FileRepository, FolderRepository, NotificationRepository, PermissionResolverRepository,
    ShareRepository, UserRepository,
};
use rustshare_server::handlers::health::{ComponentHealth, evaluate_readiness, readiness_check};
use rustshare_server::state::AppState;

use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use sqlx::PgPool;
use tokio::sync::Mutex;
use uuid::Uuid;

async fn setup_test_env() -> AppState {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let metadata_store = Arc::new(MetadataStore::new(pool.clone()));
    let event_store = Arc::new(EventStore::new(pool.clone()));
    let broadcaster = Arc::new(EventBroadcaster::new(100));

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

    let jwt_manager = Arc::new(rustshare_auth::JwtManager::new(
        "test_secret_key_at_least_32_chars_long_for_security".to_string(),
    ));

    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(pool.clone()),
    )));

    let file_service = Arc::new(FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        broadcaster.clone(),
        permission_resolver.clone(),
    ));

    let folder_service = Arc::new(FolderService::new(
        event_store.clone(),
        metadata_store.clone(),
        broadcaster.clone(),
        permission_resolver.clone(),
    ));

    let share_notification_repo =
        Arc::new(rustshare_storage::repos::ShareNotificationRepoImpl::new(pool.clone()));

    let share_service = Arc::new(ShareService::new(
        event_store.clone(),
        metadata_store.clone(),
        broadcaster.clone(),
        jwt_manager.clone(),
        share_notification_repo.clone(),
    ));

    let thumbnail_service = Arc::new(ThumbnailService::new(pool.clone(), object_store.clone()));
    let notification_service = Arc::new(NotificationService::new(NotificationRepository::new(
        pool.clone(),
    )));

    let user_repository = Arc::new(UserRepository::new(pool.clone()));
    let file_repository = Arc::new(FileRepository::new(pool.clone()));
    let folder_repository = Arc::new(FolderRepository::new(pool.clone()));
    let share_repository = Arc::new(ShareRepository::new(pool.clone()));

    #[allow(deprecated)]
    let user_share_service = Arc::new(
        rustshare_core::services::UserShareService::new(
            rustshare_core::services::UserShareServiceDeps {
                share_repo: share_repository.clone(),
                user_repo: user_repository.clone(),
                file_repo: file_repository.clone(),
                folder_repo: folder_repository.clone(),
                permission_resolver: permission_resolver.clone(),
                notification_service: notification_service.clone(),
                event_store: event_store.clone(),
                broadcaster: broadcaster.clone(),
            },
        ),
    );

    let note_service = Arc::new(rustshare_server::services::note_service::NoteService::new(
        file_service.clone(),
        folder_service.clone(),
        metadata_store.clone(),
        object_store.clone(),
    ));

    let decision_service =
        Arc::new(rustshare_server::services::decision_service::DecisionService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ));

    let meeting_service =
        Arc::new(rustshare_server::services::meeting_service::MeetingService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ));

    let standup_service =
        Arc::new(rustshare_server::services::standup_service::StandupService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ));

    let module_service = Arc::new(rustshare_server::services::module_service::ModuleService::new(
        folder_service.clone(),
        metadata_store.clone(),
    ));

    let template_service =
        Arc::new(rustshare_server::services::template_service::TemplateService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
        ));

    let kanban_service = Arc::new(rustshare_server::services::kanban_service::KanbanService::new(
        file_service.clone(),
        folder_service.clone(),
        metadata_store.clone(),
        object_store.clone(),
        user_repository.clone(),
    ));

    let brainstorming_service = Arc::new(
        rustshare_server::services::brainstorming_service::BrainstormingService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ),
    );

    let secret_key = rustshare_crypto::SecretEncryptionKey::from_bytes([0u8; 32]);

    AppState {
        db_pool: pool,
        metadata_store,
        event_store,
        object_store,
        jwt_manager,
        broadcaster,
        file_service,
        folder_service,
        share_service,
        thumbnail_service,
        permission_resolver,
        notification_service,
        user_share_service,
        ai_service: None,
        upload_service: None,
        rate_limit_config: Arc::new(rustshare_server::middleware::RateLimitConfig::new()),
        secret_key,
        oidc_runtime_cache: rustshare_server::oidc_runtime::OidcRuntimeCache::new(),
        poll_rate_limiter: Arc::new(Mutex::new(HashMap::new())),
        default_tenant_id: Uuid::nil(),
        note_service,
        decision_service,
        meeting_service,
        standup_service,
        module_service,
        template_service,
        kanban_service,
        brainstorming_service,
        user_repository,
        public_base_url: "http://localhost:8080".to_string(),
        collab_rooms: Arc::new(rustshare_server::handlers::collab::CollabRooms::new()),
    }
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_readiness_healthy_with_real_deps() {
    let state = setup_test_env().await;

    let (status, json) = readiness_check(State(state)).await;
    let response = json.0;

    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(response.status, "ready");
    assert_eq!(
        response.components.get("database").unwrap().status,
        "healthy"
    );
    assert_eq!(
        response.components.get("object_storage").unwrap().status,
        "healthy"
    );
    assert_eq!(
        response.components.get("event_delivery").unwrap().status,
        "healthy"
    );
    assert_eq!(
        response.components.get("auth_session").unwrap().status,
        "healthy"
    );
    assert_eq!(
        response.components.get("ai").unwrap().status,
        "disabled"
    );
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_readiness_disabled_ai_does_not_fail() {
    let state = setup_test_env().await;

    let (status, json) = readiness_check(State(state)).await;
    let response = json.0;

    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(response.status, "ready");
    assert_eq!(
        response.components.get("ai").unwrap().status,
        "disabled"
    );
}

// ---------------------------------------------------------------------------
// Pure logic tests (no external infrastructure required)
// ---------------------------------------------------------------------------

#[test]
fn test_healthy_readiness_includes_all_required_checks() {
    let mut components = HashMap::new();
    components.insert("database".to_string(), ComponentHealth { status: "healthy".to_string(), error: None });
    components.insert("object_storage".to_string(), ComponentHealth { status: "healthy".to_string(), error: None });
    components.insert("event_delivery".to_string(), ComponentHealth { status: "healthy".to_string(), error: None });
    components.insert("auth_session".to_string(), ComponentHealth { status: "healthy".to_string(), error: None });
    components.insert("ai".to_string(), ComponentHealth { status: "disabled".to_string(), error: None });

    let (status_code, response) = evaluate_readiness(components);
    assert_eq!(status_code, StatusCode::OK);
    assert_eq!(response.status, "ready");
    assert!(response.components.contains_key("database"));
    assert!(response.components.contains_key("object_storage"));
    assert!(response.components.contains_key("event_delivery"));
    assert!(response.components.contains_key("auth_session"));
    assert!(response.components.contains_key("ai"));
}

#[test]
fn test_disabled_ai_does_not_fail_readiness() {
    let mut components = HashMap::new();
    components.insert("database".to_string(), ComponentHealth { status: "healthy".to_string(), error: None });
    components.insert("object_storage".to_string(), ComponentHealth { status: "healthy".to_string(), error: None });
    components.insert("event_delivery".to_string(), ComponentHealth { status: "healthy".to_string(), error: None });
    components.insert("auth_session".to_string(), ComponentHealth { status: "healthy".to_string(), error: None });
    components.insert("ai".to_string(), ComponentHealth { status: "disabled".to_string(), error: None });

    let (status_code, response) = evaluate_readiness(components);
    assert_eq!(status_code, StatusCode::OK);
    assert_eq!(response.status, "ready", "disabled AI should not make readiness fail");
}

#[test]
fn test_simulated_dependency_failure_returns_not_ready() {
    let mut components = HashMap::new();
    components.insert("database".to_string(), ComponentHealth { status: "healthy".to_string(), error: None });
    components.insert(
        "object_storage".to_string(),
        ComponentHealth { status: "unhealthy".to_string(), error: Some("connection refused".to_string()) },
    );
    components.insert("event_delivery".to_string(), ComponentHealth { status: "healthy".to_string(), error: None });
    components.insert("auth_session".to_string(), ComponentHealth { status: "healthy".to_string(), error: None });
    components.insert("ai".to_string(), ComponentHealth { status: "disabled".to_string(), error: None });

    let (status_code, response) = evaluate_readiness(components);
    assert_eq!(status_code, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.status, "not_ready");
    assert_eq!(
        response.components.get("object_storage").unwrap().status,
        "unhealthy"
    );
}
