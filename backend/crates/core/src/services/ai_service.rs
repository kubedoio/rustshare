//! AI Service for RustShare
//!
//! This service provides AI-powered features:
//! - Permission-filtered semantic search
//! - File summarization
//! - RAG-based Q&A with citations
//!
//! Contract A-01 through A-07: AI Safety
//! - A-01: All results are permission-filtered
//! - A-02: All responses include source citations
//! - A-03: No hallucinations - only content from indexed files
//! - A-04: Rate limiting enforced at handler level
//! - A-05: Input validation and sanitization
//! - A-06: Content boundaries respected
//! - A-07: Tenant isolation maintained

use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::{FileId, SharePermissions, UserId};
use crate::services::permission_resolver::{PermissionResolver, PermissionResolverOps, Resource};

use super::ai::indexing::{
    is_hidden_file_name, ContentIndexer, IndexAclProjection, IndexedDocument, RetrievalPrincipal,
};
use super::ai::EmbeddingGenerator;

/// Errors that can occur during AI operations.
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    /// User lacks permission to access this resource.
    #[error("Permission denied: user {user_id} cannot access resource")]
    PermissionDenied { user_id: UserId },

    /// File not found or not indexed.
    #[error("File not found: {0}")]
    FileNotFound(FileId),

    /// Content not available for this file type.
    #[error("Content not extractable for file type: {0}")]
    ContentNotExtractable(String),

    /// Rate limit exceeded.
    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    /// Invalid query.
    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    /// Internal error.
    #[error("AI service error: {0}")]
    Internal(String),
}

/// A search result with relevance score and citation.
#[derive(Debug, Clone)]
pub struct SemanticSearchResult {
    /// The file ID
    pub file_id: FileId,
    /// File name
    pub file_name: String,
    /// Full file path
    pub file_path: String,
    /// Relevance score (0.0 to 1.0)
    pub relevance_score: f32,
    /// Content snippet (sanitized)
    pub snippet: String,
    /// MIME type
    pub mime_type: String,
    /// Owner ID
    pub owner_id: UserId,
    /// Whether user has edit permission
    pub can_edit: bool,
}

/// A source citation for AI-generated content.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceCitation {
    /// The file ID
    pub file_id: String,
    /// File name
    pub file_name: String,
    /// Full file path
    pub file_path: String,
    /// Relevance score
    pub relevance_score: f32,
    /// Content excerpt
    pub excerpt: String,
}

/// A summary of a file's content.
#[derive(Debug, Clone)]
pub struct FileSummary {
    /// The file ID
    pub file_id: FileId,
    /// File name
    pub file_name: String,
    /// Generated summary
    pub summary: String,
    /// Key topics/themes extracted
    pub key_topics: Vec<String>,
    /// Source citation
    pub citation: SourceCitation,
}

/// AI Service for RustShare.
///
/// Provides permission-aware AI features with safety guarantees.
pub struct AiService<EG, PR>
where
    EG: EmbeddingGenerator,
    PR: PermissionResolverOps,
{
    /// The content indexer for semantic search
    indexer: Arc<ContentIndexer<EG>>,
    /// The permission resolver for access control
    permission_resolver: Arc<PermissionResolver<PR>>,
}

