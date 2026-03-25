//! RustShare Server
//!
//! # Reverse Proxy Support
//!
//! The server supports deployment behind reverse proxies (nginx, Cloudflare, AWS ALB, etc.).
//! Rate limiting automatically extracts real client IPs from standard proxy headers:
//!
//! - **X-Forwarded-For**: Takes leftmost non-private IP (most common)
//! - **X-Real-IP**: Used by nginx and Cloudflare
//! - **Forwarded**: RFC 7239 standard header
//! - **Direct connection**: Falls back to ConnectInfo when no proxy headers present
//!
//! ## Security
//!
//! - Private/loopback IPs in headers are rejected (prevents spoofing)
//! - Header values are validated and sanitized
//! - IP extraction source is logged for debugging
//!
//! ## Proxy Configuration Examples
//!
//! ### nginx
//! ```nginx
//! location / {
//!     proxy_pass http://localhost:8080;
//!     proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
//!     proxy_set_header X-Real-IP $remote_addr;
//!     proxy_set_header Host $host;
//! }
//! ```
//!
//! ### Cloudflare
//! - X-Forwarded-For is automatically set by Cloudflare
//! - No additional configuration required
//!
//! ### AWS Application Load Balancer
//! - X-Forwarded-For is automatically set
//! - Ensure target group health checks are configured
//!

mod handlers;
mod middleware;
mod oidc;
mod replication;
mod replication_handlers;
mod web_session;

use crate::replication::{spawn_replication_worker, ReplicationWorkerConfig};
use crate::web_session::{
    build_expired_session_cookie, build_session_cookie, create_user_session, extract_cookie_value,
};
use anyhow::Result;
use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{any, delete, get, patch, post, put},
    Json, Router,
};
use rustshare_auth::{JwtManager, PasswordHasher};
use rustshare_crypto::SecretEncryptionKey;
use rustshare_core::{
    domain::{SharePermissions, User},
    events::EventBroadcaster,
    services::{
        FileService, FolderService, NotificationService, PermissionResolver, ShareService,
        ThumbnailService, UserShareService, UserShareServiceDeps,
    },
};
use rustshare_infrastructure::repositories::{
    FileRepository, FolderRepository, NotificationRepository, ShareRepository, UserRepository,
};
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::Mutex;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::info;

type AppUserShareService = UserShareService<
    ShareRepository,
    UserRepository,
    FileRepository,
    FolderRepository,
    ShareRepository,
    FileRepository,
    FolderRepository,
    NotificationRepository,
    EventStore,
