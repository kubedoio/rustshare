//! Factory for creating repository instances based on configuration

use std::sync::Arc;

use super::*;
use crate::metadata_v2::{EventLogStore, MetadataBackendConfig, MetadataBackendType, MetadataDocumentStore, RuntimeMetadataCache};

/// Configuration for repository factory
#[derive(Debug, Clone)]
pub struct RepositoryFactoryConfig {
    /// Backend type
    pub backend_type: MetadataBackendType,
    /// Backend configuration
    pub backend_config: MetadataBackendConfig,
    /// Enable runtime cache
    pub enable_cache: bool,
    /// Dual-write configuration (when applicable)
    pub dual_write_config: Option<DualWriteConfig>,
}

impl RepositoryFactoryConfig {
    /// Create from environment variables
    pub fn from_env() -> anyhow::Result<Self> {
        let backend_type = std::env::var("RUSTSHARE_METADATA_BACKEND")
            .unwrap_or_else(|_| "postgres".to_string())
            .parse()?;
        
        let base_prefix = std::env::var("RUSTSHARE_METADATA_PREFIX")
            .unwrap_or_else(|_| "apps/rustshare".to_string());
        
        let namespace = std::env::var("RUSTSHARE_METADATA_NAMESPACE")
            .unwrap_or_else(|_| "default".to_string());
        
        let enable_cache = std::env::var("RUSTSHARE_METADATA_CACHE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);
        
        let backend_config = MetadataBackendConfig {
            base_prefix,
            namespace,
            enable_optimistic_concurrency: true,
            fallback_to_leases: true,
        };
        
        Ok(Self {
            backend_type,
            backend_config,
            enable_cache,
            dual_write_config: Some(DualWriteConfig::default()),
        })
    }
}

/// Repository factory
pub struct RepositoryFactory;

impl RepositoryFactory {
    /// Create folder repository
    pub fn create_folder_repo(
        doc_store: Arc<dyn MetadataDocumentStore>,
        path_builder: PathBuilder,
        cache: Option<Arc<RuntimeMetadataCache>>,
    ) -> Arc<dyn FolderRepository> {
        Arc::new(RustFsFolderRepository::new(doc_store, path_builder, cache))
    }
    
    /// Create file repository
    pub fn create_file_repo(
        doc_store: Arc<dyn MetadataDocumentStore>,
        path_builder: PathBuilder,
        cache: Option<Arc<RuntimeMetadataCache>>,
    ) -> Arc<dyn FileRepository> {
        Arc::new(RustFsFileRepository::new(doc_store, path_builder, cache))
    }
    
    /// Create file version repository
    pub fn create_file_version_repo(
        doc_store: Arc<dyn MetadataDocumentStore>,
        path_builder: PathBuilder,
    ) -> Arc<dyn FileVersionRepository> {
        Arc::new(RustFsFileVersionRepository::new(doc_store, path_builder))
    }
    
    /// Create share repository
    pub fn create_share_repo(
        doc_store: Arc<dyn MetadataDocumentStore>,
        path_builder: PathBuilder,
        cache: Option<Arc<RuntimeMetadataCache>>,
    ) -> Arc<dyn ShareRepository> {
        Arc::new(RustFsShareRepository::new(doc_store, path_builder, cache))
    }
    
    /// Create event repository
    pub fn create_event_repo(
        event_store: Arc<dyn EventLogStore>,
    ) -> Arc<dyn EventRepository> {
        Arc::new(RustFsEventRepository::new(event_store))
    }
    
    /// Create folder children index repository
    pub fn create_folder_children_index_repo(
        doc_store: Arc<dyn MetadataDocumentStore>,
        path_builder: PathBuilder,
        cache: Option<Arc<RuntimeMetadataCache>>,
    ) -> Arc<dyn FolderChildrenIndexRepository> {
        Arc::new(RustFsFolderChildrenIndexRepository::new(doc_store, path_builder, cache))
    }
    
    /// Create tombstone repository
    pub fn create_tombstone_repo(
        doc_store: Arc<dyn MetadataDocumentStore>,
        path_builder: PathBuilder,
    ) -> Arc<dyn TombstoneRepository> {
        Arc::new(RustFsTombstoneRepository::new(doc_store, path_builder))
    }
    
    /// Create search index repository
    pub fn create_search_index_repo(
        doc_store: Arc<dyn MetadataDocumentStore>,
        path_builder: PathBuilder,
    ) -> Arc<dyn SearchIndexRepository> {
        Arc::new(RustFsSearchIndexRepository::new(doc_store, path_builder))
    }
}

/// Combined metadata repository implementation
pub struct CombinedMetadataRepository {
    folders: Arc<dyn FolderRepository>,
    files: Arc<dyn FileRepository>,
    file_versions: Arc<dyn FileVersionRepository>,
    shares: Arc<dyn ShareRepository>,
    events: Arc<dyn EventRepository>,
    folder_children_index: Arc<dyn FolderChildrenIndexRepository>,
    tombstones: Arc<dyn TombstoneRepository>,
    search_index: Arc<dyn SearchIndexRepository>,
}

impl CombinedMetadataRepository {
    pub fn new(
        folders: Arc<dyn FolderRepository>,
        files: Arc<dyn FileRepository>,
        file_versions: Arc<dyn FileVersionRepository>,
        shares: Arc<dyn ShareRepository>,
        events: Arc<dyn EventRepository>,
        folder_children_index: Arc<dyn FolderChildrenIndexRepository>,
        tombstones: Arc<dyn TombstoneRepository>,
        search_index: Arc<dyn SearchIndexRepository>,
    ) -> Self {
        Self {
            folders,
            files,
            file_versions,
            shares,
            events,
            folder_children_index,
            tombstones,
            search_index,
        }
    }
    
