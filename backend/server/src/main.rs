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
mod oidc_runtime;
mod replication;
mod replication_handlers;
mod web_session;

use crate::handlers::{
    ensure_optional_seed_user, get_share_access_log, list_user_shares, login, logout, revoke_share,
};
use crate::oidc_runtime::{seed_oidc_config_from_env, OidcRuntimeCache};
use crate::replication::{spawn_replication_worker, ReplicationWorkerConfig};
use anyhow::Result;
use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    response::IntoResponse,
    routing::{any, delete, get, patch, post, put},
    Json, Router,
};
use rustshare_auth::{JwtManager, PasswordHasher};
use rustshare_core::{
    domain::User,
    events::EventBroadcaster,
    services::{
        AiService, ContentIndexer, FileService, FolderService, NotificationService,
        PermissionResolver, ShareService, SimpleEmbeddingGenerator, ThumbnailService,
        UserShareService, UserShareServiceDeps,
    },
};
use rustshare_crypto::SecretEncryptionKey;
use rustshare_infrastructure::repositories::{
    FileRepository, FolderRepository, NotificationRepository, PermissionResolverRepository,
    ShareRepository, UserRepository,
};
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use serde::Serialize;
use sqlx::PgPool;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::Mutex;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::info;
use uuid::Uuid;

type AppUserShareService = UserShareService<
    ShareRepository,
    UserRepository,
    FileRepository,
    FolderRepository,
    PermissionResolverRepository,
    NotificationRepository,
    EventStore,
>;

/// Type alias for AI service
type AppAiService = AiService<SimpleEmbeddingGenerator, PermissionResolverRepository>;

// Note: Upload service disabled due to trait mismatch between storage and core crates
pub type AppUploadService = rustshare_core::services::UploadService<
    rustshare_storage::repos::RustFsUploadSessionRepository,
    UploadObjectStoreAdapter,
    UploadMetadataStoreAdapter,
    EventStore,
>;

/// Adapter for ObjectStore to implement UploadObjectStore trait
#[derive(Clone)]
pub struct UploadObjectStoreAdapter {
    inner: Arc<ObjectStore>,
}

impl UploadObjectStoreAdapter {
    pub fn new(inner: Arc<ObjectStore>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl rustshare_core::services::UploadObjectStore for UploadObjectStoreAdapter {
    async fn put_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        data: bytes::Bytes,
    ) -> Result<(), rustshare_core::services::UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.inner
            .put(&key, data)
            .await
            .map_err(|e| rustshare_core::services::UploadError::Storage(e.to_string()))
    }

    async fn get_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
    ) -> Result<Option<bytes::Bytes>, rustshare_core::services::UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        match self.inner.get(&key).await {
            Ok(data) => Ok(Some(data)),
            Err(_) => Ok(None), // Chunk not found
        }
    }

    async fn delete_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
    ) -> Result<(), rustshare_core::services::UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.inner
            .delete(&key)
            .await
            .map_err(|e| rustshare_core::services::UploadError::Storage(e.to_string()))
    }

    async fn delete_session_chunks(
        &self,
        session_id: Uuid,
        total_chunks: u32,
    ) -> Result<(), rustshare_core::services::UploadError> {
        for chunk_index in 0..total_chunks {
            let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
            if let Err(e) = self.inner.delete(&key).await {
                    tracing::warn!(key = %key, error = %e, "failed to delete object during cleanup");
                }
        }
        Ok(())
    }

    async fn chunk_exists(
        &self,
        session_id: Uuid,
        chunk_index: u32,
    ) -> Result<bool, rustshare_core::services::UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.inner
            .exists(&key)
            .await
            .map_err(|e| rustshare_core::services::UploadError::Storage(e.to_string()))
    }

    async fn assemble_chunks(
        &self,
        session_id: Uuid,
        total_chunks: u32,
        final_key: &str,
    ) -> Result<(), rustshare_core::services::UploadError> {
        // Download all chunks and concatenate
        let mut assembled = Vec::new();
        for chunk_index in 0..total_chunks {
            let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
            let chunk_data = self
                .inner
                .get(&key)
                .await
                .map_err(|e| rustshare_core::services::UploadError::Storage(e.to_string()))?;
            assembled.extend_from_slice(&chunk_data);
        }

        // Upload assembled file
        self.inner
            .put(final_key, bytes::Bytes::from(assembled))
            .await
            .map_err(|e| rustshare_core::services::UploadError::Storage(e.to_string()))
    }
}