>;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub metadata_store: Arc<MetadataStore>,
    pub event_store: Arc<EventStore>,
    pub object_store: Arc<ObjectStore>,
    pub jwt_manager: Arc<JwtManager>,
    pub broadcaster: Arc<EventBroadcaster>,
    pub file_service: Arc<FileService<EventStore, MetadataStore, ObjectStore>>,
    pub folder_service: Arc<FolderService<EventStore, MetadataStore>>,
    pub share_service: Arc<ShareService<EventStore, MetadataStore, JwtManager>>,
    pub thumbnail_service: Arc<ThumbnailService<ObjectStore>>,
    pub permission_resolver:
        Arc<PermissionResolver<ShareRepository, FileRepository, FolderRepository>>,
    pub notification_service: Arc<NotificationService<NotificationRepository>>,
    pub user_share_service: Arc<AppUserShareService>,
    pub rate_limit_config: Arc<middleware::RateLimitConfig>,
    pub secret_key: SecretEncryptionKey,
    pub poll_rate_limiter: Arc<Mutex<HashMap<String, Instant>>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,rustshare=debug".to_string()),
        )
        .init();

    info!("Starting RustShare server");

    // Connect to database
    let database_url = std::env::var("DATABASE_URL")?;
    let db_pool = PgPool::connect(&database_url).await?;

    info!("Connected to database");

    // Run migrations (path relative to workspace root)
    sqlx::migrate!("../migrations").run(&db_pool).await?;

    info!("Database migrations applied");

    // Initialize stores
    let metadata_store = Arc::new(MetadataStore::new(db_pool.clone()));
    let event_store = Arc::new(EventStore::new(db_pool.clone()));

    // Initialize object store
    let rustfs_endpoint = std::env::var("RUSTFS_ENDPOINT")?;
    let rustfs_region = std::env::var("RUSTFS_REGION")?;
    let rustfs_bucket = std::env::var("RUSTFS_BUCKET")?;

    let object_store =
        Arc::new(ObjectStore::new(rustfs_endpoint, rustfs_region, rustfs_bucket).await?);

    info!("Object store initialized");

    // Initialize JWT manager
    let jwt_secret = std::env::var("JWT_SECRET")?;
    let jwt_manager = Arc::new(JwtManager::new(jwt_secret));

    // Initialize EventBroadcaster
    let capacity = std::env::var("BROADCAST_CAPACITY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    let broadcaster = Arc::new(EventBroadcaster::new(capacity));

    info!("EventBroadcaster initialized with capacity {}", capacity);

    // Initialize services
    let file_service = Arc::new(FileService::new(
        Arc::clone(&event_store),
        Arc::clone(&metadata_store),
        Arc::clone(&object_store),
        Arc::clone(&broadcaster),
    ));
    let folder_service = Arc::new(FolderService::new(
        Arc::clone(&event_store),
        Arc::clone(&metadata_store),
        Arc::clone(&broadcaster),
    ));
    let share_service = Arc::new(ShareService::new(
        Arc::clone(&event_store),
        Arc::clone(&metadata_store),
        Arc::clone(&broadcaster),
        Arc::clone(&jwt_manager),
    ));
    let thumbnail_service = Arc::new(ThumbnailService::new(
        db_pool.clone(),
        Arc::clone(&object_store),
    ));

    // Initialize repositories for new services
    let notification_repository = NotificationRepository::new(db_pool.clone());
    let share_repository = Arc::new(ShareRepository::new(db_pool.clone()));
    let user_repository = Arc::new(UserRepository::new(db_pool.clone()));
    let file_repository = Arc::new(FileRepository::new(db_pool.clone()));
    let folder_repository = Arc::new(FolderRepository::new(db_pool.clone()));

    // Initialize permission resolver
    let permission_resolver = Arc::new(PermissionResolver::new(
        Arc::clone(&share_repository),
        Arc::clone(&file_repository),
        Arc::clone(&folder_repository),
    ));

    // Initialize notification service
    let notification_service = Arc::new(NotificationService::new(notification_repository));

    // Initialize user share service
    let user_share_service = Arc::new(UserShareService::new(UserShareServiceDeps {
        share_repo: Arc::clone(&share_repository),
        user_repo: Arc::clone(&user_repository),
        file_repo: Arc::clone(&file_repository),
        folder_repo: Arc::clone(&folder_repository),
        permission_resolver: Arc::clone(&permission_resolver),
        notification_service: Arc::clone(&notification_service),
        event_store: Arc::clone(&event_store),
        broadcaster: Arc::clone(&broadcaster),
    }));

    // Initialize rate limiting configuration
    let rate_limit_config = Arc::new(middleware::RateLimitConfig::new());

    info!("Rate limiting initialized");

    let replication_worker_config = ReplicationWorkerConfig::from_env();
    spawn_replication_worker(
        Arc::clone(&metadata_store),
        Arc::clone(&object_store),
        Arc::clone(&event_store),
        Arc::clone(&broadcaster),
        replication_worker_config,
    );

    // Bootstrap admin user if no users exist
    if !metadata_store.has_users().await? {
        let admin_username = std::env::var("RUSTSHARE_ADMIN_USERNAME")?;
        let admin_email = std::env::var("RUSTSHARE_ADMIN_EMAIL")?;
        let admin_password = std::env::var("RUSTSHARE_ADMIN_PASSWORD")?;

        let password_hash = PasswordHasher::hash(&admin_password)?;
        let admin_user = User::new(
            admin_username.clone(),
            "Administrator".to_string(),
            password_hash,
            admin_email.clone(),
            true,
            default_storage_quota_bytes(),
        );

        metadata_store.create_user(&admin_user).await?;

        info!("Admin user created: {} ({})", admin_username, admin_email);
    }

    ensure_optional_seed_user(
        &metadata_store,
        "RUSTSHARE_DEMO_VIEWER_USERNAME",
        "RUSTSHARE_DEMO_VIEWER_EMAIL",
        "RUSTSHARE_DEMO_VIEWER_PASSWORD",
        std::env::var("RUSTSHARE_DEMO_VIEWER_DISPLAY_NAME")
            .unwrap_or_else(|_| "Viewer User".to_string()),
        false,
    )
    .await?;

    // Load secret encryption key
    let secret_key = SecretEncryptionKey::from_env()
        .map_err(|e| anyhow::anyhow!("Secret encryption key error: {}", e))?;

    // Build application state
    let state = AppState {
        db_pool,
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
        rate_limit_config,
        secret_key,
        poll_rate_limiter: Arc::new(Mutex::new(HashMap::new())),
    };

    // Build router.
    //
    // Contract freeze note:
    // - `/api/v1/...` is the stable client surface
    // - `/api/ws` is the stable realtime endpoint
    // - realtime compatibility aliases were removed in Phase 7 wave 1
    // - legacy `/api/auth/...` aliases were removed in Phase 7 wave 2
    // - unversioned resource aliases were removed in Phase 7 wave 3
    // - remaining unversioned `/api/...` routes are limited to narrower compatibility or internal/operator surfaces
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Canonical versioned auth routes
        .route("/api/v1/auth/config", get(oidc::auth_config))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/oidc/login", get(oidc::oidc_login))
        .route("/api/v1/auth/oidc/callback", get(oidc::oidc_callback))
        .route(
            "/api/v1/auth/oidc/mobile/authorize",
            post(oidc::mobile_oidc_authorize),
        )
        .route(
            "/api/v1/auth/oidc/mobile/exchange",
            post(oidc::mobile_oidc_exchange),
        )
        // Device pairing auth routes
        .route(
            "/api/v1/auth/device/qr-info",
            get(handlers::device_auth::device_qr_info),
        )
        .route(
            "/api/v1/auth/device/request",
            post(handlers::device_auth::device_request),
        )
        .route(
            "/api/v1/auth/device/poll",
            post(handlers::device_auth::device_poll),
        )
        .route(
            "/api/v1/auth/device/approve",
            post(handlers::device_auth::device_approve),
        )
        // Device management routes
        .route(
            "/api/v1/user/devices",
            get(handlers::devices::list_devices),
        )
        .route(
            "/api/v1/user/devices/:id",
            delete(handlers::devices::revoke_device),
        )
        // File routes (Task 15-19)
        .route("/api/v1/files", get(handlers::list_files))
        .route("/api/v1/files/upload", post(handlers::upload_file))
        .route("/api/v1/files/:id", get(handlers::get_file))
        .route("/api/v1/files/:id", put(handlers::update_file))
        .route("/api/v1/files/:id", delete(handlers::delete_file))
        .route("/api/v1/files/:id/download", get(handlers::download_file))
        .route(
            "/api/v1/files/:id/versions",
            get(handlers::get_file_versions),
        )
        .route(
            "/api/v1/files/:id/restore",
            post(handlers::restore_file_version),
        )
        .route("/api/v1/files/:id/move", post(handlers::move_file))
        .route("/api/v1/files/:id/rename", post(handlers::rename_file))
        .route(
            "/api/v1/files/:id/thumbnail",
            get(handlers::get_file_thumbnail),
        )
        .route(
            "/api/admin/replication/jobs",
            get(replication_handlers::list_replication_jobs),
        )
        .route(
            "/api/admin/replication/summary",
            get(replication_handlers::get_replication_summary),
        )
        .route(
            "/api/admin/replication/targets",
            get(replication_handlers::list_replication_targets),
        )
        // Forward-compatible v1 aliases for new replication visibility endpoints
        .route(
            "/api/v1/files/:id/replication",
            get(replication_handlers::get_file_replication_status),
        )
        .route(
            "/api/v1/admin/replication/jobs",
            get(replication_handlers::list_replication_jobs),
        )
        .route(
            "/api/v1/admin/replication/summary",
            get(replication_handlers::get_replication_summary),
        )
        .route(
            "/api/v1/admin/replication/targets",
            get(replication_handlers::list_replication_targets),
        )
        // Admin user management (Task 4)
        .route(
            "/api/v1/admin/users",
            get(handlers::admin::users::list_admin_users),
        )
        .route(
            "/api/v1/admin/users",
            post(handlers::admin::users::create_admin_user),
        )
        .route(
            "/api/v1/admin/users/:id",
            get(handlers::admin::users::get_admin_user),
        )
        .route(
            "/api/v1/admin/users/:id",
            patch(handlers::admin::users::update_admin_user),
        )
        .route(
            "/api/v1/admin/users/:id/disable",
            post(handlers::admin::users::disable_admin_user),
        )
        .route(
            "/api/v1/admin/users/:id/enable",
            post(handlers::admin::users::enable_admin_user),
        )
        .route(
            "/api/v1/admin/users/:id",
            delete(handlers::admin::users::delete_admin_user),
        )
        // Admin audit log (Task 4)
        .route(
            "/api/v1/admin/audit",
            get(handlers::admin::audit::list_audit_log),
        )
        // Admin group management (Task 1)
        .route(
            "/api/v1/admin/groups",
            get(handlers::admin::groups::list_groups),
        )
        .route(
            "/api/v1/admin/groups",
            post(handlers::admin::groups::create_group),
        )
        .route(
            "/api/v1/admin/groups/:id",
            get(handlers::admin::groups::get_group),
        )
        .route(
            "/api/v1/admin/groups/:id",
            patch(handlers::admin::groups::update_group),
        )
        .route(
            "/api/v1/admin/groups/:id",
            delete(handlers::admin::groups::delete_group),
        )
        .route(
            "/api/v1/admin/groups/:id/members",
            post(handlers::admin::groups::add_member),
        )
        .route(
            "/api/v1/admin/groups/:id/members/:user_id",
            delete(handlers::admin::groups::remove_member),
        )
        // Admin OIDC/SMTP config
        .route(
            "/api/v1/admin/config/oidc",
            get(handlers::admin::config::get_oidc_config),
        )
        .route(
            "/api/v1/admin/config/oidc",
            put(handlers::admin::config::update_oidc_config),
        )
        .route(
            "/api/v1/admin/config/oidc/test",
            post(handlers::admin::config::test_oidc_config),
        )
        .route(
            "/api/v1/admin/config/smtp",
            get(handlers::admin::config::get_smtp_config),
        )
        .route(
            "/api/v1/admin/config/smtp",
            put(handlers::admin::config::update_smtp_config),
        )
        .route(
            "/api/v1/admin/config/smtp/test",
            post(handlers::admin::config::test_smtp_config),
        )
        // Admin webhooks
        .route(
            "/api/v1/admin/integrations/webhooks",
            get(handlers::admin::webhooks::list_webhooks),
        )
        .route(
            "/api/v1/admin/integrations/webhooks",
            post(handlers::admin::webhooks::create_webhook),
        )
        .route(
            "/api/v1/admin/integrations/webhooks/:id",
            patch(handlers::admin::webhooks::update_webhook),
        )
        .route(
            "/api/v1/admin/integrations/webhooks/:id",
            delete(handlers::admin::webhooks::delete_webhook),
        )
        .route(
            "/api/v1/admin/integrations/webhooks/:id/test",
            post(handlers::admin::webhooks::test_webhook),
        )
        // Folder routes (Task 20-22)
        // NOTE: More specific routes (with literal path segments) must come BEFORE parameterized routes
        .route("/api/v1/folders", post(handlers::create_folder))
        .route(
            "/api/v1/folders/root/contents",
            get(handlers::get_root_contents),
        )
        .route("/api/v1/folders/tree", get(handlers::get_folder_tree))
        .route(
            "/api/v1/folders/:id/contents",
            get(handlers::get_folder_contents),
        )
        .route("/api/v1/folders/:id/move", post(handlers::move_folder))
        .route("/api/v1/folders/:id/rename", post(handlers::rename_folder))
        .route("/api/v1/folders/:id", get(handlers::get_folder))
        .route("/api/v1/folders/:id", delete(handlers::delete_folder))
        // Share routes (Task 9)
        .route(
            "/api/v1/files/:file_id/shares",
            post(handlers::create_public_file_share),
        )
        .route(
            "/api/v1/folders/:folder_id/shares",
            post(handlers::create_public_folder_share),
        )
        .route(
            "/api/v1/files/:file_id/shares",
            get(handlers::list_public_file_shares),
        )
        .route(
            "/api/v1/folders/:folder_id/shares",
            get(handlers::list_public_folder_shares),
        )
        .route("/api/v1/shares", get(list_user_shares))
        .route("/api/v1/shares/:id/access-log", get(get_share_access_log))
        .route("/api/v1/shares/:id", delete(revoke_share))
        // User routes
        .route("/api/users/me", get(handlers::get_user_profile))
        .route("/api/v1/users/me", get(handlers::get_user_profile))
        .route("/api/v1/me", get(handlers::get_user_profile))
        .route("/api/users/me/theme", patch(handlers::update_user_theme))
        .route("/api/v1/users/me/theme", patch(handlers::update_user_theme))
        .route("/api/v1/me/theme", patch(handlers::update_user_theme))
        .route("/api/users/me/sessions", get(handlers::list_user_sessions))
        .route(
            "/api/v1/users/me/sessions",
            get(handlers::list_user_sessions),
        )
        .route("/api/v1/me/sessions", get(handlers::list_user_sessions))
        .route(
            "/api/users/me/security-events",
            get(handlers::list_user_security_events),
        )
        .route(
            "/api/v1/users/me/security-events",
            get(handlers::list_user_security_events),
        )
        .route(
            "/api/v1/me/security-events",
            get(handlers::list_user_security_events),
        )
        .route(
            "/api/users/me/sessions/:id",
            delete(handlers::delete_user_session),
        )
        .route(
            "/api/v1/users/me/sessions/:id",
            delete(handlers::delete_user_session),
        )
        .route(
            "/api/v1/me/sessions/:id",
            delete(handlers::delete_user_session),
        )
        .route(
            "/api/users/me/password",
            patch(handlers::update_user_password),
        )
        .route(
            "/api/v1/users/me/password",
            patch(handlers::update_user_password),
        )
        .route("/api/v1/me/password", patch(handlers::update_user_password))
        // Profile routes (Task 17)
        .route(
            "/api/v1/users/me/profile",
            get(handlers::get_profile),
        )
        .route(
            "/api/v1/users/me/profile",
            patch(handlers::update_profile),
        )
        // Internal user share routes
        .route("/api/v1/files/:id/share", post(handlers::create_file_share))
        .route(
            "/api/v1/folders/:id/share",
            post(handlers::create_folder_share),
        )
        .route(
            "/api/v1/shares/received",
            get(handlers::list_received_shares),
        )
        .route(
            "/api/v1/files/:id/recipients",
            get(handlers::list_file_recipients),
        )
        .route(
            "/api/v1/folders/:id/recipients",
            get(handlers::list_folder_recipients),
        )
        .route(
            "/api/v1/shares/:id/permission",
            put(handlers::update_recipient_permission),
        )
        .route(
            "/api/v1/shares/:id/recipient",
            delete(handlers::remove_recipient),
        )
        .route("/api/v1/notifications", get(handlers::list_notifications))
        .route(
            "/api/v1/notifications/unread-count",
            get(handlers::count_unread_notifications),
        )
        .route(
            "/api/v1/notifications/:id/read",
            put(handlers::mark_notification_read),
        )
        .route(
            "/api/v1/notifications/:id",
            delete(handlers::delete_notification),
        )
        // Public share routes (Task 10 - no authentication required for session creation and info)
        .route(
            "/api/v1/public/share/:token/session",
            post(handlers::create_session),
        )
        .route(
            "/api/v1/public/share/:token/info",
            get(handlers::get_share_info),
        )
        .route(
            "/api/v1/public/share/:token/file",
            get(handlers::download_shared_file),
        )
        .route(
            "/api/v1/public/share/:token/folder/contents",
            get(handlers::get_shared_folder_contents),
        )
        .route(
            "/api/v1/public/share/:token/folder/files/:file_id",
            get(handlers::download_shared_folder_file),
        )
        .route(
            "/api/v1/public/share/:token/folder/upload",
            post(handlers::upload_shared_folder_file),
        )
        // WebSocket sync endpoint (Task Phase 3A)
        .route("/api/ws", get(handlers::sync_handler))
        .route("/api", any(api_not_found))
        .route("/api/*path", any(api_not_found))
        .with_state(state.clone())
        // Increase body size limit for file uploads (500MB)
        // This must be applied BEFORE other middleware layers
        .layer(DefaultBodyLimit::max(500 * 1024 * 1024))
        .layer(axum::middleware::from_fn(middleware::csrf_middleware))
        // Apply rate limiting middleware after state is set
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::rate_limit_middleware,
        ))
        // Tracing
        .layer(TraceLayer::new_for_http())
        // All non-API requests are served by the compiled SPA bundle.
        .fallback_service(frontend_service());

    // Start server
    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("{}:{}", host, port);

    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}

