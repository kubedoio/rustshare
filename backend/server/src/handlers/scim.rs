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

use crate::AppState;

// ---------------------------------------------------------------------------
// Authentication middleware for SCIM endpoints
// ---------------------------------------------------------------------------

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
    let service = ScimService::new(
        repository,
        state.default_tenant_id,
        crate::default_storage_quota_bytes(),
    );

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
    let service = ScimService::new(
        repository,
        state.default_tenant_id,
        crate::default_storage_quota_bytes(),
    );

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
    let service = ScimService::new(
        repository,
        state.default_tenant_id,
        crate::default_storage_quota_bytes(),
    );

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
    let service = ScimService::new(
        repository,
        state.default_tenant_id,
        crate::default_storage_quota_bytes(),
    );

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
            WHERE external_id = $1
            "#,
            external_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    async fn find_user_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        let email_lower = email.trim().to_lowercase();

        let row = sqlx::query_as!(
            User,
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme as "theme: _", created_at, updated_at, disabled_at,
                   name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id,
                   dashboard_config as "dashboard_config: _"
            FROM users
            WHERE LOWER(email) = $1
            "#,
            email_lower
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

    async fn update_user(
        &self,
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
            WHERE id = $1
            "#,
            user_id,
            display_name,
            email,
            name,
            surname,
            disabled_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn set_user_disabled(
        &self,
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
            WHERE external_id = $1
            "#,
            external_id,
            disabled_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_group_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<Option<GroupRecord>, sqlx::Error> {
        let row = sqlx::query_as!(
            GroupRecordRow,
            r#"
            SELECT id, external_id, name
            FROM user_groups
            WHERE external_id = $1
            "#,
            external_id
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
        external_id: &str,
        display_name: &str,
    ) -> Result<uuid::Uuid, sqlx::Error> {
        let id = uuid::Uuid::new_v4();

        sqlx::query!(
            r#"
            INSERT INTO user_groups (id, name, external_id, created_by)
            VALUES ($1, $2, $3, NULL)
            "#,
            id,
            display_name,
            external_id
        )
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    async fn update_group(&self, external_id: &str, display_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE user_groups
            SET name = $2,
                updated_at = NOW()
            WHERE external_id = $1
            "#,
            external_id,
            display_name
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_group(&self, external_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM user_groups WHERE external_id = $1",
            external_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_user_id_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<Option<uuid::Uuid>, sqlx::Error> {
        let id: Option<uuid::Uuid> =
            sqlx::query_scalar!("SELECT id FROM users WHERE external_id = $1", external_id)
                .fetch_optional(&self.pool)
                .await?;

        Ok(id)
    }

    async fn get_group_members(
        &self,
        group_id: uuid::Uuid,
    ) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
        let members: Vec<uuid::Uuid> = sqlx::query_scalar!(
            "SELECT user_id FROM group_members WHERE group_id = $1",
            group_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(members)
    }

    async fn add_group_member(
        &self,
        group_id: uuid::Uuid,
        user_id: uuid::Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO group_members (group_id, user_id, added_by)
            VALUES ($1, $2, NULL)
            ON CONFLICT (group_id, user_id) DO NOTHING
            "#,
            group_id,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn remove_group_member(
        &self,
        group_id: uuid::Uuid,
        user_id: uuid::Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "DELETE FROM group_members WHERE group_id = $1 AND user_id = $2",
            group_id,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn clear_group_members(&self, group_id: uuid::Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM group_members WHERE group_id = $1", group_id)
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
