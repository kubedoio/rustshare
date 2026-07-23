//! Content indexing for AI-powered search.
//!
//! This module provides:
//! - Document extraction from files
//! - In-memory content indexing
//! - Background indexing job support

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

use serde::{Deserialize, Serialize};

use super::embedding::{Embedding, EmbeddingGenerator};
use super::vector_store::VectorStore;
use crate::okf::frontmatter::split_frontmatter;

// Introduced for permission-aware indexing; used in Tasks 6-10.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexVisibility {
    Private,
    Workspace,
    Public,
}

impl std::fmt::Display for IndexVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Private => write!(f, "private"),
            Self::Workspace => write!(f, "workspace"),
            Self::Public => write!(f, "public"),
        }
    }
}

impl FromStr for IndexVisibility {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "private" => Ok(Self::Private),
            "workspace" => Ok(Self::Workspace),
            "public" => Ok(Self::Public),
            _ => Err(anyhow::anyhow!("unknown visibility: {s}")),
        }
    }
}

// Introduced for permission-aware indexing; used in Tasks 6-10.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingPolicy {
    #[default]
    Allowed,
    Denied,
}

impl std::fmt::Display for EmbeddingPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Allowed => write!(f, "allowed"),
            Self::Denied => write!(f, "denied"),
        }
    }
}

impl FromStr for EmbeddingPolicy {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "allowed" => Ok(Self::Allowed),
            "denied" => Ok(Self::Denied),
            _ => Err(anyhow::anyhow!("unknown embedding policy: {s}")),
        }
    }
}

/// A typed principal that may appear in an indexed ACL.
// Introduced for permission-aware indexing; used in Tasks 6-10.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum IndexPrincipal {
    Owner(Uuid),
    User(Uuid),
    Group(Uuid),
    Workspace(Uuid),
    Public,
}

impl std::fmt::Display for IndexPrincipal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Owner(id) => write!(f, "owner:{id}"),
            Self::User(id) => write!(f, "user:{id}"),
            Self::Group(id) => write!(f, "group:{id}"),
            Self::Workspace(id) => write!(f, "workspace:{id}"),
            Self::Public => write!(f, "public"),
        }
    }
}

impl FromStr for IndexPrincipal {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "public" {
            return Ok(Self::Public);
        }
        let (kind, id) = s
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("principal missing colon: {s}"))?;
        let id =
            Uuid::parse_str(id).map_err(|e| anyhow::anyhow!("invalid principal uuid {id}: {e}"))?;
        match kind {
            "owner" => Ok(Self::Owner(id)),
            "user" => Ok(Self::User(id)),
            "group" => Ok(Self::Group(id)),
            "workspace" => Ok(Self::Workspace(id)),
            _ => Err(anyhow::anyhow!("unknown principal kind: {kind}")),
        }
    }
}

/// Canonical ACL projection for an indexed object.
// Introduced for permission-aware indexing; used in Tasks 6-10.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct IndexAclProjection {
    pub tenant_id: Uuid,
    pub workspace_id: Uuid,
    pub object_id: Uuid,
    pub owner_id: Uuid,
    pub read_principals: Vec<IndexPrincipal>,
    pub visibility: IndexVisibility,
    pub acl_hash: String,
    pub acl_version: i64,
    pub embedding_policy: EmbeddingPolicy,
}

/// Principal used at retrieval time.
// Introduced for permission-aware indexing; used in Tasks 6-10.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct RetrievalPrincipal {
    pub tenant_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub user_id: Uuid,
    pub group_ids: Vec<Uuid>,
    /// object_id -> minimum accepted acl_version.
    pub min_acl_versions: HashMap<Uuid, i64>,
}

// Introduced for permission-aware indexing; used in Tasks 6-10.
#[allow(dead_code)]
impl RetrievalPrincipal {
    /// Return the principal strings to match against the stored ACL.
    pub fn to_index_principals(&self) -> Vec<String> {
        let mut out = vec![
            format!("owner:{}", self.user_id),
            format!("user:{}", self.user_id),
        ];
        for gid in &self.group_ids {
            out.push(format!("group:{gid}"));
        }
        if let Some(wid) = self.workspace_id {
            out.push(format!("workspace:{wid}"));
        }
        out
    }
}