async fn ensure_optional_seed_user(
    metadata_store: &Arc<MetadataStore>,
    username_env: &str,
    email_env: &str,
    password_env: &str,
    display_name: String,
    is_admin: bool,
) -> Result<()> {
    let username = std::env::var(username_env).ok();
    let email = std::env::var(email_env).ok();
    let password = std::env::var(password_env).ok();

    if username.is_none() && email.is_none() && password.is_none() {
        return Ok(());
    }

    let username =
        username.ok_or_else(|| anyhow::anyhow!("Missing required env {}", username_env))?;
    let email = email.ok_or_else(|| anyhow::anyhow!("Missing required env {}", email_env))?;
    let password =
        password.ok_or_else(|| anyhow::anyhow!("Missing required env {}", password_env))?;

    if metadata_store.find_user_by_email(&email).await?.is_some() {
        return Ok(());
    }

    let password_hash = PasswordHasher::hash(&password)?;
    let user = User::new(
        username.clone(),
        display_name,
        password_hash,
        email.clone(),
        is_admin,
        default_storage_quota_bytes(),
    );

    metadata_store.create_user(&user).await?;

    info!("Seed user created: {} ({})", username, email);

    Ok(())
}

fn default_storage_quota_bytes() -> i64 {
    std::env::var("RUSTSHARE_DEFAULT_STORAGE_QUOTA_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_737_418_240)
}

/// Health check endpoint
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

fn frontend_service() -> ServeDir<ServeFile> {
    let dist_dir = frontend_dist_dir();
    let fallback_file = dist_dir.join("200.html");

    ServeDir::new(dist_dir).fallback(ServeFile::new(fallback_file))
}

fn frontend_dist_dir() -> PathBuf {
    std::env::var("FRONTEND_DIST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/app/frontend-build"))
}

