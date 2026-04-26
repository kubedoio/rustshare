//! HTTP handlers for folder operations.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::domain::Folder;

use super::{folder_error_response, AuthenticatedUser};
use crate::AppState;

// ============================================================================
// Share Indicator Types
// ============================================================================

/// Folder with share information for list responses
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FolderWithShares {
    // Folder fields
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub starred_at: Option<chrono::DateTime<chrono::Utc>>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Total recursive size of all non-deleted files within this folder (in bytes)
    pub size: i64,
    // Share info
    pub is_shared: bool,
    pub share_count: i64,
    /// Earliest share expiration date (None if no shares have expiration)
    pub share_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_permission: Option<String>,
}

/// Folder contents with share indicators
#[derive(Debug, Serialize)]
pub struct FolderContentsWithShares {
    pub folders: Vec<FolderWithShares>,
    pub files: Vec<crate::handlers::files::FileWithShares>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_folder_permission: Option<String>,
}

/// Folder tree node with share information for sidebar
#[derive(Debug, Serialize)]
pub struct FolderTreeNode {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub tenant_id: Uuid,
    pub ancestor_ids: Option<Vec<Uuid>>,
    pub is_shared: bool,
    pub share_count: i64,
    pub share_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_permission: Option<String>,
}

/// Folder tree response with share indicators
#[derive(Debug, Serialize)]
pub struct FolderTreeWithShares {
    pub folder: FolderTreeNode,
    pub subfolders: Vec<FolderTreeWithShares>,
}

// ============================================================================
// Task 20: Folder CRUD
// ============================================================================

/// Create a new folder.
///
/// POST /api/folders
///
/// Request body: { "name": "Documents", "parent_folder_id": "uuid-or-null" }
pub async fn create_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateFolderRequest>,
) -> Result<(StatusCode, Json<Folder>), Response> {
    let folder = state
        .folder_service
        .create_folder(req.name, req.parent_folder_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(folder_error_response)?;

    Ok((StatusCode::CREATED, Json(folder)))
}

#[derive(Debug, Deserialize)]
pub struct CreateFolderRequest {
    pub name: String,
    pub parent_folder_id: Option<Uuid>,
}

/// Get folder metadata.
///
/// GET /api/folders/{id}
pub async fn get_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<Json<Folder>, Response> {
    let folder = state
        .folder_service
        .get_folder(folder_id, auth.user_id)
        .await
        .map_err(folder_error_response)?;
    Ok(Json(folder))
}

/// Delete a folder and its contents.
///
/// DELETE /api/folders/{id}
pub async fn delete_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<StatusCode, Response> {
    state
        .folder_service
        .delete_folder(folder_id, auth.user_id)
        .await
        .map_err(folder_error_response)?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Task 21: Folder List/Tree
// ============================================================================

/// List folder contents (immediate children only) with share indicators.
///
/// GET /api/folders/{id}/contents
pub async fn get_folder_contents(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<Json<FolderContentsWithShares>, Response> {
    // Get folders in this parent with share info
    let folders = sqlx::query_as::<_, FolderWithShares>(
        r#"
        WITH RECURSIVE folder_tree AS (
            SELECT id, parent_folder_id, id as root_id
            FROM folders
            WHERE parent_folder_id = $1 AND owner_id = $2 AND tenant_id = $3 AND deleted_at IS NULL
            UNION ALL
            SELECT child.id, child.parent_folder_id, parent.root_id
            FROM folders child
            INNER JOIN folder_tree parent ON child.parent_folder_id = parent.id
            WHERE child.deleted_at IS NULL
        ),
        folder_sizes AS (
            SELECT ft.root_id, COALESCE(SUM(files.size), 0)::bigint as total_size
            FROM folder_tree ft
            LEFT JOIN files ON files.parent_folder_id = ft.id AND files.deleted_at IS NULL
            GROUP BY ft.root_id
        )
        SELECT
            f.id, f.name, f.path, f.parent_folder_id, f.owner_id,
            f.created_at, f.updated_at, f.starred_at, f.deleted_at,
            COALESCE(fs.total_size, 0) as size,
            EXISTS(
                SELECT 1 FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
            ) as is_shared,
            (
                SELECT COUNT(*) FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
            ) as share_count,
            (
                SELECT MIN(expires_at) FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
                AND expires_at IS NOT NULL
            ) as share_expires_at,
            'Admin'::TEXT as effective_permission
        FROM folders f
        LEFT JOIN folder_sizes fs ON fs.root_id = f.id
        WHERE f.parent_folder_id = $1 AND f.owner_id = $2 AND f.tenant_id = $3 AND f.deleted_at IS NULL
        ORDER BY f.name
        "#,
    )
    .bind(folder_id)
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|_| {
        use axum::{http::StatusCode, response::IntoResponse, Json};
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(super::ErrorResponse::new("Internal server error")),
        )
            .into_response()
    })?;

    // Get files in this parent with share info
    let files = sqlx::query_as::<_, crate::handlers::files::FileWithShares>(
        r#"
        SELECT
            f.id, f.name, f.path, f.size, f.mime_type,
            f.parent_folder_id, f.owner_id, f.current_version,
            f.created_at, f.modified_at, f.starred_at, f.deleted_at,
            EXISTS(
                SELECT 1 FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
            ) as is_shared,
            (
                SELECT COUNT(*) FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
            ) as share_count,
            (
                SELECT MIN(expires_at) FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
                AND expires_at IS NOT NULL
            ) as share_expires_at,
            'Admin'::TEXT as effective_permission
        FROM files f
        WHERE f.parent_folder_id = $1 AND f.owner_id = $2 AND f.tenant_id = $3 AND f.deleted_at IS NULL
        ORDER BY f.name
        "#,
    )
    .bind(folder_id)
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|_| {
        use axum::{http::StatusCode, response::IntoResponse, Json};
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(super::ErrorResponse::new("Internal server error")),
        )
            .into_response()
    })?;

    Ok(Json(FolderContentsWithShares {
        folders,
        files,
        current_folder_permission: None,
    }))
}

