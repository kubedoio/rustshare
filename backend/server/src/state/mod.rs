//! Application state for zero-PostgreSQL RustShare
//!
//! This module provides the application state structure that replaces
//! the PostgreSQL-dependent AppState with a RustFS-based implementation.

use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::time::Instant;

use rustshare_auth::JwtManager;
use rustshare_core::events::EventBroadcaster;
use rustshare_crypto::SecretEncryptionKey;
use rustshare_storage::{
    coordination::CoordinationStore,
    metadata_v2::{MetadataDocumentStore, EventLogStore, MetadataBackendConfig},
    object_store::ObjectStore,
    repos::{
        PathBuilder, UserRepository, DeviceRepository, GroupRepository, 
        AuditRepository, ConfigRepository, PairingRepository, WebhookRepository,
        NotificationRepository, ShareRepository,
        RustFsUserRepository, RustFsDeviceRepository, RustFsGroupRepository,
        RustFsAuditRepository, RustFsConfigRepository, RustFsPairingRepository,
        RustFsWebhookRepository, RustFsNotificationRepository, RustFsShareRepository,
    },
    session::SessionManager,
};

pub mod profile;
pub use profile::{RuntimeProfile, ProfileConfig};

/// Zero-PostgreSQL application state
///
/// This struct holds all the components needed by the application
/// without any PostgreSQL dependencies.
#[derive(Clone)]
pub struct AppState {
    /// Runtime profile (standalone or distributed)
    pub runtime_profile: RuntimeProfile,
    
    /// Metadata document store (RustFS)
    pub metadata_store: Arc<dyn MetadataDocumentStore>,
    
    /// Event log store (RustFS)
    pub event_store: Arc<dyn EventLogStore>,
    
    /// Object store for blob storage (RustFS/S3)
    pub object_store: Arc<ObjectStore>,
    
    /// Coordination store (memory or Redis)
    pub coordination_store: Arc<dyn CoordinationStore>,
    
    /// Session manager for authentication
    pub session_manager: Arc<SessionManager>,
    
    /// JWT manager for token generation
    pub jwt_manager: Arc<JwtManager>,
    
    /// Event broadcaster for real-time updates
    pub broadcaster: Arc<EventBroadcaster>,
    
    /// Secret key for encryption
    pub secret_key: SecretEncryptionKey,
    
    /// Rate limiter state
    pub rate_limit_state: Arc<Mutex<HashMap<String, Instant>>>,
    
    // Repository layer
    /// User repository
    pub user_repo: Arc<dyn UserRepository>,
    /// Device repository
    pub device_repo: Arc<dyn DeviceRepository>,
    /// Group repository
    pub group_repo: Arc<dyn GroupRepository>,
    /// Audit repository
    pub audit_repo: Arc<dyn AuditRepository>,
    /// Config repository
    pub config_repo: Arc<dyn ConfigRepository>,
    /// Pairing repository
    pub pairing_repo: Arc<dyn PairingRepository>,
    /// Webhook repository
    pub webhook_repo: Arc<dyn WebhookRepository>,
    /// Notification repository
    pub notification_repo: Arc<dyn NotificationRepository>,
    /// Share repository
    pub share_repo: Arc<dyn ShareRepository>,
    /// Path builder for key generation
    pub path_builder: PathBuilder,
}

impl AppState {
    /// Create a new application state
    ///
    /// This is the main entry point for initializing the application
    /// in zero-PostgreSQL mode.
    pub async fn new(config: ProfileConfig) -> anyhow::Result<Self> {
        let profile = config.detect_profile();
        
        tracing::info!("Initializing RustShare with profile: {:?}", profile);
        
        // Initialize stores based on profile
        let (metadata_store, event_store, coordination_store) = match profile {
            RuntimeProfile::Standalone => {
                Self::init_standalone_stores(&config).await?
            }
            RuntimeProfile::Distributed => {
                Self::init_distributed_stores(&config).await?
            }
        };
        
        // Initialize object store
        let object_store = Arc::new(
            ObjectStore::new(
                &config.rustfs_endpoint,
                &config.rustfs_region,
                &config.rustfs_bucket,
            ).await?
        );
        
        // Initialize session manager
        let session_config = rustshare_storage::session::SessionConfig::new(config.jwt_secret.clone());
        let session_manager = Arc::new(SessionManager::new(session_config));
        
        // Initialize JWT manager
        let jwt_manager = Arc::new(JwtManager::new(config.jwt_secret));
        
        // Initialize event broadcaster
        let broadcaster = Arc::new(EventBroadcaster::new(config.broadcast_capacity));
        
        // Create path builder
        let path_builder = PathBuilder::new(
            config.metadata_prefix.clone(),
            config.metadata_namespace.clone(),
        );
        
        // Initialize repositories
        let user_repo: Arc<dyn UserRepository> = Arc::new(
            RustFsUserRepository::new(Arc::clone(&metadata_store), path_builder.clone())
        );
        
        let device_repo: Arc<dyn DeviceRepository> = Arc::new(
            RustFsDeviceRepository::new(Arc::clone(&metadata_store), path_builder.clone())
        );
        
        let group_repo: Arc<dyn GroupRepository> = Arc::new(
            RustFsGroupRepository::new(Arc::clone(&metadata_store), path_builder.clone())
        );
        
        let audit_repo: Arc<dyn AuditRepository> = Arc::new(
            RustFsAuditRepository::new(Arc::clone(&metadata_store), path_builder.clone())
        );
        
        let config_repo: Arc<dyn ConfigRepository> = Arc::new(
            RustFsConfigRepository::new(Arc::clone(&metadata_store), path_builder.clone())
        );
        
        let pairing_repo: Arc<dyn PairingRepository> = Arc::new(
            RustFsPairingRepository::new(Arc::clone(&metadata_store), path_builder.clone())
        );
        
        let webhook_repo: Arc<dyn WebhookRepository> = Arc::new(
            RustFsWebhookRepository::new(Arc::clone(&metadata_store), path_builder.clone())
        );
        
        let notification_repo: Arc<dyn NotificationRepository> = Arc::new(
            RustFsNotificationRepository::new(Arc::clone(&metadata_store), path_builder.clone())
        );
        
        let share_repo: Arc<dyn ShareRepository> = Arc::new(
            RustFsShareRepository::new(Arc::clone(&metadata_store), path_builder.clone())
        );
        
        Ok(Self {
            runtime_profile: profile,
            metadata_store,
            event_store,
            object_store,
            coordination_store,
            session_manager,
            jwt_manager,
            broadcaster,
            secret_key: config.secret_key,
            rate_limit_state: Arc::new(Mutex::new(HashMap::new())),
            user_repo,
            device_repo,
            group_repo,
            audit_repo,
            config_repo,
            pairing_repo,
            webhook_repo,
            notification_repo,
            share_repo,
            path_builder,
        })
    }
    
