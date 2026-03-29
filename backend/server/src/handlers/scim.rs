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
use sqlx::Row;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::AppState;

// ---------------------------------------------------------------------------
// Authentication middleware for SCIM endpoints
// ---------------------------------------------------------------------------

/// Verify SCIM bearer token from Authorization header.
fn verify_scim_token(headers: &axum::http::HeaderMap) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
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
// Request types
// ---------------------------------------------------------------------------

/// Query parameters for SCIM user deprovisioning.
#[derive(Debug, Deserialize)]
pub struct DeprovisionQuery {
    /// If true, permanently delete the user instead of disabling
    #[serde(default)]
    pub permanent: bool,
}

// ---------------------------------------------------------------------------
// User handlers
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
        default_storage_quota_bytes(),
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
    let external_id = urlencoding::decode(&external_id)
        .map(|s| s.into_owned())
        .unwrap_or(external_id);

    let repository = Arc::new(ScimRepositoryImpl::new(state.db_pool.clone()));
    let service = ScimService::new(
        repository,
        state.default_tenant_id,
        default_storage_quota_bytes(),
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
        default_storage_quota_bytes(),
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
    let external_id = urlencoding::decode(&external_id)
        .map(|s| s.into_owned())
        .unwrap_or(external_id);

    let repository = Arc::new(ScimRepositoryImpl::new(state.db_pool.clone()));
    let service = ScimService::new(
        repository,
        state.default_tenant_id,
        default_storage_quota_bytes(),
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

use rustshare_core::services::{GroupRecord, ScimRepository};
use rustshare_core::domain::{Theme, User};
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

#[async_trait::async_trait]
impl ScimRepository for ScimRepositoryImpl {
    async fn find_user_by_external_id(&self, external_id: &str) -> Result<Option<User>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme, created_at, updated_at, disabled_at, 
                   name, surname, avatar_path, email_sharing_enabled, tenant_id
            FROM users
            WHERE external_id = $1
            "#,
        )
        .bind(external_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| map_user_row(&r)).transpose()
    }

    async fn find_user_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        let email_lower = email.trim().to_lowercase();

        let row = sqlx::query(
            r#"
            SELECT id, username, display_name, password_hash, email, is_admin,
                   storage_quota, theme, created_at, updated_at, disabled_at,
                   name, surname, avatar_path, email_sharing_enabled, tenant_id
            FROM users
            WHERE LOWER(email) = $1
            "#,
        )
        .bind(email_lower)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| map_user_row(&r)).transpose()
    }

    async fn create_user(&self, user: &User, external_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO users (
                id, username, display_name, password_hash, email, is_admin,
                storage_quota, theme, created_at, updated_at, disabled_at,
                name, surname, avatar_path, email_sharing_enabled, tenant_id, external_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            "#,
        )
        .bind(user.id)
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(&user.email)
        .bind(user.is_admin)
        .bind(user.storage_quota)
        .bind(user.theme.to_string())
        .bind(user.created_at)
        .bind(user.updated_at)
        .bind(user.disabled_at)
        .bind(&user.name)
        .bind(&user.surname)
        .bind(&user.avatar_path)
        .bind(user.email_sharing_enabled)
        .bind(user.tenant_id)
        .bind(external_id)
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
        disabled_at: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
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
        )
        .bind(user_id)
        .bind(display_name)
        .bind(email)
        .bind(name)
        .bind(surname)
        .bind(disabled_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn set_user_disabled(&self, external_id: &str, disabled: bool) -> Result<(), sqlx::Error> {
        let disabled_at: Option<DateTime<Utc>> = if disabled { Some(Utc::now()) } else { None };

        sqlx::query(
            r#"
            UPDATE users
            SET disabled_at = $2,
                updated_at = NOW()
            WHERE external_id = $1
            "#,
        )
        .bind(external_id)
        .bind(disabled_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_group_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<Option<GroupRecord>, sqlx::Error> {
        let row = sqlx::query_as::<_, GroupRecordRow>(
            r#"
            SELECT id, external_id, name
            FROM user_groups
            WHERE external_id = $1
            "#,
        )
        .bind(external_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| GroupRecord {
            id: r.id,
            external_id: r.external_id,
            name: r.name,
        }))
    }

    async fn create_group(
        &self,
        external_id: &str,
        display_name: &str,
    ) -> Result<uuid::Uuid, sqlx::Error> {
        let id = uuid::Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO user_groups (id, name, external_id, created_by)
            VALUES ($1, $2, $3, NULL)
            "#,
        )
        .bind(id)
        .bind(display_name)
        .bind(external_id)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    async fn update_group(
        &self,
        external_id: &str,
        display_name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE user_groups
            SET name = $2,
                updated_at = NOW()
            WHERE external_id = $1
            "#,
        )
        .bind(external_id)
        .bind(display_name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_group(&self, external_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM user_groups WHERE external_id = $1")
            .bind(external_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn find_user_id_by_external_id(&self, external_id: &str) -> Result<Option<uuid::Uuid>, sqlx::Error> {
        let id: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id FROM users WHERE external_id = $1"
        )
        .bind(external_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(id)
    }

    async fn get_group_members(&self, group_id: uuid::Uuid) -> Result<Vec<uuid::Uuid>, sqlx::Error> {
        let members: Vec<uuid::Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM group_members WHERE group_id = $1"
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(members)
    }

    async fn add_group_member(&self, group_id: uuid::Uuid, user_id: uuid::Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO group_members (group_id, user_id, added_by)
            VALUES ($1, $2, NULL)
            ON CONFLICT (group_id, user_id) DO NOTHING
            "#,
        )
        .bind(group_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn remove_group_member(&self, group_id: uuid::Uuid, user_id: uuid::Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "DELETE FROM group_members WHERE group_id = $1 AND user_id = $2"
        )
        .bind(group_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn clear_group_members(&self, group_id: uuid::Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM group_members WHERE group_id = $1")
            .bind(group_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

use chrono::{DateTime, Utc};

#[derive(sqlx::FromRow)]
struct GroupRecordRow {
    id: uuid::Uuid,
    external_id: String,
    name: String,
}

fn map_user_row(row: &sqlx::postgres::PgRow) -> Result<User, sqlx::Error> {
    Ok(User {
        id: row.try_get("id")?,
        username: row.try_get("username")?,
        display_name: row.try_get("display_name")?,
        password_hash: row.try_get("password_hash")?,
        email: row.try_get("email")?,
        is_admin: row.try_get("is_admin")?,
        storage_quota: row.try_get("storage_quota")?,
        theme: row
            .try_get::<String, _>("theme")?
            .parse()
            .unwrap_or_default(),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        disabled_at: row.try_get("disabled_at")?,
        name: row.try_get("name")?,
        surname: row.try_get("surname")?,
        avatar_path: row.try_get("avatar_path")?,
        email_sharing_enabled: row.try_get("email_sharing_enabled")?,
        tenant_id: row.try_get("tenant_id")?,
    })
}

fn default_storage_quota_bytes() -> i64 {
    std::env::var("RUSTSHARE_DEFAULT_STORAGE_QUOTA_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_737_418_240) // 10 GB
}

// Constant-time comparison for tokens
mod constant_time_eq {
    pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        let mut result = 0u8;
        for (x, y) in a.iter().zip(b.iter()) {
            result |= x ^ y;
        }
        result == 0
    }
}
