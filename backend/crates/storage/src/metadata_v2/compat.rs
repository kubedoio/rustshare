//! Compatibility layer for migrating from old MetadataStore to new repositories
//!
//! This module provides adapters that implement the old MetadataStoreOps traits
//! using the new metadata_v2 repositories. This allows gradual migration without
//! rewriting all services at once.

use rustshare_core::domain::{
    File, FileVersion, Folder, ReplicationJob, ReplicationState, Share,
};
use std::sync::Arc;
use tracing::debug;

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

    async fn create_file_version(&self, version: &FileVersion) -> anyhow::Result<()> {
        let doc = version_to_document(version);
        self.repo.file_versions().create(&doc).await.map_err(|e| e.into())
    }

    async fn find_folder_by_id(&self, id: uuid::Uuid) -> anyhow::Result<Option<Folder>> {
        match self.repo.folders().get(id).await? {
            Some(doc) => Ok(Some(folder_from_document(&doc))),
            None => Ok(None),
        }
    }

    async fn find_file_by_id(&self, id: uuid::Uuid) -> anyhow::Result<Option<File>> {
        match self.repo.files().get(id).await? {
            Some(doc) => Ok(Some(file_from_document(&doc))),
            None => Ok(None),
        }
    }

    async fn update_file(&self, file: &File) -> anyhow::Result<()> {
        let doc = file_to_document(file);
        self.repo.files().update(&doc).await.map_err(|e| e.into())
    }

    async fn delete_file(&self, id: uuid::Uuid) -> anyhow::Result<()> {
        // TODO: The trait method doesn't receive `deleted_by`, but repository requires it.
        // Proper audit tracking requires passing the actor through the trait.
        // For now, try to get the owner_id from the file as a fallback, otherwise use nil.
        let deleted_by = match self.repo.files().get(id).await? {
            Some(file) => file.owner_id,
            None => uuid::Uuid::nil(),
        };
        self.repo.files().delete(id, deleted_by).await.map_err(|e| e.into())
    }

    async fn list_file_versions(&self, file_id: uuid::Uuid) -> anyhow::Result<Vec<FileVersion>> {
        let docs = self.repo.file_versions().list_by_file(file_id).await?;
        Ok(docs.iter().map(version_from_document).collect())
    }

    async fn find_file_version(
        &self,
        file_id: uuid::Uuid,
        version: i32,
    ) -> anyhow::Result<Option<FileVersion>> {
        match self.repo.file_versions().get_by_number(file_id, version).await? {
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

    async fn find_folder_by_id(&self, id: uuid::Uuid) -> anyhow::Result<Option<Folder>> {
        match self.repo.folders().get(id).await? {
            Some(doc) => Ok(Some(folder_from_document(&doc))),
            None => Ok(None),
        }
    }

    async fn update_folder(&self, folder: &Folder) -> anyhow::Result<()> {
        let doc = folder_to_document(folder);
        self.repo.folders().update(&doc).await.map_err(|e| e.into())
    }

    async fn delete_folder(&self, id: uuid::Uuid) -> anyhow::Result<()> {
        // TODO: The trait method doesn't receive `deleted_by`, but repository requires it.
        // Proper audit tracking requires passing the actor through the trait.
        // For now, try to get the owner_id from the folder as a fallback, otherwise use nil.
        let deleted_by = match self.repo.folders().get(id).await? {
            Some(folder) => folder.owner_id,
            None => uuid::Uuid::nil(),
        };
        self.repo.folders().delete(id, deleted_by).await.map_err(|e| e.into())
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
}

/// Compatibility layer for share operations
#[allow(async_fn_in_trait)]
impl rustshare_core::services::ShareMetadataStoreOps for MetadataStoreCompat {
    async fn find_user_by_id(&self, _id: uuid::Uuid) -> anyhow::Result<Option<rustshare_core::domain::User>> {
        // TODO: Implement user lookup in the compat layer
        // For now, return None as this is primarily used for notification purposes
        Ok(None)
    }

    async fn find_file_by_id(&self, id: uuid::Uuid) -> anyhow::Result<Option<File>> {
        match self.repo.files().get(id).await? {
            Some(doc) => Ok(Some(file_from_document(&doc))),
            None => Ok(None),
        }
    }

    async fn find_folder_by_id(&self, id: uuid::Uuid) -> anyhow::Result<Option<Folder>> {
        match self.repo.folders().get(id).await? {
            Some(doc) => Ok(Some(folder_from_document(&doc))),
            None => Ok(None),
        }
    }

    async fn create_share(&self, share: &Share) -> anyhow::Result<()> {
        let doc = share_to_document(share);
        self.repo.shares().create(&doc).await.map_err(|e| e.into())
    }

    async fn get_share_by_id(&self, id: uuid::Uuid) -> anyhow::Result<Option<Share>> {
        match self.repo.shares().get(id).await? {
            Some(doc) => Ok(Some(share_from_document(&doc))),
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

    async fn get_file_shares(&self, file_id: uuid::Uuid) -> anyhow::Result<Vec<Share>> {
        let docs = self.repo.shares().list_by_resource("file", file_id).await?;
        Ok(docs.iter().map(share_from_document).collect())
    }

    async fn get_folder_shares(&self, folder_id: uuid::Uuid) -> anyhow::Result<Vec<Share>> {
        let docs = self.repo.shares().list_by_resource("folder", folder_id).await?;
        Ok(docs.iter().map(share_from_document).collect())
    }

    async fn list_files(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<File>> {
        // Delegate to FolderMetadataStoreOps implementation
        <Self as rustshare_core::services::FolderMetadataStoreOps>::list_files(
            self, parent_id, owner_id, tenant_id
        ).await
    }

    async fn list_folders(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<Folder>> {
        // Delegate to FolderMetadataStoreOps implementation
        <Self as rustshare_core::services::FolderMetadataStoreOps>::list_folders(
            self, parent_id, owner_id, tenant_id
        ).await
    }

    async fn find_descendant_folders(&self, folder_id: uuid::Uuid) -> anyhow::Result<Vec<Folder>> {
        let docs = self.repo.folders().list_descendants(folder_id).await?;
        Ok(docs.iter().map(folder_from_document).collect())
    }

    async fn revoke_share(&self, share_id: uuid::Uuid) -> anyhow::Result<()> {
        let revoked_by = uuid::Uuid::nil();
        self.repo.shares().revoke(share_id, revoked_by).await.map_err(|e| e.into())
    }

    async fn update_share(&self, share: &Share) -> anyhow::Result<()> {
        let doc = share_to_document(share);
        self.repo.shares().update(&doc).await.map_err(|e| e.into())
    }

    async fn is_user_in_group(&self, user_id: uuid::Uuid, group_id: uuid::Uuid) -> anyhow::Result<bool> {
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

    let permissions = match share.permissions {
        rustshare_core::domain::SharePermissions::View => SharePermission::View,
        rustshare_core::domain::SharePermissions::Edit => SharePermission::Edit,
        rustshare_core::domain::SharePermissions::Admin => SharePermission::Admin,
    };

    ShareDocument {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: share.id,
        resource_type,
        resource_id,
        scope,
        permissions,
        token_hash: share.share_token.as_ref().map(|t| format!("{:x}", md5::compute(t))),
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
    use rustshare_core::domain::SharePermissions;

    let (file_id, folder_id) = match doc.resource_type.as_str() {
        "file" => (Some(doc.resource_id), None),
        "folder" => (None, Some(doc.resource_id)),
        _ => (None, None),
    };

    let permissions = match doc.permissions {
        SharePermission::View => SharePermissions::View,
        SharePermission::Edit => SharePermissions::Edit,
        SharePermission::Admin => SharePermissions::Admin,
    };

    Share {
        id: doc.id,
        file_id,
        folder_id,
        share_token: doc.token_hash.clone(), // Note: this is the hash, not the original token
        permissions,
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