async fn api_not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "API route not found" })),
    )
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
}

/// Login request
#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

/// Login response
#[derive(Serialize)]
struct LoginResponse {
    token: String,
    user: UserResponse,
}

#[derive(Serialize)]
struct UserResponse {
    id: String,
    email: String,
    display_name: String,
    is_admin: bool,
}

#[derive(Serialize)]
struct OwnedShareResponse {
    id: uuid::Uuid,
    resource_id: uuid::Uuid,
    resource_type: String,
    resource_name: String,
    share_token: String,
    permissions: SharePermissions,
    password_protected: bool,
    access_count: i32,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
struct ShareAccessLogQuery {
    limit: Option<i64>,
}

#[derive(Serialize)]
struct ShareAccessLogResponse {
    accessed_at: chrono::DateTime<chrono::Utc>,
    action: String,
    success: bool,
    actor_type: Option<String>,
    actor_label: Option<String>,
    ip_address: Option<String>,
    user_agent: Option<String>,
    share_session_id: Option<uuid::Uuid>,
    share_session_subject: Option<String>,
}

async fn list_user_shares(
    axum::extract::State(state): axum::extract::State<AppState>,
    handlers::AuthenticatedUser { user_id }: handlers::AuthenticatedUser,
) -> Result<Json<Vec<OwnedShareResponse>>, (StatusCode, String)> {
    let shares = state
        .metadata_store
        .get_user_public_shares(user_id)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list shares: {error}"),
            )
        })?;

    let response = shares
        .into_iter()
        .filter_map(|entry| {
            let share = entry.share;
            let share_token = share.share_token?;

            Some(OwnedShareResponse {
                id: share.id,
                resource_id: entry.resource_id,
                resource_type: entry.resource_type,
                resource_name: entry.resource_name,
                share_token,
                permissions: share.permissions,
                password_protected: share.password_hash.is_some(),
                access_count: share.access_count,
                expires_at: share.expires_at,
                created_at: share.created_at,
            })
        })
        .collect();

    Ok(Json(response))
}

