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

mod adapters;
mod bootstrap;
mod handlers;
mod middleware;
mod oidc;
mod oidc_runtime;
mod replication;
mod replication_handlers;
mod routes;
mod services;
mod trash_cleanup;
mod web_session;

pub use bootstrap::default_storage_quota_bytes;

use crate::oidc_runtime::OidcRuntimeCache;
use anyhow::Result;
use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    response::IntoResponse,
    routing::{any},
    Json, Router,
};
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use serde::Serialize;
use sqlx::PgPool;
use std::{path::PathBuf, sync::Arc};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::info;

#[allow(deprecated)]
pub type AppUserShareService = rustshare_core::services::UserShareService<
    rustshare_infrastructure::repositories::ShareRepository,
    rustshare_infrastructure::repositories::UserRepository,
    rustshare_infrastructure::repositories::FileRepository,
    rustshare_infrastructure::repositories::FolderRepository,
    rustshare_infrastructure::repositories::PermissionResolverRepository,
    rustshare_infrastructure::repositories::NotificationRepository,
    EventStore,
>;

/// Type alias for AI service
pub type AppAiService = rustshare_core::services::AiService<
    rustshare_core::services::SimpleEmbeddingGenerator,
    rustshare_infrastructure::repositories::PermissionResolverRepository,
>;

// Note: Upload service disabled due to trait mismatch between storage and core crates
pub type AppUploadService = rustshare_core::services::UploadService<
    rustshare_storage::repos::RustFsUploadSessionRepository,
    adapters::UploadObjectStoreAdapter,
    adapters::UploadMetadataStoreAdapter,
    EventStore,
>;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub metadata_store: Arc<MetadataStore>,
    pub event_store: Arc<EventStore>,
    pub object_store: Arc<ObjectStore>,
    pub jwt_manager: Arc<rustshare_auth::JwtManager>,
    pub broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
    pub file_service:
        Arc<rustshare_core::services::FileService<EventStore, MetadataStore, ObjectStore, rustshare_infrastructure::repositories::PermissionResolverRepository>>,
    pub folder_service: Arc<rustshare_core::services::FolderService<EventStore, MetadataStore, rustshare_infrastructure::repositories::PermissionResolverRepository>>,
    pub share_service:
        Arc<rustshare_core::services::ShareService<EventStore, MetadataStore, rustshare_auth::JwtManager, rustshare_storage::repos::ShareNotificationRepoImpl>>,
    pub thumbnail_service: Arc<rustshare_core::services::ThumbnailService<ObjectStore>>,
    pub permission_resolver: Arc<rustshare_core::services::PermissionResolver<rustshare_infrastructure::repositories::PermissionResolverRepository>>,
    pub notification_service: Arc<rustshare_core::services::NotificationService<rustshare_infrastructure::repositories::NotificationRepository>>,
    pub user_share_service: Arc<AppUserShareService>,
    pub ai_service: Option<Arc<AppAiService>>,
    pub upload_service: Option<Arc<AppUploadService>>,
    pub rate_limit_config: Arc<middleware::RateLimitConfig>,
    pub secret_key: rustshare_crypto::SecretEncryptionKey,
    pub oidc_runtime_cache: OidcRuntimeCache,
    pub poll_rate_limiter: Arc<tokio::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>>,
    pub default_tenant_id: uuid::Uuid,
    pub note_service: Arc<services::note_service::NoteService>,
    pub public_base_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let state = bootstrap::init_app().await?;

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
        .merge(routes::health_routes())
        .merge(routes::auth_routes())
        .merge(routes::device_auth_routes())
        .merge(routes::device_management_routes())
        .merge(routes::feature_routes())
        .merge(routes::file_routes())
        .merge(routes::upload_routes())
        .merge(routes::note_routes())
        .merge(routes::note_public_routes())
        .merge(routes::replication_routes())
        .merge(routes::admin_routes())
        .merge(routes::scim_routes())
        .merge(routes::folder_routes())
        .merge(routes::share_routes())
        .merge(routes::user_routes())
        .merge(routes::group_routes())
        .merge(routes::notification_routes())
        .merge(routes::public_share_routes())
        .merge(routes::invite_routes())
        .merge(routes::ai_routes())
        .merge(routes::sync_routes())
        .merge(routes::trash_routes())
        .route("/api", any(api_not_found))
        .route("/api/{*path}", any(api_not_found))
        .with_state(state.clone())
        // Increase body size limit for file uploads (2GB)
        // This must be applied BEFORE other middleware layers
        .layer(DefaultBodyLimit::max(2048 * 1024 * 1024))
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

/// Health check endpoint
pub(crate) async fn health_check() -> Json<HealthResponse> {
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
// test