/// Validate a `NoteAclPayload` and project it into the typed `IndexAclProjection`.
///
/// Returns an error if any field is malformed or if the embedding policy is not
/// `"allowed"`. This is the fail-closed validation used at retrieval time.
pub fn validate_and_project(acl: &NoteAclPayload) -> anyhow::Result<IndexAclProjection> {
    let visibility = IndexVisibility::from_str(&acl.visibility)?;
    let embedding_policy = EmbeddingPolicy::from_str(&acl.embedding_policy)?;
    if embedding_policy != EmbeddingPolicy::Allowed {
        return Err(anyhow::anyhow!(
            "embedding policy is not allowed for note_id={}",
            acl.note_id
        ));
    }
    let read_principals = acl
        .read_acl
        .iter()
        .map(|s| IndexPrincipal::from_str(s))
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(IndexAclProjection {
        tenant_id: acl.tenant_id,
        workspace_id: acl.workspace_id,
        object_id: acl.note_id,
        owner_id: acl.owner_id,
        read_principals,
        visibility,
        acl_hash: acl.acl_hash.clone(),
        acl_version: acl.acl_version,
        embedding_policy,
    })
}

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
    /// Resolved read principals, e.g. `["owner:<uuid>", "group:<uuid>"]`.
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

        let acl_payload = NoteAclPayload {
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
        };

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
            acl: Some(acl_payload.clone()),
            chunk_id: file_id,
        };

        self.store
            .upsert_chunk(tenant_id, file_id, &document, &acl_payload)
            .await
    }

    /// Index a note's content with an ACL projection.
    ///
    /// Frontmatter is stripped before embedding generation so that YAML metadata
    /// does not dominate the semantic vector. If `acl.embedding_policy` is
    /// [`EmbeddingPolicy::Denied`], the note is removed from the index instead
    /// of inserted.
    #[allow(clippy::too_many_arguments)]
    pub async fn index_note(
        &self,
        file_id: Uuid,
        file_name: String,
        file_path: String,
        content: String,
        mime_type: String,
        owner_id: Uuid,
        acl: IndexAclProjection,
    ) -> anyhow::Result<()> {
        if acl.embedding_policy == EmbeddingPolicy::Denied {
            self.store
                .remove_note_chunks(acl.tenant_id, acl.object_id)
                .await?;
            return Ok(());
        }

        let payload = note_acl_payload_from_projection(file_id, &acl);

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
            acl: Some(payload.clone()),
            chunk_id: file_id,
        };

        self.store
            .upsert_chunk(acl.tenant_id, file_id, &document, &payload)
            .await
    }

    /// Search for documents similar to the query, pre-filtered by ACL.
    ///
    /// Documents without a valid, allowed ACL payload are rejected; retrieval
    /// fails closed.
    pub async fn search_with_acl(
        &self,
        principal: &RetrievalPrincipal,
        query: &str,
        limit: usize,
    ) -> Vec<(IndexedDocument, f32)> {
        let query_embedding = self.embedding_generator.generate(query).await;
        self.store
            .search_with_acl(principal, query_embedding.as_slice(), limit)
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

    /// Get a document by file ID, enforcing the caller's ACL.
    ///
    /// # Arguments
    /// * `file_id` - The file ID
    /// * `principal` - The authenticated retrieval principal
    ///
    /// # Returns
    /// The indexed document if found and the caller is authorized
    pub async fn get_document(
        &self,
        file_id: Uuid,
        principal: &RetrievalPrincipal,
    ) -> Option<IndexedDocument> {
        self.get_chunk(principal, file_id).await
    }

    /// Look up a single chunk by id, enforcing the caller's ACL.
    async fn get_chunk(
        &self,
        principal: &RetrievalPrincipal,
        chunk_id: Uuid,
    ) -> Option<IndexedDocument> {
        self.store
            .get_chunk(principal, chunk_id)
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

/// Convert a typed [`IndexAclProjection`] into the storage-level
/// [`NoteAclPayload`].
///
/// The storage payload keeps the stringified forms of enums and principals so
/// existing vector-store implementations and database columns continue to work
/// without migration.
fn note_acl_payload_from_projection(
    file_id: Uuid,
    acl: &IndexAclProjection,
) -> NoteAclPayload {
    NoteAclPayload {
        tenant_id: acl.tenant_id,
        workspace_id: acl.workspace_id,
        note_id: acl.object_id,
        source_file_id: file_id,
        source_folder_id: None,
        owner_id: acl.owner_id,
        read_acl: acl.read_principals.iter().map(|p| p.to_string()).collect(),
        visibility: acl.visibility.to_string(),
        acl_hash: acl.acl_hash.clone(),
        acl_version: acl.acl_version,
        embedding_policy: acl.embedding_policy.to_string(),
    }
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
        let owner_id = Uuid::new_v4();

        // Index some documents
        indexer
            .index_file(
                Uuid::new_v4(),
                "rust_guide.md".to_string(),
                "/docs/rust_guide.md".to_string(),
                "Rust is a systems programming language with memory safety".to_string(),
                "text/markdown".to_string(),
                owner_id,
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
                owner_id,
                tenant_id,
            )
            .await
            .unwrap();

        // Search for Rust-related content as the owner
        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = indexer
            .search_with_acl(&principal, "memory safety programming", 10)
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
        let owner_id = Uuid::new_v4();

        indexer
            .index_file(
                file_id,
                "test.txt".to_string(),
                "/test.txt".to_string(),
                "test content".to_string(),
                "text/plain".to_string(),
                owner_id,
                tenant_id,
            )
            .await
            .unwrap();

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let doc = indexer.get_document(file_id, &principal).await;
        assert!(doc.is_some());
        assert_eq!(doc.unwrap().file_id, file_id);
    }

    fn make_acl_payload(
        tenant_id: Uuid,
        note_id: Uuid,
        _source_file_id: Uuid,
        owner_id: Uuid,
        visibility: &str,
        embedding_policy: &str,
        acl_version: i64,
    ) -> IndexAclProjection {
        IndexAclProjection {
            tenant_id,
            workspace_id: tenant_id,
            object_id: note_id,
            owner_id,
            read_principals: vec![format!("owner:{}", owner_id).parse().unwrap()],
            visibility: visibility.parse().unwrap(),
            acl_hash: format!("hash-{}", acl_version),
            acl_version,
            embedding_policy: embedding_policy.parse().unwrap(),
        }
    }

    #[test]
    fn test_note_acl_payload_serializes() {
        let file_id = Uuid::new_v4();
        let acl = make_acl_payload(
            Uuid::new_v4(),
            Uuid::new_v4(),
            file_id,
            Uuid::new_v4(),
            "private",
            "allowed",
            1,
        );
        let payload = note_acl_payload_from_projection(file_id, &acl);
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["tenant_id"], payload.tenant_id.to_string());
        assert_eq!(json["visibility"], "private");
        assert_eq!(json["embedding_policy"], "allowed");
        assert_eq!(json["acl_version"], 1);

        let decoded: NoteAclPayload = serde_json::from_value(json).unwrap();
        assert_eq!(payload, decoded);
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

        let filter = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
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

        let principal = RetrievalPrincipal {
            tenant_id: tenant_a,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = indexer.search_with_acl(&principal, "content", 10).await;
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

        let filter = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: stranger_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = indexer.search_with_acl(&filter, "private", 10).await;
        assert!(results.is_empty());

        // The owner should still see it.
        let owner_filter = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
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
        acl.read_principals = vec![
            format!("owner:{owner_id}").parse().unwrap(),
            format!("user:{shared_user_id}").parse().unwrap(),
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

        let filter = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: shared_user_id,
            group_ids: vec![],
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
        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![engineering_id],
            min_acl_versions,
        };
        let results = indexer.search_with_acl(&principal, "engineering", 10).await;
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
        new_acl.read_principals = vec![format!("group:{engineering_id}").parse().unwrap()];

        let updated = indexer
            .update_note_acl(tenant_id, note_id, note_acl_payload_from_projection(file_id, &new_acl))
            .await;
        assert_eq!(updated, 1);

        let filter = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
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

    #[test]
    fn index_visibility_round_trip() {
        for vis in [
            IndexVisibility::Private,
            IndexVisibility::Workspace,
            IndexVisibility::Public,
        ] {
            let s = vis.to_string();
            let parsed: IndexVisibility = s.parse().unwrap();
            assert_eq!(parsed, vis);
        }
        assert!("unknown".parse::<IndexVisibility>().is_err());
    }

    #[test]
    fn embedding_policy_round_trip() {
        for policy in [EmbeddingPolicy::Allowed, EmbeddingPolicy::Denied] {
            let s = policy.to_string();
            let parsed: EmbeddingPolicy = s.parse().unwrap();
            assert_eq!(parsed, policy);
        }
        assert_eq!(EmbeddingPolicy::default(), EmbeddingPolicy::Allowed);
        assert!("blocked".parse::<EmbeddingPolicy>().is_err());
    }

    #[test]
    fn index_principal_round_trip() {
        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();

        for principal in [
            IndexPrincipal::Owner(owner_id),
            IndexPrincipal::User(user_id),
            IndexPrincipal::Group(group_id),
            IndexPrincipal::Workspace(workspace_id),
            IndexPrincipal::Public,
        ] {
            let s = principal.to_string();
            let parsed: IndexPrincipal = s.parse().unwrap();
            assert_eq!(parsed, principal);
        }

        assert_eq!(
            "public".parse::<IndexPrincipal>().unwrap(),
            IndexPrincipal::Public
        );
        assert!("garbage".parse::<IndexPrincipal>().is_err());
        assert!("user:not-a-uuid".parse::<IndexPrincipal>().is_err());
        assert!("unknown:00000000-0000-0000-0000-000000000000"
            .parse::<IndexPrincipal>()
            .is_err());
    }

    #[test]
    fn index_principal_serializes_with_kind_and_id() {
        let user_id = Uuid::new_v4();
        let principal = IndexPrincipal::User(user_id);
        let json = serde_json::to_value(&principal).unwrap();
        assert_eq!(json["kind"], "user");
        assert_eq!(json["id"], user_id.to_string());

        let public = IndexPrincipal::Public;
        let json = serde_json::to_value(&public).unwrap();
        assert_eq!(json["kind"], "public");
        assert!(json["id"].is_null());
    }

    #[test]
    fn retrieval_principal_to_index_principals() {
        let user_id = Uuid::new_v4();
        let group_a = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();

        let principal = RetrievalPrincipal {
            tenant_id: Uuid::new_v4(),
            workspace_id: Some(workspace_id),
            user_id,
            group_ids: vec![group_a],
            min_acl_versions: HashMap::new(),
        };

        let principals = principal.to_index_principals();
        assert!(principals.contains(&format!("owner:{user_id}")));
        assert!(principals.contains(&format!("user:{user_id}")));
        assert!(principals.contains(&format!("group:{group_a}")));
        assert!(principals.contains(&format!("workspace:{workspace_id}")));
    }
}
