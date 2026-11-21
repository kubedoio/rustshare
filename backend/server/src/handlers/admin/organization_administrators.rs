//! Installation-operator management of tenant-scoped organization-admin grants.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{
    admin_bad_request, admin_conflict, admin_internal_error, admin_not_found,
    insert_scoped_admin_action, request_correlation_id,
};
pub(crate) use crate::services::organization_administrators::{
    list_active_for_tenant, lock_tenant_admin_grants, removal_would_leave_no_enabled_admin,
    revoke_for_lifecycle, revoke_operator_grant, OperatorRevokeOutcome,
    OrganizationAdministratorRow,
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

    let mut tx = state.db_pool.begin().await.map_err(db_error)?;
    // Serialize grant classification within a tenant so bootstrap/recovery audit
    // events cannot race across distinct target users.
    lock_tenant_admin_grants(&mut tx, tenant_id)
        .await
        .map_err(db_error)?;

    let target = sqlx::query_as::<_, (Uuid, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT tenant_id, disabled_at FROM users WHERE id = $1 FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    if !target.is_some_and(|(target_tenant, disabled_at)| {
        target_tenant == tenant_id && disabled_at.is_none()
    }) {
        return Err(admin_not_found("Enabled principal not found in tenant"));
    }

    let target_has_active_grant: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM organization_administrators
         WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NULL)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_error)?;
    if target_has_active_grant {
        return Err(admin_conflict(
            "An active organization administrator grant already exists",
        ));
    }

    let history_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM organization_administrators WHERE tenant_id = $1)",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_error)?;
    let active_admin_exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
               SELECT 1 FROM organization_administrators oa
               JOIN users u ON u.id = oa.user_id AND u.tenant_id = oa.tenant_id
               WHERE oa.tenant_id = $1 AND oa.revoked_at IS NULL AND u.disabled_at IS NULL
           )"#,
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_error)?;
    let action_type = if !history_exists {
        "organization_admin.bootstrap"
    } else if !active_admin_exists {
        "organization_admin.recovered"
    } else {
        "organization_admin.granted"
    };

    if action_type == "organization_admin.recovered" && request.recovery_reason.is_none() {
        return Err(admin_bad_request(
            "A recovery reason is required when restoring administration",
        ));
    }
    if action_type != "organization_admin.recovered" && request.recovery_reason.is_some() {
        return Err(admin_bad_request(
            "A recovery reason is only valid when restoring administration",
        ));
    }

    let row = sqlx::query_as::<_, OrganizationAdministratorRow>(
        r#"INSERT INTO organization_administrators (tenant_id, user_id, granted_by)
           VALUES ($1, $2, $3)
           RETURNING id AS grant_id, tenant_id, user_id,
             (SELECT username FROM users WHERE id = $2) AS username,
             (SELECT display_name FROM users WHERE id = $2) AS display_name,
             granted_by, granted_at, NULL::timestamptz AS disabled_at"#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(actor_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| {
        if let sqlx::Error::Database(db) = &error {
            if db.constraint() == Some("organization_administrators_one_active_grant") {
                return admin_conflict("An active organization administrator grant already exists");
            }
        }
        db_error(error)
    })?;

    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        actor_id,
        action_type,
        Some("organization_administrator"),
        Some(user_id),
        if let Some(recovery_reason) = request.recovery_reason {
            json!({ "grant_id": row.grant_id, "recovery_reason": recovery_reason })
        } else {
            json!({ "grant_id": row.grant_id })
        },
        request_correlation_id(&headers),
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;

    Ok((StatusCode::CREATED, Json(row.into())))
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
