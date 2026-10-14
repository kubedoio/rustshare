//! SCIM-lite provisioning webhook handlers.
//!
//! Provides webhook-style SCIM endpoints for enterprise IdP integration.
//! These endpoints require bearer token authentication via RUSTSHARE_SCIM_BEARER_TOKEN.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum_extra::headers::{authorization::Bearer, Authorization, HeaderMapExt};
use rustshare_core::services::{ScimGroup, ScimService, ScimUser};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

// ---------------------------------------------------------------------------
// Authentication middleware for SCIM endpoints
// ---------------------------------------------------------------------------

/// Resolve the single organization this SCIM integration is bound to
/// (ADR-0040 D5).
///
/// `RUSTSHARE_SCIM_TENANT_ID` supplies the scope; when it is unset, an
/// explicitly configured `RUSTSHARE_DEFAULT_TENANT_ID` supplies it. Any other
/// state — unset with no explicit default, empty, or not a valid UUID — fails
/// closed: a SCIM token without exactly one valid organization must never
/// operate installation-wide.
pub(crate) fn resolve_scim_tenant_scope() -> Result<Uuid, String> {
    if let Ok(raw) = std::env::var("RUSTSHARE_SCIM_TENANT_ID") {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("RUSTSHARE_SCIM_TENANT_ID is set but empty".to_string());
        }
        return raw
            .parse::<Uuid>()
            .map_err(|_| format!("RUSTSHARE_SCIM_TENANT_ID is not a valid UUID: {raw:?}"));
    }
    if let Ok(raw) = std::env::var("RUSTSHARE_DEFAULT_TENANT_ID") {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("RUSTSHARE_DEFAULT_TENANT_ID is set but empty".to_string());
        }
        return raw
            .parse::<Uuid>()
            .map_err(|_| format!("RUSTSHARE_DEFAULT_TENANT_ID is not a valid UUID: {raw:?}"));
    }
    Err(
        "no SCIM organization scope configured: set RUSTSHARE_SCIM_TENANT_ID \
         to the organization this integration provisions"
            .to_string(),
    )
}

/// Resolve the SCIM organization scope or map the misconfiguration to a
/// fail-closed 500 response. The specific misconfiguration is logged
/// server-side; the response body stays generic.
fn scim_scope_or_error() -> Result<Uuid, (StatusCode, Json<serde_json::Value>)> {
    resolve_scim_tenant_scope().map_err(|detail| {
        tracing::error!("SCIM organization scope misconfigured: {}", detail);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "SCIM misconfigured",
                "detail": "SCIM organization scope is not configured correctly"
            })),
        )
    })
}

/// Verify SCIM bearer token from Authorization header.
fn verify_scim_token(
    headers: &axum::http::HeaderMap,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let expected_token = match std::env::var("RUSTSHARE_SCIM_BEARER_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => {
            tracing::error!("RUSTSHARE_SCIM_BEARER_TOKEN not configured");
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": "SCIM not configured",
                    "detail": "RUSTSHARE_SCIM_BEARER_TOKEN environment variable not set"
                })),
            ));
        }
    };

    let auth_header = headers
        .typed_get::<Authorization<Bearer>>()
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "Unauthorized",
                    "detail": "Missing Authorization header with Bearer token"
                })),
            )
        })?;

    // Use constant-time comparison to prevent timing attacks
    if !constant_time_eq::constant_time_eq(
        auth_header.token().as_bytes(),
        expected_token.as_bytes(),
    ) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "Unauthorized",
                "detail": "Invalid bearer token"
            })),
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
/// POST /api/v1/scim/users
/// Provision or update a user from SCIM data.
pub async fn provision_user(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(scim_user): Json<ScimUser>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    verify_scim_token(&headers)?;

    // Validate required fields
    if scim_user.external_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": "externalId is required"
            })),
        ));
    }

    if scim_user.user_name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": "userName is required"
            })),
        ));
    }

    // Create repository and service
    let repository = Arc::new(ScimRepositoryImpl::new(state.db_pool.clone()));
    let scope = scim_scope_or_error()?;
    let service = ScimService::new(repository, scope, crate::default_storage_quota_bytes());

    match service.provision_user(scim_user).await {
        Ok(result) => {
            let status = match result.action {
                rustshare_core::services::ScimAction::Created => StatusCode::CREATED,
                rustshare_core::services::ScimAction::Updated => StatusCode::OK,
            };

            Ok((
                status,
                Json(json!({
                    "id": result.id.to_string(),
                    "external_id": result.external_id,
                    "action": match result.action {
                        rustshare_core::services::ScimAction::Created => "created",
                        rustshare_core::services::ScimAction::Updated => "updated",
                    }
                })),
            ))
        }
        Err(rustshare_core::services::ScimError::InvalidRequest(detail)) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": detail
            })),
        )),
        Err(rustshare_core::services::ScimError::Conflict(detail)) => Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "Conflict",
                "detail": detail
            })),
        )),
        Err(e) => {
            tracing::error!("SCIM provision_user error: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Internal server error",
                    "detail": e.to_string()
                })),
            ))
        }
    }
}

