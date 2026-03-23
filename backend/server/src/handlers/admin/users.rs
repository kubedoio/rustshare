//! Admin user management handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use rustshare_auth::PasswordHasher;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{handlers::AdminUser, AppState};
use super::log_admin_action;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    pub search: Option<String>,
    pub status: Option<String>, // "active" | "disabled" | None (all)
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AdminUserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub display_name: String,
    pub is_admin: bool,
    pub storage_quota_bytes: i64,
    pub disabled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct AdminUserDetailResponse {
    #[serde(flatten)]
    pub user: AdminUserResponse,
    pub storage_used_bytes: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
    pub is_admin: Option<bool>,
    pub storage_quota_bytes: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub storage_quota_bytes: Option<i64>,
    pub is_admin: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedUsers {
    pub users: Vec<AdminUserResponse>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

// ---------------------------------------------------------------------------
// Internal row type
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    email: String,
    display_name: String,
    is_admin: bool,
    storage_quota: i64,
    disabled_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<UserRow> for AdminUserResponse {
    fn from(row: UserRow) -> Self {
        AdminUserResponse {
            id: row.id.to_string(),
            username: row.username,
            email: row.email,
            display_name: row.display_name,
            is_admin: row.is_admin,
            storage_quota_bytes: row.storage_quota,
            disabled_at: row.disabled_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/users
pub async fn list_admin_users(
    State(state): State<AppState>,
    AdminUser { user_id: _ }: AdminUser,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<PaginatedUsers>, (StatusCode, Json<serde_json::Value>)> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).min(100).max(1);
    let offset = (page - 1) * per_page;

    let cols = "id, username, email, display_name, is_admin, storage_quota, disabled_at, created_at, updated_at";
    let order = "ORDER BY created_at DESC";

    let (rows, total): (Vec<UserRow>, i64) =
        match (query.search.as_deref(), query.status.as_deref()) {
            (None, None) | (None, Some("all")) => {
                let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
                    .fetch_one(&state.db_pool)
                    .await
                    .map_err(db_error)?;
                let rows = sqlx::query_as::<_, UserRow>(&format!(
                    "SELECT {cols} FROM users {order} LIMIT $1 OFFSET $2"
                ))
                .bind(per_page)
                .bind(offset)
                .fetch_all(&state.db_pool)
                .await
                .map_err(db_error)?;
                (rows, total)
            }
            (None, Some("active")) => {
                let total: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM users WHERE disabled_at IS NULL",
                )
                .fetch_one(&state.db_pool)
                .await
                .map_err(db_error)?;
                let rows = sqlx::query_as::<_, UserRow>(&format!(
                    "SELECT {cols} FROM users WHERE disabled_at IS NULL {order} LIMIT $1 OFFSET $2"
                ))
                .bind(per_page)
                .bind(offset)
                .fetch_all(&state.db_pool)
                .await
                .map_err(db_error)?;
                (rows, total)
            }
            (None, Some("disabled")) => {
                let total: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM users WHERE disabled_at IS NOT NULL",
                )
                .fetch_one(&state.db_pool)
                .await
                .map_err(db_error)?;
                let rows = sqlx::query_as::<_, UserRow>(&format!(
                    "SELECT {cols} FROM users WHERE disabled_at IS NOT NULL {order} LIMIT $1 OFFSET $2"
                ))
                .bind(per_page)
                .bind(offset)
                .fetch_all(&state.db_pool)
                .await
                .map_err(db_error)?;
                (rows, total)
            }
            (Some(search), None) | (Some(search), Some("all")) => {
                let pattern = format!("%{}%", search);
                let total: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM users WHERE username ILIKE $1 OR email ILIKE $1",
                )
                .bind(&pattern)
                .fetch_one(&state.db_pool)
                .await
                .map_err(db_error)?;
                let rows = sqlx::query_as::<_, UserRow>(&format!(
                    "SELECT {cols} FROM users WHERE username ILIKE $1 OR email ILIKE $1 {order} LIMIT $2 OFFSET $3"
                ))
                .bind(&pattern)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&state.db_pool)
                .await
                .map_err(db_error)?;
                (rows, total)
            }
            (Some(search), Some("active")) => {
                let pattern = format!("%{}%", search);
                let total: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM users WHERE (username ILIKE $1 OR email ILIKE $1) AND disabled_at IS NULL",
                )
                .bind(&pattern)
                .fetch_one(&state.db_pool)
                .await
                .map_err(db_error)?;
                let rows = sqlx::query_as::<_, UserRow>(&format!(
                    "SELECT {cols} FROM users WHERE (username ILIKE $1 OR email ILIKE $1) AND disabled_at IS NULL {order} LIMIT $2 OFFSET $3"
                ))
                .bind(&pattern)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&state.db_pool)
                .await
                .map_err(db_error)?;
                (rows, total)
            }
            (Some(search), Some("disabled")) => {
                let pattern = format!("%{}%", search);
                let total: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM users WHERE (username ILIKE $1 OR email ILIKE $1) AND disabled_at IS NOT NULL",
                )
                .bind(&pattern)
                .fetch_one(&state.db_pool)
                .await
                .map_err(db_error)?;
                let rows = sqlx::query_as::<_, UserRow>(&format!(
                    "SELECT {cols} FROM users WHERE (username ILIKE $1 OR email ILIKE $1) AND disabled_at IS NOT NULL {order} LIMIT $2 OFFSET $3"
                ))
                .bind(&pattern)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&state.db_pool)
                .await
                .map_err(db_error)?;
                (rows, total)
            }
            _ => {
                // Fallback: all users
                let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
                    .fetch_one(&state.db_pool)
                    .await
                    .map_err(db_error)?;
                let rows = sqlx::query_as::<_, UserRow>(&format!(
                    "SELECT {cols} FROM users {order} LIMIT $1 OFFSET $2"
                ))
                .bind(per_page)
                .bind(offset)
                .fetch_all(&state.db_pool)
                .await
                .map_err(db_error)?;
                (rows, total)
            }
        };

    Ok(Json(PaginatedUsers {
        users: rows.into_iter().map(AdminUserResponse::from).collect(),
        total,
        page,
        per_page,
    }))
}

