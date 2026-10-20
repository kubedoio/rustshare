//! HTTP handlers for meeting note operations.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use rustshare_core::domain::SharePermissions;
use rustshare_resource_auth::{PrincipalContext, Purpose, ResourceRef, SourceError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AuthenticatedUser;
use crate::handlers::AppError;
use crate::services::meeting_service::{MeetingNote, MeetingSummary};
use crate::AppState;
use rustshare_core::events::{AggregateType, Event, EventType, MeetingNoteModifiedPayload};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LinkMeetingResourceRequest {
    /// Canonical, credential-free ResourceRef URI returned by the owning Application.
    pub resource_ref: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeetingResourceLinkView {
    pub id: Uuid,
    pub available: bool,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_url: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeetingResourceLinkPage {
    pub items: Vec<MeetingResourceLinkView>,
    pub total: i64,
    pub page: u32,
    pub per_page: i64,
}

fn principal(auth: &AuthenticatedUser) -> PrincipalContext {
    PrincipalContext::user(
        rustshare_core::domain::PrincipalId(auth.user_id),
        rustshare_core::domain::TenantId(auth.tenant_id),
        rustshare_core::domain::WorkspaceId(auth.tenant_id),
    )
}

fn parse_meeting_resource_ref(uri: &str) -> Result<ResourceRef, AppError> {
    let mut resource = ResourceRef::from_uri(uri)
        .map_err(|_| AppError::bad_request("Invalid resource reference"))?;
    if resource.application.0 != "io.elembra.files"
        || resource.resource_type != "file"
        || resource.version.is_some()
    {
        return Err(AppError::bad_request(
            "Meeting links require a current Files file reference",
        ));
    }
    resource.resource_id = Uuid::parse_str(&resource.resource_id)
        .map_err(|_| AppError::bad_request("Invalid Files resource ID"))?
        .to_string();
    Ok(resource)
}

async fn meeting_resource_view(
    state: &AppState,
    auth: &AuthenticatedUser,
    id: Uuid,
    resource_ref_uri: &str,
    created_at: DateTime<Utc>,
) -> Result<MeetingResourceLinkView, AppError> {
    let unavailable = || MeetingResourceLinkView {
        id,
        available: false,
        created_at,
        resource_ref: None,
        display_name: None,
        media_type: None,
        resource_kind: None,
        open_url: None,
    };
    let Ok(resource) = ResourceRef::from_uri(resource_ref_uri) else {
        tracing::warn!(link_id = %id, "invalid persisted meeting resource reference");
        return Ok(unavailable());
    };
    let resolved = match state
        .source_authorizer
        .resolve(&principal(auth), &resource, Purpose::UserOpen)
        .await
    {
        Ok(resolved) if resolved.available => resolved,
        Ok(_) => return Ok(unavailable()),
        Err(SourceError::OwnerUnavailable) => {
            return Err(AppError::service_unavailable("Resource owner unavailable"));
        }
        Err(SourceError::Internal(_)) => return Err(AppError::internal("Resource access failed")),
        Err(_) => return Ok(unavailable()),
    };

    // The canonical Notes resources are file-backed. Keep their Files ResourceRef,
    // and derive the owning UI route only after the Files owner reauthorizes access.
    let file_id = match Uuid::parse_str(&resource.resource_id) {
        Ok(id) => id,
        Err(_) => return Ok(unavailable()),
    };
    let file = match state.file_service.get_file(file_id, auth.user_id).await {
        Ok(file)
            if file.tenant_id == auth.tenant_id
                && !super::files::is_hidden_kanban_file(&file.name) =>
        {
            file
        }
        Ok(_)
        | Err(rustshare_core::services::FileError::NotFound(_))
        | Err(rustshare_core::services::FileError::PermissionDenied { .. }) => {
            return Ok(unavailable());
        }
        Err(_) => return Err(AppError::internal("Resource access failed")),
    };
    let workspace_notes = format!(
        "/{}/{}/",
        state.note_service.workspace_name, state.note_service.folder_name
    );
    let legacy_notes = format!("/{}/", state.note_service.folder_name);
    let is_note = file.path.starts_with(&workspace_notes) || file.path.starts_with(&legacy_notes);
    let (display_name, open_url, resource_kind) = if is_note {
        // Keep listing read-only: NoteService::get_note reconciles external
        // edits and can mutate sidecar metadata or the bundle name.
        let title = file.name.trim_end_matches(".md").to_string();
        (title, format!("/apps/notes/{file_id}"), "note")
    } else {
        (
            resolved.display_name,
            format!("/files?preview={file_id}"),
            "file",
        )
    };

    Ok(MeetingResourceLinkView {
        id,
        available: true,
        created_at,
        resource_ref: Some(resource.to_uri()),
        display_name: Some(display_name),
        media_type: resolved.media_type,
        resource_kind: Some(resource_kind.into()),
        open_url: Some(open_url),
    })
}

async fn require_meeting_edit(
    state: &AppState,
    auth: &AuthenticatedUser,
    meeting_id: Uuid,
) -> Result<(), AppError> {
    state
        .meeting_service
        .get_meeting(meeting_id, auth.user_id, auth.tenant_id)
        .await?;
    let allowed = state
        .permission_resolver
        .check_folder_permission(
            auth.user_id,
            auth.tenant_id,
            meeting_id,
            SharePermissions::Edit,
        )
        .await
        .map_err(|_| AppError::internal("Meeting authorization failed"))?;
    if !allowed {
        return Err(AppError::Forbidden("Permission denied".into()));
    }
    Ok(())
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateMeetingRequest {
    pub title: String,
    pub team: String,
    pub date: DateTime<Utc>,
    pub content: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/meetings",
    tag = "Meetings",
    request_body = CreateMeetingRequest,
    responses(
        (status = 200, description = "Success", body = MeetingNote),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_meeting(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateMeetingRequest>,
) -> Result<(StatusCode, Json<MeetingNote>), AppError> {
    let meeting = state
        .meeting_service
        .create_meeting(
            auth.user_id,
            auth.tenant_id,
            req.title,
            req.team,
            req.date,
            req.content,
        )
        .await?;

    let payload = MeetingNoteModifiedPayload {
        meeting_id: meeting.id.to_string(),
        title: meeting.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::MeetingNoteModified,
        meeting.id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok((StatusCode::CREATED, Json(meeting)))
}

#[utoipa::path(
    get,
    path = "/api/v1/meetings/{id}",
    tag = "Meetings",
    params(("meeting_id" = Uuid, Path, description = "Meeting Id")),
    responses(
        (status = 200, description = "Success", body = MeetingNote),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_meeting(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(meeting_id): Path<Uuid>,
) -> Result<Json<MeetingNote>, AppError> {
    let meeting = state
        .meeting_service
        .get_meeting(meeting_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(meeting))
}

#[utoipa::path(
    get,
    path = "/api/v1/meetings/{id}/resources",
    tag = "Meetings",
    params(
        ("id" = Uuid, Path, description = "Meeting ID"),
        ("page" = Option<u32>, Query, description = "One-based page, defaults to 1"),
        ("per_page" = Option<u32>, Query, description = "Page size, defaults to 50 and is capped at 100")
    ),
    responses(
        (status = 200, description = "Current permission-aware linked resources", body = MeetingResourceLinkPage),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Meeting not found", body = crate::handlers::ErrorResponse)
    )
)]
pub async fn list_meeting_resources(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(meeting_id): Path<Uuid>,
    Query(query): Query<super::PaginationQuery>,
) -> Result<Json<MeetingResourceLinkPage>, AppError> {
    state
        .meeting_service
        .get_meeting(meeting_id, auth.user_id, auth.tenant_id)
        .await?;
    let limit = query.limit();
    let (links, total) = state
        .meeting_service
        .list_resource_links(meeting_id, auth.tenant_id, limit, query.offset())
        .await?;
    let mut items = Vec::with_capacity(links.len());
    for link in links {
        items.push(
            meeting_resource_view(
                &state,
                &auth,
                link.id,
                &link.resource_ref_uri,
                link.created_at,
            )
            .await?,
        );
    }
    Ok(Json(MeetingResourceLinkPage {
        items,
        total,
        page: query.page.max(1),
        per_page: limit,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/meetings/{id}/resources",
    tag = "Meetings",
    params(("id" = Uuid, Path, description = "Meeting ID")),
    request_body = LinkMeetingResourceRequest,
    responses(
        (status = 200, description = "Existing or newly linked resource", body = MeetingResourceLinkView),
        (status = 400, description = "Invalid resource reference", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Meeting mutation denied", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Resource unavailable", body = crate::handlers::ErrorResponse)
    )
)]
pub async fn link_meeting_resource(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(meeting_id): Path<Uuid>,
    axum::Json(request): axum::Json<LinkMeetingResourceRequest>,
) -> Result<Json<MeetingResourceLinkView>, AppError> {
    require_meeting_edit(&state, &auth, meeting_id).await?;
    let resource = parse_meeting_resource_ref(&request.resource_ref)?;
    let resolved = state
        .source_authorizer
        .resolve(&principal(&auth), &resource, Purpose::UserOpen)
        .await
        .map_err(|error| match error {
            SourceError::OwnerUnavailable => {
                AppError::service_unavailable("Resource owner unavailable")
            }
            SourceError::Internal(_) => AppError::internal("Resource access failed"),
            _ => AppError::not_found("Resource unavailable"),
        })?;
    if !resolved.available {
        return Err(AppError::not_found("Resource unavailable"));
    }
    let file_id = Uuid::parse_str(&resource.resource_id)
        .map_err(|_| AppError::not_found("Resource unavailable"))?;
    let file = state
        .file_service
        .get_file(file_id, auth.user_id)
        .await
        .map_err(|error| match error {
            rustshare_core::services::FileError::NotFound(_)
            | rustshare_core::services::FileError::PermissionDenied { .. } => {
                AppError::not_found("Resource unavailable")
            }
            error => {
                tracing::error!(%error, "Failed to validate meeting resource link");
                AppError::internal("Resource access failed")
            }
        })?;
    if file.tenant_id != auth.tenant_id || super::files::is_hidden_kanban_file(&file.name) {
        return Err(AppError::not_found("Resource unavailable"));
    }
    let link = state
        .meeting_service
        .link_resource(meeting_id, auth.tenant_id, resource.to_uri())
        .await?;
    Ok(Json(
        meeting_resource_view(
            &state,
            &auth,
            link.id,
            &link.resource_ref_uri,
            link.created_at,
        )
        .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/v1/meetings/{id}/resources/{link_id}",
    tag = "Meetings",
    params(
        ("id" = Uuid, Path, description = "Meeting ID"),
        ("link_id" = Uuid, Path, description = "Meeting resource link ID")
    ),
    responses(
        (status = 204, description = "Link removed; target resource is unchanged"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Meeting mutation denied", body = crate::handlers::ErrorResponse)
    )
)]
pub async fn unlink_meeting_resource(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((meeting_id, link_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    require_meeting_edit(&state, &auth, meeting_id).await?;
    state
        .meeting_service
        .unlink_resource(meeting_id, auth.tenant_id, link_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateMeetingRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub attendees: Option<Vec<String>>,
}

#[utoipa::path(
    put,
    path = "/api/v1/meetings/{id}",
    tag = "Meetings",
    params(("meeting_id" = Uuid, Path, description = "Meeting Id")),
    request_body = UpdateMeetingRequest,
    responses(
        (status = 200, description = "Success", body = MeetingNote),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_meeting(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(meeting_id): Path<Uuid>,
    Json(req): Json<UpdateMeetingRequest>,
) -> Result<Json<MeetingNote>, AppError> {
    let meeting = state
        .meeting_service
        .update_meeting(
            meeting_id,
            auth.user_id,
            auth.tenant_id,
            req.title,
            req.content,
            req.attendees,
        )
        .await?;

    let payload = MeetingNoteModifiedPayload {
        meeting_id: meeting_id.to_string(),
        title: meeting.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::MeetingNoteModified,
        meeting_id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(Json(meeting))
}

#[utoipa::path(
    delete,
    path = "/api/v1/meetings/{id}",
    tag = "Meetings",
    params(("meeting_id" = Uuid, Path, description = "Meeting Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn delete_meeting(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(meeting_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let meeting = state
        .meeting_service
        .get_meeting(meeting_id, auth.user_id, auth.tenant_id)
        .await?;
    state
        .meeting_service
        .delete_meeting(meeting_id, auth.user_id, auth.tenant_id)
        .await?;

    let payload = MeetingNoteModifiedPayload {
        meeting_id: meeting_id.to_string(),
        title: meeting.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::MeetingNoteModified,
        meeting_id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/meetings",
    tag = "Meetings",
    responses(
        (status = 200, description = "Success", body = Vec<MeetingSummary>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_meetings(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<crate::handlers::PaginationQuery>,
) -> Result<Json<Vec<MeetingSummary>>, AppError> {
    let meetings = state
        .meeting_service
        .list_meetings(auth.user_id, auth.tenant_id, query.limit(), query.offset())
        .await?;

    Ok(Json(meetings))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MoveMeetingRequest {
    pub target_folder_id: Option<Uuid>,
}

#[utoipa::path(
    post,
    path = "/api/v1/meetings/{id}/move",
    tag = "Meetings",
    params(("meeting_id" = Uuid, Path, description = "Meeting Id")),
    request_body = MoveMeetingRequest,
    responses(
        (status = 200, description = "Success", body = MeetingNote),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn move_meeting(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(meeting_id): Path<Uuid>,
    Json(req): Json<MoveMeetingRequest>,
) -> Result<Json<MeetingNote>, AppError> {
    let meeting = state
        .meeting_service
        .move_meeting(
            meeting_id,
            auth.user_id,
            auth.tenant_id,
            req.target_folder_id,
        )
        .await?;

    let payload = MeetingNoteModifiedPayload {
        meeting_id: meeting_id.to_string(),
        title: meeting.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::MeetingNoteModified,
        meeting_id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(Json(meeting))
}

#[utoipa::path(
    post,
    path = "/api/v1/meetings/{id}/duplicate",
    tag = "Meetings",
    params(("meeting_id" = Uuid, Path, description = "Meeting Id")),
    responses(
        (status = 201, description = "Created", body = MeetingNote),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn duplicate_meeting(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(meeting_id): Path<Uuid>,
) -> Result<(StatusCode, Json<MeetingNote>), AppError> {
    let meeting = state
        .meeting_service
        .duplicate_meeting(meeting_id, auth.user_id, auth.tenant_id)
        .await?;

    let payload = MeetingNoteModifiedPayload {
        meeting_id: meeting.id.to_string(),
        title: meeting.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::MeetingNoteModified,
        meeting.id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok((StatusCode::CREATED, Json(meeting)))
}