/// List root contents (folders and files with no parent) with share indicators.
///
/// GET /api/folders/root/contents
pub async fn get_root_contents(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<FolderContentsWithShares>, Response> {
    // Get root folders with share info
    let folders = sqlx::query_as::<_, FolderWithShares>(
        r#"
        WITH RECURSIVE folder_tree AS (
            SELECT id, parent_folder_id, id as root_id
            FROM folders
            WHERE parent_folder_id IS NULL AND owner_id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            UNION ALL
            SELECT child.id, child.parent_folder_id, parent.root_id
            FROM folders child
            INNER JOIN folder_tree parent ON child.parent_folder_id = parent.id
            WHERE child.deleted_at IS NULL
        ),
        folder_sizes AS (
            SELECT ft.root_id, COALESCE(SUM(files.size), 0)::bigint as total_size
            FROM folder_tree ft
            LEFT JOIN files ON files.parent_folder_id = ft.id AND files.deleted_at IS NULL
            GROUP BY ft.root_id
        )
        SELECT
            f.id, f.name, f.path, f.parent_folder_id, f.owner_id,
            f.created_at, f.updated_at, f.starred_at, f.deleted_at,
            COALESCE(fs.total_size, 0) as size,
            EXISTS(
                SELECT 1 FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
            ) as is_shared,
            (
                SELECT COUNT(*) FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
            ) as share_count,
            (
                SELECT MIN(expires_at) FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
                AND expires_at IS NOT NULL
            ) as share_expires_at,
            'Admin'::TEXT as effective_permission
        FROM folders f
        LEFT JOIN folder_sizes fs ON fs.root_id = f.id
        WHERE f.parent_folder_id IS NULL AND f.owner_id = $1 AND f.tenant_id = $2 AND f.deleted_at IS NULL
        ORDER BY f.name
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|_| {
        use axum::{http::StatusCode, response::IntoResponse, Json};
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(super::ErrorResponse::new("Internal server error")),
        )
            .into_response()
    })?;

    // Get root files with share info
    let files = sqlx::query_as::<_, crate::handlers::files::FileWithShares>(
        r#"
        SELECT
            f.id, f.name, f.path, f.size, f.mime_type,
            f.parent_folder_id, f.owner_id, f.current_version,
            f.created_at, f.modified_at, f.starred_at, f.deleted_at,
            EXISTS(
                SELECT 1 FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
            ) as is_shared,
            (
                SELECT COUNT(*) FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
            ) as share_count,
            (
                SELECT MIN(expires_at) FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
                AND expires_at IS NOT NULL
            ) as share_expires_at,
            'Admin'::TEXT as effective_permission
        FROM files f
        WHERE f.parent_folder_id IS NULL AND f.owner_id = $1 AND f.tenant_id = $2 AND f.deleted_at IS NULL
        ORDER BY f.name
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|_| {
        use axum::{http::StatusCode, response::IntoResponse, Json};
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(super::ErrorResponse::new("Internal server error")),
        )
            .into_response()
    })?;

    Ok(Json(FolderContentsWithShares {
        folders,
        files,
        current_folder_permission: None,
    }))
}

