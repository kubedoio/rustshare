//! Note service for RustShare notes MVP-1.
//!
//! Notes are first-class markdown files with metadata sidecars stored in object storage.

use crate::services::note_index_sink::NoteIndexSink;

use anyhow::Result;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use rustshare_core::{
    domain::{Folder, UserId},
    okf::frontmatter::{
        default_note_frontmatter, merge_required_okf_keys, parse_frontmatter, to_document,
        OkfNoteFrontmatter, RustshareFrontmatter,
    },
    services::{FileService, FolderService, NoteAclPayload, PermissionResolver},
};
use rustshare_storage::{MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NoteAttachment {
    pub file_id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub size: i64,
    pub created_at: DateTime<Utc>,
}

/// Note-specific metadata sidecar schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub okf_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acl_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acl_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<NoteConflict>,
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
            okf_id: None,
            acl_hash: None,
            acl_version: None,
            conflict: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
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

/// A reconciliation conflict detected between the YAML frontmatter, sidecar,
/// and bundle folder name of an OKF-native note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NoteConflict {
    pub kind: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaml_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaml_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidecar_id: Option<Uuid>,
}

/// Resolution strategy for a [`NoteConflict`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NoteConflictResolution {
    PreferYaml,
    PreferFolder,
    Custom(String),
}

impl NoteConflictResolution {
    fn validate_custom_title(title: &str) -> Result<String, NoteError> {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return Err(NoteError::InvalidName(
                "Custom title cannot be empty".to_string(),
            ));
        }
        Ok(trimmed.to_string())
    }
}

/// Unified note payload returned to clients.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Note {
    pub id: Uuid,
    pub okf_id: Option<Uuid>,
    pub name: String,
    pub path: String,
    pub content: String,
    pub metadata: NoteMetadata,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub current_version: i32,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<NoteConflict>,
}

/// Public note view (no internal identifiers).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PublicNote {
    pub title: String,
    pub content: String,
    pub excerpt: String,
    pub visibility: NoteVisibility,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Note summary for listings (includes file id but not full content).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NoteSummary {
    pub id: Uuid,
    pub okf_id: Option<Uuid>,
    pub name: String,
    pub path: String,
    pub metadata: NoteMetadata,
    pub parent_folder_id: Option<Uuid>,
    #[schema(value_type = Uuid)]
    pub owner_id: Uuid,
    pub current_version: i32,
    pub size: i64,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub attachment_count: i64,
    pub drawing_count: i64,
    pub export_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<NoteConflict>,
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
            rustshare_core::services::FileError::Database(e) => NoteError::Database(e),
            _ => NoteError::Storage(e.to_string()),
        }
    }
}

use rustshare_infrastructure::repositories::PermissionResolverRepository;

