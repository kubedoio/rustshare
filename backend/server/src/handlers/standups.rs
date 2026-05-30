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
use rustshare_core::events::{AggregateType, Event, EventType, StandupModifiedPayload};

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

    let payload = StandupModifiedPayload {
        standup_id: standup.id.to_string(),
        title: standup.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::StandupModified,
        standup.id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok((StatusCode::CREATED, Json(standup)))
}

pub async fn get_standup(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(standup_id): Path<Uuid>,
) -> Result<Json<StandupRecord>, AppError> {
    let standup = state
        .standup_service
        .get_standup(standup_id, auth.user_id, auth.tenant_id)
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
        .update_standup(standup_id, auth.user_id, auth.tenant_id, req.title, req.content)
        .await?;

    let payload = StandupModifiedPayload {
        standup_id: standup_id.to_string(),
        title: standup.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::StandupModified,
        standup_id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(Json(standup))
}

pub async fn delete_standup(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(standup_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let standup = state
        .standup_service
        .get_standup(standup_id, auth.user_id, auth.tenant_id)
        .await?;
    state
        .standup_service
        .delete_standup(standup_id, auth.user_id, auth.tenant_id)
        .await?;

    let payload = StandupModifiedPayload {
        standup_id: standup_id.to_string(),
        title: standup.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::StandupModified,
        standup_id,
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
