//! HTTP integration tests for Vault Sync handlers.
//!
//! These tests exercise the HTTP layer for vault-sync endpoints using
//! `axum::Router::oneshot` for fast, in-process request dispatch.
//!
//! Run with: cargo test --test vault_sync_http_test -- --ignored
//!
//! All tests are ignored because they require a live database and S3.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rustshare_core::domain::{CreateVaultRequest, VaultAdapter};
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
// Setup helpers
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
        .expect("Failed to create object store"),
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
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_sha256_mismatch_returns_400() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "sha256_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);

    let device = create_test_device(&state, user.id, tenant_id).await;

    let vault = state
        .vault_sync_service
        .create_vault(
            CreateVaultRequest {
                name: "SHA256 Test Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content = b"hello world";
    let wrong_sha256 = "0000000000000000000000000000000000000000000000000000000000000000";

    let app = build_app(state.clone());

    let request = Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/files/notes/test.md",
            vault.id
        ))
        .method("PUT")
        .header("Authorization", format!("Bearer {}", token))
        .header("X-RustShare-Base-Server-Rev", "0")
        .header("X-RustShare-SHA256", wrong_sha256)
        .header("X-RustShare-Device-ID", device.id.to_string())
        .header("Content-Type", "text/markdown")
        .body(Body::from(content.as_slice()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_without_auth_returns_401() {
    let state = setup_test_env().await;
    let app = build_app(state);

    let request = Request::builder()
        .uri("/api/vault-sync/v1/vaults")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_upload_oversized_returns_413() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "oversized_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);

    let device = create_test_device(&state, user.id, tenant_id).await;

    let vault = state
        .vault_sync_service
        .create_vault(
            CreateVaultRequest {
                name: "Oversized Test Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let oversized = vec![0u8; 50 * 1024 * 1024 + 1];
    let sha256 = hex::encode(Sha256::digest(&oversized));

    let app = build_app(state.clone());

    let request = Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/files/notes/big.md",
            vault.id
        ))
        .method("PUT")
        .header("Authorization", format!("Bearer {}", token))
        .header("X-RustShare-Base-Server-Rev", "0")
        .header("X-RustShare-SHA256", sha256)
        .header("X-RustShare-Device-ID", device.id.to_string())
        .header("Content-Type", "text/markdown")
        .body(Body::from(oversized))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_conflict_returns_structured_409() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "conflict_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);

    let device = create_test_device(&state, user.id, tenant_id).await;

    let vault = state
        .vault_sync_service
        .create_vault(
            CreateVaultRequest {
                name: "Conflict Test Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content1 = b"first version";
    let sha1 = hex::encode(Sha256::digest(content1));

    let app = build_app(state.clone());

    // First upload
    let request = Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/files/notes/conflict.md",
            vault.id
        ))
        .method("PUT")
        .header("Authorization", format!("Bearer {}", token))
        .header("X-RustShare-Base-Server-Rev", "0")
        .header("X-RustShare-SHA256", &sha1)
        .header("X-RustShare-Device-ID", device.id.to_string())
        .header("Content-Type", "text/markdown")
        .body(Body::from(content1.as_slice()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Second upload with stale base_rev
    let content2 = b"second version";
    let sha2 = hex::encode(Sha256::digest(content2));

    let request = Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/files/notes/conflict.md",
            vault.id
        ))
        .method("PUT")
        .header("Authorization", format!("Bearer {}", token))
        .header("X-RustShare-Base-Server-Rev", "0") // stale
        .header("X-RustShare-SHA256", &sha2)
        .header("X-RustShare-Device-ID", device.id.to_string())
        .header("Content-Type", "text/markdown")
        .body(Body::from(content2.as_slice()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "conflict");
    assert_eq!(json["message"], "Conflict detected");
    assert_eq!(json["resolution"], "create_conflict_copy");
    assert!(json["client_rev"].is_number());
    assert!(json["current_rev"].is_number());
    assert!(json["server_sha256"].is_string() || json["server_sha256"].is_null());

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_rename_uses_correct_headers() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "rename_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);

    let device = create_test_device(&state, user.id, tenant_id).await;

    let vault = state
        .vault_sync_service
        .create_vault(
            CreateVaultRequest {
                name: "Rename Test Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content = b"file to rename";
    let sha256 = hex::encode(Sha256::digest(content));

    let app = build_app(state.clone());

    // Upload file
    let request = Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/files/notes/old.md",
            vault.id
        ))
        .method("PUT")
        .header("Authorization", format!("Bearer {}", token))
        .header("X-RustShare-Base-Server-Rev", "0")
        .header("X-RustShare-SHA256", &sha256)
        .header("X-RustShare-Device-ID", device.id.to_string())
        .header("Content-Type", "text/markdown")
        .body(Body::from(content.as_slice()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let file_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let server_rev = file_json["server_rev"].as_i64().unwrap();

    // Rename using required headers
    let rename_body = serde_json::json!({
        "old_path": "notes/old.md",
        "new_path": "notes/new.md"
    });

    let request = Request::builder()
        .uri(format!("/api/vault-sync/v1/vaults/{}/rename", vault.id))
        .method("POST")
        .header("Authorization", format!("Bearer {}", token))
        .header("X-RustShare-Base-Server-Rev", server_rev.to_string())
        .header("X-RustShare-Device-ID", device.id.to_string())
        .header("Content-Type", "application/json")
        .body(Body::from(rename_body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_device_revoke_returns_200_or_204() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "device_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);

    let app = build_app(state.clone());

    // Register device
    let register_body = serde_json::json!({
        "device_name": "Test Device",
        "client_type": "obsidian",
        "client_version": "1.0.0",
        "vault_id": null
    });

    let request = Request::builder()
        .uri("/api/vault-sync/v1/devices/register")
        .method("POST")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(Body::from(register_body.to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let device_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let device_id = device_json["id"].as_str().unwrap();

    // Revoke device
    let request = Request::builder()
        .uri(format!("/api/vault-sync/v1/devices/{}", device_id))
        .method("DELETE")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_download_file_success() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "download_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let device = create_test_device(&state, user.id, tenant_id).await;

    let vault = state
        .vault_sync_service
        .create_vault(
            CreateVaultRequest {
                name: "Download Test Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content = b"download me";
    let sha256 = hex::encode(Sha256::digest(content));

    let app = build_app(state.clone());

    // Upload file
    let request = Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/files/notes/download.md",
            vault.id
        ))
        .method("PUT")
        .header("Authorization", format!("Bearer {}", token))
        .header("X-RustShare-Base-Server-Rev", "0")
        .header("X-RustShare-SHA256", &sha256)
        .header("X-RustShare-Device-ID", device.id.to_string())
        .header("Content-Type", "text/markdown")
        .body(Body::from(content.as_slice()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Download file
    let request = Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/files/notes/download.md",
            vault.id
        ))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), content.as_slice());

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_delete_file_success() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "delete_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);
    let device = create_test_device(&state, user.id, tenant_id).await;

    let vault = state
        .vault_sync_service
        .create_vault(
            CreateVaultRequest {
                name: "Delete Test Vault".to_string(),
                adapter: VaultAdapter::ObsidianVault,
                client_vault_id: None,
                device_id: device.id.to_string(),
            },
            tenant_id,
            user.id,
        )
        .await
        .expect("Failed to create vault");

    let content = b"delete me";
    let sha256 = hex::encode(Sha256::digest(content));

    let app = build_app(state.clone());

    // Upload file
    let request = Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/files/notes/delete.md",
            vault.id
        ))
        .method("PUT")
        .header("Authorization", format!("Bearer {}", token))
        .header("X-RustShare-Base-Server-Rev", "0")
        .header("X-RustShare-SHA256", &sha256)
        .header("X-RustShare-Device-ID", device.id.to_string())
        .header("Content-Type", "text/markdown")
        .body(Body::from(content.as_slice()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let file_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let server_rev = file_json["server_rev"].as_i64().unwrap();

    // Delete file
    let request = Request::builder()
        .uri(format!(
            "/api/vault-sync/v1/vaults/{}/files/notes/delete.md",
            vault.id
        ))
        .method("DELETE")
        .header("Authorization", format!("Bearer {}", token))
        .header("X-RustShare-Base-Server-Rev", server_rev.to_string())
        .header("X-RustShare-Device-ID", device.id.to_string())
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify tombstone via manifest
    let request = Request::builder()
        .uri(format!("/api/vault-sync/v1/vaults/{}/manifest", vault.id))
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let entry = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"] == "notes/delete.md")
        .expect("File should appear in manifest");
    assert!(entry["deleted"].as_bool().unwrap());

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_vault_name_path_traversal_returns_400() {
    let state = setup_test_env().await;
    let tenant_id = Uuid::new_v4();
    let user = create_test_user(&state, "traversal_user", tenant_id).await;
    let token = create_auth_token(&state, user.id, tenant_id);

    let app = build_app(state.clone());

    for bad_name in &["../escape", "foo/bar", "foo\\bar"] {
        let body = serde_json::json!({
            "name": bad_name,
            "adapter": "ObsidianVault",
            "device_id": "550e8400-e29b-41d4-a716-446655440000"
        });

        let request = Request::builder()
            .uri("/api/vault-sync/v1/vaults")
            .method("POST")
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Expected 400 for vault name: {}",
            bad_name
        );
    }

    cleanup_user(&state.db_pool, user.id).await;
}

#[tokio::test]
#[ignore] // Requires database and S3
async fn test_security_headers_present() {
    let state = setup_test_env().await;
    let app = build_app(state);

    let request = Request::builder()
        .uri("/api/vault-sync/v1/vaults")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let headers = response.headers();

    assert!(headers.contains_key("x-content-type-options"));
    assert_eq!(
        headers["x-content-type-options"].to_str().unwrap(),
        "nosniff"
    );
    assert!(headers.contains_key("x-frame-options"));
    assert_eq!(headers["x-frame-options"].to_str().unwrap(), "DENY");
    assert!(headers.contains_key("referrer-policy"));
    assert_eq!(
        headers["referrer-policy"].to_str().unwrap(),
        "strict-origin-when-cross-origin"
    );
}