/// Build folder tree with share information recursively
async fn build_folder_tree_with_shares(
    state: &AppState,
    folder_id: Uuid,
    user_id: Uuid,
    tenant_id: Uuid,
) -> Result<FolderTreeWithShares, Response> {
    use sqlx::Row;

    // Get folder with share info (ancestor_ids is stored in folder_documents, not folders table)
    let folder_row = sqlx::query(
        r#"
        SELECT 
            f.id, f.name, f.path, f.parent_folder_id, f.owner_id, 
            f.created_at, f.updated_at, f.tenant_id,
            EXISTS (
                SELECT 1 FROM shares 
                WHERE folder_id = f.id 
                AND revoked_at IS NULL
            ) as is_shared,
            (
                SELECT COUNT(*) FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
            ) as share_count,
            (
                SELECT MIN(expires_at) FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
            ) as share_expires_at
        FROM folders f
        WHERE f.id = $1 AND f.owner_id = $2 AND f.tenant_id = $3 AND f.deleted_at IS NULL
        "#,
    )
    .bind(folder_id)
    .bind(user_id)
    .bind(tenant_id)
    .fetch_one(state.metadata_store.pool())
    .await
    .map_err(|_| {
        use axum::{http::StatusCode, response::IntoResponse, Json};
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(super::ErrorResponse::new("Internal server error")),
        )
            .into_response()
    })?;

    let folder_node = FolderTreeNode {
        id: folder_row
            .try_get("id")
            .map_err(|_| super::internal_error_response())?,
        name: folder_row
            .try_get("name")
            .map_err(|_| super::internal_error_response())?,
        path: folder_row
            .try_get("path")
            .map_err(|_| super::internal_error_response())?,
        parent_folder_id: folder_row
            .try_get("parent_folder_id")
            .map_err(|_| super::internal_error_response())?,
        owner_id: folder_row
            .try_get("owner_id")
            .map_err(|_| super::internal_error_response())?,
        created_at: folder_row
            .try_get("created_at")
            .map_err(|_| super::internal_error_response())?,
        updated_at: folder_row
            .try_get("updated_at")
            .map_err(|_| super::internal_error_response())?,
        tenant_id: folder_row
            .try_get("tenant_id")
            .map_err(|_| super::internal_error_response())?,
        ancestor_ids: None, // Not stored in folders table, would need to fetch from folder_documents
        is_shared: folder_row
            .try_get("is_shared")
            .map_err(|_| super::internal_error_response())?,
        share_count: folder_row
            .try_get("share_count")
            .map_err(|_| super::internal_error_response())?,
        share_expires_at: folder_row.try_get("share_expires_at").ok(),
        effective_permission: Some("Admin".to_string()),
    };

    // Get child folders
    let child_rows: Vec<sqlx::postgres::PgRow> = sqlx::query(
        r#"
        SELECT id FROM folders 
        WHERE parent_folder_id = $1 AND owner_id = $2 AND tenant_id = $3 AND deleted_at IS NULL
        ORDER BY name ASC
        "#,
    )
    .bind(folder_id)
    .bind(user_id)
    .bind(tenant_id)
    .fetch_all(state.metadata_store.pool())
    .await
    .map_err(|_| super::internal_error_response())?;

    let mut subfolders = Vec::new();
    for row in child_rows {
        let child_id: Uuid = row.try_get("id").map_err(|_| super::internal_error_response())?;
        let subtree = Box::pin(build_folder_tree_with_shares(
            state, child_id, user_id, tenant_id,
        ))
        .await?;
        subfolders.push(subtree);
    }

    Ok(FolderTreeWithShares {
        folder: folder_node,
        subfolders,
    })
}

