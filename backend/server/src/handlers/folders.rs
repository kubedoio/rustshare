//! HTTP handlers for folder operations.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use rustshare_core::domain::Folder;

use super::{AppError, AuthenticatedUser};
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
    pub note_bundle_file_id: Option<Uuid>,
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
    pub note_bundle_file_id: Option<Uuid>,
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
    crate::handlers::ValidatedJson(req): crate::handlers::ValidatedJson<CreateFolderRequest>,
) -> Result<(StatusCode, Json<Folder>), AppError> {
    let folder = state
        .folder_service
        .create_folder(req.name, req.parent_folder_id, auth.user_id, auth.tenant_id)
        .await?;

    Ok((StatusCode::CREATED, Json(folder)))
}

#[derive(Debug, Deserialize, validator::Validate)]
pub struct CreateFolderRequest {
    #[validate(length(
        min = 1,
        max = 255,
        message = "Folder name must be between 1 and 255 characters"
    ))]
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
) -> Result<Json<Folder>, AppError> {
    let folder = state
        .folder_service
        .get_folder(folder_id, auth.user_id)
        .await?;
    Ok(Json(folder))
}

/// Delete a folder and its contents.
///
/// DELETE /api/folders/{id}
pub async fn delete_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .folder_service
        .delete_folder(folder_id, auth.user_id)
        .await?;
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
) -> Result<Json<FolderContentsWithShares>, AppError> {
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
            'Admin'::TEXT as effective_permission,
            (
                SELECT fi.id
                FROM files fi
                WHERE fi.parent_folder_id = f.id AND fi.name = 'note.md' AND fi.deleted_at IS NULL
                LIMIT 1
            ) as note_bundle_file_id
        FROM folders f
        LEFT JOIN folder_sizes fs ON fs.root_id = f.id
        WHERE f.parent_folder_id = $1 AND f.owner_id = $2 AND f.tenant_id = $3 AND f.deleted_at IS NULL
          AND f.name NOT LIKE '.rustshare-%'
          AND f.name NOT IN ('index.md', '__primary__.md')
          AND f.name NOT LIKE '%.editor.json'
        ORDER BY f.name
        "#,
    )
    .bind(folder_id)
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .fetch_all(&state.db_pool)
    .await?;

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
          AND f.name NOT LIKE '.rustshare-%'
          AND f.name NOT IN ('index.md', '__primary__.md')
          AND f.name NOT LIKE '%.editor.json'
        ORDER BY f.name
        "#,
    )
    .bind(folder_id)
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .fetch_all(&state.db_pool)
    .await?;

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
) -> Result<Json<FolderContentsWithShares>, AppError> {
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
            'Admin'::TEXT as effective_permission,
            (
                SELECT fi.id
                FROM files fi
                WHERE fi.parent_folder_id = f.id AND fi.name = 'note.md' AND fi.deleted_at IS NULL
                LIMIT 1
            ) as note_bundle_file_id
        FROM folders f
        LEFT JOIN folder_sizes fs ON fs.root_id = f.id
        WHERE f.parent_folder_id IS NULL AND f.owner_id = $1 AND f.tenant_id = $2 AND f.deleted_at IS NULL
          AND f.name NOT LIKE '.rustshare-%'
          AND f.name NOT IN ('index.md', '__primary__.md')
          AND f.name NOT LIKE '%.editor.json'
        ORDER BY f.name
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .fetch_all(&state.db_pool)
    .await?;

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
          AND f.name NOT LIKE '.rustshare-%'
          AND f.name NOT IN ('index.md', '__primary__.md')
          AND f.name NOT LIKE '%.editor.json'
        ORDER BY f.name
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .fetch_all(&state.db_pool)
    .await?;

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
) -> Result<FolderTreeWithShares, AppError> {
    // Get folder with share info (ancestor_ids is stored in folder_documents, not folders table)
    let folder_row = sqlx::query!(
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
            ) as share_expires_at,
            (
                SELECT fi.id
                FROM files fi
                WHERE fi.parent_folder_id = f.id AND fi.name = 'note.md' AND fi.deleted_at IS NULL
                LIMIT 1
            ) as note_bundle_file_id
        FROM folders f
        WHERE f.id = $1 AND f.owner_id = $2 AND f.tenant_id = $3 AND f.deleted_at IS NULL
        "#,
        folder_id,
        user_id,
        tenant_id
    )
    .fetch_one(state.metadata_store.pool())
    .await?;

    let folder_node = FolderTreeNode {
        id: folder_row.id,
        name: folder_row.name,
        path: folder_row.path,
        parent_folder_id: folder_row.parent_folder_id,
        owner_id: folder_row.owner_id,
        created_at: folder_row.created_at,
        updated_at: folder_row.updated_at,
        tenant_id: folder_row.tenant_id,
        ancestor_ids: None,
        is_shared: folder_row.is_shared.unwrap_or(false),
        share_count: folder_row.share_count.unwrap_or(0),
        share_expires_at: folder_row.share_expires_at,
        effective_permission: Some("Admin".to_string()),
        note_bundle_file_id: folder_row.note_bundle_file_id,
    };

    // Get child folders
    let child_rows = sqlx::query!(
        r#"
        SELECT id FROM folders 
        WHERE parent_folder_id = $1 AND owner_id = $2 AND tenant_id = $3 AND deleted_at IS NULL
        ORDER BY name ASC
        "#,
        folder_id,
        user_id,
        tenant_id
    )
    .fetch_all(state.metadata_store.pool())
    .await?;

    let mut subfolders = Vec::new();
    for row in child_rows {
        let child_id = row.id;
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
) -> Result<Json<FolderTreeWithShares>, AppError> {
    // Find all user's root-level folders (folders with no parent)
    let root_folders = state
        .metadata_store
        .list_folders_with_shares(None, auth.user_id, auth.tenant_id)
        .await
        .map_err(|_| AppError::internal("Internal server error"))?;

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
        note_bundle_file_id: None,
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
) -> Result<StatusCode, AppError> {
    let updated = state
        .metadata_store
        .set_folder_starred(folder_id, auth.user_id, req.starred)
        .await
        .map_err(|e| AppError::internal(format!("Failed to update folder star state: {}", e)))?;

    if !updated {
        return Err(AppError::not_found(format!(
            "Folder not found: {}",
            folder_id
        )));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn restore_folder_from_trash(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let restored = state
        .metadata_store
        .restore_folder(folder_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("already exists") {
                AppError::conflict(msg)
            } else {
                AppError::internal(msg)
            }
        })?;

    if !restored {
        return Err(AppError::not_found(format!(
            "Folder not found: {}",
            folder_id
        )));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn permanently_delete_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let deleted = state
        .metadata_store
        .permanently_delete_folder(folder_id, auth.user_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to permanently delete folder: {}", e)))?;

    if !deleted {
        return Err(AppError::not_found(format!(
            "Folder not found: {}",
            folder_id
        )));
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
) -> Result<Json<Folder>, AppError> {
    let folder = state
        .folder_service
        .move_folder(folder_id, req.target_parent_id, auth.user_id)
        .await?;

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
) -> Result<Json<Folder>, AppError> {
    let folder = state
        .folder_service
        .rename_folder(folder_id, req.new_name, auth.user_id)
        .await?;

    Ok(Json(folder))
}

#[derive(Debug, Deserialize)]
pub struct RenameFolderRequest {
    pub new_name: String,
}

// ============================================================================
// Folder Download as Zip
// ============================================================================

const MAX_ZIP_SIZE_BYTES: i64 = 1024 * 1024 * 1024; // 1 GB

/// Hidden kanban metadata files that should be excluded from zip downloads.
fn is_hidden_kanban_file(name: &str) -> bool {
    matches!(
        name,
        ".rustshare-board.json"
            | ".rustshare-column.json"
            | ".rustshare-card.json"
            | "events.jsonl"
            | "index.md"
            | "__primary__.md"
    ) || name.ends_with(".editor.json")
}

/// Download a folder and all its contents as a zip archive.
///
/// GET /api/v1/folders/{id}/download
pub async fn download_folder(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(folder_id): Path<Uuid>,
) -> Result<Response, AppError> {
    // 1. Verify folder exists and user has access
    let folder = state
        .folder_service
        .get_folder(folder_id, auth.user_id)
        .await?;

    // 2. Collect all descendant folders (includes the root folder)
    let all_folders = state
        .metadata_store
        .find_descendant_folders(folder_id, auth.user_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to list folders: {e}")))?;

    let folder_ids: Vec<Uuid> = all_folders.iter().map(|f| f.id).collect();

    // 3. Collect all files in those folders
    let mut files = state
        .metadata_store
        .find_files_in_folders(&folder_ids, auth.user_id, auth.tenant_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to list files: {e}")))?;

    // Filter out hidden kanban files
    files.retain(|f| !is_hidden_kanban_file(&f.name));

    // 4. Size limit check
    let total_size: i64 = files.iter().map(|f| f.size).sum();
    if total_size > MAX_ZIP_SIZE_BYTES {
        return Err(AppError::bad_request(format!(
            "Folder exceeds {}GB download limit",
            MAX_ZIP_SIZE_BYTES / (1024 * 1024 * 1024)
        )));
    }

    // 5. Build zip in a blocking task
    let folder_name = folder.name.clone();
    let folder_path = folder.path.clone();
    let object_store = Arc::clone(&state.object_store);
    let folder_name_for_zip = folder_name.clone();

    let temp_file = tokio::task::spawn_blocking(move || -> Result<tempfile::NamedTempFile, AppError> {
        let mut temp = tempfile::NamedTempFile::new()
            .map_err(|e| AppError::internal(format!("Failed to create temp file: {e}")))?;

        {
            let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(&mut temp));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            // Add empty folders as directory entries
            for f in &all_folders {
                let relative_path = f
                    .path
                    .strip_prefix(&folder_path)
                    .unwrap_or(&f.path)
                    .trim_start_matches('/');
                if relative_path.is_empty() {
                    continue;
                }
                let dir_path = format!("{}/{}/", folder_name_for_zip, relative_path);
                if let Err(e) = zip.add_directory(&dir_path, options) {
                    tracing::warn!(error = %e, path = %dir_path, "Failed to add directory to zip");
                }
            }

            // Add files
            for file in &files {
                let relative_path = file
                    .path
                    .strip_prefix(&folder_path)
                    .unwrap_or(&file.path)
                    .trim_start_matches('/');
                let zip_path = format!("{}/{}", folder_name_for_zip, relative_path);

                let storage_key = format!("blobs/{}", file.content_hash);
                let bytes = match tokio::runtime::Handle::try_current() {
                    Ok(handle) => handle.block_on(async { object_store.get(&storage_key).await }),
                    Err(_) => {
                        // If no runtime, we can't fetch - skip this file
                        tracing::warn!(file = %file.name, "No Tokio runtime available for object store fetch");
                        continue;
                    }
                };

                let bytes = match bytes {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!(file = %file.name, error = %e, "Failed to fetch file from object store");
                        continue;
                    }
                };

                if let Err(e) = zip.start_file(&zip_path, options) {
                    tracing::warn!(file = %file.name, error = %e, "Failed to start zip file");
                    continue;
                }
                if let Err(e) = std::io::Write::write_all(&mut zip, &bytes) {
                    tracing::warn!(file = %file.name, error = %e, "Failed to write file to zip");
                    continue;
                }
            }

            zip.finish()
                .map_err(|e| AppError::internal(format!("Failed to finalize zip: {e}")))?;
        }

        Ok(temp)
    })
    .await
    .map_err(|e| AppError::internal(format!("Zip generation task panicked: {e}")))??;

    // 6. Stream the temp file back
    let temp_path = temp_file.path().to_path_buf();
    let file = tokio::fs::File::open(&temp_path)
        .await
        .map_err(|e| AppError::internal(format!("Failed to open temp file: {e}")))?;

    let filename = format!("{}.zip", folder_name);
    let content_disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        filename.replace('"', "\\\""),
        urlencoding::encode(&filename)
    );

    let stream = futures_util::stream::unfold(file, |mut file| async move {
        let mut buf = vec![0u8; 64 * 1024];
        match file.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => Some((Ok::<_, std::io::Error>(Bytes::copy_from_slice(&buf[..n])), file)),
            Err(e) => Some((Err(e), file)),
        }
    });

    // Append a final chunk that drops the temp file after streaming
    let stream = stream.chain(futures_util::stream::once(async move {
        drop(temp_file);
        Ok::<_, std::io::Error>(Bytes::new())
    }));

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip"),
            (header::CONTENT_DISPOSITION, content_disposition.as_str()),
        ],
        axum::body::Body::from_stream(stream),
    )
        .into_response())
}