/// Adapter for MetadataStore to implement UploadMetadataStore trait
#[derive(Clone)]
pub struct UploadMetadataStoreAdapter {
    inner: Arc<MetadataStore>,
}

impl UploadMetadataStoreAdapter {
    pub fn new(inner: Arc<MetadataStore>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl rustshare_core::services::UploadMetadataStore for UploadMetadataStoreAdapter {
    async fn find_folder_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<rustshare_core::domain::Folder>, rustshare_core::services::UploadError> {
        self.inner
            .find_folder_by_id(id)
            .await
            .map_err(|e| rustshare_core::services::UploadError::Database(e.to_string()))
    }

    async fn create_file(
        &self,
        file: &rustshare_core::domain::File,
    ) -> Result<(), rustshare_core::services::UploadError> {
        self.inner
            .create_file(file)
            .await
            .map_err(|e| rustshare_core::services::UploadError::Database(e.to_string()))
    }

    async fn create_file_version(
        &self,
        _file: &rustshare_core::domain::File,
        version: &rustshare_core::domain::FileVersion,
    ) -> Result<(), rustshare_core::services::UploadError> {
        self.inner
            .create_file_version(version)
            .await
            .map_err(|e| rustshare_core::services::UploadError::Database(e.to_string()))
    }
}

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
    pub permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    pub notification_service: Arc<NotificationService<NotificationRepository>>,
    pub user_share_service: Arc<AppUserShareService>,
    pub ai_service: Option<Arc<AppAiService>>,
    // pub upload_service: Option<Arc<AppUploadService>>, // TODO: Fix upload service type issues
    pub rate_limit_config: Arc<middleware::RateLimitConfig>,
    pub secret_key: SecretEncryptionKey,
    pub oidc_runtime_cache: OidcRuntimeCache,
    pub poll_rate_limiter: Arc<Mutex<HashMap<String, Instant>>>,
    pub default_tenant_id: uuid::Uuid,
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
    let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
        PermissionResolverRepository::new(db_pool.clone()),
    )));

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

    // Initialize AI service
    let ai_service_enabled = std::env::var("RUSTSHARE_AI_ENABLED")
        .ok()
        .and_then(|s| s.parse::<bool>().ok())
        .unwrap_or(true);

    let ai_service: Option<Arc<AppAiService>> = if ai_service_enabled {
        let embedding_generator = Arc::new(SimpleEmbeddingGenerator::new());
        let content_indexer = Arc::new(ContentIndexer::new(embedding_generator));
        Some(Arc::new(AiService::new(
            content_indexer,
            Arc::clone(&permission_resolver),
        )))
    } else {
        None
    };

    if ai_service.is_some() {
        info!("AI service initialized");
    } else {
        info!("AI service disabled");
    }

    // Initialize upload service for resumable uploads
    // Use local filesystem document store for upload session metadata
    let upload_doc_store_path = std::env::var("RUSTSHARE_UPLOAD_STORE_PATH")
        .unwrap_or_else(|_| "/tmp/rustshare-uploads".to_string());

    let upload_backend_config = rustshare_storage::metadata_v2::MetadataBackendConfig {
        base_prefix: "apps/rustshare".to_string(),
        namespace: "uploads".to_string(),
        enable_optimistic_concurrency: true,
        fallback_to_leases: true,
    };

    let upload_doc_store: Arc<dyn rustshare_storage::metadata_v2::MetadataDocumentStore> = Arc::new(
        rustshare_storage::metadata_v2::stores::LocalFsDocumentStore::new(
            std::path::PathBuf::from(&upload_doc_store_path),
            upload_backend_config,
        ),
    );

    let upload_session_repo = rustshare_storage::repos::RustFsUploadSessionRepository::new(
        upload_doc_store,
        "apps/rustshare".to_string(),
        "uploads".to_string(),
    );

    let _upload_service = Arc::new(AppUploadService::new(
        Arc::new(upload_session_repo),
        Arc::new(UploadObjectStoreAdapter::new(Arc::clone(&object_store))),
        Arc::new(UploadMetadataStoreAdapter::new(Arc::clone(&metadata_store))),
        Arc::clone(&event_store),
        Arc::clone(&broadcaster),
    ));

    info!(
        "Upload service initialized with store path: {}",
        upload_doc_store_path
    );

    // Initialize rate limiting configuration
    let rate_limit_config = Arc::new(middleware::RateLimitConfig::new());

    info!("Rate limiting initialized");

    // TODO: Fix chat_integration compilation errors
    // // Initialize chat integration service
    // let chat_webhook_secret = std::env::var("RUSTSHARE_CHAT_WEBHOOK_SECRET")
    //     .unwrap_or_else(|_| "default_chat_webhook_secret_change_in_production".to_string());
    // let webhook_dispatcher = Arc::new(HttpWebhookDispatcher::new());
    // let chat_integration_service = Arc::new(ChatIntegrationService::new(
    //     Arc::clone(&metadata_store),
    //     Arc::clone(&event_store),
    //     Arc::clone(&broadcaster),
    //     chat_webhook_secret,
    //     webhook_dispatcher,
    // ));

    // info!("Chat integration service initialized");

    // Parse default tenant ID from env or use nil UUID
    let default_tenant_id = std::env::var("RUSTSHARE_DEFAULT_TENANT_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(uuid::Uuid::nil);

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
            default_tenant_id,
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
        default_tenant_id,
    )
    .await?;

    // Load secret encryption key
    let secret_key = SecretEncryptionKey::from_env()
        .map_err(|e| anyhow::anyhow!("Secret encryption key error: {}", e))?;

    if seed_oidc_config_from_env(&db_pool, &secret_key).await? {
        info!("Seeded initial OIDC config from environment bootstrap values");
    }

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
        ai_service,
        // upload_service: Some(upload_service),
        // upload_service: None,
        rate_limit_config,
        secret_key,
        oidc_runtime_cache: OidcRuntimeCache::new(),
        poll_rate_limiter: Arc::new(Mutex::new(HashMap::new())),
        default_tenant_id,
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
        .route("/api/v1/user/devices", get(handlers::devices::list_devices))
        .route(
            "/api/v1/user/devices/{id}",
            delete(handlers::devices::revoke_device),
        )
        // File routes (Task 15-19)
        .route("/api/v1/files", get(handlers::list_files))
        .route("/api/v1/files/starred", get(handlers::list_starred_items))
        .route("/api/v1/files/deleted", get(handlers::list_deleted_items))
        .route("/api/v1/files/upload", post(handlers::upload_file))
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
        // Upload session routes (TODO-004: Resumable uploads)
        // Upload endpoints disabled - TODO: Fix upload service type issues
        // .route("/api/v1/uploads/sessions", get(handlers::list_upload_sessions))
        // .route("/api/v1/uploads/sessions", post(handlers::create_upload_session))
        // .route("/api/v1/uploads/sessions/{id}", get(handlers::get_upload_session_status))
        // .route("/api/v1/uploads/sessions/{id}/chunks/{index}", put(handlers::upload_chunk))
        // .route("/api/v1/uploads/sessions/{id}/complete", post(handlers::complete_upload))
        // .route("/api/v1/uploads/sessions/{id}", delete(handlers::abort_upload_session))
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
        // Chat integration routes (TODO: Uncomment when chat_integration module is fixed)
        // .route(
        //     "/api/v1/integrations/chat/unfurl",
        //     post(handlers::unfurl_link),
        // )
        // .route(
        //     "/api/v1/integrations/chat/unfurl/public",
        //     post(handlers::unfurl_link_public),
        // )
        // .route(
        //     "/api/v1/integrations/chat/events",
        //     post(handlers::receive_chat_event),
        // )
        // .route(
        //     "/api/v1/integrations/webhooks/dispatch",
        //     post(handlers::dispatch_webhooks),
        // )
        // .route(
        //     "/api/v1/admin/integrations/chat/webhooks",
        //     get(handlers::list_chat_webhooks),
        // )
        // .route(
        //     "/api/v1/admin/integrations/chat/webhooks",
        //     post(handlers::register_chat_webhook),
        // )
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
        .route("/api/v1/folders/{id}/star", patch(handlers::toggle_folder_star))
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
        // AI endpoints (TODO-001)
        .route("/api/v1/ai/search", post(handlers::semantic_search))
        .route("/api/v1/ai/summarize", post(handlers::summarize_file))
        .route("/api/v1/ai/ask", post(handlers::ask_question))
        // TODO: Fix search_service compilation errors
        // // Search endpoint (Task Phase 1)
        // .route("/api/v1/search", get(handlers::search))
        // WebSocket sync endpoint (Task Phase 3A)
        .route("/api/ws", get(handlers::sync_handler))
        // HTTP Sync API endpoints (Desktop Client Sync)
        .route("/api/v1/sync/cursor", get(handlers::get_sync_cursor))
        .route("/api/v1/sync/delta", get(handlers::get_sync_delta))
        .route("/api", any(api_not_found))
        .route("/api/{*path}", any(api_not_found))
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
// test
