//! Installation-operator management of tenant-scoped organization-admin grants.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use serde_json::json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::{
    admin_conflict, admin_internal_error, admin_not_found, insert_scoped_admin_action,
    request_correlation_id,
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

#[derive(sqlx::FromRow)]
struct AdministratorRow {
    grant_id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    username: String,
    display_name: String,
    granted_by: Uuid,
    granted_at: chrono::DateTime<chrono::Utc>,
    disabled_at: Option<chrono::DateTime<chrono::Utc>>,
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
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM organization_administrators WHERE tenant_id = $1 AND revoked_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(db_error)?;

    let rows = sqlx::query_as::<_, AdministratorRow>(
        r#"SELECT oa.id AS grant_id, oa.tenant_id, oa.user_id, u.username, u.display_name,
                  oa.granted_by, oa.granted_at, u.disabled_at
           FROM organization_administrators oa
           JOIN users u ON u.id = oa.user_id AND u.tenant_id = oa.tenant_id
           WHERE oa.tenant_id = $1 AND oa.revoked_at IS NULL
           ORDER BY oa.granted_at DESC, oa.id DESC
           LIMIT $2 OFFSET $3"#,
    )
    .bind(tenant_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db_pool)
    .await
    .map_err(db_error)?;

    Ok(Json(PaginatedOrganizationAdministrators {
        administrators: rows.into_iter().map(Into::into).collect(),
        total,
        page: pagination.page,
        per_page: limit,
    }))
}

impl From<AdministratorRow> for OrganizationAdministrator {
    fn from(row: AdministratorRow) -> Self {
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
    responses((status = 201, description = "Administrator grant created", body = OrganizationAdministrator), (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse), (status = 403, description = "Installation operator required", body = crate::handlers::ErrorResponse), (status = 404, description = "Enabled principal not found in tenant", body = crate::handlers::ErrorResponse), (status = 409, description = "Self-grants are prohibited or an active grant already exists", body = crate::handlers::ErrorResponse))
)]
pub async fn grant_organization_administrator(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    headers: axum::http::HeaderMap,
    Path((tenant_id, user_id)): Path<(Uuid, Uuid)>,
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

    let row = sqlx::query_as::<_, AdministratorRow>(
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
        json!({ "grant_id": row.grant_id }),
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
    let mut tx = state.db_pool.begin().await.map_err(db_error)?;
    lock_tenant_admin_grants(&mut tx, tenant_id)
        .await
        .map_err(db_error)?;

    let active_grant = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM organization_administrators WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NULL FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("Active organization administrator grant not found"))?;

    if removal_would_leave_no_enabled_admin(&mut tx, tenant_id, user_id)
        .await
        .map_err(db_error)?
    {
        return Err(admin_conflict(
            "Grant a replacement enabled administrator before revoking the last one",
        ));
    }

    sqlx::query(
        "UPDATE organization_administrators
         SET revoked_by = $2, revoked_at = NOW(), revocation_source = 'operator'
         WHERE id = $1",
    )
    .bind(active_grant)
    .bind(actor_id)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        actor_id,
        "organization_admin.revoked",
        Some("organization_administrator"),
        Some(user_id),
        json!({ "grant_id": active_grant }),
        request_correlation_id(&headers),
    )
    .await
    .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

fn db_error(error: sqlx::Error) -> AppError {
    tracing::error!(
        ?error,
        "Organization administrator database operation failed"
    );
    admin_internal_error("Database error")
}

pub(crate) async fn removal_would_leave_no_enabled_admin(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT EXISTS(
               SELECT 1 FROM organization_administrators target
               JOIN users target_user
                 ON target_user.id = target.user_id AND target_user.tenant_id = target.tenant_id
               WHERE target.tenant_id = $1 AND target.user_id = $2
                 AND target.revoked_at IS NULL AND target_user.disabled_at IS NULL
           )
           AND NOT EXISTS(
               SELECT 1 FROM organization_administrators successor
               JOIN users successor_user
                 ON successor_user.id = successor.user_id AND successor_user.tenant_id = successor.tenant_id
               WHERE successor.tenant_id = $1 AND successor.user_id <> $2
                 AND successor.revoked_at IS NULL AND successor_user.disabled_at IS NULL
           )"#,
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await
}

pub(crate) async fn lock_tenant_admin_grants(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(tenant_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Revoke grants when the installation operator disables or deletes the principal.
pub(crate) async fn revoke_for_lifecycle(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    actor_id: Option<Uuid>,
    user_id: Uuid,
    source: &str,
    correlation_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    let ids = sqlx::query_scalar::<_, Uuid>(
        "UPDATE organization_administrators
         SET revoked_by = $3, revoked_at = NOW(), revocation_source = $4
         WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NULL RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(actor_id)
    .bind(source)
    .fetch_all(&mut **tx)
    .await?;

    for grant_id in ids {
        if let Some(actor_id) = actor_id {
            insert_scoped_admin_action(
                tx,
                tenant_id,
                actor_id,
                "organization_admin.revoked",
                Some("organization_administrator"),
                Some(user_id),
                json!({ "grant_id": grant_id }),
                correlation_id,
            )
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO admin_actions
                    (tenant_id, actor_id, actor_principal_id, action_type, target_type,
                     target_id, detail, correlation_id, outcome)
                 VALUES ($1, NULL, NULL, 'organization_admin.revoked',
                         'organization_administrator', $2, $3, NULL, 'success')",
            )
            .bind(tenant_id)
            .bind(user_id)
            .bind(json!({ "actor_kind": source, "grant_id": grant_id }))
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

pub(crate) async fn revoke_for_scim_lifecycle(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    revoke_for_lifecycle(tx, tenant_id, None, user_id, "scim", None).await
}
