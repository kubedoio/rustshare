//! Integration tests for mail message read endpoints.
//!
//! Run with: cargo test --test mail_read_test -- --ignored

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rustshare_server::middleware;
use rustshare_server::routes;
use rustshare_server::state::AppState;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceExt;
use uuid::Uuid;

async fn setup_test_env() -> AppState {
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string())
        .replace("@postgres:", "@localhost:");

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let metadata_store = Arc::new(rustshare_storage::MetadataStore::new(pool.clone()));
    let event_store = Arc::new(rustshare_storage::EventStore::new(pool.clone()));
    let broadcaster = Arc::new(rustshare_core::events::EventBroadcaster::new(100));

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
        rustshare_storage::ObjectStore::new_with_options(
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

    let permission_resolver =
        Arc::new(rustshare_core::services::PermissionResolver::new(Arc::new(
            rustshare_infrastructure::repositories::PermissionResolverRepository::new(pool.clone()),
        )));

    let file_service = Arc::new(rustshare_core::services::FileService::new(
        event_store.clone(),
        metadata_store.clone(),
        object_store.clone(),
        broadcaster.clone(),
        permission_resolver.clone(),
    ));

    let folder_service = Arc::new(rustshare_core::services::FolderService::new(
        event_store.clone(),
        metadata_store.clone(),
        broadcaster.clone(),
        permission_resolver.clone(),
    ));

    let share_notification_repo = Arc::new(
        rustshare_storage::repos::ShareNotificationRepoImpl::new(pool.clone()),
    );

    let share_service = Arc::new(rustshare_core::services::ShareService::new(
        event_store.clone(),
        metadata_store.clone(),
        broadcaster.clone(),
        jwt_manager.clone(),
        share_notification_repo.clone(),
    ));

    let thumbnail_service = Arc::new(rustshare_core::services::ThumbnailService::new(
        pool.clone(),
        object_store.clone(),
    ));
    let notification_service = Arc::new(rustshare_core::services::NotificationService::new(
        rustshare_infrastructure::repositories::NotificationRepository::new(pool.clone()),
    ));

    let user_repository = Arc::new(rustshare_infrastructure::repositories::UserRepository::new(
        pool.clone(),
    ));
    let file_repository = Arc::new(rustshare_infrastructure::repositories::FileRepository::new(
        pool.clone(),
    ));
    let folder_repository =
        Arc::new(rustshare_infrastructure::repositories::FolderRepository::new(pool.clone()));
    let share_repository =
        Arc::new(rustshare_infrastructure::repositories::ShareRepository::new(pool.clone()));

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

    let chat_integration_service = Arc::new(rustshare_core::services::ChatIntegrationService::new(
        metadata_store.clone(),
        event_store.clone(),
        broadcaster.clone(),
        "test-secret",
        Arc::new(rustshare_core::services::HttpWebhookDispatcher::new()),
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
        user_repository,
        public_base_url: "http://localhost:8080".to_string(),
        collab_rooms: Arc::new(rustshare_server::handlers::collab::CollabRooms::new()),
        outbox_status: Arc::new(rustshare_server::outbox_dispatcher::OutboxStatus::default()),
        outbox_worker_enabled: false,
        outbox_readiness_staleness_secs: 60,
        shutdown_tx: tokio::sync::broadcast::channel(1).0,
        prometheus_handle: rustshare_server::metrics::init_metrics(),
    }
}

async fn create_test_tenant(pool: &PgPool) -> Uuid {
    let tenant_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, updated_at) VALUES ($1, $2, NOW(), NOW()) ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(format!("Test Tenant {}", tenant_id))
    .execute(pool)
    .await
    .expect("Failed to create test tenant");
    tenant_id
}

async fn create_test_user(
    state: &AppState,
    username: &str,
    tenant_id: Uuid,
) -> rustshare_core::domain::User {
    let unique_username = format!("{}-{}", username, Uuid::new_v4());
    let user = rustshare_core::domain::User::new(
        unique_username.clone(),
        format!("{} Display", unique_username),
        "test_password_hash".to_string(),
        format!("{}@test.local", unique_username),
        false,
        10_737_418_240,
        tenant_id,
    );
    state
        .metadata_store
        .create_user(&user)
        .await
        .expect("Failed to create test user");
    user
}

