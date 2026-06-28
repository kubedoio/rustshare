//! Integration test: chat integration admin handlers require `AdminUser` (Task A5).
//!
//! The compile-time test verifies that the admin chat-integration handlers are
//! typed to accept the `AdminUser` extractor. The runtime tests below hit the
//! HTTP routes anonymously and as a non-admin user and assert 401/403.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use axum::{body::Body, extract::State, http::Request, Json, Router};
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{
    ChatIntegrationService, FileService, FolderService, HttpWebhookDispatcher, NotificationService,
    PermissionResolver, ShareService, ThumbnailService,
};
use rustshare_infrastructure::repositories::{
    FileRepository, FolderRepository, NotificationRepository, PermissionResolverRepository,
    ShareRepository, UserRepository,
};
use rustshare_server::{
    handlers::{
        chat_integration::{list_chat_webhooks, register_chat_webhook, RegisterWebhookRequest},
        extractors::AdminUser,
    },
    AppState,
};
use rustshare_storage::{repos::ShareNotificationRepoImpl, EventStore, MetadataStore, ObjectStore};
use sqlx::PgPool;
use tokio::sync::Mutex;
use tower::ServiceExt;
use uuid::Uuid;

#[test]
fn chat_integration_admin_authorization() {
    fn assert_list_requires_admin<H, Fut>(_handler: H)
    where
        H: Fn(State<AppState>, AdminUser) -> Fut,
        Fut: Future,
    {
    }
    assert_list_requires_admin(list_chat_webhooks);

    fn assert_register_requires_admin<H, Fut>(_handler: H)
    where
        H: Fn(State<AppState>, AdminUser, Json<RegisterWebhookRequest>) -> Fut,
        Fut: Future,
    {
    }
    assert_register_requires_admin(register_chat_webhook);
}

// ---------------------------------------------------------------------------
// Runtime HTTP tests (require database)
// ---------------------------------------------------------------------------

async fn test_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    PgPool::connect(&url).await.expect("DB connect failed")
}

async fn setup_app_state(pool: PgPool) -> AppState {
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

    let jwt_manager = Arc::new(rustshare_auth::JwtManager::new(
        "test_secret_key_at_least_32_chars_long_for_security".to_string(),
        "rustshare",
        "rustshare-api",
        24,
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

    let share_notification_repo = Arc::new(ShareNotificationRepoImpl::new(pool.clone()));

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
    let user_share_service = Arc::new(rustshare_core::services::UserShareService::new(
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
    ));

    let note_service = Arc::new(rustshare_server::services::note_service::NoteService::new(
        file_service.clone(),
        folder_service.clone(),
        metadata_store.clone(),
        object_store.clone(),
        permission_resolver.clone(),
        pool.clone(),
    ));

    let decision_service = Arc::new(
        rustshare_server::services::decision_service::DecisionService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ),
    );

    let meeting_service = Arc::new(
        rustshare_server::services::meeting_service::MeetingService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ),
    );

    let standup_service = Arc::new(
        rustshare_server::services::standup_service::StandupService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ),
    );

    let module_service = Arc::new(
        rustshare_server::services::module_service::ModuleService::new(
            folder_service.clone(),
            metadata_store.clone(),
        ),
    );

    let template_service = Arc::new(
        rustshare_server::services::template_service::TemplateService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
        ),
    );

    let kanban_service = Arc::new(
        rustshare_server::services::kanban_service::KanbanService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
            user_repository.clone(),
        ),
    );

    let brainstorming_service = Arc::new(
        rustshare_server::services::brainstorming_service::BrainstormingService::new(
            file_service.clone(),
            folder_service.clone(),
            metadata_store.clone(),
            object_store.clone(),
        ),
    );

    let vault_sync_service = Arc::new(rustshare_core::services::VaultSyncService::new(
        metadata_store.clone(),
        object_store.clone(),
    ));

    let chat_integration_service = Arc::new(ChatIntegrationService::new(
        metadata_store.clone(),
        event_store.clone(),
        broadcaster.clone(),
        "test-secret",
        Arc::new(HttpWebhookDispatcher::new()),
    ));

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
        vault_sync_service,
        chat_integration_service,
        user_repository,
        public_base_url: "http://localhost:8080".to_string(),
        collab_rooms: Arc::new(rustshare_server::handlers::collab::CollabRooms::new()),
        shutdown_tx: tokio::sync::broadcast::channel(1).0,
        prometheus_handle: rustshare_server::metrics::init_metrics(),
    }
}

