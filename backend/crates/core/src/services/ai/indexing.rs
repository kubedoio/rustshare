//! Content indexing for AI-powered search.
//!
//! This module provides:
//! - Document extraction from files
//! - In-memory content indexing
//! - Background indexing job support

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::embedding::{Embedding, EmbeddingGenerator};

/// Maximum content length to index per document (to prevent memory issues).
const MAX_CONTENT_LENGTH: usize = 100_000;

/// Maximum number of documents to keep in the index.
const MAX_DOCUMENTS: usize = 10_000;

/// An indexed document with its embedding and metadata.
#[derive(Debug, Clone)]
pub struct IndexedDocument {
    /// The file ID this document represents
    pub file_id: Uuid,
    /// File name
    pub file_name: String,
    /// Full file path
    pub file_path: String,
    /// The extracted and indexed content
    pub content: String,
    /// The embedding vector for this document
    pub embedding: Embedding,
    /// MIME type of the file
    pub mime_type: String,
    /// File owner ID
    pub owner_id: Uuid,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// When this document was indexed
    pub indexed_at: chrono::DateTime<chrono::Utc>,
}

/// A content index entry for a tenant.
#[derive(Debug, Clone, Default)]
pub struct ContentIndex {
    /// Map of file_id to indexed document
    pub documents: HashMap<Uuid, IndexedDocument>,
    /// Total number of documents
    pub document_count: usize,
}

/// Content indexer that manages embeddings for file content.
///
/// This provides an in-memory index for Phase 1.5. Future phases can:
/// - Use a persistent vector database (pgvector, Qdrant, etc.)
/// - Add background jobs for async indexing
/// - Support incremental updates
pub struct ContentIndexer<EG: EmbeddingGenerator> {
    /// The embedding generator to use
    embedding_generator: Arc<EG>,
    /// In-memory index: tenant_id -> ContentIndex
    indexes: Arc<RwLock<HashMap<Uuid, ContentIndex>>>,
}

