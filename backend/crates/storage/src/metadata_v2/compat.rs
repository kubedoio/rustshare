//! Compatibility layer for migrating from old MetadataStore to new repositories
//!
//! This module provides adapters that implement the old MetadataStoreOps traits
//! using the new metadata_v2 repositories. This allows gradual migration without
//! rewriting all services at once.

use rustshare_core::domain::{File, FileVersion, Folder, ReplicationJob, ReplicationState, Share};
use sqlx::Row;
use std::sync::Arc;

use crate::repos::*;

/// Adapter that implements the old MetadataStoreOps using new repositories
#[derive(Clone)]
pub struct MetadataStoreCompat {
    repo: Arc<dyn MetadataRepository>,
    pool: sqlx::PgPool,
}

impl MetadataStoreCompat {
    pub fn new(repo: Arc<dyn MetadataRepository>, pool: sqlx::PgPool) -> Self {
        Self { repo, pool }
    }
}

/// Compatibility layer for file operations
#[allow(async_fn_in_trait)]
impl rustshare_core::services::FileMetadataStoreOps for MetadataStoreCompat {
    async fn create_file(&self, file: &File) -> anyhow::Result<()> {
        // Convert old File to new FileDocument
        let doc = file_to_document(file);
        self.repo.files().create(&doc).await.map_err(|e| e.into())
    }

