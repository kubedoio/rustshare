//! HTTP handlers for search operations.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::services::SearchService;

use super::{AuthenticatedUser, ErrorResponse};
use crate::AppState;

/// Search query parameters
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Search query string
    pub q: String,
    /// Maximum number of results (default: 20, max: 100)
    pub limit: Option<usize>,
}

/// Search result response item
#[derive(Debug, Serialize)]
pub struct SearchResultResponse {
    /// Resource ID
    pub id: Uuid,
    /// Resource type: "file" or "folder"
    pub resource_type: String,
    /// Resource name
    pub name: String,
    /// Full path
    pub path: String,
    /// Owner ID
    pub owner_id: Uuid,
    /// Permission level the user has
    pub permission: String,
}

/// Search response
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    /// Search results
    pub results: Vec<SearchResultResponse>,
    /// Total number of results returned
    pub count: usize,
    /// The query that was searched
    pub query: String,
}

/// Search for files and folders
///
/// GET /api/v1/search?q=query&limit=20
///
/// Searches across file and folder names and paths.
/// Results are filtered to only include resources the user has view permission on.
pub async fn search(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(params): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, Response> {
    // Validate and clamp limit
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    
    // Validate query
    if params.q.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Search query cannot be empty")),
        )
            .into_response());
    }

    // Perform search
    let results = state
        .search_service
        .search(auth.user_id, auth.tenant_id, &params.q, limit)
        .await
        .map_err(|e| {
            tracing::error!("Search failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Search failed")),
            )
                .into_response()
        })?;

    // Convert to response format, filtering out hidden metadata files
    let response_results: Vec<SearchResultResponse> = results
        .into_iter()
        .filter(|r| {
            !r.name.starts_with(".rustshare")
                && r.name != "events.jsonl"
                && r.name != "index.md"
                && r.name != "__primary__.md"
                && !r.name.ends_with(".editor.json")
        })
        .map(|r| SearchResultResponse {
            id: r.id,
            resource_type: r.resource_type,
            name: r.name,
            path: r.path,
            owner_id: r.owner_id,
            permission: r
                .permission
                .map(|p| format!("{:?}", p).to_lowercase())
                .unwrap_or_else(|| "view".to_string()),
        })
        .collect();

    let count = response_results.len();

    Ok(Json(SearchResponse {
        results: response_results,
        count,
        query: params.q,
    }))
}
