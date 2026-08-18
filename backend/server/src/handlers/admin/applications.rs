//! Admin Application management handlers.

use axum::{
    extract::{Path, State},
    Json,
};
use rustshare_core::domain::{ApplicationConfig, TenantId, WorkspaceId};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{admin_bad_request, admin_internal_error, admin_not_found, log_admin_action};
use crate::config::ChatProvisioningMode;
use crate::services::application_service::UpdateApplicationInput;
use crate::services::chat_bootstrap::ChatBootstrapError;
use crate::{
    handlers::{AdminUser, AppError, AuthenticatedUser},
    state::{AppState, ApplicationState},
};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ApplicationListResponse {
    pub applications: Vec<ApplicationConfig>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateApplicationRequest {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub root_path: Option<String>,
    pub renderer: Option<String>,
    pub default_template: Option<Option<String>>,
    pub permissions: Option<serde_json::Value>,
    pub ai_indexing: Option<serde_json::Value>,
    pub audit: Option<serde_json::Value>,
    pub ui_config: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/admin/applications",
    tag = "Applications",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_applications(
    _admin: AdminUser,
    AuthenticatedUser { tenant_id, .. }: AuthenticatedUser,
    State(state): State<ApplicationState>,
) -> Result<Json<ApplicationListResponse>, AppError> {
    let applications = state
        .application_service
        .list_applications(tenant_id)
        .await
        .map_err(|e| admin_internal_error(e.to_string()))?;

    Ok(Json(ApplicationListResponse { applications }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/applications/{key}",
    tag = "Applications",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_application(
    _admin: AdminUser,
    AuthenticatedUser { tenant_id, .. }: AuthenticatedUser,
    State(state): State<ApplicationState>,
    Path(key): Path<String>,
) -> Result<Json<ApplicationConfig>, AppError> {
    let application = state
        .application_service
        .get_application(&key, tenant_id)
        .await
        .map_err(|e| match e.to_string().contains("not found") {
            true => admin_not_found(e.to_string()),
            false => admin_internal_error(e.to_string()),
        })?;

    Ok(Json(application))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/applications/{key}/enable",
    tag = "Applications",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn enable_application(
    AdminUser { user_id }: AdminUser,
    AuthenticatedUser { tenant_id, .. }: AuthenticatedUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ApplicationConfig>, AppError> {
    let application = state
        .application_service
        .enable_application(&key, user_id, tenant_id)
        .await
        .map_err(|e| match e.to_string().contains("not found") {
            true => admin_not_found(e.to_string()),
            false => admin_internal_error(e.to_string()),
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "application.enabled",
        Some("application"),
        Some(application.id),
        json!({"application_id": key}),
    )
    .await;

    // Zero-config bootstrap (ADR-0036): in auto mode, enabling Chat
    // provisions the deployment Buzz community immediately. A failure is
    // logged and leaves Chat safely unconfigured — the admin retries via
    // POST .../chat/workspaces/{id}/provision.
    if key == crate::authz::chat_owner::CHAT_APPLICATION_ID
        && state.chat_provisioning == ChatProvisioningMode::Auto
    {
        if let Some(bootstrap) = &state.chat_bootstrap {
            if let Err(error) = bootstrap
                .provision(TenantId(tenant_id), WorkspaceId(tenant_id))
                .await
            {
                match error {
                    ChatBootstrapError::ServiceIdentityRejected => {
                        tracing::warn!(
                            "chat auto-provisioning failed: Buzz rejected Elembra's service identity; chat remains unconfigured"
                        );
                    }
                    ChatBootstrapError::Discovery(_) => {
                        tracing::warn!(
                            %error,
                            "chat auto-provisioning failed: relay discovery error; chat remains unconfigured"
                        );
                    }
                    ChatBootstrapError::CommunityInUse { .. }
                    | ChatBootstrapError::CommunityMismatch { .. } => {
                        tracing::warn!(
                            %error,
                            "chat auto-provisioning failed: mapping conflict; chat remains unconfigured"
                        );
                    }
                    _ => {
                        tracing::warn!(%error, "chat auto-provisioning failed; chat remains unconfigured");
                    }
                }
            }
        }
    }

    Ok(Json(application))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/applications/{key}/disable",
    tag = "Applications",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn disable_application(
    AdminUser { user_id }: AdminUser,
    AuthenticatedUser { tenant_id, .. }: AuthenticatedUser,
    State(state): State<ApplicationState>,
    Path(key): Path<String>,
) -> Result<Json<ApplicationConfig>, AppError> {
    let application = state
        .application_service
        .disable_application(&key, user_id, tenant_id)
        .await
        .map_err(|e| match e.to_string().contains("not found") {
            true => admin_not_found(e.to_string()),
            false => admin_internal_error(e.to_string()),
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "application.disabled",
        Some("application"),
        Some(application.id),
        json!({"application_id": key}),
    )
    .await;

    Ok(Json(application))
}

#[utoipa::path(
    patch,
    path = "/api/v1/admin/applications/{key}",
    tag = "Applications",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_application(
    AdminUser { user_id }: AdminUser,
    AuthenticatedUser { tenant_id, .. }: AuthenticatedUser,
    State(state): State<ApplicationState>,
    Path(key): Path<String>,
    Json(body): Json<UpdateApplicationRequest>,
) -> Result<Json<ApplicationConfig>, AppError> {
    let application = state
        .application_service
        .update_application(
            &key,
            UpdateApplicationInput {
                display_name: body.display_name,
                description: body.description,
                icon: body.icon,
                root_path: body.root_path,
                renderer: body.renderer,
                default_template: body.default_template,
                permissions: body.permissions,
                ai_indexing: body.ai_indexing,
                audit: body.audit,
                ui_config: body.ui_config,
            },
            tenant_id,
        )
        .await
        .map_err(|e| match e.to_string().contains("not found") {
            true => admin_not_found(e.to_string()),
            false => admin_bad_request(e.to_string()),
        })?;

    log_admin_action(
        &state.db_pool,
        user_id,
        "application.updated",
        Some("application"),
        Some(application.id),
        json!({"application_id": key}),
    )
    .await;

    Ok(Json(application))
}
