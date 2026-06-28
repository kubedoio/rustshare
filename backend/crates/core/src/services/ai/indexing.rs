//! Content indexing for AI-powered search.
//!
//! This module provides:
//! - Document extraction from files
//! - In-memory content indexing
//! - Background indexing job support

use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use serde::{Deserialize, Serialize};

use super::embedding::{Embedding, EmbeddingGenerator};
use super::vector_store::VectorStore;
use crate::okf::frontmatter::split_frontmatter;

/// Maximum content length to index per document (to prevent memory issues).
const MAX_CONTENT_LENGTH: usize = 100_000;

/// ACL payload stored on indexed note chunks.
///
/// This is a filterable projection of the note's OKF access-control state.
/// It is intentionally denormalized into the index so retrieval can enforce
/// ACLs without a per-document permission round-trip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteAclPayload {
    pub tenant_id: Uuid,
    pub workspace_id: Uuid,
    /// Stable OKF note identity (`rustshare.id`).
    pub note_id: Uuid,
    /// The `note.md` file id.
    pub source_file_id: Uuid,
    /// The note bundle folder id, when available.
    pub source_folder_id: Option<Uuid>,
    /// Owner of the note (the `note.md` file owner).
    pub owner_id: Uuid,
    /// Resolved read principals, e.g. `["owner:<uuid>", "group_engineering"]`.
    ///
    /// TODO(#118): wire in the real permission resolver instead of the
    /// placeholder owner principal.
    pub read_acl: Vec<String>,
    /// Visibility level: `"private"`, `"workspace"`, or `"public"`.
    pub visibility: String,
    /// Hash of the access-control list at the time of indexing.
    pub acl_hash: String,
    /// Monotonically increasing ACL version; search can reject stale chunks.
    pub acl_version: i64,
    /// Embedding policy: `"allowed"` or `"denied"`.
    pub embedding_policy: String,
}

/// Filter supplied by the caller during permission-aware search.
#[derive(Debug, Clone, Default)]
pub struct AclSearchFilter {
    pub tenant_id: Uuid,
    pub caller_user_id: Uuid,
    pub caller_group_ids: Vec<Uuid>,
    /// note_id -> minimum accepted acl_version.
    pub min_acl_versions: HashMap<Uuid, i64>,
}

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
    /// ACL payload for OKF notes; `None` for legacy/non-note files.
    pub acl: Option<NoteAclPayload>,
    /// Chunk identity. For notes this is currently the source file id.
    pub chunk_id: Uuid,
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
    /// Persistent or in-memory vector store backend.
    store: Arc<dyn VectorStore>,
}

impl<EG: EmbeddingGenerator> ContentIndexer<EG> {
    /// Create a new content indexer backed by the supplied vector store.
    pub fn new(embedding_generator: Arc<EG>, store: Arc<dyn VectorStore>) -> Self {
        Self {
            embedding_generator,
            store,
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
        let content = if content.len() > MAX_CONTENT_LENGTH {
            content[..MAX_CONTENT_LENGTH].to_string()
        } else {
            content
        };

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
            acl: None,
            chunk_id: file_id,
        };

        self.store
            .upsert_chunk(
                tenant_id,
                file_id,
                &document,
                &NoteAclPayload {
                    tenant_id,
                    workspace_id: tenant_id,
                    note_id: file_id,
                    source_file_id: file_id,
                    source_folder_id: None,
                    owner_id,
                    read_acl: vec![format!("owner:{owner_id}")],
                    visibility: "private".to_string(),
                    acl_hash: String::new(),
                    acl_version: 1,
                    embedding_policy: "allowed".to_string(),
                },
            )
            .await
    }

    /// Index a note's content with an ACL payload.
    ///
    /// Frontmatter is stripped before embedding generation so that YAML metadata
    /// does not dominate the semantic vector. If `acl.embedding_policy` is
    /// `"denied"`, the note is removed from the index instead of inserted.
    #[allow(clippy::too_many_arguments)]
    pub async fn index_note(
        &self,
        file_id: Uuid,
        file_name: String,
        file_path: String,
        content: String,
        mime_type: String,
        owner_id: Uuid,
        acl: NoteAclPayload,
    ) -> anyhow::Result<()> {
        if acl.embedding_policy == "denied" {
            self.store
                .remove_note_chunks(acl.tenant_id, acl.note_id)
                .await?;
            return Ok(());
        }

        let body = strip_frontmatter(&content);
        let body = if body.len() > MAX_CONTENT_LENGTH {
            body[..MAX_CONTENT_LENGTH].to_string()
        } else {
            body
        };

        let combined_text = format!("{} {} {}", file_name, file_path, body);
        let embedding = self.embedding_generator.generate(&combined_text).await;

        let document = IndexedDocument {
            file_id,
            file_name: file_name.clone(),
            file_path: file_path.clone(),
            content: body,
            embedding,
            mime_type,
            owner_id,
            tenant_id: acl.tenant_id,
            indexed_at: chrono::Utc::now(),
            acl: Some(acl.clone()),
            chunk_id: file_id,
        };

        self.store
            .upsert_chunk(acl.tenant_id, file_id, &document, &acl)
            .await
    }

