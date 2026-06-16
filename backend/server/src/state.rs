use axum::extract::FromRef;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::adapters;
use crate::handlers::collab::CollabRooms;
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

/// Type alias for chat integration service
pub type AppChatIntegrationService = rustshare_core::services::ChatIntegrationService<
    rustshare_storage::MetadataStore,
    rustshare_storage::EventStore,
    rustshare_core::services::HttpWebhookDispatcher,
>;

// Note: Upload service disabled due to trait mismatch between storage and core crates
pub type AppUploadService = rustshare_core::services::UploadService<
    rustshare_storage::repos::RustFsUploadSessionRepository,
    adapters::UploadObjectStoreAdapter,
    adapters::UploadMetadataStoreAdapter,
    EventStore,
>;

// ---------------------------------------------------------------------------
// Focused sub-states — handlers can extract these via `FromRef` instead of
// depending on the entire `AppState` god-object.
// ---------------------------------------------------------------------------

/// Database and storage infrastructure state.
#[derive(Clone)]
pub struct DatabaseState {
    pub db_pool: PgPool,
    pub metadata_store: Arc<MetadataStore>,
    pub event_store: Arc<EventStore>,
    pub object_store: Arc<ObjectStore>,
}

/// Domain service layer state.
#[derive(Clone)]
pub struct ServiceState {
    pub file_service: Arc<
        rustshare_core::services::FileService<
            EventStore,
            MetadataStore,
            ObjectStore,
            rustshare_infrastructure::repositories::PermissionResolverRepository,
        >,
    >,
    pub folder_service: Arc<
        rustshare_core::services::FolderService<
            EventStore,
            MetadataStore,
            rustshare_infrastructure::repositories::PermissionResolverRepository,
        >,
    >,
    pub share_service: Arc<
        rustshare_core::services::ShareService<
            EventStore,
            MetadataStore,
            rustshare_auth::JwtManager,
            rustshare_storage::repos::ShareNotificationRepoImpl,
        >,
    >,
    pub thumbnail_service: Arc<rustshare_core::services::ThumbnailService<ObjectStore>>,
    pub permission_resolver: Arc<
        rustshare_core::services::PermissionResolver<
            rustshare_infrastructure::repositories::PermissionResolverRepository,
        >,
    >,
    pub notification_service: Arc<
        rustshare_core::services::NotificationService<
            rustshare_infrastructure::repositories::NotificationRepository,
        >,
    >,
    pub user_share_service: Arc<AppUserShareService>,
    pub ai_service: Option<Arc<AppAiService>>,
    pub upload_service: Option<Arc<AppUploadService>>,
    pub note_service: Arc<services::note_service::NoteService>,
    pub decision_service: Arc<services::decision_service::DecisionService>,
    pub meeting_service: Arc<services::meeting_service::MeetingService>,
    pub standup_service: Arc<services::standup_service::StandupService>,
    pub module_service: Arc<services::module_service::ModuleService>,
    pub template_service: Arc<services::template_service::TemplateService>,
    pub kanban_service: Arc<services::kanban_service::KanbanService>,
    pub brainstorming_service: Arc<services::brainstorming_service::BrainstormingService>,
    pub user_repository: Arc<rustshare_infrastructure::repositories::UserRepository>,
    pub vault_sync_service:
        Arc<rustshare_core::services::VaultSyncService<MetadataStore, ObjectStore>>,
}

/// Application configuration and runtime state.
#[derive(Clone)]
pub struct AppConfigState {
    pub jwt_manager: Arc<rustshare_auth::JwtManager>,
    pub broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
    pub rate_limit_config: Arc<middleware::RateLimitConfig>,
    pub secret_key: rustshare_crypto::SecretEncryptionKey,
    pub oidc_runtime_cache: OidcRuntimeCache,
    pub poll_rate_limiter: Arc<Mutex<HashMap<String, Instant>>>,
    pub default_tenant_id: Uuid,
    pub public_base_url: String,
}

