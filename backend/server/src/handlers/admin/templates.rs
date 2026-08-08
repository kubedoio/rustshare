//! Admin template management handlers.

use axum::{
    extract::{Path, State},
    Json,
};

use crate::{
    handlers::{admin::log_admin_action, extractors::AdminUser, AppError},
    services::template_service::{CreateTemplateRequest, UpdateTemplateRequest},
    state::AppState,
};
use rustshare_core::domain::Template;

#[utoipa::path(
    get,
    path = "/api/v1/admin/templates",
    tag = "Admin",
    responses(
        (status = 200, description = "Success", body = Vec<Template>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_templates(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Template>>, AppError> {
    let templates = state
        .template_service
        .list_templates(state.default_tenant_id)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(Json(templates))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/applications/{key}/templates",
    tag = "Admin",
    params(("application_id" = String, Path, description = "ApplicationConfig Key")),
    responses(
        (status = 200, description = "Success", body = Vec<Template>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_templates_by_application(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(application_id): Path<String>,
) -> Result<Json<Vec<Template>>, AppError> {
    let templates = state
        .template_service
        .list_templates_by_application(&application_id, state.default_tenant_id)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(Json(templates))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/templates/{key}",
    tag = "Admin",
    params(("key" = String, Path, description = "Key")),
    responses(
        (status = 200, description = "Success", body = Template),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_template(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Template>, AppError> {
    let template = state
        .template_service
        .get_template(&key, state.default_tenant_id)
        .await?;

    Ok(Json(template))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/templates",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_template(
    AdminUser { user_id, .. }: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTemplateRequest>,
) -> Result<Json<Template>, AppError> {
    let template = state
        .template_service
        .create_template(body.clone(), user_id, state.default_tenant_id)
        .await?;

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

#[utoipa::path(
    put,
    path = "/api/v1/admin/templates/{key}",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_template(
    AdminUser { user_id, .. }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<UpdateTemplateRequest>,
) -> Result<Json<Template>, AppError> {
    let template = state
        .template_service
        .update_template(&key, body.clone(), state.default_tenant_id)
        .await?;

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

#[utoipa::path(
    delete,
    path = "/api/v1/admin/templates/{key}",
    tag = "Admin",
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn delete_template(
    AdminUser { user_id, .. }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<axum::http::StatusCode, AppError> {
    let template = state
        .template_service
        .get_template(&key, state.default_tenant_id)
        .await?;

    state
        .template_service
        .delete_template(&key, state.default_tenant_id)
        .await?;

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

#[utoipa::path(
    post,
    path = "/api/v1/admin/templates/{key}/duplicate",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn duplicate_template(
    AdminUser { user_id, .. }: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Template>, AppError> {
    let template = state
        .template_service
        .get_template(&key, state.default_tenant_id)
        .await?;

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
        application_id: template.application_id,
        description: template.description,
        ui_config: Some(template.ui_config),
        folder_structure: serde_json::from_value(template.folder_structure).unwrap_or_default(),
        default_files: serde_json::from_value(template.default_files).unwrap_or_default(),
        metadata_schema: template.metadata_schema,
        renderer: template.renderer,
        visibility_policy: template.visibility_policy,
        application_config: Some(template.application_config),
    };

    let new_template = state
        .template_service
        .create_template(request.clone(), user_id, state.default_tenant_id)
        .await?;

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
