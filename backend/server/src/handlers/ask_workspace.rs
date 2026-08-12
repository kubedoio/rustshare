//! `POST /api/v1/memory/ask`: source-grounded workspace Q&A.

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_resource_auth::{PrincipalContext, Purpose, ResourceRef, SourceError};

use crate::handlers::{AppError, AuthenticatedUser};
use crate::services::ask_workspace::{AskWorkspaceResponse, LlmError};
use crate::services::unified_search::{parse_source_filter, SearchScope, SearchSource};
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
    #[serde(default)]
    pub scope: Option<AskScopeRequest>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AskScopeRequest {
    Workspace,
    Folder {
        #[serde(rename = "resourceRef")]
        resource_ref: String,
    },
    Note {
        #[serde(rename = "resourceRef")]
        resource_ref: String,
    },
    ChatChannel {
        #[serde(rename = "communityId")]
        community_id: String,
        #[serde(rename = "channelId")]
        channel_id: String,
    },
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
    let sources = parse_source_filter(request.sources.as_deref()).map_err(AppError::bad_request)?;
    let ctx = PrincipalContext::user(
        rustshare_core::domain::PrincipalId(auth.user_id),
        rustshare_core::domain::TenantId(auth.tenant_id),
        rustshare_core::domain::WorkspaceId(request.workspace_id),
    );
    let scope = match request.scope {
        None | Some(AskScopeRequest::Workspace) => SearchScope::Workspace,
        Some(AskScopeRequest::Folder { resource_ref }) => {
            if !sources.is_empty() && !sources.contains(&SearchSource::Files) {
                return Err(AppError::bad_request(
                    "folder scope requires the Files source",
                ));
            }
            let resource = ResourceRef::from_uri(&resource_ref)
                .map_err(|error| AppError::bad_request(error.to_string()))?;
            if resource.application.to_string() != "io.elembra.files"
                || resource.resource_type != "folder"
            {
                return Err(AppError::bad_request("scope is not a Files folder"));
            }
            SearchScope::Folder(resource)
        }
        Some(AskScopeRequest::Note { resource_ref }) => {
            if !sources.is_empty() && !sources.contains(&SearchSource::Files) {
                return Err(AppError::bad_request(
                    "note scope requires the Files source",
                ));
            }
            let resource = ResourceRef::from_uri(&resource_ref)
                .map_err(|error| AppError::bad_request(error.to_string()))?;
            if resource.application.to_string() != "io.elembra.files"
                || resource.resource_type != "file"
            {
                return Err(AppError::bad_request("scope is not a Files note"));
            }
            SearchScope::Resource(resource)
        }
        Some(AskScopeRequest::ChatChannel {
            community_id,
            channel_id,
        }) => {
            if !sources.is_empty() && !sources.contains(&SearchSource::Chat) {
                return Err(AppError::bad_request(
                    "channel scope requires the Chat source",
                ));
            }
            if community_id.trim().is_empty()
                || channel_id.trim().is_empty()
                || community_id.chars().count() > 256
                || channel_id.chars().count() > 256
            {
                return Err(AppError::bad_request("invalid chat channel scope"));
            }
            SearchScope::ChatChannel {
                community_id,
                channel_id,
            }
        }
    };
    let response = state
        .ask_workspace_service
        .ask_scoped(
            &ctx,
            &request.question,
            &sources,
            request.result_limit,
            &scope,
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_request_shapes_are_explicit_and_typed() {
        let request: AskWorkspaceRequest = serde_json::from_value(serde_json::json!({
            "question": "where?",
            "workspace_id": Uuid::nil(),
            "scope": {"type": "chatChannel", "communityId": "c", "channelId": "ch"}
        }))
        .expect("valid scoped request");
        assert!(matches!(
            request.scope,
            Some(AskScopeRequest::ChatChannel { .. })
        ));
    }
}
