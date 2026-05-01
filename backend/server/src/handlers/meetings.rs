//! HTTP handlers for meeting note operations.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;
use chrono::{DateTime, Utc};

use super::AuthenticatedUser;
use crate::services::meeting_service::{MeetingError, MeetingSummary, MeetingNote};
use crate::{handlers::ErrorResponse, AppState};

pub fn meeting_error_response(err: MeetingError) -> Response {
    let (status, message) = match err {
        MeetingError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        MeetingError::PermissionDenied => (StatusCode::FORBIDDEN, err.to_string()),
        MeetingError::InvalidData(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        MeetingError::Database(_) | MeetingError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        ),
    };
    (status, Json(ErrorResponse::new(message))).into_response()
}

#[derive(Debug, Deserialize)]
pub struct CreateMeetingRequest {
    pub title: String,
    pub team: String,
    pub date: DateTime<Utc>,
    pub content: String,
}

pub async fn create_meeting(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateMeetingRequest>,
) -> Result<(StatusCode, Json<MeetingNote>), Response> {
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
        .await
        .map_err(meeting_error_response)?;

    Ok((StatusCode::CREATED, Json(meeting)))
}

pub async fn get_meeting(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(meeting_id): Path<Uuid>,
) -> Result<Json<MeetingNote>, Response> {
    let meeting = state
        .meeting_service
        .get_meeting(meeting_id, auth.user_id)
        .await
        .map_err(meeting_error_response)?;

    Ok(Json(meeting))
}

#[derive(Debug, Deserialize)]
pub struct UpdateMeetingRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub attendees: Option<Vec<String>>,
}

pub async fn update_meeting(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(meeting_id): Path<Uuid>,
    Json(req): Json<UpdateMeetingRequest>,
) -> Result<Json<MeetingNote>, Response> {
    let meeting = state
        .meeting_service
        .update_meeting(
            meeting_id,
            auth.user_id,
            req.title,
            req.content,
            req.attendees,
        )
        .await
        .map_err(meeting_error_response)?;

    Ok(Json(meeting))
}

pub async fn list_meetings(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<MeetingSummary>>, Response> {
    let meetings = state
        .meeting_service
        .list_meetings(auth.user_id, auth.tenant_id)
        .await
        .map_err(meeting_error_response)?;

    Ok(Json(meetings))
}