/// DELETE /api/v1/scim/users/{external_id}
/// Deprovision (disable) a user.
pub async fn deprovision_user(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(external_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    verify_scim_token(&headers)?;

    // URL decode the external_id
    let external_id = percent_encoding::percent_decode_str(&external_id)
        .decode_utf8()
        .map(|s| s.into_owned())
        .unwrap_or(external_id);

    let repository = Arc::new(ScimRepositoryImpl::new(state.db_pool.clone()));
    let scope = scim_scope_or_error()?;
    let service = ScimService::new(repository, scope, crate::default_storage_quota_bytes());

    match service.deprovision_user(&external_id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(rustshare_core::services::ScimError::UserNotFound(_)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "Not Found",
                "detail": format!("User with external_id '{}' not found", external_id)
            })),
        )),
        Err(e) => {
            tracing::error!("SCIM deprovision_user error: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Internal server error",
                    "detail": e.to_string()
                })),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Group handlers
// ---------------------------------------------------------------------------

/// POST /api/v1/scim/groups
/// Provision or update a group from SCIM data.
pub async fn provision_group(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(scim_group): Json<ScimGroup>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    verify_scim_token(&headers)?;

    // Validate required fields
    if scim_group.external_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": "externalId is required"
            })),
        ));
    }

    if scim_group.display_name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": "displayName is required"
            })),
        ));
    }

    let repository = Arc::new(ScimRepositoryImpl::new(state.db_pool.clone()));
    let scope = scim_scope_or_error()?;
    let service = ScimService::new(repository, scope, crate::default_storage_quota_bytes());

    match service.provision_group(scim_group).await {
        Ok(result) => {
            let status = match result.action {
                rustshare_core::services::ScimAction::Created => StatusCode::CREATED,
                rustshare_core::services::ScimAction::Updated => StatusCode::OK,
            };

            Ok((
                status,
                Json(json!({
                    "id": result.id.to_string(),
                    "external_id": result.external_id,
                    "action": match result.action {
                        rustshare_core::services::ScimAction::Created => "created",
                        rustshare_core::services::ScimAction::Updated => "updated",
                    }
                })),
            ))
        }
        Err(rustshare_core::services::ScimError::InvalidRequest(detail)) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Bad Request",
                "detail": detail
            })),
        )),
        Err(rustshare_core::services::ScimError::Conflict(detail)) => Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "Conflict",
                "detail": detail
            })),
        )),
        Err(e) => {
            tracing::error!("SCIM provision_group error: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Internal server error",
                    "detail": e.to_string()
                })),
            ))
        }
    }
}

/// DELETE /api/v1/scim/groups/{external_id}
/// Delete a group.
pub async fn delete_group(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(external_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    verify_scim_token(&headers)?;

    // URL decode the external_id
    let external_id = percent_encoding::percent_decode_str(&external_id)
        .decode_utf8()
        .map(|s| s.into_owned())
        .unwrap_or(external_id);

    let repository = Arc::new(ScimRepositoryImpl::new(state.db_pool.clone()));
    let scope = scim_scope_or_error()?;
    let service = ScimService::new(repository, scope, crate::default_storage_quota_bytes());

    match service.delete_group(&external_id).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(rustshare_core::services::ScimError::GroupNotFound(_)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "Not Found",
                "detail": format!("Group with external_id '{}' not found", external_id)
            })),
        )),
        Err(e) => {
            tracing::error!("SCIM delete_group error: {:?}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Internal server error",
                    "detail": e.to_string()
                })),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// SCIM Repository Implementation
// ---------------------------------------------------------------------------

use rustshare_core::domain::User;
use rustshare_core::services::{GroupRecord, ScimRepository};
use sqlx::PgPool;

/// SQLx-based implementation of SCIM repository.
pub struct ScimRepositoryImpl {
    pool: PgPool,
}

