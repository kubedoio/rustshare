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
mod services;
mod trash_cleanup;
mod web_session;

pub use bootstrap::default_storage_quota_bytes;

use crate::handlers::{
    get_share_access_log, list_user_shares, login, logout, revoke_share,
};
use crate::oidc_runtime::OidcRuntimeCache;
use anyhow::Result;
use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    response::IntoResponse,
    routing::{any, delete, get, patch, post, put},
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
        .route("/api/v1/user/devices", get(handlers::devices::list_devices))
        .route(
            "/api/v1/user/devices/{id}",
            delete(handlers::devices::revoke_device),
        )
        .route("/api/v1/features", get(handlers::get_features))
        // File routes (Task 15-19)
        .route("/api/v1/files", get(handlers::list_files))
        .route("/api/v1/files/starred", get(handlers::list_starred_items))
        .route("/api/v1/files/deleted", get(handlers::list_deleted_items))
        .route("/api/v1/trash/summary", get(handlers::get_trash_summary))
        .route("/api/v1/trash/empty", delete(handlers::empty_trash))
        .route(
            "/api/v1/files/upload",
            post(handlers::upload_file).layer(DefaultBodyLimit::disable()),
        )
        // Resumable upload routes
        .route(
            "/api/v1/uploads/sessions",
            post(handlers::upload::create_upload_session),
        )
        .route(
            "/api/v1/uploads/sessions",
            get(handlers::upload::list_upload_sessions),
        )
        .route(
            "/api/v1/uploads/sessions/{id}",
            get(handlers::upload::get_upload_session_status),
        )
        .route(
            "/api/v1/uploads/sessions/{id}",
            delete(handlers::upload::abort_upload_session),
        )
        .route(
            "/api/v1/uploads/sessions/{id}/chunks/{index}",
            put(handlers::upload::upload_chunk),
        )
        .route(
            "/api/v1/uploads/sessions/{id}/complete",
            post(handlers::upload::complete_upload),
        )
        .route("/api/v1/files/{id}", get(handlers::get_file))
        .route("/api/v1/files/{id}", put(handlers::update_file))
        .route("/api/v1/files/{id}", delete(handlers::delete_file))
        .route("/api/v1/files/{id}/star", patch(handlers::toggle_file_star))
        .route(
            "/api/v1/files/{id}/restore-from-trash",
            post(handlers::restore_file_from_trash),
        )
        .route(
            "/api/v1/files/{id}/permanent",
            delete(handlers::permanently_delete_file),
        )
        .route("/api/v1/files/{id}/download", get(handlers::download_file))
        .route(
            "/api/v1/files/{id}/content",
            get(handlers::download_file_content),
        )
        .route("/api/v1/files/{id}/preview", get(handlers::preview_file))
        .route(
            "/api/v1/files/{id}/versions",
            get(handlers::get_file_versions),
        )
        .route(
            "/api/v1/files/{id}/restore",
            post(handlers::restore_file_version),
        )
        .route("/api/v1/files/{id}/move", post(handlers::move_file))
        .route("/api/v1/files/{id}/rename", post(handlers::rename_file))
        .route(
            "/api/v1/files/{id}/thumbnail",
            get(handlers::get_file_thumbnail),
        )
        .route("/api/v1/files/{id}/edit", post(handlers::edit_file))
        // Note routes (MVP-1)
        .route("/api/v1/notes", post(handlers::create_note))
        .route("/api/v1/notes", get(handlers::list_notes))
        .route("/api/v1/notes/recent", get(handlers::list_recent_notes))
        .route("/api/v1/notes/{id}", get(handlers::get_note))
        .route("/api/v1/notes/{id}", put(handlers::save_note))
        .route("/api/v1/notes/{id}/rename", post(handlers::rename_note))
        .route("/api/v1/notes/{id}/move", post(handlers::move_note))
        .route(
            "/api/v1/notes/{id}/visibility",
            post(handlers::toggle_visibility),
        )
        .route("/api/v1/notes/{id}", delete(handlers::delete_note))
        .route(
            "/api/v1/public/notes/{share_id}",
            get(handlers::get_public_note),
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
            "/api/v1/files/{id}/replication",
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
            "/api/v1/admin/users/{id}",
            get(handlers::admin::users::get_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}",
            patch(handlers::admin::users::update_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}/disable",
            post(handlers::admin::users::disable_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}/enable",
            post(handlers::admin::users::enable_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}",
            delete(handlers::admin::users::delete_admin_user),
        )
        // Admin audit log (Task 4)
        .route(
            "/api/v1/admin/audit",
            get(handlers::admin::audit::list_audit_log),
        )
        // Admin workflows (Task 4)
        .route(
            "/api/v1/admin/workflows",
            get(handlers::admin::workflows::list_workflows),
        )
        .route(
            "/api/v1/admin/workflows/{id}",
            get(handlers::admin::workflows::get_workflow),
        )
        .route(
            "/api/v1/admin/workflows/{id}",
            put(handlers::admin::workflows::update_workflow),
        )
        .route(
            "/api/v1/admin/workflows/{id}/enable",
            post(handlers::admin::workflows::enable_workflow),
        )
        .route(
            "/api/v1/admin/workflows/{id}/disable",
            post(handlers::admin::workflows::disable_workflow),
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
            "/api/v1/admin/groups/{id}",
            get(handlers::admin::groups::get_group),
        )
        .route(
            "/api/v1/admin/groups/{id}",
            patch(handlers::admin::groups::update_group),
        )
        .route(
            "/api/v1/admin/groups/{id}",
            delete(handlers::admin::groups::delete_group),
        )
        .route(
            "/api/v1/admin/groups/{id}/members",
            post(handlers::admin::groups::add_member),
        )
        .route(
            "/api/v1/admin/groups/{id}/members/{user_id}",
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
        // Admin security config
        .route(
            "/api/v1/admin/config/security",
            get(handlers::admin::config::get_security_config),
        )
        .route(
            "/api/v1/admin/config/security",
            put(handlers::admin::config::update_security_config),
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
            "/api/v1/admin/integrations/webhooks/{id}",
            patch(handlers::admin::webhooks::update_webhook),
        )
        .route(
            "/api/v1/admin/integrations/webhooks/{id}",
            delete(handlers::admin::webhooks::delete_webhook),
        )
        .route(
            "/api/v1/admin/integrations/webhooks/{id}/test",
            post(handlers::admin::webhooks::test_webhook),
        )
        // SCIM provisioning endpoints (webhook-style, not full RFC 7644)
        .route("/api/v1/scim/users", post(handlers::scim::provision_user))
        .route(
            "/api/v1/scim/users/{external_id}",
            delete(handlers::scim::deprovision_user),
        )
        .route("/api/v1/scim/groups", post(handlers::scim::provision_group))
        .route(
            "/api/v1/scim/groups/{external_id}",
            delete(handlers::scim::delete_group),
        )
        // SCIM v2 REST API endpoints (RFC 7643/7644 full compliance)
        .route(
            "/scim/v2/Users",
            get(handlers::scim_v2::list_users).post(handlers::scim_v2::create_user),
        )
        .route(
            "/scim/v2/Users/{id}",
            get(handlers::scim_v2::get_user)
                .put(handlers::scim_v2::update_user)
                .patch(handlers::scim_v2::patch_user)
                .delete(handlers::scim_v2::delete_user),
        )
        .route(
            "/scim/v2/Groups",
            get(handlers::scim_v2::list_groups).post(handlers::scim_v2::create_group),
        )
        .route(
            "/scim/v2/Groups/{id}",
            get(handlers::scim_v2::get_group)
                .put(handlers::scim_v2::update_group)
                .patch(handlers::scim_v2::patch_group)
                .delete(handlers::scim_v2::delete_group),
        )
        .route(
            "/scim/v2/ServiceProviderConfig",
            get(handlers::scim_v2::get_service_provider_config),
        )
        .route(
            "/scim/v2/ResourceTypes",
            get(handlers::scim_v2::get_resource_types),
        )
        .route("/scim/v2/Schemas", get(handlers::scim_v2::get_schemas))
        // Folder routes (Task 20-22)
        // NOTE: More specific routes (with literal path segments) must come BEFORE parameterized routes
        .route("/api/v1/folders", post(handlers::create_folder))
        .route(
            "/api/v1/folders/root/contents",
            get(handlers::get_root_contents),
        )
        .route("/api/v1/folders/tree", get(handlers::get_folder_tree))
        .route(
            "/api/v1/folders/{id}/contents",
            get(handlers::get_folder_contents),
        )
        .route(
            "/api/v1/folders/{id}/star",
            patch(handlers::toggle_folder_star),
        )
        .route(
            "/api/v1/folders/{id}/restore-from-trash",
            post(handlers::restore_folder_from_trash),
        )
        .route(
            "/api/v1/folders/{id}/permanent",
            delete(handlers::permanently_delete_folder),
        )
        .route("/api/v1/folders/{id}/move", post(handlers::move_folder))
        .route("/api/v1/folders/{id}/rename", post(handlers::rename_folder))
        .route("/api/v1/folders/{id}", get(handlers::get_folder))
        .route("/api/v1/folders/{id}", delete(handlers::delete_folder))
        // Share routes (Task 9)
        .route(
            "/api/v1/files/{file_id}/shares",
            post(handlers::create_public_file_share),
        )
        .route(
            "/api/v1/folders/{folder_id}/shares",
            post(handlers::create_public_folder_share),
        )
        .route(
            "/api/v1/files/{file_id}/shares",
            get(handlers::list_public_file_shares),
        )
        .route(
            "/api/v1/folders/{folder_id}/shares",
            get(handlers::list_public_folder_shares),
        )
        .route("/api/v1/shares", get(list_user_shares))
        .route("/api/v1/shares/{id}/access-log", get(get_share_access_log))
        .route("/api/v1/shares/{id}", delete(revoke_share))
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
            "/api/users/me/sessions/{id}",
            delete(handlers::delete_user_session),
        )
        .route(
            "/api/v1/users/me/sessions/{id}",
            delete(handlers::delete_user_session),
        )
        .route(
            "/api/v1/me/sessions/{id}",
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
        .route("/api/v1/users/me/profile", get(handlers::get_profile))
        .route("/api/v1/users/me/profile", patch(handlers::update_profile))
        .route(
            "/api/v1/users/me/trash-retention",
            patch(handlers::update_trash_retention),
        )
        // Avatar routes (Task 18)
        .route(
            "/api/v1/users/me/avatar",
            post(handlers::upload_avatar).delete(handlers::delete_avatar),
        )
        .route("/api/v1/users/{id}/avatar", get(handlers::get_avatar))
        // Internal user share routes
        .route(
            "/api/v1/files/{id}/share",
            post(handlers::create_file_share),
        )
        .route(
            "/api/v1/folders/{id}/share",
            post(handlers::create_folder_share),
        )
        .route(
            "/api/v1/shares/received",
            get(handlers::list_received_shares),
        )
        .route(
            "/api/v1/files/{id}/recipients",
            get(handlers::list_file_recipients),
        )
        .route(
            "/api/v1/folders/{id}/recipients",
            get(handlers::list_folder_recipients),
        )
        .route(
            "/api/v1/shares/{id}/permission",
            put(handlers::update_recipient_permission),
        )
        .route(
            "/api/v1/shares/{id}/recipient",
            delete(handlers::remove_recipient),
        )
        .route(
            "/api/v1/shares/folders/{id}/contents",
            get(handlers::get_user_shared_folder_contents),
        )
        .route(
            "/api/v1/shares/folders/{id}/tree",
            get(handlers::get_user_shared_folder_tree),
        )
        // Group sharing routes
        .route("/api/v1/groups/my", get(handlers::list_my_groups))
        .route("/api/v1/groups/my/{id}", get(handlers::get_my_group))
        .route(
            "/api/v1/files/{id}/share/group",
            post(handlers::create_file_group_share),
        )
        .route(
            "/api/v1/files/{id}/share/groups",
            get(handlers::list_file_group_shares),
        )
        .route(
            "/api/v1/folders/{id}/share/group",
            post(handlers::create_folder_group_share),
        )
        .route(
            "/api/v1/folders/{id}/share/groups",
            get(handlers::list_folder_group_shares),
        )
        .route(
            "/api/v1/shares/{id}/group",
            delete(handlers::revoke_group_share),
        )
        .route(
            "/api/v1/shares/{id}/group/permission",
            put(handlers::update_group_share_permission),
        )
        .route("/api/v1/notifications", get(handlers::list_notifications))
        .route(
            "/api/v1/notifications/unread-count",
            get(handlers::count_unread_notifications),
        )
        .route(
            "/api/v1/notifications/{id}/read",
            put(handlers::mark_notification_read),
        )
        .route(
            "/api/v1/notifications/{id}",
            delete(handlers::delete_notification),
        )
        // Public share routes (Task 10 - no authentication required for session creation and info)
        .route(
            "/api/v1/public/share/{token}/session",
            post(handlers::create_session),
        )
        .route(
            "/api/v1/public/share/{token}/info",
            get(handlers::get_share_info),
        )
        .route(
            "/api/v1/public/share/{token}/file",
            get(handlers::download_shared_file),
        )
        .route(
            "/api/v1/public/share/{token}/folder/contents",
            get(handlers::get_shared_folder_contents),
        )
        .route(
            "/api/v1/public/share/{token}/folder/files/{file_id}",
            get(handlers::download_shared_folder_file),
        )
        .route(
            "/api/v1/public/share/{token}/folder/upload",
            post(handlers::upload_shared_folder_file),
        )
        .route("/api/v1/invites", post(handlers::create_invite))
        .route("/api/v1/invites/{token}", get(handlers::get_invite))
        .route(
            "/api/v1/invites/{token}/accept",
            post(handlers::accept_invite),
        )
        // AI endpoints (TODO-001)
        .route("/api/v1/ai/search", post(handlers::semantic_search))
        .route("/api/v1/ai/summarize", post(handlers::summarize_file))
        .route("/api/v1/ai/ask", post(handlers::ask_question))
        // WebSocket sync endpoint (Task Phase 3A)
        .route("/api/ws", get(handlers::sync_handler))
        // HTTP Sync API endpoints (Desktop Client Sync)
        .route("/api/v1/sync/cursor", get(handlers::get_sync_cursor))
        .route("/api/v1/sync/delta", get(handlers::get_sync_delta))
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
// test
