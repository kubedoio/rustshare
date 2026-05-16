//! HTTP handlers for AI endpoints.
//!
//! Provides endpoints for:
//! - Semantic search with permission filtering
//! - File summarization
//! - RAG-based Q&A with citations
//!
//! Contract A-04: Rate limiting enforced on all AI endpoints.
//! Contract A-05: Input validation and sanitization.

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::handlers::{AppError, AuthenticatedUser};
use crate::AppState;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Semantic search request.
#[derive(Debug, Deserialize)]
pub struct SemanticSearchRequest {
    /// The search query
    pub query: String,
    /// Maximum number of results (default: 10, max: 50)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

/// Semantic search result item.
#[derive(Debug, Serialize)]
pub struct SemanticSearchResultItem {
    pub file_id: String,
    pub file_name: String,
    pub file_path: String,
    pub relevance_score: f32,
    pub snippet: String,
    pub mime_type: String,
    pub owner_id: String,
    pub can_edit: bool,
}

/// Semantic search response.
#[derive(Debug, Serialize)]
pub struct SemanticSearchResponse {
    pub results: Vec<SemanticSearchResultItem>,
    pub total_found: usize,
}

/// File summary request.
#[derive(Debug, Deserialize)]
pub struct SummarizeRequest {
    /// The file ID to summarize
    pub file_id: Uuid,
}

/// File summary response.
#[derive(Debug, Serialize)]
pub struct SummarizeResponse {
    pub file_id: String,
    pub file_name: String,
    pub summary: String,
    pub key_topics: Vec<String>,
    pub citation: SourceCitation,
}

/// Source citation.
#[derive(Debug, Serialize)]
pub struct SourceCitation {
    pub file_id: String,
    pub file_name: String,
    pub file_path: String,
    pub relevance_score: f32,
    pub excerpt: String,
}

/// Question answering request.
#[derive(Debug, Deserialize)]
pub struct AskQuestionRequest {
    /// The question to answer
    pub question: String,
}

/// Question answering response.
#[derive(Debug, Serialize)]
pub struct AskQuestionResponse {
    pub answer: String,
    pub citations: Vec<SourceCitation>,
    pub confidence: f32,
}

// ============================================================================
// Handlers
// ============================================================================

/// POST /api/v1/ai/search
///
/// Perform permission-filtered semantic search.
///
/// Contract A-01: Results are filtered by user permissions.
/// Contract A-04: Rate limited.
pub async fn semantic_search(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(request): Json<SemanticSearchRequest>,
) -> Result<(StatusCode, Json<SemanticSearchResponse>), AppError> {
    // Contract A-04: Rate limiting enforced by middleware

    // Contract A-05: Input validation
    let query = request.query.trim();
    if query.is_empty() {
        return Err(AppError::bad_request("Query cannot be empty"));
    }

    let limit = request.limit.clamp(1, 50);

    // Get AI service from state or return not implemented
    // Note: AI service needs to be added to AppState
    let results = if let Some(ref ai_service) = state.ai_service {
        ai_service
            .semantic_search(query, auth.user_id, auth.tenant_id, limit)
            .await?
    } else {
        // AI service not configured - return empty results for now
        Vec::new()
    };

    let total_found = results.len();
    let response_results: Vec<SemanticSearchResultItem> = results
        .into_iter()
        .map(|r| SemanticSearchResultItem {
            file_id: r.file_id.to_string(),
            file_name: r.file_name,
            file_path: r.file_path,
            relevance_score: r.relevance_score,
            snippet: r.snippet,
            mime_type: r.mime_type,
            owner_id: r.owner_id.to_string(),
            can_edit: r.can_edit,
        })
        .collect();

    Ok((
        StatusCode::OK,
        Json(SemanticSearchResponse {
            results: response_results,
            total_found,
        }),
    ))
}

/// POST /api/v1/ai/summarize
///
/// Generate a summary of a file if the user has access.
///
/// Contract A-01: Permission checked before summarizing.
/// Contract A-02: Source citation included.
/// Contract A-04: Rate limited.
pub async fn summarize_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(request): Json<SummarizeRequest>,
) -> Result<(StatusCode, Json<SummarizeResponse>), AppError> {
    // Contract A-04: Rate limiting enforced by middleware

    // Get AI service from state or return not implemented
    let summary = if let Some(ref ai_service) = state.ai_service {
        ai_service
            .summarize_file(request.file_id, auth.user_id, auth.tenant_id)
            .await?
    } else {
        return Err(AppError::internal("AI service not configured"));
    };

    let response = SummarizeResponse {
        file_id: summary.file_id.to_string(),
        file_name: summary.file_name,
        summary: summary.summary,
        key_topics: summary.key_topics,
        citation: SourceCitation {
            file_id: summary.citation.file_id,
            file_name: summary.citation.file_name,
            file_path: summary.citation.file_path,
            relevance_score: summary.citation.relevance_score,
            excerpt: summary.citation.excerpt,
        },
    };

    Ok((StatusCode::OK, Json(response)))
}

/// POST /api/v1/ai/ask
///
/// Answer a question using RAG with citations.
///
/// Contract A-01: Only uses accessible documents.
/// Contract A-02: Citations included.
/// Contract A-03: No hallucinations.
/// Contract A-04: Rate limited.
pub async fn ask_question(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(request): Json<AskQuestionRequest>,
) -> Result<(StatusCode, Json<AskQuestionResponse>), AppError> {
    // Contract A-04: Rate limiting enforced by middleware

    // Contract A-05: Input validation
    let question = request.question.trim();
    if question.is_empty() {
        return Err(AppError::bad_request("Question cannot be empty"));
    }

    // Get AI service from state or return not implemented
    let answer = if let Some(ref ai_service) = state.ai_service {
        ai_service
            .ask_question(question, auth.user_id, auth.tenant_id)
            .await?
    } else {
        return Err(AppError::internal("AI service not configured"));
    };

    let citations: Vec<SourceCitation> = answer
        .citations
        .into_iter()
        .map(|c| SourceCitation {
            file_id: c.file_id,
            file_name: c.file_name,
            file_path: c.file_path,
            relevance_score: c.relevance_score,
            excerpt: c.excerpt,
        })
        .collect();

    let response = AskQuestionResponse {
        answer: answer.answer,
        citations,
        confidence: answer.confidence,
    };

    Ok((StatusCode::OK, Json(response)))
}
