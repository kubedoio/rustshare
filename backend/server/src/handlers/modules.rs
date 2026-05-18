//! User-facing module handlers.

use axum::{
    extract::{Path, State},
    Json,
};
use rustshare_core::domain::{CreateFromTemplateRequest, CreatedObject, Module};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::{
    handlers::{admin::log_admin_action, extractors::AuthenticatedUser, AppError},
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
) -> Result<Json<EnabledModulesResponse>, AppError> {
    let modules = state
        .module_service
        .list_enabled_modules(tenant_id, user_id)
        .await?;

    Ok(Json(EnabledModulesResponse { modules }))
}

pub async fn get_module(
    AuthenticatedUser { user_id, tenant_id }: AuthenticatedUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ModuleDetailResponse>, AppError> {
    let module = state.module_service.get_module(&key, tenant_id).await?;

    if !module.enabled {
        return Err(AppError::forbidden("Module disabled"));
    }

    let visible_modules = state
        .module_service
        .list_enabled_modules(tenant_id, user_id)
        .await?;

    if !visible_modules
        .iter()
        .any(|visible| visible.module_key == module.module_key)
    {
        return Err(AppError::forbidden("Access denied"));
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
) -> Result<Json<ModuleSummaryResponse>, AppError> {
    let summary = state
        .module_service
        .get_module_summary(&key, tenant_id, user_id)
        .await?;

    Ok(Json(ModuleSummaryResponse { summary }))
}

pub async fn create_from_template(
    AuthenticatedUser { user_id, tenant_id }: AuthenticatedUser,
    State(state): State<AppState>,
    Json(body): Json<CreateFromTemplateRequest>,
) -> Result<Json<CreatedObject>, AppError> {
    let object = state
        .template_service
        .create_from_template(
            &body.template_key,
            user_id,
            tenant_id,
            body.name,
            body.parent_folder_id,
        )
        .await?;

    // Initialize kanban board metadata if created from a kanban template
    let template = state
        .template_service
        .get_template(&body.template_key, tenant_id)
        .await?;

    if template.module_key == "kanban" {
        if let Ok(board_id) = Uuid::parse_str(&object.object_id.to_string()) {
            state
                .kanban_service
                .initialize_board(board_id, user_id, tenant_id, Some(template.module_config))
                .await
                .map_err(|e| {
                    tracing::error!("Failed to initialize kanban board: {}", e);
                    AppError::bad_request(format!("Board created but initialization failed: {}", e))
                })?;
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
