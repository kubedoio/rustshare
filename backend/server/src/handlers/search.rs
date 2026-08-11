//! Unified search handler: `POST /api/v1/search`.
//!
//! Ranked, permission-aware search across Files/Notes and Buzz Chat. Every
//! returned result is reauthorized by its current owning source and snippets
//! come only from authorized content (see `services::unified_search`).

use axum::{extract::State, Json};
use rustshare_resource_auth::PrincipalContext;
use serde::{Deserialize, Serialize};

use crate::handlers::{AppError, AuthenticatedUser};
use crate::services::unified_search::{SearchProvenance, SearchSource, UnifiedSearchError};
use crate::AppState;

/// Unified search request.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SearchRequest {
    /// The search query
    pub query: String,
    /// Maximum number of results (default: 10, max: 50)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Restrict the search to specific sources: `"files"` and/or `"chat"`.
    /// Omitted or empty searches both.
    #[serde(default)]
    pub sources: Option<Vec<String>>,
}

fn default_limit() -> usize {
    10
}

/// One ranked, permission-aware search result.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SearchResultItem {
    pub source_application: String,
    pub source_type: String,
    /// Canonical `elembra://` resource reference (the cross-Application
    /// identity contract; feed it back into source authorization to open).
    pub resource_ref: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub score: f32,
    pub provenance: SearchProvenance,
}

/// Unified search response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SearchResponse {
    pub results: Vec<SearchResultItem>,
    pub total: usize,
}

/// POST /api/v1/search
///
/// Ranked, permission-aware search across Files/Notes and Buzz Chat. Results
/// are reauthorized by their current owning sources; snippets come only from
/// authorized content.
#[utoipa::path(
    post,
    path = "/api/v1/search",
    tag = "Search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results", body = SearchResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 429, description = "Rate limited", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn unified_search(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, AppError> {
    // Contract A-05: input validation.
    let query = request.query.trim();
    if query.is_empty() {
        return Err(AppError::bad_request("Query cannot be empty"));
    }
    if query.len() > 1000 {
        return Err(AppError::bad_request("Query too long (max 1000 chars)"));
    }

    // Parse the requested sources; omitted or empty means both.
    let sources = match request.sources.as_deref() {
        None | Some([]) => vec![SearchSource::Files, SearchSource::Chat],
        Some(names) => {
            let mut parsed = Vec::with_capacity(names.len());
            for name in names {
                match name.as_str() {
                    "files" => parsed.push(SearchSource::Files),
                    "chat" => parsed.push(SearchSource::Chat),
                    other => {
                        return Err(AppError::bad_request(format!("Unknown source '{other}'")));
                    }
                }
            }
            parsed
        }
    };

    // The 1:1 tenant/workspace mapping: WorkspaceId == TenantId today.
    let ctx = PrincipalContext::user(
        rustshare_core::domain::PrincipalId(auth.user_id),
        rustshare_core::domain::TenantId(auth.tenant_id),
        rustshare_core::domain::WorkspaceId(auth.tenant_id),
    );

    let response = state
        .unified_search_service
        .search(&ctx, query, &sources, request.limit)
        .await
        .map_err(|error| match error {
            UnifiedSearchError::InvalidQuery(message) => AppError::bad_request(message),
        })?;

    let results: Vec<SearchResultItem> = response
        .results
        .into_iter()
        .map(|r| SearchResultItem {
            source_application: r.source_application,
            source_type: r.source_type,
            resource_ref: r.resource_ref,
            title: r.title,
            snippet: r.snippet,
            location: r.location,
            occurred_at: r.occurred_at,
            updated_at: r.updated_at,
            score: r.score,
            provenance: r.provenance,
        })
        .collect();

    Ok(Json(SearchResponse {
        total: results.len(),
        results,
    }))
}
