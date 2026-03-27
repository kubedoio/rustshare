//! Admin group management handlers.
//!
//! TODO: This module needs to be rewritten to use the new RustFS-based
//! repositories for group management instead of PostgreSQL.

use axum::{
    extract::{Path, State},
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
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct GroupResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub member_count: i64,
}

#[derive(Debug, Serialize)]
pub struct GroupDetailResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub members: Vec<GroupMemberResponse>,
}

#[derive(Debug, Serialize)]
pub struct GroupMemberResponse {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub added_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/groups
/// 
/// TODO: Implement using new GroupRepository
pub async fn list_groups(
    State(_state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<Vec<GroupResponse>>, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new GroupRepository
    tracing::warn!("Group list not yet implemented in zero-PostgreSQL mode");
    
    // Return empty list for now
    Ok(Json(vec![]))
}

/// POST /api/v1/admin/groups
/// 
/// TODO: Implement using new GroupRepository
pub async fn create_group(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(_req): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<GroupResponse>), (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new GroupRepository
    tracing::warn!("Group creation not yet implemented in zero-PostgreSQL mode");
    
    // Log admin action (noop for now)
    log_admin_action(
        actor_id,
        "group.created",
        Some("group"),
        Some(Uuid::new_v4()),
        json!({"name": "placeholder"}),
    )
    .await;

    Err(not_implemented("Group management not yet implemented"))
}

/// GET /api/v1/admin/groups/:id
/// 
/// TODO: Implement using new GroupRepository
pub async fn get_group(
    State(_state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(_group_id): Path<Uuid>,
) -> Result<Json<GroupDetailResponse>, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new GroupRepository
    tracing::warn!("Group get not yet implemented in zero-PostgreSQL mode");
    
    Err(not_found("Group not found"))
}

/// PATCH /api/v1/admin/groups/:id
/// 
/// TODO: Implement using new GroupRepository
pub async fn update_group(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(group_id): Path<Uuid>,
    Json(_req): Json<UpdateGroupRequest>,
) -> Result<Json<GroupResponse>, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new GroupRepository
    tracing::warn!("Group update not yet implemented in zero-PostgreSQL mode");
    
    // Log admin action (noop for now)
    log_admin_action(
        actor_id,
        "group.updated",
        Some("group"),
        Some(group_id),
        json!({}),
    )
    .await;

    Err(not_found("Group not found"))
}

/// DELETE /api/v1/admin/groups/:id
/// 
/// TODO: Implement using new GroupRepository
pub async fn delete_group(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(group_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new GroupRepository
    tracing::warn!("Group deletion not yet implemented in zero-PostgreSQL mode");
    
    // Log admin action (noop for now)
    log_admin_action(
        actor_id,
        "group.deleted",
        Some("group"),
        Some(group_id),
        json!({}),
    )
    .await;

    Err(not_found("Group not found"))
}

/// POST /api/v1/admin/groups/:id/members
/// 
/// TODO: Implement using new GroupRepository
pub async fn add_member(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(group_id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new GroupRepository
    tracing::warn!("Group member add not yet implemented in zero-PostgreSQL mode");
    
    // Log admin action (noop for now)
    log_admin_action(
        actor_id,
        "group.member_added",
        Some("group"),
        Some(group_id),
        json!({"user_id": req.user_id}),
    )
    .await;

    Err(not_found("Group not found"))
}

/// DELETE /api/v1/admin/groups/:id/members/:user_id
/// 
/// TODO: Implement using new GroupRepository
pub async fn remove_member(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path((group_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new GroupRepository
    tracing::warn!("Group member remove not yet implemented in zero-PostgreSQL mode");
    
    // Log admin action (noop for now)
    log_admin_action(
        actor_id,
        "group.member_removed",
        Some("group"),
        Some(group_id),
        json!({"user_id": user_id}),
    )
    .await;

    Err(not_found("Membership not found"))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn not_found(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": msg })))
}

fn not_implemented(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({ "error": msg })),
    )
}
