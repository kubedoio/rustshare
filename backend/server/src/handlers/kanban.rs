//! HTTP handlers for Kanban operations.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    handlers::{extractors::AuthenticatedUser, AppError},
    state::AppState,
};

use crate::services::module_service::ModuleError;

async fn require_kanban_enabled(state: &AppState, tenant_id: Uuid) -> Result<(), AppError> {
    let module = state.module_service.get_module("kanban", tenant_id).await;
    let module = match module {
        Ok(m) => m,
        Err(ModuleError::NotFound(_)) => {
            return Err(AppError::forbidden("Kanban module is disabled"));
        }
        Err(e) => {
            return Err(AppError::internal(e.to_string()));
        }
    };
    if !module.enabled {
        return Err(AppError::forbidden("Kanban module is disabled"));
    }
    Ok(())
}

use crate::services::kanban_service::{
    CreateBoardInput, CreateCardInput, CreateLabelInput, KanbanAssignee, KanbanBoard,
    KanbanBoardSummary, KanbanCard, KanbanCardAttachment, KanbanCardDetail, KanbanChecklistGroup,
    KanbanChecklistItem, KanbanLabel, MoveCardInput, UpdateBoardInput,
    UpdateCardInput, UpdateLabelInput,
};
use axum::extract::Multipart;

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
) -> Result<Json<Vec<KanbanBoardSummary>>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let mut boards = state
        .kanban_service
        .list_boards(auth.user_id, auth.tenant_id)
        .await?;

    if let Some(limit) = query.limit {
        boards.truncate(limit);
    }

    Ok(Json(boards))
}

pub async fn create_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateBoardInput>,
) -> Result<(StatusCode, Json<KanbanBoard>), AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let board = state
        .kanban_service
        .create_board(req, auth.user_id, auth.tenant_id)
        .await?;

    Ok((StatusCode::CREATED, Json(board)))
}

pub async fn get_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id_or_slug): Path<String>,
) -> Result<Json<KanbanBoard>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let board = state
        .kanban_service
        .get_board(board_id_or_slug, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(board))
}

pub async fn update_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id_or_slug): Path<String>,
    Json(req): Json<UpdateBoardInput>,
) -> Result<Json<KanbanBoard>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let board = state
        .kanban_service
        .update_board(board_id_or_slug, req, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(board))
}

pub async fn archive_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id_or_slug): Path<String>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .archive_board(board_id_or_slug, auth.user_id, auth.tenant_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// -------------------------------------------------------------------------
// Labels
// -------------------------------------------------------------------------

pub async fn create_label(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
    Json(input): Json<CreateLabelInput>,
) -> Result<(StatusCode, Json<KanbanLabel>), AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let label = state
        .kanban_service
        .create_label(board_id, input, auth.user_id)
        .await?;

    Ok((StatusCode::CREATED, Json(label)))
}

pub async fn update_label(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((board_id, label_id)): Path<(Uuid, String)>,
    Json(input): Json<UpdateLabelInput>,
) -> Result<Json<KanbanLabel>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let label = state
        .kanban_service
        .update_label(board_id, label_id, input, auth.user_id)
        .await?;

    Ok(Json(label))
}

