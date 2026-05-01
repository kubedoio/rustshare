//! HTTP handlers for decision operations.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use super::AuthenticatedUser;
use crate::services::decision_service::{DecisionError, DecisionSummary};
use crate::{handlers::ErrorResponse, AppState};

pub fn decision_error_response(err: DecisionError) -> Response {
    let (status, message) = match err {
        DecisionError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        DecisionError::PermissionDenied => (StatusCode::FORBIDDEN, err.to_string()),
        DecisionError::InvalidData(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        DecisionError::Database(_) | DecisionError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        ),
    };
    (status, Json(ErrorResponse::new(message))).into_response()
}

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
) -> Result<(StatusCode, Json<crate::services::decision_service::Decision>), Response> {
    let decision = state
        .decision_service
        .create_decision(
            auth.user_id,
            auth.tenant_id,
            req.title,
            req.category,
            req.content,
        )
        .await
        .map_err(decision_error_response)?;

    Ok((StatusCode::CREATED, Json(decision)))
}

pub async fn get_decision(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(decision_id): Path<Uuid>,
) -> Result<Json<crate::services::decision_service::Decision>, Response> {
    let decision = state
        .decision_service
        .get_decision(decision_id, auth.user_id)
        .await
        .map_err(decision_error_response)?;

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
) -> Result<Json<crate::services::decision_service::Decision>, Response> {
    let decision = state
        .decision_service
        .update_decision(
            decision_id,
            auth.user_id,
            req.title,
            req.content,
            req.status,
        )
        .await
        .map_err(decision_error_response)?;

    Ok(Json(decision))
}

pub async fn list_decisions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<DecisionSummary>>, Response> {
    let decisions = state
        .decision_service
        .list_decisions(auth.user_id, auth.tenant_id)
        .await
        .map_err(decision_error_response)?;

    Ok(Json(decisions))
}
