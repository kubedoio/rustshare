//! Admin group management handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use rustshare_storage::metadata_v2::schemas::UserGroupDocument;

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
pub async fn list_groups(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<Vec<GroupResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let groups = state.group_repo
        .list()
        .await
        .map_err(|e| {
            tracing::error!("Failed to list groups: {}", e);
            internal_error("Failed to list groups")
        })?;

    let group_responses: Vec<GroupResponse> = groups
        .into_iter()
        .map(|g| GroupResponse {
            id: g.id.to_string(),
            name: g.name,
            description: if g.description.is_empty() { None } else { Some(g.description) },
            created_by: Some(g.created_by.to_string()),
            created_at: g.created_at,
            updated_at: g.updated_at,
            member_count: g.member_ids.len() as i64,
        })
        .collect();

    Ok(Json(group_responses))
}

/// POST /api/v1/admin/groups
pub async fn create_group(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<GroupResponse>), (StatusCode, Json<serde_json::Value>)> {
    if req.name.trim().is_empty() {
        return Err(bad_request("Group name must not be empty"));
    }

    let group_id = Uuid::new_v4();
    let group = UserGroupDocument::new(
        group_id,
        req.name.trim().to_string(),
        req.description.unwrap_or_default(),
        actor_id,
    );

    state.group_repo
        .create(&group)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create group: {}", e);
            internal_error("Failed to create group")
        })?;

    log_admin_action(
        &State(state),
        actor_id,
        "group.created",
        Some("group"),
        Some(group_id),
        json!({"name": group.name}),
    )
    .await;

    Ok((StatusCode::CREATED, Json(GroupResponse {
        id: group.id.to_string(),
        name: group.name,
        description: if group.description.is_empty() { None } else { Some(group.description) },
        created_by: Some(group.created_by.to_string()),
        created_at: group.created_at,
        updated_at: group.updated_at,
        member_count: 0,
    })))
}

/// GET /api/v1/admin/groups/:id
pub async fn get_group(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(group_id): Path<Uuid>,
) -> Result<Json<GroupDetailResponse>, (StatusCode, Json<serde_json::Value>)> {
    let group = state.group_repo
        .get(group_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get group: {}", e);
            internal_error("Failed to get group")
        })?
        .ok_or_else(|| not_found("Group not found"))?;

    // Get member details
    let mut members = Vec::new();
    for member_id in &group.member_ids {
        if let Ok(Some(user)) = state.user_metadata_repo.get((*member_id).into()).await {
            members.push(GroupMemberResponse {
                user_id: user.id.to_string(),
                username: user.username,
                email: user.email,
                added_at: chrono::Utc::now(), // TODO: Track when user was added to group
            });
        }
    }

    Ok(Json(GroupDetailResponse {
        id: group.id.to_string(),
        name: group.name,
        description: if group.description.is_empty() { None } else { Some(group.description) },
        created_by: Some(group.created_by.to_string()),
        created_at: group.created_at,
        updated_at: group.updated_at,
        members,
    }))
}

/// PATCH /api/v1/admin/groups/:id
pub async fn update_group(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(group_id): Path<Uuid>,
    Json(req): Json<UpdateGroupRequest>,
) -> Result<Json<GroupResponse>, (StatusCode, Json<serde_json::Value>)> {
    let mut group = state.group_repo
        .get(group_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get group: {}", e);
            internal_error("Failed to update group")
        })?
        .ok_or_else(|| not_found("Group not found"))?;

    // Apply updates
    if let Some(name) = req.name {
        if name.trim().is_empty() {
            return Err(bad_request("Group name must not be empty"));
        }
        group.name = name.trim().to_string();
    }
    
    if let Some(description) = req.description {
        group.description = description;
    }

    state.group_repo
        .update(&group)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update group: {}", e);
            internal_error("Failed to update group")
        })?;

    log_admin_action(
        &State(state),
        actor_id,
        "group.updated",
        Some("group"),
        Some(group_id),
        json!({}),
    )
    .await;

    Ok(Json(GroupResponse {
        id: group.id.to_string(),
        name: group.name,
        description: if group.description.is_empty() { None } else { Some(group.description) },
        created_by: Some(group.created_by.to_string()),
        created_at: group.created_at,
        updated_at: group.updated_at,
        member_count: group.member_ids.len() as i64,
    }))
}

/// DELETE /api/v1/admin/groups/:id
pub async fn delete_group(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(group_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Check group exists
    let _group = state.group_repo
        .get(group_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get group: {}", e);
            internal_error("Failed to delete group")
        })?
        .ok_or_else(|| not_found("Group not found"))?;

    state.group_repo
        .delete(group_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete group: {}", e);
            internal_error("Failed to delete group")
        })?;

    log_admin_action(
        &State(state),
        actor_id,
        "group.deleted",
        Some("group"),
        Some(group_id),
        json!({}),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/admin/groups/:id/members
pub async fn add_member(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(group_id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Verify group exists
    let _group = state.group_repo
        .get(group_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get group: {}", e);
            internal_error("Failed to add member")
        })?
        .ok_or_else(|| not_found("Group not found"))?;

    // Verify user exists
    let _user = state.user_metadata_repo
        .get(req.user_id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get user: {}", e);
            internal_error("Failed to add member")
        })?
        .ok_or_else(|| not_found("User not found"))?;

    state.group_repo
        .add_member(group_id, req.user_id.into(), actor_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to add member: {}", e);
            internal_error("Failed to add member")
        })?;

    log_admin_action(
        &State(state),
        actor_id,
        "group.member_added",
        Some("group"),
        Some(group_id),
        json!({"user_id": req.user_id}),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/admin/groups/:id/members/:user_id
pub async fn remove_member(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path((group_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Verify group exists
    let _group = state.group_repo
        .get(group_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get group: {}", e);
            internal_error("Failed to remove member")
        })?
        .ok_or_else(|| not_found("Group not found"))?;

    state.group_repo
        .remove_member(group_id, user_id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to remove member: {}", e);
            internal_error("Failed to remove member")
        })?;

    log_admin_action(
        &State(state),
        actor_id,
        "group.member_removed",
        Some("group"),
        Some(group_id),
        json!({"user_id": user_id}),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
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
