//! Decision service for RustShare decisions.
//!
//! Decisions are file-backed Markdown records with sidecar metadata.

use anyhow::Result;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use rustshare_core::{
    domain::{File, Folder, SharePermissions, UserId},
    services::{FileService, FolderService},
};
use rustshare_storage::{MetadataStore, ObjectStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

fn utc_now() -> DateTime<Utc> {
    Utc::now()
}

fn normalize_decision_status(status: &str) -> Result<String, DecisionError> {
    let normalized = status.to_ascii_lowercase();
    match normalized.as_str() {
        "" => Ok(normalized),
        "proposed" | "accepted" | "rejected" | "deprecated" | "superseded" => Ok(normalized),
        _ => Err(DecisionError::InvalidData("Invalid decision status".into())),
    }
}

fn decision_prefix(stem: &str) -> String {
    let identifier = stem.strip_prefix("DEC-").unwrap_or(stem);
    if let Some(uuid) = identifier
        .get(..36)
        .filter(|value| Uuid::parse_str(value).is_ok())
    {
        format!("DEC-{uuid}")
    } else {
        stem.split('-').take(2).collect::<Vec<_>>().join("-")
    }
}

/// Decision-specific metadata sidecar schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DecisionMetadata {
    #[serde(alias = "type")]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String, // normalized lowercase Decision lifecycle value
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Decision {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub content: String,
    pub metadata: DecisionMetadata,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

/// Decision summary for listings.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DecisionSummary {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub metadata: DecisionMetadata,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
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
            .create_folder_or_get(category.to_string(), Some(parent_id), owner_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Storage(e.to_string()))?;
        Ok(folder)
    }

    /// List all decisions for a user.
    pub async fn list_decisions(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
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
                || !file.name.starts_with("DEC-")
                || !file.name.ends_with(".md")
            {
                continue;
            }

            let meta = self.load_metadata(&file, user_id, tenant_id).await?;
            if meta.kind != "decision" {
                continue;
            }
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

        let paginated: Vec<DecisionSummary> = summaries
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();

        Ok(paginated)
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

    /// Resume a durable meeting creation command without overwriting either file.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_for_meeting_command(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
        title: String,
        category: String,
        content: String,
        decision_id: Uuid,
        sidecar_id: Uuid,
        created_at: DateTime<Utc>,
    ) -> Result<Decision, DecisionError> {
        let root = self.ensure_decisions_folder(owner_id, tenant_id).await?;
        let category_folder = self
            .ensure_category_folder(root.id, &category, owner_id, tenant_id)
            .await?;
        let stem = format!("DEC-{decision_id}");
        let file = self
            .file_service
            .create_file_if_absent(
                decision_id,
                owner_id,
                format!("{stem}.md"),
                Some(category_folder.id),
                Bytes::from(content),
                "text/markdown".into(),
                tenant_id,
            )
            .await?;
        if file.id != decision_id {
            return Err(DecisionError::InvalidData(
                "Decision identity conflict".into(),
            ));
        }
        let mut metadata = DecisionMetadata::new(title, category);
        metadata.status = "proposed".into();
        metadata.created_at = created_at;
        metadata.updated_at = created_at;
        metadata.decision_date = Some(created_at);
        let sidecar = self
            .file_service
            .create_file_if_absent(
                sidecar_id,
                owner_id,
                format!(
                    "{}.rustshare.json",
                    file.name.strip_suffix(".md").unwrap_or(&file.name)
                ),
                file.parent_folder_id,
                Bytes::from(
                    serde_json::to_vec_pretty(&metadata)
                        .map_err(|error| DecisionError::InvalidData(error.to_string()))?,
                ),
                "application/json".into(),
                tenant_id,
            )
            .await?;
        if sidecar.id != sidecar_id
            || sidecar.owner_id != file.owner_id
            || sidecar.tenant_id != file.tenant_id
            || sidecar.parent_folder_id != file.parent_folder_id
            || sidecar.name
                != format!(
                    "{}.rustshare.json",
                    file.name.strip_suffix(".md").unwrap_or(&file.name)
                )
        {
            return Err(DecisionError::InvalidData(
                "Decision metadata identity conflict".into(),
            ));
        }
        self.get_decision(decision_id, owner_id, tenant_id).await
    }

    async fn require_decision(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        required: SharePermissions,
    ) -> Result<(File, DecisionMetadata), DecisionError> {
        let file = self.file_service.get_file(id, user_id).await?;
        if file.tenant_id != tenant_id {
            return Err(DecisionError::PermissionDenied);
        }
        if !file.name.starts_with("DEC-") || !file.name.ends_with(".md") {
            return Err(DecisionError::NotFound(id));
        }
        self.file_service
            .require_file_permission(user_id, tenant_id, id, required)
            .await?;
        let metadata = self.load_metadata(&file, user_id, tenant_id).await?;
        if metadata.kind != "decision" {
            return Err(DecisionError::NotFound(id));
        }
        Ok((file, metadata))
    }

    async fn save_metadata(
        &self,
        file: &File,
        user_id: UserId,
        metadata: &DecisionMetadata,
    ) -> Result<(), DecisionError> {
        let data = Bytes::from(
            serde_json::to_vec_pretty(metadata)
                .map_err(|error| DecisionError::InvalidData(error.to_string()))?,
        );
        self.file_service
            .edit_resource_sidecar(file.id, user_id, data)
            .await?;
        Ok(())
    }

    async fn ensure_metadata_sidecar(
        &self,
        file: &File,
        user_id: UserId,
        metadata: &DecisionMetadata,
    ) -> Result<(), DecisionError> {
        let sidecar_name = format!(
            "{}.rustshare.json",
            file.name.strip_suffix(".md").unwrap_or(&file.name)
        );
        let siblings = self
            .metadata_store
            .list_files(file.parent_folder_id, file.owner_id, file.tenant_id)
            .await
            .map_err(|error| DecisionError::Database(error.to_string()))?;
        if siblings.iter().any(|sidecar| sidecar.name == sidecar_name) {
            return Ok(());
        }
        if let Some(parent_id) = file.parent_folder_id {
            if self
                .metadata_store
                .find_folder_by_id(parent_id, file.owner_id)
                .await
                .map_err(|error| DecisionError::Database(error.to_string()))?
                .is_none()
            {
                return Err(DecisionError::PermissionDenied);
            }
        } else if user_id != file.owner_id {
            return Err(DecisionError::PermissionDenied);
        }
        // An interrupted meeting Decision already owns a durable sidecar ID. Reuse it
        // so repairing metadata cannot strand the original creation command.
        let sidecar_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT sidecar_file_id FROM meeting_decision_creations
             WHERE tenant_id=$1 AND decision_file_id=$2 AND ready=false",
        )
        .bind(file.tenant_id)
        .bind(file.id)
        .fetch_optional(self.metadata_store.pool())
        .await
        .map_err(|error| DecisionError::Database(error.to_string()))?;
        // Repair needs the caller's normal folder Edit authority before changing the Decision.
        let sidecar = self
            .file_service
            .create_file_if_absent(
                sidecar_id.unwrap_or_else(|| Uuid::new_v5(&file.id, b"decision-sidecar")),
                user_id,
                sidecar_name.clone(),
                file.parent_folder_id,
                Bytes::from(
                    serde_json::to_vec_pretty(metadata)
                        .map_err(|error| DecisionError::InvalidData(error.to_string()))?,
                ),
                "application/json".into(),
                file.tenant_id,
            )
            .await?;
        if sidecar.name != sidecar_name
            || sidecar.owner_id != file.owner_id
            || sidecar.tenant_id != file.tenant_id
            || sidecar.parent_folder_id != file.parent_folder_id
        {
            return Err(DecisionError::InvalidData(
                "Decision metadata identity conflict".into(),
            ));
        }
        Ok(())
    }

    async fn load_metadata(
        &self,
        file: &rustshare_core::domain::File,
        _user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<DecisionMetadata, DecisionError> {
        let stem = file.name.strip_suffix(".md").unwrap_or(&file.name);
        let sidecar_name = format!("{}.rustshare.json", stem);

        let siblings = self
            .metadata_store
            .list_files(file.parent_folder_id, file.owner_id, tenant_id)
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
            if !(file.path.starts_with("/Workspace/Decisions/")
                || file.path.starts_with("/Decisions/"))
            {
                return Err(DecisionError::NotFound(file.id));
            }
            // Fallback
            let mut metadata = DecisionMetadata::new(stem, "Uncategorized");
            metadata.created_at = file.created_at;
            metadata.updated_at = file.modified_at;
            Ok(metadata)
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

    pub async fn get_decision_summary(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<DecisionSummary, DecisionError> {
        let (file, metadata) = self
            .require_decision(id, user_id, tenant_id, SharePermissions::View)
            .await?;
        Ok(DecisionSummary {
            id: file.id,
            name: file.name,
            path: file.path,
            metadata,
            parent_folder_id: file.parent_folder_id,
            owner_id: file.owner_id,
            modified_at: file.modified_at,
        })
    }

    pub async fn get_decision(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Decision, DecisionError> {
        let (file, meta) = self
            .require_decision(id, user_id, tenant_id, SharePermissions::View)
            .await?;
        let content_bytes = self
            .object_store
            .get(&file.storage_key())
            .await
            .map_err(|e| DecisionError::Storage(e.to_string()))?;
        let content = String::from_utf8_lossy(&content_bytes).to_string();
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
        let (file, _) = self
            .require_decision(id, user_id, tenant_id, SharePermissions::Admin)
            .await?;
        // Delete sidecar if it exists
        let stem = file.name.strip_suffix(".md").unwrap_or(&file.name);
        let sidecar_name = format!("{}.rustshare.json", stem);
        let siblings = self
            .metadata_store
            .list_files(file.parent_folder_id, file.owner_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Database(e.to_string()))?;
        if let Some(sidecar) = siblings.into_iter().find(|f| f.name == sidecar_name) {
            self.file_service
                .require_file_permission(user_id, tenant_id, sidecar.id, SharePermissions::Admin)
                .await?;
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
        self.update_decision_details(id, user_id, tenant_id, title, content, status, None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_decision_details(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        title: Option<String>,
        content: Option<String>,
        status: Option<String>,
        decision_date: Option<DateTime<Utc>>,
    ) -> Result<Decision, DecisionError> {
        let (file, mut meta) = self
            .require_decision(id, user_id, tenant_id, SharePermissions::Edit)
            .await?;

        if title.as_ref().is_some_and(|title| title.trim().is_empty()) {
            return Err(DecisionError::InvalidData("Title cannot be empty".into()));
        }
        let status = status
            .map(|status| normalize_decision_status(&status))
            .transpose()?;
        self.ensure_metadata_sidecar(&file, user_id, &meta).await?;

        if let Some(t) = title {
            meta.title = t;
        }
        if let Some(s) = status {
            meta.status = s;
        }
        if let Some(date) = decision_date {
            meta.decision_date = Some(date);
        }
        meta.updated_at = Utc::now();

        // Update content if provided
        if let Some(c) = content {
            self.file_service
                .edit_file(id, user_id, Bytes::from(c), "overwrite", None)
                .await?;
        }

        self.save_metadata(&file, user_id, &meta).await?;

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
        let (file, mut meta) = self
            .require_decision(id, user_id, tenant_id, SharePermissions::Edit)
            .await?;

        // Validate title
        let trimmed = new_title.trim();
        if trimmed.is_empty() {
            return Err(DecisionError::InvalidData(
                "Title cannot be empty".to_string(),
            ));
        }

        // Keep command UUIDs intact; legacy records retain their numeric DEC prefix.
        let stem = file.name.strip_suffix(".md").unwrap_or(&file.name);
        let dec_prefix = decision_prefix(stem);

        // Generate new filename preserving DEC-ID prefix
        let slug = slug::slugify(trimmed);
        let new_name = format!("{}-{}.md", dec_prefix, slug);

        let old_sidecar_name = format!("{}.rustshare.json", stem);
        let sidecar = self
            .metadata_store
            .list_files(file.parent_folder_id, file.owner_id, tenant_id)
            .await
            .map_err(|error| DecisionError::Database(error.to_string()))?
            .into_iter()
            .find(|entry| entry.name == old_sidecar_name);
        if let Some(sidecar) = &sidecar {
            self.file_service
                .require_file_permission(user_id, tenant_id, sidecar.id, SharePermissions::Edit)
                .await?;
        }

        // Rename the main file
        let renamed_file = self.file_service.rename_file(id, new_name, user_id).await?;

        // Metadata was loaded before renaming, while the old sidecar still matched.
        meta.title = trimmed.to_string();
        meta.updated_at = Utc::now();

        // Save metadata to sidecar
        let new_stem = renamed_file
            .name
            .strip_suffix(".md")
            .unwrap_or(&renamed_file.name);
        let new_sidecar_name = format!("{}.rustshare.json", new_stem);
        let sidecar_data = serde_json::to_vec_pretty(&meta)
            .map_err(|e| DecisionError::InvalidData(e.to_string()))?;

        // Find old sidecar, update its content and name
        if let Some(sidecar_file) = sidecar {
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

    /// Move a decision (Markdown file + sidecar) to a different folder.
    pub async fn move_decision(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
        target_folder_id: Option<Uuid>,
    ) -> Result<Decision, DecisionError> {
        let (file, _) = self
            .require_decision(id, user_id, tenant_id, SharePermissions::Edit)
            .await?;

        // Locate the sidecar in the original folder before moving the main file.
        let stem = file.name.strip_suffix(".md").unwrap_or(&file.name);
        let sidecar_name = format!("{}.rustshare.json", stem);
        let siblings = self
            .metadata_store
            .list_files(file.parent_folder_id, file.owner_id, tenant_id)
            .await
            .map_err(|e| DecisionError::Database(e.to_string()))?;
        let sidecar = siblings.into_iter().find(|f| f.name == sidecar_name);
        if let Some(sidecar) = &sidecar {
            self.file_service
                .require_file_permission(user_id, tenant_id, sidecar.id, SharePermissions::Edit)
                .await?;
        }

        // Move the Markdown file; the sidecar follows to the same destination.
        self.file_service
            .move_file(id, target_folder_id, user_id)
            .await?;

        if let Some(sidecar) = sidecar {
            self.file_service
                .move_file(sidecar.id, target_folder_id, user_id)
                .await?;
        }

        self.get_decision(id, user_id, tenant_id).await
    }

    /// Duplicate a decision in the same folder with a fresh DEC-ID and "(copy)" title.
    pub async fn duplicate_decision(
        &self,
        id: Uuid,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Decision, DecisionError> {
        let original = self.get_decision(id, user_id, tenant_id).await?;
        let original_file = self.file_service.get_file(id, user_id).await?;
        if original_file.tenant_id != tenant_id {
            return Err(DecisionError::PermissionDenied);
        }

        let parent_folder_id = original_file.parent_folder_id.ok_or_else(|| {
            DecisionError::InvalidData("Decision has no parent folder".to_string())
        })?;

        let copy_title = format!("{} (copy)", original.metadata.title);
        let copy_id = Uuid::new_v4();
        let sidecar_id = Uuid::new_v4();
        let slug = slug::slugify(&copy_title);
        let file_name = format!("DEC-{copy_id}-{slug}.md");
        let sidecar_name = format!("DEC-{copy_id}-{slug}.rustshare.json");

        let file = self
            .file_service
            .create_file_if_absent(
                copy_id,
                user_id,
                file_name,
                Some(parent_folder_id),
                Bytes::from(original.content),
                "text/markdown".to_string(),
                tenant_id,
            )
            .await?;

        let mut meta = original.metadata.clone();
        meta.title = copy_title;
        meta.created_at = Utc::now();
        meta.updated_at = Utc::now();

        let sidecar_data = serde_json::to_vec_pretty(&meta)
            .map_err(|e| DecisionError::InvalidData(e.to_string()))?;
        self.file_service
            .create_file_if_absent(
                sidecar_id,
                user_id,
                sidecar_name,
                Some(parent_folder_id),
                Bytes::from(sidecar_data),
                "application/json".to_string(),
                tenant_id,
            )
            .await?;

        self.get_decision(file.id, user_id, tenant_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::{decision_prefix, normalize_decision_status};
    use uuid::Uuid;

    #[test]
    fn accepted_statuses_are_normalized_without_accepting_arbitrary_states() {
        for status in [
            "",
            "Proposed",
            "accepted",
            "Rejected",
            "Deprecated",
            "Superseded",
        ] {
            assert_eq!(
                normalize_decision_status(status).unwrap(),
                status.to_ascii_lowercase()
            );
        }
        assert!(normalize_decision_status("completed").is_err());
    }

    #[test]
    fn renamed_decisions_keep_the_entire_command_identity() {
        let id = Uuid::new_v4();
        let prefix = format!("DEC-{id}");
        assert_eq!(decision_prefix(&prefix), prefix);
        assert_eq!(decision_prefix(&format!("{prefix}-renamed-title")), prefix);
        assert_eq!(decision_prefix("DEC-0042-original-title"), "DEC-0042");
    }

    #[test]
    fn decision_prefix_accepts_unicode_without_panicking() {
        assert_eq!(
            decision_prefix("DEC-決定決定決定決定決定決定決定-title"),
            "DEC-決定決定決定決定決定決定決定"
        );
    }
}
