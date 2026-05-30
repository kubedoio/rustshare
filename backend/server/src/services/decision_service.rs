//! Decision service for RustShare decisions.
//!
//! Decisions are file-backed Markdown records with sidecar metadata.

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

fn utc_now() -> DateTime<Utc> {
    Utc::now()
}

/// Decision-specific metadata sidecar schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionMetadata {
    #[serde(alias = "type")]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String, // "Proposed", "Accepted", "Rejected", "Deprecated"
    #[serde(default)]
    pub category: String,
    #[serde(default = "utc_now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "utc_now")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub decision_date: Option<DateTime<Utc>>,
}

impl DecisionMetadata {
    pub fn new(title: impl Into<String>, category: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            kind: "decision".to_string(),
            title: title.into(),
            status: "".to_string(),
            category: category.into(),
            created_at: now,
            updated_at: now,
            decision_date: None,
        }
    }
}

/// Unified decision payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub content: String,
    pub metadata: DecisionMetadata,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: UserId,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

/// Decision summary for listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionSummary {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub metadata: DecisionMetadata,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: UserId,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum DecisionError {
    #[error("Decision not found: {0}")]
    NotFound(Uuid),
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

impl From<rustshare_core::services::FileError> for DecisionError {
    fn from(e: rustshare_core::services::FileError) -> Self {
        match e {
            rustshare_core::services::FileError::NotFound(id) => DecisionError::NotFound(id),
            rustshare_core::services::FileError::PermissionDenied { .. } => {
                DecisionError::PermissionDenied
            }
            rustshare_core::services::FileError::InvalidName(s) => DecisionError::InvalidData(s),
            _ => DecisionError::Storage(e.to_string()),
        }
    }
}

use rustshare_infrastructure::repositories::PermissionResolverRepository;

pub struct DecisionService {
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

impl DecisionService {
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

    /// Ensure the root "Decisions" folder exists.
    /// Ensure the canonical /Workspace/Decisions folder exists.
    ///
    /// Legacy module root policy: new writes are always directed to the
    /// canonical /Workspace/Decisions path. Legacy roots are read-only.
    async fn ensure_decisions_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, DecisionError> {
        let ws = self.ensure_workspace_folder(owner_id, tenant_id).await?;
        let ws_folders = self
            .metadata_store
            .list_folders(Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Database(e.to_string()))?;
        if let Some(folder) = ws_folders.into_iter().find(|f| f.name == "Decisions") {
            return Ok(folder);
        }
        let folder = self
            .folder_service
            .create_folder_or_get("Decisions".to_string(), Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Storage(e.to_string()))?;
        Ok(folder)
    }

    async fn ensure_workspace_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, DecisionError> {
        let folders = self
            .metadata_store
            .list_folders(None, owner_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Database(e.to_string()))?;
        if let Some(ws) = folders.into_iter().find(|f| f.name == "Workspace") {
            return Ok(ws);
        }
        self.folder_service
            .create_folder_or_get("Workspace".into(), None, owner_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Storage(e.to_string()))
    }

