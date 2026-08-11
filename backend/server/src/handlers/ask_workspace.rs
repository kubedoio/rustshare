//! `POST /api/v1/memory/ask`: source-grounded workspace Q&A.

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_resource_auth::{PrincipalContext, Purpose, ResourceRef, SourceError};

use crate::handlers::{AppError, AuthenticatedUser};
use crate::services::ask_workspace::{AskWorkspaceResponse, LlmError};
use crate::services::unified_search::SearchSource;
use crate::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AskWorkspaceRequest {
    pub question: String,
    /// Must equal the authenticated tenant/workspace in the current 1:1 model.
    pub workspace_id: Uuid,
    #[serde(default)]
    pub sources: Option<Vec<String>>,
    #[serde(default = "default_limit")]
    pub result_limit: usize,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OpenCitationRequest {
    pub resource_ref: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenCitationResponse {
    pub resource_ref: String,
    pub display_name: String,
    pub media_type: Option<String>,
    pub size: Option<i64>,
    pub updated_at: Option<DateTime<Utc>>,
    pub available: bool,
}

fn default_limit() -> usize {
    8
}

#[utoipa::path(
    post,
    path = "/api/v1/memory/ask",
    tag = "Memory",
    request_body = AskWorkspaceRequest,
    responses(
        (status = 200, description = "Grounded workspace answer", body = AskWorkspaceResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 503, description = "LLM provider unavailable", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn ask_workspace(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(request): Json<AskWorkspaceRequest>,
) -> Result<(StatusCode, Json<AskWorkspaceResponse>), AppError> {
    if request.workspace_id != auth.tenant_id {
        return Err(AppError::bad_request(
            "workspace is outside the authenticated scope",
        ));
    }
    let sources = match request.sources.as_deref() {
        None | Some([]) => Vec::new(),
        Some(names) => names
            .iter()
            .map(|name| match name.as_str() {
                "files" => Ok(SearchSource::Files),
                "chat" => Ok(SearchSource::Chat),
                other => Err(AppError::bad_request(format!("Unknown source '{other}'"))),
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    let ctx = PrincipalContext::user(
        rustshare_core::domain::PrincipalId(auth.user_id),
        rustshare_core::domain::TenantId(auth.tenant_id),
        rustshare_core::domain::WorkspaceId(request.workspace_id),
    );
    let response = state
        .ask_workspace_service
        .ask(&ctx, &request.question, &sources, request.result_limit)
        .await
        .map_err(|error| match error {
            LlmError::InvalidInput(message) => AppError::bad_request(message),
            LlmError::Unavailable => AppError::service_unavailable("LLM provider not configured"),
            LlmError::Failed => AppError::service_unavailable("LLM provider unavailable"),
        })?;
    Ok((StatusCode::OK, Json(response)))
}

/// Reauthorize a citation before opening its owning source representation.
#[utoipa::path(
    post,
    path = "/api/v1/memory/citations/open",
    tag = "Memory",
    request_body = OpenCitationRequest,
    responses(
        (status = 200, description = "Authorized source metadata", body = OpenCitationResponse),
        (status = 400, description = "Invalid citation", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Unavailable citation", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn open_citation(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(request): Json<OpenCitationRequest>,
) -> Result<Json<OpenCitationResponse>, AppError> {
    let resource = ResourceRef::from_uri(&request.resource_ref)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    let ctx = PrincipalContext::user(
        rustshare_core::domain::PrincipalId(auth.user_id),
        rustshare_core::domain::TenantId(auth.tenant_id),
        rustshare_core::domain::WorkspaceId(auth.tenant_id),
    );
    state
        .source_authorizer
        .resolve(&ctx, &resource, Purpose::UserOpen)
        .await
        .map(|resolved| {
            Json(OpenCitationResponse {
                resource_ref: resolved.resource.to_uri(),
                display_name: resolved.display_name,
                media_type: resolved.media_type,
                size: resolved.size,
                updated_at: resolved.updated_at,
                available: resolved.available,
            })
        })
        .map_err(|error| match error {
            SourceError::InvalidRef(message) => AppError::bad_request(message),
            _ => AppError::not_found("citation unavailable"),
        })
}
