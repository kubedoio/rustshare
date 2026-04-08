//! Search index repository for name/path search
//!
//! This module provides a minimal Phase 1 search implementation using
//! an inverted index stored in RustFS. The index maps search terms
//! to resources (files and folders) for fast name/path searching.

use async_trait::async_trait;
use uuid::Uuid;

use crate::metadata_v2::schemas::{FileDocument, FolderDocument, SearchResult};
use crate::repos::RepositoryError;

pub mod rustfs;

pub use rustfs::RustFsSearchIndexRepository;

/// Repository for search index operations
#[async_trait]
pub trait SearchIndexRepository: Send + Sync {
    /// Index a file document
    async fn index_file(&self, file: &FileDocument) -> Result<(), RepositoryError>;

    /// Index a folder document
    async fn index_folder(&self, folder: &FolderDocument) -> Result<(), RepositoryError>;

    /// Remove a resource from the index
    async fn remove_from_index(&self, resource_id: Uuid) -> Result<(), RepositoryError>;

    /// Search for resources matching the query
    ///
    /// Returns up to `limit` results. Results are not filtered by permission;
    /// the caller must filter results based on user permissions.
    async fn search(
        &self,
        tenant_id: Uuid,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, RepositoryError>;
}
