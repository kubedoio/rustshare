//! HTTP handlers for folder operations.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::domain::{Folder, FolderTree};

use super::{folder_error_response, AuthenticatedUser};
use crate::AppState;

// ============================================================================
// Share Indicator Types
// ============================================================================

/// Folder with share information for list responses
#[derive(Debug, Serialize)]
pub struct FolderWithShares {
    // Folder fields
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    // Share info
    pub is_shared: bool,
    pub share_count: i64,
    /// Earliest share expiration date (None if no shares have expiration)
    pub share_expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Folder contents with share indicators
#[derive(Debug, Serialize)]
pub struct FolderContentsWithShares {
    pub folders: Vec<FolderWithShares>,
    pub files: Vec<crate::handlers::files::FileWithShares>,
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
        .create_folder(req.name, req.parent_folder_id, auth.user_id)
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
    State(_state): State<AppState>,
    _auth: AuthenticatedUser,
    Path(_folder_id): Path<Uuid>,
) -> Result<Json<FolderContentsWithShares>, Response> {
    // TODO: Implement proper folder/file listing using V2 services
    // For now, return empty lists to allow UI to load
    Ok(Json(FolderContentsWithShares { 
        folders: Vec::new(), 
        files: Vec::new() 
    }))
}

/// List root contents (folders and files with no parent) with share indicators.
///
/// GET /api/folders/root/contents
pub async fn get_root_contents(
    State(_state): State<AppState>,
    _auth: AuthenticatedUser,
) -> Result<Json<FolderContentsWithShares>, Response> {
    // TODO: Implement proper folder/file listing using V2 services
    // For now, return empty lists to allow UI to load
    Ok(Json(FolderContentsWithShares { 
        folders: Vec::new(), 
        files: Vec::new() 
    }))
}

/// Get full folder tree (recursive).
///
/// GET /api/folders/tree
///
/// Returns a virtual root folder containing all user's root-level folders as subfolders.
pub async fn get_folder_tree(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<FolderTree>, Response> {
    // Find all user's root-level folders (folders with no parent)
    let root_folders = state
        .metadata_store
        .list_folders(None, auth.user_id)
        .await
        .map_err(|_| {
            use axum::{http::StatusCode, response::IntoResponse, Json};
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new("Internal server error")),
            )
                .into_response()
        })?;

    // Build subtrees for each root folder
    let mut subtrees = Vec::new();
    for folder in root_folders {
        let subtree = state
            .folder_service
            .get_tree(folder.id, auth.user_id)
            .await
            .map_err(folder_error_response)?;
        subtrees.push(subtree);
    }

    // Create a virtual root folder to contain all root-level folders
    let virtual_root = Folder {
        id: Uuid::nil(), // Use nil UUID for virtual root
        name: "Root".to_string(),
        path: "/".to_string(),
        parent_folder_id: None,
        owner_id: auth.user_id,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        deleted: false,
    };

    // Get files at root level (files with no parent folder)
    let root_files = state
        .metadata_store
        .list_files(None, auth.user_id)
        .await
        .map_err(|_| {
            use axum::{http::StatusCode, response::IntoResponse, Json};
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new("Internal server error")),
            )
                .into_response()
        })?;

    let tree = FolderTree::with_contents(virtual_root, root_files, subtrees);
    Ok(Json(tree))
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
