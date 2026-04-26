use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use uuid::Uuid;
use sqlx::PgPool;

use crate::adapters;
use crate::middleware;
use crate::oidc_runtime::OidcRuntimeCache;
use crate::services;

use rustshare_storage::{EventStore, MetadataStore, ObjectStore};

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
