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
        mail_service,
        user_repository,
        public_base_url: "http://localhost:8080".to_string(),
        collab_rooms: Arc::new(rustshare_server::handlers::collab::CollabRooms::new()),
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
        .module_service
        .ensure_default_modules(tenant_id)
        .await
        .expect("ensure_default_modules should succeed");
    state
        .module_service
        .enable_module("mail", user_id, tenant_id)
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
    sqlx::query("DELETE FROM modules WHERE tenant_id = $1")
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

/// Multi-attachment EML: plain body plus three attachments — ASCII name,
/// RFC 2047 encoded-word Unicode name, and a zero-byte payload.
fn multi_attachment_eml() -> Vec<u8> {
    // "first bytes" and "pdf-bytes" base64-encoded; the second filename is the
    // RFC 2047 encoded-word for "我的報告.pdf".
    "From: sender@example.com\r\n\
     To: recipient@example.com\r\n\
     Subject: Multi attachments\r\n\
     Message-ID: <multi-attach@example.com>\r\n\
     Date: Mon, 06 Jul 2026 14:00:00 +0000\r\n\
     MIME-Version: 1.0\r\n\
     Content-Type: multipart/mixed; boundary=\"mixboundary\"\r\n\
     \r\n\
     --mixboundary\r\n\
     Content-Type: text/plain; charset=utf-8\r\n\
     \r\n\
     See attached files.\r\n\
     --mixboundary\r\n\
     Content-Type: text/plain; name=\"first.txt\"\r\n\
     Content-Disposition: attachment; filename=\"first.txt\"\r\n\
     Content-Transfer-Encoding: base64\r\n\
     \r\n\
     Zmlyc3QgYnl0ZXM=\r\n\
     --mixboundary\r\n\
     Content-Type: application/pdf; name=\"=?UTF-8?B?5oiR55qE5aCx5ZGKLnBkZg==?=\"\r\n\
     Content-Disposition: attachment; filename=\"=?UTF-8?B?5oiR55qE5aCx5ZGKLnBkZg==?=\"\r\n\
     Content-Transfer-Encoding: base64\r\n\
     \r\n\
     cGRmLWJ5dGVz\r\n\
     --mixboundary\r\n\
     Content-Type: application/octet-stream; name=\"empty.bin\"\r\n\
     Content-Disposition: attachment; filename=\"empty.bin\"\r\n\
     Content-Transfer-Encoding: base64\r\n\
     \r\n\
     \r\n\
     --mixboundary--\r\n"
        .as_bytes()
        .to_vec()
}

async fn list_attachment_ids(
    app: axum::Router,
    message_id: Uuid,
    token: &str,
) -> Vec<serde_json::Value> {
    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages/{message_id}/attachments"))
        .method("GET")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    parsed["attachments"]
        .as_array()
        .expect("attachments array")
        .clone()
}