/// POST /api/v1/admin/users
pub async fn create_admin_user(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<AdminUserResponse>), (StatusCode, Json<serde_json::Value>)> {
    // Validate inputs
    if req.username.trim().is_empty() {
        return Err(bad_request("Username must not be empty"));
    }
    if !req.email.contains('@') {
        return Err(bad_request("Invalid email address"));
    }
    if req.password.len() < 8 {
        return Err(bad_request("Password must be at least 8 characters"));
    }

    // Check username uniqueness
    let username_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = $1")
            .bind(&req.username)
            .fetch_one(&state.db_pool)
            .await
            .map_err(db_error)?;
    if username_count > 0 {
        return Err(conflict_error("Username already taken"));
    }

    // Check email uniqueness
    let email_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = $1")
        .bind(&req.email)
        .fetch_one(&state.db_pool)
        .await
        .map_err(db_error)?;
    if email_count > 0 {
        return Err(conflict_error("Email already registered"));
    }

    // Hash password
    let password_hash =
        PasswordHasher::hash(&req.password).map_err(|_| internal_error("Password hashing failed"))?;

    let new_id = Uuid::new_v4();
    let display_name = req
        .display_name
        .as_deref()
        .unwrap_or(&req.username)
        .to_string();
    let is_admin = req.is_admin.unwrap_or(false);
    let storage_quota = req.storage_quota_bytes.unwrap_or(10_737_418_240_i64);

    let cols = "id, username, email, display_name, is_admin, storage_quota, disabled_at, created_at, updated_at";
    let row = sqlx::query_as::<_, UserRow>(&format!(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {cols}"
    ))
    .bind(new_id)
    .bind(&req.username)
    .bind(&req.email)
    .bind(&password_hash)
    .bind(&display_name)
    .bind(is_admin)
    .bind(storage_quota)
    .fetch_one(&state.db_pool)
    .await
    .map_err(db_error)?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "user.created",
        Some("user"),
        Some(new_id),
        json!({"username": req.username}),
    )
    .await;

    Ok((StatusCode::CREATED, Json(AdminUserResponse::from(row))))
}

/// GET /api/v1/admin/users/:id
pub async fn get_admin_user(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<AdminUserDetailResponse>, (StatusCode, Json<serde_json::Value>)> {
    let cols = "id, username, email, display_name, is_admin, storage_quota, disabled_at, created_at, updated_at";
    let row = sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {cols} FROM users WHERE id = $1"
    ))
    .bind(user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("User not found"))?;

    let storage_used_bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size), 0) FROM (
             SELECT DISTINCT ON (fv.storage_key) fv.size
             FROM file_versions fv
             JOIN files f ON f.id = fv.file_id
             WHERE f.owner_id = $1
         ) sub",
    )
    .bind(user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(db_error)?;

    Ok(Json(AdminUserDetailResponse {
        user: AdminUserResponse::from(row),
        storage_used_bytes,
    }))
}

