//! Metadata system integration for the server
//!
//! TODO: This module is deprecated - it was used during the migration from
//! PostgreSQL to RustFS. It should be removed or rewritten for the new
//! zero-PostgreSQL architecture.

use std::sync::Arc;

use rustshare_storage::{
    metadata_v2, repos, service_integration, EventStore, MetadataStore, ObjectStore,
};

/// Extended application state with new metadata repositories
/// 
/// DEPRECATED: This is being replaced by the new AppState in main.rs
#[derive(Clone)]
pub struct MetadataState {
    /// Legacy metadata store
    pub metadata_store: Arc<MetadataStore>,
    /// Legacy event store
    pub event_store: Arc<EventStore>,
    /// New metadata repository (when enabled)
    pub new_repo: Option<Arc<dyn repos::MetadataRepository>>,
    /// Metadata compatibility layer
    pub compat: Option<metadata_v2::MetadataStoreCompat>,
    /// Backend type being used
    pub backend_type: metadata_v2::MetadataBackendType,
}

impl MetadataState {
    /// Initialize the metadata system
    /// 
    /// TODO: Remove db_pool parameter - this is a temporary measure
    pub async fn from_env(
        _db_pool_placeholder: Option<()>,
        object_store: Arc<ObjectStore>,
    ) -> anyhow::Result<Self> {
        // Initialize legacy stores
        let metadata_store = Arc::new(MetadataStore::new(()));
        let event_store = Arc::new(EventStore::new(()));

        // Load configuration
        let config = service_integration::MetadataConfig::from_env()?;

        // Initialize new metadata system if not using postgres-only mode
        let (new_repo, compat) = match config.backend_type {
            metadata_v2::MetadataBackendType::Postgres => {
                tracing::info!("Using PostgreSQL-only metadata backend (deprecated)");
                (None, None)
            }
            _ => {
                tracing::info!(
                    backend_type = ?config.backend_type,
                    "Initializing new metadata backend"
                );

                // Create S3 client for RustFS
                let s3_client =
                    service_integration::create_s3_client(&config.rustfs_endpoint, &config.rustfs_region).await?;

                // Build metadata system
                let builder = service_integration::MetadataSystemBuilder::new(config.clone())
                    .with_s3_client(s3_client);

                let repo = builder.build().await?;
                let compat = metadata_v2::MetadataStoreCompat::new(Arc::clone(&repo));

                (Some(repo), Some(compat))
            }
        };

        Ok(Self {
            metadata_store,
            event_store,
            new_repo,
            compat,
            backend_type: config.backend_type,
        })
    }

    /// Get the folder repository
    pub fn folders(&self) -> anyhow::Result<&dyn repos::FolderRepository> {
        match &self.new_repo {
            Some(repo) => Ok(repo.folders()),
            None => anyhow::bail!("New metadata repository not initialized"),
        }
    }

    /// Get the file repository
    pub fn files(&self) -> anyhow::Result<&dyn repos::FileRepository> {
        match &self.new_repo {
            Some(repo) => Ok(repo.files()),
            None => anyhow::bail!("New metadata repository not initialized"),
        }
    }

    /// Get the file version repository
    pub fn file_versions(&self) -> anyhow::Result<&dyn repos::FileVersionRepository> {
        match &self.new_repo {
            Some(repo) => Ok(repo.file_versions()),
            None => anyhow::bail!("New metadata repository not initialized"),
        }
    }

    /// Get the share repository
    pub fn shares(&self) -> anyhow::Result<&dyn repos::ShareRepository> {
        match &self.new_repo {
            Some(repo) => Ok(repo.shares()),
            None => anyhow::bail!("New metadata repository not initialized"),
        }
    }

    /// Check if using the new metadata system
    pub fn using_new_backend(&self) -> bool {
        self.new_repo.is_some()
    }

    /// Create admin handler for verification/repair
    pub fn admin_handler(&self) -> Option<service_integration::MetadataAdminHandler> {
        // This would need both postgres and rustfs repos
        // For now, return None
        None
    }
}

/// Configuration for metadata backend selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataBackendMode {
    /// Use PostgreSQL only (legacy - DEPRECATED)
    PostgresOnly,
    /// Use RustFS for reads, PostgreSQL for writes (migration - DEPRECATED)
    RustFsReads,
    /// Use RustFS for all operations (new system)
    RustFsFull,
    /// Dual-write mode for verification
    DualWrite,
}

impl MetadataBackendMode {
    /// Determine mode from environment
    pub fn from_env() -> Self {
        let backend = std::env::var("RUSTSHARE_METADATA_BACKEND")
            .unwrap_or_else(|_| "rustfs".to_string());

        match backend.as_str() {
            "rustfs" => Self::RustFsFull,
            "dual_write" => Self::DualWrite,
            "rustfs_reads" => Self::RustFsReads,
            _ => Self::PostgresOnly,
        }
    }
}

/// Factory for creating service layer components
pub struct ServiceFactory;

impl ServiceFactory {
    /// Create file service based on backend mode
    pub fn create_file_service(
        mode: MetadataBackendMode,
        state: &MetadataState,
        object_store: Arc<ObjectStore>,
        broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
    ) -> Arc<dyn FileServiceTrait> {
        match mode {
            MetadataBackendMode::RustFsFull | MetadataBackendMode::RustFsReads => {
                // Use new metadata system
                if let Some(ref compat) = state.compat {
                    // Create service with compatibility layer
                    // This would need a wrapper trait
                    Arc::new(FileServiceV2::new(
                        Arc::clone(&state.event_store),
                        compat.clone(),
                        object_store,
                        broadcaster,
                    ))
                } else {
                    panic!("Compatibility layer not initialized")
                }
            }
            _ => {
                // Use legacy service
                Arc::new(FileServiceV1::new(
                    Arc::clone(&state.event_store),
                    Arc::clone(&state.metadata_store),
                    object_store,
                    broadcaster,
                ))
            }
        }
    }
}

// Placeholder traits and types for compilation
trait FileServiceTrait: Send + Sync {}

struct FileServiceV1 {
    event_store: Arc<EventStore>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
}

impl FileServiceV1 {
    fn new(
        event_store: Arc<EventStore>,
        metadata_store: Arc<MetadataStore>,
        object_store: Arc<ObjectStore>,
        broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
    ) -> Self {
        Self {
            event_store,
            metadata_store,
            object_store,
            broadcaster,
        }
    }
}

impl FileServiceTrait for FileServiceV1 {}

struct FileServiceV2 {
    event_store: Arc<EventStore>,
    compat: metadata_v2::MetadataStoreCompat,
    object_store: Arc<ObjectStore>,
    broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
}

impl FileServiceV2 {
    fn new(
        event_store: Arc<EventStore>,
        compat: metadata_v2::MetadataStoreCompat,
        object_store: Arc<ObjectStore>,
        broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
    ) -> Self {
        Self {
            event_store,
            compat,
            object_store,
            broadcaster,
        }
    }
}

impl FileServiceTrait for FileServiceV2 {}