    /// Initialize stores for standalone mode
    async fn init_standalone_stores(
        config: &ProfileConfig,
    ) -> anyhow::Result<(Arc<dyn MetadataDocumentStore>, Arc<dyn EventLogStore>, Arc<dyn CoordinationStore>)> {
        use rustshare_storage::metadata_v2::stores::LocalFsDocumentStore;
        use rustshare_storage::metadata_v2::MetadataBackendConfig;
        use rustshare_storage::coordination::InMemoryCoordinationStore;
        
        tracing::info!("Initializing standalone stores (LocalFS + InMemoryCoordination)");
        
        let backend_config = MetadataBackendConfig {
            base_prefix: config.metadata_prefix.clone(),
            namespace: config.metadata_namespace.clone(),
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        };
        
        // Create local filesystem document store
        let local_path = std::path::PathBuf::from(&config.local_storage_path);
        let doc_store: Arc<dyn MetadataDocumentStore> = Arc::new(
            LocalFsDocumentStore::new(local_path, backend_config)
        );
        
        // Create event store wrapper
        let event_store: Arc<dyn EventLogStore> = Arc::new(
            rustshare_storage::metadata_v2::stores::RustFsEventStore::new(Arc::clone(&doc_store))
        );
        
        // Create in-memory coordination store
        let coord_store: Arc<dyn CoordinationStore> = Arc::new(InMemoryCoordinationStore::new());
        
        Ok((doc_store, event_store, coord_store))
    }
    
    /// Initialize stores for distributed mode
    async fn init_distributed_stores(
        config: &ProfileConfig,
    ) -> anyhow::Result<(Arc<dyn MetadataDocumentStore>, Arc<dyn EventLogStore>, Arc<dyn CoordinationStore>)> {
        use rustshare_storage::metadata_v2::stores::RustFsDocumentStore;
        use rustshare_storage::metadata_v2::MetadataBackendConfig;
        use rustshare_storage::coordination::RedisCoordinationStore;
        use aws_config::BehaviorVersion;
        
        tracing::info!("Initializing distributed stores (RustFS + RedisCoordination)");
        
        let backend_config = MetadataBackendConfig {
            base_prefix: config.metadata_prefix.clone(),
            namespace: config.metadata_namespace.clone(),
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        };
        
        // Create S3 client for RustFS
        let aws_config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&config.rustfs_endpoint)
            .region(aws_config::Region::new(config.rustfs_region.clone()))
            .load()
            .await;
        
        let s3_config = aws_sdk_s3::config::Builder::from(&aws_config)
            .force_path_style(true)
            .build();
        
        let s3_client = aws_sdk_s3::Client::from_conf(s3_config);
        
        // Create RustFS document store
        let doc_store: Arc<dyn MetadataDocumentStore> = Arc::new(
            RustFsDocumentStore::new(s3_client, config.rustfs_bucket.clone(), backend_config)
        );
        
        // Create event store wrapper
        let event_store: Arc<dyn EventLogStore> = Arc::new(
            rustshare_storage::metadata_v2::stores::RustFsEventStore::new(Arc::clone(&doc_store))
        );
        
        // Create Redis coordination store
        let redis_url = config.redis_url.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Redis URL required for distributed profile"))?;
        
        let coord_store: Arc<dyn CoordinationStore> = Arc::new(
            RedisCoordinationStore::new(redis_url).await?
        );
        
        Ok((doc_store, event_store, coord_store))
    }
    
    /// Check if running in standalone mode
    pub fn is_standalone(&self) -> bool {
        matches!(self.runtime_profile, RuntimeProfile::Standalone)
    }
    
    /// Check if running in distributed mode
    pub fn is_distributed(&self) -> bool {
        matches!(self.runtime_profile, RuntimeProfile::Distributed)
    }
}

/// Re-export for backwards compatibility
pub use AppState as ZeroPgAppState;
