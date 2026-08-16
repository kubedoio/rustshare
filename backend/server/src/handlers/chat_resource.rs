//! Permission-aware Elembra Files attachments for signed Buzz messages.

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use rustshare_resource_auth::{
    ChatResourceAttachment, PrincipalContext, Purpose, Representation, ResourceRef, SourceError,
    BUZZ_RESOURCE_REF_TAG,
};
use serde::{Deserialize, Serialize};

use super::{AppError, AuthenticatedUser};
use crate::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ResourceRequest {
    pub resource: ResourceRef,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AttachmentResponse {
    pub attachment: ChatResourceAttachment,
    /// The exact tag a client adds to its normal signed Buzz event.
    pub buzz_tag: [String; 2],
}

fn principal(auth: &AuthenticatedUser) -> PrincipalContext {
    PrincipalContext::user(
        rustshare_core::domain::PrincipalId(auth.user_id),
        rustshare_core::domain::TenantId(auth.tenant_id),
        rustshare_core::domain::WorkspaceId(auth.tenant_id),
    )
}

fn attachment(
    resource: ResourceRef,
    resolved: rustshare_resource_auth::ResolvedResource,
) -> AttachmentResponse {
    AttachmentResponse {
        buzz_tag: [BUZZ_RESOURCE_REF_TAG.into(), resource.to_uri()],
        attachment: ChatResourceAttachment {
            resource,
            display_name: resolved.display_name,
            media_type: resolved.media_type,
            size: resolved.size,
            available: resolved.available,
        },
    }
}

fn source_error(error: SourceError) -> AppError {
    tracing::debug!(%error, "Chat resource access denied or unavailable");
    match error {
        SourceError::OwnerUnavailable => {
            AppError::service_unavailable("resource owner unavailable")
        }
        SourceError::Internal(_) => AppError::internal("resource owner failure"),
        // Existence-hiding: a ref, a tenant hint, or a stale event never
        // reveals whether an inaccessible Files resource exists.
        SourceError::Unauthorized
        | SourceError::NotFound
        | SourceError::VersionUnavailable
        | SourceError::InvalidRef(_)
        | SourceError::UnknownApplication(_)
        | SourceError::UnknownResourceType { .. }
        | SourceError::UnsupportedAction { .. }
        | SourceError::Delegation(_)
        | SourceError::WorkspaceMismatch
        | SourceError::UnsupportedRepresentation(_)
        | SourceError::BatchTooLarge { .. } => AppError::not_found("resource unavailable"),
    }
}

/// Validate a selected Files ref and return safe metadata plus the exact
/// credential-free Buzz tag. This endpoint never signs or stores a message.
#[utoipa::path(
    post,
    path = "/api/v1/applications/chat/attachments/prepare",
    tag = "Chat",
    request_body = ResourceRequest,
    responses(
        (status = 200, description = "Safe metadata plus the exact buzz tag", body = AttachmentResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Resource unavailable", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn prepare_attachment(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(input): Json<ResourceRequest>,
) -> Result<Json<AttachmentResponse>, AppError> {
    let resource = input.resource;
    let resolved = state
        .source_authorizer
        .resolve(&principal(&auth), &resource, Purpose::ChatUnfurl)
        .await
        .map_err(source_error)?;
    if !resolved.available {
        return Err(AppError::not_found("resource unavailable"));
    }
    Ok(Json(attachment(resource, resolved)))
}

/// Reauthorize the referenced Files resource and return only safe preview
/// metadata. Historical Buzz events are not consulted as authorization.
#[utoipa::path(
    post,
    path = "/api/v1/applications/chat/attachments/preview",
    tag = "Chat",
    request_body = ResourceRequest,
    responses(
        (status = 200, description = "Safe preview metadata", body = ChatResourceAttachment),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Resource unavailable", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn preview_attachment(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(input): Json<ResourceRequest>,
) -> Result<Json<ChatResourceAttachment>, AppError> {
    let resolved = state
        .source_authorizer
        .resolve(&principal(&auth), &input.resource, Purpose::ChatUnfurl)
        .await
        .map_err(source_error)?;
    if !resolved.available {
        return Err(AppError::not_found("resource unavailable"));
    }
    Ok(Json(ChatResourceAttachment {
        resource: input.resource,
        display_name: resolved.display_name,
        media_type: resolved.media_type,
        size: resolved.size,
        available: resolved.available,
    }))
}

/// Reauthorize at access time, then stream the authorized bytes through
/// Elembra Files. No Buzz event contains or redirects to storage access.
///
/// The bytes are served as a forced download (`Content-Disposition:
/// attachment`, `X-Content-Type-Options: nosniff`) — mirroring the regular
/// Files path — so a malicious or surprising attachment can never execute as
/// same-origin script in the recipient's browser.
#[utoipa::path(
    post,
    path = "/api/v1/applications/chat/attachments/open",
    tag = "Chat",
    request_body = ResourceRequest,
    responses(
        (status = 200, description = "Authorized file bytes (forced download)"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Resource unavailable", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn open_attachment(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(input): Json<ResourceRequest>,
) -> Result<Response, AppError> {
    let fetched = state
        .source_authorizer
        .fetch(&principal(&auth), &input.resource, Representation::Raw)
        .await
        .map_err(source_error)?;
    let mut response = (StatusCode::OK, fetched.data).into_response();
    if let Some(media_type) = fetched.media_type {
        if let Ok(value) = HeaderValue::from_str(&media_type) {
            response.headers_mut().insert(header::CONTENT_TYPE, value);
        }
    }
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}
