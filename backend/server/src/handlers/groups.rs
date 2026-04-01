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

    // Verify user is a member of the group
    let is_member = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM group_members WHERE group_id = $1 AND user_id = $2)",
    )
    .bind(req.group_id)
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

    // Get group name
    let group_name = sqlx::query_as::<_, GroupNameRow>(
        "SELECT name FROM user_groups WHERE id = $1",
    )
    .bind(req.group_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get group name: {}", e);
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

    // Check if share already exists
    let existing_share = sqlx::query_as::<_, (Uuid,)>(
        r#"
        SELECT id FROM shares
        WHERE file_id = $1
          AND recipient_group_id = $2
          AND revoked_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(file_id)
    .bind(req.group_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check existing share: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to check existing share" })),
        )
    })?;

    let share_id;
    let created_at;

    if let Some((existing_id,)) = existing_share {
        // Update existing share permission
        share_id = existing_id;
        let now = chrono::Utc::now();
        sqlx::query(
            r#"
            UPDATE shares
            SET permissions = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(format!("{:?}", permission))
        .bind(now)
        .bind(share_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update share" })),
            )
        })?;
        created_at = now;
    } else {
        // Create new share
        share_id = Uuid::new_v4();
        created_at = chrono::Utc::now();
        sqlx::query(
            r#"
            INSERT INTO shares (
                id, file_id, recipient_group_id, permissions,
                created_by, created_at, upload_only, access_count, tenant_id
            )
            SELECT $1, $2, $3, $4, $5, $6, false, 0, f.tenant_id
            FROM files f
            WHERE f.id = $2
            "#,
        )
        .bind(share_id)
        .bind(file_id)
        .bind(req.group_id)
        .bind(format!("{:?}", permission))
        .bind(auth.user_id)
        .bind(created_at)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to create share" })),
            )
        })?;
    }

    let response = GroupShareResponse {
        share_id: share_id.to_string(),
        resource_id: file_id.to_string(),
        resource_type: "file".to_string(),
        group_id: req.group_id.to_string(),
        group_name: group_name.name,
        permission: format!("{:?}", permission),
        created_at: created_at.to_rfc3339(),
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

    // Verify user is a member of the group
    let is_member = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM group_members WHERE group_id = $1 AND user_id = $2)",
    )
    .bind(req.group_id)
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

    // Get group name
    let group_name = sqlx::query_as::<_, GroupNameRow>(
        "SELECT name FROM user_groups WHERE id = $1",
    )
    .bind(req.group_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get group name: {}", e);
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

    // Check if share already exists
    let existing_share = sqlx::query_as::<_, (Uuid,)>(
        r#"
        SELECT id FROM shares
        WHERE folder_id = $1
          AND recipient_group_id = $2
          AND revoked_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(folder_id)
    .bind(req.group_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check existing share: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to check existing share" })),
        )
    })?;

    let share_id;
    let created_at;

    if let Some((existing_id,)) = existing_share {
        // Update existing share permission
        share_id = existing_id;
        let now = chrono::Utc::now();
        sqlx::query(
            r#"
            UPDATE shares
            SET permissions = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(format!("{:?}", permission))
        .bind(now)
        .bind(share_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update share" })),
            )
        })?;
        created_at = now;
    } else {
        // Create new share
        share_id = Uuid::new_v4();
        created_at = chrono::Utc::now();
        sqlx::query(
            r#"
            INSERT INTO shares (
                id, folder_id, recipient_group_id, permissions,
                created_by, created_at, upload_only, access_count, tenant_id
            )
            SELECT $1, $2, $3, $4, $5, $6, false, 0, f.tenant_id
            FROM folders f
            WHERE f.id = $2
            "#,
        )
        .bind(share_id)
        .bind(folder_id)
        .bind(req.group_id)
        .bind(format!("{:?}", permission))
        .bind(auth.user_id)
        .bind(created_at)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to create share" })),
            )
        })?;
    }

    let response = GroupShareResponse {
        share_id: share_id.to_string(),
        resource_id: folder_id.to_string(),
        resource_type: "folder".to_string(),
        group_id: req.group_id.to_string(),
        group_name: group_name.name,
        permission: format!("{:?}", permission),
        created_at: created_at.to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(response)))
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
