//! HTTP handlers for standup record operations.

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
use crate::services::standup_service::{StandupRecord, StandupSummary};
use crate::AppState;

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
) -> Result<(StatusCode, Json<StandupRecord>), AppError> {
    let standup = state
        .standup_service
        .create_standup(
            auth.user_id,
            auth.tenant_id,
            req.title,
            req.date,
            req.content,
        )
        .await?;

    Ok((StatusCode::CREATED, Json(standup)))
}

pub async fn get_standup(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(standup_id): Path<Uuid>,
) -> Result<Json<StandupRecord>, AppError> {
    let standup = state
        .standup_service
        .get_standup(standup_id, auth.user_id)
        .await?;

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
) -> Result<Json<StandupRecord>, AppError> {
    let standup = state
        .standup_service
        .update_standup(standup_id, auth.user_id, req.title, req.content)
        .await?;

    Ok(Json(standup))
}

pub async fn list_standups(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<StandupSummary>>, AppError> {
    let standups = state
        .standup_service
        .list_standups(auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(standups))
}
