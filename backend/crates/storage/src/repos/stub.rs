//! Stub repository implementations for zero-PostgreSQL mode
//!
//! These are placeholder implementations that allow the server to compile
//! and run while the full RustFS repositories are being implemented.

use async_trait::async_trait;
use std::sync::Arc;

use super::{
    FileRepository, FileVersionRepository, FolderChildrenIndexRepository,
    FolderRepository, MetadataRepository, ShareRepository,
    EventRepository, TombstoneRepository,
};

/// Stub metadata repository that returns empty results
pub struct StubMetadataRepository;

impl StubMetadataRepository {
    /// Create a new stub repository
    pub fn new() -> Arc<dyn MetadataRepository> {
        Arc::new(Self)
    }
}

#[async_trait]
impl MetadataRepository for StubMetadataRepository {
    fn folders(&self) -> &dyn FolderRepository {
        unimplemented!("StubMetadataRepository::folders")
    }

    fn files(&self) -> &dyn FileRepository {
        unimplemented!("StubMetadataRepository::files")
    }

    fn file_versions(&self) -> &dyn FileVersionRepository {
        unimplemented!("StubMetadataRepository::file_versions")
    }

    fn shares(&self) -> &dyn ShareRepository {
        unimplemented!("StubMetadataRepository::shares")
    }
    
    fn events(&self) -> &dyn EventRepository {
        unimplemented!("StubMetadataRepository::events")
    }
    
    fn folder_children_index(&self) -> &dyn FolderChildrenIndexRepository {
        unimplemented!("StubMetadataRepository::folder_children_index")
    }
    
    fn tombstones(&self) -> &dyn TombstoneRepository {
        unimplemented!("StubMetadataRepository::tombstones")
    }
    
}

impl Default for StubMetadataRepository {
    fn default() -> Self {
        Self
    }
}