    /// Create from components
    pub fn from_components(
        doc_store: Arc<dyn MetadataDocumentStore>,
        event_store: Arc<dyn EventLogStore>,
        path_builder: PathBuilder,
        cache: Option<Arc<RuntimeMetadataCache>>,
    ) -> Self {
        let folders = RepositoryFactory::create_folder_repo(
            Arc::clone(&doc_store),
            path_builder.clone(),
            cache.clone(),
        );
        
        let files = RepositoryFactory::create_file_repo(
            Arc::clone(&doc_store),
            path_builder.clone(),
            cache.clone(),
        );
        
        let file_versions = RepositoryFactory::create_file_version_repo(
            Arc::clone(&doc_store),
            path_builder.clone(),
        );
        
        let shares = RepositoryFactory::create_share_repo(
            Arc::clone(&doc_store),
            path_builder.clone(),
            cache.clone(),
        );
        
        let events = RepositoryFactory::create_event_repo(event_store);
        
        let folder_children_index = RepositoryFactory::create_folder_children_index_repo(
            Arc::clone(&doc_store),
            path_builder.clone(),
            cache.clone(),
        );
        
        let tombstones = RepositoryFactory::create_tombstone_repo(
            Arc::clone(&doc_store),
            path_builder.clone(),
        );
        
        let search_index = RepositoryFactory::create_search_index_repo(
            Arc::clone(&doc_store),
            path_builder.clone(),
        );
        
        Self::new(
            folders,
            files,
            file_versions,
            shares,
            events,
            folder_children_index,
            tombstones,
            search_index,
        )
    }
}

impl MetadataRepository for CombinedMetadataRepository {
    fn folders(&self) -> &dyn FolderRepository {
        self.folders.as_ref()
    }
    
    fn files(&self) -> &dyn FileRepository {
        self.files.as_ref()
    }
    
    fn file_versions(&self) -> &dyn FileVersionRepository {
        self.file_versions.as_ref()
    }
    
    fn shares(&self) -> &dyn ShareRepository {
        self.shares.as_ref()
    }
    
    fn events(&self) -> &dyn EventRepository {
        self.events.as_ref()
    }
    
    fn folder_children_index(&self) -> &dyn FolderChildrenIndexRepository {
        self.folder_children_index.as_ref()
    }
    
    fn tombstones(&self) -> &dyn TombstoneRepository {
        self.tombstones.as_ref()
    }
    
    fn search_index(&self) -> &dyn SearchIndexRepository {
        self.search_index.as_ref()
    }
}

/// Builder for creating repository configurations
pub struct RepositoryBuilder {
    doc_store: Option<Arc<dyn MetadataDocumentStore>>,
    event_store: Option<Arc<dyn EventLogStore>>,
    path_builder: Option<PathBuilder>,
    cache: Option<Arc<RuntimeMetadataCache>>,
    backend_type: MetadataBackendType,
}

impl RepositoryBuilder {
    pub fn new(backend_type: MetadataBackendType) -> Self {
        Self {
            doc_store: None,
            event_store: None,
            path_builder: None,
            cache: None,
            backend_type,
        }
    }
    
    pub fn with_doc_store(mut self, store: Arc<dyn MetadataDocumentStore>) -> Self {
        self.doc_store = Some(store);
        self
    }
    
    pub fn with_event_store(mut self, store: Arc<dyn EventLogStore>) -> Self {
        self.event_store = Some(store);
        self
    }
    
    pub fn with_path_builder(mut self, builder: PathBuilder) -> Self {
        self.path_builder = Some(builder);
        self
    }
    
    pub fn with_cache(mut self, cache: Arc<RuntimeMetadataCache>) -> Self {
        self.cache = Some(cache);
        self
    }
    
    pub fn build(self) -> anyhow::Result<Arc<dyn MetadataRepository>> {
        let doc_store = self.doc_store.ok_or_else(|| anyhow::anyhow!("Document store required"))?;
        let event_store = self.event_store.ok_or_else(|| anyhow::anyhow!("Event store required"))?;
        let path_builder = self.path_builder.ok_or_else(|| anyhow::anyhow!("Path builder required"))?;
        
        let repo = CombinedMetadataRepository::from_components(
            doc_store,
            event_store,
            path_builder,
            self.cache,
        );
        
        Ok(Arc::new(repo))
    }
}
