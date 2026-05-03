//! User-facing module handlers.

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use rustshare_core::domain::{CreateFromTemplateRequest, CreatedObject, Module};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::{
    handlers::{admin::log_admin_action, extractors::AuthenticatedUser, ErrorResponse},
    state::AppState,
};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct EnabledModulesResponse {
    pub modules: Vec<Module>,
}

#[derive(Debug, Serialize)]
pub struct ModuleDetailResponse {
    pub module: Module,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn list_enabled_modules(
    AuthenticatedUser { user_id, tenant_id }: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<Json<EnabledModulesResponse>, axum::response::Response> {
    let modules = state
        .module_service
        .list_enabled_modules(tenant_id, user_id)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response()
        })?;

    Ok(Json(EnabledModulesResponse { modules }))
}

pub async fn get_module(
    AuthenticatedUser { user_id, tenant_id }: AuthenticatedUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ModuleDetailResponse>, axum::response::Response> {
    let module = state
        .module_service
        .get_module(&key, tenant_id)
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("not found") {
                axum::http::StatusCode::NOT_FOUND
            } else {
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        })?;

    if !module.enabled {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Module disabled".to_string())),
        )
            .into_response());
    }

    let visible_modules = state
        .module_service
        .list_enabled_modules(tenant_id, user_id)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response()
        })?;

    if !visible_modules
        .iter()
        .any(|visible| visible.module_key == module.module_key)
    {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied".to_string())),
        )
            .into_response());
    }

    Ok(Json(ModuleDetailResponse { module }))
}

#[derive(Debug, Serialize)]
pub struct ModuleSummaryResponse {
    pub summary: crate::services::module_service::ModuleSummary,
}

pub async fn get_module_summary(
    AuthenticatedUser { user_id, tenant_id }: AuthenticatedUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ModuleSummaryResponse>, axum::response::Response> {
    let summary = state
        .module_service
        .get_module_summary(&key, tenant_id, user_id)
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("not found") {
                axum::http::StatusCode::NOT_FOUND
            } else if e.to_string().contains("Permission denied") {
                axum::http::StatusCode::FORBIDDEN
            } else {
                axum::http::StatusCode::BAD_REQUEST
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        })?;

    Ok(Json(ModuleSummaryResponse { summary }))
}

pub async fn create_from_template(
    AuthenticatedUser { user_id, tenant_id }: AuthenticatedUser,
    State(state): State<AppState>,
    Json(body): Json<CreateFromTemplateRequest>,
) -> Result<Json<CreatedObject>, axum::response::Response> {
    let object = state
        .template_service
        .create_from_template(
            &body.template_key,
            user_id,
            tenant_id,
            body.name,
            body.parent_folder_id,
        )
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("not found") {
                axum::http::StatusCode::NOT_FOUND
            } else if e.to_string().contains("disabled") || e.to_string().contains("denied") {
                axum::http::StatusCode::FORBIDDEN
            } else {
                axum::http::StatusCode::BAD_REQUEST
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        })?;

    // Initialize kanban board metadata if created from a kanban template
    if body.template_key.starts_with("template_default_kanban") {
        if let Ok(board_id) = Uuid::parse_str(&object.object_id.to_string()) {
            let _ = state
                .kanban_service
                .initialize_board(board_id, user_id, tenant_id)
                .await;
        }
    }

    // Log object creation from template (audit trail)
    log_admin_action(
        &state.db_pool,
        user_id,
        "object.created.from_template",
        Some("template"),
        None,
        json!({
            "template_key": body.template_key,
            "object_id": object.object_id.to_string(),
            "path": object.path
        }),
    )
    .await;

    Ok(Json(object))
}
