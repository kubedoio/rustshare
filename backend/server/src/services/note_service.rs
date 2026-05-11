//! Note service for RustShare notes MVP-1.
//!
//! Notes are first-class markdown files with metadata sidecars stored in object storage.

use anyhow::Result;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use rustshare_core::{
    domain::{Folder, UserId},
    services::{FileService, FolderService},
};
use rustshare_storage::{MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Note-specific metadata sidecar schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub kind: String,
    pub title: String,
    pub visibility: NoteVisibility,
    pub public_share_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub excerpt: String,
    pub mime_type: String,
    pub extension: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl NoteMetadata {
    pub fn new(title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            kind: "note".to_string(),
            title: title.into(),
            visibility: NoteVisibility::Private,
            public_share_id: None,
            created_at: now,
            updated_at: now,
            excerpt: String::new(),
            mime_type: "text/markdown".to_string(),
            extension: "md".to_string(),
            pinned: Some(false),
            icon: None,
            color: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteVisibility {
    Private,
    Public,
}

impl NoteVisibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Public => "public",
        }
    }
}

/// Unified note payload returned to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub content: String,
    pub metadata: NoteMetadata,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: UserId,
    pub current_version: i32,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

/// Public note view (no internal identifiers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicNote {
    pub title: String,
    pub content: String,
    pub excerpt: String,
    pub visibility: NoteVisibility,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Note summary for listings (includes file id but not full content).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteSummary {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub metadata: NoteMetadata,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: UserId,
    pub current_version: i32,
    pub size: i64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

/// Errors that can occur in note operations.
#[derive(Debug, thiserror::Error)]
pub enum NoteError {
    #[error("Note not found: {0}")]
    NotFound(Uuid),
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Invalid name: {0}")]
    InvalidName(String),
}

impl From<rustshare_core::services::FileError> for NoteError {
    fn from(e: rustshare_core::services::FileError) -> Self {
        match e {
            rustshare_core::services::FileError::NotFound(id) => NoteError::NotFound(id),
            rustshare_core::services::FileError::PermissionDenied { .. } => {
                NoteError::PermissionDenied
            }
            rustshare_core::services::FileError::InvalidName(s) => NoteError::InvalidName(s),
            rustshare_core::services::FileError::Storage(s) => NoteError::Storage(s),
            rustshare_core::services::FileError::Database(e) => NoteError::Database(e.to_string()),
            _ => NoteError::Storage(e.to_string()),
        }
    }
}

use rustshare_infrastructure::repositories::PermissionResolverRepository;

/// Service for managing notes as files with sidecar metadata.
pub struct NoteService {
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

impl NoteService {
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

    fn public_share_key(share_id: &str) -> String {
        format!("meta/notes/public/{}.json", share_id)
    }

    async fn load_metadata(
        &self,
        file_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Option<NoteMetadata>, NoteError> {
        let file = self
            .file_service
            .get_file(file_id, user_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        // 1. Try visible sidecar: {path}.rustshare.json
        let sidecar_name = format!("{}.rustshare.json", file.name);
        let parent_id = file.parent_folder_id;
        let siblings = self
            .metadata_store
            .list_files(parent_id, file.owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        if let Some(sidecar) = siblings.into_iter().find(|f| f.name == sidecar_name) {
            let data = self
                .object_store
                .get(&sidecar.storage_key())
                .await
                .map_err(|e| NoteError::Storage(e.to_string()))?;
            let meta: NoteMetadata = serde_json::from_slice(&data)
                .map_err(|e| NoteError::Storage(format!("Corrupt sidecar: {}", e)))?;
            return Ok(Some(meta));
        }

        // 2. Fallback to legacy hidden sidecar
        let legacy_key = format!("meta/notes/{}.json", file_id);
        if let Ok(bytes) = self.object_store.get(&legacy_key).await {
            let meta: NoteMetadata = serde_json::from_slice(&bytes)
                .map_err(|e| NoteError::Storage(format!("Corrupt legacy sidecar: {}", e)))?;
            return Ok(Some(meta));
        }

        Ok(None)
    }

    async fn save_metadata(
        &self,
        file_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        meta: &NoteMetadata,
    ) -> Result<(), NoteError> {
        let file = self
            .file_service
            .get_file(file_id, user_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        let sidecar_name = format!("{}.rustshare.json", file.name);
        let parent_id = file.parent_folder_id;

        let siblings = self
            .metadata_store
            .list_files(parent_id, file.owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        let meta_data = serde_json::to_vec_pretty(meta)
            .map_err(|e| NoteError::Storage(format!("Failed to serialize metadata: {}", e)))?;

        if let Some(sidecar) = siblings.into_iter().find(|f| f.name == sidecar_name) {
            // Update existing sidecar
            self.file_service
                .edit_file(
                    sidecar.id,
                    file.owner_id,
                    Bytes::from(meta_data),
                    "overwrite",
                    None,
                )
                .await?;
        } else {
            // Create new sidecar
            self.file_service
                .upload_file(
                    file.owner_id,
                    sidecar_name,
                    parent_id,
                    Bytes::from(meta_data),
                    "application/json".to_string(),
                    tenant_id,
                )
                .await?;
        }

        // Also update legacy if it exists? No, let's just move forward.
        Ok(())
    }

    async fn delete_metadata(
        &self,
        file_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), NoteError> {
        let file = self
            .file_service
            .get_file(file_id, user_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;
        let sidecar_name = format!("{}.rustshare.json", file.name);
        let parent_id = file.parent_folder_id;

        let siblings = self
            .metadata_store
            .list_files(parent_id, user_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;
        if let Some(sidecar) = siblings.into_iter().find(|f| f.name == sidecar_name) {
            self.file_service.delete_file(sidecar.id, user_id).await?;
        }

        // Cleanup legacy
        let legacy_key = format!("meta/notes/{}.json", file_id);
        let _ = self.object_store.delete(&legacy_key).await;
        Ok(())
    }

    async fn delete_public_share_index(&self, share_id: &str) -> Result<(), NoteError> {
        let key = Self::public_share_key(share_id);
        let _ = self.object_store.delete(&key).await;
        Ok(())
    }

    async fn load_public_share_index(&self, share_id: &str) -> Result<Option<Uuid>, NoteError> {
        let key = Self::public_share_key(share_id);
        match self.object_store.get(&key).await {
            Ok(bytes) => {
                let id_str = String::from_utf8_lossy(&bytes);
                let id = Uuid::parse_str(id_str.trim())
                    .map_err(|e| NoteError::Storage(format!("Corrupt public index: {}", e)))?;
                Ok(Some(id))
            }
            Err(_) => Ok(None),
        }
    }

    async fn save_public_share_index(
        &self,
        share_id: &str,
        file_id: Uuid,
    ) -> Result<(), NoteError> {
        let key = Self::public_share_key(share_id);
        self.object_store
            .put(&key, Bytes::from(file_id.to_string()))
            .await
            .map_err(|e| NoteError::Storage(format!("Failed to write public index: {}", e)))?;
        Ok(())
    }

    /// Find or create the user's "Notes" folder under /Workspace.
    async fn ensure_notes_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, NoteError> {
        // Legacy: try to find existing Notes folder at root
        let root_folders = self
            .metadata_store
            .list_folders(None, owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        if let Some(notes_folder) = root_folders.into_iter().find(|f| f.name == "Notes") {
            return Ok(notes_folder);
        }

        // New: find or create Workspace, then Notes under it
        let ws = self.ensure_workspace_folder(owner_id, tenant_id).await?;
        let ws_folders = self
            .metadata_store
            .list_folders(Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        if let Some(notes_folder) = ws_folders.into_iter().find(|f| f.name == "Notes") {
            return Ok(notes_folder);
        }

        let folder = self
            .folder_service
            .create_folder("Notes".to_string(), Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Storage(e.to_string()))?;

        Ok(folder)
    }

    async fn ensure_workspace_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, NoteError> {
        let folders = self
            .metadata_store
            .list_folders(None, owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        if let Some(ws) = folders.into_iter().find(|f| f.name == "Workspace") {
            return Ok(ws);
        }

        self.folder_service
            .create_folder("Workspace".into(), None, owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Storage(e.to_string()))
    }

    /// Generate a collision-safe unique filename in the target folder.
    async fn unique_note_name(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        parent_folder_id: Option<Uuid>,
        base_name: &str,
    ) -> Result<String, NoteError> {
        let files = self
            .metadata_store
            .list_files(parent_folder_id, owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        let base = format!("{}.md", base_name);
        if !files.iter().any(|f| f.name == base) {
            return Ok(base);
        }

        for i in 2..=1000 {
            let candidate = format!("{} {}.md", base_name, i);
            if !files.iter().any(|f| f.name == candidate) {
                return Ok(candidate);
            }
        }

        Err(NoteError::InvalidName(
            "Could not find unique name".to_string(),
        ))
    }

    /// Create a new note.
    pub async fn create_note(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        title: Option<String>,
        parent_folder_id: Option<Uuid>,
        content: Option<String>,
    ) -> Result<Note, NoteError> {
        let title = title.unwrap_or_else(|| "Untitled Note".to_string());
        let content = content.unwrap_or_default();

        // Determine parent folder (default to Notes/)
        let parent_folder_id = if let Some(id) = parent_folder_id {
            Some(id)
        } else {
            let notes_folder = self.ensure_notes_folder(owner_id, tenant_id).await?;
            Some(notes_folder.id)
        };

        // Generate unique filename
        let file_name = self
            .unique_note_name(owner_id, tenant_id, parent_folder_id, &title)
            .await?;

        // Create file via FileService
        let file = self
            .file_service
            .upload_file(
                owner_id,
                file_name.clone(),
                parent_folder_id,
                Bytes::from(content.clone()),
                "text/markdown".to_string(),
                tenant_id,
            )
            .await?;

        // Build and save metadata sidecar
        let mut meta = NoteMetadata::new(title.clone());
        meta.excerpt = generate_excerpt(&content);
        self.save_metadata(file.id, owner_id, tenant_id, &meta)
            .await?;

        Ok(Note {
            id: file.id,
            name: file.name,
            path: file.path,
            content,
            metadata: meta,
            parent_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            current_version: file.current_version,
            created_at: file.created_at,
            modified_at: file.modified_at,
        })
    }

    /// Read a note by file ID.
    pub async fn get_note(&self, file_id: Uuid, user_id: UserId) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;

        let meta = self
            .load_metadata(file_id, user_id, file.tenant_id)
            .await?
            .unwrap_or_else(|| {
                // Graceful fallback for markdown files without sidecars
                let mut fallback = NoteMetadata::new(file.name.trim_end_matches(".md"));
                fallback.created_at = file.created_at;
                fallback.updated_at = file.modified_at;
                fallback
            });

        // Fetch content from object store
        let storage_key = file.storage_key();
        let content = match self.object_store.get(&storage_key).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(e) => {
                tracing::error!("Failed to load note content: {}", e);
                String::new()
            }
        };

        Ok(Note {
            id: file.id,
            name: file.name,
            path: file.path,
            content,
            metadata: meta,
            parent_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            current_version: file.current_version,
            created_at: file.created_at,
            modified_at: file.modified_at,
        })
    }

    /// Save note content (autosave).
    pub async fn save_note(
        &self,
        file_id: Uuid,
        user_id: UserId,
        content: String,
        color: Option<String>,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;

        // Extract H1 as the title if present
        let extracted_title = extract_h1_title(&content);

        // Update file content via edit_file (overwrite mode)
        let mut updated_file = self
            .file_service
            .edit_file(
                file_id,
                user_id,
                Bytes::from(content.clone()),
                "overwrite",
                None,
            )
            .await?;

        // Update sidecar
        let mut meta = self
            .load_metadata(file_id, user_id, file.tenant_id)
            .await?
            .unwrap_or_else(|| {
                let mut fallback = NoteMetadata::new(file.name.trim_end_matches(".md"));
                fallback.created_at = file.created_at;
                fallback
            });

        // Sync title and filename if H1 changed
        if let Some(new_title) = extracted_title {
            if new_title != meta.title {
                meta.title = new_title.clone();

                // Also attempt to rename the file to match the new title
                let new_filename = self
                    .unique_note_name(file.owner_id, file.tenant_id, file.parent_folder_id, &new_title)
                    .await?;

                if new_filename != updated_file.name {
                    if let Ok(renamed) = self
                        .file_service
                        .rename_file(file_id, new_filename, user_id)
                        .await
                    {
                        updated_file = renamed;
                    }
                }
            }
        }

        if let Some(new_color) = color {
            meta.color = Some(new_color);
        }

        meta.updated_at = Utc::now();
        meta.excerpt = generate_excerpt(&content);
        self.save_metadata(file_id, user_id, file.tenant_id, &meta)
            .await?;

        Ok(Note {
            id: updated_file.id,
            name: updated_file.name,
            path: updated_file.path,
            content,
            metadata: meta,
            parent_folder_id: updated_file.parent_folder_id,
            owner_id: updated_file.owner_id,
            current_version: updated_file.current_version,
            created_at: updated_file.created_at,
            modified_at: updated_file.modified_at,
        })
    }

    /// Rename a note (updates title and filename).
    pub async fn rename_note(
        &self,
        file_id: Uuid,
        user_id: UserId,
        new_title: String,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;

        // Determine new filename based on target folder
        let parent_folder_id = file.parent_folder_id;
        let new_name = self
            .unique_note_name(user_id, file.tenant_id, parent_folder_id, &new_title)
            .await?;

        // Load metadata BEFORE renaming so sidecar is found by old name
        let mut meta = self
            .load_metadata(file_id, user_id, file.tenant_id)
            .await?
            .unwrap_or_else(|| NoteMetadata::new(&new_title));

        // Rename file
        let renamed_file = self
            .file_service
            .rename_file(file_id, new_name, user_id)
            .await?;

        // Update sidecar title
        meta.title = new_title;
        meta.updated_at = Utc::now();
        self.save_metadata(file_id, user_id, file.tenant_id, &meta)
            .await?;

        // Load content for response
        let storage_key = renamed_file.storage_key();
        let content = match self.object_store.get(&storage_key).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(_) => String::new(),
        };

        Ok(Note {
            id: renamed_file.id,
            name: renamed_file.name,
            path: renamed_file.path,
            content,
            metadata: meta,
            parent_folder_id: renamed_file.parent_folder_id,
            owner_id: renamed_file.owner_id,
            current_version: renamed_file.current_version,
            created_at: renamed_file.created_at,
            modified_at: renamed_file.modified_at,
        })
    }

    /// Delete a note (and its sidecar).
    pub async fn delete_note(&self, file_id: Uuid, user_id: UserId) -> Result<(), NoteError> {
        let _file = self.file_service.get_file(file_id, user_id).await?;

        // If public, invalidate share index
        if let Some(meta) = self
            .load_metadata(file_id, user_id, _file.tenant_id)
            .await?
        {
            if let Some(share_id) = meta.public_share_id {
                let _ = self.delete_public_share_index(&share_id).await;
            }
        }

        // Delete sidecar and file
        self.delete_metadata(file_id, user_id, _file.tenant_id)
            .await?;
        self.file_service.delete_file(file_id, user_id).await?;

        Ok(())
    }

    /// Move a note to another folder.
    pub async fn move_note(
        &self,
        file_id: Uuid,
        user_id: UserId,
        target_folder_id: Option<Uuid>,
    ) -> Result<Note, NoteError> {
        let moved_file = self
            .file_service
            .move_file(file_id, target_folder_id, user_id)
            .await?;

        let mut meta = self
            .load_metadata(file_id, user_id, moved_file.tenant_id)
            .await?
            .unwrap_or_else(|| NoteMetadata::new(moved_file.name.trim_end_matches(".md")));
        meta.updated_at = Utc::now();
        self.save_metadata(file_id, user_id, moved_file.tenant_id, &meta)
            .await?;

        let storage_key = moved_file.storage_key();
        let content = match self.object_store.get(&storage_key).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(_) => String::new(),
        };

        Ok(Note {
            id: moved_file.id,
            name: moved_file.name,
            path: moved_file.path,
            content,
            metadata: meta,
            parent_folder_id: moved_file.parent_folder_id,
            owner_id: moved_file.owner_id,
            current_version: moved_file.current_version,
            created_at: moved_file.created_at,
            modified_at: moved_file.modified_at,
        })
    }

    /// List all notes for a user.
    pub async fn list_notes(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<NoteSummary>, NoteError> {
        self.list_notes_filtered(user_id, tenant_id, None, limit)
            .await
    }

    /// List notes for a user, optionally filtered to a specific folder path prefix.
    pub async fn list_notes_filtered(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        path_prefix: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<NoteSummary>, NoteError> {
        // Find all markdown files owned by the user
        let files = self
            .metadata_store
            .list_all_markdown_files(user_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        let mut notes = Vec::new();
        for file in files {
            if let Some(prefix) = path_prefix {
                if !file.path.starts_with(prefix) {
                    continue;
                }
            }

            let meta = match self.load_metadata(file.id, user_id, tenant_id).await {
                Ok(Some(m)) => m,
                _ => {
                    // Treat plain markdown files without sidecars as notes
                    let mut fallback = NoteMetadata::new(file.name.trim_end_matches(".md"));
                    fallback.created_at = file.created_at;
                    fallback.updated_at = file.modified_at;
                    fallback
                }
            };

            notes.push(NoteSummary {
                id: file.id,
                name: file.name,
                path: file.path,
                metadata: meta,
                parent_folder_id: file.parent_folder_id,
                owner_id: file.owner_id,
                current_version: file.current_version,
                size: file.size,
                created_at: file.created_at,
                modified_at: file.modified_at,
            });
        }

        // Sort by updated_at desc
        notes.sort_by_key(|b| std::cmp::Reverse(b.modified_at));

        if let Some(limit) = limit {
            notes.truncate(limit);
        }

        Ok(notes)
    }

    /// Toggle note visibility between private and public.
    pub async fn toggle_visibility(
        &self,
        file_id: Uuid,
        user_id: UserId,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;

        let mut meta = self
            .load_metadata(file_id, user_id, file.tenant_id)
            .await?
            .unwrap_or_else(|| NoteMetadata::new(file.name.trim_end_matches(".md")));

        match meta.visibility {
            NoteVisibility::Private => {
                // Make public
                let share_id = meta
                    .public_share_id
                    .clone()
                    .unwrap_or_else(generate_share_id);
                meta.visibility = NoteVisibility::Public;
                meta.public_share_id = Some(share_id.clone());
                meta.updated_at = Utc::now();
                self.save_public_share_index(&share_id, file_id).await?;
            }
            NoteVisibility::Public => {
                // Make private
                if let Some(ref share_id) = meta.public_share_id {
                    let _ = self.delete_public_share_index(share_id).await;
                }
                meta.visibility = NoteVisibility::Private;
                meta.public_share_id = None;
                meta.updated_at = Utc::now();
            }
        }

        self.save_metadata(file_id, user_id, file.tenant_id, &meta)
            .await?;

        let storage_key = file.storage_key();
        let content = match self.object_store.get(&storage_key).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(_) => String::new(),
        };

        Ok(Note {
            id: file.id,
            name: file.name,
            path: file.path,
            content,
            metadata: meta,
            parent_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            current_version: file.current_version,
            created_at: file.created_at,
            modified_at: file.modified_at,
        })
    }

    /// Read a public note by share ID (no auth required).
    pub async fn get_public_note(&self, share_id: &str) -> Result<PublicNote, NoteError> {
        let file_id = self
            .load_public_share_index(share_id)
            .await?
            .ok_or(NoteError::NotFound(Uuid::nil()))?;

        let file = self
            .metadata_store
            .find_file_by_id_unchecked(file_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?
            .ok_or(NoteError::NotFound(file_id))?;

        let meta = self
            .load_metadata(file_id, file.owner_id, file.tenant_id)
            .await?
            .ok_or(NoteError::NotFound(file_id))?;

        if meta.visibility != NoteVisibility::Public {
            return Err(NoteError::NotFound(file_id));
        }

        let storage_key = file.storage_key();
        let content = match self.object_store.get(&storage_key).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(_) => String::new(),
        };

        Ok(PublicNote {
            title: meta.title,
            content,
            excerpt: meta.excerpt,
            visibility: meta.visibility,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
        })
    }
}

fn generate_excerpt(content: &str) -> String {
    // Strip some common markdown syntax for a plain-text excerpt
    let mut plain = content
        .replace("# ", "")
        .replace("## ", "")
        .replace("### ", "")
        .replace("**", "")
        .replace(['*', '`'], "")
        .replace("\n", " ");

    plain.truncate(200);
    plain.trim().to_string()
}

fn generate_share_id() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

fn extract_h1_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let title = rest.trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}