fn chat_admin_router() -> Router<AppState> {
    rustshare_server::routes::chat_integration_routes()
}

async fn create_test_user(pool: &PgPool, username: &str, email: &str, is_admin: bool) -> Uuid {
    let id = Uuid::new_v4();
    let tenant_id = Uuid::nil();

    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota, tenant_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(username)
    .bind(email)
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("Test {}", username))
    .bind(is_admin)
    .bind(10_737_418_240i64)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("insert user");

    id
}

async fn cleanup_users(pool: &PgPool, ids: &[Uuid]) {
    for id in ids {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await
            .ok();
    }
}

fn bearer_token(state: &AppState, user_id: Uuid) -> String {
    state
        .jwt_manager
        .generate(user_id, "test@example.com", Uuid::nil())
        .expect("generate token")
}

#[tokio::test]
#[ignore] // Requires database and S3-compatible object store
async fn admin_chat_webhook_endpoints_require_authentication() {
    let pool = test_pool().await;
    let state = setup_app_state(pool.clone()).await;
    let router = chat_admin_router().with_state(state.clone());

    let list_response = router
        .clone()
        .oneshot(
            Request::get("/api/v1/admin/integrations/chat/webhooks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        list_response.status(),
        401,
        "anonymous list must be rejected with 401"
    );

    let register_response = router
        .oneshot(
            Request::post("/api/v1/admin/integrations/chat/webhooks")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url":"https://example.com/webhook"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        register_response.status(),
        401,
        "anonymous register must be rejected with 401"
    );

    cleanup_users(&pool, &[]).await;
}

#[tokio::test]
#[ignore] // Requires database and S3-compatible object store
async fn admin_chat_webhook_endpoints_require_admin_role() {
    let pool = test_pool().await;
    let state = setup_app_state(pool.clone()).await;
    let router = chat_admin_router().with_state(state.clone());

    let non_admin_id =
        create_test_user(&pool, "non_admin_user", "nonadmin@test.local", false).await;
    let token = bearer_token(&state, non_admin_id);

    let list_response = router
        .clone()
        .oneshot(
            Request::get("/api/v1/admin/integrations/chat/webhooks")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        list_response.status(),
        403,
        "non-admin list must be rejected with 403"
    );

    let register_response = router
        .oneshot(
            Request::post("/api/v1/admin/integrations/chat/webhooks")
                .header("authorization", format!("Bearer {}", token))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url":"https://example.com/webhook"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        register_response.status(),
        403,
        "non-admin register must be rejected with 403"
    );

    cleanup_users(&pool, &[non_admin_id]).await;
}

#[tokio::test]
#[ignore] // Requires database and S3-compatible object store
async fn admin_chat_webhook_endpoints_allow_admin() {
    let pool = test_pool().await;
    let state = setup_app_state(pool.clone()).await;
    let router = chat_admin_router().with_state(state.clone());

    let admin_id = create_test_user(&pool, "admin_user", "admin@test.local", true).await;
    let token = bearer_token(&state, admin_id);

    let list_response = router
        .clone()
        .oneshot(
            Request::get("/api/v1/admin/integrations/chat/webhooks")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        list_response.status(),
        200,
        "admin list must succeed with 200"
    );

    let register_response = router
        .oneshot(
            Request::post("/api/v1/admin/integrations/chat/webhooks")
                .header("authorization", format!("Bearer {}", token))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"url":"https://example.com/webhook"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        register_response.status(),
        201,
        "admin register must succeed with 201"
    );

    cleanup_users(&pool, &[admin_id]).await;
}