async fn revoke_share(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(share_id): axum::extract::Path<uuid::Uuid>,
    handlers::AuthenticatedUser { user_id }: handlers::AuthenticatedUser,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .share_service
        .revoke_share(share_id, user_id)
        .await
        .map_err(|error| match error {
            rustshare_core::services::ShareError::NotFound => {
                (StatusCode::NOT_FOUND, error.to_string())
            }
            rustshare_core::services::ShareError::NotFoundById(_) => {
                (StatusCode::NOT_FOUND, error.to_string())
            }
            rustshare_core::services::ShareError::PermissionDenied { .. } => {
                (StatusCode::FORBIDDEN, error.to_string())
            }
            rustshare_core::services::ShareError::FileNotFound(_) => {
                (StatusCode::NOT_FOUND, error.to_string())
            }
            rustshare_core::services::ShareError::Revoked => (StatusCode::GONE, error.to_string()),
            rustshare_core::services::ShareError::Expired => (StatusCode::GONE, error.to_string()),
            rustshare_core::services::ShareError::PasswordRequired => {
                (StatusCode::UNAUTHORIZED, error.to_string())
            }
            rustshare_core::services::ShareError::InvalidPassword => {
                (StatusCode::UNAUTHORIZED, error.to_string())
            }
            rustshare_core::services::ShareError::RecipientNotFound(_) => {
                (StatusCode::NOT_FOUND, error.to_string())
            }
            rustshare_core::services::ShareError::InsufficientPermission { .. } => {
                (StatusCode::FORBIDDEN, error.to_string())
            }
            rustshare_core::services::ShareError::CannotShareWithSelf => {
                (StatusCode::BAD_REQUEST, error.to_string())
            }
            rustshare_core::services::ShareError::ShareAlreadyExists(_) => {
                (StatusCode::CONFLICT, error.to_string())
            }
            rustshare_core::services::ShareError::CannotRemoveOwner => {
                (StatusCode::FORBIDDEN, error.to_string())
            }
            rustshare_core::services::ShareError::Database(_)
            | rustshare_core::services::ShareError::PasswordHash(_)
            | rustshare_core::services::ShareError::Jwt(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            ),
        })?;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_share_access_log(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(share_id): axum::extract::Path<uuid::Uuid>,
    axum::extract::Query(query): axum::extract::Query<ShareAccessLogQuery>,
    handlers::AuthenticatedUser { user_id }: handlers::AuthenticatedUser,
) -> Result<Json<Vec<ShareAccessLogResponse>>, (StatusCode, String)> {
    let requested_limit = query.limit.unwrap_or(50);
    let limit = requested_limit.clamp(1, 200);

    let entries = state
        .metadata_store
        .get_public_share_access_log(share_id, user_id, limit)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch share access log: {error}"),
            )
        })?;

    let response = entries
        .into_iter()
        .map(|entry| ShareAccessLogResponse {
            accessed_at: entry.accessed_at,
            action: entry.action,
            success: entry.success,
            actor_type: entry.actor_type,
            actor_label: entry.actor_label,
            ip_address: entry.ip_address,
            user_agent: entry.user_agent,
            share_session_id: entry.share_session_id,
            share_session_subject: entry.share_session_subject,
        })
        .collect();

    Ok(Json(response))
}