/// Service for managing notes as files with sidecar metadata.
#[derive(Clone)]
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
    permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    pub workspace_name: String,
    pub folder_name: String,
    index_sink: Option<Arc<dyn NoteIndexSink>>,
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
        permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    ) -> Self {
        Self {
            file_service,
            folder_service,
            metadata_store,
            object_store,
            permission_resolver,
            workspace_name: "Workspace".to_string(),
            folder_name: "Notes".to_string(),
            index_sink: None,
        }
    }

    /// Attach an optional indexing callback sink.
    pub fn with_index_sink(mut self, sink: Option<Arc<dyn NoteIndexSink>>) -> Self {
        self.index_sink = sink;
        self
    }

    /// Customize the workspace and folder names.
    pub fn with_custom_paths(mut self, workspace_name: String, folder_name: String) -> Self {
        self.workspace_name = workspace_name;
        self.folder_name = folder_name;
        self
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

        // 1. Try visible sidecar: {path}.rustshare.json (source of truth for OKF metadata).
        let sidecar_name = format!("{}.rustshare.json", file.name);
        let parent_id = file.parent_folder_id;
        let siblings = self
            .metadata_store
            .list_files(parent_id, file.owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        if let Some(sidecar) = siblings.iter().find(|f| f.name == sidecar_name) {
            let data = self
                .object_store
                .get(&sidecar.storage_key())
                .await
                .map_err(|e| NoteError::Storage(e.to_string()))?;
            let mut meta: NoteMetadata = serde_json::from_slice(&data)
                .map_err(|e| NoteError::Storage(format!("Corrupt sidecar: {}", e)))?;

            // For folder-backed notes, fall back to manifest title if sidecar has no title.
            if Self::is_folder_backed_note(&file) && meta.title.is_empty() {
                if let Some(manifest_title) = self
                    .load_manifest_title(file.parent_folder_id, file.owner_id, tenant_id)
                    .await
                {
                    meta.title = manifest_title;
                }
            }
            meta.created_at = file.created_at;
            meta.updated_at = file.modified_at;
            return Ok(Some(meta));
        }

        // 2. For folder-backed notes without a visible sidecar, try manifest.json.
        if Self::is_folder_backed_note(&file) {
            if let Some(title) = self
                .load_manifest_title(file.parent_folder_id, file.owner_id, tenant_id)
                .await
            {
                let mut meta = NoteMetadata::new(title);
                meta.created_at = file.created_at;
                meta.updated_at = file.modified_at;
                meta.excerpt = generate_excerpt(&file.name);
                // Try to load legacy sidecar for attachments, color, etc.
                let _ = self
                    .enrich_from_legacy_sidecar(&mut meta, &file, user_id, tenant_id)
                    .await;
                return Ok(Some(meta));
            }
        }

        // 3. Fallback to legacy hidden sidecar
        let legacy_key = format!("meta/notes/{}.json", file_id);
        if let Ok(bytes) = self.object_store.get(&legacy_key).await {
            let meta: NoteMetadata = serde_json::from_slice(&bytes)
                .map_err(|e| NoteError::Storage(format!("Corrupt legacy sidecar: {}", e)))?;
            return Ok(Some(meta));
        }

        Ok(None)
    }

    async fn load_manifest_title(
        &self,
        parent_folder_id: Option<Uuid>,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Option<String> {
        let (_, manifest) = self
            .load_manifest(parent_folder_id, owner_id, tenant_id)
            .await?;
        manifest.get("title")?.as_str().map(|s| s.to_string())
    }

    /// Load the raw manifest.json file and value for a folder-backed note.
    async fn load_manifest(
        &self,
        parent_folder_id: Option<Uuid>,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Option<(rustshare_core::domain::File, serde_json::Value)> {
        let parent_id = parent_folder_id?;
        let subfolders = self
            .metadata_store
            .list_folders(Some(parent_id), owner_id, tenant_id)
            .await
            .ok()?;
        let rustshare_folder = subfolders.into_iter().find(|f| f.name == "_rustshare")?;
        let manifest_files = self
            .metadata_store
            .list_files(Some(rustshare_folder.id), owner_id, tenant_id)
            .await
            .ok()?;
        let manifest_file = manifest_files
            .into_iter()
            .find(|f| f.name == "manifest.json")?;
        let data = self
            .object_store
            .get(&manifest_file.storage_key())
            .await
            .ok()?;
        let manifest = serde_json::from_slice::<serde_json::Value>(&data).ok()?;
        Some((manifest_file, manifest))
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
                    if let Some(rustshare_folder) =
                        subfolders.into_iter().find(|f| f.name == "_rustshare")
                    {
                        if let Ok(manifest_files) = self
                            .metadata_store
                            .list_files(Some(rustshare_folder.id), file.owner_id, tenant_id)
                            .await
                        {
                            let manifest_data = serde_json::json!({
                                "type": "rustshare.note",
                                "version": 1,
                                "id": file_id.to_string(),
                                "rustshare_id": meta.okf_id.map(|id| id.to_string()).unwrap_or_default(),
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
                                "exports": [],
                                "conflict": meta.conflict,
                            });
                            let manifest_bytes = Bytes::from(manifest_data.to_string());

                            if let Some(manifest_file) = manifest_files
                                .into_iter()
                                .find(|f| f.name == "manifest.json")
                            {
                                let _ = self
                                    .file_service
                                    .edit_file(
                                        manifest_file.id,
                                        file.owner_id,
                                        manifest_bytes,
                                        "overwrite",
                                        None,
                                    )
                                    .await;
                            } else {
                                let _ = self
                                    .file_service
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

    fn build_note(
        &self,
        file: rustshare_core::domain::File,
        meta: NoteMetadata,
        content: String,
        conflict: Option<NoteConflict>,
    ) -> Note {
        Note {
            id: file.id,
            okf_id: meta.okf_id,
            name: file.name.clone(),
            path: file.path,
            content,
            metadata: meta,
            parent_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            current_version: file.current_version,
            created_at: file.created_at,
            modified_at: file.modified_at,
            conflict,
        }
    }

    /// Resolve every principal that has read access to a note.
    pub async fn resolve_note_read_principals(
        &self,
        file: &rustshare_core::domain::File,
        tenant_id: Uuid,
    ) -> Result<Vec<String>, NoteError> {
        self.permission_resolver
            .resolve_read_principals(file, tenant_id)
            .await
            .map_err(|e| NoteError::Database(format!("Failed to resolve ACL principals: {e}")))
    }

    /// Build the ACL payload used by the AI content indexer.
    pub fn build_acl_payload(
        file: &rustshare_core::domain::File,
        meta: &NoteMetadata,
        tenant_id: Uuid,
        read_acl: Vec<String>,
    ) -> NoteAclPayload {
        NoteAclPayload {
            tenant_id,
            workspace_id: tenant_id,
            note_id: meta.okf_id.unwrap_or(file.id),
            source_file_id: file.id,
            source_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            read_acl,
            visibility: meta.visibility.as_str().to_string(),
            acl_hash: meta.acl_hash.clone().unwrap_or_default(),
            acl_version: meta.acl_version.unwrap_or(1),
            embedding_policy: "allowed".to_string(),
        }
    }

    async fn emit_index_note(
        &self,
        file: &rustshare_core::domain::File,
        meta: &NoteMetadata,
        content: &str,
        tenant_id: Uuid,
    ) {
        if let Some(sink) = &self.index_sink {
            let read_acl = match self.resolve_note_read_principals(file, tenant_id).await {
                Ok(acl) => acl,
                Err(e) => {
                    tracing::warn!("Failed to resolve ACL principals for {}: {}", file.id, e);
                    return;
                }
            };
            let acl = Self::build_acl_payload(file, meta, tenant_id, read_acl);
            sink.index_note(
                file.id,
                file.name.clone(),
                file.path.clone(),
                content.to_string(),
                meta.mime_type.clone(),
                file.owner_id,
                acl,
            )
            .await;
        } else {
            tracing::debug!("No note index sink configured; skipping indexing");
        }
    }

    /// Find or create the target folder under workspace.
    ///
    /// Legacy module root policy: new writes are always directed to the
    /// canonical /Workspace/<Module> path. Legacy roots are read-only.
    async fn ensure_target_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, NoteError> {
        let ws = self.ensure_workspace_folder(owner_id, tenant_id).await?;
        let ws_folders = self
            .metadata_store
            .list_folders(Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        if let Some(target_folder) = ws_folders.into_iter().find(|f| f.name == self.folder_name) {
            return Ok(target_folder);
        }

        let folder = self
            .folder_service
            .create_folder_or_get(self.folder_name.clone(), Some(ws.id), owner_id, tenant_id)
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

        if let Some(ws) = folders.into_iter().find(|f| f.name == self.workspace_name) {
            return Ok(ws);
        }

        self.folder_service
            .create_folder_or_get(self.workspace_name.clone(), None, owner_id, tenant_id)
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

    /// Returns true if a file is hidden metadata that should not be counted or exposed.
    fn is_hidden_metadata_file(name: &str) -> bool {
        name.starts_with(".rustshare") || name.ends_with(".editor.json")
    }

    /// Count visible files in a note bundle's attachments, drawings, and exports subfolders.
    async fn count_bundle_contents(
        &self,
        bundle_folder_id: Uuid,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(i64, i64, i64), NoteError> {
        let subfolders = self
            .metadata_store
            .list_folders(Some(bundle_folder_id), owner_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        let mut attachment_count = 0i64;
        let mut drawing_count = 0i64;
        let mut export_count = 0i64;

        for subfolder in subfolders {
            match subfolder.name.as_str() {
                "attachments" => {
                    let files = self
                        .metadata_store
                        .list_files(Some(subfolder.id), owner_id, tenant_id)
                        .await
                        .map_err(|e| NoteError::Database(e.to_string()))?;
                    attachment_count = files
                        .into_iter()
                        .filter(|f| !Self::is_hidden_metadata_file(&f.name))
                        .count() as i64;
                }
                "drawings" => {
                    let files = self
                        .metadata_store
                        .list_files(Some(subfolder.id), owner_id, tenant_id)
                        .await
                        .map_err(|e| NoteError::Database(e.to_string()))?;
                    drawing_count = files
                        .into_iter()
                        .filter(|f| !Self::is_hidden_metadata_file(&f.name))
                        .count() as i64;
                }
                "exports" => {
                    let files = self
                        .metadata_store
                        .list_files(Some(subfolder.id), owner_id, tenant_id)
                        .await
                        .map_err(|e| NoteError::Database(e.to_string()))?;
                    export_count = files
                        .into_iter()
                        .filter(|f| !Self::is_hidden_metadata_file(&f.name))
                        .count() as i64;
                }
                _ => {}
            }
        }

        Ok((attachment_count, drawing_count, export_count))
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
            .create_folder(
                name.to_string(),
                Some(parent_folder_id),
                owner_id,
                tenant_id,
            )
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

        // Stable OKF identity for this note.
        let okf_id = Uuid::new_v4();
        let acl_hash = compute_acl_hash(tenant_id, tenant_id, okf_id, "private");
        let acl_version = 1i64;

        // Determine parent folder (default to generic target folder)
        let parent_folder_id = if let Some(id) = parent_folder_id {
            Some(id)
        } else {
            let target_folder = self.ensure_target_folder(owner_id, tenant_id).await?;
            Some(target_folder.id)
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

        // Build OKF-native note.md content. The body must start with `# {title}\n\n`.
        let body = if content.trim_start().starts_with("# ") {
            let first_line = content.trim_start().lines().next().unwrap_or("");
            let title_from_content = first_line.strip_prefix("# ").unwrap_or("").trim();
            if title_from_content == title {
                content
            } else {
                format!("# {}\n\n{}", title, content)
            }
        } else if content.is_empty() {
            format!("# {}\n\n", title)
        } else {
            format!("# {}\n\n{}", title, content)
        };
        let frontmatter = default_note_frontmatter(
            &title, okf_id, tenant_id, // workspace_id_or_tenant_id
            "",        // source_id; empty means source_id defaults to note_id
            &title,    // bundle_name
            &acl_hash,
        );
        let document = to_document(&frontmatter, &body)
            .map_err(|e| NoteError::Storage(format!("Failed to serialize frontmatter: {}", e)))?;

        // Create note.md inside the bundle
        let file = self
            .file_service
            .upload_file(
                owner_id,
                "note.md".to_string(),
                Some(note_folder.id),
                Bytes::from(document.clone()),
                "text/markdown".to_string(),
                tenant_id,
            )
            .await?;

        // Create manifest.json inside _rustshare/
        let manifest_folder = self
            .get_or_create_subfolder(note_folder.id, "_rustshare", owner_id, tenant_id)
            .await?;
        let now = chrono::Utc::now();
        let manifest = serde_json::json!({
            "type": "rustshare.note",
            "version": 1,
            "id": file.id.to_string(),
            "rustshare_id": okf_id.to_string(),
            "title": title,
            "main": "note.md",
            "created_at": now.to_rfc3339(),
            "updated_at": now.to_rfc3339(),
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
        meta.okf_id = Some(okf_id);
        meta.acl_hash = Some(acl_hash);
        meta.acl_version = Some(acl_version);
        meta.excerpt = generate_excerpt(&body);
        self.save_metadata(file.id, owner_id, tenant_id, &meta)
            .await?;

        self.emit_index_note(&file, &meta, &document, tenant_id)
            .await;

        Ok(Note {
            id: file.id,
            okf_id: Some(okf_id),
            name: file.name.clone(),
            path: file.path,
            content: document,
            metadata: meta,
            parent_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            current_version: file.current_version,
            created_at: file.created_at,
            modified_at: file.modified_at,
            conflict: None,
        })
    }

    /// Read a note by file ID.
    ///
    /// Reconciles any external edits to the frontmatter or bundle folder before
    /// returning, surfacing conflicts in the response.
    pub async fn get_note(
        &self,
        file_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(NoteError::PermissionDenied);
        }

        // Fetch content from object store before reconciliation so external
        // edits are visible.
        let storage_key = file.storage_key();
        let _content = match self.object_store.get(&storage_key).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(e) => {
                tracing::error!("Failed to load note content: {}", e);
                String::new()
            }
        };

        self.reconcile_note(file_id, user_id, tenant_id).await
    }

    /// Reconcile external edits to an OKF-native note.
    ///
    /// Compares the note.md frontmatter, sidecar metadata, manifest, and bundle
    /// folder name, then applies YAML-title changes, folder-rename changes, or
    /// records a conflict when the two disagree.
    pub async fn reconcile_note(
        &self,
        file_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(NoteError::PermissionDenied);
        }

        let mut meta = self
            .load_metadata(file_id, user_id, tenant_id)
            .await?
            .unwrap_or_else(|| {
                let mut fallback = NoteMetadata::new(file.name.trim_end_matches(".md"));
                fallback.created_at = file.created_at;
                fallback.updated_at = file.modified_at;
                fallback
            });

        let storage_key = file.storage_key();
        let content = match self.object_store.get(&storage_key).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(e) => {
                tracing::error!("Failed to load note content: {}", e);
                String::new()
            }
        };

        let (mut fm, body) = parse_frontmatter(&content)
            .map_err(|e| NoteError::Storage(format!("Failed to parse frontmatter: {}", e)))?;

        let is_folder_backed = Self::is_folder_backed_note(&file);

        // ------------------------------------------------------------------
        // Identity reconciliation
        // ------------------------------------------------------------------
        let yaml_id = fm.rustshare.as_ref().and_then(|rs| rs.id);
        let sidecar_id = meta.okf_id;

        if let (Some(y), Some(s)) = (yaml_id, sidecar_id) {
            if y != s {
                let conflict = NoteConflict {
                    kind: "identity_mismatch".to_string(),
                    message: format!(
                        "Frontmatter rustshare.id ({}) does not match sidecar okf_id ({}).",
                        y, s
                    ),
                    yaml_title: fm.title.clone(),
                    folder_name: None,
                    manifest_title: None,
                    yaml_id: Some(y),
                    sidecar_id: Some(s),
                };
                meta.conflict = Some(conflict.clone());
                meta.updated_at = Utc::now();
                self.save_metadata(file_id, user_id, tenant_id, &meta)
                    .await?;
                return Ok(self.build_note(file, meta, content, Some(conflict)));
            }
        }

        // Repair missing ids, anchoring identity to the sidecar when present.
        let resolved_id = if let Some(id) = sidecar_id {
            if yaml_id != Some(id) {
                let mut rs = fm.rustshare.take().unwrap_or_default();
                rs.id = Some(id);
                fm.rustshare = Some(rs);
            }
            id
        } else if let Some(id) = yaml_id {
            meta.okf_id = Some(id);
            id
        } else {
            let new_id = Uuid::new_v4();
            let mut rs = fm.rustshare.take().unwrap_or_default();
            rs.id = Some(new_id);
            fm.rustshare = Some(rs);
            meta.okf_id = Some(new_id);
            new_id
        };

        if is_folder_backed {
            let mut rs = fm.rustshare.take().unwrap_or_default();
            if rs.bundle_name.is_none() {
                rs.bundle_name = Some(meta.title.clone());
            }
            fm.rustshare = Some(rs);
        }

        // ------------------------------------------------------------------
        // Title / folder reconciliation (folder-backed notes only)
        // ------------------------------------------------------------------
        let mut frontmatter_changed = yaml_id != fm.rustshare.as_ref().and_then(|rs| rs.id);
        let mut metadata_changed = sidecar_id != meta.okf_id;

        if is_folder_backed {
            let parent_folder_id = file.parent_folder_id.ok_or_else(|| {
                NoteError::Storage("Folder-backed note has no parent folder".to_string())
            })?;
            let bundle_folder = self
                .metadata_store
                .find_folder_by_id(parent_folder_id, user_id)
                .await
                .map_err(|e| NoteError::Database(e.to_string()))?
                .ok_or(NoteError::NotFound(parent_folder_id))?;

            let yaml_title = fm.title.clone().unwrap_or_default();
            let folder_name = bundle_folder.name.clone();
            let manifest_title = self
                .load_manifest_title(Some(parent_folder_id), user_id, tenant_id)
                .await
                .unwrap_or_else(|| meta.title.clone());

            let yaml_mtime = file.modified_at;
            let folder_mtime = bundle_folder.updated_at;
            let manifest_mtime = self
                .load_manifest(Some(parent_folder_id), user_id, tenant_id)
                .await
                .map(|(mf, _)| mf.modified_at)
                .unwrap_or(meta.updated_at);

            let yaml_newer = yaml_mtime >= manifest_mtime && yaml_mtime >= folder_mtime;
            let folder_newer = folder_mtime >= yaml_mtime && folder_mtime >= manifest_mtime;

            let yaml_diverged = !yaml_title.is_empty() && yaml_title != manifest_title;
            let folder_diverged = !folder_name.is_empty() && folder_name != manifest_title;

            if yaml_diverged && folder_diverged && yaml_title != folder_name {
                // Both sources changed independently and disagree.
                let conflict = NoteConflict {
                    kind: "title_mismatch".to_string(),
                    message: format!(
                        "YAML title ({:?}) and folder name ({:?}) both changed and differ; manifest title was {:?}.",
                        yaml_title, folder_name, manifest_title
                    ),
                    yaml_title: Some(yaml_title.clone()),
                    folder_name: Some(folder_name.clone()),
                    manifest_title: Some(manifest_title.clone()),
                    yaml_id: Some(resolved_id),
                    sidecar_id: Some(resolved_id),
                };
                meta.conflict = Some(conflict.clone());
                meta.updated_at = Utc::now();
                self.save_metadata(file_id, user_id, tenant_id, &meta)
                    .await?;
                return Ok(self.build_note(file, meta, content, Some(conflict)));
            } else if yaml_diverged && yaml_newer {
                // YAML wins: update metadata and folder name.
                meta.title = yaml_title.clone();
                meta.conflict = None;
                meta.updated_at = Utc::now();
                metadata_changed = true;

                let mut rs = fm.rustshare.take().unwrap_or_default();
                rs.bundle_name = Some(yaml_title.clone());
                fm.rustshare = Some(rs);
                frontmatter_changed = true;

                let unique_name = self
                    .unique_folder_name(
                        user_id,
                        tenant_id,
                        bundle_folder.parent_folder_id,
                        &yaml_title,
                    )
                    .await?;
                if unique_name != folder_name {
                    self.folder_service
                        .rename_folder(parent_folder_id, unique_name, user_id)
                        .await
                        .map_err(|e| NoteError::Storage(e.to_string()))?;
                }
            } else if folder_diverged && folder_newer {
                // Folder wins: update YAML title and metadata.
                meta.title = folder_name.clone();
                meta.conflict = None;
                meta.updated_at = Utc::now();
                metadata_changed = true;

                fm.title = Some(folder_name.clone());
                let mut rs = fm.rustshare.take().unwrap_or_default();
                rs.bundle_name = Some(folder_name.clone());
                fm.rustshare = Some(rs);
                frontmatter_changed = true;
            } else if yaml_diverged && folder_diverged && yaml_title == folder_name {
                // Both sources independently changed to the same new title.
                meta.title = yaml_title.clone();
                meta.conflict = None;
                meta.updated_at = Utc::now();
                metadata_changed = true;

                let mut rs = fm.rustshare.take().unwrap_or_default();
                rs.bundle_name = Some(yaml_title.clone());
                fm.rustshare = Some(rs);
                frontmatter_changed = true;
            } else {
                // No reconciliation action; clear any stale conflict if the
                // sources are now in sync.
                if meta.conflict.is_some()
                    && yaml_title == folder_name
                    && yaml_title == manifest_title
                {
                    meta.conflict = None;
                    metadata_changed = true;
                }
            }
        }

        // Persist any frontmatter or metadata repairs.
        if frontmatter_changed {
            let document = to_document(&fm, &body).map_err(|e| {
                NoteError::Storage(format!("Failed to serialize frontmatter: {}", e))
            })?;
            self.file_service
                .edit_file(file_id, user_id, Bytes::from(document), "overwrite", None)
                .await?;
            metadata_changed = true;
        }

        if metadata_changed {
            self.save_metadata(file_id, user_id, tenant_id, &meta)
                .await?;
        }

        // Reload file so the returned path reflects any folder rename.
        let file = self.file_service.get_file(file_id, user_id).await?;
        let content = match self.object_store.get(&file.storage_key()).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(_) => {
                // Best-effort: reconstruct from the in-memory frontmatter.
                to_document(&fm, &body).unwrap_or_else(|_| content.clone())
            }
        };

        let conflict = meta.conflict.clone();
        Ok(self.build_note(file, meta, content, conflict))
    }

    /// Resolve a recorded [`NoteConflict`] by picking one source of truth.
    pub async fn resolve_note_conflict(
        &self,
        file_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        resolution: NoteConflictResolution,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(NoteError::PermissionDenied);
        }

        let mut meta = self
            .load_metadata(file_id, user_id, tenant_id)
            .await?
            .unwrap_or_else(|| NoteMetadata::new(file.name.trim_end_matches(".md")));

        let storage_key = file.storage_key();
        let content = self
            .object_store
            .get(&storage_key)
            .await
            .map_err(|e| NoteError::Storage(e.to_string()))?;
        let content_str = String::from_utf8_lossy(&content);
        let (mut fm, body) = parse_frontmatter(&content_str)
            .map_err(|e| NoteError::Storage(format!("Failed to parse frontmatter: {}", e)))?;

        let is_folder_backed = Self::is_folder_backed_note(&file);

        let resolved_title = match &resolution {
            NoteConflictResolution::PreferYaml => fm.title.clone().or(Some(meta.title.clone())),
            NoteConflictResolution::PreferFolder => {
                if is_folder_backed {
                    if let Some(pid) = file.parent_folder_id {
                        self.metadata_store
                            .find_folder_by_id(pid, user_id)
                            .await
                            .ok()
                            .flatten()
                            .map(|f| f.name)
                    } else {
                        None
                    }
                } else {
                    Some(file.name.trim_end_matches(".md").to_string())
                }
            }
            NoteConflictResolution::Custom(title) => {
                Some(NoteConflictResolution::validate_custom_title(title)?)
            }
        };

        let title = resolved_title.ok_or_else(|| {
            NoteError::InvalidName("Could not resolve conflict title".to_string())
        })?;

        meta.title = title.clone();
        meta.conflict = None;
        meta.updated_at = Utc::now();

        fm.title = Some(title.clone());
        let mut rs = fm.rustshare.take().unwrap_or_default();
        rs.bundle_name = Some(title.clone());
        fm.rustshare = Some(rs);

        let document = to_document(&fm, &body)
            .map_err(|e| NoteError::Storage(format!("Failed to serialize frontmatter: {}", e)))?;
        self.file_service
            .edit_file(
                file_id,
                user_id,
                Bytes::from(document.clone()),
                "overwrite",
                None,
            )
            .await?;

        if is_folder_backed {
            if let Some(parent_id) = file.parent_folder_id {
                let bundle_folder = self
                    .metadata_store
                    .find_folder_by_id(parent_id, user_id)
                    .await
                    .map_err(|e| NoteError::Database(e.to_string()))?
                    .ok_or(NoteError::NotFound(parent_id))?;
                // If the resolved title already matches the current bundle folder
                // name, keep it. Otherwise generate a collision-safe name.
                let unique_name = if title == bundle_folder.name {
                    bundle_folder.name.clone()
                } else {
                    self.unique_folder_name(
                        user_id,
                        tenant_id,
                        bundle_folder.parent_folder_id,
                        &title,
                    )
                    .await?
                };
                if unique_name != bundle_folder.name {
                    self.folder_service
                        .rename_folder(parent_id, unique_name, user_id)
                        .await
                        .map_err(|e| NoteError::Storage(e.to_string()))?;
                }
            }
        }

        self.save_metadata(file_id, user_id, tenant_id, &meta)
            .await?;

        let file = self.file_service.get_file(file_id, user_id).await?;
        let content = match self.object_store.get(&file.storage_key()).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(_) => document,
        };

        Ok(self.build_note(file, meta, content, None))
    }

    /// Scan notes in `/Workspace/Notes` across the tenant and mark any notes
    /// that share the same `okf_id` with a `duplicate_id` conflict.
    ///
    /// Returns a list of `(okf_id, file_ids)` for each duplicate set.
    pub async fn detect_duplicate_okf_ids(&self, tenant_id: Uuid) -> Vec<(Uuid, Vec<Uuid>)> {
        let mut by_id: std::collections::HashMap<Uuid, Vec<Uuid>> =
            std::collections::HashMap::new();

        let root_folders = match self
            .metadata_store
            .list_folders_by_parent(None, tenant_id)
            .await
        {
            Ok(folders) => folders,
            Err(_) => return Vec::new(),
        };

        for workspace in root_folders
            .into_iter()
            .filter(|f| f.name == self.workspace_name)
        {
            let notes_folders = match self
                .metadata_store
                .list_folders_by_parent(Some(workspace.id), tenant_id)
                .await
            {
                Ok(folders) => folders,
                Err(_) => continue,
            };
            for notes in notes_folders
                .into_iter()
                .filter(|f| f.name == self.folder_name)
            {
                let bundles = match self
                    .metadata_store
                    .list_folders_by_parent(Some(notes.id), tenant_id)
                    .await
                {
                    Ok(folders) => folders,
                    Err(_) => continue,
                };
                for bundle in bundles {
                    let files = match self
                        .metadata_store
                        .list_files_by_parent(Some(bundle.id), tenant_id)
                        .await
                    {
                        Ok(files) => files,
                        Err(_) => continue,
                    };
                    if let Some(note_file) = files.into_iter().find(|f| f.name == "note.md") {
                        if let Ok(Some(note_meta)) = self
                            .load_metadata(note_file.id, note_file.owner_id, tenant_id)
                            .await
                        {
                            if let Some(okf_id) = note_meta.okf_id {
                                by_id.entry(okf_id).or_default().push(note_file.id);
                            }
                        }
                    }
                }
            }
        }

        let duplicates: Vec<(Uuid, Vec<Uuid>)> = by_id
            .iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|(id, ids)| (*id, ids.clone()))
            .collect();

        for (okf_id, file_ids) in &duplicates {
            for file_id in file_ids {
                let _ = self
                    .mark_duplicate_conflict(*file_id, tenant_id, *okf_id)
                    .await;
            }
        }

        duplicates
    }

    async fn mark_duplicate_conflict(
        &self,
        file_id: Uuid,
        tenant_id: Uuid,
        okf_id: Uuid,
    ) -> Result<(), NoteError> {
        let file = self
            .metadata_store
            .find_file_by_id_unchecked(file_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?
            .ok_or(NoteError::NotFound(file_id))?;
        if file.tenant_id != tenant_id {
            return Ok(());
        }
        let mut meta = self
            .load_metadata(file_id, file.owner_id, tenant_id)
            .await?
            .unwrap_or_else(|| NoteMetadata::new(file.name.trim_end_matches(".md")));
        meta.conflict = Some(NoteConflict {
            kind: "duplicate_id".to_string(),
            message: format!("OKF ID {} is shared by multiple notes.", okf_id),
            yaml_title: None,
            folder_name: None,
            manifest_title: None,
            yaml_id: None,
            sidecar_id: Some(okf_id),
        });
        meta.updated_at = Utc::now();
        self.save_metadata(file_id, file.owner_id, tenant_id, &meta)
            .await?;
        Ok(())
    }

    /// Save note content (autosave).
    pub async fn save_note(
        &self,
        file_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        content: String,
        color: Option<String>,
        attachments: Option<Vec<NoteAttachment>>,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(NoteError::PermissionDenied);
        }
        let is_folder_backed = Self::is_folder_backed_note(&file);

        // Load existing note.md content and parse its frontmatter.
        let existing_content = self
            .object_store
            .get(&file.storage_key())
            .await
            .map_err(|e| NoteError::Storage(e.to_string()))?;
        let existing_content_str = String::from_utf8_lossy(&existing_content);
        let (existing_fm, _) = parse_frontmatter(&existing_content_str)
            .map_err(|e| NoteError::Storage(format!("Failed to parse frontmatter: {}", e)))?;

        // Load sidecar metadata.
        let mut meta = self
            .load_metadata(file_id, user_id, file.tenant_id)
            .await?
            .unwrap_or_else(|| {
                let mut fallback = NoteMetadata::new(file.name.trim_end_matches(".md"));
                fallback.created_at = file.created_at;
                fallback
            });

        if let Some(new_color) = color {
            meta.color = Some(new_color);
        }

        if let Some(new_attachments) = attachments {
            meta.attachments = new_attachments;
        }

        // Anchor frontmatter identity to the sidecar.
        let mut base_fm = existing_fm;
        if let Some(okf_id) = meta.okf_id {
            let mut rs = base_fm.rustshare.take().unwrap_or_default();
            rs.id = Some(okf_id);
            base_fm.rustshare = Some(rs);
        }
        if is_folder_backed {
            let mut rs = base_fm.rustshare.take().unwrap_or_default();
            rs.bundle_name = Some(meta.title.clone());
            base_fm.rustshare = Some(rs);
        }

        // Determine new frontmatter and body from incoming content.
        let (mut final_fm, new_body) = if content.starts_with("---\n") {
            let (incoming_fm, incoming_body) = parse_frontmatter(&content)
                .map_err(|e| NoteError::Storage(format!("Invalid incoming frontmatter: {}", e)))?;
            (
                merge_incoming_frontmatter(base_fm, incoming_fm),
                incoming_body,
            )
        } else {
            (base_fm, content.clone())
        };

        // Preserve rustshare.id and bundle_name from the sidecar regardless of incoming content.
        if let Some(okf_id) = meta.okf_id {
            let mut rs = final_fm.rustshare.take().unwrap_or_default();
            rs.id = Some(okf_id);
            if is_folder_backed {
                rs.bundle_name = Some(meta.title.clone());
            }
            final_fm.rustshare = Some(rs);
        }

        // Update operational metadata.
        final_fm.timestamp = Some(Utc::now());

        // Ensure required OKF keys are present, especially for legacy single-file notes.
        let required = OkfNoteFrontmatter {
            okf_type: Some("Note".to_string()),
            rustshare: Some(RustshareFrontmatter {
                module: Some("notes".to_string()),
                source_kind: Some("note".to_string()),
                main: Some("note.md".to_string()),
                visibility: Some("private".to_string()),
                embedding_policy: Some("allowed".to_string()),
                verification_status: Some("draft".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        final_fm = merge_required_okf_keys(Some(final_fm), required);

        // Serialize the document and write it back to note.md.
        let document = to_document(&final_fm, &new_body)
            .map_err(|e| NoteError::Storage(format!("Failed to serialize frontmatter: {}", e)))?;

        let updated_file = self
            .file_service
            .edit_file(
                file_id,
                user_id,
                Bytes::from(document.clone()),
                "overwrite",
                None,
            )
            .await?;

        meta.updated_at = Utc::now();
        meta.excerpt = generate_excerpt(&new_body);

        // ADR-0029: changing the first H1 must NOT rename the bundle folder.
        let _ = is_folder_backed;

        self.save_metadata(file_id, user_id, file.tenant_id, &meta)
            .await?;

        self.emit_index_note(&updated_file, &meta, &document, tenant_id)
            .await;

        Ok(Note {
            id: updated_file.id,
            okf_id: meta.okf_id,
            name: updated_file.name.clone(),
            path: updated_file.path,
            content: document,
            metadata: meta,
            parent_folder_id: updated_file.parent_folder_id,
            owner_id: updated_file.owner_id,
            current_version: updated_file.current_version,
            created_at: updated_file.created_at,
            modified_at: updated_file.modified_at,
            conflict: None,
        })
    }

    /// Rename a note (updates title and filename/folder name).
    pub async fn rename_note(
        &self,
        file_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        new_title: String,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(NoteError::PermissionDenied);
        }
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
                    .unique_folder_name(
                        user_id,
                        file.tenant_id,
                        parent_folder.parent_folder_id,
                        &new_title,
                    )
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
        meta.title = new_title.clone();
        meta.updated_at = Utc::now();
        self.save_metadata(file_id, user_id, file.tenant_id, &meta)
            .await?;

        // Update note.md frontmatter title and bundle_name, preserving rustshare.id.
        let storage_key = file.storage_key();
        let existing_content = match self.object_store.get(&storage_key).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(_) => String::new(),
        };
        if let Ok((mut fm, body)) = parse_frontmatter(&existing_content) {
            fm.title = Some(new_title.clone());
            let mut rs = fm.rustshare.take().unwrap_or_default();
            rs.bundle_name = Some(new_title.clone());
            // Preserve rustshare.id from the sidecar.
            if let Some(okf_id) = meta.okf_id {
                rs.id = Some(okf_id);
            }
            fm.rustshare = Some(rs);
            fm.timestamp = Some(Utc::now());
            if let Ok(document) = to_document(&fm, &body) {
                let _ = self
                    .file_service
                    .edit_file(file_id, user_id, Bytes::from(document), "overwrite", None)
                    .await;
            }
        }

        // Reload file after potential rename
        let file = self.file_service.get_file(file_id, user_id).await?;

        // Load content for response
        let storage_key = file.storage_key();
        let content = match self.object_store.get(&storage_key).await {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(_) => String::new(),
        };

        self.emit_index_note(&file, &meta, &content, tenant_id)
            .await;

        Ok(Note {
            id: file.id,
            okf_id: meta.okf_id,
            name: file.name.clone(),
            path: file.path,
            content,
            metadata: meta,
            parent_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            current_version: file.current_version,
            created_at: file.created_at,
            modified_at: file.modified_at,
            conflict: None,
        })
    }

    /// Delete a note (and its sidecar).
    pub async fn delete_note(
        &self,
        file_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(NoteError::PermissionDenied);
        }
        let is_folder_backed = Self::is_folder_backed_note(&file);

        // If public, invalidate share index
        if let Some(meta) = self.load_metadata(file_id, user_id, file.tenant_id).await? {
            if let Some(share_id) = meta.public_share_id {
                let _ = self.delete_public_share_index(&share_id).await;
            }

            // Delete attachment files
            for attachment in &meta.attachments {
                let _ = self
                    .file_service
                    .delete_file(attachment.file_id, user_id)
                    .await;
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
                let _ = self
                    .folder_service
                    .delete_folder(parent_folder_id, user_id)
                    .await;
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
        tenant_id: Uuid,
        target_folder_id: Option<Uuid>,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(NoteError::PermissionDenied);
        }
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
            okf_id: meta.okf_id,
            name: moved_file.name.clone(),
            path: moved_file.path,
            content,
            metadata: meta,
            parent_folder_id: moved_file.parent_folder_id,
            owner_id: moved_file.owner_id,
            current_version: moved_file.current_version,
            created_at: moved_file.created_at,
            modified_at: moved_file.modified_at,
            conflict: None,
        })
    }

    /// Duplicate a note (creates a copy of the bundle with new IDs).
    pub async fn duplicate_note(
        &self,
        file_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Note, NoteError> {
        let original = self.get_note(file_id, user_id, tenant_id).await?;
        let original_file = self.file_service.get_file(file_id, user_id).await?;
        let tenant_id = original_file.tenant_id;
        let is_folder_backed = Self::is_folder_backed_note(&original_file);

        // Determine copy title and parent folder
        let copy_title = format!("{} (copy)", original.metadata.title);
        let parent_folder_id = original_file.parent_folder_id;

        if is_folder_backed {
            // Create new bundle folder with unique name
            let new_folder_name = self
                .unique_folder_name(user_id, tenant_id, parent_folder_id, &copy_title)
                .await?;
            let note_folder = self
                .folder_service
                .create_folder(new_folder_name, parent_folder_id, user_id, tenant_id)
                .await
                .map_err(|e| NoteError::Storage(e.to_string()))?;

            // Create subfolders
            for subfolder in &["attachments", "drawings", "exports", "_rustshare"] {
                self.get_or_create_subfolder(note_folder.id, subfolder, user_id, tenant_id)
                    .await?;
            }

            // Copy note.md content, generating a fresh OKF identity.
            let new_okf_id = Uuid::new_v4();
            let new_acl_hash = compute_acl_hash(tenant_id, tenant_id, new_okf_id, "private");
            let duplicated_content =
                duplicate_note_content(&original.content, new_okf_id, tenant_id, &copy_title)
                    .unwrap_or_else(|| original.content.clone());
            let new_file = self
                .file_service
                .upload_file(
                    user_id,
                    "note.md".to_string(),
                    Some(note_folder.id),
                    Bytes::from(duplicated_content.clone()),
                    "text/markdown".to_string(),
                    tenant_id,
                )
                .await?;

            // Copy attachments
            let attachments_folder = self
                .get_or_create_subfolder(note_folder.id, "attachments", user_id, tenant_id)
                .await?;
            let mut new_attachments = Vec::new();
            for att in &original.metadata.attachments {
                let att_file = self.file_service.get_file(att.file_id, user_id).await;
                if let Ok(att_file) = att_file {
                    let att_bytes = self
                        .object_store
                        .get(&att_file.storage_key())
                        .await
                        .map_err(|e| {
                            NoteError::Storage(format!("Failed to read attachment: {}", e))
                        })?;
                    let new_att_file = self
                        .file_service
                        .upload_file(
                            user_id,
                            att.name.clone(),
                            Some(attachments_folder.id),
                            att_bytes,
                            att.mime_type.clone(),
                            tenant_id,
                        )
                        .await?;
                    new_attachments.push(NoteAttachment {
                        file_id: new_att_file.id,
                        name: att.name.clone(),
                        mime_type: att.mime_type.clone(),
                        size: new_att_file.size,
                        created_at: new_att_file.created_at,
                    });
                }
            }

            // Build new metadata
            let mut meta = original.metadata.clone();
            meta.title = copy_title.clone();
            meta.excerpt = generate_excerpt(&duplicated_content);
            meta.visibility = NoteVisibility::Private;
            meta.public_share_id = None;
            meta.attachments = new_attachments;
            meta.created_at = Utc::now();
            meta.updated_at = Utc::now();
            meta.okf_id = Some(new_okf_id);
            meta.acl_hash = Some(new_acl_hash);
            meta.acl_version = Some(1);
            self.save_metadata(new_file.id, user_id, tenant_id, &meta)
                .await?;

            // Create manifest.json
            let manifest_folder = self
                .get_or_create_subfolder(note_folder.id, "_rustshare", user_id, tenant_id)
                .await?;
            let manifest = serde_json::json!({
                "type": "rustshare.note",
                "version": 1,
                "id": new_file.id.to_string(),
                "rustshare_id": new_okf_id.to_string(),
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
            self.file_service
                .upload_file(
                    user_id,
                    "manifest.json".to_string(),
                    Some(manifest_folder.id),
                    Bytes::from(manifest.to_string()),
                    "application/json".to_string(),
                    tenant_id,
                )
                .await
                .ok();

            return Ok(Note {
                id: new_file.id,
                okf_id: Some(new_okf_id),
                name: new_file.name.clone(),
                path: new_file.path,
                content: duplicated_content,
                metadata: meta,
                parent_folder_id: new_file.parent_folder_id,
                owner_id: new_file.owner_id,
                current_version: new_file.current_version,
                created_at: new_file.created_at,
                modified_at: new_file.modified_at,
                conflict: None,
            });
        }

        // Legacy single-file note duplication
        let new_name = self
            .unique_note_name(user_id, tenant_id, parent_folder_id, &copy_title)
            .await?;
        let new_file = self
            .file_service
            .upload_file(
                user_id,
                new_name,
                parent_folder_id,
                Bytes::from(original.content.clone()),
                "text/markdown".to_string(),
                tenant_id,
            )
            .await?;

        let mut meta = original.metadata.clone();
        meta.title = copy_title;
        meta.visibility = NoteVisibility::Private;
        meta.public_share_id = None;
        meta.attachments = Vec::new(); // Legacy: attachments not copied
        meta.created_at = Utc::now();
        meta.updated_at = Utc::now();
        self.save_metadata(new_file.id, user_id, tenant_id, &meta)
            .await?;

        Ok(Note {
            id: new_file.id,
            okf_id: None,
            name: new_file.name,
            path: new_file.path,
            content: original.content,
            metadata: meta,
            parent_folder_id: new_file.parent_folder_id,
            owner_id: new_file.owner_id,
            current_version: new_file.current_version,
            created_at: new_file.created_at,
            modified_at: new_file.modified_at,
            conflict: None,
        })
    }

    /// List all notes for a user.
    pub async fn list_notes(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<NoteSummary>, NoteError> {
        // Load all markdown files but filter to Notes paths only
        let files = self
            .metadata_store
            .list_all_markdown_files(user_id, tenant_id)
            .await
            .map_err(|e| NoteError::Database(e.to_string()))?;

        let mut notes = Vec::new();
        let workspace_prefix = format!("/{}/{}/", self.workspace_name, self.folder_name);
        let folder_prefix = format!("/{}/", self.folder_name);

        for file in files {
            if !(file.path.starts_with(&workspace_prefix) || file.path.starts_with(&folder_prefix))
            {
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
                    match self
                        .metadata_store
                        .find_folder_by_id(parent_id, user_id)
                        .await
                    {
                        Ok(Some(folder)) => folder.name,
                        _ => file.name.clone(),
                    }
                } else {
                    file.name.clone()
                }
            } else {
                file.name.clone()
            };

            let (attachment_count, drawing_count, export_count) =
                if Self::is_folder_backed_note(&file) {
                    if let Some(parent_id) = file.parent_folder_id {
                        self.count_bundle_contents(parent_id, file.owner_id, tenant_id)
                            .await?
                    } else {
                        (0, 0, 0)
                    }
                } else {
                    (0, 0, 0)
                };

            notes.push(NoteSummary {
                id: file.id,
                okf_id: meta.okf_id,
                name: display_name,
                path: file.path,
                metadata: meta.clone(),
                parent_folder_id: file.parent_folder_id,
                owner_id: file.owner_id,
                current_version: file.current_version,
                size: file.size,
                created_at: file.created_at,
                modified_at: file.modified_at,
                attachment_count,
                drawing_count,
                export_count,
                conflict: meta.conflict.clone(),
            });
        }

        notes.sort_by_key(|b| std::cmp::Reverse(b.modified_at));

        let paginated: Vec<NoteSummary> = notes
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();

        Ok(paginated)
    }

    /// List notes for a user, optionally filtered to a specific folder path prefix.
    pub async fn list_notes_filtered(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        path_prefix: Option<&str>,
        limit: i64,
        offset: i64,
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
                    match self
                        .metadata_store
                        .find_folder_by_id(parent_id, user_id)
                        .await
                    {
                        Ok(Some(folder)) => folder.name,
                        _ => file.name.clone(),
                    }
                } else {
                    file.name.clone()
                }
            } else {
                file.name.clone()
            };

            let (attachment_count, drawing_count, export_count) =
                if Self::is_folder_backed_note(&file) {
                    if let Some(parent_id) = file.parent_folder_id {
                        self.count_bundle_contents(parent_id, file.owner_id, tenant_id)
                            .await?
                    } else {
                        (0, 0, 0)
                    }
                } else {
                    (0, 0, 0)
                };

            notes.push(NoteSummary {
                id: file.id,
                okf_id: meta.okf_id,
                name: display_name,
                path: file.path,
                metadata: meta.clone(),
                parent_folder_id: file.parent_folder_id,
                owner_id: file.owner_id,
                current_version: file.current_version,
                size: file.size,
                created_at: file.created_at,
                modified_at: file.modified_at,
                attachment_count,
                drawing_count,
                export_count,
                conflict: meta.conflict.clone(),
            });
        }

        // Sort by updated_at desc
        notes.sort_by_key(|b| std::cmp::Reverse(b.modified_at));

        let paginated: Vec<NoteSummary> = notes
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();

        Ok(paginated)
    }

    /// Toggle note visibility between private and public.
    pub async fn toggle_visibility(
        &self,
        file_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Note, NoteError> {
        let file = self.file_service.get_file(file_id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(NoteError::PermissionDenied);
        }

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
            okf_id: meta.okf_id,
            name: file.name.clone(),
            path: file.path,
            content,
            metadata: meta,
            parent_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            current_version: file.current_version,
            created_at: file.created_at,
            modified_at: file.modified_at,
            conflict: None,
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

fn compute_acl_hash(tenant_id: Uuid, workspace_id: Uuid, okf_id: Uuid, visibility: &str) -> String {
    let input = format!(
        "tenant:{}:workspace:{}:note:{}:{}",
        tenant_id, workspace_id, okf_id, visibility
    );
    hex::encode(Sha256::digest(input.as_bytes()))
}

/// Merge incoming frontmatter over an existing frontmatter, preserving identity
/// and bundle metadata that should not change on a normal save.
fn merge_incoming_frontmatter(
    existing: OkfNoteFrontmatter,
    incoming: OkfNoteFrontmatter,
) -> OkfNoteFrontmatter {
    let mut merged = existing.clone();

    // Overlay top-level fields.
    if incoming.okf_type.is_some() {
        merged.okf_type = incoming.okf_type;
    }
    if incoming.title.is_some() {
        merged.title = incoming.title;
    }
    if incoming.description.is_some() {
        merged.description = incoming.description;
    }
    if incoming.resource.is_some() {
        merged.resource = incoming.resource;
    }
    if !incoming.tags.is_empty() {
        merged.tags = incoming.tags;
    }

    // Merge top-level extra keys.
    if let (Some(merged_obj), Some(incoming_obj)) =
        (merged.extra.as_object_mut(), incoming.extra.as_object())
    {
        for (k, v) in incoming_obj {
            merged_obj.insert(k.clone(), v.clone());
        }
    }

    // Merge rustshare block.
    if let Some(incoming_rs) = incoming.rustshare {
        let mut merged_rs = merged.rustshare.take().unwrap_or_default();
        if incoming_rs.module.is_some() {
            merged_rs.module = incoming_rs.module;
        }
        if incoming_rs.source_kind.is_some() {
            merged_rs.source_kind = incoming_rs.source_kind;
        }
        if incoming_rs.source_id.is_some() {
            merged_rs.source_id = incoming_rs.source_id;
        }
        if incoming_rs.main.is_some() {
            merged_rs.main = incoming_rs.main;
        }
        if incoming_rs.visibility.is_some() {
            merged_rs.visibility = incoming_rs.visibility;
        }
        if incoming_rs.acl_hash.is_some() {
            merged_rs.acl_hash = incoming_rs.acl_hash;
        }
        if incoming_rs.embedding_policy.is_some() {
            merged_rs.embedding_policy = incoming_rs.embedding_policy;
        }
        if incoming_rs.verification_status.is_some() {
            merged_rs.verification_status = incoming_rs.verification_status;
        }
        // Merge nested extra keys.
        if let (Some(merged_obj), Some(incoming_obj)) = (
            merged_rs.extra.as_object_mut(),
            incoming_rs.extra.as_object(),
        ) {
            for (k, v) in incoming_obj {
                merged_obj.insert(k.clone(), v.clone());
            }
        }
        merged.rustshare = Some(merged_rs);
    }

    merged
}

/// Rewrite a note document's frontmatter with a new rustshare.id and resource.
fn duplicate_note_content(
    doc: &str,
    new_okf_id: Uuid,
    workspace_id: Uuid,
    new_title: &str,
) -> Option<String> {
    let (mut fm, body) = parse_frontmatter(doc).ok()?;
    fm.title = Some(new_title.to_string());
    let mut rs = fm.rustshare.take().unwrap_or_default();
    rs.id = Some(new_okf_id);
    rs.bundle_name = Some(new_title.to_string());
    fm.resource = Some(format!(
        "rustshare://workspace/{}/notes/{}",
        workspace_id, new_okf_id
    ));
    fm.rustshare = Some(rs);
    let body = rewrite_first_h1(&body, new_title).unwrap_or(body);
    to_document(&fm, &body).ok()
}

/// Rewrite the first Markdown H1 heading in `content` to `new_title`.
/// Returns `None` if no H1 heading is found.
fn rewrite_first_h1(content: &str, new_title: &str) -> Option<String> {
    let mut replaced = false;
    let lines: Vec<String> = content
        .lines()
        .map(|line| {
            if !replaced && line.trim_start().starts_with("# ") {
                replaced = true;
                format!("# {}", new_title)
            } else {
                line.to_string()
            }
        })
        .collect();
    replaced.then(|| lines.join("\n"))
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

/// Extract the first H1 heading from markdown content.
/// Returns None if no H1 is found.
#[allow(dead_code)]
fn extract_h1_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            let title = title.trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}

fn generate_share_id() -> String {
    use rand::RngExt;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..32)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_conflict_resolution_serializes_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&NoteConflictResolution::PreferYaml).unwrap(),
            r#""prefer_yaml""#
        );
        assert_eq!(
            serde_json::to_string(&NoteConflictResolution::PreferFolder).unwrap(),
            r#""prefer_folder""#
        );
        assert_eq!(
            serde_json::to_string(&NoteConflictResolution::Custom("My Title".to_string())).unwrap(),
            r#"{"custom":"My Title"}"#
        );
    }

    #[test]
    fn note_conflict_resolution_round_trips() {
        for resolution in [
            NoteConflictResolution::PreferYaml,
            NoteConflictResolution::PreferFolder,
            NoteConflictResolution::Custom("Custom Title".to_string()),
        ] {
            let json = serde_json::to_string(&resolution).unwrap();
            let decoded: NoteConflictResolution = serde_json::from_str(&json).unwrap();
            assert_eq!(resolution, decoded);
        }
    }

    #[test]
    fn custom_title_rejects_empty_or_whitespace() {
        for title in ["", "   ", "\t\n"] {
            let result = NoteConflictResolution::validate_custom_title(title);
            assert!(
                matches!(result, Err(NoteError::InvalidName(_))),
                "expected InvalidName for {:?}, got {:?}",
                title,
                result
            );
        }
    }

    #[test]
    fn custom_title_trims_whitespace() {
        let title = NoteConflictResolution::validate_custom_title("  My Title  ").unwrap();
        assert_eq!(title, "My Title");
    }

    #[test]
    fn note_conflict_serializes_optional_fields() {
        let conflict = NoteConflict {
            kind: "title_mismatch".to_string(),
            message: "Titles differ".to_string(),
            yaml_title: Some("YAML Title".to_string()),
            folder_name: Some("Folder Name".to_string()),
            manifest_title: Some("Manifest Title".to_string()),
            yaml_id: Some(Uuid::nil()),
            sidecar_id: Some(Uuid::nil()),
        };
        let json = serde_json::to_value(&conflict).unwrap();
        assert_eq!(json["kind"], "title_mismatch");
        assert_eq!(json["yaml_title"], "YAML Title");
        assert!(!json.get("yaml_id").unwrap().is_null());
    }

    #[test]
    fn build_acl_payload_maps_note_metadata() {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let okf_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();

        let file = rustshare_core::domain::File::new(
            "note.md".to_string(),
            "/Workspace/Notes/Test/note.md".to_string(),
            "hash".to_string(),
            100,
            "text/markdown".to_string(),
            Some(parent_id),
            owner_id,
            tenant_id,
        );

        let mut meta = NoteMetadata::new("Test Note");
        meta.okf_id = Some(okf_id);
        meta.acl_hash = Some("test-hash".to_string());
        meta.acl_version = Some(3);
        meta.visibility = NoteVisibility::Public;

        let acl = NoteService::build_acl_payload(
            &file,
            &meta,
            tenant_id,
            vec![format!("owner:{owner_id}")],
        );

        assert_eq!(acl.tenant_id, tenant_id);
        assert_eq!(acl.workspace_id, tenant_id);
        assert_eq!(acl.note_id, okf_id);
        assert_eq!(acl.source_file_id, file.id);
        assert_eq!(acl.source_folder_id, Some(parent_id));
        assert_eq!(acl.owner_id, owner_id);
        assert_eq!(acl.acl_hash, "test-hash");
        assert_eq!(acl.acl_version, 3);
        assert_eq!(acl.visibility, "public");
        assert_eq!(acl.embedding_policy, "allowed");
        assert_eq!(acl.read_acl, vec![format!("owner:{}", owner_id)]);
    }

    #[test]
    fn build_acl_payload_includes_shared_user_principal() {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let okf_id = Uuid::new_v4();
        let parent_id = Uuid::new_v4();

        let file = rustshare_core::domain::File::new(
            "note.md".to_string(),
            "/Workspace/Notes/Test/note.md".to_string(),
            "hash".to_string(),
            100,
            "text/markdown".to_string(),
            Some(parent_id),
            owner_id,
            tenant_id,
        );

        let mut meta = NoteMetadata::new("Test Note");
        meta.okf_id = Some(okf_id);
        meta.acl_hash = Some("test-hash".to_string());
        meta.acl_version = Some(3);
        meta.visibility = NoteVisibility::Public;

        let acl = NoteService::build_acl_payload(
            &file,
            &meta,
            tenant_id,
            vec![format!("owner:{owner_id}"), format!("user:{user_id}")],
        );

        assert!(acl.read_acl.contains(&format!("owner:{owner_id}")));
        assert!(acl.read_acl.contains(&format!("user:{user_id}")));
    }
}
