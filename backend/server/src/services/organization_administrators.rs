//! Tenant-scoped organization-administrator grant persistence.

use crate::services::admin_audit_projection::project_audit_detail;
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub(crate) struct OrganizationAdministratorRow {
    pub(crate) grant_id: Uuid,
    pub(crate) tenant_id: Uuid,
    pub(crate) user_id: Uuid,
    pub(crate) username: String,
    pub(crate) display_name: String,
    pub(crate) granted_by: Uuid,
    pub(crate) granted_at: chrono::DateTime<chrono::Utc>,
    pub(crate) disabled_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub(crate) enum OperatorGrantOutcome {
    Created(OrganizationAdministratorRow),
    PrincipalUnavailable,
    ActiveGrantExists,
    RecoveryReasonRequired,
    RecoveryReasonNotApplicable,
}

pub(crate) async fn list_active_for_tenant(
    pool: &PgPool,
    tenant_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(i64, Vec<OrganizationAdministratorRow>), sqlx::Error> {
    let total = sqlx::query_scalar(
        "SELECT COUNT(*) FROM organization_administrators WHERE tenant_id = $1 AND revoked_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query_as::<_, OrganizationAdministratorRow>(
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
    .fetch_all(pool)
    .await?;

    Ok((total, rows))
}

pub(crate) enum OperatorRevokeOutcome {
    Revoked,
    NotFound,
    LastEnabledAdmin,
}

/// Insert a tenant-attributable success record with the mutation in one transaction.
#[allow(clippy::too_many_arguments)]
pub async fn insert_scoped_admin_action(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    actor_id: Uuid,
    action_type: &str,
    target_type: Option<&str>,
    target_id: Option<Uuid>,
    detail: serde_json::Value,
    correlation_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    let detail = project_audit_detail("admin_action", action_type, &detail);
    sqlx::query(
        "INSERT INTO admin_actions
            (tenant_id, actor_id, actor_principal_id, action_type, target_type, target_id,
             detail, correlation_id, outcome)
         VALUES ($1, $2, $2, $3, $4, $5, $6, $7, 'success')",
    )
    .bind(tenant_id)
    .bind(actor_id)
    .bind(action_type)
    .bind(target_type)
    .bind(target_id)
    .bind(detail)
    .bind(correlation_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
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

/// Serialize a tenant-admin-authorized mutation against grant revocation and
/// confirm the actor's grant is still active inside that mutation transaction.
pub(crate) async fn lock_and_check_active_grant(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sqlx::Error> {
    lock_tenant_admin_grants(tx, tenant_id).await?;
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM organization_administrators oa
            JOIN users u ON u.id=oa.user_id AND u.tenant_id=oa.tenant_id
            WHERE oa.tenant_id=$1 AND oa.user_id=$2
              AND oa.revoked_at IS NULL AND u.disabled_at IS NULL
        )",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await
}

pub(crate) async fn grant_operator_organization_admin(
    pool: &PgPool,
    tenant_id: Uuid,
    actor_id: Uuid,
    user_id: Uuid,
    recovery_reason: Option<&str>,
    correlation_id: Option<&str>,
) -> Result<OperatorGrantOutcome, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // Serialize bootstrap and recovery classification across different targets.
    lock_tenant_admin_grants(&mut tx, tenant_id).await?;

    let target = sqlx::query_as::<_, (Uuid, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT tenant_id, disabled_at FROM users WHERE id = $1 FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    if !target.is_some_and(|(target_tenant, disabled_at)| {
        target_tenant == tenant_id && disabled_at.is_none()
    }) {
        return Ok(OperatorGrantOutcome::PrincipalUnavailable);
    }

    let target_has_active_grant: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM organization_administrators
         WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NULL)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    if target_has_active_grant {
        return Ok(OperatorGrantOutcome::ActiveGrantExists);
    }

    let history_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM organization_administrators WHERE tenant_id = $1)",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await?;
    let active_admin_exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
               SELECT 1 FROM organization_administrators oa
               JOIN users u ON u.id = oa.user_id AND u.tenant_id = oa.tenant_id
               WHERE oa.tenant_id = $1 AND oa.revoked_at IS NULL AND u.disabled_at IS NULL
           )"#,
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await?;
    let action_type = if !history_exists {
        "organization_admin.bootstrap"
    } else if !active_admin_exists {
        "organization_admin.recovered"
    } else {
        "organization_admin.granted"
    };
    if action_type == "organization_admin.recovered" && recovery_reason.is_none() {
        return Ok(OperatorGrantOutcome::RecoveryReasonRequired);
    }
    if action_type != "organization_admin.recovered" && recovery_reason.is_some() {
        return Ok(OperatorGrantOutcome::RecoveryReasonNotApplicable);
    }

    let row = match sqlx::query_as::<_, OrganizationAdministratorRow>(
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
    {
        Ok(row) => row,
        Err(sqlx::Error::Database(db))
            if db.constraint() == Some("organization_administrators_one_active_grant") =>
        {
            return Ok(OperatorGrantOutcome::ActiveGrantExists);
        }
        Err(error) => return Err(error),
    };
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        actor_id,
        action_type,
        Some("organization_administrator"),
        Some(user_id),
        if let Some(recovery_reason) = recovery_reason {
            json!({ "grant_id": row.grant_id, "recovery_reason": recovery_reason })
        } else {
            json!({ "grant_id": row.grant_id })
        },
        correlation_id,
    )
    .await?;
    tx.commit().await?;
    Ok(OperatorGrantOutcome::Created(row))
}

pub(crate) async fn revoke_operator_grant(
    pool: &PgPool,
    tenant_id: Uuid,
    actor_id: Uuid,
    user_id: Uuid,
    correlation_id: Option<&str>,
) -> Result<OperatorRevokeOutcome, sqlx::Error> {
    let mut tx = pool.begin().await?;
    lock_tenant_admin_grants(&mut tx, tenant_id).await?;

    let Some(grant_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM organization_administrators WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NULL FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(OperatorRevokeOutcome::NotFound);
    };

    if removal_would_leave_no_enabled_admin(&mut tx, tenant_id, user_id).await? {
        return Ok(OperatorRevokeOutcome::LastEnabledAdmin);
    }

    sqlx::query(
        "UPDATE organization_administrators
         SET revoked_by = $2, revoked_at = NOW(), revocation_source = 'operator'
         WHERE id = $1",
    )
    .bind(grant_id)
    .bind(actor_id)
    .execute(&mut *tx)
    .await?;
    insert_scoped_admin_action(
        &mut tx,
        tenant_id,
        actor_id,
        "organization_admin.revoked",
        Some("organization_administrator"),
        Some(user_id),
        json!({ "grant_id": grant_id }),
        correlation_id,
    )
    .await?;
    tx.commit().await?;
    Ok(OperatorRevokeOutcome::Revoked)
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
                         'organization_administrator', $2, $3, $4, 'success')",
            )
            .bind(tenant_id)
            .bind(user_id)
            .bind(json!({ "actor_kind": source, "grant_id": grant_id }))
            .bind(correlation_id)
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
    correlation_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    revoke_for_lifecycle(tx, tenant_id, None, user_id, "scim", correlation_id).await
}
