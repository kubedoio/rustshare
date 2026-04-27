//! RustFS-backed search index repository implementation

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use crate::metadata_v2::{
    schemas::{
        tokenize_search_query, FileDocument, FolderDocument, SearchIndexDocument, SearchIndexEntry,
        SearchResult,
    },
    MetadataDocumentStore, MetadataDocumentStoreExt, PutOptions,
};
use crate::repos::{PathBuilder, RepositoryError};

use super::SearchIndexRepository;

/// RustFS-backed search index repository
pub struct RustFsSearchIndexRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsSearchIndexRepository {
    /// Create a new RustFS search index repository
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, path_builder: PathBuilder) -> Self {
        Self {
            doc_store,
            path_builder,
        }
    }

    /// Get the storage path for a search term index
    fn term_index_path(&self, tenant_id: Uuid, term: &str) -> String {
        // Hash the term for safe filesystem path
        let term_hash = Self::hash_term(term);
        format!(
            "{}/{}/indexes/search/{}/{}.json",
            self.path_builder.base_prefix(),
            self.path_builder.namespace(),
            tenant_id,
            term_hash
        )
    }

    /// Simple hash for index keys
    fn hash_term(s: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(s.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Get or create a search index document for a term
    async fn get_or_create_index(
        &self,
        tenant_id: Uuid,
        term: &str,
    ) -> Result<SearchIndexDocument, RepositoryError> {
        let path = self.term_index_path(tenant_id, term);

        match self.doc_store.get::<SearchIndexDocument>(&path).await {
            Ok(Some((doc, _))) => Ok(doc),
            Ok(None) => Ok(SearchIndexDocument::new(tenant_id, term.to_string())),
            Err(e) => Err(RepositoryError::StorageError(e.to_string())),
        }
    }

    /// Save a search index document
    async fn save_index(&self, index: &SearchIndexDocument) -> Result<(), RepositoryError> {
        let path = self.term_index_path(index.tenant_id, &index.term);

        // If the index is empty, delete it instead of storing an empty document
        if index.is_empty() {
            if let Err(e) = self.doc_store.delete(&path).await {
                tracing::debug!(path = %path, error = %e, "failed to delete empty search index");
            }
            return Ok(());
        }

        self.doc_store
            .put(&path, index, PutOptions::default())
            .await
            .map(|_| ())
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
    }

    /// Extract search terms from a file/folder name and path
    fn extract_terms(&self, name: &str, path: &str) -> Vec<String> {
        // Combine name and path for indexing
        let combined = format!("{} {}", name, path);
        tokenize_search_query(&combined)
    }
}

#[async_trait]
impl SearchIndexRepository for RustFsSearchIndexRepository {
    async fn index_file(&self, file: &FileDocument) -> Result<(), RepositoryError> {
        // Skip deleted files
        if file.deleted {
            return self.remove_from_index(file.id).await;
        }

        let terms = self.extract_terms(&file.name, &file.path);
        let entry = SearchIndexEntry {
            resource_id: file.id,
            resource_type: "file".to_string(),
            name: file.name.clone(),
            path: file.path.clone(),
            owner_id: file.owner_id,
            updated_at: file.updated_at,
        };

        // Add entry to each term's index
        for term in terms {
            let mut index = self.get_or_create_index(file.tenant_id, &term).await?;
            index.upsert_entry(entry.clone());
            self.save_index(&index).await?;
        }

        Ok(())
    }

    async fn index_folder(&self, folder: &FolderDocument) -> Result<(), RepositoryError> {
        // Skip deleted folders
        if folder.deleted {
            return self.remove_from_index(folder.id).await;
        }

        let terms = self.extract_terms(&folder.name, &folder.path);
        let entry = SearchIndexEntry {
            resource_id: folder.id,
            resource_type: "folder".to_string(),
            name: folder.name.clone(),
            path: folder.path.clone(),
            owner_id: folder.owner_id,
            updated_at: folder.updated_at,
        };

        // Add entry to each term's index
        for term in terms {
            let mut index = self.get_or_create_index(folder.tenant_id, &term).await?;
            index.upsert_entry(entry.clone());
            self.save_index(&index).await?;
        }

        Ok(())
    }

    async fn remove_from_index(&self, resource_id: Uuid) -> Result<(), RepositoryError> {
        // We need to find all index documents that contain this resource
        // For efficiency, we'll scan all search indexes for this tenant
        // In a production system, we'd maintain a reverse index

        let prefix = format!(
            "{}/{}/indexes/search/",
            self.path_builder.base_prefix(),
            self.path_builder.namespace()
        );

        let keys = self
            .doc_store
            .list_prefix(&prefix)
            .await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;

        for key in keys {
            if let Ok(Some((mut index, _))) = self.doc_store.get::<SearchIndexDocument>(&key).await
            {
                // Check if this index contains the resource
                if index.entries.iter().any(|e| e.resource_id == resource_id) {
                    index.remove_entry(resource_id);

                    // Save or delete the index
                    if let Err(e) = self.save_index(&index).await {
                        tracing::warn!("Failed to update search index {}: {}", key, e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn search(
        &self,
        tenant_id: Uuid,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, RepositoryError> {
        let terms = tokenize_search_query(query);

        if terms.is_empty() {
            return Ok(Vec::new());
        }

        // Collect all matching entries
        let mut resource_scores: HashMap<Uuid, (SearchResult, usize)> = HashMap::new();

        for term in &terms {
            let path = self.term_index_path(tenant_id, term);

            if let Ok(Some((index, _))) = self.doc_store.get::<SearchIndexDocument>(&path).await {
                for entry in index.entries {
                    let score = if entry.name.to_lowercase().contains(term) {
                        // Higher score for name matches
                        10
                    } else {
                        // Lower score for path-only matches
                        5
                    };

                    let result = SearchResult {
                        id: entry.resource_id,
                        resource_type: entry.resource_type.clone(),
                        name: entry.name,
                        path: entry.path,
                        parent_id: None, // Would need to look up from parent path
                        owner_id: entry.owner_id,
                        updated_at: entry.updated_at,
                    };

                    // Accumulate score for resources matching multiple terms
                    resource_scores
                        .entry(entry.resource_id)
                        .and_modify(|(_, s)| *s += score)
                        .or_insert((result, score));
                }
            }
        }

        // Convert to vec and sort by score (descending), then by updated_at (descending)
        let mut results: Vec<(SearchResult, usize)> = resource_scores.into_values().collect();
        results.sort_by(|a, b| {
            b.1.cmp(&a.1) // Higher score first
                .then_with(|| b.0.updated_at.cmp(&a.0.updated_at)) // More recent first
        });

        // Limit results
        let limited: Vec<SearchResult> = results
            .into_iter()
            .take(limit)
            .map(|(result, _)| result)
            .collect();

        Ok(limited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata_v2::PutResult;

    #[test]
    fn test_extract_terms() {
        let repo = RustFsSearchIndexRepository::new(
            Arc::new(MockDocStore::new()),
            PathBuilder::new("test".to_string(), "ns".to_string()),
        );

        let terms = repo.extract_terms("My Document.txt", "/Documents/My Document.txt");
        assert!(terms.contains(&"my".to_string()));
        assert!(terms.contains(&"document".to_string()));
        assert!(terms.contains(&"txt".to_string()));
        assert!(terms.contains(&"documents".to_string()));
    }

    // Mock for testing
    struct MockDocStore;

    impl MockDocStore {
        fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl MetadataDocumentStore for MockDocStore {
        async fn get_raw(
            &self,
            _key: &str,
        ) -> anyhow::Result<Option<(Vec<u8>, crate::metadata_v2::ObjectMetadata)>> {
            Ok(None)
        }

        async fn get_multi_raw(
            &self,
            _keys: &[&str],
        ) -> anyhow::Result<Vec<(String, Vec<u8>, crate::metadata_v2::ObjectMetadata)>> {
            Ok(Vec::new())
        }

        async fn head(
            &self,
            _key: &str,
        ) -> anyhow::Result<Option<crate::metadata_v2::ObjectMetadata>> {
            Ok(None)
        }

        async fn put_raw(
            &self,
            _key: &str,
            _data: &[u8],
            _opts: PutOptions,
        ) -> anyhow::Result<PutResult> {
            Ok(PutResult { etag: None, version_id: None })
        }

        async fn delete(&self, _key: &str) -> anyhow::Result<()> {
            Ok(())
        }

        async fn list_prefix(&self, _prefix: &str) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }
    }
}
