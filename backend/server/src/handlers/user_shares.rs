//! HTTP handlers for user share operations.
//!
//! This module implements endpoints for sharing files and folders with specific users,
//! managing share permissions, and listing shared resources.

// Allow deprecated UserShareService usage during migration period
#![allow(deprecated)]

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use rustshare_core::{domain::SharePermissions, services::Resource};

use super::{AppError, AuthenticatedUser};
use crate::AppState;

// Re-export folder/file with shares types from folders handler
use super::folders::{
    FolderContentsWithShares, FolderTreeNode, FolderTreeWithShares, FolderWithShares,
};
// removed unused import

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Request to create a file share with a specific user.
#[derive(Debug, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct CreateFileShareRequest {
    /// Email of the recipient user.
    #[validate(email(message = "Invalid recipient email address"))]
    pub recipient_email: String,
    /// Permission level to grant.
    pub permission: SharePermissions,
}

/// Request to create a folder share with a specific user.
#[derive(Debug, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct CreateFolderShareRequest {
    /// Email of the recipient user.
    #[validate(email(message = "Invalid recipient email address"))]
    pub recipient_email: String,
    /// Permission level to grant.
    pub permission: SharePermissions,
}

/// Response for a created or updated share.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UserShareResponse {
    pub share_id: Uuid,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub recipient_email: String,
    pub permission: SharePermissions,
    pub created_at: String,
}

/// Response for a received share.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReceivedShareResponse {
    pub share_id: Uuid,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub resource_name: String,
    pub resource_path: String,
    pub permission: SharePermissions,
    pub shared_by: Uuid,
    pub shared_by_name: String,
    pub shared_by_email: String,
    pub created_at: String,
}

/// Response for a share recipient.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ShareRecipientResponse {
    pub share_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub permission: SharePermissions,
    pub added_at: String,
    pub added_by: Uuid,
}

/// Request to update a recipient's permission.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdatePermissionRequest {
    pub permission: SharePermissions,
}

/// Query parameters for listing received shares.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ListReceivedSharesQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

// ============================================================================
// 1. POST /api/files/{id}/share - Create file share
// ============================================================================

/// Create a share for a file with a specific user.
///
/// POST /api/files/{id}/share
///
/// Creates a user-specific share (not a public share link).
/// If a share already exists for this recipient, updates the permission level.
#[utoipa::path(
    post,
    path = "/api/v1/files/{id}/share",
    tag = "Admin",
    params(("file_id" = Uuid, Path, description = "File Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_file_share(
    State(state): State<AppState>,
    Path(file_id): Path<Uuid>,
    auth: AuthenticatedUser,
    crate::handlers::ValidatedJson(req): crate::handlers::ValidatedJson<CreateFileShareRequest>,
) -> Result<Response, AppError> {
    let share = state
        .user_share_service
        .create_file_share(file_id, &req.recipient_email, req.permission, auth.user_id)
        .await?;

    // Best-effort refresh of the AI index ACL for this file.
    if let Some(file_id) = share.file_id {
        let _ = state
            .note_service
            .refresh_note_index_acl(file_id, auth.user_id, auth.tenant_id)
            .await;
    }

    // Get recipient email for response (it was validated in the service)
    let response = UserShareResponse {
        share_id: share.id,
        resource_id: file_id,
        resource_type: "file".to_string(),
        recipient_email: req.recipient_email,
        permission: share.permissions,
        created_at: share.created_at.to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(response)).into_response())
}

// ============================================================================
// 2. POST /api/folders/{id}/share - Create folder share
// ============================================================================

/// Create a share for a folder with a specific user.
///
/// POST /api/folders/{id}/share
///
/// Creates a user-specific share (not a public share link).
/// If a share already exists for this recipient, updates the permission level.
#[utoipa::path(
    post,
    path = "/api/v1/folders/{id}/share",
    tag = "Admin",
    params(("folder_id" = Uuid, Path, description = "Folder Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_folder_share(
    State(state): State<AppState>,
    Path(folder_id): Path<Uuid>,
    auth: AuthenticatedUser,
    crate::handlers::ValidatedJson(req): crate::handlers::ValidatedJson<CreateFolderShareRequest>,
) -> Result<Response, AppError> {
    let share = state
        .user_share_service
        .create_folder_share(
            folder_id,
            &req.recipient_email,
            req.permission,
            auth.user_id,
        )
        .await?;

    let response = UserShareResponse {
        share_id: share.id,
        resource_id: folder_id,
        resource_type: "folder".to_string(),
        recipient_email: req.recipient_email,
        permission: share.permissions,
        created_at: share.created_at.to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(response)).into_response())
}