fn create_auth_token(state: &AppState, user_id: Uuid, tenant_id: Uuid) -> String {
    state
        .jwt_manager
        .generate(user_id, "test@example.com", tenant_id)
        .unwrap()
}

async fn enable_mail_module(state: &AppState, tenant_id: Uuid, user_id: Uuid) {
    state
        .application_service
        .ensure_default_applications(tenant_id)
        .await
        .expect("ensure_default_applications should succeed");
    state
        .application_service
        .enable_application("io.elembra.mail", user_id, tenant_id)
        .await
        .expect("enable mail module should succeed");
}

fn build_app(state: AppState) -> axum::Router {
    routes::mail_routes()
        .with_state(state)
        .layer(axum::middleware::from_fn(
            middleware::security_headers_middleware,
        ))
}

async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
    sqlx::query("DELETE FROM files WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("failed to clean up files");
    sqlx::query("DELETE FROM folders WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("failed to clean up folders");
    sqlx::query("DELETE FROM mail_message_parts WHERE message_id IN (SELECT id FROM mail_messages WHERE owner_id = $1)")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("failed to clean up mail message parts");
    sqlx::query("DELETE FROM mail_attachments WHERE message_id IN (SELECT id FROM mail_messages WHERE owner_id = $1)")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("failed to clean up mail attachments");
    sqlx::query("DELETE FROM mail_messages WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("failed to clean up mail messages");
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("failed to clean up user");
}

async fn cleanup_tenant(pool: &PgPool, tenant_id: Uuid) {
    sqlx::query("DELETE FROM application_enablements WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("failed to clean up modules");
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("failed to clean up tenant");
}

fn read_fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/core/tests/fixtures/eml")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|_| panic!("failed to read fixture {}", name))
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn read_message_parts_source_and_attachments() {
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "mail_read", tenant_id).await;
    enable_mail_module(&state, tenant_id, user.id).await;
    let token = create_auth_token(&state, user.id, tenant_id);

    let raw = read_fixture("with_attachment.eml");
    let message = state
        .mail_service
        .import_eml(tenant_id, user.id, user.id, raw.clone())
        .await
        .expect("import_eml should succeed");

    let app = build_app(state.clone());

    // List parts
    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages/{}/parts", message.id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parts_resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let parts = parts_resp["parts"].as_array().expect("parts array");
    assert!(!parts.is_empty(), "message should have at least one part");

    // Source download
    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages/{}/source", message.id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .expect("content-type header")
        .to_str()
        .unwrap();
    assert_eq!(content_type, "message/rfc822");
    let content_disposition = response
        .headers()
        .get("content-disposition")
        .expect("content-disposition header")
        .to_str()
        .unwrap();
    assert!(
        content_disposition.starts_with("attachment"),
        "content-disposition should be attachment"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body.to_vec(), raw);

    // Attachments
    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages/{}/attachments", message.id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let att_resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let attachments = att_resp["attachments"]
        .as_array()
        .expect("attachments array");
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0]["filename"], "note.txt");

    cleanup_user(&state.db_pool, user.id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn sanitized_html_part_strips_scripts() {
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "mail_sanitize", tenant_id).await;
    enable_mail_module(&state, tenant_id, user.id).await;
    let token = create_auth_token(&state, user.id, tenant_id);

    let mut raw = read_fixture("simple_html.eml");
    // Inject a script tag so we can verify server-side sanitization.
    let html = String::from_utf8_lossy(&raw);
    let injected = html.replace(
        "</body></html>",
        "<script>alert('xss')</script></body></html>",
    );
    raw = injected.into_bytes();

    let message = state
        .mail_service
        .import_eml(tenant_id, user.id, user.id, raw)
        .await
        .expect("import_eml should succeed");

    let app = build_app(state.clone());

    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages/{}/parts", message.id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parts_resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let html_part = parts_resp["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["content_type"] == "text/html")
        .expect("HTML part should exist")
        .clone();

    let request = Request::builder()
        .uri(format!(
            "/api/v1/mail/messages/{}/parts/{}",
            message.id,
            html_part["id"].as_str().unwrap()
        ))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(
        !text.contains("<script>"),
        "sanitized HTML should not contain <script>"
    );
    assert!(text.contains("<p>This is an HTML email.</p>"));

    cleanup_user(&state.db_pool, user.id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn cross_tenant_message_access_rejected() {
    let state = setup_test_env().await;

    let tenant_a = create_test_tenant(&state.db_pool).await;
    let user_a = create_test_user(&state, "mail_owner_a", tenant_a).await;
    enable_mail_module(&state, tenant_a, user_a.id).await;

    let tenant_b = create_test_tenant(&state.db_pool).await;
    let user_b = create_test_user(&state, "mail_intruder_b", tenant_b).await;
    enable_mail_module(&state, tenant_b, user_b.id).await;

    let raw = read_fixture("simple_plain.eml");
    let message = state
        .mail_service
        .import_eml(tenant_a, user_a.id, user_a.id, raw)
        .await
        .expect("import_eml should succeed");

    let token_b = create_auth_token(&state, user_b.id, tenant_b);
    let app = build_app(state.clone());

    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages/{}/parts", message.id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token_b))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_user(&state.db_pool, user_a.id).await;
    cleanup_user(&state.db_pool, user_b.id).await;
    cleanup_tenant(&state.db_pool, tenant_a).await;
    cleanup_tenant(&state.db_pool, tenant_b).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn unknown_message_and_part_return_404() {
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "mail_404", tenant_id).await;
    enable_mail_module(&state, tenant_id, user.id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let unknown_message_id = Uuid::new_v4();

    // Unknown message: list parts
    let request = Request::builder()
        .uri(format!(
            "/api/v1/mail/messages/{}/parts",
            unknown_message_id
        ))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Unknown message: source
    let request = Request::builder()
        .uri(format!(
            "/api/v1/mail/messages/{}/source",
            unknown_message_id
        ))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Known message but unknown part
    let raw = read_fixture("simple_plain.eml");
    let message = state
        .mail_service
        .import_eml(tenant_id, user.id, user.id, raw)
        .await
        .expect("import_eml should succeed");

    let unknown_part_id = Uuid::new_v4();
    let request = Request::builder()
        .uri(format!(
            "/api/v1/mail/messages/{}/parts/{}",
            message.id, unknown_part_id
        ))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_user(&state.db_pool, user.id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn mail_module_disabled_returns_403() {
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "mail_disabled", tenant_id).await;
    // Do not enable the mail module.
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let message_id = Uuid::new_v4();

    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages/{}/parts", message_id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages/{}/source", message_id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_user(&state.db_pool, user.id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn reading_part_and_source_appends_viewed_event() {
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "mail_viewed", tenant_id).await;
    enable_mail_module(&state, tenant_id, user.id).await;
    let token = create_auth_token(&state, user.id, tenant_id);

    let raw = read_fixture("simple_plain.eml");
    let message = state
        .mail_service
        .import_eml(tenant_id, user.id, user.id, raw)
        .await
        .expect("import_eml should succeed");

    let app = build_app(state.clone());

    // Fetch parts to obtain a part id.
    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages/{}/parts", message.id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parts_resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let parts = parts_resp["parts"].as_array().expect("parts array");
    let first_part = parts
        .first()
        .expect("message should have at least one part");

    // Read the body part.
    let request = Request::builder()
        .uri(format!(
            "/api/v1/mail/messages/{}/parts/{}",
            message.id,
            first_part["id"].as_str().unwrap()
        ))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // The event store serializes AggregateType and EventType as JSON strings,
    // so bind the serde_json-encoded values to match the stored representation.
    let aggregate_type_str =
        serde_json::to_string(&rustshare_core::events::AggregateType::MailMessage).unwrap();
    let event_type_str =
        serde_json::to_string(&rustshare_core::events::EventType::MailMessageViewed).unwrap();

    // Assert MailMessageViewed event with view_type "body".
    let body_events = sqlx::query(
        "SELECT payload FROM events
         WHERE aggregate_id = $1
           AND aggregate_type = $2
           AND event_type = $3",
    )
    .bind(message.id)
    .bind(&aggregate_type_str)
    .bind(&event_type_str)
    .fetch_all(&state.db_pool)
    .await
    .expect("query events");
    let body_viewed = body_events.iter().any(|row| {
        let payload: serde_json::Value = row.try_get("payload").unwrap();
        payload.get("view_type").and_then(|v| v.as_str()) == Some("body")
    });
    assert!(
        body_viewed,
        "body view should append a MailMessageViewed event"
    );

    // Read the source.
    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages/{}/source", message.id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Assert MailMessageViewed event with view_type "source".
    let all_events = sqlx::query(
        "SELECT payload FROM events
         WHERE aggregate_id = $1
           AND aggregate_type = $2
           AND event_type = $3",
    )
    .bind(message.id)
    .bind(&aggregate_type_str)
    .bind(&event_type_str)
    .fetch_all(&state.db_pool)
    .await
    .expect("query events");
    assert_eq!(
        all_events.len(),
        2,
        "should have one body and one source MailMessageViewed event"
    );
    let source_viewed = all_events.iter().any(|row| {
        let payload: serde_json::Value = row.try_get("payload").unwrap();
        payload.get("view_type").and_then(|v| v.as_str()) == Some("source")
    });
    assert!(
        source_viewed,
        "source view should append a MailMessageViewed event"
    );

    cleanup_user(&state.db_pool, user.id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

// ---------------------------------------------------------------------------
// Deterministic date sorting (issue #182)
// ---------------------------------------------------------------------------

async fn insert_sorted_message(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
    subject: &str,
    sent_at: Option<chrono::DateTime<chrono::Utc>>,
    imported_at: chrono::DateTime<chrono::Utc>,
) -> Uuid {
    // eml_upload is excluded from the account-source unique index, so several
    // synthetic messages per user can coexist without fake source identities.
    let mut message = rustshare_core::domain::MailMessage::new(
        tenant_id,
        user_id,
        user_id,
        rustshare_core::domain::MailSourceMode::EmlUpload,
    );
    message.subject = Some(subject.to_string());
    message.sent_at = sent_at;
    message.imported_at = imported_at;
    state
        .metadata_store
        .create_mail_message(&message)
        .await
        .expect("insert mail message");
    message.id
}

async fn get_saved_messages(app: &axum::Router, token: &str, query: &str) -> serde_json::Value {
    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages?{query}"))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "query: {query}");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn message_ids(body: &serde_json::Value) -> Vec<String> {
    body["messages"]
        .as_array()
        .expect("messages array")
        .iter()
        .map(|message| message["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn saved_messages_sort_both_directions_with_null_sent_at_fallback() {
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "mail_sort", tenant_id).await;
    enable_mail_module(&state, tenant_id, user.id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let base = chrono::DateTime::parse_from_rfc3339("2026-07-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let day = chrono::Duration::days(1);

    // A: dated oldest. B: dated newest. C: no Date header, but imported last,
    // so it falls between A and B on the coalesced sort value.
    let a = insert_sorted_message(&state, tenant_id, user.id, "A", Some(base), base).await;
    let b = insert_sorted_message(
        &state,
        tenant_id,
        user.id,
        "B",
        Some(base + day * 10),
        base + day,
    )
    .await;
    let c = insert_sorted_message(&state, tenant_id, user.id, "C", None, base + day * 5).await;

    let desc = get_saved_messages(&app, &token, "").await;
    assert_eq!(
        message_ids(&desc),
        vec![b.to_string(), c.to_string(), a.to_string()],
        "default order should be newest coalesced date first"
    );

    let explicit_desc = get_saved_messages(&app, &token, "sort=date_desc").await;
    assert_eq!(message_ids(&explicit_desc), message_ids(&desc));

    let asc = get_saved_messages(&app, &token, "sort=date_asc").await;
    assert_eq!(
        message_ids(&asc),
        vec![a.to_string(), c.to_string(), b.to_string()],
        "ascending order should be oldest coalesced date first"
    );

    let request = Request::builder()
        .uri("/api/v1/mail/messages?sort=sideways")
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "unknown sort values must be rejected"
    );

    cleanup_user(&state.db_pool, user.id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn saved_messages_sort_equal_dates_use_id_tiebreak() {
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "mail_sort_tie", tenant_id).await;
    enable_mail_module(&state, tenant_id, user.id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let ts = chrono::DateTime::parse_from_rfc3339("2026-07-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let first = insert_sorted_message(&state, tenant_id, user.id, "Tie 1", Some(ts), ts).await;
    let second = insert_sorted_message(&state, tenant_id, user.id, "Tie 2", Some(ts), ts).await;

    // Postgres uuid ordering is plain byte ordering, matching Rust's Uuid Ord.
    let (low, high) = if first < second {
        (first, second)
    } else {
        (second, first)
    };

    let desc = get_saved_messages(&app, &token, "sort=date_desc").await;
    assert_eq!(
        message_ids(&desc),
        vec![high.to_string(), low.to_string()],
        "descending ties should break on id DESC"
    );

    let asc = get_saved_messages(&app, &token, "sort=date_asc").await;
    assert_eq!(
        message_ids(&asc),
        vec![low.to_string(), high.to_string()],
        "ascending ties should break on id ASC"
    );

    cleanup_user(&state.db_pool, user.id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn saved_messages_sort_paginates_in_both_directions() {
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "mail_sort_page", tenant_id).await;
    enable_mail_module(&state, tenant_id, user.id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let base = chrono::DateTime::parse_from_rfc3339("2026-07-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let day = chrono::Duration::days(1);
    let mut ids = Vec::new();
    for i in 0..3 {
        ids.push(
            insert_sorted_message(
                &state,
                tenant_id,
                user.id,
                &format!("Page {i}"),
                Some(base + day * i),
                base + day * i,
            )
            .await
            .to_string(),
        );
    }

    for (sort, expected) in [
        (
            "date_desc",
            vec![ids[2].clone(), ids[1].clone(), ids[0].clone()],
        ),
        ("date_asc", ids.clone()),
    ] {
        let page1 = get_saved_messages(&app, &token, &format!("sort={sort}&limit=2")).await;
        assert_eq!(message_ids(&page1), expected[..2], "first page for {sort}");
        let cursor_at = page1["next_cursor_at"]
            .as_str()
            .expect("next_cursor_at present");
        let cursor_id = page1["next_cursor_id"]
            .as_str()
            .expect("next_cursor_id present");

        let page2 = get_saved_messages(
            &app,
            &token,
            &format!("sort={sort}&limit=2&cursor_at={cursor_at}&cursor_id={cursor_id}"),
        )
        .await;
        assert_eq!(message_ids(&page2), expected[2..], "second page for {sort}");
        assert!(
            page2["next_cursor_at"].is_null(),
            "no third page expected for {sort}"
        );
    }

    cleanup_user(&state.db_pool, user.id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn saved_messages_search_combines_with_sort() {
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "mail_sort_search", tenant_id).await;
    enable_mail_module(&state, tenant_id, user.id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let app = build_app(state.clone());

    let base = chrono::DateTime::parse_from_rfc3339("2026-07-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let day = chrono::Duration::days(1);
    let older = insert_sorted_message(
        &state,
        tenant_id,
        user.id,
        "Quarterly report Q1",
        Some(base),
        base,
    )
    .await;
    let newer = insert_sorted_message(
        &state,
        tenant_id,
        user.id,
        "Quarterly report Q2",
        Some(base + day),
        base + day,
    )
    .await;
    insert_sorted_message(
        &state,
        tenant_id,
        user.id,
        "Unrelated",
        Some(base + day * 2),
        base + day * 2,
    )
    .await;

    let asc = get_saved_messages(&app, &token, "sort=date_asc&search=quarterly").await;
    assert_eq!(
        message_ids(&asc),
        vec![older.to_string(), newer.to_string()],
        "search results should honor ascending date order"
    );

    let desc = get_saved_messages(&app, &token, "sort=date_desc&search=quarterly").await;
    assert_eq!(
        message_ids(&desc),
        vec![newer.to_string(), older.to_string()],
        "search results should honor descending date order"
    );

    cleanup_user(&state.db_pool, user.id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}