/// Application state shared across handlers.
///
/// Handlers that only need a subset of state should extract `DatabaseState`,
/// `ServiceState`, or `AppConfigState` via `State<T>` + `FromRef` instead of
/// taking the entire `AppState`.
#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub metadata_store: Arc<MetadataStore>,
    pub event_store: Arc<EventStore>,
    pub object_store: Arc<ObjectStore>,
    pub jwt_manager: Arc<rustshare_auth::JwtManager>,
    pub broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
    pub file_service: Arc<
        rustshare_core::services::FileService<
            EventStore,
            MetadataStore,
            ObjectStore,
            rustshare_infrastructure::repositories::PermissionResolverRepository,
        >,
    >,
    pub folder_service: Arc<
        rustshare_core::services::FolderService<
            EventStore,
            MetadataStore,
            rustshare_infrastructure::repositories::PermissionResolverRepository,
        >,
    >,
    pub share_service: Arc<
        rustshare_core::services::ShareService<
            EventStore,
            MetadataStore,
            rustshare_auth::JwtManager,
            rustshare_storage::repos::ShareNotificationRepoImpl,
        >,
    >,
    pub thumbnail_service: Arc<rustshare_core::services::ThumbnailService<ObjectStore>>,
    pub permission_resolver: Arc<
        rustshare_core::services::PermissionResolver<
            rustshare_infrastructure::repositories::PermissionResolverRepository,
        >,
    >,
    pub notification_service: Arc<
        rustshare_core::services::NotificationService<
            rustshare_infrastructure::repositories::NotificationRepository,
        >,
    >,
    pub user_share_service: Arc<AppUserShareService>,
    pub ai_service: Option<Arc<AppAiService>>,
    pub upload_service: Option<Arc<AppUploadService>>,
    pub rate_limit_config: Arc<middleware::RateLimitConfig>,
    pub secret_key: rustshare_crypto::SecretEncryptionKey,
    pub oidc_runtime_cache: OidcRuntimeCache,
    pub poll_rate_limiter: Arc<Mutex<HashMap<String, Instant>>>,
    pub default_tenant_id: Uuid,
    pub note_service: Arc<services::note_service::NoteService>,
    pub decision_service: Arc<services::decision_service::DecisionService>,
    pub meeting_service: Arc<services::meeting_service::MeetingService>,
    pub standup_service: Arc<services::standup_service::StandupService>,
    pub module_service: Arc<services::module_service::ModuleService>,
    pub template_service: Arc<services::template_service::TemplateService>,
    pub kanban_service: Arc<services::kanban_service::KanbanService>,
    pub brainstorming_service: Arc<services::brainstorming_service::BrainstormingService>,
    pub user_repository: Arc<rustshare_infrastructure::repositories::UserRepository>,
    pub public_base_url: String,
    pub collab_rooms: Arc<CollabRooms>,
    pub vault_sync_service:
        Arc<rustshare_core::services::VaultSyncService<MetadataStore, ObjectStore>>,
    pub chat_integration_service: Arc<AppChatIntegrationService>,
    pub shutdown_tx: broadcast::Sender<()>,
    pub prometheus_handle: metrics_exporter_prometheus::PrometheusHandle,
}

impl AsRef<PgPool> for AppState {
    fn as_ref(&self) -> &PgPool {
        &self.db_pool
    }
}

impl FromRef<AppState> for DatabaseState {
    fn from_ref(state: &AppState) -> DatabaseState {
        DatabaseState {
            db_pool: state.db_pool.clone(),
            metadata_store: state.metadata_store.clone(),
            event_store: state.event_store.clone(),
            object_store: state.object_store.clone(),
        }
    }
}

impl FromRef<AppState> for ServiceState {
    fn from_ref(state: &AppState) -> ServiceState {
        ServiceState {
            file_service: state.file_service.clone(),
            folder_service: state.folder_service.clone(),
            share_service: state.share_service.clone(),
            thumbnail_service: state.thumbnail_service.clone(),
            permission_resolver: state.permission_resolver.clone(),
            notification_service: state.notification_service.clone(),
            user_share_service: state.user_share_service.clone(),
            ai_service: state.ai_service.clone(),
            upload_service: state.upload_service.clone(),
            note_service: state.note_service.clone(),
            decision_service: state.decision_service.clone(),
            meeting_service: state.meeting_service.clone(),
            standup_service: state.standup_service.clone(),
            module_service: state.module_service.clone(),
            template_service: state.template_service.clone(),
            kanban_service: state.kanban_service.clone(),
            brainstorming_service: state.brainstorming_service.clone(),
            user_repository: state.user_repository.clone(),
            vault_sync_service: state.vault_sync_service.clone(),
        }
    }
}

impl FromRef<AppState> for AppConfigState {
    fn from_ref(state: &AppState) -> AppConfigState {
        AppConfigState {
            jwt_manager: state.jwt_manager.clone(),
            broadcaster: state.broadcaster.clone(),
            rate_limit_config: state.rate_limit_config.clone(),
            secret_key: state.secret_key.clone(),
            oidc_runtime_cache: state.oidc_runtime_cache.clone(),
            poll_rate_limiter: state.poll_rate_limiter.clone(),
            default_tenant_id: state.default_tenant_id,
            public_base_url: state.public_base_url.clone(),
        }
    }
}
