//! User-facing module handlers.

use axum::{
    extract::{Path, State},
    Json,
};
use rustshare_core::domain::{CreateFromTemplateRequest, CreatedObject, Module};
use serde::Serialize;
use serde_json::json;

use crate::{
    handlers::{extractors::AuthenticatedUser, ErrorResponse},
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
    #[serde(flatten)]
    pub module: Module,
    pub available: bool,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn list_enabled_modules(
    AuthenticatedUser { user_id: _, tenant_id }: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<Json<EnabledModulesResponse>, axum::response::Response> {
    let modules = state
        .module_service
        .list_enabled_modules(tenant_id)
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
    AuthenticatedUser { user_id: _, tenant_id }: AuthenticatedUser,
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
            axum::http::StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Module not found".to_string())),
        )
            .into_response());
    }

    Ok(Json(ModuleDetailResponse {
        module,
        available: true,
    }))
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

    // Log admin action if audit is enabled for the module
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

/// Helper to log admin action for object creation.
/// Errors are logged as warnings — audit failures must not block the operation.
async fn log_admin_action(
    pool: &sqlx::PgPool,
    actor_id: uuid::Uuid,
    action_type: &str,
    target_type: Option<&str>,
    target_id: Option<uuid::Uuid>,
    detail: serde_json::Value,
) {
    let result = sqlx::query(
        r#"
        INSERT INTO admin_actions (actor_id, action_type, target_type, target_id, detail)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(actor_id)
    .bind(action_type)
    .bind(target_type)
    .bind(target_id)
    .bind(detail)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::warn!(
            actor_id = %actor_id,
            action_type = action_type,
            target_id = ?target_id,
            "Failed to log admin action: {:?}",
            e
        );
    }
}