impl ScimRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl ScimRepository for ScimRepositoryImpl {
    async fn find_user_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        let row = sqlx::query_as!(
            User,
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme as "theme: _", created_at, updated_at, disabled_at,
                   name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id,
                   dashboard_config as "dashboard_config: _"
            FROM users
            WHERE external_id = $1 AND tenant_id = $2
            "#,
            external_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    async fn create_user(&self, user: &User, external_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO users (
                id, username, display_name, password_hash, email, is_admin,
                storage_quota, theme, created_at, updated_at, disabled_at,
                name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, external_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            "#,
            user.id,
            user.username,
            user.display_name,
            user.password_hash,
            user.email,
            user.is_admin,
            user.storage_quota,
            user.theme.to_string(),
            user.created_at,
            user.updated_at,
            user.disabled_at,
            user.name,
            user.surname,
            user.avatar_path,
            user.email_sharing_enabled,
            user.trash_retention_days,
            user.tenant_id,
            external_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn update_user(
        &self,
        tenant_id: Uuid,
        user_id: uuid::Uuid,
        display_name: &str,
        email: &str,
        name: Option<&str>,
        surname: Option<&str>,
        disabled_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE users
            SET display_name = $2,
                email = $3,
                name = $4,
                surname = $5,
                disabled_at = $6,
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $7
            "#,
            user_id,
            display_name,
            email,
            name,
            surname,
            disabled_at,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn set_user_disabled(
        &self,
        tenant_id: Uuid,
        external_id: &str,
        disabled: bool,
    ) -> Result<(), sqlx::Error> {
        let disabled_at: Option<chrono::DateTime<chrono::Utc>> = if disabled {
            Some(chrono::Utc::now())
        } else {
            None
        };

        sqlx::query!(
            r#"
            UPDATE users
            SET disabled_at = $2,
                updated_at = NOW()
            WHERE external_id = $1 AND tenant_id = $3
            "#,
            external_id,
            disabled_at,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_group_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<GroupRecord>, sqlx::Error> {
        let row = sqlx::query_as!(
            GroupRecordRow,
            r#"
            SELECT id, external_id, name
            FROM user_groups
            WHERE external_id = $1 AND tenant_id = $2
            "#,
            external_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| GroupRecord {
            id: r.id,
            external_id: r.external_id.unwrap_or_default(),
            name: r.name,
        }))
    }

    async fn create_group(
        &self,
        tenant_id: Uuid,
        external_id: &str,
        display_name: &str,
    ) -> Result<uuid::Uuid, sqlx::Error> {
        let id = uuid::Uuid::new_v4();

        sqlx::query!(
            r#"
            INSERT INTO user_groups (id, name, external_id, created_by, tenant_id)
            VALUES ($1, $2, $3, NULL, $4)
            "#,
            id,
            display_name,
            external_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    async fn update_group(
        &self,
        tenant_id: Uuid,
        external_id: &str,
        display_name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE user_groups
            SET name = $2,
                updated_at = NOW()
            WHERE external_id = $1 AND tenant_id = $3
            "#,
            external_id,
            display_name,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_group(&self, tenant_id: Uuid, external_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM user_groups WHERE external_id = $1 AND tenant_id = $2",
            external_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_user_id_by_external_id(
        &self,
        tenant_id: Uuid,
        external_id: &str,
    ) -> Result<Option<uuid::Uuid>, sqlx::Error> {
        let id: Option<uuid::Uuid> = sqlx::query_scalar!(
            "SELECT id FROM users WHERE external_id = $1 AND tenant_id = $2",
            external_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(id)
    }

    async fn get_group_members(
        &self,
        tenant_id: Uuid,
        group_id: uuid::Uuid,
    ) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
        // Both legs of the membership must belong to the organization: the
        // group (defense in depth) and the member user.
        let members: Vec<uuid::Uuid> = sqlx::query_scalar!(
            "SELECT gm.user_id FROM group_members gm WHERE gm.group_id = $1 \
                AND EXISTS (SELECT 1 FROM user_groups g WHERE g.id = gm.group_id AND g.tenant_id = $2) \
                AND EXISTS (SELECT 1 FROM users u WHERE u.id = gm.user_id AND u.tenant_id = $2)",
            group_id,
            tenant_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(members)
    }

    async fn add_group_member(
        &self,
        tenant_id: Uuid,
        group_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        // The membership is written only when both the group and the user
        // belong to the configured organization.
        sqlx::query!(
            r#"
            INSERT INTO group_members (group_id, user_id, added_by)
            SELECT $1, $2, NULL
            WHERE EXISTS (
                SELECT 1
                FROM user_groups g
                JOIN users u ON u.id = $2
                WHERE g.id = $1 AND g.tenant_id = $3 AND u.tenant_id = $3
            )
            ON CONFLICT (group_id, user_id) DO NOTHING
            "#,
            group_id,
            user_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn remove_group_member(
        &self,
        tenant_id: Uuid,
        group_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM group_members gm WHERE gm.group_id = $1 AND gm.user_id = $2 \
                AND EXISTS (SELECT 1 FROM user_groups g WHERE g.id = gm.group_id AND g.tenant_id = $3) \
                AND EXISTS (SELECT 1 FROM users u WHERE u.id = gm.user_id AND u.tenant_id = $3)",
            group_id,
            user_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn clear_group_members(
        &self,
        tenant_id: Uuid,
        group_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        // Per-row user leg: in-scope memberships are cleared, cross-tenant
        // member rows survive.
        sqlx::query!(
            "DELETE FROM group_members gm WHERE gm.group_id = $1 \
                AND EXISTS (SELECT 1 FROM user_groups g WHERE g.id = gm.group_id AND g.tenant_id = $2) \
                AND EXISTS (SELECT 1 FROM users u WHERE u.id = gm.user_id AND u.tenant_id = $2)",
            group_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct GroupRecordRow {
    id: uuid::Uuid,
    external_id: Option<String>,
    name: String,
}