pub async fn delete_label(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((board_id, label_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .delete_label(board_id, label_id, auth.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn add_card_label(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let label_id = payload["labelId"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("labelId required"))?
        .to_string();

    state
        .kanban_service
        .add_card_label(card_id, label_id, auth.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_card_label(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((card_id, label_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .remove_card_label(card_id, label_id, auth.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// -------------------------------------------------------------------------
// Assignees
// -------------------------------------------------------------------------

pub async fn get_assignable_users(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<KanbanAssignee>>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let users = state
        .kanban_service
        .get_assignable_users(auth.tenant_id)
        .await?;

    Ok(Json(users))
}

pub async fn assign_card_member(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let assignee_id = payload["assigneeId"]
        .as_str()
        .ok_or_else(|| AppError::bad_request("assigneeId required"))?
        .to_string();

    state
        .kanban_service
        .assign_card_member(card_id, assignee_id, auth.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn unassign_card_member(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((card_id, assignee_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .unassign_card_member(card_id, assignee_id, auth.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Cards
// ============================================================================

pub async fn list_cards(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id_or_slug): Path<String>,
) -> Result<Json<Vec<KanbanCard>>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let cards = state
        .kanban_service
        .list_cards(board_id_or_slug, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(cards))
}

pub async fn create_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id_or_slug): Path<String>,
    Json(req): Json<CreateCardInput>,
) -> Result<(StatusCode, Json<KanbanCard>), AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let card = state
        .kanban_service
        .create_card(board_id_or_slug, req, auth.user_id, auth.tenant_id)
        .await?;

    Ok((StatusCode::CREATED, Json(card)))
}

pub async fn get_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
) -> Result<Json<KanbanCard>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let card = state
        .kanban_service
        .get_card(card_id, auth.user_id)
        .await?;

    Ok(Json(card))
}

pub async fn get_card_detail(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
) -> Result<Json<KanbanCardDetail>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let detail = state
        .kanban_service
        .get_card_detail(card_id, auth.user_id)
        .await?;

    Ok(Json(detail))
}

pub async fn update_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
    Json(req): Json<UpdateCardInput>,
) -> Result<Json<KanbanCard>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let card = state
        .kanban_service
        .update_card(card_id, req, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(card))
}

pub async fn update_card_description(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
    Json(req): Json<UpdateCardDescriptionInput>,
) -> Result<Json<KanbanCard>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let card = state
        .kanban_service
        .update_card_description(card_id, req.content, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(card))
}

#[derive(Debug, Deserialize)]
pub struct UpdateCardDescriptionInput {
    pub content: String,
}

pub async fn move_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
    Json(req): Json<MoveCardInput>,
) -> Result<Json<KanbanBoard>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let board = state
        .kanban_service
        .move_card(card_id, req, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(board))
}

pub async fn archive_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
) -> Result<Json<KanbanCard>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let card = state
        .kanban_service
        .archive_card(card_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(card))
}

pub async fn delete_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .delete_card(card_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Attachments
// ============================================================================

pub async fn add_card_attachment(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<KanbanCardAttachment>), AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let mut file_data: Option<bytes::Bytes> = None;
    let mut file_name: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError::bad_request(format!(
            "Failed to read multipart field: {}",
            e
        ))
    })? {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "file" {
            file_name = field.file_name().map(|s| s.to_string());
            file_data = Some(field.bytes().await.map_err(|e| {
                AppError::bad_request(format!(
                    "Failed to read file data: {}",
                    e
                ))
            })?);
        }
    }

    let file_data = file_data.ok_or_else(|| {
        AppError::bad_request("Missing file data")
    })?;
    let file_name = file_name.ok_or_else(|| {
        AppError::bad_request("Missing file name")
    })?;

    let mime_type = mime_guess::from_path(&file_name)
        .first_or_octet_stream()
        .to_string();

    let attachment = state
        .kanban_service
        .add_card_attachment(
            card_id,
            file_name,
            file_data,
            mime_type,
            auth.user_id,
            auth.tenant_id,
        )
        .await?;

    Ok((StatusCode::CREATED, Json(attachment)))
}

pub async fn delete_card_attachment(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((card_id, attachment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .delete_card_attachment(card_id, attachment_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Checklists
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AddChecklistInput {
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct AddChecklistItemInput {
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct ToggleChecklistItemInput {
    pub done: bool,
}

pub async fn create_checklist(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
    Json(req): Json<AddChecklistInput>,
) -> Result<(StatusCode, Json<KanbanChecklistGroup>), AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let group = state
        .kanban_service
        .add_checklist(card_id, req.title, auth.user_id, auth.tenant_id)
        .await?;

    Ok((StatusCode::CREATED, Json(group)))
}

pub async fn create_checklist_item(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((card_id, checklist_id)): Path<(Uuid, String)>,
    Json(req): Json<AddChecklistItemInput>,
) -> Result<(StatusCode, Json<KanbanChecklistItem>), AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let item = state
        .kanban_service
        .add_checklist_item(
            card_id,
            checklist_id,
            req.text,
            auth.user_id,
            auth.tenant_id,
        )
        .await?;

    Ok((StatusCode::CREATED, Json(item)))
}

pub async fn toggle_checklist_item(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((card_id, checklist_id, item_id)): Path<(Uuid, String, String)>,
    Json(req): Json<ToggleChecklistItemInput>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .toggle_checklist_item(
            card_id,
            checklist_id,
            item_id,
            req.done,
            auth.user_id,
            auth.tenant_id,
        )
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_checklist_item(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((card_id, checklist_id, item_id)): Path<(Uuid, String, String)>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .delete_checklist_item(card_id, checklist_id, item_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_checklist(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((card_id, checklist_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .delete_checklist(card_id, checklist_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