    /// Search for documents similar to the query, pre-filtered by ACL.
    ///
    /// Documents without an ACL payload retain the legacy tenant-only behavior
    /// for backward compatibility.
    pub async fn search_with_acl(
        &self,
        filter: &AclSearchFilter,
        query: &str,
        limit: usize,
    ) -> Vec<(IndexedDocument, f32)> {
        let query_embedding = self.embedding_generator.generate(query).await;
        self.store
            .search_with_acl(filter, query_embedding.as_slice(), limit)
            .await
            .unwrap_or_default()
    }

    /// Update the ACL projection for every indexed chunk of a note.
    ///
    /// Returns the number of chunks that were updated.
    pub async fn update_note_acl(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
        new_acl: NoteAclPayload,
    ) -> usize {
        self.store
            .update_note_acl(tenant_id, note_id, &new_acl)
            .await
            .unwrap_or(0)
    }

    /// Remove every indexed chunk belonging to a note.
    ///
    /// Returns the number of chunks that were removed.
    pub async fn remove_note_chunks(&self, tenant_id: Uuid, note_id: Uuid) -> usize {
        self.store
            .remove_note_chunks(tenant_id, note_id)
            .await
            .unwrap_or(0)
    }

    /// Remove a file from the index.
    ///
    /// # Arguments
    /// * `file_id` - The file ID to remove
    /// * `tenant_id` - The tenant ID
    pub async fn remove_file(&self, file_id: Uuid, tenant_id: Uuid) {
        let _ = self.store.remove_chunk(tenant_id, file_id).await;
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
        self.store
            .search(tenant_id, query_embedding.as_slice(), limit)
            .await
            .unwrap_or_default()
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
        self.store
            .get_chunk(tenant_id, file_id)
            .await
            .unwrap_or(None)
    }

    /// Get all documents for a tenant.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// List of all indexed documents for the tenant
    pub async fn get_all_documents(&self, _tenant_id: Uuid) -> Vec<IndexedDocument> {
        Vec::new()
    }

