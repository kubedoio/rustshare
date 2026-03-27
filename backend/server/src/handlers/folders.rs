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
use rustshare_storage::metadata_v2::schemas::{FolderDocument, FileDocument, TombstoneDocument};
use rustshare_storage::metadata_v2::PutOptions;
use rustshare_storage::MetadataDocumentStoreExt;

use super::{folder_error_response, AuthenticatedUser};
use crate::AppState;

/// Recursively update paths for all descendants of a folder.
/// This is called after a folder is moved or renamed to ensure
/// all child folders and files have correct paths.
async fn update_descendant_paths(
    state: &AppState,
    user_id: Uuid,
    folder_id: Uuid,
    new_parent_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use chrono::Utc;

    // Update direct child folders
    let folder_prefix = format!("{}/{}/meta/folders/",
        state.metadata_prefix, state.metadata_namespace);
    let folder_keys = state.doc_store
        .list_prefix(&folder_prefix)
        .await?;

    for key in folder_keys {
        if let Ok(Some((mut doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
            if doc.owner_id == user_id && !doc.deleted && doc.parent_id == Some(folder_id) {
                // Recompute child's path
                let new_path = if new_parent_path == "/" {
                    format!("/{}", doc.name)
                } else {
                    format!("{}/{}", new_parent_path, doc.name)
                };
                doc.path = new_path.clone();
                doc.updated_at = Utc::now();

                // Store updated document
                state.doc_store.put(&key, &doc, PutOptions::default()).await?;

                // Recursively update this folder's descendants
                Box::pin(update_descendant_paths(state, user_id, doc.id, &new_path)).await?;
            }
        }
    }

    // Update direct child files
    let file_prefix = format!("{}/{}/meta/files/",
        state.metadata_prefix, state.metadata_namespace);
    let file_keys = state.doc_store
        .list_prefix(&file_prefix)
        .await?;

    for key in file_keys {
        if let Ok(Some((mut doc, _))) = state.doc_store.get::<FileDocument>(&key).await {
            if doc.owner_id == user_id && !doc.deleted && doc.parent_id == Some(folder_id) {
                // Recompute file's path
                let new_path = if new_parent_path == "/" {
                    format!("/{}", doc.name)
                } else {
                    format!("{}/{}", new_parent_path, doc.name)
                };
                doc.path = new_path;
                doc.updated_at = Utc::now();

                // Store updated document
                state.doc_store.put(&key, &doc, PutOptions::default()).await?;
            }
        }
    }

    Ok(())
}

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
    use chrono::Utc;
    use uuid::Uuid;
    use rustshare_storage::metadata_v2::schemas::FolderDocument;

    let folder_id = Uuid::new_v4();

    // Build path from parent or root
    let path = if let Some(parent_id) = req.parent_folder_id {
        // Load parent to get its path
        let parent_key = format!("{}/{}/meta/folders/{}.json",
            state.metadata_prefix, state.metadata_namespace, parent_id);
        let parent_doc = state.doc_store
            .get::<FolderDocument>(&parent_key).await
            .map_err(|e| {
                tracing::error!("Failed to load parent folder: {}", e);
                use axum::{http::StatusCode, response::IntoResponse, Json};
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(super::ErrorResponse::new("Internal server error")),
                ).into_response()
            })?
            .ok_or_else(|| {
                use axum::{http::StatusCode, response::IntoResponse, Json};
                (
                    StatusCode::BAD_REQUEST,
                    Json(super::ErrorResponse::new(format!("Parent folder not found: {}", parent_id))),
                ).into_response()
            })?;
        format!("{}/{}", parent_doc.0.path, req.name)
    } else {
        format!("/{}", req.name)
    };

    // Check for duplicate names in the same parent
    let folder_prefix = format!("{}/{}/meta/folders/",
        state.metadata_prefix, state.metadata_namespace);
    let folder_keys = state.doc_store
        .list_prefix(&folder_prefix)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list folders: {}", e);
            use axum::{http::StatusCode, response::IntoResponse, Json};
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new("Internal server error")),
            ).into_response()
        })?;

    for key in folder_keys {
        if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
            if doc.owner_id == auth.user_id
                && !doc.deleted
                && doc.parent_id == req.parent_folder_id
                && doc.name == req.name {
                return Err(folder_error_response(
                    rustshare_core::services::FolderError::DuplicateName(req.name)
                ));
            }
        }
    }

    // Create folder document
    let folder_doc = FolderDocument {
        schema_version: 2,
        id: folder_id,
        namespace_id: Uuid::nil(),
        owner_id: auth.user_id,
        parent_id: req.parent_folder_id,
        name: req.name.clone(),
        path: path.clone(),
        deleted: false,
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    // Store document
    let key = format!("{}/{}/meta/folders/{}.json",
        state.metadata_prefix, state.metadata_namespace, folder_id);
    state.doc_store.put(&key, &folder_doc, PutOptions::default()).await
        .map_err(|e| {
            tracing::error!("Failed to store folder: {}", e);
            use axum::{http::StatusCode, response::IntoResponse, Json};
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new("Internal server error")),
            ).into_response()
        })?;

    // Convert to domain Folder for response
    let folder = Folder {
        id: folder_id,
        name: req.name,
        path,
        parent_folder_id: req.parent_folder_id,
        owner_id: auth.user_id,
        created_at: folder_doc.created_at,
        updated_at: folder_doc.updated_at,
        deleted: false,
    };

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
    use chrono::Utc;

    // Load folder document
    let folder_key = format!("{}/{}/meta/folders/{}.json",
        state.metadata_prefix, state.metadata_namespace, folder_id);
    let (folder_doc, _) = state.doc_store
        .get::<FolderDocument>(&folder_key).await
        .map_err(|e| {
            tracing::error!("Failed to load folder: {}", e);
            folder_error_response(rustshare_core::services::FolderError::Storage(e.to_string()))
        })?
        .ok_or_else(|| {
            folder_error_response(rustshare_core::services::FolderError::NotFound(folder_id))
        })?;

    // Verify ownership
    if folder_doc.owner_id != auth.user_id {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::PermissionDenied { folder_id, user_id: auth.user_id }
        ));
    }

    // Check if folder is empty (no subfolders and no files)
    let folder_prefix = format!("{}/{}/meta/folders/",
        state.metadata_prefix, state.metadata_namespace);
    let folder_keys = state.doc_store
        .list_prefix(&folder_prefix)
        .await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?;

    for key in folder_keys {
        if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
            if doc.owner_id == auth.user_id
                && !doc.deleted
                && doc.parent_id == Some(folder_id) {
                return Err(folder_error_response(
                    rustshare_core::services::FolderError::NotEmpty(folder_id)
                ));
            }
        }
    }

    // Check for files in the folder
    let file_prefix = format!("{}/{}/meta/files/",
        state.metadata_prefix, state.metadata_namespace);
    let file_keys = state.doc_store
        .list_prefix(&file_prefix)
        .await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?;

    for key in file_keys {
        if let Ok(Some((doc, _))) = state.doc_store.get::<FileDocument>(&key).await {
            if doc.owner_id == auth.user_id
                && !doc.deleted
                && doc.parent_id == Some(folder_id) {
                return Err(folder_error_response(
                    rustshare_core::services::FolderError::NotEmpty(folder_id)
                ));
            }
        }
    }

    // Soft delete: mark as deleted and update
    let mut updated_doc = folder_doc;
    updated_doc.deleted = true;
    updated_doc.updated_at = Utc::now();

    state.doc_store.put(&folder_key, &updated_doc, PutOptions::default()).await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?;

    // Create tombstone for potential recovery
    let tombstone = TombstoneDocument::from_folder(&updated_doc, auth.user_id);

    let tombstone_key = format!("{}/{}/meta/tombstones/folders/{}.json",
        state.metadata_prefix, state.metadata_namespace, folder_id);
    let _ = state.doc_store.put(&tombstone_key, &tombstone, PutOptions::default()).await;

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
    let user_id = auth.user_id;
    
    // Build prefix for user's folders
    let folder_prefix = format!("{}/{}/meta/folders/", 
        state.metadata_prefix, state.metadata_namespace);
    
    // List all folders and filter by owner and parent
    let folder_keys = state.doc_store
        .list_prefix(&folder_prefix)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list folders: {}", e);
            use axum::{http::StatusCode, response::IntoResponse, Json};
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new("Internal server error")),
            )
                .into_response()
        })?;
    
    let mut folders_with_shares: Vec<FolderWithShares> = Vec::new();
    for key in folder_keys {
        if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
            // Filter by owner and specific parent
            if doc.owner_id == user_id && !doc.deleted && doc.parent_id == Some(folder_id) {
                // Get share info
                let shares = state.share_repo
                    .list_by_resource("folder", doc.id)
                    .await
                    .unwrap_or_default();
                
                folders_with_shares.push(FolderWithShares {
                    id: doc.id,
                    name: doc.name,
                    path: doc.path,
                    parent_folder_id: doc.parent_id,
                    owner_id: doc.owner_id,
                    created_at: doc.created_at,
                    updated_at: doc.updated_at,
                    is_shared: !shares.is_empty(),
                    share_count: shares.len() as i64,
                    share_expires_at: shares.iter().filter_map(|s| s.expires_at).min(),
                });
            }
        }
    }
    
    // Build prefix for user's files
    let file_prefix = format!("{}/{}/meta/files/", 
        state.metadata_prefix, state.metadata_namespace);
    
    // List all files and filter by owner and parent
    let file_keys = state.doc_store
        .list_prefix(&file_prefix)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list files: {}", e);
            use axum::{http::StatusCode, response::IntoResponse, Json};
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new("Internal server error")),
            )
                .into_response()
        })?;
    
    let mut files_with_shares: Vec<crate::handlers::files::FileWithShares> = Vec::new();
    for key in file_keys {
        if let Ok(Some((doc, _))) = state.doc_store.get::<FileDocument>(&key).await {
            // Filter by owner and specific parent
            if doc.owner_id == user_id && !doc.deleted && doc.parent_id == Some(folder_id) {
                // Get share info
                let shares = state.share_repo
                    .list_by_resource("file", doc.id)
                    .await
                    .unwrap_or_default();
                
                files_with_shares.push(crate::handlers::files::FileWithShares {
                    id: doc.id,
                    name: doc.name,
                    path: doc.path,
                    content_hash: doc.content_ref.clone(),
                    size: doc.size,
                    mime_type: doc.mime_type.clone(),
                    parent_folder_id: doc.parent_id,
                    owner_id: doc.owner_id,
                    current_version: doc.version_number,
                    created_at: doc.created_at,
                    modified_at: doc.updated_at,
                    is_shared: !shares.is_empty(),
                    share_count: shares.len() as i64,
                    share_expires_at: shares.iter().filter_map(|s| s.expires_at).min(),
                });
            }
        }
    }
    
    Ok(Json(FolderContentsWithShares { 
        folders: folders_with_shares, 
        files: files_with_shares 
    }))
}

