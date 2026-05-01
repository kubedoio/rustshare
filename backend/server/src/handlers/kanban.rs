//! HTTP handlers for Kanban operations.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handlers::{extractors::AuthenticatedUser, ErrorResponse},
    state::AppState,
};

use crate::services::kanban_service::{
    CreateBoardInput, CreateCardInput, KanbanBoard, KanbanBoardSummary, KanbanCard,
    KanbanError, MoveCardInput, UpdateBoardInput, UpdateCardInput,
};

// ============================================================================
// Error response mapping
// ============================================================================

fn kanban_error_response(err: KanbanError) -> Response {
    let (status, message) = match err {
        KanbanError::BoardNotFound | KanbanError::CardNotFound | KanbanError::ColumnNotFound(_) => {
            (StatusCode::NOT_FOUND, err.to_string())
        }
        KanbanError::PermissionDenied => (StatusCode::FORBIDDEN, err.to_string()),
        KanbanError::InvalidName(_) | KanbanError::InvalidData(_) => {
            (StatusCode::BAD_REQUEST, err.to_string())
        }
        KanbanError::Database(_) | KanbanError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        ),
    };
    (status, Json(ErrorResponse::new(message))).into_response()
}

// ============================================================================
// Boards
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListBoardsQuery {
    pub limit: Option<usize>,
}

pub async fn list_boards(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ListBoardsQuery>,
) -> Result<Json<Vec<KanbanBoardSummary>>, Response> {
    let mut boards = state
        .kanban_service
        .list_boards(auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    if let Some(limit) = query.limit {
        boards.truncate(limit);
    }

    Ok(Json(boards))
}

pub async fn create_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateBoardInput>,
) -> Result<(StatusCode, Json<KanbanBoard>), Response> {
    let board = state
        .kanban_service
        .create_board(req, auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    Ok((StatusCode::CREATED, Json(board)))
}

pub async fn get_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id_or_slug): Path<String>,
) -> Result<Json<KanbanBoard>, Response> {
    let board = state
        .kanban_service
        .get_board(board_id_or_slug, auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    Ok(Json(board))
}

pub async fn update_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id_or_slug): Path<String>,
    Json(req): Json<UpdateBoardInput>,
) -> Result<Json<KanbanBoard>, Response> {
    let board = state
        .kanban_service
        .update_board(board_id_or_slug, req, auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    Ok(Json(board))
}

pub async fn archive_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id_or_slug): Path<String>,
) -> Result<StatusCode, Response> {
    state
        .kanban_service
        .archive_board(board_id_or_slug, auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Cards
// ============================================================================

pub async fn list_cards(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id_or_slug): Path<String>,
) -> Result<Json<Vec<KanbanCard>>, Response> {
    let cards = state
        .kanban_service
        .list_cards(board_id_or_slug, auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    Ok(Json(cards))
}

pub async fn create_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id_or_slug): Path<String>,
    Json(req): Json<CreateCardInput>,
) -> Result<(StatusCode, Json<KanbanCard>), Response> {
    // column_id is part of the request body for simplicity
    // but spec says POST /boards/:boardId/cards - we accept column_id in body
    let card = state
        .kanban_service
        .create_card(board_id_or_slug, req, auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    Ok((StatusCode::CREATED, Json(card)))
}

pub async fn get_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
) -> Result<Json<KanbanCard>, Response> {
    let card = state
        .kanban_service
        .get_card(card_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    Ok(Json(card))
}

pub async fn update_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
    Json(req): Json<UpdateCardInput>,
) -> Result<Json<KanbanCard>, Response> {
    let card = state
        .kanban_service
        .update_card(card_id, req, auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    Ok(Json(card))
}

pub async fn move_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
    Json(req): Json<MoveCardInput>,
) -> Result<Json<KanbanBoard>, Response> {
    let board = state
        .kanban_service
        .move_card(card_id, req.target_column_id, req.target_order, auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    Ok(Json(board))
}

pub async fn archive_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
) -> Result<Json<KanbanCard>, Response> {
    let card = state
        .kanban_service
        .archive_card(card_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    Ok(Json(card))
}

pub async fn delete_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
) -> Result<StatusCode, Response> {
    state
        .kanban_service
        .delete_card(card_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(kanban_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}