/// PATCH /api/v1/admin/users/:id
pub async fn update_admin_user(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(user_id): Path<Uuid>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<AdminUserResponse>, (StatusCode, Json<serde_json::Value>)> {
    let cols = "id, username, email, display_name, is_admin, storage_quota, disabled_at, created_at, updated_at";

    // Fetch current user
    let current = sqlx::query_as::<_, UserRow>(&format!(
        "SELECT {cols} FROM users WHERE id = $1"
    ))
    .bind(user_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("User not found"))?;

    let new_display_name = req
        .display_name
        .as_deref()
        .unwrap_or(&current.display_name)
        .to_string();
    let new_email = req
        .email
        .as_deref()
        .unwrap_or(&current.email)
        .to_string();
    let new_quota = req.storage_quota_bytes.unwrap_or(current.storage_quota);
    let new_is_admin = req.is_admin.unwrap_or(current.is_admin);

    let quota_changed = new_quota != current.storage_quota;

    let row = sqlx::query_as::<_, UserRow>(&format!(
        "UPDATE users
         SET display_name = $2, email = $3, storage_quota = $4, is_admin = $5, updated_at = NOW()
         WHERE id = $1
         RETURNING {cols}"
    ))
    .bind(user_id)
    .bind(&new_display_name)
    .bind(&new_email)
    .bind(new_quota)
    .bind(new_is_admin)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("User not found"))?;

    if quota_changed {
        log_admin_action(
            &state.db_pool,
            actor_id,
            "user.quota_changed",
            Some("user"),
            Some(user_id),
            json!({
                "old_quota": current.storage_quota,
                "new_quota": new_quota,
            }),
        )
        .await;
    }

    Ok(Json(AdminUserResponse::from(row)))
}

/// POST /api/v1/admin/users/:id/disable
pub async fn disable_admin_user(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    if user_id == actor_id {
        return Err(bad_request("Cannot disable your own account"));
    }

    sqlx::query("UPDATE users SET disabled_at = NOW() WHERE id = $1")
        .bind(user_id)
        .execute(&state.db_pool)
        .await
        .map_err(db_error)?;

    sqlx::query("DELETE FROM user_sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(&state.db_pool)
        .await
        .map_err(db_error)?;

    // Revoke all device tokens for the disabled user
    sqlx::query("UPDATE device_tokens SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(user_id)
        .execute(&state.db_pool)
        .await
        .map_err(db_error)?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "user.disabled",
        Some("user"),
        Some(user_id),
        json!({}),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/admin/users/:id/enable
pub async fn enable_admin_user(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    sqlx::query("UPDATE users SET disabled_at = NULL WHERE id = $1")
        .bind(user_id)
        .execute(&state.db_pool)
        .await
        .map_err(db_error)?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "user.enabled",
        Some("user"),
        Some(user_id),
        json!({}),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/admin/users/:id
pub async fn delete_admin_user(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    if user_id == actor_id {
        return Err(bad_request("Cannot delete your own account"));
    }

    // Collect distinct storage keys before deletion
    let storage_keys: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT storage_key FROM file_versions
         WHERE file_id IN (SELECT id FROM files WHERE owner_id = $1)",
    )
    .bind(user_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(db_error)?;

    // Log before deletion so actor_id is still valid
    log_admin_action(
        &state.db_pool,
        actor_id,
        "user.deleted",
        Some("user"),
        Some(user_id),
        json!({"storage_keys_count": storage_keys.len()}),
    )
    .await;

    // Delete user (CASCADE handles files, file_versions, shares, etc.)
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&state.db_pool)
        .await
        .map_err(db_error)?;

    // Spawn background blob cleanup
    let object_store = std::sync::Arc::clone(&state.object_store);
    tokio::spawn(async move {
        for key in storage_keys {
            let _ = object_store.delete(&key).await;
        }
    });

    Ok(StatusCode::ACCEPTED)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn db_error(e: sqlx::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("Database error: {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "Database error" })),
    )
}

fn not_found(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": msg })))
}

fn bad_request(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg })))
}

fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
}

fn conflict_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::CONFLICT, Json(json!({ "error": msg })))
}
