//! Admin panel handlers. All routes require `AdminUser` extractor (is_admin = true).

pub mod applications;
pub mod audit;
pub mod config;
pub mod groups;
pub mod organization_administrators;
pub mod templates;
pub mod users;
pub mod webhooks;
pub mod workflows;

use axum::http::HeaderMap;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Resolve the organization a new principal is created in (AR-01; ADR-0040 D4,
/// ADR-0043 D-D2 I1).
///
/// The organization is always derived from configuration — never supplied by
/// the request. An unset or unparsable `RUSTSHARE_DEFAULT_TENANT_ID` resolves
/// to the nil default organization (ADR-0043). Where no organization can be
/// resolved at all, creation fails closed and writes nothing rather than
/// letting the column default become the source of the row.
pub(crate) fn resolve_creation_tenant(state: &crate::AppState) -> Result<Uuid, AppError> {
    state
        .default_tenant_config
        .resolved_tenant()
        .ok_or_else(|| admin_internal_error("No default organization could be resolved"))
}

/// Record a non-transactional admin action in the `admin_actions` table.
pub async fn log_admin_action(
    pool: &PgPool,
    actor_id: Uuid,
    action_type: &str,
    target_type: Option<&str>,
    target_id: Option<Uuid>,
    detail: serde_json::Value,
) {
    let detail = audit::project_audit_detail("admin_action", action_type, &detail);
    let result = sqlx::query(
        r#"
        INSERT INTO admin_actions
            (actor_id, actor_principal_id, tenant_id, action_type, target_type, target_id, detail)
        VALUES ($1, $1, (SELECT tenant_id FROM users WHERE id = $1), $2, $3, $4, $5)
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
        tracing::error!(actor_id = %actor_id, action_type, ?target_id, "Failed to log admin action: {e}");
    }
}

/// Insert an admin action in the same transaction as its corresponding mutation.
pub async fn insert_admin_action(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    action_type: &str,
    target_type: Option<&str>,
    target_id: Option<Uuid>,
    detail: serde_json::Value,
) -> Result<(), sqlx::Error> {
    let detail = audit::project_audit_detail("admin_action", action_type, &detail);
    sqlx::query(
        r#"
        INSERT INTO admin_actions
            (actor_id, actor_principal_id, tenant_id, action_type, target_type, target_id, detail)
        VALUES ($1, $1, (SELECT tenant_id FROM users WHERE id = $1), $2, $3, $4, $5)
        "#,
    )
    .bind(actor_id)
    .bind(action_type)
    .bind(target_type)
    .bind(target_id)
    .bind(detail)
    .execute(&mut **tx)
    .await?;

    Ok(())
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
    let detail = audit::project_audit_detail("admin_action", action_type, &detail);
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

pub fn request_correlation_id(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(crate::middleware::REQUEST_ID_HEADER)?
        .to_str()
        .ok()
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        })
}

use crate::handlers::AppError;

pub fn admin_not_found(msg: impl Into<String>) -> AppError {
    AppError::not_found(msg)
}

pub fn admin_bad_request(msg: impl Into<String>) -> AppError {
    AppError::bad_request(msg)
}

pub fn admin_conflict(msg: impl Into<String>) -> AppError {
    AppError::conflict(msg)
}

pub fn admin_internal_error(msg: impl Into<String>) -> AppError {
    AppError::internal(msg)
}