impl<EG, PR> AiService<EG, PR>
where
    EG: EmbeddingGenerator,
    PR: PermissionResolverOps,
{
    /// Create a new AI service.
    pub fn new(
        indexer: Arc<ContentIndexer<EG>>,
        permission_resolver: Arc<PermissionResolver<PR>>,
    ) -> Self {
        Self {
            indexer,
            permission_resolver,
        }
    }

    /// Perform permission-filtered semantic search.
    ///
    /// # Arguments
    /// * `query` - The search query
    /// * `user_id` - The user performing the search
    /// * `tenant_id` - The tenant ID
    /// * `limit` - Maximum number of results
    ///
    /// # Returns
    /// List of search results the user has permission to view.
    ///
    /// # Contract A-01: Permission Filtering
    /// Only returns files the user has View permission or higher on.
    pub async fn semantic_search(
        &self,
        query: &str,
        user_id: UserId,
        tenant_id: Uuid,
        limit: usize,
    ) -> Result<Vec<SemanticSearchResult>, AiError> {
        // Validate query
        let query = query.trim();
        if query.is_empty() {
            return Err(AiError::InvalidQuery("Query cannot be empty".to_string()));
        }
        if query.len() > 1000 {
            return Err(AiError::InvalidQuery(
                "Query too long (max 1000 chars)".to_string(),
            ));
        }

        // Resolve the caller's group IDs for ACL pre-filtering.
        let group_ids = match self
            .permission_resolver
            .resolve_user_group_ids(user_id, tenant_id)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!(
                    "Failed to resolve group IDs for user {} in tenant {}: {}. Continuing with empty groups.",
                    user_id,
                    tenant_id,
                    e
                );
                Vec::new()
            }
        };

        // Build a retrieval principal for permission-aware search.
        // In the current domain each tenant maps to exactly one workspace, so the
        // caller's workspace scope is the tenant. `File::workspace_id()` documents
        // this identity guarantee.
        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: Some(tenant_id),
            user_id,
            group_ids,
            min_acl_versions: HashMap::new(),
        };

        // Perform semantic search
        let raw_results = self
            .indexer
            .search_with_acl(&principal, query, limit * 3)
            .await;

        // Filter by permission and build results
        let results = self
            .build_search_results(user_id, tenant_id, limit, raw_results)
            .await;

        Ok(results)
    }

    /// Perform permission-filtered keyword search.
    ///
    /// Same contract as [`Self::semantic_search`], but matches the query terms
    /// against the indexed file name/path/content instead of vector similarity.
    ///
    /// # Contract A-01: Permission Filtering
    /// Only returns files the user has View permission or higher on.
    pub async fn keyword_search(
        &self,
        query: &str,
        user_id: UserId,
        tenant_id: Uuid,
        limit: usize,
    ) -> Result<Vec<SemanticSearchResult>, AiError> {
        // Validate query
        let query = query.trim();
        if query.is_empty() {
            return Err(AiError::InvalidQuery("Query cannot be empty".to_string()));
        }
        if query.len() > 1000 {
            return Err(AiError::InvalidQuery(
                "Query too long (max 1000 chars)".to_string(),
            ));
        }

        // Resolve the caller's group IDs for ACL pre-filtering.
        let group_ids = match self
            .permission_resolver
            .resolve_user_group_ids(user_id, tenant_id)
            .await
        {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!(
                    "Failed to resolve group IDs for user {} in tenant {}: {}. Continuing with empty groups.",
                    user_id,
                    tenant_id,
                    e
                );
                Vec::new()
            }
        };

        // Build a retrieval principal for permission-aware search.
        // In the current domain each tenant maps to exactly one workspace, so the
        // caller's workspace scope is the tenant. `File::workspace_id()` documents
        // this identity guarantee.
        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: Some(tenant_id),
            user_id,
            group_ids,
            min_acl_versions: HashMap::new(),
        };

        // Perform keyword search. Candidates are fetched 3x over so the
        // permission post-filter can drop results without starving the limit.
        let raw_results = self
            .indexer
            .keyword_search_with_acl(&principal, query, limit * 3)
            .await;

        // Filter by permission and build results
        let results = self
            .build_search_results(user_id, tenant_id, limit, raw_results)
            .await;

        Ok(results)
    }

    /// Shared tail of [`Self::semantic_search`] and [`Self::keyword_search`]:
    /// drops hidden metadata files, filters by effective permission
    /// (Contract A-01; unresolvable permission skips the result), and builds
    /// [`SemanticSearchResult`]s, stopping once `limit` results are produced.
    async fn build_search_results(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        limit: usize,
        raw_results: Vec<(IndexedDocument, f32)>,
    ) -> Vec<SemanticSearchResult> {
        // Filter out hidden metadata files
        let raw_results: Vec<_> = raw_results
            .into_iter()
            .filter(|(doc, _)| !is_hidden_file_name(&doc.file_name))
            .collect();

        // Filter by permission and build results
        let mut results = Vec::new();
        for (document, score) in raw_results {
            // Check permission - Contract A-01
            let resource = Resource::File(document.file_id);
            let permission = match self
                .permission_resolver
                .resolve_permission(user_id, tenant_id, resource)
                .await
            {
                Ok(perm) => perm,
                Err(e) => {
                    tracing::warn!(
                        "Permission resolution failed for file {}: {}. Skipping.",
                        document.file_id,
                        e
                    );
                    continue;
                }
            };

            if let Some(perm) = permission {
                // Generate snippet (first 200 chars of content)
                let snippet = truncate_with_ellipsis(&document.content, 200);

                // Sanitize snippet
                let snippet = sanitize_snippet(&snippet);

                results.push(SemanticSearchResult {
                    file_id: document.file_id,
                    file_name: document.file_name.clone(),
                    file_path: document.file_path.clone(),
                    relevance_score: score,
                    snippet,
                    mime_type: document.mime_type.clone(),
                    owner_id: document.owner_id,
                    can_edit: perm >= SharePermissions::Edit,
                });

                if results.len() >= limit {
                    break;
                }
            }
        }

        results
    }

    /// Generate a summary of a file if the user has access.
    ///
    /// # Arguments
    /// * `file_id` - The file to summarize
    /// * `user_id` - The user requesting the summary
    ///
    /// # Returns
    /// A summary of the file with citation.
    ///
    /// # Contract A-01, A-02: Permission + Citation
    /// Only summarizes if user has View permission. Always cites source.
    pub async fn summarize_file(
        &self,
        file_id: FileId,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<FileSummary, AiError> {
        // Check permission - Contract A-01
        let resource = Resource::File(file_id);
        let permission = match self
            .permission_resolver
            .resolve_permission(user_id, tenant_id, resource)
            .await
        {
            Ok(perm) => perm,
            Err(e) => {
                tracing::warn!(
                    "Permission resolution failed for file {}: {}. Treating as denied.",
                    file_id,
                    e
                );
                return Err(AiError::PermissionDenied { user_id });
            }
        };

        if permission.is_none() {
            return Err(AiError::PermissionDenied { user_id });
        }

        // Build a retrieval principal for ACL-enforced lookup.
        // In the current domain each tenant maps to exactly one workspace, so the
        // caller's workspace scope is the tenant. `File::workspace_id()` documents
        // this identity guarantee.
        let group_ids = self
            .permission_resolver
            .resolve_user_group_ids(user_id, tenant_id)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "Failed to resolve group IDs for user {} in tenant {}: {}. Continuing with empty groups.",
                    user_id,
                    tenant_id,
                    e
                );
                Vec::new()
            });
        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: Some(tenant_id),
            user_id,
            group_ids,
            min_acl_versions: HashMap::new(),
        };

        // Get the indexed document
        let document = self
            .indexer
            .get_document(file_id, &principal)
            .await
            .ok_or(AiError::FileNotFound(file_id))?;

        // Verify document belongs to this tenant
        if document.tenant_id != tenant_id {
            return Err(AiError::FileNotFound(file_id));
        }

        // Generate summary
        let summary = generate_document_summary(&document);
        let key_topics = extract_key_topics(&document.content);

        // Create citation - Contract A-02
        let citation = SourceCitation {
            file_id: document.file_id.to_string(),
            file_name: document.file_name.clone(),
            file_path: document.file_path.clone(),
            relevance_score: 1.0,
            excerpt: truncate_with_ellipsis(&document.content, 300),
        };

        Ok(FileSummary {
            file_id,
            file_name: document.file_name,
            summary,
            key_topics,
            citation,
        })
    }

    /// Answer a question using RAG (Retrieval-Augmented Generation).
    ///
    /// # Arguments
    /// * `query` - The user's question
    /// * `user_id` - The user asking the question
    /// * `tenant_id` - The tenant ID
    ///
    /// # Returns
    /// Index a file for AI search with an ACL projection.
    ///
    /// # Arguments
    /// * `file_id` - The file ID
    /// * `file_name` - The file name
    /// * `file_path` - The full file path
    /// * `content` - The extracted text content
    /// * `mime_type` - The MIME type
    /// * `acl` - The canonical ACL projection for the object
    ///
    /// # Returns
    /// Ok(()) if successfully indexed
    pub async fn index_file(
        &self,
        file_id: FileId,
        file_name: String,
        file_path: String,
        content: String,
        mime_type: String,
        acl: IndexAclProjection,
    ) -> Result<(), AiError> {
        self.indexer
            .index_file(file_id, file_name, file_path, content, mime_type, acl)
            .await
            .map_err(|e| AiError::Internal(e.to_string()))
    }

    /// Remove a file from the AI index.
    ///
    /// # Arguments
    /// * `file_id` - The file ID
    /// * `tenant_id` - The tenant ID
    pub async fn remove_file(&self, file_id: FileId, tenant_id: Uuid) -> anyhow::Result<()> {
        self.indexer.remove_file(file_id, tenant_id).await
    }

    /// Remove every indexed chunk belonging to a note/file.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID
    /// * `note_id` - The note_id value stored in note_index_chunks.note_id
    ///
    /// Returns the number of chunks that were removed.
    pub async fn remove_note_chunks(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
    ) -> anyhow::Result<usize> {
        self.indexer.remove_note_chunks(tenant_id, note_id).await
    }
}

