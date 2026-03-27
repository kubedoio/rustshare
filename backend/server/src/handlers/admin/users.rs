//! Admin user management handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
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
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/users
pub async fn list_admin_users(
    State(_state): State<AppState>,
    AdminUser { user_id: _ }: AdminUser,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<PaginatedUsers>, (StatusCode, Json<serde_json::Value>)> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");

    Ok(Json(PaginatedUsers {
        users: vec![],
        total: 0,
        page,
        per_page,
    }))
}

/// POST /api/v1/admin/users
pub async fn create_admin_user(
    State(_state): State<AppState>,
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

    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");

    let new_id = Uuid::new_v4();

    log_admin_action(
        actor_id,
        "user.created",
        Some("user"),
        Some(new_id),
        json!({"username": req.username}),
    )
    .await;

    Err(internal_error("User creation not yet implemented in zero-PostgreSQL mode"))
}

/// GET /api/v1/admin/users/:id
pub async fn get_admin_user(
    State(_state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(_user_id): Path<Uuid>,
) -> Result<Json<AdminUserDetailResponse>, (StatusCode, Json<serde_json::Value>)> {
    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");
    Err(not_found("User not found"))
}

/// PATCH /api/v1/admin/users/:id
pub async fn update_admin_user(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(user_id): Path<Uuid>,
    Json(_req): Json<UpdateUserRequest>,
) -> Result<Json<AdminUserResponse>, (StatusCode, Json<serde_json::Value>)> {
    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");

    log_admin_action(
        actor_id,
        "user.updated",
        Some("user"),
        Some(user_id),
        json!({}),
    )
    .await;

    Err(not_found("User not found"))
}

/// POST /api/v1/admin/users/:id/disable
pub async fn disable_admin_user(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    if user_id == actor_id {
        return Err(bad_request("Cannot disable your own account"));
    }

    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");

    log_admin_action(
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
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");

    log_admin_action(
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
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    if user_id == actor_id {
        return Err(bad_request("Cannot delete your own account"));
    }

    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");

    log_admin_action(
        actor_id,
        "user.deleted",
        Some("user"),
        Some(user_id),
        json!({"storage_keys_count": 0}),
    )
    .await;

    Ok(StatusCode::ACCEPTED)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
