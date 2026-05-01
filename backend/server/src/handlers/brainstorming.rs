//! HTTP handlers for brainstorming board operations.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AuthenticatedUser;
use crate::services::brainstorming_service::{BrainstormBoard, BrainstormError};
use crate::{handlers::ErrorResponse, AppState};

// ============================================================================
// Error Response Mapping
// ============================================================================

pub fn brainstorming_error_response(err: BrainstormError) -> Response {
    let (status, message) = match &err {
        BrainstormError::BoardNotFound => (StatusCode::NOT_FOUND, err.to_string()),
        BrainstormError::PermissionDenied => (StatusCode::FORBIDDEN, err.to_string()),
        BrainstormError::InvalidName(_) | BrainstormError::InvalidSlug(_) | BrainstormError::InvalidData(_) => {
            (StatusCode::BAD_REQUEST, err.to_string())
        }
        BrainstormError::Database(_) | BrainstormError::Storage(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        }
    };
    (status, Json(ErrorResponse::new(message))).into_response()
}

// ============================================================================
// List Boards
// ============================================================================

#[derive(Debug, Serialize)]
pub struct ListBoardsResponse {
    pub boards: Vec<BrainstormBoard>,
}

pub async fn list_brainstorm_boards(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<ListBoardsResponse>, Response> {
    let boards = state
        .brainstorming_service
        .list_boards(auth.user_id, auth.tenant_id)
        .await
        .map_err(brainstorming_error_response)?;

    Ok(Json(ListBoardsResponse { boards }))
}

// ============================================================================
// Create Board
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateBoardRequest {
    pub title: String,
    pub template_key: String,
}

#[derive(Debug, Serialize)]
pub struct CreateBoardResponse {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub path: String,
    pub template: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn create_brainstorm_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateBoardRequest>,
) -> Result<(StatusCode, Json<CreateBoardResponse>), Response> {
    // Validate title
    if req.title.trim().is_empty() {
        return Err(brainstorming_error_response(BrainstormError::InvalidName(
            "Title cannot be empty".to_string(),
        )));
    }
    if req.title.contains('/') || req.title.contains('\0') {
        return Err(brainstorming_error_response(BrainstormError::InvalidName(
            "Title cannot contain slashes or null characters".to_string(),
        )));
    }

    // Validate template key
    let valid_templates = [
        "template_blank_brainstorm",
        "template_decision_making_brainstorm",
        "template_meeting_whiteboard",
    ];
    if !valid_templates.contains(&req.template_key.as_str()) {
        return Err(brainstorming_error_response(BrainstormError::InvalidData(
            format!("Invalid template key: {}", req.template_key),
        )));
    }

    // Create from template
    let object = state
        .template_service
        .create_from_template(
            &req.template_key,
            auth.user_id,
            auth.tenant_id,
            req.title.clone(),
            None,
        )
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else if e.to_string().contains("disabled") || e.to_string().contains("denied") {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        })?;

    // Parse created board
    let board = state
        .brainstorming_service
        .get_board(object.object_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(brainstorming_error_response)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateBoardResponse {
            id: board.id,
            title: board.title,
            slug: board.slug,
            path: board.path,
            template: board.template,
            created_at: board.created_at.to_rfc3339(),
            updated_at: board.updated_at.to_rfc3339(),
        }),
    ))
}

// ============================================================================
// Get Board
// ============================================================================

#[derive(Debug, Serialize)]
pub struct GetBoardResponse {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub path: String,
    pub template: String,
    pub source_file_id: Option<String>,
    pub preview_file_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get_brainstorm_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
) -> Result<Json<GetBoardResponse>, Response> {
    let board = state
        .brainstorming_service
        .get_board(board_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(brainstorming_error_response)?;

    Ok(Json(GetBoardResponse {
        id: board.id,
        title: board.title,
        slug: board.slug,
        path: board.path,
        template: board.template,
        source_file_id: board.source_file_id,
        preview_file_id: board.preview_file_id,
        created_at: board.created_at.to_rfc3339(),
        updated_at: board.updated_at.to_rfc3339(),
    }))
}

// ============================================================================
// Get Board Source
// ============================================================================

#[derive(Debug, Serialize)]
pub struct GetBoardSourceResponse {
    pub source: String,
}

pub async fn get_brainstorm_board_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
) -> Result<Json<GetBoardSourceResponse>, Response> {
    let source = state
        .brainstorming_service
        .get_board_source(board_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(brainstorming_error_response)?;

    Ok(Json(GetBoardSourceResponse { source }))
}

// ============================================================================
// Save Board Source
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SaveBoardSourceRequest {
    pub source: String,
}

pub async fn save_brainstorm_board_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
    Json(req): Json<SaveBoardSourceRequest>,
) -> Result<Json<GetBoardResponse>, Response> {
    let board = state
        .brainstorming_service
        .save_board_source(board_id, auth.user_id, auth.tenant_id, req.source)
        .await
        .map_err(brainstorming_error_response)?;

    Ok(Json(GetBoardResponse {
        id: board.id,
        title: board.title,
        slug: board.slug,
        path: board.path,
        template: board.template,
        source_file_id: board.source_file_id,
        preview_file_id: board.preview_file_id,
        created_at: board.created_at.to_rfc3339(),
        updated_at: board.updated_at.to_rfc3339(),
    }))
}

// ============================================================================
// Update Board Preview
// ============================================================================

pub async fn update_brainstorm_board_preview(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
    body: Bytes,
) -> Result<Json<GetBoardResponse>, Response> {
    let board = state
        .brainstorming_service
        .update_board_preview(board_id, auth.user_id, auth.tenant_id, body)
        .await
        .map_err(brainstorming_error_response)?;

    Ok(Json(GetBoardResponse {
        id: board.id,
        title: board.title,
        slug: board.slug,
        path: board.path,
        template: board.template,
        source_file_id: board.source_file_id,
        preview_file_id: board.preview_file_id,
        created_at: board.created_at.to_rfc3339(),
        updated_at: board.updated_at.to_rfc3339(),
    }))
}

// ============================================================================
// Delete Board
// ============================================================================

pub async fn delete_brainstorm_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
) -> Result<StatusCode, Response> {
    state
        .brainstorming_service
        .delete_board(board_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(brainstorming_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}
