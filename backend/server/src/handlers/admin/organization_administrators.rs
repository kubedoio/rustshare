//! Installation-operator management of tenant-scoped organization-admin grants.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    admin_bad_request, admin_conflict, admin_internal_error, admin_not_found,
    request_correlation_id,
};
pub(crate) use crate::services::organization_administrators::{
    grant_operator_organization_admin, list_active_for_tenant, lock_tenant_admin_grants,
    removal_would_leave_no_enabled_admin, revoke_for_lifecycle, revoke_operator_grant,
    OperatorGrantOutcome, OperatorRevokeOutcome, OrganizationAdministratorRow,
};
use crate::{
    handlers::{AdminUser, AppError, PaginationQuery},
    AppState,
};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrganizationAdministrator {
    pub grant_id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub granted_by: Uuid,
    pub granted_at: chrono::DateTime<chrono::Utc>,
    pub disabled_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PaginatedOrganizationAdministrators {
    pub administrators: Vec<OrganizationAdministrator>,
    pub total: i64,
    pub page: u32,
    pub per_page: i64,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationAdministratorRecoveryReason {
    AdministratorUnavailable,
    SecurityIncident,
    AdministrativeContinuity,
}

impl OrganizationAdministratorRecoveryReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::AdministratorUnavailable => "administrator_unavailable",
            Self::SecurityIncident => "security_incident",
            Self::AdministrativeContinuity => "administrative_continuity",
        }
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct OrganizationAdministratorGrantRequest {
    /// Required only when recovering a tenant with grant history and no active administrator.
    #[serde(default)]
    pub recovery_reason: Option<OrganizationAdministratorRecoveryReason>,
}

/// List current grants for one tenant. This endpoint remains installation-operator-only.
#[utoipa::path(
    get,
    path = "/api/v1/admin/organizations/{tenant_id}/administrators",
    tag = "Admin",
    params(
        ("tenant_id" = Uuid, Path, description = "Tenant UUID"),
        ("page" = Option<u32>, Query, description = "Page number, starting at 1"),
        ("per_page" = Option<u32>, Query, description = "Page size, capped at 100")
    ),
    responses((status = 200, description = "Current tenant administrator grants", body = PaginatedOrganizationAdministrators), (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse), (status = 403, description = "Installation operator required", body = crate::handlers::ErrorResponse))
)]
pub async fn list_organization_administrators(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(tenant_id): Path<Uuid>,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<PaginatedOrganizationAdministrators>, AppError> {
    let limit = pagination.limit();
    let offset = pagination.offset();
    let (total, rows) = list_active_for_tenant(&state.db_pool, tenant_id, limit, offset)
        .await
        .map_err(db_error)?;

    Ok(Json(PaginatedOrganizationAdministrators {
        administrators: rows.into_iter().map(Into::into).collect(),
        total,
        page: pagination.page,
        per_page: limit,
    }))
}

impl From<OrganizationAdministratorRow> for OrganizationAdministrator {
    fn from(row: OrganizationAdministratorRow) -> Self {
        Self {
            grant_id: row.grant_id,
            tenant_id: row.tenant_id,
            user_id: row.user_id,
            username: row.username,
            display_name: row.display_name,
            granted_by: row.granted_by,
            granted_at: row.granted_at,
            disabled_at: row.disabled_at,
        }
    }
}

/// Grant or re-grant administration to an enabled principal in the selected tenant.
#[utoipa::path(
    post,
    path = "/api/v1/admin/organizations/{tenant_id}/administrators/{user_id}",
    tag = "Admin",
    params(("tenant_id" = Uuid, Path, description = "Tenant UUID"), ("user_id" = Uuid, Path, description = "Principal UUID")),
    request_body = OrganizationAdministratorGrantRequest,
    responses((status = 201, description = "Administrator grant created", body = OrganizationAdministrator), (status = 400, description = "Recovery reason is required for recovery grants and forbidden otherwise", body = crate::handlers::ErrorResponse), (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse), (status = 403, description = "Installation operator required", body = crate::handlers::ErrorResponse), (status = 404, description = "Enabled principal not found in tenant", body = crate::handlers::ErrorResponse), (status = 409, description = "Self-grants are prohibited or an active grant already exists", body = crate::handlers::ErrorResponse))
)]
pub async fn grant_organization_administrator(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    headers: axum::http::HeaderMap,
    Path((tenant_id, user_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<OrganizationAdministratorGrantRequest>,
) -> Result<(StatusCode, Json<OrganizationAdministrator>), AppError> {
    if actor_id == user_id {
        return Err(admin_conflict(
            "Installation operators cannot grant themselves organization administration",
        ));
    }

    match grant_operator_organization_admin(
        &state.db_pool,
        tenant_id,
        actor_id,
        user_id,
        request
            .recovery_reason
            .map(OrganizationAdministratorRecoveryReason::as_str),
        request_correlation_id(&headers),
    )
    .await
    .map_err(db_error)?
    {
        OperatorGrantOutcome::Created(row) => Ok((StatusCode::CREATED, Json(row.into()))),
        OperatorGrantOutcome::PrincipalUnavailable => {
            Err(admin_not_found("Enabled principal not found in tenant"))
        }
        OperatorGrantOutcome::ActiveGrantExists => Err(admin_conflict(
            "An active organization administrator grant already exists",
        )),
        OperatorGrantOutcome::RecoveryReasonRequired => Err(admin_bad_request(
            "A recovery reason is required when restoring administration",
        )),
        OperatorGrantOutcome::RecoveryReasonNotApplicable => Err(admin_bad_request(
            "A recovery reason is only valid when restoring administration",
        )),
    }
}

/// Revoke a grant while preserving its history. The final enabled administrator
/// can only be replaced by granting a successor first.
#[utoipa::path(
    delete,
    path = "/api/v1/admin/organizations/{tenant_id}/administrators/{user_id}",
    tag = "Admin",
    params(("tenant_id" = Uuid, Path, description = "Tenant UUID"), ("user_id" = Uuid, Path, description = "Principal UUID")),
    responses((status = 204, description = "Grant revoked"), (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse), (status = 403, description = "Installation operator required", body = crate::handlers::ErrorResponse), (status = 404, description = "Active grant not found", body = crate::handlers::ErrorResponse), (status = 409, description = "Cannot revoke the last enabled administrator", body = crate::handlers::ErrorResponse))
)]
pub async fn revoke_organization_administrator(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    headers: axum::http::HeaderMap,
    Path((tenant_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    match revoke_operator_grant(
        &state.db_pool,
        tenant_id,
        actor_id,
        user_id,
        request_correlation_id(&headers),
    )
    .await
    .map_err(db_error)?
    {
        OperatorRevokeOutcome::Revoked => Ok(StatusCode::NO_CONTENT),
        OperatorRevokeOutcome::NotFound => Err(admin_not_found(
            "Active organization administrator grant not found",
        )),
        OperatorRevokeOutcome::LastEnabledAdmin => Err(admin_conflict(
            "Grant a replacement enabled administrator before revoking the last one",
        )),
    }
}

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(
        ?error,
        "Organization administrator database operation failed"
    );
    admin_internal_error("Database error")
}
