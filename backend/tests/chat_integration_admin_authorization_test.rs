//! Integration test: chat integration admin handlers require `AdminUser` (Task A5).
//!
//! The compile-time test verifies that the admin chat-integration handlers are
//! typed to accept the `AdminUser` extractor. The runtime tests below hit the
//! HTTP routes anonymously and as a non-admin user and assert 401/403.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use axum::{body::Body, extract::State, http::Request, Json, Router};
use nostr::{EventBuilder, Keys, Kind, Tag};
use rustshare_core::events::EventBroadcaster;
use rustshare_core::services::{
    ChatIntegrationService, FileService, FolderService, HttpWebhookDispatcher, NotificationService,
    PermissionResolver, ShareService, ThumbnailService,
};
use rustshare_infrastructure::repositories::{
    FileRepository, FolderRepository, NotificationRepository, PermissionResolverRepository,
    ShareRepository, UserRepository,
};
use rustshare_resource_auth::BindingChallenge;
use rustshare_server::{
    config::ChatProvisioningMode,
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
        .expect("Failed to create object store")
        .with_blob_lock_pool(pool.clone()),
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

    let application_service = Arc::new(
        rustshare_server::services::application_service::ApplicationService::new(
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

    let mail_service = Arc::new(rustshare_server::services::mail_service::MailService::new(
        metadata_store.clone(),
        object_store.clone(),
        file_service.clone(),
        folder_service.clone(),
        permission_resolver.clone(),
        event_store.clone(),
        broadcaster.clone(),
        Arc::new(secret_key.clone()),
    ));

    let calendar_service = Arc::new(
        rustshare_server::services::calendar_service::CalendarService::new(
            metadata_store.clone(),
            Arc::new(secret_key.clone()),
        ),
    );

    let outbox_store = Arc::new(rustshare_storage::OutboxStore::new(
        pool.clone(),
        Arc::new(rustshare_core::domain::ApplicationRegistry::first_party().unwrap()),
    ));
    let chat_observation_store =
        Arc::new(rustshare_storage::ChatObservationStore::new(pool.clone()));
    let memory_catalog_store = Arc::new(rustshare_storage::MemoryCatalogStore::new(pool.clone()));
    let buzz_observation_service = Arc::new(
        rustshare_server::buzz_observation::BuzzObservationService::new(
            pool.clone(),
            rustshare_storage::ChatIdentityStore::new(pool.clone()),
            (*chat_observation_store).clone(),
            outbox_store.clone(),
            rustshare_crypto::WebhookSigner::new("test-secret"),
            300,
            Arc::new(rustshare_core::events::EventBroadcaster::new(64)),
        ),
    );

    let unified_search_service = Arc::new(
        rustshare_server::services::unified_search::UnifiedSearchService::new(
            Arc::new(rustshare_resource_auth::SourceAuthorizer::empty()),
            metadata_store.clone(),
            None,
            memory_catalog_store.clone(),
        ),
    );

    let chat_owner = Arc::new(rustshare_server::authz::ChatResourceOwner::new(
        rustshare_storage::ChatIdentityStore::new(pool.clone()),
        (*chat_observation_store).clone(),
    ));

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
        source_authorizer: Arc::new(rustshare_resource_auth::SourceAuthorizer::empty()),
        notification_service,
        user_share_service,
        ai_service: None,
        upload_service: None,
        rate_limit_config: Arc::new(rustshare_server::middleware::RateLimitConfig::new()),
        secret_key,
        oidc_runtime_cache: rustshare_server::oidc_runtime::OidcRuntimeCache::new(),
        poll_rate_limiter: Arc::new(Mutex::new(HashMap::new())),
        default_tenant_id: Uuid::nil(),
        default_tenant_config: rustshare_server::config::DefaultTenantConfig::Unset,
        note_service,
        decision_service,
        meeting_service,
        standup_service,
        application_service,
        template_service,
        kanban_service,
        brainstorming_service,
        vault_sync_service,
        chat_integration_service,
        mail_service,
        calendar_service,
        outbox_store,
        chat_observation_store,
        memory_catalog_store,
        unified_search_service: unified_search_service.clone(),
        ask_workspace_service: Arc::new(
            rustshare_server::services::ask_workspace::AskWorkspaceService::new(
                unified_search_service.clone(),
                None,
            ),
        ),
        buzz_observation_service,
        chat_owner,
        buzz_gateway: None,
        chat_bootstrap: None,
        chat_provisioning: ChatProvisioningMode::Manual,
        user_repository,
        public_base_url: "http://localhost:8080".to_string(),
        collab_rooms: Arc::new(rustshare_server::handlers::collab::CollabRooms::new()),
        outbox_status: Arc::new(rustshare_server::outbox_dispatcher::OutboxStatus::default()),
        outbox_worker_enabled: false,
        outbox_readiness_staleness_secs: 60,
        shutdown_tx: tokio::sync::broadcast::channel(1).0,
        memory_files_reconcile_worker: None,
        memory_orphan_cleanup_worker: None,
        memory_orphan_cleanup_dry_run: true,
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

#[tokio::test]
#[ignore = "requires migrated disposable PostgreSQL and S3-compatible object store"]
async fn replacement_initiation_http_gate_leaves_challenge_and_old_binding_untouched() {
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref(),
        Ok("1")
    );
    let pool = test_pool().await;
    let state = setup_app_state(pool.clone()).await;
    let router = chat_admin_router().with_state(state.clone());
    let principal = create_test_user(
        &pool,
        &format!("replacement_{}", Uuid::new_v4().simple()),
        &format!("{}@test.local", Uuid::new_v4()),
        false,
    )
    .await;
    let old_binding_id = Uuid::new_v4();
    let old_keys = Keys::generate();
    let candidate_keys = Keys::generate();
    let challenge = BindingChallenge::issue(
        rustshare_core::domain::TenantId(Uuid::nil()),
        rustshare_core::domain::PrincipalId(principal),
        candidate_keys.public_key().to_hex(),
        "wss://relay.example.test",
        Some(old_binding_id),
        chrono::Utc::now(),
        chrono::Duration::minutes(5),
    );
    sqlx::query("INSERT INTO chat_identity_bindings(binding_id,tenant_id,principal_id,buzz_pubkey,status) VALUES($1,$2,$3,$4,'active')")
        .bind(old_binding_id)
        .bind(Uuid::nil())
        .bind(principal)
        .bind(old_keys.public_key().to_hex())
        .execute(&pool)
        .await
        .unwrap();
    rustshare_storage::ChatIdentityStore::new(pool.clone())
        .insert_challenge(&challenge)
        .await
        .unwrap();
    let event = EventBuilder::new(Kind::Authentication, "")
        .tags([
            Tag::parse(["relay", "wss://relay.example.test"]).unwrap(),
            Tag::parse(["challenge", challenge.nonce.as_str()]).unwrap(),
        ])
        .sign_with_keys(&candidate_keys)
        .unwrap();
    let body = serde_json::json!({
        "workspace_id": Uuid::nil(),
        "challenge_id": challenge.challenge_id,
        "old_binding_id": old_binding_id,
        "event": event,
        "operation_id": Uuid::new_v4(),
    });
    let token = bearer_token(&state, principal);
    let response = router
        .oneshot(
            Request::post("/api/v1/applications/chat/identity-replacements")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
    let untouched: (bool, String, i64) = sqlx::query_as(
        "SELECT c.consumed_at IS NULL,b.status,
            (SELECT count(*) FROM chat_identity_replacements WHERE principal_id=$2)
         FROM chat_binding_challenges c JOIN chat_identity_bindings b ON b.binding_id=$3
         WHERE c.challenge_id=$1",
    )
    .bind(challenge.challenge_id)
    .bind(principal)
    .bind(old_binding_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(untouched, (true, "active".into(), 0));
    sqlx::query("DELETE FROM chat_binding_challenges WHERE challenge_id=$1")
        .bind(challenge.challenge_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM chat_identity_bindings WHERE binding_id=$1")
        .bind(old_binding_id)
        .execute(&pool)
        .await
        .unwrap();
    cleanup_users(&pool, &[principal]).await;
}