/// Sanitize a text snippet for display.
fn sanitize_snippet(snippet: &str) -> String {
    snippet
        .chars()
        .map(|c| {
            if c.is_control() && c != '\n' && c != '\t' {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn truncate_with_ellipsis(content: &str, max_chars: usize) -> String {
    let mut chars = content.chars();
    let preview: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        content.to_string()
    }
}

/// Generate a simple document summary.
/// Phase 1.5: Basic summary generation.
/// Future phases: Use LLM for more sophisticated summaries.
fn generate_document_summary(document: &IndexedDocument) -> String {
    let word_count = document.content.split_whitespace().count();

    let content_preview = truncate_with_ellipsis(document.content.trim(), 500);

    format!(
        "This {} file ({}) contains approximately {} words. Preview: {}",
        document.file_name.rsplit('.').next().unwrap_or("text"),
        document.mime_type,
        word_count,
        content_preview
    )
}

/// Extract key topics from content.
/// Phase 1.5: Simple keyword extraction.
fn extract_key_topics(content: &str) -> Vec<String> {
    use std::collections::HashMap;

    let words: Vec<String> = content
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty() && s.len() > 3 && !is_common_word(s))
        .map(|s| s.to_string())
        .collect();

    let mut frequencies: HashMap<String, u32> = HashMap::new();
    for word in words {
        *frequencies.entry(word).or_insert(0) += 1;
    }

    let mut topics: Vec<(String, u32)> = frequencies.into_iter().collect();
    topics.sort_by_key(|topic| std::cmp::Reverse(topic.1));
    topics.truncate(5);

    topics.into_iter().map(|(word, _)| word).collect()
}

/// Check if a word is a common stop word.
fn is_common_word(word: &str) -> bool {
    const STOP_WORDS: &[&str] = &[
        "the", "and", "for", "are", "but", "not", "you", "all", "can", "her", "was", "one", "our",
        "out", "day", "get", "has", "him", "his", "how", "its", "may", "new", "now", "old", "see",
        "two", "who", "boy", "did", "she", "use", "her", "way", "many", "oil", "sit", "set", "run",
        "eat", "far", "sea", "eye", "ask", "own", "say", "too", "any", "try", "let", "put", "end",
        "why", "turn", "here", "show", "every", "good", "give", "our", "under", "name", "very",
        "through", "just", "form", "much", "great", "think", "where", "help", "much", "before",
        "move", "right", "too", "means", "old", "any", "same", "tell", "very", "when", "come",
        "also", "around", "another", "came", "come", "work", "three", "must", "because", "does",
        "part", "even", "place", "well", "such", "here", "take", "than", "them", "these", "time",
        "make", "well", "were", "first", "water", "been", "call", "who", "its", "now", "find",
        "long", "down", "most", "over", "think", "where", "much", "would", "there", "their",
        "what", "said", "each", "which", "will", "about", "could", "other", "after", "made",
        "from", "them", "many", "some", "like", "into", "time", "have", "more", "word", "been",
        "call", "who", "oil", "sit", "set", "run", "eat", "far", "sea",
    ];

    STOP_WORDS.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ai::embedding::SimpleEmbeddingGenerator;
    use crate::services::ai::indexing::{
        EmbeddingPolicy, IndexAclProjection, IndexPrincipal, IndexVisibility,
    };
    use crate::services::{ContentIndexer, InMemoryVectorStore};

    fn make_file_acl(tenant_id: Uuid, file_id: Uuid, owner_id: Uuid) -> IndexAclProjection {
        IndexAclProjection {
            tenant_id,
            workspace_id: tenant_id,
            object_id: file_id,
            source_folder_id: None,
            owner_id,
            read_principals: vec![IndexPrincipal::Owner(owner_id)],
            visibility: IndexVisibility::Private,
            acl_hash: "hash-1".to_string(),
            acl_version: 1,
            embedding_policy: EmbeddingPolicy::Allowed,
        }
    }

    fn test_indexer() -> Arc<ContentIndexer<SimpleEmbeddingGenerator>> {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        Arc::new(ContentIndexer::new(
            generator,
            Arc::new(InMemoryVectorStore::new()),
        ))
    }

    // Mock PermissionResolverOps for testing
    struct MockPermissionOps;

    impl PermissionResolverOps for MockPermissionOps {
        async fn find_user_share(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<Uuid>,
            _recipient_user_id: UserId,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Option<crate::domain::Share>> {
            Ok(None)
        }

        async fn find_group_shares(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<Uuid>,
            _group_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<crate::domain::Share>> {
            Ok(Vec::new())
        }

        async fn find_user_shares_for_folders(
            &self,
            _folder_ids: &[Uuid],
            _recipient_user_id: UserId,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<crate::domain::Share>> {
            Ok(Vec::new())
        }

        async fn find_group_shares_for_folders(
            &self,
            _folder_ids: &[Uuid],
            _group_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<crate::domain::Share>> {
            Ok(Vec::new())
        }

        async fn find_file_by_id(
            &self,
            id: FileId,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Option<crate::domain::File>> {
            // Return file with user as owner for testing
            Ok(Some(crate::domain::File::new(
                "test.txt".to_string(),
                "/test.txt".to_string(),
                "hash".to_string(),
                100,
                "text/plain".to_string(),
                None,
                id, // owner is the same as the ID for testing
                Uuid::new_v4(),
            )))
        }

        async fn find_folder_by_id(
            &self,
            id: Uuid,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Option<crate::domain::Folder>> {
            Ok(Some(crate::domain::Folder::new_root(id, Uuid::new_v4())))
        }

        async fn get_user_group_ids(
            &self,
            _user_id: UserId,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<Uuid>> {
            Ok(Vec::new())
        }

        async fn find_all_user_shares_for_file(
            &self,
            _file_id: FileId,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<crate::domain::Share>> {
            Ok(Vec::new())
        }

        async fn find_all_group_shares_for_file(
            &self,
            _file_id: FileId,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<crate::domain::Share>> {
            Ok(Vec::new())
        }

        async fn find_all_user_shares_for_folders(
            &self,
            _folder_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<crate::domain::Share>> {
            Ok(Vec::new())
        }

        async fn find_all_group_shares_for_folders(
            &self,
            _folder_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<crate::domain::Share>> {
            Ok(Vec::new())
        }
    }

    fn create_test_service() -> AiService<SimpleEmbeddingGenerator, MockPermissionOps> {
        let permission_ops = Arc::new(MockPermissionOps);
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops));

        AiService::new(test_indexer(), permission_resolver)
    }

    #[tokio::test]
    async fn test_semantic_search_empty_query() {
        let service = create_test_service();
        let result = service
            .semantic_search("", Uuid::new_v4(), Uuid::new_v4(), 10)
            .await;

        assert!(matches!(result, Err(AiError::InvalidQuery(_))));
    }

    #[tokio::test]
    async fn test_semantic_search_long_query() {
        let service = create_test_service();
        let long_query = "a".repeat(1001);
        let result = service
            .semantic_search(&long_query, Uuid::new_v4(), Uuid::new_v4(), 10)
            .await;

        assert!(matches!(result, Err(AiError::InvalidQuery(_))));
    }

    #[tokio::test]
    async fn test_extract_key_topics() {
        let content = "Rust is a systems programming language. Rust provides memory safety without garbage collection.";
        let topics = extract_key_topics(content);

        assert!(!topics.is_empty());
        assert!(topics.contains(&"rust".to_string()) || topics.contains(&"language".to_string()));
    }

    #[tokio::test]
    async fn test_sanitize_snippet() {
        let snippet = "Hello\x00World\x01\nTest";
        let sanitized = sanitize_snippet(snippet);

        assert!(!sanitized.contains('\x00'));
        assert!(!sanitized.contains('\x01'));
        assert!(sanitized.contains('\n') || sanitized.contains(' '));
    }

    #[test]
    fn test_preview_truncates_unicode_on_character_boundary() {
        let content = "é".repeat(300);
        let preview = truncate_with_ellipsis(&content, 200);

        assert!(preview.ends_with("..."));
        assert!(preview.is_char_boundary(preview.len() - 3));
        assert_eq!(preview.trim_end_matches("...").chars().count(), 200);
    }

    // --- Configurable mock for permission-boundary tests ---

    use crate::domain::{File, Folder, Share, SharePermissions};
    use chrono::{Duration, Utc};
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct ConfigurableMockOps {
        files: Mutex<HashMap<Uuid, File>>,
        folders: Mutex<HashMap<Uuid, Folder>>,
        shares: Mutex<Vec<Share>>,
    }

    impl ConfigurableMockOps {
        fn new() -> Self {
            Self {
                files: Mutex::new(HashMap::new()),
                folders: Mutex::new(HashMap::new()),
                shares: Mutex::new(Vec::new()),
            }
        }

        fn add_file(&self, file: File) {
            self.files.lock().unwrap().insert(file.id, file);
        }

        fn add_share(&self, share: Share) {
            self.shares.lock().unwrap().push(share);
        }
    }

    impl PermissionResolverOps for ConfigurableMockOps {
        async fn find_user_share(
            &self,
            file_id: Option<Uuid>,
            folder_id: Option<Uuid>,
            recipient_user_id: UserId,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Option<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .find(|s| {
                    s.file_id == file_id
                        && s.folder_id == folder_id
                        && s.recipient_user_id == Some(recipient_user_id)
                })
                .cloned())
        }

        async fn find_group_shares(
            &self,
            _file_id: Option<Uuid>,
            _folder_id: Option<Uuid>,
            _group_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<Share>> {
            Ok(Vec::new())
        }

        async fn find_user_shares_for_folders(
            &self,
            _folder_ids: &[Uuid],
            _recipient_user_id: UserId,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<Share>> {
            Ok(Vec::new())
        }

        async fn find_group_shares_for_folders(
            &self,
            _folder_ids: &[Uuid],
            _group_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<Share>> {
            Ok(Vec::new())
        }

        async fn find_file_by_id(
            &self,
            id: Uuid,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Option<File>> {
            Ok(self.files.lock().unwrap().get(&id).cloned())
        }

        async fn find_folder_by_id(
            &self,
            id: Uuid,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Option<Folder>> {
            Ok(self.folders.lock().unwrap().get(&id).cloned())
        }

        async fn get_user_group_ids(
            &self,
            _user_id: UserId,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<Uuid>> {
            Ok(Vec::new())
        }

        async fn find_all_user_shares_for_file(
            &self,
            file_id: Uuid,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id == Some(file_id)
                        && s.folder_id.is_none()
                        && s.recipient_user_id.is_some()
                })
                .cloned()
                .collect())
        }

        async fn find_all_group_shares_for_file(
            &self,
            file_id: Uuid,
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id == Some(file_id)
                        && s.folder_id.is_none()
                        && s.recipient_group_id.is_some()
                })
                .cloned()
                .collect())
        }

        async fn find_all_user_shares_for_folders(
            &self,
            folder_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id.is_none()
                        && s.folder_id
                            .map(|fid| folder_ids.contains(&fid))
                            .unwrap_or(false)
                        && s.recipient_user_id.is_some()
                })
                .cloned()
                .collect())
        }

        async fn find_all_group_shares_for_folders(
            &self,
            folder_ids: &[Uuid],
            _tenant_id: Uuid,
        ) -> anyhow::Result<Vec<Share>> {
            let shares = self.shares.lock().unwrap();
            Ok(shares
                .iter()
                .filter(|s| {
                    s.file_id.is_none()
                        && s.folder_id
                            .map(|fid| folder_ids.contains(&fid))
                            .unwrap_or(false)
                        && s.recipient_group_id.is_some()
                })
                .cloned()
                .collect())
        }
    }

    fn create_configurable_service() -> AiService<SimpleEmbeddingGenerator, ConfigurableMockOps> {
        let indexer = test_indexer();
        let permission_ops = Arc::new(ConfigurableMockOps::new());
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops));
        AiService::new(indexer, permission_resolver)
    }

    fn make_file(id: Uuid, name: &str, owner_id: Uuid, tenant_id: Uuid) -> File {
        File {
            id,
            name: name.to_string(),
            path: format!("/{}", name),
            content_hash: "hash".to_string(),
            size: 100,
            mime_type: "text/plain".to_string(),
            parent_folder_id: None,
            owner_id,
            current_version: 1,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            starred_at: None,
            deleted_at: None,
            tenant_id,
        }
    }

    fn make_share(
        file_id: Uuid,
        recipient_user_id: Uuid,
        permissions: SharePermissions,
        revoked_at: Option<chrono::DateTime<Utc>>,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Share {
        Share {
            id: Uuid::new_v4(),
            file_id: Some(file_id),
            folder_id: None,
            share_token: Some(Uuid::new_v4().to_string()),
            permissions,
            password_hash: None,
            expires_at,
            upload_only: false,
            access_count: 0,
            recipient_user_id: Some(recipient_user_id),
            recipient_group_id: None,
            created_by: Uuid::new_v4(),
            created_at: Utc::now(),
            revoked_at,
            tenant_id: Uuid::new_v4(),
        }
    }

    #[tokio::test]
    async fn test_ai_excludes_deleted_content() {
        let service = create_configurable_service();
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        // Index a file but do NOT register it in the permission ops
        service
            .index_file(
                file_id,
                "deleted.txt".to_string(),
                "/deleted.txt".to_string(),
                "sensitive content".to_string(),
                "text/plain".to_string(),
                make_file_acl(tenant_id, file_id, user_id),
            )
            .await
            .unwrap();

        let results = service
            .semantic_search("sensitive", user_id, tenant_id, 10)
            .await
            .unwrap();

        assert!(
            results.is_empty(),
            "AI search should exclude deleted/unreachable content"
        );
    }

    #[tokio::test]
    async fn test_ai_excludes_revoked_content() {
        let owner_id = Uuid::new_v4();
        let recipient_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        let indexer = test_indexer();
        let ops = Arc::new(ConfigurableMockOps::new());
        ops.add_file(make_file(file_id, "revoked.txt", owner_id, tenant_id));
        ops.add_share(make_share(
            file_id,
            recipient_id,
            SharePermissions::View,
            Some(Utc::now()), // revoked
            None,
        ));
        let permission_resolver = Arc::new(PermissionResolver::new(ops));
        let service = AiService::new(indexer, permission_resolver);

        service
            .index_file(
                file_id,
                "revoked.txt".to_string(),
                "/revoked.txt".to_string(),
                "revoked content".to_string(),
                "text/plain".to_string(),
                make_file_acl(tenant_id, file_id, owner_id),
            )
            .await
            .unwrap();

        let results = service
            .semantic_search("revoked", recipient_id, tenant_id, 10)
            .await
            .unwrap();

        assert!(
            results.is_empty(),
            "AI search should exclude revoked shares"
        );
    }

    #[tokio::test]
    async fn test_ai_excludes_expired_content() {
        let indexer = test_indexer();
        let ops = Arc::new(ConfigurableMockOps::new());

        let owner_id = Uuid::new_v4();
        let recipient_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        ops.add_file(make_file(file_id, "expired.txt", owner_id, tenant_id));
        ops.add_share(make_share(
            file_id,
            recipient_id,
            SharePermissions::View,
            None,
            Some(Utc::now() - Duration::hours(1)),
        ));

        let permission_resolver = Arc::new(PermissionResolver::new(ops));
        let service = AiService::new(indexer, permission_resolver);

        service
            .index_file(
                file_id,
                "expired.txt".to_string(),
                "/expired.txt".to_string(),
                "expired content".to_string(),
                "text/plain".to_string(),
                make_file_acl(tenant_id, file_id, owner_id),
            )
            .await
            .unwrap();

        let results = service
            .semantic_search("expired", recipient_id, tenant_id, 10)
            .await
            .unwrap();

        assert!(
            results.is_empty(),
            "AI search should exclude expired shares"
        );
    }

    #[tokio::test]
    async fn test_ai_uses_normal_effective_permissions() {
        let indexer = test_indexer();
        let ops = Arc::new(ConfigurableMockOps::new());

        let owner_id = Uuid::new_v4();
        let recipient_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        ops.add_file(make_file(file_id, "shared.txt", owner_id, tenant_id));
        ops.add_share(make_share(
            file_id,
            recipient_id,
            SharePermissions::Edit,
            None,
            None,
        ));

        // Index with an ACL that grants the recipient direct read access.
        indexer
            .index_note(
                file_id,
                "shared.txt".to_string(),
                "/shared.txt".to_string(),
                "Rust is a systems programming language with memory safety".to_string(),
                "text/plain".to_string(),
                owner_id,
                IndexAclProjection {
                    tenant_id,
                    workspace_id: tenant_id,
                    object_id: file_id,
                    source_folder_id: None,
                    owner_id,
                    read_principals: vec![
                        IndexPrincipal::Owner(owner_id),
                        IndexPrincipal::User(recipient_id),
                    ],
                    visibility: IndexVisibility::Private,
                    acl_hash: "hash-1".to_string(),
                    acl_version: 1,
                    embedding_policy: EmbeddingPolicy::Allowed,
                },
            )
            .await
            .unwrap();

        let permission_resolver = Arc::new(PermissionResolver::new(ops));
        let service = AiService::new(indexer, permission_resolver);

        let results = service
            .semantic_search("programming language", recipient_id, tenant_id, 10)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_name, "shared.txt");
        assert!(
            results[0].can_edit,
            "AI should reflect effective Edit permission"
        );
    }

    #[tokio::test]
    async fn test_keyword_search_finds_owner_and_excludes_stranger() {
        let indexer = test_indexer();
        let ops = Arc::new(ConfigurableMockOps::new());

        let owner_id = Uuid::new_v4();
        let stranger_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        ops.add_file(make_file(
            file_id,
            "keyword_target.txt",
            owner_id,
            tenant_id,
        ));

        let permission_resolver = Arc::new(PermissionResolver::new(ops));
        let service = AiService::new(indexer, permission_resolver);

        service
            .index_file(
                file_id,
                "keyword_target.txt".to_string(),
                "/keyword_target.txt".to_string(),
                "documentation about keyword matching".to_string(),
                "text/plain".to_string(),
                make_file_acl(tenant_id, file_id, owner_id),
            )
            .await
            .unwrap();

        // The owner finds the document by keyword.
        let results = service
            .keyword_search("keyword", owner_id, tenant_id, 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_name, "keyword_target.txt");
        assert!(results[0].relevance_score > 0.0);

        // A stranger with no share cannot see it.
        let results = service
            .keyword_search("keyword", stranger_id, tenant_id, 10)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "keyword search must exclude a stranger's denied document"
        );
    }

    #[tokio::test]
    async fn test_ai_disabled_mode_returns_empty() {
        // When ai_service is None (disabled), handlers return empty / 503.
        // This is an AppState-level behavior verified by readiness tests.
        // At the service level, semantic_search simply requires an AiService instance.
        // We verify that an AiService with no indexed docs returns empty.
        let indexer = test_indexer();
        let ops = Arc::new(ConfigurableMockOps::new());
        let permission_resolver = Arc::new(PermissionResolver::new(ops));
        let service = AiService::new(indexer, permission_resolver);

        let results = service
            .semantic_search("anything", Uuid::new_v4(), Uuid::new_v4(), 10)
            .await
            .unwrap();

        assert!(
            results.is_empty(),
            "AI with no indexed data should return empty results"
        );
    }

    #[tokio::test]
    async fn test_ai_summarize_denies_deleted_file() {
        let indexer = test_indexer();
        let ops = Arc::new(ConfigurableMockOps::new());

        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        // Index but do not register file in permission ops
        let service = AiService::new(indexer, Arc::new(PermissionResolver::new(ops)));
        service
            .index_file(
                file_id,
                "ghost.txt".to_string(),
                "/ghost.txt".to_string(),
                "ghost content".to_string(),
                "text/plain".to_string(),
                make_file_acl(tenant_id, file_id, user_id),
            )
            .await
            .unwrap();

        let result = service.summarize_file(file_id, user_id, tenant_id).await;
        assert!(
            matches!(result, Err(AiError::PermissionDenied { .. })),
            "Summarizing a deleted file should be denied"
        );
    }
}
