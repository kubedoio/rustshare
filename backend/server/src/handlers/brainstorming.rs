//! HTTP handlers for brainstorming board operations.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AuthenticatedUser;
use crate::handlers::AppError;
use crate::services::application_service::ApplicationError;
use crate::services::brainstorming_service::BrainstormBoard;
use crate::AppState;
use rustshare_core::events::{AggregateType, BrainstormBoardModifiedPayload, Event, EventType};

// ============================================================================
// List Boards
// ============================================================================

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListBoardsResponse {
    pub boards: Vec<BrainstormBoard>,
}

async fn require_brainstorming_enabled(state: &AppState, tenant_id: Uuid) -> Result<(), AppError> {
    let module = state
        .application_service
        .get_application("brainstorming", tenant_id)
        .await;
    let module = match module {
        Ok(m) => m,
        Err(ApplicationError::NotFound(_)) => {
            return Err(AppError::forbidden("Brainstorming module is disabled"));
        }
        Err(e) => {
            return Err(AppError::internal(e.to_string()));
        }
    };
    if !module.enabled {
        return Err(AppError::forbidden("Brainstorming module is disabled"));
    }
    Ok(())
}

#[utoipa::path(
    get,
    path = "/api/v1/applications/brainstorming/boards",
    tag = "Brainstorming",
    responses(
        (status = 200, description = "Success", body = ListBoardsResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_brainstorm_boards(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<crate::handlers::PaginationQuery>,
) -> Result<Json<ListBoardsResponse>, AppError> {
    require_brainstorming_enabled(&state, auth.tenant_id).await?;
    let boards = state
        .brainstorming_service
        .list_boards(auth.user_id, auth.tenant_id, query.limit(), query.offset())
        .await?;

    Ok(Json(ListBoardsResponse { boards }))
}

// ============================================================================
// Create Board
// ============================================================================

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateBoardRequest {
    pub title: String,
    pub template_key: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateBoardResponse {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub path: String,
    pub template: String,
    pub created_at: String,
    pub updated_at: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/applications/brainstorming/boards",
    tag = "Brainstorming",
    request_body = CreateBoardRequest,
    responses(
        (status = 200, description = "Success", body = CreateBoardResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_brainstorm_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateBoardRequest>,
) -> Result<(StatusCode, Json<CreateBoardResponse>), AppError> {
    require_brainstorming_enabled(&state, auth.tenant_id).await?;
    // Validate title
    if req.title.trim().is_empty() {
        return Err(AppError::bad_request("Title cannot be empty"));
    }
    if req.title.contains('/') || req.title.contains('\0') {
        return Err(AppError::bad_request(
            "Title cannot contain slashes or null characters",
        ));
    }

    // Validate template key
    let valid_templates = ["template_blank_brainstorm"];
    if !valid_templates.contains(&req.template_key.as_str()) {
        return Err(AppError::bad_request(format!(
            "Invalid template key: {}",
            req.template_key
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
            if e.to_string().contains("not found") {
                AppError::not_found(e.to_string())
            } else if e.to_string().contains("disabled") || e.to_string().contains("denied") {
                AppError::forbidden(e.to_string())
            } else if e.to_string().contains("already exists") {
                AppError::conflict(e.to_string())
            } else {
                AppError::internal("Internal server error")
            }
        })?;

    // Parse created board
    let board = state
        .brainstorming_service
        .get_board(object.object_id, auth.user_id, auth.tenant_id)
        .await?;

    let payload = BrainstormBoardModifiedPayload {
        board_id: object.object_id.to_string(),
        title: req.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::BrainstormBoardModified,
        object.object_id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

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

#[derive(Debug, Serialize, utoipa::ToSchema)]
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

#[utoipa::path(
    get,
    path = "/api/v1/applications/brainstorming/boards/{board_id}",
    tag = "Brainstorming",
    params(("board_id" = Uuid, Path, description = "Board Id")),
    responses(
        (status = 200, description = "Success", body = GetBoardResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_brainstorm_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
) -> Result<Json<GetBoardResponse>, AppError> {
    require_brainstorming_enabled(&state, auth.tenant_id).await?;
    let board = state
        .brainstorming_service
        .get_board(board_id, auth.user_id, auth.tenant_id)
        .await?;

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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct GetBoardSourceResponse {
    pub source: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/applications/brainstorming/boards/{board_id}/source",
    tag = "Brainstorming",
    params(("board_id" = Uuid, Path, description = "Board Id")),
    responses(
        (status = 200, description = "Success", body = GetBoardSourceResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_brainstorm_board_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
) -> Result<Json<GetBoardSourceResponse>, AppError> {
    require_brainstorming_enabled(&state, auth.tenant_id).await?;
    tracing::info!(
        board_id = %board_id,
        user_id = %auth.user_id,
        tenant_id = %auth.tenant_id,
        "handler: get_brainstorm_board_source called"
    );
    let source = state
        .brainstorming_service
        .get_board_source(board_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok(Json(GetBoardSourceResponse { source }))
}

// ============================================================================
// Save Board Source
// ============================================================================

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SaveBoardSourceRequest {
    pub source: String,
}

#[utoipa::path(
    put,
    path = "/api/v1/applications/brainstorming/boards/{board_id}/source",
    tag = "Brainstorming",
    params(("board_id" = Uuid, Path, description = "Board Id")),
    request_body = SaveBoardSourceRequest,
    responses(
        (status = 200, description = "Success", body = GetBoardResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn save_brainstorm_board_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
    Json(req): Json<SaveBoardSourceRequest>,
) -> Result<Json<GetBoardResponse>, AppError> {
    require_brainstorming_enabled(&state, auth.tenant_id).await?;
    tracing::info!(
        board_id = %board_id,
        user_id = %auth.user_id,
        tenant_id = %auth.tenant_id,
        "handler: save_brainstorm_board_source called"
    );
    let board = state
        .brainstorming_service
        .save_board_source(board_id, auth.user_id, auth.tenant_id, req.source)
        .await?;

    let payload = BrainstormBoardModifiedPayload {
        board_id: board_id.to_string(),
        title: board.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::BrainstormBoardModified,
        board_id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

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

#[utoipa::path(
    put,
    path = "/api/v1/applications/brainstorming/boards/{board_id}/preview",
    tag = "Brainstorming",
    params(("board_id" = Uuid, Path, description = "Board Id")),
    responses(
        (status = 200, description = "Success", body = GetBoardResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_brainstorm_board_preview(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
    body: Bytes,
) -> Result<Json<GetBoardResponse>, AppError> {
    require_brainstorming_enabled(&state, auth.tenant_id).await?;
    let board = state
        .brainstorming_service
        .update_board_preview(board_id, auth.user_id, auth.tenant_id, body)
        .await?;

    let payload = BrainstormBoardModifiedPayload {
        board_id: board_id.to_string(),
        title: board.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::BrainstormBoardModified,
        board_id,
        AggregateType::File,
        serde_json::to_value(payload).map_err(|e| AppError::internal(e.to_string()))?,
        auth.user_id,
    );
    state
        .event_store
        .append(&event, &state.broadcaster)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

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

#[utoipa::path(
    delete,
    path = "/api/v1/applications/brainstorming/boards/{board_id}",
    tag = "Brainstorming",
    params(("board_id" = Uuid, Path, description = "Board Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn delete_brainstorm_board(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(board_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_brainstorming_enabled(&state, auth.tenant_id).await?;
    let board = state
        .brainstorming_service
        .get_board(board_id, auth.user_id, auth.tenant_id)
        .await?;
    state
        .brainstorming_service
        .delete_board(board_id, auth.user_id, auth.tenant_id)
        .await?;

    let payload = BrainstormBoardModifiedPayload {
        board_id: board_id.to_string(),
        title: board.title.clone(),
        modified_by: auth.user_id,
    };
    let event = Event::new(
        EventType::BrainstormBoardModified,
        board_id,
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
