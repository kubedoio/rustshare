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

use std::sync::Arc;
use uuid::Uuid;

use crate::domain::{FileId, SharePermissions, UserId};
use crate::services::permission_resolver::{PermissionResolver, PermissionResolverOps, Resource};

use super::ai::indexing::{ContentIndexer, IndexedDocument};
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

/// An answer to a user's question with citations.
#[derive(Debug, Clone)]
pub struct QuestionAnswer {
    /// The answer text
    pub answer: String,
    /// Source citations
    pub citations: Vec<SourceCitation>,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f32,
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

        // Perform semantic search
        let raw_results = self.indexer.search(tenant_id, query, limit * 3).await;

        // Filter by permission and build results
        let mut results = Vec::new();
        for (document, score) in raw_results {
            // Check permission - Contract A-01
            let resource = Resource::File(document.file_id);
            let permission = self
                .permission_resolver
                .resolve_permission(user_id, resource)
                .await
                .map_err(|e| AiError::Internal(e.to_string()))?;

            if let Some(perm) = permission {
                // Generate snippet (first 200 chars of content)
                let snippet = if document.content.len() > 200 {
                    format!("{}...", &document.content[..200])
                } else {
                    document.content.clone()
                };

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

        Ok(results)
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
        let permission = self
            .permission_resolver
            .resolve_permission(user_id, resource)
            .await
            .map_err(|e| AiError::Internal(e.to_string()))?;

        if permission.is_none() {
            return Err(AiError::PermissionDenied { user_id });
        }

        // Get the indexed document
        let document = self
            .indexer
            .get_document(file_id, tenant_id)
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
            excerpt: if document.content.len() > 300 {
                format!("{}...", &document.content[..300])
            } else {
                document.content.clone()
            },
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
    /// An answer with citations to source documents.
    ///
    /// # Contract A-01, A-02, A-03: Permission + Citation + No Hallucinations
    /// - Only uses documents the user can access
    /// - Always cites sources
    /// - Only answers based on retrieved content
    pub async fn ask_question(
        &self,
        query: &str,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<QuestionAnswer, AiError> {
        // Validate query
        let query = query.trim();
        if query.is_empty() {
            return Err(AiError::InvalidQuery(
                "Question cannot be empty".to_string(),
            ));
        }
        if query.len() > 2000 {
            return Err(AiError::InvalidQuery(
                "Question too long (max 2000 chars)".to_string(),
            ));
        }

        // Retrieve relevant documents
        let search_results = self.semantic_search(query, user_id, tenant_id, 5).await?;

        if search_results.is_empty() {
            return Ok(QuestionAnswer {
                answer: "I couldn't find any relevant documents to answer your question."
                    .to_string(),
                citations: Vec::new(),
                confidence: 0.0,
            });
        }

        // Build citations from search results
        let citations: Vec<SourceCitation> = search_results
            .iter()
            .map(|result| SourceCitation {
                file_id: result.file_id.to_string(),
                file_name: result.file_name.clone(),
                file_path: result.file_path.clone(),
                relevance_score: result.relevance_score,
                excerpt: result.snippet.clone(),
            })
            .collect();

        // Generate answer based on retrieved content
        // Contract A-03: Only use retrieved content, no hallucinations
        let answer = generate_rag_answer(query, &search_results);

        // Calculate confidence based on relevance scores
        let confidence = if !search_results.is_empty() {
            let avg_score: f32 = search_results
                .iter()
                .map(|r| r.relevance_score)
                .sum::<f32>()
                / search_results.len() as f32;
            avg_score.clamp(0.0, 1.0)
        } else {
            0.0
        };

        Ok(QuestionAnswer {
            answer,
            citations,
            confidence,
        })
    }

    /// Index a file for AI search.
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
    pub async fn index_file(
        &self,
        file_id: FileId,
        file_name: String,
        file_path: String,
        content: String,
        mime_type: String,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), AiError> {
        self.indexer
            .index_file(
                file_id, file_name, file_path, content, mime_type, owner_id, tenant_id,
            )
            .await
            .map_err(|e| AiError::Internal(e.to_string()))
    }

    /// Remove a file from the AI index.
    ///
    /// # Arguments
    /// * `file_id` - The file ID
    /// * `tenant_id` - The tenant ID
    pub async fn remove_file_from_index(&self, file_id: FileId, tenant_id: Uuid) {
        self.indexer.remove_file(file_id, tenant_id).await;
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

/// Generate a simple document summary.
/// Phase 1.5: Basic summary generation.
/// Future phases: Use LLM for more sophisticated summaries.
fn generate_document_summary(document: &IndexedDocument) -> String {
    let word_count = document.content.split_whitespace().count();

    let content_preview = if document.content.len() > 500 {
        format!("{}...", &document.content[..500].trim())
    } else {
        document.content.clone()
    };

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
    topics.sort_by(|a, b| b.1.cmp(&a.1));
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

/// Generate a RAG-based answer from search results.
/// Phase 1.5: Simple answer generation based on retrieved content.
/// Contract A-03: Only use retrieved content, no hallucinations.
fn generate_rag_answer(query: &str, results: &[SemanticSearchResult]) -> String {
    if results.is_empty() {
        return "I couldn't find any relevant information to answer your question.".to_string();
    }

    // Build answer from top results
    let mut answer_parts = Vec::new();

    // Add intro
    answer_parts.push(format!(
        "Based on the documents I found, here's what I can tell you about \"{}\":",
        query
    ));

    // Add information from each result
    for (i, result) in results.iter().take(3).enumerate() {
        answer_parts.push(format!(
            "\n{}. From \"{}\" (relevance: {:.0}%):\n   {}",
            i + 1,
            result.file_name,
            result.relevance_score * 100.0,
            result.snippet
        ));
    }

    // Add closing
    answer_parts.push(format!(
        "\nI found {} relevant document(s). See the citations for more details.",
        results.len()
    ));

    answer_parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ai::embedding::SimpleEmbeddingGenerator;

    // Mock PermissionResolverOps for testing
    struct MockPermissionOps;

    impl PermissionResolverOps for MockPermissionOps {
        async fn find_user_share(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<Uuid>,
            _recipient_user_id: UserId,
        ) -> anyhow::Result<Option<crate::domain::Share>> {
            Ok(None)
        }

        async fn find_group_shares(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<Uuid>,
            _group_ids: &[Uuid],
        ) -> anyhow::Result<Vec<crate::domain::Share>> {
            Ok(Vec::new())
        }

        async fn find_user_shares_for_folders(
            &self,
            _folder_ids: &[Uuid],
            _recipient_user_id: UserId,
        ) -> anyhow::Result<Vec<crate::domain::Share>> {
            Ok(Vec::new())
        }

        async fn find_group_shares_for_folders(
            &self,
            _folder_ids: &[Uuid],
            _group_ids: &[Uuid],
        ) -> anyhow::Result<Vec<crate::domain::Share>> {
            Ok(Vec::new())
        }

        async fn find_file_by_id(&self, id: FileId) -> anyhow::Result<Option<crate::domain::File>> {
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
        ) -> anyhow::Result<Option<crate::domain::Folder>> {
            Ok(Some(crate::domain::Folder::new_root(id, Uuid::new_v4())))
        }

        async fn get_user_group_ids(&self, _user_id: UserId) -> anyhow::Result<Vec<Uuid>> {
            Ok(Vec::new())
        }
    }

    fn create_test_service() -> AiService<SimpleEmbeddingGenerator, MockPermissionOps> {
        let generator = Arc::new(SimpleEmbeddingGenerator::new());
        let indexer = Arc::new(ContentIndexer::new(generator));
        let permission_ops = Arc::new(MockPermissionOps);
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops));

        AiService::new(indexer, permission_resolver)
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
    async fn test_ask_question_valid() {
        let service = create_test_service();
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Index a document first
        service
            .index_file(
                Uuid::new_v4(),
                "test.txt".to_string(),
                "/test.txt".to_string(),
                "Rust is a programming language with memory safety guarantees".to_string(),
                "text/plain".to_string(),
                user_id,
                tenant_id,
            )
            .await
            .unwrap();

        let answer = service
            .ask_question("What is Rust?", user_id, tenant_id)
            .await;

        assert!(answer.is_ok());
        let answer = answer.unwrap();
        assert!(!answer.answer.is_empty());
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
}
