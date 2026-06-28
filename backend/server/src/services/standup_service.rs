//! Standup service for RustShare standup records.
//!
//! Standup records use a folder-per-day pattern:
//! /Workspace/Standups/{yyyy}/{yyyy-mm-dd}/
//!   index.md
//!   events.jsonl
//!   .rustshare.json

use anyhow::Result;
use bytes::Bytes;
use chrono::{DateTime, Datelike, Utc};
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

/// Standup-specific metadata sidecar schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StandupMetadata {
    #[serde(alias = "type")]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default = "utc_now")]
    pub date: DateTime<Utc>,
    #[serde(default = "utc_now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "utc_now")]
    pub updated_at: DateTime<Utc>,
}

impl StandupMetadata {
    pub fn new(title: impl Into<String>, date: DateTime<Utc>) -> Self {
        let now = Utc::now();
        Self {
            kind: "standup".to_string(),
            title: title.into(),
            date,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Unified standup record payload.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StandupRecord {
    pub id: Uuid,     // Folder ID
    pub name: String, // Folder name
    pub path: String,
    pub content: String,           // From index.md
    pub metadata: StandupMetadata, // From .rustshare.json
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Standup summary for listings.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StandupSummary {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub metadata: StandupMetadata,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum StandupError {
    #[error("Standup not found: {0}")]
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

impl From<rustshare_core::services::FolderError> for StandupError {
    fn from(e: rustshare_core::services::FolderError) -> Self {
        match e {
            rustshare_core::services::FolderError::NotFound(id) => StandupError::NotFound(id),
            rustshare_core::services::FolderError::PermissionDenied { .. } => {
                StandupError::PermissionDenied
            }
            rustshare_core::services::FolderError::InvalidName(s) => StandupError::InvalidData(s),
            _ => StandupError::Storage(e.to_string()),
        }
    }
}

impl From<rustshare_core::services::FileError> for StandupError {
    fn from(e: rustshare_core::services::FileError) -> Self {
        StandupError::Storage(e.to_string())
    }
}

use rustshare_infrastructure::repositories::PermissionResolverRepository;

pub struct StandupService {
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

impl StandupService {
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

    /// Ensure the root "Standups" folder exists under /Workspace.
    /// Ensure the canonical /Workspace/Standups folder exists.
    ///
    /// Legacy module root policy: new writes are always directed to the
    /// canonical /Workspace/Standups path. Legacy roots are read-only.
    async fn ensure_standups_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, StandupError> {
        let ws = self.ensure_workspace_folder(owner_id, tenant_id).await?;
        let ws_folders = self
            .metadata_store
            .list_folders(Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| StandupError::Database(e.to_string()))?;
        if let Some(folder) = ws_folders.into_iter().find(|f| f.name == "Standups") {
            return Ok(folder);
        }
        let folder = self
            .folder_service
            .create_folder_or_get("Standups".to_string(), Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| StandupError::Storage(e.to_string()))?;
        Ok(folder)
    }

    async fn ensure_workspace_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, StandupError> {
        let folders = self
            .metadata_store
            .list_folders(None, owner_id, tenant_id)
            .await
            .map_err(|e| StandupError::Database(e.to_string()))?;
        if let Some(ws) = folders.into_iter().find(|f| f.name == "Workspace") {
            return Ok(ws);
        }
        self.folder_service
            .create_folder_or_get("Workspace".into(), None, owner_id, tenant_id)
            .await
            .map_err(|e| StandupError::Storage(e.to_string()))
    }

    /// Ensure path /{yyyy}/ exists under Standups.
    async fn ensure_standup_year_path(
        &self,
        root_id: Uuid,
        year: i32,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, StandupError> {
        let folders = self
            .metadata_store
            .list_folders(Some(root_id), owner_id, tenant_id)
            .await
            .map_err(|e| StandupError::Database(e.to_string()))?;
        let year_str = year.to_string();
        if let Some(f) = folders.into_iter().find(|f| f.name == year_str) {
            return Ok(f);
        }
        self.folder_service
            .create_folder(year_str, Some(root_id), owner_id, tenant_id)
            .await
            .map_err(|e| StandupError::Storage(e.to_string()))
    }

    /// List all standup records.
    pub async fn list_standups(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StandupSummary>, StandupError> {
        let folders = self
            .metadata_store
            .list_all_folders(user_id, tenant_id)
            .await
            .map_err(|e| StandupError::Database(e.to_string()))?;

        let mut summaries = Vec::new();
        for folder in folders {
            if !(folder.path.starts_with("/Standups/")
                || folder.path.starts_with("/Workspace/Standups/"))
            {
                continue;
            }

            // Check if it's a standup folder (has .rustshare.json with kind="standup")
            if let Ok(Some(meta)) = self.load_metadata(folder.id, user_id, tenant_id).await {
                summaries.push(StandupSummary {
                    id: folder.id,
                    name: folder.name,
                    path: folder.path,
                    metadata: meta,
                    modified_at: folder.updated_at,
                });
            }
        }

        summaries.sort_by_key(|s| std::cmp::Reverse(s.modified_at));

        let paginated: Vec<StandupSummary> = summaries
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();

        Ok(paginated)
    }

    /// Create a new standup record.
    /// If a standup for the given date already exists, returns the existing one.
    pub async fn create_standup(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        title: String,
        date: DateTime<Utc>,
        content: String,
    ) -> Result<StandupRecord, StandupError> {
        let root = self.ensure_standups_folder(owner_id, tenant_id).await?;
        let year_folder = self
            .ensure_standup_year_path(root.id, date.year(), owner_id, tenant_id)
            .await?;

        // Folder name: yyyy-mm-dd
        let folder_name = date.format("%Y-%m-%d").to_string();

        // Check if a standup for this date already exists
        let existing_folders = self
            .metadata_store
            .list_folders(Some(year_folder.id), owner_id, tenant_id)
            .await
            .map_err(|e| StandupError::Database(e.to_string()))?;
        if let Some(existing) = existing_folders.into_iter().find(|f| f.name == folder_name) {
            // Return the existing standup
            return self.get_standup(existing.id, owner_id, tenant_id).await;
        }

        let standup_folder = self
            .folder_service
            .create_folder(folder_name, Some(year_folder.id), owner_id, tenant_id)
            .await
            .map_err(|e| StandupError::Storage(e.to_string()))?;

        // Create index.md
        self.file_service
            .upload_file(
                owner_id,
                "index.md".to_string(),
                Some(standup_folder.id),
                Bytes::from(content.clone()),
                "text/markdown".to_string(),
                tenant_id,
            )
            .await?;

        // Create events.jsonl
        self.file_service
            .upload_file(
                owner_id,
                "events.jsonl".to_string(),
                Some(standup_folder.id),
                Bytes::from(""),
                "application/jsonlines".to_string(),
                tenant_id,
            )
            .await?;

        // Create .rustshare.json
        let meta = StandupMetadata::new(title, date);
        let meta_data = serde_json::to_vec_pretty(&meta)
            .map_err(|e| StandupError::InvalidData(e.to_string()))?;
        self.file_service
            .upload_file(
                owner_id,
                ".rustshare.json".to_string(),
                Some(standup_folder.id),
                Bytes::from(meta_data),
                "application/json".to_string(),
                tenant_id,
            )
            .await?;

        Ok(StandupRecord {
            id: standup_folder.id,
            name: standup_folder.name,
            path: standup_folder.path,
            content,
            metadata: meta,
            owner_id,
            created_at: standup_folder.created_at,
            updated_at: standup_folder.updated_at,
        })
    }

    async fn load_metadata(
        &self,
        folder_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Option<StandupMetadata>, StandupError> {
        let files = self
            .metadata_store
            .list_files(Some(folder_id), user_id, tenant_id)
            .await
            .map_err(|e| StandupError::Database(e.to_string()))?;

        if let Some(sidecar) = files.into_iter().find(|f| f.name == ".rustshare.json") {
            let data = self
                .object_store
                .get(&sidecar.storage_key())
                .await
                .map_err(|e| StandupError::Storage(e.to_string()))?;
            let meta: StandupMetadata = serde_json::from_slice(&data)
                .map_err(|e| StandupError::InvalidData(e.to_string()))?;
            if meta.kind == "standup" {
                return Ok(Some(meta));
            }
        }
        Ok(None)
    }

    pub async fn get_standup(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<StandupRecord, StandupError> {
        let folder = self.folder_service.get_folder(id, user_id).await?;
        if folder.tenant_id != tenant_id {
            return Err(StandupError::PermissionDenied);
        }
        let files = self
            .metadata_store
            .list_files_by_parent(Some(id), folder.tenant_id)
            .await
            .map_err(|e| StandupError::Database(e.to_string()))?;

        // Load index.md
        let content = if let Some(index_file) = files.iter().find(|f| f.name == "index.md") {
            let bytes = self
                .object_store
                .get(&index_file.storage_key())
                .await
                .map_err(|e| StandupError::Storage(e.to_string()))?;
            String::from_utf8_lossy(&bytes).to_string()
        } else {
            String::new()
        };

        // Load metadata
        let meta = self
            .load_metadata(id, user_id, folder.tenant_id)
            .await?
            .ok_or(StandupError::NotFound(id))?;

        Ok(StandupRecord {
            id: folder.id,
            name: folder.name,
            path: folder.path,
            content,
            metadata: meta,
            owner_id: folder.owner_id,
            created_at: folder.created_at,
            updated_at: folder.updated_at,
        })
    }

    pub async fn delete_standup(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), StandupError> {
        let folder = self.folder_service.get_folder(id, user_id).await?;
        if folder.tenant_id != tenant_id {
            return Err(StandupError::PermissionDenied);
        }
        self.folder_service
            .delete_folder(id, user_id)
            .await
            .map_err(StandupError::from)?;
        Ok(())
    }

    pub async fn update_standup(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        title: Option<String>,
        content: Option<String>,
    ) -> Result<StandupRecord, StandupError> {
        let folder = self.folder_service.get_folder(id, user_id).await?;
        if folder.tenant_id != tenant_id {
            return Err(StandupError::PermissionDenied);
        }
        let mut meta = self
            .load_metadata(id, user_id, folder.tenant_id)
            .await?
            .ok_or(StandupError::NotFound(id))?;

        if let Some(t) = title {
            meta.title = t;
        }
        meta.updated_at = Utc::now();

        // Update sidecar
        let files = self
            .metadata_store
            .list_files_by_parent(Some(id), folder.tenant_id)
            .await
            .map_err(|e| StandupError::Database(e.to_string()))?;
        if let Some(sidecar) = files.iter().find(|f| f.name == ".rustshare.json") {
            let meta_data = serde_json::to_vec_pretty(&meta)
                .map_err(|e| StandupError::InvalidData(e.to_string()))?;
            self.file_service
                .edit_file(
                    sidecar.id,
                    user_id,
                    Bytes::from(meta_data),
                    "overwrite",
                    None,
                )
                .await?;
        }

        // Update content
        if let Some(c) = content {
            if let Some(index_file) = files.iter().find(|f| f.name == "index.md") {
                self.file_service
                    .edit_file(index_file.id, user_id, Bytes::from(c), "overwrite", None)
                    .await?;
            }
        }

        self.get_standup(id, user_id, tenant_id).await
    }

    /// Move a standup folder to a different parent folder.
    pub async fn move_standup(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        target_folder_id: Option<Uuid>,
    ) -> Result<StandupRecord, StandupError> {
        let folder = self.folder_service.get_folder(id, user_id).await?;
        if folder.tenant_id != tenant_id {
            return Err(StandupError::PermissionDenied);
        }
        self.folder_service
            .move_folder(id, target_folder_id, user_id)
            .await?;
        self.get_standup(id, user_id, tenant_id).await
    }

    /// Duplicate a standup in the same parent folder with a regenerated date folder name.
    pub async fn duplicate_standup(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<StandupRecord, StandupError> {
        let original = self.get_standup(id, user_id, tenant_id).await?;
        let original_folder = self.folder_service.get_folder(id, user_id).await?;
        if original_folder.tenant_id != tenant_id {
            return Err(StandupError::PermissionDenied);
        }

        let parent_folder_id = original_folder.parent_folder_id;
        let copy_title = format!("{} (copy)", original.metadata.title);
        // Design choice: a duplicated standup represents a new day, so use the
        // current date for both the folder name and the standup metadata.
        let new_date = Utc::now();
        let new_folder_name = self
            .unique_standup_folder_name(parent_folder_id, user_id, tenant_id, &new_date)
            .await?;

        let new_folder = self
            .folder_service
            .create_folder(new_folder_name, parent_folder_id, user_id, tenant_id)
            .await?;

        // Copy index.md
        self.file_service
            .upload_file(
                user_id,
                "index.md".to_string(),
                Some(new_folder.id),
                Bytes::from(original.content),
                "text/markdown".to_string(),
                tenant_id,
            )
            .await?;

        // Copy events.jsonl if it exists
        let files = self
            .metadata_store
            .list_files_by_parent(Some(id), tenant_id)
            .await
            .map_err(|e| StandupError::Database(e.to_string()))?;
        let events_content =
            if let Some(events_file) = files.iter().find(|f| f.name == "events.jsonl") {
                self.object_store
                    .get(&events_file.storage_key())
                    .await
                    .map_err(|e| StandupError::Storage(e.to_string()))?
            } else {
                Bytes::new()
            };
        self.file_service
            .upload_file(
                user_id,
                "events.jsonl".to_string(),
                Some(new_folder.id),
                events_content,
                "application/jsonlines".to_string(),
                tenant_id,
            )
            .await?;

        // Copy .rustshare.json with updated title/timestamps
        let mut meta = original.metadata.clone();
        meta.title = copy_title;
        meta.date = new_date;
        meta.created_at = Utc::now();
        meta.updated_at = Utc::now();
        let meta_data = serde_json::to_vec_pretty(&meta)
            .map_err(|e| StandupError::InvalidData(e.to_string()))?;
        self.file_service
            .upload_file(
                user_id,
                ".rustshare.json".to_string(),
                Some(new_folder.id),
                Bytes::from(meta_data),
                "application/json".to_string(),
                tenant_id,
            )
            .await?;

        self.get_standup(new_folder.id, user_id, tenant_id).await
    }

    async fn unique_standup_folder_name(
        &self,
        parent_id: Option<Uuid>,
        user_id: UserId,
        tenant_id: Uuid,
        date: &DateTime<Utc>,
    ) -> Result<String, StandupError> {
        let base = date.format("%Y-%m-%d").to_string();
        let siblings = self
            .metadata_store
            .list_folders(parent_id, user_id, tenant_id)
            .await
            .map_err(|e| StandupError::Database(e.to_string()))?;
        if !siblings.iter().any(|f| f.name == base) {
            return Ok(base);
        }
        for n in 1.. {
            let candidate = if n == 1 {
                format!("{}-copy", base)
            } else {
                format!("{}-copy-{}", base, n)
            };
            if !siblings.iter().any(|f| f.name == candidate) {
                return Ok(candidate);
            }
            if n > 1000 {
                return Err(StandupError::Storage(
                    "Could not generate unique standup folder name".to_string(),
                ));
            }
        }
        Err(StandupError::Storage(
            "Could not generate unique standup folder name".to_string(),
        ))
    }
}
