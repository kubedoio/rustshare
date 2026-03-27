//! Admin user management handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use rustshare_crypto::PasswordHasher;
use rustshare_storage::metadata_v2::schemas::{UserDocument, UserFilter};

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
    State(state): State<AppState>,
    AdminUser { user_id: _ }: AdminUser,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<PaginatedUsers>, (StatusCode, Json<serde_json::Value>)> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    // Build filter
    let mut filter = UserFilter::new()
        .with_pagination((page - 1) * per_page, per_page);
    
    if let Some(search) = query.search {
        filter = filter.with_search(search);
    }
    
    // Status filter
    match query.status.as_deref() {
        Some("active") => filter.disabled = Some(false),
        Some("disabled") => filter.disabled = Some(true),
        _ => {}
    }

    let users = state.user_metadata_repo
        .list(filter)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list users: {}", e);
            internal_error("Failed to list users")
        })?;

    let total = users.len() as i64; // Note: In production, use a count query
    
    let user_responses: Vec<AdminUserResponse> = users
        .into_iter()
        .map(|u| AdminUserResponse {
            id: u.id.to_string(),
            username: u.username,
            email: u.email,
            display_name: u.display_name,
            is_admin: u.is_admin,
            storage_quota_bytes: u.storage_quota_bytes,
            disabled_at: u.disabled_at,
            created_at: u.created_at,
            updated_at: u.updated_at,
        })
        .collect();

    Ok(Json(PaginatedUsers {
        users: user_responses,
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

    // Check if email already exists
    if let Some(_) = state.user_metadata_repo
        .get_by_email(&req.email)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check email: {}", e);
            internal_error("Failed to create user")
        })? 
    {
        return Err(bad_request("Email already in use"));
    }

    let new_id = Uuid::new_v4();
    let password_hash = PasswordHasher::hash(&req.password)
        .map_err(|e| {
            tracing::error!("Failed to hash password: {}", e);
            internal_error("Failed to create user")
        })?;

    let user = UserDocument::new(
        new_id,
        req.username.trim().to_string(),
        req.display_name.unwrap_or_else(|| req.username.trim().to_string()),
        req.email.to_lowercase().trim().to_string(),
        password_hash,
        req.is_admin.unwrap_or(false),
        req.storage_quota_bytes.unwrap_or(10_737_418_240), // 10GB default
    );

    state.user_metadata_repo
        .create(&user)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create user: {}", e);
            internal_error("Failed to create user")
        })?;

    log_admin_action(
        &State(state),
        actor_id,
        "user.created",
        Some("user"),
        Some(new_id),
        json!({"username": user.username, "email": user.email}),
    )
    .await;

    Ok((StatusCode::CREATED, Json(AdminUserResponse {
        id: user.id.to_string(),
        username: user.username,
        email: user.email,
        display_name: user.display_name,
        is_admin: user.is_admin,
        storage_quota_bytes: user.storage_quota_bytes,
        disabled_at: user.disabled_at,
        created_at: user.created_at,
        updated_at: user.updated_at,
    })))
}

/// GET /api/v1/admin/users/:id
pub async fn get_admin_user(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(user_id): Path<Uuid>,
) -> Result<Json<AdminUserDetailResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user = state.user_metadata_repo
        .get(user_id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get user: {}", e);
            internal_error("Failed to get user")
        })?
        .ok_or_else(|| not_found("User not found"))?;

    // TODO: Calculate actual storage used
    let storage_used_bytes = 0i64;

    Ok(Json(AdminUserDetailResponse {
        user: AdminUserResponse {
            id: user.id.to_string(),
            username: user.username,
            email: user.email,
            display_name: user.display_name,
            is_admin: user.is_admin,
            storage_quota_bytes: user.storage_quota_bytes,
            disabled_at: user.disabled_at,
            created_at: user.created_at,
            updated_at: user.updated_at,
        },
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
    let mut user = state.user_metadata_repo
        .get(user_id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get user: {}", e);
            internal_error("Failed to update user")
        })?
        .ok_or_else(|| not_found("User not found"))?;

    // Apply updates
    if let Some(display_name) = req.display_name {
        user.display_name = display_name;
    }
    
    if let Some(email) = req.email {
        if !email.contains('@') {
            return Err(bad_request("Invalid email address"));
        }
        // Check if email is already used by another user
        if let Some(existing) = state.user_metadata_repo
            .get_by_email(&email)
            .await
            .map_err(|e| {
                tracing::error!("Failed to check email: {}", e);
                internal_error("Failed to update user")
            })? 
        {
            if existing.id != user.id {
                return Err(bad_request("Email already in use"));
            }
        }
        user.email = email.to_lowercase().trim().to_string();
    }
    
    if let Some(quota) = req.storage_quota_bytes {
        user.storage_quota_bytes = quota;
    }
    
    if let Some(is_admin) = req.is_admin {
        user.is_admin = is_admin;
    }

    user.bump_version();

    state.user_metadata_repo
        .update(&user)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update user: {}", e);
            internal_error("Failed to update user")
        })?;

    log_admin_action(
        &State(state),
        actor_id,
        "user.updated",
        Some("user"),
        Some(user_id),
        json!({}),
    )
    .await;

    Ok(Json(AdminUserResponse {
        id: user.id.to_string(),
        username: user.username,
        email: user.email,
        display_name: user.display_name,
        is_admin: user.is_admin,
        storage_quota_bytes: user.storage_quota_bytes,
        disabled_at: user.disabled_at,
        created_at: user.created_at,
        updated_at: user.updated_at,
    }))
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

    let mut user = state.user_metadata_repo
        .get(user_id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get user: {}", e);
            internal_error("Failed to disable user")
        })?
        .ok_or_else(|| not_found("User not found"))?;

    user.disable(Some("Disabled by admin".to_string()));

    state.user_metadata_repo
        .update(&user)
        .await
        .map_err(|e| {
            tracing::error!("Failed to disable user: {}", e);
            internal_error("Failed to disable user")
        })?;

    log_admin_action(
        &State(state),
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
    let mut user = state.user_metadata_repo
        .get(user_id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get user: {}", e);
            internal_error("Failed to enable user")
        })?
        .ok_or_else(|| not_found("User not found"))?;

    user.enable();

    state.user_metadata_repo
        .update(&user)
        .await
        .map_err(|e| {
            tracing::error!("Failed to enable user: {}", e);
            internal_error("Failed to enable user")
        })?;

    log_admin_action(
        &State(state),
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

    // Check user exists
    let _user = state.user_metadata_repo
        .get(user_id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get user: {}", e);
            internal_error("Failed to delete user")
        })?
        .ok_or_else(|| not_found("User not found"))?;

    // TODO: Clean up user data (files, shares, etc.) or mark for async cleanup

    state.user_metadata_repo
        .delete(user_id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete user: {}", e);
            internal_error("Failed to delete user")
        })?;

    log_admin_action(
        &State(state),
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
