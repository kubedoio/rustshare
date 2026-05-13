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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteAttachment {
    pub file_id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub size: i64,
    pub created_at: DateTime<Utc>,
}

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<NoteAttachment>,
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
            attachments: Vec::new(),
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

        // For folder-backed notes, try manifest.json first
        if Self::is_folder_backed_note(&file) {
            if let Some(parent_id) = file.parent_folder_id {
                let subfolders = self
                    .metadata_store
                    .list_folders(Some(parent_id), file.owner_id, tenant_id)
                    .await
                    .map_err(|e| NoteError::Database(e.to_string()))?;

                if let Some(rustshare_folder) = subfolders.into_iter().find(|f| f.name == "_rustshare") {
                    let manifest_files = self
                        .metadata_store
                        .list_files(Some(rustshare_folder.id), file.owner_id, tenant_id)
                        .await
                        .map_err(|e| NoteError::Database(e.to_string()))?;

                    if let Some(manifest_file) = manifest_files.into_iter().find(|f| f.name == "manifest.json") {
                        let data = self
                            .object_store
                            .get(&manifest_file.storage_key())
                            .await
                            .map_err(|e| NoteError::Storage(e.to_string()))?;
                        if let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&data) {
                            if let Some(title) = manifest.get("title").and_then(|v| v.as_str()) {
                                let mut meta = NoteMetadata::new(title);
                                meta.created_at = file.created_at;
                                meta.updated_at = file.modified_at;
                                meta.excerpt = generate_excerpt(&file.name);
                                // Try to load legacy sidecar for attachments, color, etc.
                                let _ = self.enrich_from_legacy_sidecar(&mut meta, &file, user_id, tenant_id).await;
                                return Ok(Some(meta));
                            }
                        }
                    }
                }
            }
        }

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

    /// Enrich metadata from legacy sidecar for folder-backed notes.
    async fn enrich_from_legacy_sidecar(
        &self,
        meta: &mut NoteMetadata,
        file: &rustshare_core::domain::File,
        _user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), NoteError> {
        let sidecar_name = format!("{}.rustshare.json", file.name);
        let siblings = self
            .metadata_store
            .list_files(file.parent_folder_id, file.owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        if let Some(sidecar) = siblings.into_iter().find(|f| f.name == sidecar_name) {
            let data = self
                .object_store
                .get(&sidecar.storage_key())
                .await
                .map_err(|e| NoteError::Storage(e.to_string()))?;
            if let Ok(legacy) = serde_json::from_slice::<NoteMetadata>(&data) {
                meta.visibility = legacy.visibility;
                meta.public_share_id = legacy.public_share_id;
                meta.pinned = legacy.pinned;
                meta.icon = legacy.icon;
                meta.color = legacy.color;
                meta.attachments = legacy.attachments;
            }
        }
        Ok(())
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

        // For folder-backed notes, also update manifest.json
        if Self::is_folder_backed_note(&file) {
            if let Some(parent_id) = file.parent_folder_id {
                if let Ok(subfolders) = self
                    .metadata_store
                    .list_folders(Some(parent_id), file.owner_id, tenant_id)
                    .await
                {
                    if let Some(rustshare_folder) = subfolders.into_iter().find(|f| f.name == "_rustshare") {
                        if let Ok(manifest_files) = self
                            .metadata_store
                            .list_files(Some(rustshare_folder.id), file.owner_id, tenant_id)
                            .await
                        {
                            let manifest_data = serde_json::json!({
                                "type": "rustshare.note",
                                "version": 1,
                                "id": file_id.to_string(),
                                "title": meta.title,
                                "main": "note.md",
                                "created_at": meta.created_at.to_rfc3339(),
                                "updated_at": meta.updated_at.to_rfc3339(),
                                "attachments": meta.attachments.iter().map(|a| serde_json::json!({
                                    "file_id": a.file_id.to_string(),
                                    "name": a.name,
                                    "mime_type": a.mime_type,
                                    "size": a.size,
                                    "created_at": a.created_at.to_rfc3339()
                                })).collect::<Vec<_>>(),
                                "drawings": [],
                                "exports": []
                            });
                            let manifest_bytes = Bytes::from(manifest_data.to_string());

                            if let Some(manifest_file) = manifest_files.into_iter().find(|f| f.name == "manifest.json") {
                                let _ = self.file_service
                                    .edit_file(manifest_file.id, file.owner_id, manifest_bytes, "overwrite", None)
                                    .await;
                            } else {
                                let _ = self.file_service
                                    .upload_file(
                                        file.owner_id,
                                        "manifest.json".to_string(),
                                        Some(rustshare_folder.id),
                                        manifest_bytes,
                                        "application/json".to_string(),
                                        tenant_id,
                                    )
                                    .await;
                            }
                        }
                    }
                }
            }
        }

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
            .create_folder_or_get("Notes".to_string(), Some(ws.id), owner_id, tenant_id)
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
            .create_folder_or_get("Workspace".into(), None, owner_id, tenant_id)
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

    /// Generate a collision-safe unique folder name in the target folder.
    async fn unique_folder_name(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        parent_folder_id: Option<Uuid>,
        base_name: &str,
    ) -> Result<String, NoteError> {
        let folders = self
            .metadata_store
            .list_folders(parent_folder_id, owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        if !folders.iter().any(|f| f.name == base_name) {
            return Ok(base_name.to_string());
        }

        for i in 2..=1000 {
            let candidate = format!("{} {}", base_name, i);
            if !folders.iter().any(|f| f.name == candidate) {
                return Ok(candidate);
            }
        }

        Err(NoteError::InvalidName(
            "Could not find unique folder name".to_string(),
        ))
    }

    /// Check if a file represents a folder-backed note (note.md inside a bundle).
    fn is_folder_backed_note(file: &rustshare_core::domain::File) -> bool {
        file.name == "note.md"
    }

    /// Get or create a subfolder inside a note bundle.
    async fn get_or_create_subfolder(
        &self,
        parent_folder_id: Uuid,
        name: &str,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, NoteError> {
        let folders = self
            .metadata_store
            .list_folders(Some(parent_folder_id), owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        if let Some(existing) = folders.into_iter().find(|f| f.name == name) {
            return Ok(existing);
        }

        self.folder_service
            .create_folder(name.to_string(), Some(parent_folder_id), owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Storage(e.to_string()))
    }

    /// Create a new note as a folder-based bundle.
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

        // Generate collision-safe folder name
        let folder_name = self
            .unique_folder_name(owner_id, tenant_id, parent_folder_id, &title)
            .await?;

        // Create note bundle folder
        let note_folder = self
            .folder_service
            .create_folder(folder_name.clone(), parent_folder_id, owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Storage(e.to_string()))?;

        // Create subfolders
        for subfolder in &["attachments", "drawings", "exports", "_rustshare"] {
            self.get_or_create_subfolder(note_folder.id, subfolder, owner_id, tenant_id)
                .await?;
        }

        // Create note.md inside the bundle
        let file = self
            .file_service
            .upload_file(
                owner_id,
                "note.md".to_string(),
                Some(note_folder.id),
                Bytes::from(content.clone()),
                "text/markdown".to_string(),
                tenant_id,
            )
            .await?;

        // Create manifest.json inside _rustshare/
        let manifest_folder = self
            .get_or_create_subfolder(note_folder.id, "_rustshare", owner_id, tenant_id)
            .await?;
        let manifest = serde_json::json!({
            "type": "rustshare.note",
            "version": 1,
            "id": file.id.to_string(),
            "title": title,
            "main": "note.md",
            "created_at": chrono::Utc::now().to_rfc3339(),
            "updated_at": chrono::Utc::now().to_rfc3339(),
            "attachments": [],
            "drawings": [],
            "exports": []
        });
        self.file_service
            .upload_file(
                owner_id,
                "manifest.json".to_string(),
                Some(manifest_folder.id),
                Bytes::from(manifest.to_string()),
                "application/json".to_string(),
                tenant_id,
            )
            .await
            .ok(); // Best-effort; don't fail note creation if manifest fails

        // Build and save metadata sidecar (legacy compat)
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
        attachments: Option<Vec<NoteAttachment>>,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        let is_folder_backed = Self::is_folder_backed_note(&file);

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

                if is_folder_backed {
                    // For folder-backed notes, rename the parent folder
                    if let Some(parent_folder_id) = file.parent_folder_id {
                        let parent_folder = self
                            .folder_service
                            .get_folder(parent_folder_id, user_id)
                            .await
                            .map_err(|e| NoteError::Storage(e.to_string()))?;
                        let new_folder_name = self
                            .unique_folder_name(file.owner_id, file.tenant_id, parent_folder.parent_folder_id, &new_title)
                            .await?;
                        if new_folder_name != parent_folder.name {
                            let _ = self
                                .folder_service
                                .rename_folder(parent_folder_id, new_folder_name, user_id)
                                .await;
                        }
                    }
                } else {
                    // Legacy: rename the file
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
        }

        if let Some(new_color) = color {
            meta.color = Some(new_color);
        }

        if let Some(new_attachments) = attachments {
            meta.attachments = new_attachments;
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

    /// Rename a note (updates title and filename/folder name).
    pub async fn rename_note(
        &self,
        file_id: Uuid,
        user_id: UserId,
        new_title: String,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        let is_folder_backed = Self::is_folder_backed_note(&file);

        // Load metadata BEFORE renaming so sidecar is found by old name
        let mut meta = self
            .load_metadata(file_id, user_id, file.tenant_id)
            .await?
            .unwrap_or_else(|| NoteMetadata::new(&new_title));

        if is_folder_backed {
            // For folder-backed notes, rename the parent folder
            if let Some(parent_folder_id) = file.parent_folder_id {
                let parent_folder = self
                    .folder_service
                    .get_folder(parent_folder_id, user_id)
                    .await
                    .map_err(|e| NoteError::Storage(e.to_string()))?;
                let new_folder_name = self
                    .unique_folder_name(user_id, file.tenant_id, parent_folder.parent_folder_id, &new_title)
                    .await?;
                if new_folder_name != parent_folder.name {
                    self.folder_service
                        .rename_folder(parent_folder_id, new_folder_name, user_id)
                        .await
                        .map_err(|e| NoteError::Storage(e.to_string()))?;
                }
            }
        } else {
            // Legacy: rename the file
            let parent_folder_id = file.parent_folder_id;
            let new_name = self
                .unique_note_name(user_id, file.tenant_id, parent_folder_id, &new_title)
                .await?;

            self.file_service
                .rename_file(file_id, new_name, user_id)
                .await?;
        }

        // Update sidecar title
        meta.title = new_title;
        meta.updated_at = Utc::now();
        self.save_metadata(file_id, user_id, file.tenant_id, &meta)
            .await?;

        // Reload file after potential rename
        let file = self.file_service.get_file(file_id, user_id).await?;

        // Load content for response
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

    /// Delete a note (and its sidecar).
    pub async fn delete_note(&self, file_id: Uuid, user_id: UserId) -> Result<(), NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        let is_folder_backed = Self::is_folder_backed_note(&file);

        // If public, invalidate share index
        if let Some(meta) = self
            .load_metadata(file_id, user_id, file.tenant_id)
            .await?
        {
            if let Some(share_id) = meta.public_share_id {
                let _ = self.delete_public_share_index(&share_id).await;
            }

            // Delete attachment files
            for attachment in &meta.attachments {
                let _ = self.file_service.delete_file(attachment.file_id, user_id).await;
            }
        }

        if is_folder_backed {
            // For folder-backed notes, delete the entire bundle folder
            if let Some(parent_folder_id) = file.parent_folder_id {
                // Delete metadata sidecar first
                let _ = self.delete_metadata(file_id, user_id, file.tenant_id).await;
                // Delete note.md file
                let _ = self.file_service.delete_file(file_id, user_id).await;
                // Delete the bundle folder (cascade deletes subfolders and remaining files)
                let _ = self.folder_service.delete_folder(parent_folder_id, user_id).await;
            }
        } else {
            // Legacy: delete sidecar and file
            self.delete_metadata(file_id, user_id, file.tenant_id)
                .await?;
            self.file_service.delete_file(file_id, user_id).await?;
        }

        Ok(())
    }

    /// Move a note to another folder.
    pub async fn move_note(
        &self,
        file_id: Uuid,
        user_id: UserId,
        target_folder_id: Option<Uuid>,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        let is_folder_backed = Self::is_folder_backed_note(&file);

        let moved_file = if is_folder_backed {
            // For folder-backed notes, move the parent bundle folder
            if let Some(parent_folder_id) = file.parent_folder_id {
                self.folder_service
                    .move_folder(parent_folder_id, target_folder_id, user_id)
                    .await
                    .map_err(|e| NoteError::Storage(e.to_string()))?;
            }
            // Reload file after move
            self.file_service.get_file(file_id, user_id).await?
        } else {
            self.file_service
                .move_file(file_id, target_folder_id, user_id)
                .await?
        };

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
        // Load all markdown files but filter to Notes paths only
        let files = self
            .metadata_store
            .list_all_markdown_files(user_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        let mut notes = Vec::new();
        for file in files {
            if !(file.path.starts_with("/Workspace/Notes/") || file.path.starts_with("/Notes/")) {
                continue;
            }

            let meta = match self.load_metadata(file.id, user_id, tenant_id).await {
                Ok(Some(m)) => {
                    if m.kind != "note" {
                        continue; // Skip non-note artifacts
                    }
                    m
                }
                _ => {
                    // Treat plain markdown files without sidecars as notes
                    let mut fallback = NoteMetadata::new(file.name.trim_end_matches(".md"));
                    fallback.created_at = file.created_at;
                    fallback.updated_at = file.modified_at;
                    fallback
                }
            };

            // For folder-backed notes, derive display name from parent folder
            let display_name = if Self::is_folder_backed_note(&file) {
                if let Some(parent_id) = file.parent_folder_id {
                    self.metadata_store
                        .find_folder_by_id(parent_id, user_id)
                        .await
                        .ok()
                        .flatten()
                        .map(|f| f.name)
                        .unwrap_or_else(|| file.name.clone())
                } else {
                    file.name.clone()
                }
            } else {
                file.name.clone()
            };

            notes.push(NoteSummary {
                id: file.id,
                name: display_name,
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

        notes.sort_by_key(|b| std::cmp::Reverse(b.modified_at));

        if let Some(limit) = limit {
            notes.truncate(limit);
        }

        Ok(notes)
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
                Ok(Some(m)) => {
                    if m.kind != "note" {
                        continue; // Skip non-note artifacts (decisions, etc.)
                    }
                    m
                }
                _ => {
                    // Treat plain markdown files without sidecars as notes
                    let mut fallback = NoteMetadata::new(file.name.trim_end_matches(".md"));
                    fallback.created_at = file.created_at;
                    fallback.updated_at = file.modified_at;
                    fallback
                }
            };

            // For folder-backed notes, derive display name from parent folder
            let display_name = if Self::is_folder_backed_note(&file) {
                if let Some(parent_id) = file.parent_folder_id {
                    self.metadata_store
                        .find_folder_by_id(parent_id, user_id)
                        .await
                        .ok()
                        .flatten()
                        .map(|f| f.name)
                        .unwrap_or_else(|| file.name.clone())
                } else {
                    file.name.clone()
                }
            } else {
                file.name.clone()
            };

            notes.push(NoteSummary {
                id: file.id,
                name: display_name,
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
