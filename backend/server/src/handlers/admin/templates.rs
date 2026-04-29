//! Admin template management handlers.

use axum::{
    extract::{Path, State},
    Json,
};
use rustshare_core::domain::Template;
use serde::Serialize;
use serde_json::json;

use super::{admin_bad_request, admin_internal_error, admin_not_found, log_admin_action};
use crate::{handlers::AdminUser, state::AppState};
use crate::services::template_service::{CreateTemplateRequest, UpdateTemplateRequest};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct TemplateListResponse {
    pub templates: Vec<Template>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn list_templates(
    AdminUser { user_id: _ }: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<TemplateListResponse>, axum::response::Response> {
    let templates = state
        .template_service
        .list_templates(state.default_tenant_id)
        .await
        .map_err(|e| admin_internal_error(e.to_string()))?;

    Ok(Json(TemplateListResponse { templates }))
}

pub async fn get_template(
    AdminUser { user_id: _ }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Template>, axum::response::Response> {
    let template = state
        .template_service
        .get_template(&key, state.default_tenant_id)
        .await
        .map_err(|e| match e.to_string().contains("not found") {
            true => admin_not_found(e.to_string()),
            false => admin_internal_error(e.to_string()),
        })?;

    Ok(Json(template))
}

pub async fn create_template(
    AdminUser { user_id }: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTemplateRequest>,
) -> Result<Json<Template>, axum::response::Response> {
    let template = state
        .template_service
        .create_template(body, user_id, state.default_tenant_id)
        .await
        .map_err(|e| match e.to_string().contains("already exists") {
            true => {
                let err: axum::response::Response = (
                    axum::http::StatusCode::CONFLICT,
                    Json(crate::handlers::ErrorResponse::new(e.to_string())),
                )
                    .into_response();
                err
            }
            false => admin_bad_request(e.to_string()),
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "template.created",
        Some("template"),
        Some(template.id),
        json!({"template_key": template.template_key, "name": template.name}),
    )
    .await;

    Ok(Json(template))
}

pub async fn update_template(
    AdminUser { user_id: _ }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<UpdateTemplateRequest>,
) -> Result<Json<Template>, axum::response::Response> {
    let template = state
        .template_service
        .update_template(&key, body, state.default_tenant_id)
        .await
        .map_err(|e| match e.to_string().contains("not found") {
            true => admin_not_found(e.to_string()),
            false => admin_bad_request(e.to_string()),
        })?;

    Ok(Json(template))
}

pub async fn delete_template(
    AdminUser { user_id }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<axum::http::StatusCode, axum::response::Response> {
    state
        .template_service
        .delete_template(&key, state.default_tenant_id)
        .await
        .map_err(|e| match e.to_string().contains("not found") {
            true => admin_not_found(e.to_string()),
            false => admin_bad_request(e.to_string()),
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "template.deleted",
        Some("template"),
        None,
        json!({"template_key": key}),
    )
    .await;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