/// Login handler
async fn login(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Response, (StatusCode, String)> {
    if !oidc::password_login_enabled() {
        return Err((
            StatusCode::FORBIDDEN,
            "Password login is disabled for this deployment".to_string(),
        ));
    }

    // Find user
    let user = state
        .metadata_store
        .find_user_by_email(&req.email)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()))?;

    // Verify password
    let is_valid = PasswordHasher::verify(&req.password, &user.password_hash)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !is_valid {
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()));
    }

    // Reject disabled accounts
    if user.disabled_at.is_some() {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "account_disabled" })),
        )
            .into_response());
    }

    // Keep JWT generation temporarily for compatibility while the web app migrates to cookies.
    let token = state
        .jwt_manager
        .generate(user.id, user.email.clone())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let ip_address = middleware::extract_client_ip(&headers, None).map(|value| value.to_string());
    let session_token =
        create_user_session(&state, user.id, user_agent.clone(), ip_address.clone())
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    if let Err(error) = log_user_security_event(
        &state,
        rustshare_storage::UserSecurityEventRecord {
            user_id: user.id,
            event_type: "password_login",
            description: "Signed in with email and password",
            ip_address: ip_address.as_deref(),
            user_agent: user_agent.as_deref(),
            session_id: None,
        },
    )
    .await
    {
        tracing::warn!(
            "Failed to record password login security event: {:?}",
            error
        );
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_session_cookie(&session_token))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
    );

    Ok((
        response_headers,
        Json(LoginResponse {
            token,
            user: UserResponse {
                id: user.id.to_string(),
                email: user.email,
                display_name: user.display_name,
                is_admin: user.is_admin,
            },
        }),
    )
        .into_response())
}

