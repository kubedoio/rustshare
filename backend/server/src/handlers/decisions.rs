//! HTTP handlers for decision operations.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use super::AuthenticatedUser;
use crate::handlers::AppError;
use crate::services::decision_service::DecisionSummary;
use crate::AppState;
use rustshare_core::events::{AggregateType, DecisionModifiedPayload, Event, EventType};

#[derive(Debug, Deserialize)]
pub struct CreateDecisionRequest {
    pub title: String,
    pub category: String,
    pub content: String,
}

pub async fn create_decision(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateDecisionRequest>,
) -> Result<
    (
        StatusCode,
        Json<crate::services::decision_service::Decision>,
    ),
    AppError,
> {
    let decision = state
        .decision_service
        .create_decision(
            auth.user_id,
            auth.tenant_id,
            req.title.clone(),
            req.category,
            req.content,
        )
        .await?;

    let payload = DecisionModifiedPayload {
        decision_id: decision.id.to_string(),
        title: decision.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::DecisionModified,
        decision.id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok((StatusCode::CREATED, Json(decision)))
}

pub async fn get_decision(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(decision_id): Path<Uuid>,
) -> Result<Json<crate::services::decision_service::Decision>, AppError> {
    let decision = state
        .decision_service
        .get_decision(decision_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(decision))
}

#[derive(Debug, Deserialize)]
pub struct UpdateDecisionRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub status: Option<String>,
}

pub async fn update_decision(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(decision_id): Path<Uuid>,
    Json(req): Json<UpdateDecisionRequest>,
) -> Result<Json<crate::services::decision_service::Decision>, AppError> {
    let decision = state
        .decision_service
        .update_decision(
            decision_id,
            auth.user_id,
            auth.tenant_id,
            req.title,
            req.content,
            req.status,
        )
        .await?;

    let payload = DecisionModifiedPayload {
        decision_id: decision_id.to_string(),
        title: decision.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::DecisionModified,
        decision_id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(Json(decision))
}

#[derive(Debug, Deserialize)]
pub struct RenameDecisionRequest {
    pub title: String,
}

pub async fn rename_decision(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(decision_id): Path<Uuid>,
    Json(req): Json<RenameDecisionRequest>,
) -> Result<Json<crate::services::decision_service::Decision>, AppError> {
    let decision = state
        .decision_service
        .rename_decision(decision_id, auth.user_id, auth.tenant_id, req.title)
        .await?;

    let payload = DecisionModifiedPayload {
        decision_id: decision_id.to_string(),
        title: decision.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::DecisionModified,
        decision_id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(Json(decision))
}

pub async fn delete_decision(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(decision_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let decision = state
        .decision_service
        .get_decision(decision_id, auth.user_id, auth.tenant_id)
        .await?;
    state
        .decision_service
        .delete_decision(decision_id, auth.user_id, auth.tenant_id)
        .await?;

    let payload = DecisionModifiedPayload {
        decision_id: decision_id.to_string(),
        title: decision.metadata.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::DecisionModified,
        decision_id,
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

pub async fn list_decisions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<DecisionSummary>>, AppError> {
    let decisions = state
        .decision_service
        .list_decisions(auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(decisions))
}
