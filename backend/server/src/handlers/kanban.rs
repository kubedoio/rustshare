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

use crate::services::application_service::ApplicationError;

async fn require_kanban_enabled(state: &AppState, tenant_id: Uuid) -> Result<(), AppError> {
    let module = state
        .application_service
        .get_application("io.elembra.kanban", tenant_id)
        .await;
    let module = match module {
        Ok(m) => m,
        Err(ApplicationError::NotFound(_)) => {
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
    KanbanChecklistItem, KanbanLabel, MoveCardInput, UpdateBoardInput, UpdateCardInput,
    UpdateLabelInput,
};
use axum::extract::Multipart;

// ============================================================================
// Boards
// ============================================================================

#[utoipa::path(
    get,
    path = "/api/v1/applications/kanban/boards",
    tag = "Kanban",
    responses(
        (status = 200, description = "Success", body = Vec<KanbanBoardSummary>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_boards(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<crate::handlers::PaginationQuery>,
) -> Result<Json<Vec<KanbanBoardSummary>>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let boards = state
        .kanban_service
        .list_boards(auth.user_id, auth.tenant_id, query.limit(), query.offset())
        .await?;

    Ok(Json(boards))
}

#[utoipa::path(
    post,
    path = "/api/v1/applications/kanban/boards",
    tag = "Kanban",
    request_body = CreateBoardInput,
    responses(
        (status = 200, description = "Success", body = KanbanBoard),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    get,
    path = "/api/v1/applications/kanban/boards/{board_id}",
    tag = "Kanban",
    params(("board_id_or_slug" = String, Path, description = "Board Id Or Slug")),
    responses(
        (status = 200, description = "Success", body = KanbanBoard),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    patch,
    path = "/api/v1/applications/kanban/boards/{board_id}",
    tag = "Kanban",
    params(("board_id_or_slug" = String, Path, description = "Board Id Or Slug")),
    request_body = UpdateBoardInput,
    responses(
        (status = 200, description = "Success", body = KanbanBoard),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    post,
    path = "/api/v1/applications/kanban/boards/{board_id}/archive",
    tag = "Kanban",
    params(("board_id_or_slug" = String, Path, description = "Board Id Or Slug")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    post,
    path = "/api/v1/applications/kanban/boards/{board_id}/labels",
    tag = "Kanban",
    params(("board_id" = Uuid, Path, description = "Board Id")),
    request_body = CreateLabelInput,
    responses(
        (status = 200, description = "Success", body = KanbanLabel),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_label(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
    Json(input): Json<CreateLabelInput>,
) -> Result<(StatusCode, Json<KanbanLabel>), AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let label = state
        .kanban_service
        .create_label(board_id, input, auth.user_id, auth.tenant_id)
        .await?;

    Ok((StatusCode::CREATED, Json(label)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/applications/kanban/boards/{board_id}/labels/{label_id}",
    tag = "Kanban",
    params(("board_id" = Uuid, Path, description = "Board Id"), ("label_id" = String, Path, description = "Label Id")),
    request_body = UpdateLabelInput,
    responses(
        (status = 200, description = "Success", body = KanbanLabel),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_label(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((board_id, label_id)): Path<(Uuid, String)>,
    Json(input): Json<UpdateLabelInput>,
) -> Result<Json<KanbanLabel>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let label = state
        .kanban_service
        .update_label(board_id, label_id, input, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(label))
}

#[utoipa::path(
    delete,
    path = "/api/v1/applications/kanban/boards/{board_id}/labels/{label_id}",
    tag = "Kanban",
    params(("board_id" = Uuid, Path, description = "Board Id"), ("label_id" = String, Path, description = "Label Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn delete_label(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((board_id, label_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .delete_label(board_id, label_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/applications/kanban/cards/{card_id}/labels",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id")),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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
        .add_card_label(card_id, label_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/applications/kanban/cards/{card_id}/labels/{label_id}",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id"), ("label_id" = String, Path, description = "Label Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn remove_card_label(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((card_id, label_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .remove_card_label(card_id, label_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// -------------------------------------------------------------------------
// Assignees
// -------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/applications/kanban/assignable-users",
    tag = "Kanban",
    responses(
        (status = 200, description = "Success", body = Vec<KanbanAssignee>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    post,
    path = "/api/v1/applications/kanban/cards/{card_id}/assignees",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id")),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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
        .assign_card_member(card_id, assignee_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/applications/kanban/cards/{card_id}/assignees/{assignee_id}",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id"), ("assignee_id" = String, Path, description = "Assignee Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn unassign_card_member(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((card_id, assignee_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    state
        .kanban_service
        .unassign_card_member(card_id, assignee_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Cards
// ============================================================================

#[utoipa::path(
    get,
    path = "/api/v1/applications/kanban/boards/{board_id}/cards",
    tag = "Kanban",
    params(("board_id_or_slug" = String, Path, description = "Board Id Or Slug")),
    responses(
        (status = 200, description = "Success", body = Vec<KanbanCard>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_cards(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id_or_slug): Path<String>,
    Query(query): Query<crate::handlers::PaginationQuery>,
) -> Result<Json<Vec<KanbanCard>>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let cards = state
        .kanban_service
        .list_cards(
            board_id_or_slug,
            auth.user_id,
            auth.tenant_id,
            query.limit(),
            query.offset(),
        )
        .await?;

    Ok(Json(cards))
}

#[utoipa::path(
    post,
    path = "/api/v1/applications/kanban/boards/{board_id}/cards",
    tag = "Kanban",
    params(("board_id_or_slug" = String, Path, description = "Board Id Or Slug")),
    request_body = CreateCardInput,
    responses(
        (status = 200, description = "Success", body = KanbanCard),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    get,
    path = "/api/v1/applications/kanban/cards/{card_id}",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id")),
    responses(
        (status = 200, description = "Success", body = KanbanCard),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_card(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
) -> Result<Json<KanbanCard>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let card = state
        .kanban_service
        .get_card(card_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(card))
}

#[utoipa::path(
    get,
    path = "/api/v1/applications/kanban/cards/{card_id}/detail",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id")),
    responses(
        (status = 200, description = "Success", body = KanbanCardDetail),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_card_detail(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
) -> Result<Json<KanbanCardDetail>, AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let detail = state
        .kanban_service
        .get_card_detail(card_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(detail))
}

#[utoipa::path(
    patch,
    path = "/api/v1/applications/kanban/cards/{card_id}",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id")),
    request_body = UpdateCardInput,
    responses(
        (status = 200, description = "Success", body = KanbanCard),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    put,
    path = "/api/v1/applications/kanban/cards/{card_id}/description",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id")),
    request_body = UpdateCardDescriptionInput,
    responses(
        (status = 200, description = "Success", body = KanbanCard),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateCardDescriptionInput {
    pub content: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/applications/kanban/cards/{card_id}/move",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id")),
    request_body = MoveCardInput,
    responses(
        (status = 200, description = "Success", body = KanbanBoard),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    post,
    path = "/api/v1/applications/kanban/cards/{card_id}/archive",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id")),
    responses(
        (status = 200, description = "Success", body = KanbanCard),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    delete,
    path = "/api/v1/applications/kanban/cards/{card_id}",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    post,
    path = "/api/v1/applications/kanban/cards/{card_id}/attachments",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id")),
    responses(
        (status = 200, description = "Success", body = KanbanCardAttachment),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn add_card_attachment(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(card_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<KanbanCardAttachment>), AppError> {
    require_kanban_enabled(&state, auth.tenant_id).await?;
    let mut file_data: Option<bytes::Bytes> = None;
    let mut file_name: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(format!("Failed to read multipart field: {}", e)))?
    {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "file" {
            file_name = field.file_name().map(|s| s.to_string());
            file_data =
                Some(field.bytes().await.map_err(|e| {
                    AppError::bad_request(format!("Failed to read file data: {}", e))
                })?);
        }
    }

    let file_data = file_data.ok_or_else(|| AppError::bad_request("Missing file data"))?;
    let file_name = file_name.ok_or_else(|| AppError::bad_request("Missing file name"))?;

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

#[utoipa::path(
    delete,
    path = "/api/v1/applications/kanban/cards/{card_id}/attachments/{attachment_id}",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id"), ("attachment_id" = Uuid, Path, description = "Attachment Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AddChecklistInput {
    pub title: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AddChecklistItemInput {
    pub text: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ToggleChecklistItemInput {
    pub done: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/applications/kanban/cards/{card_id}/checklists",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id")),
    request_body = AddChecklistInput,
    responses(
        (status = 200, description = "Success", body = KanbanChecklistGroup),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    post,
    path = "/api/v1/applications/kanban/cards/{card_id}/checklists/{checklist_id}/items",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id"), ("checklist_id" = String, Path, description = "Checklist Id")),
    request_body = AddChecklistItemInput,
    responses(
        (status = 200, description = "Success", body = KanbanChecklistItem),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    patch,
    path = "/api/v1/applications/kanban/cards/{card_id}/checklists/{checklist_id}/items/{item_id}",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id"), ("checklist_id" = String, Path, description = "Checklist Id"), ("item_id" = String, Path, description = "Item Id")),
    request_body = ToggleChecklistItemInput,
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    delete,
    path = "/api/v1/applications/kanban/cards/{card_id}/checklists/{checklist_id}/items/{item_id}",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id"), ("checklist_id" = String, Path, description = "Checklist Id"), ("item_id" = String, Path, description = "Item Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    delete,
    path = "/api/v1/applications/kanban/cards/{card_id}/checklists/{checklist_id}",
    tag = "Kanban",
    params(("card_id" = Uuid, Path, description = "Card Id"), ("checklist_id" = String, Path, description = "Checklist Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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