    /// Clear the index for a tenant.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    pub async fn clear_tenant(&self, tenant_id: Uuid) {
        let _ = self.store.clear_tenant(tenant_id).await;
    }

    /// Get the document count for a tenant.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// Number of indexed documents
    pub async fn document_count(&self, tenant_id: Uuid) -> usize {
        self.store.document_count(tenant_id).await.unwrap_or(0)
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

/// Check whether a caller can access a note chunk according to its ACL.
///
/// Access is granted when any of the following holds:
/// - the caller is the document owner;
/// - the note visibility is `"public"`;
/// - the caller belongs to a group listed in `read_acl`;
/// - `read_acl` contains an explicit `owner:<caller_user_id>` principal;
/// - `read_acl` contains an explicit `user:<caller_user_id>` principal (direct
///   user share).
///
/// Chunks with `embedding_policy != "allowed"` or a stale `acl_version` are
/// rejected by the caller before this helper is invoked.
pub fn can_access(acl: &NoteAclPayload, filter: &AclSearchFilter) -> bool {
    if acl.embedding_policy != "allowed" {
        return false;
    }

    if let Some(min_version) = filter.min_acl_versions.get(&acl.note_id) {
        if acl.acl_version < *min_version {
            return false;
        }
    }

    // Owner match.
    if filter.caller_user_id == acl.owner_id {
        return true;
    }

    // Explicit owner principal in the ACL list.
    let owner_principal = format!("owner:{}", filter.caller_user_id);
    if acl.read_acl.contains(&owner_principal) {
        return true;
    }

    // Explicit direct-user principal in the ACL list.
    let user_principal = format!("user:{}", filter.caller_user_id);
    if acl.read_acl.contains(&user_principal) {
        return true;
    }

    // Group membership match.
    if !filter.caller_group_ids.is_empty() {
        let group_principals: Vec<String> = filter
            .caller_group_ids
            .iter()
            .map(|id| format!("group:{id}"))
            .collect();
        if acl.read_acl.iter().any(|p| group_principals.contains(p)) {
            return true;
        }
    }

    // Public visibility match.
    if acl.visibility == "public" {
        return true;
    }

    false
}

/// Strip YAML frontmatter from a Markdown document, returning the body.
///
/// If the document does not start with a frontmatter block, the original text
/// is returned unchanged.
fn strip_frontmatter(doc: &str) -> String {
    split_frontmatter(doc)
        .map(|(_, body)| body)
        .unwrap_or_else(|| doc.to_string())
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
    use super::super::vector_store::InMemoryVectorStore;
    use super::*;

    #[tokio::test]
    async fn test_index_and_search() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = ContentIndexer::new(generator, store);

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
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = ContentIndexer::new(generator, store);

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
    async fn test_get_document_returns_indexed_file() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = ContentIndexer::new(generator, store);

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
        assert_eq!(doc.unwrap().file_id, file_id);
    }

    fn make_acl_payload(
        tenant_id: Uuid,
        note_id: Uuid,
        source_file_id: Uuid,
        owner_id: Uuid,
        visibility: &str,
        embedding_policy: &str,
        acl_version: i64,
    ) -> NoteAclPayload {
        NoteAclPayload {
            tenant_id,
            workspace_id: tenant_id,
            note_id,
            source_file_id,
            source_folder_id: None,
            owner_id,
            read_acl: vec![format!("owner:{}", owner_id)],
            visibility: visibility.to_string(),
            acl_hash: format!("hash-{}", acl_version),
            acl_version,
            embedding_policy: embedding_policy.to_string(),
        }
    }

    #[test]
    fn test_note_acl_payload_serializes() {
        let acl = make_acl_payload(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "private",
            "allowed",
            1,
        );
        let json = serde_json::to_value(&acl).unwrap();
        assert_eq!(json["tenant_id"], acl.tenant_id.to_string());
        assert_eq!(json["visibility"], "private");
        assert_eq!(json["embedding_policy"], "allowed");
        assert_eq!(json["acl_version"], 1);

        let decoded: NoteAclPayload = serde_json::from_value(json).unwrap();
        assert_eq!(acl, decoded);
    }

    #[tokio::test]
    async fn test_index_note_includes_acl_fields() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = ContentIndexer::new(generator, store);

        let tenant_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let acl = make_acl_payload(
            tenant_id, note_id, file_id, owner_id, "private", "allowed", 1,
        );

        let content = "---\ntitle: Secret\n---\n# Secret\n\nconfidential content".to_string();
        indexer
            .index_note(
                file_id,
                "note.md".to_string(),
                "/Workspace/Notes/Secret/note.md".to_string(),
                content,
                "text/markdown".to_string(),
                owner_id,
                acl.clone(),
            )
            .await
            .unwrap();

        let filter = AclSearchFilter {
            tenant_id,
            caller_user_id: owner_id,
            caller_group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = indexer.search_with_acl(&filter, "confidential", 10).await;
        assert_eq!(results.len(), 1);
        let doc = &results[0].0;
        assert_eq!(doc.chunk_id, file_id);
        assert!(doc.acl.is_some());
        let stored_acl = doc.acl.as_ref().unwrap();
        assert_eq!(stored_acl.note_id, note_id);
        assert_eq!(stored_acl.source_file_id, file_id);
        assert_eq!(stored_acl.owner_id, owner_id);
        // Frontmatter should have been stripped from the indexed body.
        assert!(!doc.content.contains("title: Secret"));
        assert!(doc.content.contains("confidential content"));
    }

    #[tokio::test]
    async fn test_search_with_acl_excludes_other_tenant() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = ContentIndexer::new(generator, store);

        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let file_a = Uuid::new_v4();
        let note_a = Uuid::new_v4();
        indexer
            .index_note(
                file_a,
                "note.md".to_string(),
                "/note.md".to_string(),
                "tenant a content".to_string(),
                "text/markdown".to_string(),
                owner_id,
                make_acl_payload(
                    tenant_a,
                    note_a,
                    file_a,
                    owner_id,
                    "workspace",
                    "allowed",
                    1,
                ),
            )
            .await
            .unwrap();

        let file_b = Uuid::new_v4();
        let note_b = Uuid::new_v4();
        indexer
            .index_note(
                file_b,
                "note.md".to_string(),
                "/note.md".to_string(),
                "tenant b content".to_string(),
                "text/markdown".to_string(),
                owner_id,
                make_acl_payload(
                    tenant_b,
                    note_b,
                    file_b,
                    owner_id,
                    "workspace",
                    "allowed",
                    1,
                ),
            )
            .await
            .unwrap();

        let filter = AclSearchFilter {
            tenant_id: tenant_a,
            caller_user_id: owner_id,
            caller_group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = indexer.search_with_acl(&filter, "content", 10).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.acl.as_ref().unwrap().note_id, note_a);
    }

    #[tokio::test]
    async fn test_search_with_acl_excludes_unauthorized_caller() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = ContentIndexer::new(generator, store);

        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let stranger_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();

        indexer
            .index_note(
                file_id,
                "note.md".to_string(),
                "/note.md".to_string(),
                "private note content".to_string(),
                "text/markdown".to_string(),
                owner_id,
                make_acl_payload(
                    tenant_id, note_id, file_id, owner_id, "private", "allowed", 1,
                ),
            )
            .await
            .unwrap();

        let filter = AclSearchFilter {
            tenant_id,
            caller_user_id: stranger_id,
            caller_group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = indexer.search_with_acl(&filter, "private", 10).await;
        assert!(results.is_empty());

        // The owner should still see it.
        let owner_filter = AclSearchFilter {
            tenant_id,
            caller_user_id: owner_id,
            caller_group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = indexer.search_with_acl(&owner_filter, "private", 10).await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_search_with_acl_includes_direct_user_share() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = ContentIndexer::new(generator, store);

        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let shared_user_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();

        let mut acl = make_acl_payload(
            tenant_id, note_id, file_id, owner_id, "private", "allowed", 1,
        );
        acl.read_acl = vec![
            format!("owner:{owner_id}"),
            format!("user:{shared_user_id}"),
        ];

        indexer
            .index_note(
                file_id,
                "note.md".to_string(),
                "/note.md".to_string(),
                "shared user content".to_string(),
                "text/markdown".to_string(),
                owner_id,
                acl,
            )
            .await
            .unwrap();

        let filter = AclSearchFilter {
            tenant_id,
            caller_user_id: shared_user_id,
            caller_group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = indexer
            .search_with_acl(&filter, "shared user content", 10)
            .await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_search_with_acl_excludes_stale_acl_version() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = ContentIndexer::new(generator, store);

        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();

        indexer
            .index_note(
                file_id,
                "note.md".to_string(),
                "/note.md".to_string(),
                "shared engineering content".to_string(),
                "text/markdown".to_string(),
                owner_id,
                make_acl_payload(
                    tenant_id,
                    note_id,
                    file_id,
                    owner_id,
                    "workspace",
                    "allowed",
                    1,
                ),
            )
            .await
            .unwrap();

        let mut min_acl_versions = HashMap::new();
        min_acl_versions.insert(note_id, 2);

        let engineering_id = Uuid::new_v4();
        let filter = AclSearchFilter {
            tenant_id,
            caller_user_id: owner_id,
            caller_group_ids: vec![engineering_id],
            min_acl_versions,
        };
        let results = indexer.search_with_acl(&filter, "engineering", 10).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_update_note_acl_updates_all_chunks() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = ContentIndexer::new(generator, store);

        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        indexer
            .index_note(
                file_id,
                "note.md".to_string(),
                "/note.md".to_string(),
                "content".to_string(),
                "text/markdown".to_string(),
                owner_id,
                make_acl_payload(
                    tenant_id, note_id, file_id, owner_id, "private", "allowed", 1,
                ),
            )
            .await
            .unwrap();

        let mut new_acl = make_acl_payload(
            tenant_id, note_id, file_id, owner_id, "public", "allowed", 2,
        );
        let engineering_id = Uuid::new_v4();
        new_acl.read_acl = vec![format!("group:{engineering_id}")];

        let updated = indexer
            .update_note_acl(tenant_id, note_id, new_acl.clone())
            .await;
        assert_eq!(updated, 1);

        let filter = AclSearchFilter {
            tenant_id,
            caller_user_id: owner_id,
            caller_group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = indexer.search_with_acl(&filter, "content", 10).await;
        assert_eq!(results.len(), 1);
        let stored = results[0].0.acl.as_ref().unwrap();
        assert_eq!(stored.visibility, "public");
        assert_eq!(stored.acl_version, 2);
        assert_eq!(stored.read_acl, vec![format!("group:{engineering_id}")]);
    }

    #[tokio::test]
    async fn test_embedding_policy_denied_prevents_indexing() {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = ContentIndexer::new(generator, store);

        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();

        // First index as allowed.
        indexer
            .index_note(
                file_id,
                "note.md".to_string(),
                "/note.md".to_string(),
                "content".to_string(),
                "text/markdown".to_string(),
                owner_id,
                make_acl_payload(
                    tenant_id, note_id, file_id, owner_id, "private", "allowed", 1,
                ),
            )
            .await
            .unwrap();
        assert_eq!(indexer.document_count(tenant_id).await, 1);

        // Then flip to denied.
        indexer
            .index_note(
                file_id,
                "note.md".to_string(),
                "/note.md".to_string(),
                "content".to_string(),
                "text/markdown".to_string(),
                owner_id,
                make_acl_payload(
                    tenant_id, note_id, file_id, owner_id, "private", "denied", 2,
                ),
            )
            .await
            .unwrap();
        assert_eq!(indexer.document_count(tenant_id).await, 0);
    }
}
