//! Admin module management handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use rustshare_core::domain::Module;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{admin_bad_request, admin_internal_error, admin_not_found, log_admin_action};
use crate::{handlers::AdminUser, state::AppState};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ModuleListResponse {
    pub modules: Vec<Module>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateModuleRequest {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub permissions: Option<serde_json::Value>,
    pub ai_indexing: Option<serde_json::Value>,
    pub audit: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn list_modules(
    AdminUser { user_id: _ }: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<ModuleListResponse>, axum::response::Response> {
    let modules = state
        .module_service
        .list_modules(state.default_tenant_id)
        .await
        .map_err(|e| admin_internal_error(e.to_string()))?;

    Ok(Json(ModuleListResponse { modules }))
}

pub async fn get_module(
    AdminUser { user_id: _ }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Module>, axum::response::Response> {
    let module = state
        .module_service
        .get_module(&key, state.default_tenant_id)
        .await
        .map_err(|e| match e.to_string().contains("not found") {
            true => admin_not_found(e.to_string()),
            false => admin_internal_error(e.to_string()),
        })?;

    Ok(Json(module))
}

pub async fn enable_module(
    AdminUser { user_id }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Module>, axum::response::Response> {
    let module = state
        .module_service
        .enable_module(&key, user_id, state.default_tenant_id)
        .await
        .map_err(|e| match e.to_string().contains("not found") {
            true => admin_not_found(e.to_string()),
            false => admin_internal_error(e.to_string()),
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "module.enabled",
        Some("module"),
        Some(module.id),
        json!({"module_key": key}),
    )
    .await;

    Ok(Json(module))
}

pub async fn disable_module(
    AdminUser { user_id }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Module>, axum::response::Response> {
    let module = state
        .module_service
        .disable_module(&key, user_id, state.default_tenant_id)
        .await
        .map_err(|e| match e.to_string().contains("not found") {
            true => admin_not_found(e.to_string()),
            false => admin_internal_error(e.to_string()),
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "module.disabled",
        Some("module"),
        Some(module.id),
        json!({"module_key": key}),
    )
    .await;

    Ok(Json(module))
}

pub async fn update_module(
    AdminUser { user_id }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<UpdateModuleRequest>,
) -> Result<Json<Module>, axum::response::Response> {
    let module = state
        .module_service
        .update_module(
            &key,
            body.display_name,
            body.description,
            body.icon,
            body.permissions,
            body.ai_indexing,
            body.audit,
            state.default_tenant_id,
        )
        .await
        .map_err(|e| match e.to_string().contains("not found") {
            true => admin_not_found(e.to_string()),
            false => admin_bad_request(e.to_string()),
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "module.updated",
        Some("module"),
        Some(module.id),
        json!({"module_key": key}),
    )
    .await;

    Ok(Json(module))
}
