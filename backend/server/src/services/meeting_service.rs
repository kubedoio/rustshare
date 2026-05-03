//! Meeting service for RustShare meeting notes.
//!
//! Meeting notes use a folder-per-meeting pattern:
//! /Meetings/{team}/{yyyy}/{yyyy-mm-dd}-{slug}/
//!   index.md
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

/// Meeting-specific metadata sidecar schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeetingMetadata {
    pub kind: String,
    pub title: String,
    pub date: DateTime<Utc>,
    pub team: String,
    pub attendees: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MeetingMetadata {
    pub fn new(title: impl Into<String>, team: impl Into<String>, date: DateTime<Utc>) -> Self {
        let now = Utc::now();
        Self {
            kind: "meeting".to_string(),
            title: title.into(),
            date,
            team: team.into(),
            attendees: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// Unified meeting note payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingNote {
    pub id: Uuid,     // Folder ID
    pub name: String, // Folder name
    pub path: String,
    pub content: String,           // From index.md
    pub metadata: MeetingMetadata, // From .rustshare.json
    pub owner_id: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Meeting summary for listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSummary {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub metadata: MeetingMetadata,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum MeetingError {
    #[error("Meeting not found: {0}")]
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

impl From<rustshare_core::services::FolderError> for MeetingError {
    fn from(e: rustshare_core::services::FolderError) -> Self {
        match e {
            rustshare_core::services::FolderError::NotFound(id) => MeetingError::NotFound(id),
            rustshare_core::services::FolderError::PermissionDenied { .. } => {
                MeetingError::PermissionDenied
            }
            rustshare_core::services::FolderError::InvalidName(s) => MeetingError::InvalidData(s),
            _ => MeetingError::Storage(e.to_string()),
        }
    }
}

impl From<rustshare_core::services::FileError> for MeetingError {
    fn from(e: rustshare_core::services::FileError) -> Self {
        MeetingError::Storage(e.to_string())
    }
}

use rustshare_infrastructure::repositories::PermissionResolverRepository;

pub struct MeetingService {
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

impl MeetingService {
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

    /// Ensure the root "Meetings" folder exists under /Workspace.
    async fn ensure_meetings_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, MeetingError> {
        let root_folders = self
            .metadata_store
            .list_folders(None, owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;
        if let Some(folder) = root_folders.into_iter().find(|f| f.name == "Meetings") {
            return Ok(folder);
        }
        let ws = self.ensure_workspace_folder(owner_id, tenant_id).await?;
        let ws_folders = self
            .metadata_store
            .list_folders(Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;
        if let Some(folder) = ws_folders.into_iter().find(|f| f.name == "Meetings") {
            return Ok(folder);
        }
        let folder = self
            .folder_service
            .create_folder("Meetings".to_string(), Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Storage(e.to_string()))?;
        Ok(folder)
    }

    async fn ensure_workspace_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, MeetingError> {
        let folders = self
            .metadata_store
            .list_folders(None, owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;
        if let Some(ws) = folders.into_iter().find(|f| f.name == "Workspace") {
            return Ok(ws);
        }
        self.folder_service
            .create_folder("Workspace".into(), None, owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Storage(e.to_string()))
    }

    /// Ensure path /{team}/{yyyy}/ exists under Meetings.
    async fn ensure_meeting_path(
        &self,
        root_id: Uuid,
        team: &str,
        year: i32,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, MeetingError> {
        // Team folder
        let folders = self
            .metadata_store
            .list_folders(Some(root_id), owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;
        let team_folder = if let Some(f) = folders.into_iter().find(|f| f.name == team) {
            f
        } else {
            self.folder_service
                .create_folder(team.to_string(), Some(root_id), owner_id, tenant_id)
                .await
                .map_err(|e| MeetingError::Storage(e.to_string()))?
        };

        // Year folder
        let folders = self
            .metadata_store
            .list_folders(Some(team_folder.id), owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;
        let year_str = year.to_string();
        let year_folder = if let Some(f) = folders.into_iter().find(|f| f.name == year_str) {
            f
        } else {
            self.folder_service
                .create_folder(year_str, Some(team_folder.id), owner_id, tenant_id)
                .await
                .map_err(|e| MeetingError::Storage(e.to_string()))?
        };

        Ok(year_folder)
    }

    /// List all meeting notes.
    pub async fn list_meetings(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Vec<MeetingSummary>, MeetingError> {
        // Meetings are folders that contain .rustshare.json with kind="meeting"
        // This is expensive to scan everything.
        // Better to list all folders in the /Meetings subtree.
        let folders = self
            .metadata_store
            .list_all_folders(user_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;

        let mut summaries = Vec::new();
        for folder in folders {
            if !folder.path.starts_with("/Meetings/") {
                continue;
            }

            // Check if it's a meeting folder (has .rustshare.json)
            if let Ok(Some(meta)) = self.load_metadata(folder.id, user_id, tenant_id).await {
                summaries.push(MeetingSummary {
                    id: folder.id,
                    name: folder.name,
                    path: folder.path,
                    metadata: meta,
                    modified_at: folder.updated_at,
                });
            }
        }

        summaries.sort_by_key(|s| std::cmp::Reverse(s.modified_at));
        Ok(summaries)
    }

    /// Create a new meeting note.
    pub async fn create_meeting(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        title: String,
        team: String,
        date: DateTime<Utc>,
        content: String,
    ) -> Result<MeetingNote, MeetingError> {
        let root = self.ensure_meetings_folder(owner_id, tenant_id).await?;
        let parent = self
            .ensure_meeting_path(root.id, &team, date.year(), owner_id, tenant_id)
            .await?;

        // Folder name: yyyy-mm-dd-slug
        let slug = slug::slugify(&title);
        let folder_name = format!(
            "{}-{}-{}",
            date.format("%Y-%m-%d"),
            slug,
            &Uuid::new_v4().to_string()[0..8]
        );

        let meeting_folder = self
            .folder_service
            .create_folder(folder_name, Some(parent.id), owner_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Storage(e.to_string()))?;

        // Create index.md
        self.file_service
            .upload_file(
                owner_id,
                "index.md".to_string(),
                Some(meeting_folder.id),
                Bytes::from(content.clone()),
                "text/markdown".to_string(),
                tenant_id,
            )
            .await?;

        // Create .rustshare.json
        let meta = MeetingMetadata::new(title, team, date);
        let meta_data = serde_json::to_vec_pretty(&meta)
            .map_err(|e| MeetingError::InvalidData(e.to_string()))?;
        self.file_service
            .upload_file(
                owner_id,
                ".rustshare.json".to_string(),
                Some(meeting_folder.id),
                Bytes::from(meta_data),
                "application/json".to_string(),
                tenant_id,
            )
            .await?;

        Ok(MeetingNote {
            id: meeting_folder.id,
            name: meeting_folder.name,
            path: meeting_folder.path,
            content,
            metadata: meta,
            owner_id,
            created_at: meeting_folder.created_at,
            updated_at: meeting_folder.updated_at,
        })
    }

    async fn load_metadata(
        &self,
        folder_id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Option<MeetingMetadata>, MeetingError> {
        let files = self
            .metadata_store
            .list_files(Some(folder_id), user_id, tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;

        if let Some(sidecar) = files.into_iter().find(|f| f.name == ".rustshare.json") {
            let data = self
                .object_store
                .get(&sidecar.storage_key())
                .await
                .map_err(|e| MeetingError::Storage(e.to_string()))?;
            let meta: MeetingMetadata = serde_json::from_slice(&data)
                .map_err(|e| MeetingError::InvalidData(e.to_string()))?;
            if meta.kind == "meeting" {
                return Ok(Some(meta));
            }
        }
        Ok(None)
    }

    pub async fn get_meeting(
        &self,
        id: Uuid,
        user_id: UserId,
    ) -> Result<MeetingNote, MeetingError> {
        let folder = self.folder_service.get_folder(id, user_id).await?;
        let files = self
            .metadata_store
            .list_files(Some(id), user_id, folder.tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;

        // Load index.md
        let content = if let Some(index_file) = files.iter().find(|f| f.name == "index.md") {
            let bytes = self
                .object_store
                .get(&index_file.storage_key())
                .await
                .map_err(|e| MeetingError::Storage(e.to_string()))?;
            String::from_utf8_lossy(&bytes).to_string()
        } else {
            String::new()
        };

        // Load metadata
        let meta = self
            .load_metadata(id, user_id, folder.tenant_id)
            .await?
            .ok_or(MeetingError::NotFound(id))?;

        Ok(MeetingNote {
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

    pub async fn update_meeting(
        &self,
        id: Uuid,
        user_id: UserId,
        title: Option<String>,
        content: Option<String>,
        attendees: Option<Vec<String>>,
    ) -> Result<MeetingNote, MeetingError> {
        let folder = self.folder_service.get_folder(id, user_id).await?;
        let mut meta = self
            .load_metadata(id, user_id, folder.tenant_id)
            .await?
            .ok_or(MeetingError::NotFound(id))?;

        if let Some(t) = title {
            meta.title = t;
        }
        if let Some(a) = attendees {
            meta.attendees = a;
        }
        meta.updated_at = Utc::now();

        // Update sidecar
        let files = self
            .metadata_store
            .list_files(Some(id), user_id, folder.tenant_id)
            .await
            .map_err(|e| MeetingError::Database(e.to_string()))?;
        if let Some(sidecar) = files.iter().find(|f| f.name == ".rustshare.json") {
            let meta_data = serde_json::to_vec_pretty(&meta)
                .map_err(|e| MeetingError::InvalidData(e.to_string()))?;
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

        self.get_meeting(id, user_id).await
    }
}