async fn logout(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    if let Some(session_token) =
        extract_cookie_value(&headers, rustshare_auth::WEB_SESSION_COOKIE_NAME)
    {
        let token_hash = rustshare_auth::hash_web_session_token(&session_token);
        let session = state
            .metadata_store
            .find_user_session_by_token_hash(&token_hash)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let user_agent = headers
            .get(header::USER_AGENT)
            .and_then(|value| value.to_str().ok());
        let ip_address =
            middleware::extract_client_ip(&headers, None).map(|value| value.to_string());

        state
            .metadata_store
            .delete_user_session_by_token_hash(&token_hash)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some(session) = session {
            if let Err(error) = log_user_security_event(
                &state,
                rustshare_storage::UserSecurityEventRecord {
                    user_id: session.user_id,
                    event_type: "logout",
                    description: "Signed out of browser session",
                    ip_address: ip_address.as_deref(),
                    user_agent,
                    session_id: Some(session.id),
                },
            )
            .await
            {
                tracing::warn!("Failed to record logout security event: {:?}", error);
            }
        }
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_expired_session_cookie())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
    );

    Ok((response_headers, StatusCode::NO_CONTENT).into_response())
}

async fn log_user_security_event(
    state: &AppState,
    event: rustshare_storage::UserSecurityEventRecord<'_>,
) -> anyhow::Result<()> {
    state.metadata_store.create_user_security_event(event).await
}
