//! HTTP handlers for meeting note operations.

use axum::{
    extract::{Path, State},
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

    Ok((StatusCode::CREATED, Json(meeting)))
}

pub async fn get_meeting(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(meeting_id): Path<Uuid>,
) -> Result<Json<MeetingNote>, AppError> {
    let meeting = state
        .meeting_service
        .get_meeting(meeting_id, auth.user_id)
        .await?;

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
) -> Result<Json<MeetingNote>, AppError> {
    let meeting = state
        .meeting_service
        .update_meeting(
            meeting_id,
            auth.user_id,
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
    state.broadcaster.publish(event);

    Ok(Json(meeting))
}

pub async fn list_meetings(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<MeetingSummary>>, AppError> {
    let meetings = state
        .meeting_service
        .list_meetings(auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(meetings))
}