async fn get_attachment(
    app: axum::Router,
    message_id: Uuid,
    attachment_id: &str,
    token: Option<&str>,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .uri(format!(
            "/api/v1/mail/messages/{message_id}/attachments/{attachment_id}"
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    if let Some(token) = token {
        builder
            .headers_mut()
            .insert("Authorization", format!("Bearer {token}").parse().unwrap());
    }
    app.oneshot(builder).await.unwrap()
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn download_imported_attachments_serves_exact_bytes_with_safe_headers() {
    let state = setup_test_env().await;
    let tenant_id = create_test_tenant(&state.db_pool).await;
    let user = create_test_user(&state, "mail_att_dl", tenant_id).await;
    enable_mail_module(&state, tenant_id, user.id).await;
    let token = create_auth_token(&state, user.id, tenant_id);

    let message = state
        .mail_service
        .import_eml(tenant_id, user.id, user.id, multi_attachment_eml())
        .await
        .expect("import_eml should succeed");

    let app = build_app(state.clone());
    let attachments = list_attachment_ids(app.clone(), message.id, &token).await;
    assert_eq!(attachments.len(), 3, "expected three attachments");

    // Each attachment is independently downloadable with its exact bytes.
    let first = attachments
        .iter()
        .find(|a| a["filename"] == "first.txt")
        .expect("first.txt attachment");
    let response = get_attachment(
        app.clone(),
        message.id,
        first["id"].as_str().unwrap(),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/plain"
    );
    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    let disposition = response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        disposition.starts_with("attachment; filename=\"first.txt\"; filename*=UTF-8''first.txt"),
        "unexpected disposition: {disposition}"
    );
    assert!(
        !disposition.contains("blobs/"),
        "disposition must not leak storage keys: {disposition}"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body.to_vec(), b"first bytes");

    // Unicode filename: ASCII-safe fallback plus RFC 5987 original.
    let unicode = attachments
        .iter()
        .find(|a| a["filename"] == "我的報告.pdf")
        .expect("unicode-named attachment");
    let response = get_attachment(
        app.clone(),
        message.id,
        unicode["id"].as_str().unwrap(),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/pdf"
    );
    let disposition = response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        disposition.contains("filename=\"____.pdf\""),
        "ASCII fallback expected: {disposition}"
    );
    assert!(
        disposition.contains("filename*=UTF-8''%E6%88%91%E7%9A%84%E5%A0%B1%E5%91%8A.pdf"),
        "RFC 5987 original expected: {disposition}"
    );
    assert!(
        !disposition.bytes().any(|b| b < 0x20 || b == 0x7f),
        "no control characters allowed: {disposition:?}"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body.to_vec(), b"pdf-bytes");

    // Zero-byte attachment downloads as an empty 200 body.
    let empty = attachments
        .iter()
        .find(|a| a["filename"] == "empty.bin")
        .expect("empty.bin attachment");
    let response = get_attachment(
        app.clone(),
        message.id,
        empty["id"].as_str().unwrap(),
        Some(&token),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(body.is_empty(), "zero-byte attachment should be empty");

    // Source download keeps its clearly separate .eml disposition.
    let request = Request::builder()
        .uri(format!("/api/v1/mail/messages/{}/source", message.id))
        .method("GET")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "message/rfc822"
    );
    let disposition = response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        disposition.contains(&format!("filename=\"message-{}.eml\"", message.id)),
        "source disposition should carry the .eml name: {disposition}"
    );

    cleanup_user(&state.db_pool, user.id).await;
    cleanup_tenant(&state.db_pool, tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn download_imported_attachment_not_found_and_authorization() {
    let state = setup_test_env().await;
    let tenant_a = create_test_tenant(&state.db_pool).await;
    let user_a = create_test_user(&state, "mail_att_owner", tenant_a).await;
    enable_mail_module(&state, tenant_a, user_a.id).await;
    let token_a = create_auth_token(&state, user_a.id, tenant_a);

    let tenant_b = create_test_tenant(&state.db_pool).await;
    let user_b = create_test_user(&state, "mail_att_intruder", tenant_b).await;
    enable_mail_module(&state, tenant_b, user_b.id).await;
    let token_b = create_auth_token(&state, user_b.id, tenant_b);

    let message = state
        .mail_service
        .import_eml(tenant_a, user_a.id, user_a.id, multi_attachment_eml())
        .await
        .expect("import_eml should succeed");

    let app = build_app(state.clone());
    let attachments = list_attachment_ids(app.clone(), message.id, &token_a).await;
    let first_id = attachments[0]["id"].as_str().unwrap().to_string();

    // Unauthenticated request is rejected.
    let response = get_attachment(app.clone(), message.id, &first_id, None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Cross-tenant request is indistinguishable from a missing attachment.
    let response = get_attachment(app.clone(), message.id, &first_id, Some(&token_b)).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Unknown attachment ID on a known message is a 404.
    let response = get_attachment(
        app.clone(),
        message.id,
        &Uuid::new_v4().to_string(),
        Some(&token_a),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Attachment whose blob vanished from the object store (and with no file
    // fallback) is a 404, not a storage error.
    sqlx::query("UPDATE mail_attachments SET blob_key = $1, file_id = NULL WHERE id = $2")
        .bind("blobs/definitely-missing-blob")
        .bind(Uuid::parse_str(&first_id).unwrap())
        .execute(&state.db_pool)
        .await
        .expect("update attachment blob_key");
    let response = get_attachment(app.clone(), message.id, &first_id, Some(&token_a)).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_user(&state.db_pool, user_a.id).await;
    cleanup_user(&state.db_pool, user_b.id).await;
    cleanup_tenant(&state.db_pool, tenant_a).await;
    cleanup_tenant(&state.db_pool, tenant_b).await;
}
