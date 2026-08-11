//! HTTP handlers for AI endpoints.
//!
//! Provides endpoints for:
//! - Semantic search with permission filtering
//! - File summarization
//! - RAG-based Q&A with citations
//!
//! Contract A-04: Rate limiting enforced on all AI endpoints.
//! Contract A-05: Input validation and sanitization.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::handlers::{AppError, AuthenticatedUser};
use crate::AppState;

/// File summary request.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SummarizeRequest {
    /// The file ID to summarize
    pub file_id: Uuid,
}

/// File summary response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SummarizeResponse {
    pub file_id: String,
    pub file_name: String,
    pub summary: String,
    pub key_topics: Vec<String>,
    pub citation: SourceCitation,
}

/// Source citation for the legacy Files summarization endpoint.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SourceCitation {
    pub file_id: String,
    pub file_name: String,
    pub file_path: String,
    pub relevance_score: f32,
    pub excerpt: String,
}

/// POST /api/v1/ai/summarize
///
/// Generate a summary of a file if the user has access.
///
/// Contract A-01: Permission checked before summarizing.
/// Contract A-02: Source citation included.
/// Contract A-04: Rate limited.
#[utoipa::path(
    post,
    path = "/api/v1/ai/summarize",
    tag = "AI",
    request_body = SummarizeRequest,
    responses(
        (status = 200, description = "File summary", body = SummarizeResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "File not found", body = crate::handlers::ErrorResponse),
        (status = 429, description = "Rate limited", body = crate::handlers::ErrorResponse),
        (status = 503, description = "AI service not configured", body = crate::handlers::ErrorResponse),
    ),
)]
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
        return Err(AppError::service_unavailable("AI service not configured"));
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
