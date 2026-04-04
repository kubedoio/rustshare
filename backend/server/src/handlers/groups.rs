//! HTTP handlers for user group operations.
//!
//! This module implements endpoints for regular users to view their groups
//! and share resources with groups they belong to.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{handlers::AuthenticatedUser, AppState};

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Response for a group member.
#[derive(Debug, Serialize)]
pub struct GroupMemberResponse {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub added_at: chrono::DateTime<chrono::Utc>,
}

/// Response for a group.
#[derive(Debug, Serialize)]
pub struct UserGroupResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub member_count: i64,
}

/// Response for a group with members.
#[derive(Debug, Serialize)]
pub struct UserGroupDetailResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub members: Vec<GroupMemberResponse>,
}

/// Request to create a group share for a file.
#[derive(Debug, Deserialize)]
pub struct CreateFileGroupShareRequest {
    pub group_id: Uuid,
    pub permission: String, // "View", "Edit", or "Admin"
}

/// Request to create a group share for a folder.
#[derive(Debug, Deserialize)]
pub struct CreateFolderGroupShareRequest {
    pub group_id: Uuid,
    pub permission: String, // "View", "Edit", or "Admin"
}

/// Request to update a group share permission.
#[derive(Debug, Deserialize)]
pub struct UpdateGroupShareRequest {
    pub permission: String,
}

/// Response for a created group share.
#[derive(Debug, Serialize)]
pub struct GroupShareResponse {
    pub share_id: String,
    pub resource_id: String,
    pub resource_type: String,
    pub group_id: String,
    pub group_name: String,
    pub permission: String,
    pub created_at: String,
}

// ============================================================================
// Internal row types
// ============================================================================

#[derive(sqlx::FromRow)]
struct GroupRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    member_count: i64,
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    user_id: Uuid,
    username: String,
    email: String,
    added_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct GroupNameRow {
    name: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/v1/groups/my
///
/// List all groups that the authenticated user is a member of.
pub async fn list_my_groups(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<UserGroupResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let rows = sqlx::query_as::<_, GroupRow>(
        r#"
        SELECT
            g.id,
            g.name,
            g.description,
            g.created_at,
            COUNT(m2.id) AS member_count
        FROM user_groups g
        JOIN group_members m ON m.group_id = g.id
        LEFT JOIN group_members m2 ON m2.group_id = g.id
        WHERE m.user_id = $1
        GROUP BY g.id
        ORDER BY g.name ASC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list user groups: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to load groups" })),
        )
    })?;

    let groups = rows
        .into_iter()
        .map(|r| UserGroupResponse {
            id: r.id.to_string(),
            name: r.name,
            description: r.description,
            created_at: r.created_at,
            member_count: r.member_count,
        })
        .collect();

    Ok(Json(groups))
}

/// GET /api/v1/groups/my/:id
///
/// Get details of a specific group the user is a member of, including members.
pub async fn get_my_group(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(group_id): Path<Uuid>,
) -> Result<Json<UserGroupDetailResponse>, (StatusCode, Json<serde_json::Value>)> {
    // First verify the user is a member of this group
    let is_member = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM group_members WHERE group_id = $1 AND user_id = $2)",
    )
    .bind(group_id)
    .bind(auth.user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check group membership: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to verify group access" })),
        )
    })?;

    if !is_member {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "You are not a member of this group" })),
        ));
    }

    // Get group details
    let group = sqlx::query_as::<_, (Uuid, String, Option<String>, chrono::DateTime<chrono::Utc>)>(
        "SELECT id, name, description, created_at FROM user_groups WHERE id = $1",
    )
    .bind(group_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get group: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to load group" })),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Group not found" })),
        )
    })?;

    // Get members
    let members = sqlx::query_as::<_, MemberRow>(
        r#"
        SELECT u.id AS user_id, u.username, u.email, m.added_at
        FROM group_members m
        JOIN users u ON u.id = m.user_id
        WHERE m.group_id = $1
        ORDER BY m.added_at ASC
        "#,
    )
    .bind(group_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get group members: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to load group members" })),
        )
    })?;

    Ok(Json(UserGroupDetailResponse {
        id: group.0.to_string(),
        name: group.1,
        description: group.2,
        created_at: group.3,
        members: members
            .into_iter()
            .map(|m| GroupMemberResponse {
                user_id: m.user_id.to_string(),
                username: m.username,
                email: m.email,
                added_at: m.added_at,
            })
            .collect(),
    }))
}