    async fn find_file_by_path(
        &self,
        path: &str,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Option<File>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id
            FROM files
            WHERE path = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(path)
        .bind(owner_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(File {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                path: row.try_get("path")?,
                size: row.try_get("size")?,
                mime_type: row.try_get("mime_type")?,
                content_hash: row.try_get("content_hash")?,
                owner_id: row.try_get("owner_id")?,
                parent_folder_id: row.try_get("parent_folder_id")?,
                current_version: row.try_get("current_version")?,
                created_at: row.try_get("created_at")?,
                modified_at: row.try_get("modified_at")?,
                starred_at: row.try_get("starred_at")?,
                deleted_at: row.try_get("deleted_at")?,
                tenant_id: row.try_get("tenant_id")?,
            }))
        } else {
            Ok(None)
        }
    }

    async fn create_file_version(&self, version: &FileVersion) -> anyhow::Result<()> {
        let doc = version_to_document(version);
        self.repo
            .file_versions()
            .create(&doc)
            .await
            .map_err(|e| e.into())
    }

    async fn find_folder_by_id(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> anyhow::Result<Option<Folder>> {
        match self.repo.folders().get(id).await? {
            Some(doc) => {
                let folder = folder_from_document(&doc);
                if folder.owner_id == owner_id {
                    Ok(Some(folder))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn find_file_by_id(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> anyhow::Result<Option<File>> {
        match self.repo.files().get(id).await? {
            Some(doc) => {
                let file = file_from_document(&doc);
                if file.owner_id == owner_id {
                    Ok(Some(file))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn update_file(&self, file: &File) -> anyhow::Result<()> {
        let doc = file_to_document(file);
        self.repo.files().update(&doc).await.map_err(|e| e.into())
    }

    async fn delete_file(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> anyhow::Result<()> {
        // Verify ownership before deleting
        match self.repo.files().get(id).await? {
            Some(file) => {
                if file.owner_id != owner_id {
                    return Err(anyhow::anyhow!("File not found or access denied"));
                }
                self.repo
                    .files()
                    .delete(id, owner_id)
                    .await
                    .map_err(|e| e.into())
            }
            None => Err(anyhow::anyhow!("File not found")),
        }
    }

    async fn list_file_versions(&self, file_id: uuid::Uuid, owner_id: uuid::Uuid) -> anyhow::Result<Vec<FileVersion>> {
        // Verify file ownership first
        match self.repo.files().get(file_id).await? {
            Some(file) => {
                if file.owner_id != owner_id {
                    return Ok(Vec::new());
                }
            }
            None => return Ok(Vec::new()),
        }
        let docs = self.repo.file_versions().list_by_file(file_id).await?;
        Ok(docs.iter().map(version_from_document).collect())
    }

    async fn find_file_version(
        &self,
        file_id: uuid::Uuid,
        version: i32,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Option<FileVersion>> {
        // Verify file ownership first
        match self.repo.files().get(file_id).await? {
            Some(file) => {
                if file.owner_id != owner_id {
                    return Ok(None);
                }
            }
            None => return Ok(None),
        }
        match self
            .repo
            .file_versions()
            .get_by_number(file_id, version)
            .await?
        {
            Some(doc) => Ok(Some(version_from_document(&doc))),
            None => Ok(None),
        }
    }

    async fn count_enabled_replication_targets(&self) -> anyhow::Result<i64> {
        // TODO: ReplicationTargetRepository not yet available in MetadataRepository trait.
        // When implemented, query: SELECT COUNT(*) FROM replication_targets WHERE enabled = true
        Ok(0)
    }

    async fn create_replication_job(&self, job: &ReplicationJob) -> anyhow::Result<()> {
        // Convert ReplicationJob to JobDocument and create via job repository
        // TODO: JobRepository is not yet accessible through MetadataRepository trait.
        // When available, use: self.repo.jobs().create_job(&job_doc).await
        tracing::debug!("Creating replication job {} (not yet implemented)", job.id);
        Ok(())
    }

    async fn update_file_version_replication_state(
        &self,
        version_id: uuid::Uuid,
        state: ReplicationState,
    ) -> anyhow::Result<()> {
        // Fetch the version, update its replication state, and save
        match self.repo.file_versions().get(version_id).await? {
            Some(_doc) => {
                // Update replication state in the document payload
                // Note: FileVersionDocument doesn't have a direct replication_state field,
                // but we could store it in a future schema update or use event sourcing.
                // For now, log the state change.
                tracing::debug!(
                    "Updating replication state for version {} to {:?} (schema update needed)",
                    version_id,
                    state
                );
                // TODO: When FileVersionDocument has replication_state field:
                // doc.replication_state = state;
                // self.repo.file_versions().update(&doc).await?;
                Ok(())
            }
            None => Err(anyhow::anyhow!("File version not found: {}", version_id)),
        }
    }
}

/// Compatibility layer for folder operations
#[allow(async_fn_in_trait)]
impl rustshare_core::services::FolderMetadataStoreOps for MetadataStoreCompat {
    async fn create_folder(&self, folder: &Folder) -> anyhow::Result<()> {
        let doc = folder_to_document(folder);
        self.repo.folders().create(&doc).await.map_err(|e| e.into())
    }

    async fn find_folder_by_id(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> anyhow::Result<Option<Folder>> {
        match self.repo.folders().get(id).await? {
            Some(doc) => {
                let folder = folder_from_document(&doc);
                if folder.owner_id == owner_id {
                    Ok(Some(folder))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn update_folder(&self, folder: &Folder) -> anyhow::Result<()> {
        let doc = folder_to_document(folder);
        self.repo.folders().update(&doc).await.map_err(|e| e.into())
    }

    async fn delete_folder(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> anyhow::Result<()> {
        // Verify ownership before deleting
        match self.repo.folders().get(id).await? {
            Some(folder) => {
                if folder.owner_id != owner_id {
                    return Err(anyhow::anyhow!("Folder not found or access denied"));
                }
                self.repo
                    .folders()
                    .delete(id, owner_id)
                    .await
                    .map_err(|e| e.into())
            }
            None => Err(anyhow::anyhow!("Folder not found")),
        }
    }

    async fn list_folders(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        _tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<Folder>> {
        // Use the folder children index for efficient lookup
        let folder_id = parent_id.unwrap_or_else(uuid::Uuid::nil);

        match self.repo.folder_children_index().get(folder_id).await? {
            Some(index) => {
                let mut folders = Vec::new();
                for entry in &index.children {
                    if entry.kind == "folder" && !entry.deleted {
                        if let Some(doc) = self.repo.folders().get(entry.id).await? {
                            // Filter by owner_id to ensure security
                            if doc.owner_id == owner_id {
                                folders.push(folder_from_document(&doc));
                            }
                        }
                    }
                }
                Ok(folders)
            }
            None => {
                // No children index yet - return empty (could fall back to scanning)
                Ok(Vec::new())
            }
        }
    }

    async fn list_folders_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        _tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<Folder>> {
        let folder_id = parent_id.unwrap_or_else(uuid::Uuid::nil);

        match self.repo.folder_children_index().get(folder_id).await? {
            Some(index) => {
                let mut folders = Vec::new();
                for entry in &index.children {
                    if entry.kind == "folder" && !entry.deleted {
                        if let Some(doc) = self.repo.folders().get(entry.id).await? {
                            folders.push(folder_from_document(&doc));
                        }
                    }
                }
                Ok(folders)
            }
            None => Ok(Vec::new()),
        }
    }

    async fn find_descendant_folders(&self, folder_id: uuid::Uuid) -> anyhow::Result<Vec<Folder>> {
        let docs = self.repo.folders().list_descendants(folder_id).await?;
        Ok(docs.iter().map(folder_from_document).collect())
    }

    async fn list_files(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        _tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<File>> {
        // Use the folder children index for efficient lookup
        let folder_id = parent_id.unwrap_or_else(uuid::Uuid::nil);

        match self.repo.folder_children_index().get(folder_id).await? {
            Some(index) => {
                let mut files = Vec::new();
                for entry in &index.children {
                    if entry.kind == "file" && !entry.deleted {
                        if let Some(doc) = self.repo.files().get(entry.id).await? {
                            // Filter by owner_id to ensure security
                            if doc.owner_id == owner_id {
                                files.push(file_from_document(&doc));
                            }
                        }
                    }
                }
                Ok(files)
            }
            None => {
                // No children index yet - return empty (could fall back to scanning)
                Ok(Vec::new())
            }
        }
    }

    async fn list_files_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        _tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<File>> {
        let folder_id = parent_id.unwrap_or_else(uuid::Uuid::nil);

        match self.repo.folder_children_index().get(folder_id).await? {
            Some(index) => {
                let mut files = Vec::new();
                for entry in &index.children {
                    if entry.kind == "file" && !entry.deleted {
                        if let Some(doc) = self.repo.files().get(entry.id).await? {
                            files.push(file_from_document(&doc));
                        }
                    }
                }
                Ok(files)
            }
            None => Ok(Vec::new()),
        }
    }
}

/// Compatibility layer for share operations
#[allow(async_fn_in_trait)]
impl rustshare_core::services::ShareMetadataStoreOps for MetadataStoreCompat {
    async fn find_user_by_id(
        &self,
        id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::User>> {
        let row = sqlx::query_as::<_, rustshare_core::domain::User>(
            r#"
            SELECT 
                id, username, email, password_hash, display_name, is_admin, 
                storage_quota, theme, created_at, updated_at, disabled_at, 
                name, surname, avatar_path, email_sharing_enabled, tenant_id
            FROM users 
            WHERE id = $1 AND disabled_at IS NULL
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    async fn find_file_by_id(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> anyhow::Result<Option<File>> {
        match self.repo.files().get(id).await? {
            Some(doc) => {
                let file = file_from_document(&doc);
                if file.owner_id == owner_id {
                    Ok(Some(file))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn find_folder_by_id(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> anyhow::Result<Option<Folder>> {
        match self.repo.folders().get(id).await? {
            Some(doc) => {
                let folder = folder_from_document(&doc);
                if folder.owner_id == owner_id {
                    Ok(Some(folder))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn create_share(&self, share: &Share) -> anyhow::Result<()> {
        let doc = share_to_document(share);
        self.repo.shares().create(&doc).await.map_err(|e| e.into())
    }

    async fn get_share_by_id(&self, id: uuid::Uuid, actor_id: uuid::Uuid) -> anyhow::Result<Option<Share>> {
        match self.repo.shares().get(id).await? {
            Some(doc) => {
                let share = share_from_document(&doc);
                if share.created_by == actor_id {
                    Ok(Some(share))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn get_share_by_token(&self, token: &str) -> anyhow::Result<Option<Share>> {
        // Hash the token for lookup
        let token_hash = format!("{:x}", md5::compute(token));
        match self.repo.shares().get_by_token(&token_hash).await? {
            Some(doc) => Ok(Some(share_from_document(&doc))),
            None => Ok(None),
        }
    }

    async fn get_file_shares(&self, file_id: uuid::Uuid, actor_id: uuid::Uuid) -> anyhow::Result<Vec<Share>> {
        let docs = self.repo.shares().list_by_resource("file", file_id).await?;
        Ok(docs
            .iter()
            .map(share_from_document)
            .filter(|s| s.created_by == actor_id)
            .collect())
    }

    async fn get_folder_shares(&self, folder_id: uuid::Uuid, actor_id: uuid::Uuid) -> anyhow::Result<Vec<Share>> {
        let docs = self
            .repo
            .shares()
            .list_by_resource("folder", folder_id)
            .await?;
        Ok(docs
            .iter()
            .map(share_from_document)
            .filter(|s| s.created_by == actor_id)
            .collect())
    }

    async fn list_files(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<File>> {
        // Delegate to FolderMetadataStoreOps implementation
        <Self as rustshare_core::services::FolderMetadataStoreOps>::list_files(
            self, parent_id, owner_id, tenant_id,
        )
        .await
    }

    async fn list_files_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<File>> {
        <Self as rustshare_core::services::FolderMetadataStoreOps>::list_files_by_parent(
            self, parent_id, tenant_id,
        )
        .await
    }

    async fn list_folders(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<Folder>> {
        // Delegate to FolderMetadataStoreOps implementation
        <Self as rustshare_core::services::FolderMetadataStoreOps>::list_folders(
            self, parent_id, owner_id, tenant_id,
        )
        .await
    }

    async fn list_folders_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<Folder>> {
        <Self as rustshare_core::services::FolderMetadataStoreOps>::list_folders_by_parent(
            self, parent_id, tenant_id,
        )
        .await
    }

    async fn find_descendant_folders(&self, folder_id: uuid::Uuid) -> anyhow::Result<Vec<Folder>> {
        let docs = self.repo.folders().list_descendants(folder_id).await?;
        Ok(docs.iter().map(folder_from_document).collect())
    }

    async fn revoke_share(&self, share_id: uuid::Uuid, actor_id: uuid::Uuid) -> anyhow::Result<()> {
        // Verify ownership before revoking
        match self.repo.shares().get(share_id).await? {
            Some(doc) => {
                let share = share_from_document(&doc);
                if share.created_by != actor_id {
                    return Err(anyhow::anyhow!("Share not found or access denied"));
                }
            }
            None => return Err(anyhow::anyhow!("Share not found")),
        }
        self.repo
            .shares()
            .revoke(share_id, actor_id)
            .await
            .map_err(|e| e.into())
    }

    async fn update_share(&self, share: &Share) -> anyhow::Result<()> {
        let doc = share_to_document(share);
        self.repo.shares().update(&doc).await.map_err(|e| e.into())
    }

    async fn is_user_in_group(
        &self,
        user_id: uuid::Uuid,
        group_id: uuid::Uuid,
    ) -> anyhow::Result<bool> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM group_members
                WHERE group_id = $1 AND user_id = $2
            )
            "#,
        )
        .bind(group_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }
}

// ============================================================================
// Conversion functions between old and new types
// ============================================================================

use crate::metadata_v2::schemas::*;

fn folder_to_document(folder: &Folder) -> FolderDocument {
    // Use folder's ancestor_ids if available, otherwise empty vec
    let ancestor_ids = folder.ancestor_ids.clone().unwrap_or_default();

    FolderDocument {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: folder.id,
        namespace_id: folder.tenant_id,
        parent_id: folder.parent_folder_id,
        name: folder.name.clone(),
        path: folder.path.clone(),
        owner_id: folder.owner_id,
        tenant_id: folder.tenant_id,
        created_at: folder.created_at,
        updated_at: folder.updated_at,
        version: 1,
        deleted: false,
        ancestor_ids,
    }
}

fn folder_from_document(doc: &FolderDocument) -> Folder {
    Folder {
        id: doc.id,
        name: doc.name.clone(),
        path: doc.path.clone(),
        parent_folder_id: doc.parent_id,
        owner_id: doc.owner_id,
        created_at: doc.created_at,
        updated_at: doc.updated_at,
        starred_at: None,
        deleted_at: None,
        tenant_id: doc.tenant_id,
        ancestor_ids: Some(doc.ancestor_ids.clone()),
    }
}

fn file_to_document(file: &File) -> FileDocument {
    FileDocument {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: file.id,
        namespace_id: file.tenant_id,
        parent_id: file.parent_folder_id,
        name: file.name.clone(),
        path: file.path.clone(),
        owner_id: file.owner_id,
        tenant_id: file.tenant_id,
        current_version_id: uuid::Uuid::nil(), // Would need proper version tracking
        version_number: file.current_version,
        size: file.size,
        mime_type: file.mime_type.clone(),
        content_ref: format!("sha256:{}", file.content_hash),
        checksum: file.content_hash.clone(),
        created_at: file.created_at,
        updated_at: file.modified_at,
        version: 1,
        deleted: false,
    }
}

fn file_from_document(doc: &FileDocument) -> File {
    File {
        id: doc.id,
        name: doc.name.clone(),
        path: doc.path.clone(),
        content_hash: doc.checksum.clone(),
        size: doc.size,
        mime_type: doc.mime_type.clone(),
        parent_folder_id: doc.parent_id,
        owner_id: doc.owner_id,
        current_version: doc.version_number,
        created_at: doc.created_at,
        modified_at: doc.updated_at,
        starred_at: None,
        deleted_at: None,
        tenant_id: doc.tenant_id,
    }
}

fn version_to_document(version: &FileVersion) -> FileVersionDocument {
    FileVersionDocument {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: version.id,
        file_id: version.file_id,
        version_number: version.version_number,
        content_ref: format!("sha256:{}", version.content_hash),
        size: version.size,
        checksum: version.content_hash.clone(),
        created_by: version.created_by,
        tenant_id: version.tenant_id,
        created_at: version.created_at,
        change_description: version.change_description.clone(),
    }
}

fn version_from_document(doc: &FileVersionDocument) -> FileVersion {
    FileVersion {
        id: doc.id,
        file_id: doc.file_id,
        version_number: doc.version_number,
        content_hash: doc.checksum.clone(),
        size: doc.size,
        replication_state: ReplicationState::PrimaryWritten, // Default
        created_by: doc.created_by,
        created_at: doc.created_at,
        change_description: doc.change_description.clone(),
        tenant_id: doc.tenant_id,
    }
}

fn share_to_document(share: &Share) -> ShareDocument {
    let (resource_type, resource_id) = if let Some(file_id) = share.file_id {
        ("file".to_string(), file_id)
    } else if let Some(folder_id) = share.folder_id {
        ("folder".to_string(), folder_id)
    } else {
        ("unknown".to_string(), uuid::Uuid::nil())
    };

    let scope = if share.recipient_group_id.is_some() {
        ShareScope::Group
    } else if share.recipient_user_id.is_some() {
        ShareScope::User
    } else {
        ShareScope::Public
    };

    ShareDocument {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: share.id,
        resource_type,
        resource_id,
        scope,
        permissions: share.permissions,
        token_hash: share
            .share_token
            .as_ref()
            .map(|t| format!("{:x}", md5::compute(t))),
        share_token: share.share_token.clone(), // Store original token
        recipient_user_id: share.recipient_user_id,
        recipient_group_id: share.recipient_group_id,
        password_hash: share.password_hash.clone(),
        expires_at: share.expires_at,
        upload_only: share.upload_only,
        access_count: share.access_count,
        created_by: share.created_by,
        tenant_id: share.tenant_id,
        created_at: share.created_at,
        revoked_at: share.revoked_at,
        version: 1,
    }
}

fn share_from_document(doc: &ShareDocument) -> Share {
    let (file_id, folder_id) = match doc.resource_type.as_str() {
        "file" => (Some(doc.resource_id), None),
        "folder" => (None, Some(doc.resource_id)),
        _ => (None, None),
    };

    Share {
        id: doc.id,
        file_id,
        folder_id,
        share_token: doc.share_token.clone(), // Use original token
        permissions: doc.permissions,
        password_hash: doc.password_hash.clone(),
        expires_at: doc.expires_at,
        upload_only: doc.upload_only,
        access_count: doc.access_count,
        recipient_user_id: doc.recipient_user_id,
        recipient_group_id: doc.recipient_group_id,
        created_by: doc.created_by,
        tenant_id: doc.tenant_id,
        created_at: doc.created_at,
        revoked_at: doc.revoked_at,
    }
}