/// Get full folder tree (recursive) with share information.
///
/// GET /api/folders/tree
///
/// Returns a virtual root folder containing all user's root-level folders as subfolders.
pub async fn get_folder_tree(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<FolderTreeWithShares>, Response> {
    // Find all user's root-level folders (folders with no parent)
    let root_folders = state
        .metadata_store
        .list_folders_with_shares(None, auth.user_id, auth.tenant_id)
        .await
        .map_err(|_| super::internal_error_response())?;

    // Build subtrees for each root folder
    let mut subfolders = Vec::new();
    for folder in root_folders {
        let subtree =
            build_folder_tree_with_shares(&state, folder.id, auth.user_id, auth.tenant_id).await?;
        subfolders.push(subtree);
    }

    // Create a virtual root folder to contain all root-level folders
    let virtual_root = FolderTreeNode {
        id: Uuid::nil(), // Use nil UUID for virtual root
        name: "Root".to_string(),
        path: "/".to_string(),
        parent_folder_id: None,
        owner_id: auth.user_id,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        tenant_id: auth.tenant_id,
        ancestor_ids: Some(Vec::new()),
        is_shared: false,
        share_count: 0,
        share_expires_at: None,
        effective_permission: Some("Admin".to_string()),
    };

    let tree = FolderTreeWithShares {
        folder: virtual_root,
        subfolders,
    };
    Ok(Json(tree))
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceStarRequest {
    pub starred: bool,
}

pub async fn toggle_folder_star(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
    Json(req): Json<WorkspaceStarRequest>,
) -> Result<StatusCode, Response> {
    let updated = state
        .metadata_store
        .set_folder_starred(folder_id, auth.user_id, req.starred)
        .await
        .map_err(|e| {
            use axum::response::IntoResponse;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new(format!(
                    "Failed to update folder star state: {}",
                    e
                ))),
            )
                .into_response()
        })?;

    if !updated {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::NotFound(folder_id),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn restore_folder_from_trash(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<StatusCode, Response> {
    let restored = state
        .metadata_store
        .restore_folder(folder_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(|e| {
            use axum::response::IntoResponse;
            let status = if e.to_string().contains("already exists") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(super::ErrorResponse::new(e.to_string()))).into_response()
        })?;

    if !restored {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::NotFound(folder_id),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn permanently_delete_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<StatusCode, Response> {
    let deleted = state
        .metadata_store
        .permanently_delete_folder(folder_id, auth.user_id)
        .await
        .map_err(|e| {
            use axum::response::IntoResponse;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new(format!(
                    "Failed to permanently delete folder: {}",
                    e
                ))),
            )
                .into_response()
        })?;

    if !deleted {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::NotFound(folder_id),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Task 22: Folder Move/Rename
// ============================================================================

/// Move a folder to a different parent.
///
/// POST /api/folders/{id}/move
///
/// Request body: { "target_parent_id": "uuid-or-null" }
pub async fn move_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
    Json(req): Json<MoveFolderRequest>,
) -> Result<Json<Folder>, Response> {
    let folder = state
        .folder_service
        .move_folder(folder_id, req.target_parent_id, auth.user_id)
        .await
        .map_err(folder_error_response)?;

    Ok(Json(folder))
}

#[derive(Debug, Deserialize)]
pub struct MoveFolderRequest {
    pub target_parent_id: Option<Uuid>,
}

/// Rename a folder.
///
/// POST /api/folders/{id}/rename
///
/// Request body: { "new_name": "New Documents" }
pub async fn rename_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
    Json(req): Json<RenameFolderRequest>,
) -> Result<Json<Folder>, Response> {
    let folder = state
        .folder_service
        .rename_folder(folder_id, req.new_name, auth.user_id)
        .await
        .map_err(folder_error_response)?;

    Ok(Json(folder))
}

#[derive(Debug, Deserialize)]
pub struct RenameFolderRequest {
    pub new_name: String,
}