/// POST /api/v1/files/:id/share/group
///
/// Share a file with a group the user belongs to.
pub async fn create_file_group_share(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<CreateFileGroupShareRequest>,
) -> Result<(StatusCode, Json<GroupShareResponse>), (StatusCode, Json<serde_json::Value>)> {
    use rustshare_core::domain::SharePermissions;
    use rustshare_core::services::ShareResource;
    use rustshare_core::services::ShareError;

    // Parse permission
    let permission = match req.permission.as_str() {
        "View" => SharePermissions::View,
        "Edit" => SharePermissions::Edit,
        "Admin" => SharePermissions::Admin,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid permission. Must be View, Edit, or Admin" })),
            ));
        }
    };

    // Create group share via service
    let share = state.share_service
        .create_group_share(
            ShareResource::File(file_id),
            req.group_id,
            permission,
            auth.user_id,
            auth.tenant_id,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to create group share: {}", e);
            match e {
                ShareError::FileNotFound(_) => (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "File not found" })),
                ),
                ShareError::NotFoundById(_) => (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Resource not found" })),
                ),
                ShareError::GroupNotFound(_) => (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Group not found" })),
                ),
                ShareError::CrossTenantSharingNotAllowed => (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "Cross-tenant sharing not allowed" })),
                ),
                ShareError::NotGroupMember(_) => (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "You must be a group member to share" })),
                ),
                ShareError::GroupShareAlreadyExists => (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({ "error": "Group already has access" })),
                ),
                ShareError::InsufficientPermission { .. } => (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "Admin permission required" })),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to create share" })),
                ),
            }
        })?;

    // Get group name for response
    let group_name = sqlx::query_scalar::<_, String>(
        "SELECT name FROM user_groups WHERE id = $1"
    )
    .bind(req.group_id)
    .fetch_one(&state.db_pool)
    .await
    .unwrap_or_else(|_| "Unknown".to_string());

    let response = GroupShareResponse {
        share_id: share.id.to_string(),
        resource_id: file_id.to_string(),
        resource_type: "file".to_string(),
        group_id: req.group_id.to_string(),
        group_name,
        permission: format!("{:?}", permission),
        created_at: share.created_at.to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// POST /api/v1/folders/:id/share/group
///
/// Share a folder with a group the user belongs to.
pub async fn create_folder_group_share(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
    Json(req): Json<CreateFolderGroupShareRequest>,
) -> Result<(StatusCode, Json<GroupShareResponse>), (StatusCode, Json<serde_json::Value>)> {
    use rustshare_core::domain::SharePermissions;
    use rustshare_core::services::ShareResource;
    use rustshare_core::services::ShareError;

    // Parse permission
    let permission = match req.permission.as_str() {
        "View" => SharePermissions::View,
        "Edit" => SharePermissions::Edit,
        "Admin" => SharePermissions::Admin,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid permission. Must be View, Edit, or Admin" })),
            ));
        }
    };

    // Create group share via service
    let share = state.share_service
        .create_group_share(
            ShareResource::Folder(folder_id),
            req.group_id,
            permission,
            auth.user_id,
            auth.tenant_id,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to create group share: {}", e);
            match e {
                ShareError::NotFoundById(_) => (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Folder not found" })),
                ),
                ShareError::GroupNotFound(_) => (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Group not found" })),
                ),
                ShareError::CrossTenantSharingNotAllowed => (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "Cross-tenant sharing not allowed" })),
                ),
                ShareError::NotGroupMember(_) => (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "You must be a group member to share" })),
                ),
                ShareError::GroupShareAlreadyExists => (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({ "error": "Group already has access" })),
                ),
                ShareError::InsufficientPermission { .. } => (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "Admin permission required" })),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to create share" })),
                ),
            }
        })?;

    // Get group name for response
    let group_name = sqlx::query_scalar::<_, String>(
        "SELECT name FROM user_groups WHERE id = $1"
    )
    .bind(req.group_id)
    .fetch_one(&state.db_pool)
    .await
    .unwrap_or_else(|_| "Unknown".to_string());

    let response = GroupShareResponse {
        share_id: share.id.to_string(),
        resource_id: folder_id.to_string(),
        resource_type: "folder".to_string(),
        group_id: req.group_id.to_string(),
        group_name,
        permission: format!("{:?}", permission),
        created_at: share.created_at.to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// DELETE /api/v1/shares/:id/group
///
/// Revoke a group share.
pub async fn revoke_group_share(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(share_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    use rustshare_core::services::ShareError;

    state.share_service
        .revoke_group_share(share_id, auth.user_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to revoke group share: {}", e);
            match e {
                ShareError::NotFoundById(_) => (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Share not found" })),
                ),
                ShareError::InvalidState(_) => (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Not a group share" })),
                ),
                ShareError::InsufficientPermission { .. } => (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "Admin permission required" })),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to revoke share" })),
                ),
            }
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// PUT /api/v1/shares/:id/group/permission
///
/// Update a group share's permission.
pub async fn update_group_share_permission(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(share_id): Path<Uuid>,
    Json(req): Json<UpdateGroupShareRequest>,
) -> Result<Json<GroupShareResponse>, (StatusCode, Json<serde_json::Value>)> {
    use rustshare_core::domain::SharePermissions;
    use rustshare_core::services::ShareError;

    let permission = match req.permission.as_str() {
        "View" => SharePermissions::View,
        "Edit" => SharePermissions::Edit,
        "Admin" => SharePermissions::Admin,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid permission" })),
            ));
        }
    };

    let share = state.share_service
        .update_group_share_permission(share_id, permission, auth.user_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update group share: {}", e);
            match e {
                ShareError::NotFoundById(_) => (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Share not found" })),
                ),
                ShareError::InvalidState(_) => (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": "Not a group share" })),
                ),
                ShareError::InsufficientPermission { .. } => (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "Admin permission required" })),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to update share" })),
                ),
            }
        })?;

    // Get group name
    let group_name = if let Some(group_id) = share.recipient_group_id {
        sqlx::query_scalar::<_, String>(
            "SELECT name FROM user_groups WHERE id = $1"
        )
        .bind(group_id)
        .fetch_one(&state.db_pool)
        .await
        .unwrap_or_else(|_| "Unknown".to_string())
    } else {
        "Unknown".to_string()
    };

    let resource_id = share.file_id.or(share.folder_id).unwrap_or_default();
    let resource_type = if share.file_id.is_some() { "file" } else { "folder" };

    let response = GroupShareResponse {
        share_id: share.id.to_string(),
        resource_id: resource_id.to_string(),
        resource_type: resource_type.to_string(),
        group_id: share.recipient_group_id.unwrap_or_default().to_string(),
        group_name,
        permission: format!("{:?}", share.permissions),
        created_at: share.created_at.to_rfc3339(),
    };

    Ok(Json(response))
}