/// List root contents (folders and files with no parent) with share indicators.
///
/// GET /api/folders/root/contents
pub async fn get_root_contents(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<FolderContentsWithShares>, Response> {
    let user_id = auth.user_id;
    
    // Build prefix for user's folders
    let folder_prefix = format!("{}/{}/meta/folders/", 
        state.metadata_prefix, state.metadata_namespace);
    
    // List all folders and filter by owner
    let folder_keys = state.doc_store
        .list_prefix(&folder_prefix)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list folders: {}", e);
            use axum::{http::StatusCode, response::IntoResponse, Json};
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new("Internal server error")),
            )
                .into_response()
        })?;
    
    let mut folders_with_shares: Vec<FolderWithShares> = Vec::new();
    for key in folder_keys {
        if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
            // Filter by owner and root (no parent)
            if doc.owner_id == user_id && !doc.deleted && doc.parent_id.is_none() {
                // Get share info
                let shares = state.share_repo
                    .list_by_resource("folder", doc.id)
                    .await
                    .unwrap_or_default();
                
                folders_with_shares.push(FolderWithShares {
                    id: doc.id,
                    name: doc.name,
                    path: doc.path,
                    parent_folder_id: doc.parent_id,
                    owner_id: doc.owner_id,
                    created_at: doc.created_at,
                    updated_at: doc.updated_at,
                    is_shared: !shares.is_empty(),
                    share_count: shares.len() as i64,
                    share_expires_at: shares.iter().filter_map(|s| s.expires_at).min(),
                });
            }
        }
    }
    
    // Build prefix for user's files
    let file_prefix = format!("{}/{}/meta/files/", 
        state.metadata_prefix, state.metadata_namespace);
    
    // List all files and filter by owner
    let file_keys = state.doc_store
        .list_prefix(&file_prefix)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list files: {}", e);
            use axum::{http::StatusCode, response::IntoResponse, Json};
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(super::ErrorResponse::new("Internal server error")),
            )
                .into_response()
        })?;
    
    let mut files_with_shares: Vec<crate::handlers::files::FileWithShares> = Vec::new();
    for key in file_keys {
        if let Ok(Some((doc, _))) = state.doc_store.get::<FileDocument>(&key).await {
            // Filter by owner and root (no parent)
            if doc.owner_id == user_id && !doc.deleted && doc.parent_id.is_none() {
                // Get share info
                let shares = state.share_repo
                    .list_by_resource("file", doc.id)
                    .await
                    .unwrap_or_default();
                
                files_with_shares.push(crate::handlers::files::FileWithShares {
                    id: doc.id,
                    name: doc.name,
                    path: doc.path,
                    content_hash: doc.content_ref.clone(),
                    size: doc.size,
                    mime_type: doc.mime_type.clone(),
                    parent_folder_id: doc.parent_id,
                    owner_id: doc.owner_id,
                    current_version: doc.version_number,
                    created_at: doc.created_at,
                    modified_at: doc.updated_at,
                    is_shared: !shares.is_empty(),
                    share_count: shares.len() as i64,
                    share_expires_at: shares.iter().filter_map(|s| s.expires_at).min(),
                });
            }
        }
    }
    
    Ok(Json(FolderContentsWithShares { 
        folders: folders_with_shares, 
        files: files_with_shares 
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
    use rustshare_storage::metadata_v2::schemas::FolderDocument;
    use chrono::Utc;

    // Load folder document
    let folder_key = format!("{}/{}/meta/folders/{}.json",
        state.metadata_prefix, state.metadata_namespace, folder_id);
    let (mut folder_doc, _) = state.doc_store
        .get::<FolderDocument>(&folder_key).await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?
        .ok_or_else(|| folder_error_response(
            rustshare_core::services::FolderError::NotFound(folder_id)
        ))?;

    // Verify ownership
    if folder_doc.owner_id != auth.user_id {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::PermissionDenied { folder_id, user_id: auth.user_id }
        ));
    }

    // Validate target parent exists if specified
    if let Some(target_id) = req.target_parent_id {
        if target_id == folder_id {
            return Err(folder_error_response(
                rustshare_core::services::FolderError::InvalidMove {
                    folder_id,
                    reason: "Cannot move a folder into itself".to_string(),
                }
            ));
        }

        let target_key = format!("{}/{}/meta/folders/{}.json",
            state.metadata_prefix, state.metadata_namespace, target_id);
        let _ = state.doc_store
            .get::<FolderDocument>(&target_key).await
            .map_err(|e| folder_error_response(
                rustshare_core::services::FolderError::Storage(e.to_string())
            ))?
            .ok_or_else(|| folder_error_response(
                rustshare_core::services::FolderError::ParentFolderNotFound(target_id)
            ))?;

        // Check for circular reference (moving into descendant)
        let mut current = target_id;
        loop {
            let current_key = format!("{}/{}/meta/folders/{}.json",
                state.metadata_prefix, state.metadata_namespace, current);
            if let Ok(Some((current_doc, _))) = state.doc_store.get::<FolderDocument>(&current_key).await {
                if current_doc.parent_id == Some(folder_id) {
                    return Err(folder_error_response(
                        rustshare_core::services::FolderError::CircularReference { folder_id, target_id }
                    ));
                }
                if let Some(parent) = current_doc.parent_id {
                    current = parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Check for duplicate name in target parent
        let folder_prefix = format!("{}/{}/meta/folders/",
            state.metadata_prefix, state.metadata_namespace);
        let folder_keys = state.doc_store
            .list_prefix(&folder_prefix)
            .await
            .map_err(|e| folder_error_response(
                rustshare_core::services::FolderError::Storage(e.to_string())
            ))?;

        for key in folder_keys {
            if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
                if doc.owner_id == auth.user_id
                    && !doc.deleted
                    && doc.parent_id == Some(target_id)
                    && doc.name == folder_doc.name {
                    return Err(folder_error_response(
                        rustshare_core::services::FolderError::DuplicateName(folder_doc.name.clone())
                    ));
                }
            }
        }
    } else {
        // Moving to root - check for duplicate name at root
        let folder_prefix = format!("{}/{}/meta/folders/",
            state.metadata_prefix, state.metadata_namespace);
        let folder_keys = state.doc_store
            .list_prefix(&folder_prefix)
            .await
            .map_err(|e| folder_error_response(
                rustshare_core::services::FolderError::Storage(e.to_string())
            ))?;

        for key in folder_keys {
            if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
                if doc.owner_id == auth.user_id
                    && !doc.deleted
                    && doc.parent_id.is_none()
                    && doc.name == folder_doc.name
                    && doc.id != folder_id {
                    return Err(folder_error_response(
                        rustshare_core::services::FolderError::DuplicateName(folder_doc.name.clone())
                    ));
                }
            }
        }
    }

    // Update parent and path
    folder_doc.parent_id = req.target_parent_id;
    folder_doc.path = if let Some(parent_id) = req.target_parent_id {
        let parent_key = format!("{}/{}/meta/folders/{}.json",
            state.metadata_prefix, state.metadata_namespace, parent_id);
        if let Ok(Some((parent_doc, _))) = state.doc_store.get::<FolderDocument>(&parent_key).await {
            if parent_doc.path == "/" {
                format!("/{}", folder_doc.name)
            } else {
                format!("{}/{}", parent_doc.path, folder_doc.name)
            }
        } else {
            format!("/{}", folder_doc.name)
        }
    } else {
        format!("/{}", folder_doc.name)
    };
    folder_doc.updated_at = Utc::now();

    // Store updated folder
    state.doc_store.put(&folder_key, &folder_doc, PutOptions::default()).await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?;

    // Recursively update descendant paths
    update_descendant_paths(
        &state,
        auth.user_id,
        folder_id,
        &folder_doc.path,
    ).await.map_err(|e| folder_error_response(
        rustshare_core::services::FolderError::Storage(e.to_string())
    ))?;

    // Convert to domain Folder for response
    let folder = Folder {
        id: folder_id,
        name: folder_doc.name,
        path: folder_doc.path.clone(),
        parent_folder_id: folder_doc.parent_id,
        owner_id: folder_doc.owner_id,
        created_at: folder_doc.created_at,
        updated_at: folder_doc.updated_at,
        deleted: folder_doc.deleted,
    };

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
    use rustshare_storage::metadata_v2::schemas::FolderDocument;
    use chrono::Utc;

    // Validate name
    if req.new_name.is_empty() {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::InvalidName("Folder name cannot be empty".to_string())
        ));
    }
    if req.new_name.contains('/') {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::InvalidName("Folder name cannot contain forward slash".to_string())
        ));
    }

    // Load folder document
    let folder_key = format!("{}/{}/meta/folders/{}.json",
        state.metadata_prefix, state.metadata_namespace, folder_id);
    let (mut folder_doc, _) = state.doc_store
        .get::<FolderDocument>(&folder_key).await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?
        .ok_or_else(|| folder_error_response(
            rustshare_core::services::FolderError::NotFound(folder_id)
        ))?;

    // Verify ownership
    if folder_doc.owner_id != auth.user_id {
        return Err(folder_error_response(
            rustshare_core::services::FolderError::PermissionDenied { folder_id, user_id: auth.user_id }
        ));
    }

    // Check for duplicate name in parent
    if folder_doc.name != req.new_name {
        let folder_prefix = format!("{}/{}/meta/folders/",
            state.metadata_prefix, state.metadata_namespace);
        let folder_keys = state.doc_store
            .list_prefix(&folder_prefix)
            .await
            .map_err(|e| folder_error_response(
                rustshare_core::services::FolderError::Storage(e.to_string())
            ))?;

        for key in folder_keys {
            if let Ok(Some((doc, _))) = state.doc_store.get::<FolderDocument>(&key).await {
                if doc.owner_id == auth.user_id
                    && !doc.deleted
                    && doc.parent_id == folder_doc.parent_id
                    && doc.name == req.new_name {
                    return Err(folder_error_response(
                        rustshare_core::services::FolderError::DuplicateName(req.new_name.clone())
                    ));
                }
            }
        }
    }

    // Update name and recompute path
    folder_doc.name = req.new_name.clone();
    folder_doc.path = if let Some(parent_id) = folder_doc.parent_id {
        let parent_key = format!("{}/{}/meta/folders/{}.json",
            state.metadata_prefix, state.metadata_namespace, parent_id);
        if let Ok(Some((parent_doc, _))) = state.doc_store.get::<FolderDocument>(&parent_key).await {
            if parent_doc.path == "/" {
                format!("/{}", req.new_name)
            } else {
                format!("{}/{}", parent_doc.path, req.new_name)
            }
        } else {
            format!("/{}", req.new_name)
        }
    } else {
        format!("/{}", req.new_name)
    };
    folder_doc.updated_at = Utc::now();

    // Store updated folder
    state.doc_store.put(&folder_key, &folder_doc, PutOptions::default()).await
        .map_err(|e| folder_error_response(
            rustshare_core::services::FolderError::Storage(e.to_string())
        ))?;

    // Recursively update descendant paths
    update_descendant_paths(
        &state,
        auth.user_id,
        folder_id,
        &folder_doc.path,
    ).await.map_err(|e| folder_error_response(
        rustshare_core::services::FolderError::Storage(e.to_string())
    ))?;

    // Convert to domain Folder for response
    let folder = Folder {
        id: folder_id,
        name: folder_doc.name,
        path: folder_doc.path.clone(),
        parent_folder_id: folder_doc.parent_id,
        owner_id: folder_doc.owner_id,
        created_at: folder_doc.created_at,
        updated_at: folder_doc.updated_at,
        deleted: folder_doc.deleted,
    };

    Ok(Json(folder))
}

#[derive(Debug, Deserialize)]
pub struct RenameFolderRequest {
    pub new_name: String,
}