// ============================================================================
// 3. GET /api/shares/received - List received shares
// ============================================================================

/// List shares received by the authenticated user.
///
/// GET /api/shares/received?limit=50&offset=0
///
/// Returns paginated list of files and folders shared with the user.
#[utoipa::path(
    get,
    path = "/api/v1/shares/received",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_received_shares(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ListReceivedSharesQuery>,
) -> Result<Response, AppError> {
    let shares = state
        .user_share_service
        .list_received_shares(auth.user_id, auth.tenant_id, query.limit, query.offset)
        .await?;

    let mut response = Vec::with_capacity(shares.len());
    for share in shares {
        let resource_id = share.file_id.or(share.folder_id).unwrap_or_else(Uuid::nil);
        let resource_type = if share.file_id.is_some() {
            "file"
        } else {
            "folder"
        };

        let (resource_name, resource_path) = if let Some(file_id) = share.file_id {
            match state
                .metadata_store
                .find_file_by_id_unchecked(file_id)
                .await
            {
                Ok(Some(file)) => (file.name, file.path),
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!("failed to load shared file {}: {}", file_id, error);
                    continue;
                }
            }
        } else if let Some(folder_id) = share.folder_id {
            match state
                .metadata_store
                .find_folder_by_id_unchecked(folder_id)
                .await
            {
                Ok(Some(folder)) => (folder.name, folder.path),
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!("failed to load shared folder {}: {}", folder_id, error);
                    continue;
                }
            }
        } else {
            continue;
        };

        let (shared_by_name, shared_by_email) =
            match state.metadata_store.find_user_by_id(share.created_by).await {
                Ok(Some(user)) => (user.display_name, user.email),
                Ok(None) => ("Unknown user".to_string(), String::new()),
                Err(error) => {
                    tracing::warn!(
                        "failed to load share creator {}: {}",
                        share.created_by,
                        error
                    );
                    ("Unknown user".to_string(), String::new())
                }
            };

        response.push(ReceivedShareResponse {
            share_id: share.id,
            resource_id,
            resource_type: resource_type.to_string(),
            resource_name,
            resource_path,
            permission: share.permissions,
            shared_by: share.created_by,
            shared_by_name,
            shared_by_email,
            created_at: share.created_at.to_rfc3339(),
        });
    }

    Ok(Json(response).into_response())
}

// ============================================================================
// 4. GET /api/files/{id}/recipients - List file recipients (Admin only)
// ============================================================================