/// GET /api/v1/files/:id/share/groups
///
/// List all group shares for a file.
pub async fn list_file_group_shares(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Json<Vec<GroupShareResponse>>, (StatusCode, Json<serde_json::Value>)> {
    // Check if user has permission to view shares (must be owner or admin)
    let has_permission = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM files WHERE id = $1 AND owner_id = $2
            UNION
            SELECT 1 FROM shares
            WHERE file_id = $1
              AND recipient_user_id = $2
              AND permissions = 'Admin'
              AND revoked_at IS NULL
        )
        "#,
    )
    .bind(file_id)
    .bind(auth.user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check permission: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to check permission" })),
        )
    })?;

    if !has_permission {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "You do not have permission to view shares for this file" })),
        ));
    }

    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"
        SELECT s.id, g.id as group_id, g.name, s.permissions, s.created_at
        FROM shares s
        JOIN user_groups g ON g.id = s.recipient_group_id
        WHERE s.file_id = $1
          AND s.recipient_group_id IS NOT NULL
          AND s.revoked_at IS NULL
        ORDER BY s.created_at ASC
        "#,
    )
    .bind(file_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list group shares: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to load group shares" })),
        )
    })?;

    let shares = rows
        .into_iter()
        .map(|(share_id, group_id, group_name, permission, created_at)| GroupShareResponse {
            share_id: share_id.to_string(),
            resource_id: file_id.to_string(),
            resource_type: "file".to_string(),
            group_id: group_id.to_string(),
            group_name,
            permission,
            created_at: created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(shares))
}

/// GET /api/v1/folders/:id/share/groups
///
/// List all group shares for a folder.
pub async fn list_folder_group_shares(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<Json<Vec<GroupShareResponse>>, (StatusCode, Json<serde_json::Value>)> {
    // Check if user has permission to view shares (must be owner or admin)
    let has_permission = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM folders WHERE id = $1 AND owner_id = $2
            UNION
            SELECT 1 FROM shares
            WHERE folder_id = $1
              AND recipient_user_id = $2
              AND permissions = 'Admin'
              AND revoked_at IS NULL
        )
        "#,
    )
    .bind(folder_id)
    .bind(auth.user_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check permission: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to check permission" })),
        )
    })?;

    if !has_permission {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "You do not have permission to view shares for this folder" })),
        ));
    }

    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"
        SELECT s.id, g.id as group_id, g.name, s.permissions, s.created_at
        FROM shares s
        JOIN user_groups g ON g.id = s.recipient_group_id
        WHERE s.folder_id = $1
          AND s.recipient_group_id IS NOT NULL
          AND s.revoked_at IS NULL
        ORDER BY s.created_at ASC
        "#,
    )
    .bind(folder_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to list group shares: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to load group shares" })),
        )
    })?;

    let shares = rows
        .into_iter()
        .map(|(share_id, group_id, group_name, permission, created_at)| GroupShareResponse {
            share_id: share_id.to_string(),
            resource_id: folder_id.to_string(),
            resource_type: "folder".to_string(),
            group_id: group_id.to_string(),
            group_name,
            permission,
            created_at: created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(shares))
}
