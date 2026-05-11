//! Admin template management handlers.

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};

use crate::{
    handlers::{admin::log_admin_action, extractors::AdminUser, ErrorResponse},
    services::template_service::{CreateTemplateRequest, UpdateTemplateRequest},
    state::AppState,
};
use rustshare_core::domain::Template;

pub async fn list_templates(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Template>>, axum::response::Response> {
    let templates = state
        .template_service
        .list_templates(state.default_tenant_id)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response()
        })?;

    Ok(Json(templates))
}

pub async fn list_templates_by_module(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(module_key): Path<String>,
) -> Result<Json<Vec<Template>>, axum::response::Response> {
    let templates = state
        .template_service
        .list_templates_by_module(&module_key, state.default_tenant_id)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response()
        })?;

    Ok(Json(templates))
}

pub async fn get_template(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Template>, axum::response::Response> {
    let template = state
        .template_service
        .get_template(&key, state.default_tenant_id)
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("not found") {
                axum::http::StatusCode::NOT_FOUND
            } else {
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(ErrorResponse::new(e.to_string()))).into_response()
        })?;

    Ok(Json(template))
}

pub async fn create_template(
    AdminUser { user_id, .. }: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTemplateRequest>,
) -> Result<Json<Template>, axum::response::Response> {
    let template = state
        .template_service
        .create_template(body.clone(), user_id, state.default_tenant_id)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response()
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "template.created",
        Some("template"),
        Some(template.id),
        serde_json::to_value(&body).unwrap_or_default(),
    )
    .await;

    Ok(Json(template))
}

pub async fn update_template(
    AdminUser { user_id, .. }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<UpdateTemplateRequest>,
) -> Result<Json<Template>, axum::response::Response> {
    let template = state
        .template_service
        .update_template(&key, body.clone(), state.default_tenant_id)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response()
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "template.updated",
        Some("template"),
        Some(template.id),
        serde_json::to_value(&body).unwrap_or_default(),
    )
    .await;

    Ok(Json(template))
}

pub async fn delete_template(
    AdminUser { user_id, .. }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<axum::http::StatusCode, axum::response::Response> {
    let template = state
        .template_service
        .get_template(&key, state.default_tenant_id)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response()
        })?;

    state
        .template_service
        .delete_template(&key, state.default_tenant_id)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response()
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "template.deleted",
        Some("template"),
        Some(template.id),
        serde_json::json!({ "key": key }),
    )
    .await;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn duplicate_template(
    AdminUser { user_id, .. }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Template>, axum::response::Response> {
    let template = state
        .template_service
        .get_template(&key, state.default_tenant_id)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response()
        })?;

    let mut new_key = format!("{}_copy", template.template_key);
    let mut i = 1;
    while state
        .template_service
        .get_template(&new_key, state.default_tenant_id)
        .await
        .is_ok()
    {
        new_key = format!("{}_copy_{}", template.template_key, i);
        i += 1;
    }

    let request = CreateTemplateRequest {
        template_key: new_key,
        name: format!("{} (Copy)", template.name),
        module_key: template.module_key,
        description: template.description,
        ui_config: Some(template.ui_config),
        folder_structure: serde_json::from_value(template.folder_structure).unwrap_or_default(),
        default_files: serde_json::from_value(template.default_files).unwrap_or_default(),
        metadata_schema: template.metadata_schema,
        renderer: template.renderer,
        visibility_policy: template.visibility_policy,
        module_config: Some(template.module_config),
    };

    let new_template = state
        .template_service
        .create_template(request.clone(), user_id, state.default_tenant_id)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e.to_string())),
            )
                .into_response()
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "template.duplicated",
        Some("template"),
        Some(template.id),
        serde_json::json!({ "original_key": key, "new_key": new_template.template_key, "new_id": new_template.id }),
    )
    .await;

    Ok(Json(new_template))
}