/// List all recipients of a shared file.
///
/// GET /api/files/{id}/recipients
///
/// Requires Admin permission on the file.
/// Returns list of users who have access to the file and their permission levels.
#[utoipa::path(
    get,
    path = "/api/v1/files/{id}/recipients",
    tag = "Admin",
    params(("file_id" = Uuid, Path, description = "File Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_file_recipients(
    State(state): State<AppState>,
    Path(file_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, AppError> {
    let recipients = state
        .user_share_service
        .list_recipients(Some(file_id), None, auth.user_id, auth.tenant_id)
        .await?;

    let response: Vec<ShareRecipientResponse> = recipients
        .into_iter()
        .map(|r| ShareRecipientResponse {
            share_id: r.share_id,
            user_id: r.user_id,
            email: r.email,
            permission: r.permission,
            added_at: r.added_at.to_rfc3339(),
            added_by: r.added_by,
        })
        .collect();

    Ok(Json(response).into_response())
}

// ============================================================================
// 5. GET /api/folders/{id}/recipients - List folder recipients (Admin only)
// ============================================================================

/// List all recipients of a shared folder.
///
/// GET /api/folders/{id}/recipients
///
/// Requires Admin permission on the folder.
/// Returns list of users who have access to the folder and their permission levels.
#[utoipa::path(
    get,
    path = "/api/v1/folders/{id}/recipients",
    tag = "Admin",
    params(("folder_id" = Uuid, Path, description = "Folder Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_folder_recipients(
    State(state): State<AppState>,
    Path(folder_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, AppError> {
    let recipients = state
        .user_share_service
        .list_recipients(None, Some(folder_id), auth.user_id, auth.tenant_id)
        .await?;

    let response: Vec<ShareRecipientResponse> = recipients
        .into_iter()
        .map(|r| ShareRecipientResponse {
            share_id: r.share_id,
            user_id: r.user_id,
            email: r.email,
            permission: r.permission,
            added_at: r.added_at.to_rfc3339(),
            added_by: r.added_by,
        })
        .collect();

    Ok(Json(response).into_response())
}

// ============================================================================
// 6. PUT /api/shares/{id}/permission - Update permission (Admin only)
// ============================================================================

/// Update the permission level for a share recipient.
///
/// PUT /api/shares/{id}/permission
///
/// Requires Admin permission on the shared resource.
/// Updates the permission level for the specified share.
#[utoipa::path(
    put,
    path = "/api/v1/shares/{id}/permission",
    tag = "Admin",
    params(("share_id" = Uuid, Path, description = "Share Id")),
    request_body = UpdatePermissionRequest,
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_recipient_permission(
    State(state): State<AppState>,
    Path(share_id): Path<Uuid>,
    auth: AuthenticatedUser,
    Json(req): Json<UpdatePermissionRequest>,
) -> Result<Response, AppError> {
    let updated_share = state
        .user_share_service
        .update_recipient_permission(share_id, req.permission, auth.user_id, auth.tenant_id)
        .await?;

    let resource_id = updated_share
        .file_id
        .or(updated_share.folder_id)
        .unwrap_or_else(Uuid::nil);
    let resource_type = if updated_share.file_id.is_some() {
        "file"
    } else {
        "folder"
    };

    let response = serde_json::json!({
        "share_id": updated_share.id,
        "resource_id": resource_id,
        "resource_type": resource_type,
        "permission": updated_share.permissions,
        "updated_at": updated_share.created_at.to_rfc3339(),
    });

    Ok(Json(response).into_response())
}

// ============================================================================
// 7. DELETE /api/shares/{id}/recipient - Remove recipient (Admin only)
// ============================================================================

/// Remove a recipient from a share (revoke access).
///
/// DELETE /api/shares/{id}/recipient
///
/// Requires Admin permission on the shared resource.
/// Revokes the share and removes the recipient's access.
#[utoipa::path(
    delete,
    path = "/api/v1/shares/{id}/recipient",
    tag = "Admin",
    params(("share_id" = Uuid, Path, description = "Share Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn remove_recipient(
    State(state): State<AppState>,
    Path(share_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, AppError> {
    state
        .user_share_service
        .remove_recipient(share_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok((StatusCode::NO_CONTENT, ()).into_response())
}

// ============================================================================
// 8. GET /api/shares/folders/{id}/contents - Get shared folder contents
// ============================================================================

/// Get contents of a shared folder (for authenticated user shares).
///
/// GET /api/shares/folders/{id}/contents
///
/// Returns the contents (subfolders and files) of a folder that has been shared
/// with the authenticated user via user-to-user sharing (not public links).
#[utoipa::path(
    get,
    path = "/api/v1/shares/folders/{id}/contents",
    tag = "Admin",
    params(("folder_id" = Uuid, Path, description = "Folder Id")),
    responses(
        (status = 200, description = "Success", body = FolderContentsWithShares),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_user_shared_folder_contents(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<Json<FolderContentsWithShares>, AppError> {
    // 1. Get contents via FolderService (which handles permissions and visibility)
    let current_folder_permission = state
        .permission_resolver
        .resolve_permission_with_source(auth.user_id, auth.tenant_id, Resource::Folder(folder_id))
        .await
        .map_err(|error| {
            tracing::error!(
                "failed to resolve permission for shared folder {} and user {}: {}",
                folder_id,
                auth.user_id,
                error
            );
            AppError::internal("failed to resolve permission")
        })?;

    let contents = state
        .folder_service
        .list_contents(folder_id, auth.user_id)
        .await?;

    // Fetch stored colors for child files in one query. The shared folder
    // may contain files owned by other users, so we read by id/tenant_id.
    let file_ids: Vec<Uuid> = contents.files.iter().map(|f| f.id).collect();
    let colors: HashMap<Uuid, Option<String>> = if file_ids.is_empty() {
        HashMap::new()
    } else {
        sqlx::query_as::<_, (Uuid, Option<String>)>(
            "SELECT id, color FROM files WHERE id = ANY($1) AND tenant_id = $2 AND deleted_at IS NULL",
        )
        .bind(&file_ids)
        .bind(auth.tenant_id)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| AppError::internal(format!("Failed to load file colors: {}", e)))?
        .into_iter()
        .collect()
    };

    // 2. Decorate folders with share information
    let mut folders_with_shares = Vec::with_capacity(contents.folders.len());
    for f in contents.folders {
        let share_info = load_folder_share_summary(&state.db_pool, f.id).await?;
        let size = load_folder_size(&state.db_pool, f.id).await?;
        let permission = state
            .permission_resolver
            .resolve_permission_with_source(auth.user_id, auth.tenant_id, Resource::Folder(f.id))
            .await
            .map_err(|error| {
                tracing::error!(
                    "failed to resolve permission for shared child folder {} and user {}: {}",
                    f.id,
                    auth.user_id,
                    error
                );
                AppError::internal("failed to resolve permission")
            })?;

        folders_with_shares.push(FolderWithShares {
            id: f.id,
            name: f.name,
            path: f.path,
            parent_folder_id: f.parent_folder_id,
            owner_id: f.owner_id,
            created_at: f.created_at,
            updated_at: f.updated_at,
            starred_at: f.starred_at,
            deleted_at: f.deleted_at,
            size,
            is_shared: share_info.0,
            share_count: share_info.1,
            share_expires_at: share_info.2,
            effective_permission: permission_to_string(permission.permission),
            note_bundle_file_id: None,
        });
    }

    // 3. Decorate files with share information
    let mut files_with_shares = Vec::with_capacity(contents.files.len());
    for f in contents.files {
        let share_info = load_file_share_summary(&state.db_pool, f.id).await?;
        let permission = state
            .permission_resolver
            .resolve_permission_with_source(auth.user_id, auth.tenant_id, Resource::File(f.id))
            .await
            .map_err(|error| {
                tracing::error!(
                    "failed to resolve permission for shared child file {} and user {}: {}",
                    f.id,
                    auth.user_id,
                    error
                );
                AppError::internal("failed to resolve permission")
            })?;

        files_with_shares.push(crate::handlers::files::FileWithShares {
            id: f.id,
            name: f.name,
            path: f.path,
            size: f.size,
            mime_type: f.mime_type,
            parent_folder_id: f.parent_folder_id,
            owner_id: f.owner_id,
            current_version: f.current_version,
            created_at: f.created_at,
            modified_at: f.modified_at,
            starred_at: f.starred_at,
            deleted_at: f.deleted_at,
            color: colors.get(&f.id).cloned().unwrap_or(None),
            is_shared: share_info.0,
            share_count: share_info.1,
            share_expires_at: share_info.2,
            effective_permission: permission_to_string(permission.permission),
        });
    }

    Ok(Json(FolderContentsWithShares {
        folders: folders_with_shares,
        files: files_with_shares,
        current_folder_permission: permission_to_string(current_folder_permission.permission),
    }))
}

/// Get recursive tree structure for a shared folder.
///
/// GET /api/shares/folders/{id}/tree
#[utoipa::path(
    get,
    path = "/api/v1/shares/folders/{id}/tree",
    tag = "Admin",
    params(("folder_id" = Uuid, Path, description = "Folder Id")),
    responses(
        (status = 200, description = "Success", body = FolderTreeWithShares),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_user_shared_folder_tree(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<Json<FolderTreeWithShares>, AppError> {
    let tree =
        build_user_shared_folder_tree(&state, auth.user_id, auth.tenant_id, folder_id).await?;
    Ok(Json(tree))
}

async fn build_user_shared_folder_tree(
    state: &AppState,
    user_id: Uuid,
    tenant_id: Uuid,
    folder_id: Uuid,
) -> Result<FolderTreeWithShares, AppError> {
    let folder = state.folder_service.get_folder(folder_id, user_id).await?;

    let share_info = load_folder_share_summary(&state.db_pool, folder_id).await?;
    let permission = state
        .permission_resolver
        .resolve_permission_with_source(user_id, tenant_id, Resource::Folder(folder_id))
        .await
        .map_err(|error| {
            tracing::error!(
                "failed to resolve permission for shared tree folder {} and user {}: {}",
                folder_id,
                user_id,
                error
            );
            AppError::internal("failed to resolve permission")
        })?;

    let contents = state
        .folder_service
        .list_contents(folder_id, user_id)
        .await?;

    let mut subfolders = Vec::with_capacity(contents.folders.len());
    for child in contents.folders {
        let subtree = Box::pin(build_user_shared_folder_tree(
            state, user_id, tenant_id, child.id,
        ))
        .await?;
        subfolders.push(subtree);
    }

    Ok(FolderTreeWithShares {
        folder: FolderTreeNode {
            id: folder.id,
            name: folder.name,
            path: folder.path,
            parent_folder_id: folder.parent_folder_id,
            owner_id: folder.owner_id,
            created_at: folder.created_at,
            updated_at: folder.updated_at,
            tenant_id: folder.tenant_id,
            ancestor_ids: folder.ancestor_ids,
            is_shared: share_info.0,
            share_count: share_info.1,
            share_expires_at: share_info.2,
            effective_permission: permission_to_string(permission.permission),
            note_bundle_file_id: None,
        },
        subfolders,
    })
}

async fn load_folder_share_summary(
    pool: &sqlx::PgPool,
    folder_id: Uuid,
) -> Result<(bool, i64, Option<chrono::DateTime<chrono::Utc>>), AppError> {
    sqlx::query_as(
        r#"
        SELECT
            EXISTS(SELECT 1 FROM shares WHERE folder_id = $1 AND revoked_at IS NULL) as is_shared,
            (SELECT COUNT(*) FROM shares WHERE folder_id = $1 AND revoked_at IS NULL) as share_count,
            (SELECT MIN(expires_at) FROM shares WHERE folder_id = $1 AND revoked_at IS NULL AND expires_at IS NOT NULL) as share_expires_at
        "#,
    )
    .bind(folder_id)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        tracing::error!("database error fetching share info for folder {}: {}", folder_id, error);
        AppError::internal("database error fetching share info")
    })
}

async fn load_file_share_summary(
    pool: &sqlx::PgPool,
    file_id: Uuid,
) -> Result<(bool, i64, Option<chrono::DateTime<chrono::Utc>>), AppError> {
    sqlx::query_as(
        r#"
        SELECT
            EXISTS(SELECT 1 FROM shares WHERE file_id = $1 AND revoked_at IS NULL) as is_shared,
            (SELECT COUNT(*) FROM shares WHERE file_id = $1 AND revoked_at IS NULL) as share_count,
            (SELECT MIN(expires_at) FROM shares WHERE file_id = $1 AND revoked_at IS NULL AND expires_at IS NOT NULL) as share_expires_at
        "#,
    )
    .bind(file_id)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        tracing::error!("database error fetching share info for file {}: {}", file_id, error);
        AppError::internal("database error fetching share info")
    })
}

async fn load_folder_size(pool: &sqlx::PgPool, folder_id: Uuid) -> Result<i64, AppError> {
    let size: i64 = sqlx::query_scalar(
        r#"
        WITH RECURSIVE folder_tree AS (
            SELECT id FROM folders WHERE id = $1
            UNION ALL
            SELECT child.id FROM folders child
            INNER JOIN folder_tree parent ON child.parent_folder_id = parent.id
            WHERE child.deleted_at IS NULL
        )
        SELECT COALESCE(SUM(files.size), 0)::bigint FROM files
        WHERE files.parent_folder_id IN (SELECT id FROM folder_tree)
        AND files.deleted_at IS NULL
        "#,
    )
    .bind(folder_id)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        tracing::error!(
            "database error fetching size for folder {}: {}",
            folder_id,
            error
        );
        AppError::internal("database error fetching folder size")
    })?;
    Ok(size)
}

fn permission_to_string(permission: Option<SharePermissions>) -> Option<String> {
    permission.map(|permission| {
        match permission {
            SharePermissions::View => "View",
            SharePermissions::Edit => "Edit",
            SharePermissions::Admin => "Admin",
        }
        .to_string()
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require database setup and axum_test.
    // These tests verify that the handler functions are correctly typed and compile.
    // Integration tests will be added when test infrastructure is set up.

    #[test]
    fn test_create_file_share_request_deserialization() {
        let json = serde_json::json!({
            "recipient_email": "user@example.com",
            "permission": "Edit"
        });

        let req: Result<CreateFileShareRequest, _> = serde_json::from_value(json);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.recipient_email, "user@example.com");
        assert_eq!(req.permission, SharePermissions::Edit);
    }

    #[test]
    fn test_create_folder_share_request_deserialization() {
        let json = serde_json::json!({
            "recipient_email": "admin@example.com",
            "permission": "Admin"
        });

        let req: Result<CreateFolderShareRequest, _> = serde_json::from_value(json);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.recipient_email, "admin@example.com");
        assert_eq!(req.permission, SharePermissions::Admin);
    }

    #[test]
    fn test_update_permission_request_deserialization() {
        let json = serde_json::json!({
            "permission": "View"
        });

        let req: Result<UpdatePermissionRequest, _> = serde_json::from_value(json);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.permission, SharePermissions::View);
    }

    #[test]
    fn test_list_received_shares_query_defaults() {
        let json = serde_json::json!({});
        let query: Result<ListReceivedSharesQuery, _> = serde_json::from_value(json);
        assert!(query.is_ok());
        let query = query.unwrap();
        assert_eq!(query.limit, 50);
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn test_list_received_shares_query_custom() {
        let json = serde_json::json!({
            "limit": 10,
            "offset": 20
        });
        let query: Result<ListReceivedSharesQuery, _> = serde_json::from_value(json);
        assert!(query.is_ok());
        let query = query.unwrap();
        assert_eq!(query.limit, 10);
        assert_eq!(query.offset, 20);
    }
}