    /// Ensure category folder exists under Decisions.
    async fn ensure_category_folder(
        &self,
        parent_id: Uuid,
        category: &str,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, DecisionError> {
        let folders = self
            .metadata_store
            .list_folders(Some(parent_id), owner_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Database(e.to_string()))?;
        if let Some(folder) = folders.into_iter().find(|f| f.name == category) {
            return Ok(folder);
        }
        let folder = self
            .folder_service
            .create_folder(category.to_string(), Some(parent_id), owner_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Storage(e.to_string()))?;
        Ok(folder)
    }

    /// List all decisions for a user.
    pub async fn list_decisions(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Vec<DecisionSummary>, DecisionError> {
        // Find all markdown files in the /Decisions subtree
        let files = self
            .metadata_store
            .list_all_markdown_files(user_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Database(e.to_string()))?;

        let mut summaries = Vec::new();
        for file in files {
            if !(file.path.starts_with("/Decisions/")
                || file.path.starts_with("/Workspace/Decisions/"))
            {
                continue;
            }

            let meta = self.load_metadata(&file, user_id, tenant_id).await?;
            summaries.push(DecisionSummary {
                id: file.id,
                name: file.name,
                path: file.path,
                metadata: meta,
                parent_folder_id: file.parent_folder_id,
                owner_id: file.owner_id,
                modified_at: file.modified_at,
            });
        }

        summaries.sort_by_key(|s| std::cmp::Reverse(s.modified_at));
        Ok(summaries)
    }

    /// Create a new decision.
    pub async fn create_decision(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        title: String,
        category: String,
        content: String,
    ) -> Result<Decision, DecisionError> {
        let root = self.ensure_decisions_folder(owner_id, tenant_id).await?;
        let cat_folder = self
            .ensure_category_folder(root.id, &category, owner_id, tenant_id)
            .await?;

        // Generate slug and ID prefix
        let next_id = self
            .get_next_decision_id(cat_folder.id, owner_id, tenant_id)
            .await?;
        let slug = slug::slugify(&title);
        let file_name = format!("DEC-{:04}-{}.md", next_id, slug);
        let sidecar_name = format!("DEC-{:04}-{}.rustshare.json", next_id, slug);

        // Upload main file
        let file = self
            .file_service
            .upload_file(
                owner_id,
                file_name,
                Some(cat_folder.id),
                Bytes::from(content.clone()),
                "text/markdown".to_string(),
                tenant_id,
            )
            .await?;

        // Create metadata
        let mut meta = DecisionMetadata::new(title, category);
        meta.decision_date = Some(Utc::now());

        // Upload sidecar
        let sidecar_data = serde_json::to_vec_pretty(&meta)
            .map_err(|e| DecisionError::InvalidData(e.to_string()))?;
        self.file_service
            .upload_file(
                owner_id,
                sidecar_name,
                Some(cat_folder.id),
                Bytes::from(sidecar_data),
                "application/json".to_string(),
                tenant_id,
            )
            .await?;

        Ok(Decision {
            id: file.id,
            name: file.name,
            path: file.path,
            content,
            metadata: meta,
            parent_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            created_at: file.created_at,
            modified_at: file.modified_at,
        })
    }

    async fn load_metadata(
        &self,
        file: &rustshare_core::domain::File,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<DecisionMetadata, DecisionError> {
        let stem = file.name.trim_end_matches(".md");
        let sidecar_name = format!("{}.rustshare.json", stem);

        let siblings = self
            .metadata_store
            .list_files(file.parent_folder_id, user_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Database(e.to_string()))?;

        if let Some(sidecar_file) = siblings.into_iter().find(|f| f.name == sidecar_name) {
            let data = self
                .object_store
                .get(&sidecar_file.storage_key())
                .await
                .map_err(|e| DecisionError::Storage(e.to_string()))?;
            let meta: DecisionMetadata = serde_json::from_slice(&data)
                .map_err(|e| DecisionError::InvalidData(e.to_string()))?;
            Ok(meta)
        } else {
            // Fallback
            Ok(DecisionMetadata::new(stem, "Uncategorized"))
        }
    }

    async fn get_next_decision_id(
        &self,
        folder_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<u32, DecisionError> {
        let files = self
            .metadata_store
            .list_files(Some(folder_id), user_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Database(e.to_string()))?;
        let mut max_id = 0;
        for file in files {
            if file.name.starts_with("DEC-") {
                if let Some(id_str) = file.name.split('-').nth(1) {
                    if let Ok(id) = id_str.parse::<u32>() {
                        if id > max_id {
                            max_id = id;
                        }
                    }
                }
            }
        }
        Ok(max_id + 1)
    }

    pub async fn get_decision(&self, id: Uuid, user_id: UserId, tenant_id: Uuid) -> Result<Decision, DecisionError> {
        let file = self.file_service.get_file(id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(DecisionError::PermissionDenied);
        }
        let content_bytes = self
            .object_store
            .get(&file.storage_key())
            .await
            .map_err(|e| DecisionError::Storage(e.to_string()))?;
        let content = String::from_utf8_lossy(&content_bytes).to_string();
        let meta = self.load_metadata(&file, user_id, file.tenant_id).await?;

        Ok(Decision {
            id: file.id,
            name: file.name,
            path: file.path,
            content,
            metadata: meta,
            parent_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            created_at: file.created_at,
            modified_at: file.modified_at,
        })
    }

    pub async fn delete_decision(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), DecisionError> {
        let file = self.file_service.get_file(id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(DecisionError::PermissionDenied);
        }
        // Delete sidecar if it exists
        let stem = file.name.trim_end_matches(".md");
        let sidecar_name = format!("{}.rustshare.json", stem);
        let siblings = self
            .metadata_store
            .list_files(file.parent_folder_id, user_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Database(e.to_string()))?;
        if let Some(sidecar) = siblings.into_iter().find(|f| f.name == sidecar_name) {
            self.file_service
                .delete_file(sidecar.id, user_id)
                .await
                .map_err(DecisionError::from)?;
        }
        self.file_service
            .delete_file(id, user_id)
            .await
            .map_err(DecisionError::from)?;
        Ok(())
    }

    pub async fn update_decision(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        title: Option<String>,
        content: Option<String>,
        status: Option<String>,
    ) -> Result<Decision, DecisionError> {
        let file = self.file_service.get_file(id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(DecisionError::PermissionDenied);
        }
        let mut meta = self.load_metadata(&file, user_id, file.tenant_id).await?;

        if let Some(t) = title {
            meta.title = t;
        }
        if let Some(s) = status {
            meta.status = s;
        }
        meta.updated_at = Utc::now();

        // Update content if provided
        if let Some(c) = content {
            self.file_service
                .edit_file(id, user_id, Bytes::from(c), "overwrite", None)
                .await?;
        }

        // Update sidecar
        let stem = file.name.trim_end_matches(".md");
        let sidecar_name = format!("{}.rustshare.json", stem);
        let siblings = self
            .metadata_store
            .list_files(file.parent_folder_id, user_id, file.tenant_id)
            .await
            .map_err(|e| DecisionError::Database(e.to_string()))?;

        if let Some(sidecar_file) = siblings.into_iter().find(|f| f.name == sidecar_name) {
            let sidecar_data = serde_json::to_vec_pretty(&meta)
                .map_err(|e| DecisionError::InvalidData(e.to_string()))?;
            self.file_service
                .edit_file(
                    sidecar_file.id,
                    user_id,
                    Bytes::from(sidecar_data),
                    "overwrite",
                    None,
                )
                .await?;
        }

        self.get_decision(id, user_id, tenant_id).await
    }

    /// Rename a decision (updates title and filename, preserving DEC-ID prefix).
    pub async fn rename_decision(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        new_title: String,
    ) -> Result<Decision, DecisionError> {
        let file = self.file_service.get_file(id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(DecisionError::PermissionDenied);
        }

        // Validate title
        let trimmed = new_title.trim();
        if trimmed.is_empty() {
            return Err(DecisionError::InvalidData(
                "Title cannot be empty".to_string(),
            ));
        }

        // Extract DEC-ID prefix from current filename (e.g., "DEC-0001" from "DEC-0001-use-rust.md")
        let stem = file.name.trim_end_matches(".md");
        let dec_prefix = stem.split('-').take(2).collect::<Vec<_>>().join("-");

        // Generate new filename preserving DEC-ID prefix
        let slug = slug::slugify(trimmed);
        let new_name = format!("{}-{}.md", dec_prefix, slug);

        // Rename the main file
        let renamed_file = self.file_service.rename_file(id, new_name, user_id).await?;

        // Load and update metadata
        let mut meta = self
            .load_metadata(&renamed_file, user_id, renamed_file.tenant_id)
            .await?;
        meta.title = trimmed.to_string();
        meta.updated_at = Utc::now();

        // Save metadata to sidecar
        let new_stem = renamed_file.name.trim_end_matches(".md");
        let new_sidecar_name = format!("{}.rustshare.json", new_stem);
        let sidecar_data = serde_json::to_vec_pretty(&meta)
            .map_err(|e| DecisionError::InvalidData(e.to_string()))?;

        // Find old sidecar, update its content and name
        let siblings = self
            .metadata_store
            .list_files(
                renamed_file.parent_folder_id,
                user_id,
                renamed_file.tenant_id,
            )
            .await
            .map_err(|e| DecisionError::Database(e.to_string()))?;

        let old_sidecar_name = format!("{}.rustshare.json", stem);
        if let Some(sidecar_file) = siblings.into_iter().find(|f| f.name == old_sidecar_name) {
            // Update sidecar content
            self.file_service
                .edit_file(
                    sidecar_file.id,
                    user_id,
                    Bytes::from(sidecar_data),
                    "overwrite",
                    None,
                )
                .await?;
            // Rename sidecar file to match new stem
            if old_sidecar_name != new_sidecar_name {
                self.file_service
                    .rename_file(sidecar_file.id, new_sidecar_name, user_id)
                    .await?;
            }
        }

        // Return updated decision
        self.get_decision(id, user_id, tenant_id).await
    }
}
