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

use rustshare_server::{bootstrap, metrics, middleware, openapi, routes};

pub use rustshare_server::{
    default_storage_quota_bytes, AppAiService, AppState, AppUploadService, AppUserShareService,
};

use anyhow::Result;
use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    response::IntoResponse,
    routing::{any, get},
    Json, Router,
};
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::info;
use utoipa::OpenApi;

#[tokio::main]
async fn main() -> Result<()> {
    // rustls 0.23 requires an explicit CryptoProvider when more than one
    // provider feature is present in the dependency graph. Mail (IMAP/SMTP)
    // and OIDC both use rustls for TLS; selecting aws-lc-rs at startup
    // prevents a panic on the first remote TLS handshake.
    tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();

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
        // Prometheus metrics endpoint. If METRICS_API_TOKEN is set, the scraper
        // must provide it as an Authorization header; otherwise it remains open.
        .route("/metrics", get(metrics::metrics_handler))
        // OpenAPI docs (no auth required for discovery)
        .merge(
            utoipa_swagger_ui::SwaggerUi::new("/api/docs")
                .url("/api/docs/openapi.json", openapi::ApiDoc::openapi()),
        )
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
        .merge(routes::chat_integration_routes())
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
        .merge(routes::module_routes())
        .merge(routes::mail_routes())
        .merge(routes::kanban_routes())
        .merge(routes::brainstorming_routes())
        .merge(routes::decision_routes())
        .merge(routes::meeting_routes())
        .merge(routes::standup_routes())
        .merge(routes::vault_sync_routes())
        .route("/api", any(api_not_found))
        .route("/api/{*path}", any(api_not_found))
        .with_state(state.clone())
        // Increase body size limit for file uploads (2GB)
        // This must be applied BEFORE other middleware layers
        .layer(DefaultBodyLimit::max(2048 * 1024 * 1024))
        .layer(axum::middleware::from_fn(middleware::csrf_middleware))
        // Refresh the CSRF cookie for sessions that pre-date the double-submit cookie.
        // This layer sits outside csrf_middleware so it can attach Set-Cookie to the
        // response (e.g., the bootstrap GET /me) even when the inner middleware would
        // otherwise reject a mutating request for missing the CSRF header.
        .layer(axum::middleware::from_fn(
            middleware::csrf_cookie_refresh_middleware,
        ))
        // Apply rate limiting middleware after state is set
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::rate_limit_middleware,
        ))
        // Tracing
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(middleware::metrics_middleware))
        .layer(axum::middleware::from_fn(
            middleware::security_headers_middleware,
        ))
        // Request-scoped tracing with correlation IDs (outermost layer)
        .layer(axum::middleware::from_fn(middleware::trace_middleware))
        // All non-API requests are served by the compiled SPA bundle.
        .fallback_service(frontend_service());

    // Start server
    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("{}:{}", host, port);

    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(state.shutdown_tx.clone()))
    .await?;

    Ok(())
}

async fn shutdown_signal(shutdown_tx: tokio::sync::broadcast::Sender<()>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, starting graceful shutdown");
    let _ = shutdown_tx.send(());
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
