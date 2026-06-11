//! Brainstorming service for visual decision boards.
//!
//! Boards are folders under /Brainstorming containing:
//! - board.excalidraw (editable source)
//! - preview.png (thumbnail)
//! - README.md (optional notes)
//! - .rustshare.json (metadata sidecar)

use bytes::Bytes;
use chrono::{DateTime, Utc};
use rustshare_core::{
    domain::{File, Folder, UserId},
    services::{FileService, FolderService},
};
use rustshare_storage::{MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use rustshare_infrastructure::repositories::PermissionResolverRepository;

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum BrainstormError {
    #[error("Board not found")]
    BoardNotFound,
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Invalid name: {0}")]
    InvalidName(String),
    #[error("Invalid slug: {0}")]
    InvalidSlug(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

impl From<rustshare_core::services::FileError> for BrainstormError {
    fn from(e: rustshare_core::services::FileError) -> Self {
        match e {
            rustshare_core::services::FileError::NotFound(_) => BrainstormError::BoardNotFound,
            rustshare_core::services::FileError::PermissionDenied { .. } => {
                BrainstormError::PermissionDenied
            }
            rustshare_core::services::FileError::InvalidName(s) => BrainstormError::InvalidName(s),
            rustshare_core::services::FileError::Database(s) => BrainstormError::Database(s),
            _ => BrainstormError::Storage(e.to_string()),
        }
    }
}

impl From<rustshare_core::services::FolderError> for BrainstormError {
    fn from(e: rustshare_core::services::FolderError) -> Self {
        match e {
            rustshare_core::services::FolderError::NotFound(_) => BrainstormError::BoardNotFound,
            rustshare_core::services::FolderError::PermissionDenied { .. } => {
                BrainstormError::PermissionDenied
            }
            rustshare_core::services::FolderError::InvalidName(s) => {
                BrainstormError::InvalidName(s)
            }
            rustshare_core::services::FolderError::Database(s) => BrainstormError::Database(s),
            _ => BrainstormError::Storage(e.to_string()),
        }
    }
}

impl From<sqlx::Error> for BrainstormError {
    fn from(e: sqlx::Error) -> Self {
        BrainstormError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for BrainstormError {
    fn from(e: serde_json::Error) -> Self {
        BrainstormError::InvalidData(e.to_string())
    }
}

// ============================================================================
// Public types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BrainstormBoard {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub path: String,
    pub template: String,
    pub source_file_id: Option<String>,
    pub preview_file_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BoardMetadata {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub title: String,
    pub slug: String,
    pub template: String,
    #[serde(rename = "sourceFile")]
    pub source_file: String,
    #[serde(rename = "previewFile")]
    pub preview_file: String,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateBoardInput {
    pub title: String,
    pub template_key: String,
}

// ============================================================================
// Service
// ============================================================================

pub struct BrainstormingService {
    file_service: Arc<
        FileService<
            rustshare_storage::EventStore,
            MetadataStore,
            ObjectStore,
            PermissionResolverRepository,
        >,
    >,
    folder_service: Arc<
        FolderService<rustshare_storage::EventStore, MetadataStore, PermissionResolverRepository>,
    >,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
}

impl BrainstormingService {
    pub fn new(
        file_service: Arc<
            FileService<
                rustshare_storage::EventStore,
                MetadataStore,
                ObjectStore,
                PermissionResolverRepository,
            >,
        >,
        folder_service: Arc<
            FolderService<
                rustshare_storage::EventStore,
                MetadataStore,
                PermissionResolverRepository,
            >,
        >,
        metadata_store: Arc<MetadataStore>,
        object_store: Arc<ObjectStore>,
    ) -> Self {
        Self {
            file_service,
            folder_service,
            metadata_store,
            object_store,
        }
    }

    // -------------------------------------------------------------------------
    // Boards
    // -------------------------------------------------------------------------

    pub async fn list_boards(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BrainstormBoard>, BrainstormError> {
        let root = match self.find_brainstorming_root(user_id, tenant_id).await? {
            Some(r) => r,
            None => return Ok(Vec::new()),
        };

        let contents = self
            .folder_service
            .list_contents(root.id, user_id)
            .await
            .map_err(BrainstormError::from)?;

        let mut boards = Vec::new();
        for folder in contents.folders {
            if let Ok(board) = self.load_board(&folder, user_id).await {
                boards.push(board);
            }
        }

        boards.sort_by_key(|a| std::cmp::Reverse(a.updated_at));

        let paginated: Vec<BrainstormBoard> = boards
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();

        Ok(paginated)
    }

    pub async fn get_board(
        &self,
        board_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<BrainstormBoard, BrainstormError> {
        let folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(BrainstormError::from)?;

        if folder.tenant_id != tenant_id {
            return Err(BrainstormError::PermissionDenied);
        }

        // Verify it's under /Brainstorming (legacy) or /Workspace/Brainstorming
        if !(folder.path.starts_with("/Brainstorming")
            || folder.path.starts_with("/Workspace/Brainstorming"))
        {
            return Err(BrainstormError::BoardNotFound);
        }

        self.load_board(&folder, user_id).await
    }

    pub async fn get_board_source(
        &self,
        board_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<String, BrainstormError> {
        tracing::info!(board_id = %board_id, user_id = %user_id, "get_board_source: start");

        let folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(|e| {
                tracing::info!(board_id = %board_id, user_id = %user_id, error = %e, "get_board_source: folder lookup failed");
                BrainstormError::from(e)
            })?;

        if folder.tenant_id != tenant_id {
            return Err(BrainstormError::PermissionDenied);
        }

        tracing::info!(folder_id = %folder.id, folder_path = %folder.path, "get_board_source: folder found");

        let file = self
            .find_file_in_folder(folder.id, user_id, "board.excalidraw")
            .await
            .map_err(|e| {
                tracing::info!(folder_id = %folder.id, user_id = %user_id, error = %e, "get_board_source: find_file_in_folder failed");
                e
            })?;

        let file = match file {
            Some(f) => {
                tracing::info!(file_id = %f.id, file_name = %f.name, "get_board_source: board.excalidraw found");
                f
            }
            None => {
                tracing::info!(folder_id = %folder.id, "get_board_source: board.excalidraw NOT found in folder");
                return Err(BrainstormError::BoardNotFound);
            }
        };

        let content = self
            .object_store
            .get(&file.storage_key())
            .await
            .map_err(|e| {
                tracing::info!(file_id = %file.id, storage_key = %file.storage_key(), error = %e, "get_board_source: object_store.get failed");
                BrainstormError::Storage(e.to_string())
            })?;
        tracing::info!(file_id = %file.id, content_len = content.len(), "get_board_source: content loaded from object store");

        String::from_utf8(content.to_vec()).map_err(|e| {
            tracing::info!(file_id = %file.id, error = %e, "get_board_source: invalid UTF-8");
            BrainstormError::InvalidData(format!("Invalid UTF-8: {}", e))
        })
    }

    pub async fn save_board_source(
        &self,
        board_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        source: String,
    ) -> Result<BrainstormBoard, BrainstormError> {
        tracing::info!(board_id = %board_id, user_id = %user_id, tenant_id = %tenant_id, source_len = source.len(), "save_board_source: start");

        // Validate Excalidraw JSON structure
        let parsed: serde_json::Value = serde_json::from_str(&source)
            .map_err(|e| BrainstormError::InvalidData(e.to_string()))?;

        if parsed.get("type").and_then(|v| v.as_str()) != Some("excalidraw") {
            tracing::info!("save_board_source: validation failed - missing type field");
            return Err(BrainstormError::InvalidData(
                "Invalid Excalidraw JSON: missing type field".to_string(),
            ));
        }

        if !parsed
            .get("elements")
            .map(|v| v.is_array())
            .unwrap_or(false)
        {
            tracing::info!("save_board_source: validation failed - missing elements array");
            return Err(BrainstormError::InvalidData(
                "Invalid Excalidraw JSON: missing elements array".to_string(),
            ));
        }

        tracing::info!(board_id = %board_id, user_id = %user_id, "save_board_source: looking up folder");
        let folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(|e| {
                tracing::info!(board_id = %board_id, user_id = %user_id, error = %e, "save_board_source: folder lookup failed");
                BrainstormError::from(e)
            })?;

        if folder.tenant_id != tenant_id {
            return Err(BrainstormError::PermissionDenied);
        }

        tracing::info!(folder_id = %folder.id, folder_path = %folder.path, "save_board_source: folder found");

        // Update board.excalidraw
        tracing::info!(folder_id = %folder.id, "save_board_source: looking for board.excalidraw");
        let source_file = self
            .find_file_in_folder(folder.id, user_id, "board.excalidraw")
            .await
            .map_err(|e| {
                tracing::info!(folder_id = %folder.id, error = %e, "save_board_source: find_file_in_folder failed");
                e
            })?;

        if let Some(file) = source_file {
            tracing::info!(file_id = %file.id, "save_board_source: editing existing board.excalidraw");
            self.file_service
                .edit_file(file.id, user_id, Bytes::from(source), "overwrite", None)
                .await
                .map_err(|e| {
                    tracing::info!(file_id = %file.id, error = %e, "save_board_source: edit_file failed");
                    BrainstormError::from(e)
                })?;
            tracing::info!(file_id = %file.id, "save_board_source: edit_file succeeded");
        } else {
            tracing::info!(folder_id = %folder.id, "save_board_source: uploading new board.excalidraw");
            self.file_service
                .upload_file(
                    user_id,
                    "board.excalidraw".to_string(),
                    Some(folder.id),
                    Bytes::from(source),
                    "application/json".to_string(),
                    tenant_id,
                )
                .await
                .map_err(|e| {
                    tracing::info!(folder_id = %folder.id, error = %e, "save_board_source: upload_file failed");
                    BrainstormError::from(e)
                })?;
            tracing::info!(folder_id = %folder.id, "save_board_source: upload_file succeeded");
        }

        // Update metadata updatedAt
        tracing::info!(folder_id = %folder.id, "save_board_source: updating metadata timestamp");
        self.update_metadata_timestamp(folder.id, user_id, tenant_id)
            .await
            .map_err(|e| {
                tracing::info!(folder_id = %folder.id, error = %e, "save_board_source: update_metadata_timestamp failed");
                e
            })?;
        tracing::info!(folder_id = %folder.id, "save_board_source: metadata timestamp updated");

        tracing::info!(folder_id = %folder.id, "save_board_source: loading board");
        self.load_board(&folder, user_id).await
    }

    pub async fn update_board_preview(
        &self,
        board_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        preview_bytes: Bytes,
    ) -> Result<BrainstormBoard, BrainstormError> {
        let folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(BrainstormError::from)?;

        if folder.tenant_id != tenant_id {
            return Err(BrainstormError::PermissionDenied);
        }

        let preview_file = self
            .find_file_in_folder(folder.id, user_id, "preview.png")
            .await?;

        if let Some(file) = preview_file {
            self.file_service
                .update_file(file.id, user_id, file.current_version, preview_bytes)
                .await
                .map_err(BrainstormError::from)?;
        } else {
            self.file_service
                .upload_file(
                    user_id,
                    "preview.png".to_string(),
                    Some(folder.id),
                    preview_bytes,
                    "image/png".to_string(),
                    tenant_id,
                )
                .await
                .map_err(BrainstormError::from)?;
        }

        // Update metadata updatedAt
        self.update_metadata_timestamp(folder.id, user_id, tenant_id)
            .await?;

        self.load_board(&folder, user_id).await
    }

    pub async fn delete_board(
        &self,
        board_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), BrainstormError> {
        let folder = self
            .folder_service
            .get_folder(board_id, user_id)
            .await
            .map_err(BrainstormError::from)?;

        if folder.tenant_id != tenant_id {
            return Err(BrainstormError::PermissionDenied);
        }

        if !(folder.path.starts_with("/Brainstorming")
            || folder.path.starts_with("/Workspace/Brainstorming"))
        {
            return Err(BrainstormError::BoardNotFound);
        }

        self.folder_service
            .delete_folder(board_id, user_id)
            .await
            .map_err(BrainstormError::from)?;

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    async fn find_brainstorming_root(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Option<Folder>, BrainstormError> {
        // Legacy: check root path
        let row = sqlx::query!(
            "SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id FROM folders WHERE path = '/Brainstorming' AND tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL LIMIT 1",
            tenant_id,
            user_id
        )
        .fetch_optional(self.metadata_store.pool())
        .await
        .map_err(|e| BrainstormError::Database(e.to_string()))?;

        if let Some(r) = row {
            return Ok(Some(Folder {
                id: r.id,
                name: r.name,
                path: r.path,
                parent_folder_id: r.parent_folder_id,
                owner_id: r.owner_id,
                created_at: r.created_at,
                updated_at: r.updated_at,
                starred_at: r.starred_at,
                deleted_at: r.deleted_at,
                tenant_id: r.tenant_id,
                ancestor_ids: None,
            }));
        }

        // New: check under /Workspace
        let row = sqlx::query!(
            "SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id FROM folders WHERE path = '/Workspace/Brainstorming' AND tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL LIMIT 1",
            tenant_id,
            user_id
        )
        .fetch_optional(self.metadata_store.pool())
        .await
        .map_err(|e| BrainstormError::Database(e.to_string()))?;

        Ok(row.map(|r| Folder {
            id: r.id,
            name: r.name,
            path: r.path,
            parent_folder_id: r.parent_folder_id,
            owner_id: r.owner_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
            starred_at: r.starred_at,
            deleted_at: r.deleted_at,
            tenant_id: r.tenant_id,
            ancestor_ids: None,
        }))
    }

    async fn ensure_workspace_folder(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, BrainstormError> {
        let folders = self
            .metadata_store
            .list_folders(None, user_id, tenant_id)
            .await
            .map_err(|e| BrainstormError::Database(e.to_string()))?;
        if let Some(ws) = folders.into_iter().find(|f| f.name == "Workspace") {
            return Ok(ws);
        }
        self.folder_service
            .create_folder_or_get("Workspace".into(), None, user_id, tenant_id)
            .await
            .map_err(BrainstormError::from)
    }

    pub async fn ensure_brainstorming_root(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, BrainstormError> {
        if let Some(root) = self.find_brainstorming_root(user_id, tenant_id).await? {
            return Ok(root);
        }
        let ws = self.ensure_workspace_folder(user_id, tenant_id).await?;
        let folder = self
            .folder_service
            .create_folder_or_get("Brainstorming".to_string(), Some(ws.id), user_id, tenant_id)
            .await
            .map_err(BrainstormError::from)?;
        Ok(folder)
    }

    async fn load_board(
        &self,
        folder: &Folder,
        user_id: UserId,
    ) -> Result<BrainstormBoard, BrainstormError> {
        let meta = self.load_board_metadata(folder, user_id).await?;

        let source_file_id = self
            .find_file_in_folder(folder.id, user_id, "board.excalidraw")
            .await?
            .map(|f| f.id.to_string());

        let preview_file_id = self
            .find_file_in_folder(folder.id, user_id, "preview.png")
            .await?
            .map(|f| f.id.to_string());

        Ok(BrainstormBoard {
            id: folder.id.to_string(),
            title: meta.title,
            slug: meta.slug,
            path: folder.path.clone(),
            template: meta.template,
            source_file_id,
            preview_file_id,
            created_at: folder.created_at,
            updated_at: meta.updated_at,
        })
    }

    async fn load_board_metadata(
        &self,
        folder: &Folder,
        _user_id: UserId,
    ) -> Result<BoardMetadata, BrainstormError> {
        if let Some(file) = self
            .find_internal_file_in_folder(folder.id, folder.tenant_id, ".rustshare.json")
            .await?
        {
            let content = self
                .object_store
                .get(&file.storage_key())
                .await
                .map_err(|e| BrainstormError::Storage(e.to_string()))?;
            if let Ok(meta) = serde_json::from_slice::<BoardMetadata>(&content) {
                return Ok(meta);
            }
        }

        // Fallback: derive from folder
        let title = folder.name.replace('-', " ");
        let slug = slugify(&title);
        Ok(BoardMetadata {
            id: folder.id.to_string(),
            type_: "brainstorming.board".to_string(),
            title: title.clone(),
            slug,
            template: "template_blank_brainstorm".to_string(),
            source_file: "board.excalidraw".to_string(),
            preview_file: "preview.png".to_string(),
            created_at: folder.created_at,
            updated_at: folder.updated_at,
            schema_version: "1.0".to_string(),
        })
    }

    async fn update_metadata_timestamp(
        &self,
        folder_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), BrainstormError> {
        let mut meta = self
            .load_board_metadata_from_folder(folder_id, user_id)
            .await?;
        meta.updated_at = Utc::now();

        let meta_json = serde_json::to_vec_pretty(&meta)?;

        let folder = self
            .folder_service
            .get_folder(folder_id, user_id)
            .await
            .map_err(BrainstormError::from)?;

        let meta_file = self
            .find_internal_file_in_folder(folder_id, folder.tenant_id, ".rustshare.json")
            .await?;

        if let Some(file) = meta_file {
            self.write_internal_file(file, Bytes::from(meta_json))
                .await?;
        } else {
            self.create_internal_file(
                &folder,
                user_id,
                tenant_id,
                ".rustshare.json",
                Bytes::from(meta_json),
                "application/json",
            )
            .await?;
        }

        Ok(())
    }

    async fn load_board_metadata_from_folder(
        &self,
        folder_id: Uuid,
        user_id: UserId,
    ) -> Result<BoardMetadata, BrainstormError> {
        let folder = self
            .folder_service
            .get_folder(folder_id, user_id)
            .await
            .map_err(BrainstormError::from)?;

        if let Some(file) = self
            .find_internal_file_in_folder(folder_id, folder.tenant_id, ".rustshare.json")
            .await?
        {
            let content = self
                .object_store
                .get(&file.storage_key())
                .await
                .map_err(|e| BrainstormError::Storage(e.to_string()))?;
            if let Ok(meta) = serde_json::from_slice::<BoardMetadata>(&content) {
                return Ok(meta);
            }
        }

        // Fallback: derive from folder (handles malformed or missing .rustshare.json)
        let title = folder.name.replace('-', " ");
        let slug = slugify(&title);
        Ok(BoardMetadata {
            id: folder.id.to_string(),
            type_: "brainstorming.board".to_string(),
            title: title.clone(),
            slug,
            template: "template_blank_brainstorm".to_string(),
            source_file: "board.excalidraw".to_string(),
            preview_file: "preview.png".to_string(),
            created_at: folder.created_at,
            updated_at: folder.updated_at,
            schema_version: "1.0".to_string(),
        })
    }

    async fn find_file_in_folder(
        &self,
        folder_id: Uuid,
        user_id: UserId,
        name: &str,
    ) -> Result<Option<File>, BrainstormError> {
        let contents = self
            .folder_service
            .list_contents(folder_id, user_id)
            .await
            .map_err(BrainstormError::from)?;
        Ok(contents.files.into_iter().find(|f| f.name == name))
    }

    async fn find_internal_file_in_folder(
        &self,
        folder_id: Uuid,
        tenant_id: Uuid,
        name: &str,
    ) -> Result<Option<File>, BrainstormError> {
        let files = self
            .metadata_store
            .list_files_by_parent(Some(folder_id), tenant_id)
            .await
            .map_err(|e| BrainstormError::Database(e.to_string()))?;
        Ok(files.into_iter().find(|f| f.name == name))
    }

    async fn create_internal_file(
        &self,
        folder: &Folder,
        owner_id: UserId,
        tenant_id: Uuid,
        name: &str,
        content: Bytes,
        mime_type: &str,
    ) -> Result<File, BrainstormError> {
        let content_hash = hex::encode(Sha256::digest(&content));
        let file = File::new(
            name.to_string(),
            format!("{}/{}", folder.path, name),
            content_hash,
            content.len() as i64,
            mime_type.to_string(),
            Some(folder.id),
            owner_id,
            tenant_id,
        );

        self.object_store
            .put(&file.storage_key(), content)
            .await
            .map_err(|e| BrainstormError::Storage(e.to_string()))?;
        self.metadata_store
            .create_file(&file)
            .await
            .map_err(|e| BrainstormError::Database(e.to_string()))?;

        Ok(file)
    }

    async fn write_internal_file(
        &self,
        mut file: File,
        content: Bytes,
    ) -> Result<(), BrainstormError> {
        file.content_hash = hex::encode(Sha256::digest(&content));
        file.size = content.len() as i64;
        file.modified_at = Utc::now();
        file.current_version += 1;

        self.object_store
            .put(&file.storage_key(), content)
            .await
            .map_err(|e| BrainstormError::Storage(e.to_string()))?;
        self.metadata_store
            .update_file(&file)
            .await
            .map_err(|e| BrainstormError::Database(e.to_string()))?;

        Ok(())
    }
}

// ============================================================================
// Utilities
// ============================================================================

fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-")
        .replace("--", "-")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("Hello World!"), "hello-world");
        assert_eq!(slugify("Valid-Slug-123"), "valid-slug-123");
    }

    #[test]
    fn test_permission_denied_error_variant() {
        assert!(matches!(
            BrainstormError::PermissionDenied,
            BrainstormError::PermissionDenied
        ));
    }
}