impl<EG: EmbeddingGenerator> ContentIndexer<EG> {
    /// Create a new content indexer.
    pub fn new(embedding_generator: Arc<EG>) -> Self {
        Self {
            embedding_generator,
            indexes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Index a file's content.
    ///
    /// # Arguments
    /// * `file_id` - The file ID
    /// * `file_name` - The file name
    /// * `file_path` - The full file path
    /// * `content` - The extracted text content
    /// * `mime_type` - The MIME type
    /// * `owner_id` - The file owner
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// Ok(()) if successfully indexed
    #[allow(clippy::too_many_arguments)]
    pub async fn index_file(
        &self,
        file_id: Uuid,
        file_name: String,
        file_path: String,
        content: String,
        mime_type: String,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> anyhow::Result<()> {
        // Truncate content if too long
        let content = if content.len() > MAX_CONTENT_LENGTH {
            content[..MAX_CONTENT_LENGTH].to_string()
        } else {
            content
        };

        // Generate embedding
        let combined_text = format!("{} {} {}", file_name, file_path, content);
        let embedding = self.embedding_generator.generate(&combined_text).await;

        let document = IndexedDocument {
            file_id,
            file_name: file_name.clone(),
            file_path: file_path.clone(),
            content,
            embedding,
            mime_type,
            owner_id,
            tenant_id,
            indexed_at: chrono::Utc::now(),
        };

        let mut indexes = self.indexes.write().await;
        let index = indexes.entry(tenant_id).or_default();

        // Enforce max documents limit (LRU eviction - remove oldest)
        if index.documents.len() >= MAX_DOCUMENTS && !index.documents.contains_key(&file_id) {
            // Find oldest document and remove it
            if let Some((oldest_id, _)) =
                index.documents.iter().min_by_key(|(_, doc)| doc.indexed_at)
            {
                let oldest_id = *oldest_id;
                index.documents.remove(&oldest_id);
            }
        }

        index.documents.insert(file_id, document);
        index.document_count = index.documents.len();

        Ok(())
    }

    /// Remove a file from the index.
    ///
    /// # Arguments
    /// * `file_id` - The file ID to remove
    /// * `tenant_id` - The tenant ID
    pub async fn remove_file(&self, file_id: Uuid, tenant_id: Uuid) {
        let mut indexes = self.indexes.write().await;
        if let Some(index) = indexes.get_mut(&tenant_id) {
            index.documents.remove(&file_id);
            index.document_count = index.documents.len();
        }
    }

    /// Search for documents similar to the query.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to search within
    /// * `query` - The search query
    /// * `limit` - Maximum number of results
    ///
    /// # Returns
    /// List of (document, similarity_score) sorted by relevance
    pub async fn search(
        &self,
        tenant_id: Uuid,
        query: &str,
        limit: usize,
    ) -> Vec<(IndexedDocument, f32)> {
        let query_embedding = self.embedding_generator.generate(query).await;

        let indexes = self.indexes.read().await;
        let index = match indexes.get(&tenant_id) {
            Some(idx) => idx,
            None => return Vec::new(),
        };

        let mut results: Vec<(IndexedDocument, f32)> = index
            .documents
            .values()
            .map(|doc| {
                let similarity = self
                    .embedding_generator
                    .similarity(&query_embedding, &doc.embedding);
                (doc.clone(), similarity)
            })
            .filter(|(_, score)| *score > 0.1) // Filter out very low similarity
            .collect();

        // Sort by similarity (descending)
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        results.truncate(limit);

        results
    }

    /// Get a document by file ID.
    ///
    /// # Arguments
    /// * `file_id` - The file ID
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// The indexed document if found
    pub async fn get_document(&self, file_id: Uuid, tenant_id: Uuid) -> Option<IndexedDocument> {
        let indexes = self.indexes.read().await;
        indexes
            .get(&tenant_id)
            .and_then(|index| index.documents.get(&file_id).cloned())
    }

    /// Get all documents for a tenant.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// List of all indexed documents for the tenant
    pub async fn get_all_documents(&self, tenant_id: Uuid) -> Vec<IndexedDocument> {
        let indexes = self.indexes.read().await;
        match indexes.get(&tenant_id) {
            Some(index) => index.documents.values().cloned().collect(),
            None => Vec::new(),
        }
    }

    /// Clear the index for a tenant.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    pub async fn clear_tenant(&self, tenant_id: Uuid) {
        let mut indexes = self.indexes.write().await;
        indexes.remove(&tenant_id);
    }

    /// Get the document count for a tenant.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// Number of indexed documents
    pub async fn document_count(&self, tenant_id: Uuid) -> usize {
        let indexes = self.indexes.read().await;
        indexes
            .get(&tenant_id)
            .map(|index| index.documents.len())
            .unwrap_or(0)
    }

    /// Extract text content from a file based on its MIME type.
    ///
    /// Phase 1.5: Simple extraction for text files.
    /// Future phases: PDF text extraction, image OCR, etc.
    ///
    /// # Arguments
    /// * `content` - The raw file bytes
    /// * `mime_type` - The MIME type
    ///
    /// # Returns
    /// Extracted text content
    pub fn extract_text(content: &[u8], mime_type: &str) -> String {
        match mime_type {
            "text/plain"
            | "text/markdown"
            | "text/csv"
            | "application/json"
            | "application/xml"
            | "text/xml"
            | "text/html"
            | "text/css"
            | "application/javascript"
            | "text/javascript" => {
                // Try to decode as UTF-8
                String::from_utf8_lossy(content).into_owned()
            }
            "text/rtf" => {
                // Basic RTF text extraction (strip RTF control words)
                let text = String::from_utf8_lossy(content);
                extract_rtf_text(&text)
            }
            _ => {
                // For other types, return empty string
                // Future: PDF extraction, docx parsing, etc.
                String::new()
            }
        }
    }
}

/// Basic RTF text extraction.
/// Strips RTF control sequences and returns plain text.
fn extract_rtf_text(rtf: &str) -> String {
    let mut result = String::new();
    let mut in_control = false;
    let mut in_group = 0u32;

    for ch in rtf.chars() {
        match ch {
            '{' => {
                in_group += 1;
            }
            '}' => {
                in_group = in_group.saturating_sub(1);
            }
            '\\' => {
                in_control = true;
            }
            ' ' if in_control => {
                in_control = false;
            }
            '\n' | '\r' => {
                // Skip newlines in control words
                if !in_control {
                    result.push(' ');
                }
            }
            _ => {
                if !in_control && in_group > 0 && ch.is_ascii_graphic() || ch.is_ascii_whitespace()
                {
                    result.push(ch);
                }
            }
        }
    }

    // Clean up whitespace
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::super::embedding::SimpleEmbeddingGenerator;
    use super::*;

    #[tokio::test]
    async fn test_index_and_search() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let indexer = ContentIndexer::new(generator);

        let tenant_id = Uuid::new_v4();

        // Index some documents
        indexer
            .index_file(
                Uuid::new_v4(),
                "rust_guide.md".to_string(),
                "/docs/rust_guide.md".to_string(),
                "Rust is a systems programming language with memory safety".to_string(),
                "text/markdown".to_string(),
                Uuid::new_v4(),
                tenant_id,
            )
            .await
            .unwrap();

        indexer
            .index_file(
                Uuid::new_v4(),
                "python_guide.md".to_string(),
                "/docs/python_guide.md".to_string(),
                "Python is a high-level programming language".to_string(),
                "text/markdown".to_string(),
                Uuid::new_v4(),
                tenant_id,
            )
            .await
            .unwrap();

        // Search for Rust-related content
        let tenant_id = indexer.indexes.read().await.keys().next().copied().unwrap();
        let results = indexer
            .search(tenant_id, "memory safety programming", 10)
            .await;

        assert!(!results.is_empty());
        // The Rust document should have higher similarity
        assert!(results[0].0.file_name.contains("rust"));
    }

    #[tokio::test]
    async fn test_remove_file() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let indexer = ContentIndexer::new(generator);

        let file_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        indexer
            .index_file(
                file_id,
                "test.txt".to_string(),
                "/test.txt".to_string(),
                "test content".to_string(),
                "text/plain".to_string(),
                Uuid::new_v4(),
                tenant_id,
            )
            .await
            .unwrap();

        assert_eq!(indexer.document_count(tenant_id).await, 1);

        indexer.remove_file(file_id, tenant_id).await;

        assert_eq!(indexer.document_count(tenant_id).await, 0);
    }

    #[tokio::test]
    async fn test_extract_text_plain() {
        let content = b"Hello, World! This is a test.";
        let text = ContentIndexer::<SimpleEmbeddingGenerator>::extract_text(content, "text/plain");
        assert_eq!(text, "Hello, World! This is a test.");
    }

    #[tokio::test]
    async fn test_extract_text_binary() {
        let content = vec![0x89, 0x50, 0x4E, 0x47]; // PNG header
        let text = ContentIndexer::<SimpleEmbeddingGenerator>::extract_text(&content, "image/png");
        assert!(text.is_empty());
    }

    #[tokio::test]
    async fn test_get_document() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let indexer = ContentIndexer::new(generator);

        let file_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        indexer
            .index_file(
                file_id,
                "test.txt".to_string(),
                "/test.txt".to_string(),
                "test content".to_string(),
                "text/plain".to_string(),
                Uuid::new_v4(),
                tenant_id,
            )
            .await
            .unwrap();

        let doc = indexer.get_document(file_id, tenant_id).await;
        assert!(doc.is_some());
        assert_eq!(doc.unwrap().file_name, "test.txt");
    }
}
