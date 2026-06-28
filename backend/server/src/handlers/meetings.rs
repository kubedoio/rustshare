//! HTTP handlers for meeting note operations.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use super::AuthenticatedUser;
use crate::handlers::AppError;
use crate::services::meeting_service::{MeetingNote, MeetingSummary};
use crate::AppState;
use rustshare_core::events::{AggregateType, Event, EventType, MeetingNoteModifiedPayload};

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
