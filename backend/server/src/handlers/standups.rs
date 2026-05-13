//! HTTP handlers for standup record operations.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use super::AuthenticatedUser;
use crate::services::standup_service::{StandupError, StandupRecord, StandupSummary};
use crate::{handlers::ErrorResponse, AppState};

pub fn standup_error_response(err: StandupError) -> Response {
    let (status, message) = match err {
        StandupError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        StandupError::PermissionDenied => (StatusCode::FORBIDDEN, err.to_string()),
        StandupError::InvalidData(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        StandupError::Database(_) | StandupError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        ),
    };
    (status, Json(ErrorResponse::new(message))).into_response()
}

#[derive(Debug, Deserialize)]
pub struct CreateStandupRequest {
    pub title: String,
    pub date: DateTime<Utc>,
    pub content: String,
}

pub async fn create_standup(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateStandupRequest>,
) -> Result<(StatusCode, Json<StandupRecord>), Response> {
    let standup = state
        .standup_service
        .create_standup(
            auth.user_id,
            auth.tenant_id,
            req.title,
            req.date,
            req.content,
        )
        .await
        .map_err(standup_error_response)?;

    Ok((StatusCode::CREATED, Json(standup)))
}

pub async fn get_standup(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(standup_id): Path<Uuid>,
) -> Result<Json<StandupRecord>, Response> {
    let standup = state
        .standup_service
        .get_standup(standup_id, auth.user_id)
        .await
        .map_err(standup_error_response)?;

    Ok(Json(standup))
}

#[derive(Debug, Deserialize)]
pub struct UpdateStandupRequest {
    pub title: Option<String>,
    pub content: Option<String>,
}

pub async fn update_standup(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(standup_id): Path<Uuid>,
    Json(req): Json<UpdateStandupRequest>,
) -> Result<Json<StandupRecord>, Response> {
    let standup = state
        .standup_service
        .update_standup(standup_id, auth.user_id, req.title, req.content)
        .await
        .map_err(standup_error_response)?;

    Ok(Json(standup))
}

pub async fn list_standups(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<StandupSummary>>, Response> {
    let standups = state
        .standup_service
        .list_standups(auth.user_id, auth.tenant_id)
        .await
        .map_err(standup_error_response)?;

    Ok(Json(standups))
}
