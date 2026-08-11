//! HTTP integration tests for the Vault Sync WebUI content endpoints.
//!
//! These tests exercise `GET/PUT /api/vault-sync/v1/vaults/{id}/content/{path}`
//! (the browser editing path) using `axum::Router::oneshot` for fast,
//! in-process request dispatch. The sync-client `/files/*` endpoints are
//! covered in `vault_sync_http_test.rs`.
//!
//! Run with: cargo test --test vault_webui_content_test -- --ignored
//!
//! All tests are ignored because they require a live database and S3.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use rustshare_core::domain::{
    CreateVaultRequest, DeleteVaultFileRequest, UploadVaultFileRequest, VaultAdapter,
    VaultWritePolicy,
};
use rustshare_server::middleware;
use rustshare_server::routes;
use rustshare_server::state::AppState;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Setup helpers (same conventions as vault_sync_http_test.rs)
// ---------------------------------------------------------------------------

async fn setup_test_env() -> AppState {
    // Load .env so tests pick up credentials without manual exports.
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

async fn create_test_device(
    state: &AppState,
    user_id: Uuid,
    tenant_id: Uuid,
) -> rustshare_core::domain::VaultDevice {
    let device = rustshare_core::domain::VaultDevice {
        id: Uuid::new_v4(),
        tenant_id,
        user_id,
        vault_id: None,
        device_name: "Test Device".to_string(),
        client_type: "test".to_string(),
        client_version: None,
        last_sync_rev: None,
        revoked_at: None,
        created_at: chrono::Utc::now(),
        last_seen_at: chrono::Utc::now(),
    };
    state
        .vault_sync_service
        .register_device(device.clone(), user_id)
        .await
        .expect("Failed to create test device");
    device
}

fn create_auth_token(state: &AppState, user_id: Uuid, tenant_id: Uuid) -> String {
    state
        .jwt_manager
        .generate(user_id, "test@example.com", tenant_id)
        .unwrap()
}

async fn cleanup_user(pool: &PgPool, user_id: Uuid) {
    sqlx::query(
        "DELETE FROM vault_files WHERE tenant_id IN (SELECT tenant_id FROM users WHERE id = $1)",
    )
    .bind(user_id)
    .execute(pool)
    .await
    .ok();
    sqlx::query("DELETE FROM vaults WHERE owner_user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM vault_devices WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM files WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM folders WHERE owner_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .ok();
}

fn build_app(state: AppState) -> axum::Router {
    routes::vault_sync_routes()
        .with_state(state)
        .layer(axum::middleware::from_fn(
            middleware::security_headers_middleware,
        ))
}

// ---------------------------------------------------------------------------
// WebUI content helpers
// ---------------------------------------------------------------------------

/// Create a vault with web editing enabled and seed one file through the
/// sync-client upload path (the same path an Obsidian client would use).
async fn seed_vault_with_file(
    state: &AppState,
    tenant_id: Uuid,
    user_id: Uuid,
    device_id: Uuid,
    path: &str,
    content: &[u8],
    content_type: &str,
) -> (
    rustshare_core::domain::Vault,
    rustshare_core::domain::VaultFile,
) {
    let vault = state
        .vault_sync_service
        .create_vault(
            CreateVaultRequest {
                name: "WebUI Content Test Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device_id.to_string(),
            },
            tenant_id,
            user_id,
        )
        .await
        .expect("Failed to create vault");

    state
        .vault_sync_service
        .update_vault_write_policy(
            vault.id,
            VaultWritePolicy::WebEditingEnabled,
            tenant_id,
            user_id,
        )
        .await
        .expect("Failed to enable web editing");

    let sha256 = hex::encode(Sha256::digest(content));
    let file = state
        .vault_sync_service
        .upload_file(
            UploadVaultFileRequest {
                vault_id: vault.id,
                relative_path: path.to_string(),
                content_type: Some(content_type.to_string()),
                sha256,
                size: content.len() as i64,
                base_server_rev: 0,
                device_id: device_id.to_string(),
                content: Bytes::from(content.to_vec()),
            },
            tenant_id,
            user_id,
        )
        .await
        .expect("Failed to upload seed file");

    (vault, file)
}

fn get_content_request(vault_id: Uuid, path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/content/{}",
            vault_id, path
        ))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

fn put_content_request(
    vault_id: Uuid,
    path: &str,
    token: &str,
    content: &str,
    expected_revision: i64,
) -> Request<Body> {
    let body = serde_json::json!({
        "content": content,
        "expected_revision": expected_revision
    });
    Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/content/{}",
            vault_id, path
        ))
        .method("PUT")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn response_json(response: axum::http::Response<Body>) -> serde_json::Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_webui_get_and_save_content_success() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "webui_save_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let device = create_test_device(&state, user.id, tenant_id).await;

    let (vault, _file) = seed_vault_with_file(
        &state,
        tenant_id,
        user.id,
        device.id,
        "notes/note.md",
        b"hello world",
        "text/markdown",
    )
    .await;

    let app = build_app(state.clone());

    // GET returns the uploaded content with the current revision.
    let response = app
        .clone()
        .oneshot(get_content_request(vault.id, "notes/note.md", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["content"], "hello world");
    assert_eq!(json["server_rev"], 1);
    assert_eq!(json["path"], "notes/note.md");

    // PUT with the matching expected revision saves and bumps the revision.
    let response = app
        .clone()
        .oneshot(put_content_request(
            vault.id,
            "notes/note.md",
            &token,
            "updated body",
            1,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["server_rev"], 2);
    assert_eq!(json["path"], "notes/note.md");

    // GET reflects the saved content.
    let response = app
        .oneshot(get_content_request(vault.id, "notes/note.md", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["content"], "updated body");
    assert_eq!(json["server_rev"], 2);

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_webui_save_stale_revision_returns_structured_409() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "webui_conflict_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let device = create_test_device(&state, user.id, tenant_id).await;

    let (vault, _file) = seed_vault_with_file(
        &state,
        tenant_id,
        user.id,
        device.id,
        "notes/conflict.md",
        b"first version",
        "text/markdown",
    )
    .await;

    let app = build_app(state.clone());

    // First save wins: rev 1 -> 2.
    let response = app
        .clone()
        .oneshot(put_content_request(
            vault.id,
            "notes/conflict.md",
            &token,
            "second version",
            1,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Second save with the stale base revision conflicts.
    let response = app
        .oneshot(put_content_request(
            vault.id,
            "notes/conflict.md",
            &token,
            "third version",
            1,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let json = response_json(response).await;
    assert_eq!(json["error"], "conflict");
    assert_eq!(json["client_rev"], 1);
    assert_eq!(json["current_rev"], 2);
    assert_eq!(
        json["server_sha256"],
        hex::encode(Sha256::digest(b"second version"))
    );
    assert_eq!(json["resolution"], "create_conflict_copy");

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_webui_save_write_policy_denied_returns_403() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "webui_policy_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let device = create_test_device(&state, user.id, tenant_id).await;

    // Vault is created with the default (read-only) write policy: no
    // update_vault_write_policy call here on purpose.
    let vault = state
        .vault_sync_service
        .create_vault(
            CreateVaultRequest {
                name: "WebUI Policy Test Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content = b"policy content";
    let sha256 = hex::encode(Sha256::digest(content));
    state
        .vault_sync_service
        .upload_file(
            UploadVaultFileRequest {
                vault_id: vault.id,
                relative_path: "notes/note.md".to_string(),
                content_type: Some("text/markdown".to_string()),
                sha256,
                size: content.len() as i64,
                base_server_rev: 0,
                device_id: device.id.to_string(),
                content: Bytes::from(content.as_slice()),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to upload seed file");

    let app = build_app(state.clone());

    let response = app
        .oneshot(put_content_request(
            vault.id,
            "notes/note.md",
            &token,
            "denied",
            1,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_webui_content_without_auth_returns_401() {
    let state = setup_test_env().await;
    let app = build_app(state);
    let vault_id = Uuid::new_v4();

    let request = Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/content/notes/note.md",
            vault_id
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let request = Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/content/notes/note.md",
            vault_id
        ))
        .method("PUT")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({"content": "x", "expected_revision": 1}).to_string(),
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_webui_content_not_editable_file_type_returns_403() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "webui_png_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let device = create_test_device(&state, user.id, tenant_id).await;

    let (vault, _file) = seed_vault_with_file(
        &state,
        tenant_id,
        user.id,
        device.id,
        "assets/image.png",
        b"\x89PNG fake image bytes",
        "image/png",
    )
    .await;

    let app = build_app(state.clone());

    let response = app
        .clone()
        .oneshot(get_content_request(vault.id, "assets/image.png", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = app
        .oneshot(put_content_request(
            vault.id,
            "assets/image.png",
            &token,
            "not an image anymore",
            1,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_webui_content_other_owner_forbidden_cross_tenant_not_found() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let owner = create_test_user(&state, "webui_owner", tenant_id).await;
    let device = create_test_device(&state, owner.id, tenant_id).await;

    let (vault, _file) = seed_vault_with_file(
        &state,
        tenant_id,
        owner.id,
        device.id,
        "notes/note.md",
        b"owner content",
        "text/markdown",
    )
    .await;

    let app = build_app(state.clone());

    // Another user in the same tenant is not the vault owner -> 403.
    let other = create_test_user(&state, "webui_other", tenant_id).await;
    let other_token = create_auth_token(&state, other.id, tenant_id);
    let response = app
        .clone()
        .oneshot(get_content_request(vault.id, "notes/note.md", &other_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = app
        .clone()
        .oneshot(put_content_request(
            vault.id,
            "notes/note.md",
            &other_token,
            "stolen",
            1,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // A user in a different tenant cannot see the vault at all -> 404.
    let other_tenant = Uuid::new_v4();
    let stranger = create_test_user(&state, "webui_stranger", other_tenant).await;
    let stranger_token = create_auth_token(&state, stranger.id, other_tenant);
    let response = app
        .clone()
        .oneshot(get_content_request(
            vault.id,
            "notes/note.md",
            &stranger_token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Unknown vault id -> 404.
    let owner_token = create_auth_token(&state, owner.id, tenant_id);
    let response = app
        .oneshot(get_content_request(
            Uuid::new_v4(),
            "notes/note.md",
            &owner_token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_user(&state.db_pool, owner.id).await;
    cleanup_user(&state.db_pool, other.id).await;
    cleanup_user(&state.db_pool, stranger.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_webui_save_tombstoned_file_returns_409() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "webui_tombstone_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let device = create_test_device(&state, user.id, tenant_id).await;

    let (vault, file) = seed_vault_with_file(
        &state,
        tenant_id,
        user.id,
        device.id,
        "notes/tombstone.md",
        b"soon deleted",
        "text/markdown",
    )
    .await;

    // Tombstone the file through the sync-client delete path.
    state
        .vault_sync_service
        .delete_file(
            DeleteVaultFileRequest {
                vault_id: vault.id,
                relative_path: "notes/tombstone.md".to_string(),
                base_server_rev: file.server_rev,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to tombstone file");

    let app = build_app(state.clone());

    // Saving against a tombstone conflicts.
    let response = app
        .clone()
        .oneshot(put_content_request(
            vault.id,
            "notes/tombstone.md",
            &token,
            "revived",
            1,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // Loading a tombstone reports not found.
    let response = app
        .oneshot(get_content_request(vault.id, "notes/tombstone.md", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_webui_save_visible_in_next_manifest() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "webui_manifest_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let device = create_test_device(&state, user.id, tenant_id).await;

    let (vault, _file) = seed_vault_with_file(
        &state,
        tenant_id,
        user.id,
        device.id,
        "notes/manifest.md",
        b"original",
        "text/markdown",
    )
    .await;

    let app = build_app(state.clone());

    // Save via the WebUI endpoint.
    let response = app
        .clone()
        .oneshot(put_content_request(
            vault.id,
            "notes/manifest.md",
            &token,
            "web edit",
            1,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // The next manifest poll observes the WebUI write: bumped revision and the
    // new content hash, so sync clients can pull the change down.
    let request = Request::builder()
        .uri(format!("/api/vault-sync/v1/vaults/{}/manifest", vault.id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let manifest = response_json(response).await;
    assert!(manifest["server_rev"].as_i64().unwrap() >= 2);

    let entry = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"] == "notes/manifest.md")
        .expect("File should appear in manifest");
    assert_eq!(entry["server_rev"], 2);
    assert_eq!(entry["sha256"], hex::encode(Sha256::digest(b"web edit")));
    assert_eq!(entry["deleted"], false);

    cleanup_user(&state.db_pool, user.id).await;
}
