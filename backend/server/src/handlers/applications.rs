//! User-facing Application handlers.

use axum::{
    extract::{Path, State},
    Json,
};
use rustshare_core::domain::{ApplicationConfig, CreateFromTemplateRequest, CreatedObject};
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EnabledApplicationsResponse {
    pub applications: Vec<ApplicationConfig>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ApplicationDetailResponse {
    pub application: ApplicationConfig,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/applications",
    tag = "Applications",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_enabled_applications(
    AuthenticatedUser { user_id, tenant_id }: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<Json<EnabledApplicationsResponse>, AppError> {
    let applications = state
        .application_service
        .list_enabled_applications(tenant_id, user_id)
        .await?;

    Ok(Json(EnabledApplicationsResponse { applications }))
}

#[utoipa::path(
    get,
    path = "/api/v1/applications/{key}",
    tag = "Applications",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_application(
    AuthenticatedUser { user_id, tenant_id }: AuthenticatedUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ApplicationDetailResponse>, AppError> {
    let application = state
        .application_service
        .get_application(&key, tenant_id)
        .await?;

    if !application.enabled {
        return Err(AppError::forbidden("Application disabled"));
    }

    let visible_applications = state
        .application_service
        .list_enabled_applications(tenant_id, user_id)
        .await?;

    if !visible_applications
        .iter()
        .any(|visible| visible.application_id == application.application_id)
    {
        return Err(AppError::forbidden("Access denied"));
    }

    Ok(Json(ApplicationDetailResponse { application }))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ApplicationSummaryResponse {
    pub summary: crate::services::application_service::ApplicationSummary,
}

#[utoipa::path(
    get,
    path = "/api/v1/applications/{key}/summary",
    tag = "Applications",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_application_summary(
    AuthenticatedUser { user_id, tenant_id }: AuthenticatedUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ApplicationSummaryResponse>, AppError> {
    let summary = state
        .application_service
        .get_application_summary(&key, tenant_id, user_id)
        .await?;

    Ok(Json(ApplicationSummaryResponse { summary }))
}

#[utoipa::path(
    post,
    path = "/api/v1/applications/from-template",
    tag = "Applications",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
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

    if template.application_id == "kanban" {
        if let Ok(board_id) = Uuid::parse_str(&object.object_id.to_string()) {
            state
                .kanban_service
                .initialize_board(
                    board_id,
                    user_id,
                    tenant_id,
                    Some(template.application_config),
                )
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
