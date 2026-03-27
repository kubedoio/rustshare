//! RustFS-backed repository implementations

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use rustshare_core::domain::{FileId, FolderId, ShareId, UserId};
use std::sync::Arc;
use uuid::Uuid;

use super::{RepositoryError, *};
use crate::metadata_v2::{
    EventLogStore, FolderChildrenIndex,
    MetadataDocumentStore, MetadataDocumentStoreExt, PutOptions, RuntimeMetadataCache,
};
use crate::metadata_v2::schemas::*;

/// Path builder for RustFS storage layout
#[derive(Clone)]
pub struct PathBuilder {
    base_prefix: String,
    namespace: String,
}

impl PathBuilder {
    pub fn new(base_prefix: String, namespace: String) -> Self {
        Self {
            base_prefix,
            namespace,
        }
    }
    
    pub fn folder(&self, id: FolderId) -> String {
        format!("{}/{}/meta/folders/{}.json", self.base_prefix, self.namespace, id)
    }
    
    pub fn file(&self, id: FileId) -> String {
        format!("{}/{}/meta/files/{}.json", self.base_prefix, self.namespace, id)
    }
    
    pub fn file_version(&self, file_id: FileId, version_id: Uuid) -> String {
        format!(
            "{}/{}/meta/file_versions/{}/{}.json",
            self.base_prefix, self.namespace, file_id, version_id
        )
    }
    
    pub fn share(&self, id: ShareId) -> String {
        format!("{}/{}/meta/shares/{}.json", self.base_prefix, self.namespace, id)
    }
    
    pub fn event(&self, event: &EventDocument) -> String {
        format!(
            "{}/{}/meta/events/{:04}/{:02}/{:02}/{}.json",
            self.base_prefix,
            self.namespace,
            event.occurred_at.year(),
            event.occurred_at.month(),
            event.occurred_at.day(),
            event.id
        )
    }
    
    pub fn tombstone(&self, resource_type: &str, resource_id: Uuid) -> String {
        format!(
            "{}/{}/meta/tombstones/{}/{}.json",
            self.base_prefix, self.namespace, resource_type, resource_id
        )
    }
    
    pub fn folder_children_index(&self, folder_id: FolderId) -> String {
        format!(
            "{}/{}/indexes/folders/{}/children.json",
            self.base_prefix, self.namespace, folder_id
        )
    }
    
    pub fn user_roots_index(&self, user_id: UserId) -> String {
        format!(
            "{}/{}/indexes/users/{}/roots.json",
            self.base_prefix, self.namespace, user_id
        )
    }
    
    pub fn shared_with_me_index(&self, user_id: UserId) -> String {
        format!(
            "{}/{}/indexes/users/{}/shared_with_me.json",
            self.base_prefix, self.namespace, user_id
        )
    }
    
    // User paths
    pub fn user(&self, id: UserId) -> String {
        format!("{}/{}/meta/users/{}.json", self.base_prefix, self.namespace, id)
    }
    
    // Device token paths
    pub fn device(&self, id: Uuid) -> String {
        format!("{}/{}/meta/devices/{}.json", self.base_prefix, self.namespace, id)
    }
    
    // Group paths
    pub fn group(&self, id: Uuid) -> String {
        format!("{}/{}/meta/groups/{}.json", self.base_prefix, self.namespace, id)
    }
    
    // Pairing paths
    pub fn pairing(&self, id: Uuid) -> String {
        format!("{}/{}/meta/pairings/{}.json", self.base_prefix, self.namespace, id)
    }
    
    // Webhook paths
    pub fn webhook(&self, id: Uuid) -> String {
        format!("{}/{}/meta/webhooks/{}.json", self.base_prefix, self.namespace, id)
    }
    
    // Job paths
    pub fn job(&self, id: Uuid) -> String {
        format!("{}/{}/meta/jobs/{}.json", self.base_prefix, self.namespace, id)
    }
    
    // Config paths
    pub fn config(&self, config_type: &str) -> String {
        format!("{}/{}/meta/config/{}.json", self.base_prefix, self.namespace, config_type)
    }
    
    // Audit log path
    pub fn audit_entry(&self, occurred_at: DateTime<Utc>, id: Uuid) -> String {
        format!(
            "{}/{}/audit/{:04}/{:02}/{:02}/{}.json",
            self.base_prefix,
            self.namespace,
            occurred_at.year(),
            occurred_at.month(),
            occurred_at.day(),
            id
        )
    }
    
    // Lookup paths
    pub fn email_lookup(&self, email_hash: &str) -> String {
        format!(
            "{}/{}/lookups/user_by_email/{}.json",
            self.base_prefix, self.namespace, email_hash
        )
    }
    
    pub fn token_lookup(&self, token_hash: &str) -> String {
        format!(
            "{}/{}/lookups/public_share_tokens/{}.json",
            self.base_prefix, self.namespace, token_hash
        )
    }
    
    pub fn pairing_code_lookup(&self, code: &str) -> String {
        format!(
            "{}/{}/lookups/pairing_codes/{}.json",
            self.base_prefix, self.namespace, code
        )
    }
    
    // Index paths
    pub fn user_devices_index(&self, user_id: UserId) -> String {
        format!(
            "{}/{}/indexes/users/{}/devices.json",
            self.base_prefix, self.namespace, user_id
        )
    }
    
    pub fn user_notifications_index(&self, user_id: UserId) -> String {
        format!(
            "{}/{}/indexes/users/{}/notifications.json",
            self.base_prefix, self.namespace, user_id
        )
    }
    
    pub fn user_groups_index(&self, user_id: UserId) -> String {
        format!(
            "{}/{}/indexes/users/{}/groups.json",
            self.base_prefix, self.namespace, user_id
        )
    }
    
    pub fn group_members_index(&self, group_id: Uuid) -> String {
        format!(
            "{}/{}/indexes/groups/{}/members.json",
            self.base_prefix, self.namespace, group_id
        )
    }
    
    pub fn job_queue_index(&self) -> String {
        format!(
            "{}/{}/indexes/jobs/queue.json",
            self.base_prefix, self.namespace
        )
    }
    
    pub fn resource_shares_index(&self, resource_id: Uuid) -> String {
        format!(
            "{}/{}/indexes/shares/by_resource/{}.json",
            self.base_prefix, self.namespace, resource_id
        )
    }
}

/// RustFS-backed folder repository
pub struct RustFsFolderRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
    cache: Option<Arc<RuntimeMetadataCache>>,
}

impl RustFsFolderRepository {
    pub fn new(
        doc_store: Arc<dyn MetadataDocumentStore>,
        path_builder: PathBuilder,
        cache: Option<Arc<RuntimeMetadataCache>>,
    ) -> Self {
        Self {
            doc_store,
            path_builder,
            cache,
        }
    }
}

#[async_trait]
impl FolderRepository for RustFsFolderRepository {
    async fn get(&self, id: FolderId) -> Result<Option<FolderDocument>, RepositoryError> {
        // Check cache first
        if let Some(cache) = &self.cache {
            if let Some(folder) = cache.get_folder(id) {
                return Ok(Some(folder));
            }
        }
        
        let key = self.path_builder.folder(id);
        let result = self.doc_store.get::<FolderDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some((ref folder, _)) = result {
            if let Some(ref cache) = self.cache {
                cache.put_folder(folder.clone());
            }
        }
        
        Ok(result.map(|(doc, _)| doc))
    }
    
    async fn create(&self, folder: &FolderDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.folder(folder.id);
        
        // Use if-none-match to ensure we don't overwrite
        let opts = PutOptions {
            if_none_match: Some("*".to_string()),
            ..Default::default()
        };
        
        self.doc_store.put(&key, folder, opts).await
            .map_err(|e| {
                if e.to_string().contains("Precondition") {
                    RepositoryError::AlreadyExists(format!("Folder {} already exists", folder.id))
                } else {
                    RepositoryError::StorageError(e.to_string())
                }
            })?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.on_folder_created(folder);
        }
        
        Ok(())
    }
    
    async fn update(&self, folder: &FolderDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.folder(folder.id);
        
        self.doc_store.put(&key, folder, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.put_folder(folder.clone());
        }
        
        Ok(())
    }
    
    async fn delete(&self, id: FolderId, deleted_by: UserId) -> Result<(), RepositoryError> {
        // Get the folder first
        let folder = self.get_required(id).await?;
        
        // Create tombstone
        let tombstone = TombstoneDocument::from_folder(&folder, deleted_by);
        let tombstone_key = self.path_builder.tombstone("folders", id);
        
        self.doc_store.put(&tombstone_key, &tombstone, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Mark as deleted in place (or we could move it)
        let mut deleted_folder = folder.clone();
        deleted_folder.deleted = true;
        deleted_folder.bump_version();
        
        let folder_key = self.path_builder.folder(id);
        self.doc_store.put(&folder_key, &deleted_folder, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.on_folder_deleted(&folder);
        }
        
        Ok(())
    }
    
    async fn hard_delete(&self, id: FolderId) -> Result<(), RepositoryError> {
        let key = self.path_builder.folder(id);
        
        self.doc_store.delete(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Invalidate cache
        if let Some(ref cache) = self.cache {
            cache.invalidate_folder(id);
        }
        
        Ok(())
    }
    
    async fn list_descendants(&self, folder_id: FolderId) -> Result<Vec<FolderDocument>, RepositoryError> {
        // This requires scanning - in production, maintain an index
        let prefix = format!("{}/{}/meta/folders/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut descendants = Vec::new();
        let mut stack = vec![folder_id];
        
        while let Some(current_id) = stack.pop() {
            let key = self.path_builder.folder(current_id);
            
            if let Some((folder, _)) = self.doc_store.get::<FolderDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                
                // Find children by scanning
                for key in &keys {
                    if let Some((child, _)) = self.doc_store.get::<FolderDocument>(key).await
                        .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                        if child.parent_id == Some(current_id) && !child.deleted {
                            stack.push(child.id);
                        }
                    }
                }
                
                if folder.id != folder_id {
                    descendants.push(folder);
                }
            }
        }
        
        Ok(descendants)
    }
    
    async fn get_user_roots(&self, user_id: UserId) -> Result<Vec<FolderDocument>, RepositoryError> {
        // This requires scanning - use index in production
        let prefix = format!("{}/{}/meta/folders/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut roots = Vec::new();
        
        for key in keys {
            if let Some((folder, _)) = self.doc_store.get::<FolderDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if folder.owner_id == user_id && folder.parent_id.is_none() && !folder.deleted {
                    roots.push(folder);
                }
            }
        }
        
        Ok(roots)
    }
    
    async fn name_exists(
        &self,
        parent_id: Option<FolderId>,
        name: &str,
        owner_id: UserId,
    ) -> Result<bool, RepositoryError> {
        // Check children index if available, otherwise scan
        if let Some(parent_id) = parent_id {
            let index_key = self.path_builder.folder_children_index(parent_id);
            
            if let Some((index, _)) = self.doc_store.get::<FolderChildrenIndex>(&index_key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                
                return Ok(index.children.iter().any(|c| {
                    c.name == name && c.kind == "folder" && !c.deleted
                }));
            }
        }
        
        // Fall back to scanning
        let prefix = format!("{}/{}/meta/folders/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        for key in keys {
            if let Some((folder, _)) = self.doc_store.get::<FolderDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if folder.parent_id == parent_id 
                    && folder.name == name 
                    && folder.owner_id == owner_id
                    && !folder.deleted {
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }
}

/// RustFS-backed file repository
pub struct RustFsFileRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
    cache: Option<Arc<RuntimeMetadataCache>>,
}

impl RustFsFileRepository {
    pub fn new(
        doc_store: Arc<dyn MetadataDocumentStore>,
        path_builder: PathBuilder,
        cache: Option<Arc<RuntimeMetadataCache>>,
    ) -> Self {
        Self {
            doc_store,
            path_builder,
            cache,
        }
    }
}

#[async_trait]
impl FileRepository for RustFsFileRepository {
    async fn get(&self, id: FileId) -> Result<Option<FileDocument>, RepositoryError> {
        // Check cache first
        if let Some(cache) = &self.cache {
            if let Some(file) = cache.get_file(id) {
                return Ok(Some(file));
            }
        }
        
        let key = self.path_builder.file(id);
        let result = self.doc_store.get::<FileDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some((ref file, _)) = result {
            if let Some(ref cache) = self.cache {
                cache.put_file(file.clone());
            }
        }
        
        Ok(result.map(|(doc, _)| doc))
    }
    
    async fn create(&self, file: &FileDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.file(file.id);
        
        let opts = PutOptions {
            if_none_match: Some("*".to_string()),
            ..Default::default()
        };
        
        self.doc_store.put(&key, file, opts).await
            .map_err(|e| {
                if e.to_string().contains("Precondition") {
                    RepositoryError::AlreadyExists(format!("File {} already exists", file.id))
                } else {
                    RepositoryError::StorageError(e.to_string())
                }
            })?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.on_file_created(file);
        }
        
        Ok(())
    }
    
    async fn update(&self, file: &FileDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.file(file.id);
        
        self.doc_store.put(&key, file, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.on_file_updated(file);
        }
        
        Ok(())
    }
    
    async fn delete(&self, id: FileId, deleted_by: UserId) -> Result<(), RepositoryError> {
        let file = self.get_required(id).await?;
        
        // Create tombstone
        let tombstone = TombstoneDocument::from_file(&file, deleted_by);
        let tombstone_key = self.path_builder.tombstone("files", id);
        
        self.doc_store.put(&tombstone_key, &tombstone, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Mark as deleted
        let mut deleted_file = file.clone();
        deleted_file.deleted = true;
        deleted_file.bump_version();
        
        let file_key = self.path_builder.file(id);
        self.doc_store.put(&file_key, &deleted_file, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.on_file_deleted(id, file.parent_id);
        }
        
        Ok(())
    }
    
    async fn hard_delete(&self, id: FileId) -> Result<(), RepositoryError> {
        let key = self.path_builder.file(id);
        
        self.doc_store.delete(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Invalidate cache
        if let Some(ref cache) = self.cache {
            cache.invalidate_file(id);
        }
        
        Ok(())
    }
    
    async fn name_exists(
        &self,
        parent_id: Option<FolderId>,
        name: &str,
        owner_id: UserId,
    ) -> Result<bool, RepositoryError> {
        // Check children index if available
        if let Some(parent_id) = parent_id {
            let index_key = self.path_builder.folder_children_index(parent_id);
            
            if let Some((index, _)) = self.doc_store.get::<FolderChildrenIndex>(&index_key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                
                return Ok(index.children.iter().any(|c| {
                    c.name == name && c.kind == "file" && !c.deleted
                }));
            }
        }
        
        // Fall back to scanning
        let prefix = format!("{}/{}/meta/files/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        for key in keys {
            if let Some((file, _)) = self.doc_store.get::<FileDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if file.parent_id == parent_id 
                    && file.name == name 
                    && file.owner_id == owner_id
                    && !file.deleted {
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }
}

/// RustFS-backed file version repository
pub struct RustFsFileVersionRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsFileVersionRepository {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, path_builder: PathBuilder) -> Self {
        Self {
            doc_store,
            path_builder,
        }
    }
}

#[async_trait]
impl FileVersionRepository for RustFsFileVersionRepository {
    async fn get(&self, version_id: Uuid) -> Result<Option<FileVersionDocument>, RepositoryError> {
        // This requires scanning since version_id is in the path
        let prefix = format!("{}/{}/meta/file_versions/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        for key in keys {
            if let Some((version, _)) = self.doc_store.get::<FileVersionDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if version.id == version_id {
                    return Ok(Some(version));
                }
            }
        }
        
        Ok(None)
    }
    
    async fn get_by_number(
        &self,
        file_id: FileId,
        version_number: i32,
    ) -> Result<Option<FileVersionDocument>, RepositoryError> {
        let prefix = format!(
            "{}/{}/meta/file_versions/{}/",
            self.path_builder.base_prefix, self.path_builder.namespace, file_id
        );
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        for key in keys {
            if let Some((version, _)) = self.doc_store.get::<FileVersionDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if version.version_number == version_number {
                    return Ok(Some(version));
                }
            }
        }
        
        Ok(None)
    }
    
    async fn create(&self, version: &FileVersionDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.file_version(version.file_id, version.id);
        
        self.doc_store.put(&key, version, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn list_by_file(&self, file_id: FileId) -> Result<Vec<FileVersionDocument>, RepositoryError> {
        let prefix = format!(
            "{}/{}/meta/file_versions/{}/",
            self.path_builder.base_prefix, self.path_builder.namespace, file_id
        );
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut versions = Vec::new();
        
        for key in keys {
            if let Some((version, _)) = self.doc_store.get::<FileVersionDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                versions.push(version);
            }
        }
        
        // Sort by version number descending
        versions.sort_by(|a, b| b.version_number.cmp(&a.version_number));
        
        Ok(versions)
    }
    
    async fn get_latest(&self, file_id: FileId) -> Result<Option<FileVersionDocument>, RepositoryError> {
        let versions = self.list_by_file(file_id).await?;
        Ok(versions.into_iter().next())
    }
}

/// RustFS-backed share repository
pub struct RustFsShareRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
    cache: Option<Arc<RuntimeMetadataCache>>,
}

impl RustFsShareRepository {
    pub fn new(
        doc_store: Arc<dyn MetadataDocumentStore>,
        path_builder: PathBuilder,
        cache: Option<Arc<RuntimeMetadataCache>>,
    ) -> Self {
        Self {
            doc_store,
            path_builder,
            cache,
        }
    }
}

#[async_trait]
impl ShareRepository for RustFsShareRepository {
    async fn get(&self, id: ShareId) -> Result<Option<ShareDocument>, RepositoryError> {
        // Check cache first
        if let Some(cache) = &self.cache {
            if let Some(share) = cache.get_share(id) {
                return Ok(Some(share));
            }
        }
        
        let key = self.path_builder.share(id);
        let result = self.doc_store.get::<ShareDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some((ref share, _)) = result {
            if let Some(ref cache) = self.cache {
                cache.put_share(share.clone());
            }
        }
        
        Ok(result.map(|(doc, _)| doc))
    }
    
    async fn get_by_token(&self, token_hash: &str) -> Result<Option<ShareDocument>, RepositoryError> {
        // This requires scanning - use an index in production
        let prefix = format!("{}/{}/meta/shares/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        for key in keys {
            if let Some((share, _)) = self.doc_store.get::<ShareDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if share.token_hash.as_deref() == Some(token_hash) && share.is_active() {
                    return Ok(Some(share));
                }
            }
        }
        
        Ok(None)
    }
    
    async fn create(&self, share: &ShareDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.share(share.id);
        
        self.doc_store.put(&key, share, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.on_share_created(share);
        }
        
        Ok(())
    }
    
    async fn update(&self, share: &ShareDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.share(share.id);
        
        self.doc_store.put(&key, share, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.put_share(share.clone());
        }
        
        Ok(())
    }
    
    async fn revoke(&self, id: ShareId, _revoked_by: UserId) -> Result<(), RepositoryError> {
        let mut share = self.get_required(id).await?;
        share.revoke();
        
        self.update(&share).await?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.on_share_revoked(&share);
        }
        
        Ok(())
    }
    
    async fn delete(&self, id: ShareId) -> Result<(), RepositoryError> {
        let key = self.path_builder.share(id);
        
        self.doc_store.delete(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Invalidate cache
        if let Some(ref cache) = self.cache {
            cache.invalidate_share(id);
        }
        
        Ok(())
    }
    
    async fn list_by_resource(
        &self,
        resource_type: &str,
        resource_id: Uuid,
    ) -> Result<Vec<ShareDocument>, RepositoryError> {
        let prefix = format!("{}/{}/meta/shares/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut shares = Vec::new();
        
        for key in keys {
            if let Some((share, _)) = self.doc_store.get::<ShareDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if share.resource_type == resource_type 
                    && share.resource_id == resource_id
                    && share.is_active() {
                    shares.push(share);
                }
            }
        }
        
        Ok(shares)
    }
    
    async fn list_by_creator(&self, user_id: UserId) -> Result<Vec<ShareDocument>, RepositoryError> {
        let prefix = format!("{}/{}/meta/shares/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut shares = Vec::new();
        
        for key in keys {
            if let Some((share, _)) = self.doc_store.get::<ShareDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if share.created_by == user_id {
                    shares.push(share);
                }
            }
        }
        
        Ok(shares)
    }
    
    async fn list_by_recipient(&self, user_id: UserId) -> Result<Vec<ShareDocument>, RepositoryError> {
        let prefix = format!("{}/{}/meta/shares/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut shares = Vec::new();
        
        for key in keys {
            if let Some((share, _)) = self.doc_store.get::<ShareDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if share.recipient_user_id == Some(user_id) && share.is_active() {
                    shares.push(share);
                }
            }
        }
        
        Ok(shares)
    }
    
    async fn increment_access_count(&self, id: ShareId) -> Result<(), RepositoryError> {
        let mut share = self.get_required(id).await?;
        share.access_count += 1;
        self.update(&share).await
    }
}

/// RustFS-backed event repository
pub struct RustFsEventRepository {
    event_store: Arc<dyn EventLogStore>,
}

impl RustFsEventRepository {
    pub fn new(event_store: Arc<dyn EventLogStore>) -> Self {
        Self { event_store }
    }
}

#[async_trait]
impl EventRepository for RustFsEventRepository {
    async fn append(&self, event: &EventDocument) -> Result<(), RepositoryError> {
        self.event_store.append(event).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
    }
    
    async fn read_for_resource(
        &self,
        resource_type: &str,
        resource_id: Uuid,
        limit: usize,
    ) -> Result<Vec<EventDocument>, RepositoryError> {
        self.event_store.read_for_resource(resource_type, &resource_id.to_string(), limit).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
    }
    
    async fn read_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<Vec<EventDocument>, RepositoryError> {
        self.event_store.read_range(start, end, limit).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
    }
}

/// RustFS-backed folder children index repository
pub struct RustFsFolderChildrenIndexRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
    cache: Option<Arc<RuntimeMetadataCache>>,
}

impl RustFsFolderChildrenIndexRepository {
    pub fn new(
        doc_store: Arc<dyn MetadataDocumentStore>,
        path_builder: PathBuilder,
        cache: Option<Arc<RuntimeMetadataCache>>,
    ) -> Self {
        Self {
            doc_store,
            path_builder,
            cache,
        }
    }
}

#[async_trait]
impl FolderChildrenIndexRepository for RustFsFolderChildrenIndexRepository {
    async fn get(&self, folder_id: FolderId) -> Result<Option<FolderChildrenIndex>, RepositoryError> {
        // Check cache first
        if let Some(cache) = &self.cache {
            if let Some(children) = cache.get_folder_children(folder_id) {
                return Ok(Some(children));
            }
        }
        
        let key = self.path_builder.folder_children_index(folder_id);
        let result = self.doc_store.get::<FolderChildrenIndex>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some((ref index, _)) = result {
            if let Some(ref cache) = self.cache {
                cache.put_folder_children(index.clone());
            }
        }
        
        Ok(result.map(|(doc, _)| doc))
    }
    
    async fn save(&self, index: &FolderChildrenIndex) -> Result<(), RepositoryError> {
        let key = self.path_builder.folder_children_index(index.folder_id);
        
        self.doc_store.put(&key, index, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.put_folder_children(index.clone());
        }
        
        Ok(())
    }
    
    async fn rebuild(&self, folder_id: FolderId) -> Result<FolderChildrenIndex, RepositoryError> {
        // This would need access to the file/folder repos to rebuild
        // For now, return an empty index
        Ok(FolderChildrenIndex::new(folder_id))
    }
}

/// RustFS-backed tombstone repository
pub struct RustFsTombstoneRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsTombstoneRepository {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, path_builder: PathBuilder) -> Self {
        Self {
            doc_store,
            path_builder,
        }
    }
}

#[async_trait]
impl TombstoneRepository for RustFsTombstoneRepository {
    async fn get(&self, resource_type: &str, resource_id: Uuid) -> Result<Option<TombstoneDocument>, RepositoryError> {
        let key = self.path_builder.tombstone(resource_type, resource_id);
        
        self.doc_store.get::<TombstoneDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
            .map(|opt| opt.map(|(doc, _)| doc))
    }
    
    async fn create(&self, tombstone: &TombstoneDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.tombstone(&tombstone.resource_type, tombstone.resource_id);
        
        self.doc_store.put(&key, tombstone, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn list_by_user(&self, user_id: UserId) -> Result<Vec<TombstoneDocument>, RepositoryError> {
        let prefix = format!("{}/{}/meta/tombstones/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut tombstones = Vec::new();
        
        for key in keys {
            if let Some((tombstone, _)) = self.doc_store.get::<TombstoneDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if tombstone.deleted_by == user_id {
                    tombstones.push(tombstone);
                }
            }
        }
        
        Ok(tombstones)
    }
    
    async fn delete(&self, resource_type: &str, resource_id: Uuid) -> Result<(), RepositoryError> {
        let key = self.path_builder.tombstone(resource_type, resource_id);
        
        self.doc_store.delete(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
}


// ============================================================================
// Additional Repository Implementations
// ============================================================================

use crate::metadata_v2::schemas::{
    AuditLogEntryDocument, EmailLookupDocument, GroupMembersIndex, PairingRequestDocument,
    PairingStatus, SystemConfigDocument, TokenLookupDocument, UserDevicesIndex,
    UserDocument, UserGroupDocument, UserGroupsIndex, WebhookDocument, ConfigType,
};

/// RustFS-backed user metadata repository (document-based)
pub struct RustFsUserMetadataRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsUserMetadataRepository {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, path_builder: PathBuilder) -> Self {
        Self {
            doc_store,
            path_builder,
        }
    }
}

#[async_trait]
impl UserMetadataRepository for RustFsUserMetadataRepository {
    async fn get(&self, id: UserId) -> Result<Option<UserDocument>, RepositoryError> {
        let key = self.path_builder.user(id);
        self.doc_store.get::<UserDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
            .map(|opt| opt.map(|(doc, _)| doc))
    }
    
    async fn get_by_email(&self, email: &str) -> Result<Option<UserDocument>, RepositoryError> {
        // First lookup the email
        let email_hash = EmailLookupDocument::hash_email(email);
        let lookup_key = self.path_builder.email_lookup(&email_hash);
        
        let lookup = self.doc_store.get::<EmailLookupDocument>(&lookup_key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?
            .map(|(doc, _)| doc);
        
        if let Some(lookup) = lookup {
            self.get(lookup.user_id).await
        } else {
            Ok(None)
        }
    }
    
    async fn create(&self, user: &UserDocument) -> Result<(), RepositoryError> {
        // Create user document
        let key = self.path_builder.user(user.id);
        let opts = PutOptions {
            if_none_match: Some("*".to_string()),
            ..Default::default()
        };
        
        self.doc_store.put(&key, user, opts).await
            .map_err(|e| {
                if e.to_string().contains("Precondition") {
                    RepositoryError::AlreadyExists(format!("User {} already exists", user.id))
                } else {
                    RepositoryError::StorageError(e.to_string())
                }
            })?;
        
        // Create email lookup
        let email_hash = EmailLookupDocument::hash_email(&user.email);
        let lookup = EmailLookupDocument::new(user.email.clone(), user.id);
        let lookup_key = self.path_builder.email_lookup(&email_hash);
        
        self.doc_store.put(&lookup_key, &lookup, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn update(&self, user: &UserDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.user(user.id);
        
        self.doc_store.put(&key, user, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn delete(&self, id: UserId) -> Result<(), RepositoryError> {
        // Get user first to clean up lookup
        if let Some(user) = self.get(id).await? {
            let email_hash = EmailLookupDocument::hash_email(&user.email);
            let lookup_key = self.path_builder.email_lookup(&email_hash);
            
            // Delete email lookup
            let _ = self.doc_store.delete(&lookup_key).await;
        }
        
        // Delete user document
        let key = self.path_builder.user(id);
        self.doc_store.delete(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn list(&self, _filter: UserFilter) -> Result<Vec<UserDocument>, RepositoryError> {
        // Scan all users - in production, use an index
        let prefix = format!("{}/{}/meta/users/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut users = Vec::new();
        
        for key in keys {
            if let Some((user, _)) = self.doc_store.get::<UserDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                users.push(user);
            }
        }
        
        Ok(users)
    }
}

/// RustFS-backed device repository
pub struct RustFsDeviceRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsDeviceRepository {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, path_builder: PathBuilder) -> Self {
        Self {
            doc_store,
            path_builder,
        }
    }
}

#[async_trait]
impl DeviceRepository for RustFsDeviceRepository {
    async fn get(&self, id: Uuid) -> Result<Option<DeviceTokenDocument>, RepositoryError> {
        let key = self.path_builder.device(id);
        self.doc_store.get::<DeviceTokenDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
            .map(|opt| opt.map(|(doc, _)| doc))
    }
    
    async fn get_by_token_hash(&self, token_hash: &str) -> Result<Option<DeviceTokenDocument>, RepositoryError> {
        // Scan for matching token hash - use index in production
        let prefix = format!("{}/{}/meta/devices/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        for key in keys {
            if let Some((device, _)) = self.doc_store.get::<DeviceTokenDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if device.token_hash == token_hash {
                    return Ok(Some(device));
                }
            }
        }
        
        Ok(None)
    }
    
    async fn create(&self, device: &DeviceTokenDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.device(device.id);
        
        self.doc_store.put(&key, device, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update user devices index
        let index_key = self.path_builder.user_devices_index(device.user_id);
        let mut index = self.doc_store.get::<UserDevicesIndex>(&index_key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?
            .map(|(doc, _)| doc)
            .unwrap_or_else(|| UserDevicesIndex::new(device.user_id));
        
        index.add_device(device.id);
        
        self.doc_store.put(&index_key, &index, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn update(&self, device: &DeviceTokenDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.device(device.id);
        
        self.doc_store.put(&key, device, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        // Get device first to update index
        if let Some(device) = self.get(id).await? {
            let index_key = self.path_builder.user_devices_index(device.user_id);
            
            if let Some((mut index, _)) = self.doc_store.get::<UserDevicesIndex>(&index_key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                index.remove_device(id);
                
                self.doc_store.put(&index_key, &index, PutOptions::default()).await
                    .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
            }
        }
        
        let key = self.path_builder.device(id);
        self.doc_store.delete(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn list_by_user(&self, user_id: UserId) -> Result<Vec<DeviceTokenDocument>, RepositoryError> {
        // Use index if available
        let index_key = self.path_builder.user_devices_index(user_id);
        
        if let Some((index, _)) = self.doc_store.get::<UserDevicesIndex>(&index_key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
            
            let mut devices = Vec::new();
            for device_id in index.device_ids {
                if let Some(device) = self.get(device_id).await? {
                    devices.push(device);
                }
            }
            return Ok(devices);
        }
        
        // Fall back to scanning
        let prefix = format!("{}/{}/meta/devices/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut devices = Vec::new();
        
        for key in keys {
            if let Some((device, _)) = self.doc_store.get::<DeviceTokenDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if device.user_id == user_id {
                    devices.push(device);
                }
            }
        }
        
        Ok(devices)
    }
}

/// RustFS-backed group repository
pub struct RustFsGroupRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsGroupRepository {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, path_builder: PathBuilder) -> Self {
        Self {
            doc_store,
            path_builder,
        }
    }
}

#[async_trait]
impl GroupRepository for RustFsGroupRepository {
    async fn get(&self, id: Uuid) -> Result<Option<UserGroupDocument>, RepositoryError> {
        let key = self.path_builder.group(id);
        self.doc_store.get::<UserGroupDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
            .map(|opt| opt.map(|(doc, _)| doc))
    }
    
    async fn create(&self, group: &UserGroupDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.group(group.id);
        let opts = PutOptions {
            if_none_match: Some("*".to_string()),
            ..Default::default()
        };
        
        self.doc_store.put(&key, group, opts).await
            .map_err(|e| {
                if e.to_string().contains("Precondition") {
                    RepositoryError::AlreadyExists(format!("Group {} already exists", group.id))
                } else {
                    RepositoryError::StorageError(e.to_string())
                }
            })?;
        
        // Create empty members index
        let index = GroupMembersIndex::new(group.id);
        let index_key = self.path_builder.group_members_index(group.id);
        
        self.doc_store.put(&index_key, &index, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn update(&self, group: &UserGroupDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.group(group.id);
        
        self.doc_store.put(&key, group, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        // Clean up memberships
        if let Some(group) = self.get(id).await? {
            for member_id in &group.member_ids {
                let user_index_key = self.path_builder.user_groups_index(*member_id);
                
                if let Some((mut user_index, _)) = self.doc_store.get::<UserGroupsIndex>(&user_index_key).await
                    .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                    user_index.remove_group(id);
                    
                    self.doc_store.put(&user_index_key, &user_index, PutOptions::default()).await
                        .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
                }
            }
        }
        
        // Delete members index
        let index_key = self.path_builder.group_members_index(id);
        let _ = self.doc_store.delete(&index_key).await;
        
        // Delete group
        let key = self.path_builder.group(id);
        self.doc_store.delete(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn list(&self) -> Result<Vec<UserGroupDocument>, RepositoryError> {
        let prefix = format!("{}/{}/meta/groups/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut groups = Vec::new();
        
        for key in keys {
            if let Some((group, _)) = self.doc_store.get::<UserGroupDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                groups.push(group);
            }
        }
        
        Ok(groups)
    }
    
    async fn add_member(&self, group_id: Uuid, user_id: UserId, _added_by: UserId) -> Result<(), RepositoryError> {
        // Update group document
        let mut group = self.get(group_id).await?
            .ok_or_else(|| RepositoryError::NotFound(format!("Group {} not found", group_id)))?;
        
        group.add_member(user_id);
        self.update(&group).await?;
        
        // Update group members index
        let index_key = self.path_builder.group_members_index(group_id);
        let mut index = self.doc_store.get::<GroupMembersIndex>(&index_key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?
            .map(|(doc, _)| doc)
            .unwrap_or_else(|| GroupMembersIndex::new(group_id));
        
        index.add_member(user_id);
        
        self.doc_store.put(&index_key, &index, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update user groups index
        let user_index_key = self.path_builder.user_groups_index(user_id);
        let mut user_index = self.doc_store.get::<UserGroupsIndex>(&user_index_key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?
            .map(|(doc, _)| doc)
            .unwrap_or_else(|| UserGroupsIndex::new(user_id));
        
        user_index.add_group(group_id);
        
        self.doc_store.put(&user_index_key, &user_index, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn remove_member(&self, group_id: Uuid, user_id: UserId) -> Result<(), RepositoryError> {
        // Update group document
        let mut group = self.get(group_id).await?
            .ok_or_else(|| RepositoryError::NotFound(format!("Group {} not found", group_id)))?;
        
        group.remove_member(user_id);
        self.update(&group).await?;
        
        // Update group members index
        let index_key = self.path_builder.group_members_index(group_id);
        
        if let Some((mut index, _)) = self.doc_store.get::<GroupMembersIndex>(&index_key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
            index.remove_member(user_id);
            
            self.doc_store.put(&index_key, &index, PutOptions::default()).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        }
        
        // Update user groups index
        let user_index_key = self.path_builder.user_groups_index(user_id);
        
        if let Some((mut user_index, _)) = self.doc_store.get::<UserGroupsIndex>(&user_index_key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
            user_index.remove_group(group_id);
            
            self.doc_store.put(&user_index_key, &user_index, PutOptions::default()).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        }
        
        Ok(())
    }
    
    async fn list_members(&self, group_id: Uuid) -> Result<Vec<Uuid>, RepositoryError> {
        // Use index
        let index_key = self.path_builder.group_members_index(group_id);
        
        if let Some((index, _)) = self.doc_store.get::<GroupMembersIndex>(&index_key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
            return Ok(index.member_ids);
        }
        
        // Fall back to group document
        if let Some(group) = self.get(group_id).await? {
            return Ok(group.member_ids);
        }
        
        Ok(Vec::new())
    }
    
    async fn list_by_user(&self, user_id: UserId) -> Result<Vec<UserGroupDocument>, RepositoryError> {
        // Use index
        let index_key = self.path_builder.user_groups_index(user_id);
        
        if let Some((index, _)) = self.doc_store.get::<UserGroupsIndex>(&index_key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
            
            let mut groups = Vec::new();
            for membership in &index.groups {
                if let Some(group) = self.get(membership.group_id).await? {
                    groups.push(group);
                }
            }
            return Ok(groups);
        }
        
        // Fall back to scanning
        let all_groups = self.list().await?;
        let groups: Vec<_> = all_groups.into_iter()
            .filter(|g| g.is_member(user_id))
            .collect();
        
        Ok(groups)
    }
}

/// RustFS-backed audit repository
pub struct RustFsAuditRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsAuditRepository {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, path_builder: PathBuilder) -> Self {
        Self {
            doc_store,
            path_builder,
        }
    }
}

#[async_trait]
impl AuditRepository for RustFsAuditRepository {
    async fn append(&self, entry: &AuditLogEntryDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.audit_entry(entry.occurred_at, entry.id);
        
        self.doc_store.put(&key, entry, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn list(&self, filter: AuditFilter) -> Result<Vec<AuditLogEntryDocument>, RepositoryError> {
        let prefix = format!("{}/{}/audit/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut entries = Vec::new();
        
        for key in keys {
            if let Some((entry, _)) = self.doc_store.get::<AuditLogEntryDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                // Apply filters
                if let Some(ref actor_id) = filter.actor_id {
                    if entry.actor_id != *actor_id {
                        continue;
                    }
                }
                
                if let Some(ref action_type) = filter.action_type {
                    if !entry.action_type.contains(action_type) {
                        continue;
                    }
                }
                
                if let Some(ref from) = filter.from {
                    if entry.occurred_at < *from {
                        continue;
                    }
                }
                
                if let Some(ref to) = filter.to {
                    if entry.occurred_at > *to {
                        continue;
                    }
                }
                
                entries.push(entry);
            }
        }
        
        // Sort by occurred_at desc
        entries.sort_by(|a, b| b.occurred_at.cmp(&a.occurred_at));
        
        // Apply pagination
        let offset = filter.offset as usize;
        let limit = filter.limit as usize;
        
        if offset < entries.len() {
            entries = entries.into_iter().skip(offset).take(limit).collect();
        } else {
            entries.clear();
        }
        
        Ok(entries)
    }
    
    async fn count(&self, filter: AuditFilter) -> Result<i64, RepositoryError> {
        let entries = self.list(filter).await?;
        Ok(entries.len() as i64)
    }
}

/// RustFS-backed config repository
pub struct RustFsConfigRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsConfigRepository {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, path_builder: PathBuilder) -> Self {
        Self {
            doc_store,
            path_builder,
        }
    }
}

#[async_trait]
impl ConfigRepository for RustFsConfigRepository {
    async fn get(&self, config_type: ConfigType) -> Result<Option<SystemConfigDocument>, RepositoryError> {
        let key = self.path_builder.config(&format!("{:?}", config_type).to_lowercase());
        self.doc_store.get::<SystemConfigDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
            .map(|opt| opt.map(|(doc, _)| doc))
    }
    
    async fn get_oidc(&self) -> Result<Option<SystemConfigDocument>, RepositoryError> {
        self.get(ConfigType::Oidc).await
    }
    
    async fn get_smtp(&self) -> Result<Option<SystemConfigDocument>, RepositoryError> {
        self.get(ConfigType::Smtp).await
    }
    
    async fn set(&self, config: &SystemConfigDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.config(&format!("{:?}", config.config_type).to_lowercase());
        
        self.doc_store.put(&key, config, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
}

/// RustFS-backed pairing repository
pub struct RustFsPairingRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsPairingRepository {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, path_builder: PathBuilder) -> Self {
        Self {
            doc_store,
            path_builder,
        }
    }
}

#[async_trait]
impl PairingRepository for RustFsPairingRepository {
    async fn get(&self, id: Uuid) -> Result<Option<PairingRequestDocument>, RepositoryError> {
        let key = self.path_builder.pairing(id);
        self.doc_store.get::<PairingRequestDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
            .map(|opt| opt.map(|(doc, _)| doc))
    }
    
    async fn get_by_user_code(&self, user_code: &str) -> Result<Option<PairingRequestDocument>, RepositoryError> {
        // Use lookup
        let lookup_key = self.path_builder.pairing_code_lookup(user_code);
        
        if let Some((lookup, _)) = self.doc_store.get::<TokenLookupDocument>(&lookup_key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
            return self.get(lookup.resource_id).await;
        }
        
        // Fall back to scanning
        let prefix = format!("{}/{}/meta/pairings/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        for key in keys {
            if let Some((pairing, _)) = self.doc_store.get::<PairingRequestDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if pairing.user_code == user_code {
                    return Ok(Some(pairing));
                }
            }
        }
        
        Ok(None)
    }
    
    async fn get_by_device_code(&self, device_code: &str) -> Result<Option<PairingRequestDocument>, RepositoryError> {
        // Scan for matching device code
        let prefix = format!("{}/{}/meta/pairings/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        for key in keys {
            if let Some((pairing, _)) = self.doc_store.get::<PairingRequestDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if pairing.device_code == device_code {
                    return Ok(Some(pairing));
                }
            }
        }
        
        Ok(None)
    }
    
    async fn create(&self, pairing: &PairingRequestDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.pairing(pairing.id);
        let opts = PutOptions {
            if_none_match: Some("*".to_string()),
            ..Default::default()
        };
        
        self.doc_store.put(&key, pairing, opts).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Create user_code lookup
        let lookup = TokenLookupDocument::new(
            pairing.user_code.clone(),
            "pairing".to_string(),
            pairing.id,
            Some(pairing.expires_at),
        );
        let lookup_key = self.path_builder.pairing_code_lookup(&pairing.user_code);
        
        self.doc_store.put(&lookup_key, &lookup, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn update(&self, pairing: &PairingRequestDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.pairing(pairing.id);
        
        self.doc_store.put(&key, pairing, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        // Clean up lookup
        if let Some(pairing) = self.get(id).await? {
            let lookup_key = self.path_builder.pairing_code_lookup(&pairing.user_code);
            let _ = self.doc_store.delete(&lookup_key).await;
        }
        
        let key = self.path_builder.pairing(id);
        self.doc_store.delete(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn cleanup_expired(&self) -> Result<u64, RepositoryError> {
        let prefix = format!("{}/{}/meta/pairings/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut cleaned = 0u64;
        
        for key in keys {
            if let Some((pairing, _)) = self.doc_store.get::<PairingRequestDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                if pairing.is_expired() && pairing.status != PairingStatus::Expired {
                    let mut expired = pairing.clone();
                    expired.expire();
                    self.update(&expired).await?;
                    cleaned += 1;
                }
            }
        }
        
        Ok(cleaned)
    }
}

/// RustFS-backed webhook repository
pub struct RustFsWebhookRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsWebhookRepository {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, path_builder: PathBuilder) -> Self {
        Self {
            doc_store,
            path_builder,
        }
    }
}

#[async_trait]
impl WebhookRepository for RustFsWebhookRepository {
    async fn get(&self, id: Uuid) -> Result<Option<WebhookDocument>, RepositoryError> {
        let key = self.path_builder.webhook(id);
        self.doc_store.get::<WebhookDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
            .map(|opt| opt.map(|(doc, _)| doc))
    }
    
    async fn create(&self, webhook: &WebhookDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.webhook(webhook.id);
        
        self.doc_store.put(&key, webhook, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn update(&self, webhook: &WebhookDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.webhook(webhook.id);
        
        self.doc_store.put(&key, webhook, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        let key = self.path_builder.webhook(id);
        self.doc_store.delete(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn list(&self, filter: WebhookFilter) -> Result<Vec<WebhookDocument>, RepositoryError> {
        let prefix = format!("{}/{}/meta/webhooks/", self.path_builder.base_prefix, self.path_builder.namespace);
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut webhooks = Vec::new();
        
        for key in keys {
            if let Some((webhook, _)) = self.doc_store.get::<WebhookDocument>(&key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                // Apply filters
                if filter.enabled_only && !webhook.enabled {
                    continue;
                }
                
                if let Some(ref event_type) = filter.event_type {
                    if !webhook.events.contains(event_type) {
                        continue;
                    }
                }
                
                webhooks.push(webhook);
            }
        }
        
        Ok(webhooks)
    }
}

/// RustFS-backed notification repository
pub struct RustFsNotificationRepository {
    doc_store: Arc<dyn MetadataDocumentStore>,
    path_builder: PathBuilder,
}

impl RustFsNotificationRepository {
    pub fn new(doc_store: Arc<dyn MetadataDocumentStore>, path_builder: PathBuilder) -> Self {
        Self {
            doc_store,
            path_builder,
        }
    }
}

#[async_trait]
impl NotificationRepository for RustFsNotificationRepository {
    async fn create(&self, notification: &NotificationDocument) -> Result<(), RepositoryError> {
        // Store the notification document
        let key = format!("{}/{}/meta/notifications/{}/{}.json",
            self.path_builder.base_prefix,
            self.path_builder.namespace,
            notification.user_id,
            notification.id
        );
        
        self.doc_store.put(&key, notification, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update the user's notification index
        let mut index = self.get_index(notification.user_id).await?;
        
        let notif_ref = NotificationRef {
            notification_id: notification.id,
            notification_type: notification.notification_type,
            resource_type: notification.resource_type.clone(),
            resource_id: notification.resource_id,
            read: notification.read,
            created_at: notification.created_at,
        };
        
        index.add_notification(&notif_ref);
        self.save_index(&index).await
    }
    
    async fn get(&self, user_id: UserId, notification_id: Uuid) -> Result<Option<NotificationDocument>, RepositoryError> {
        let key = format!("{}/{}/meta/notifications/{}/{}.json",
            self.path_builder.base_prefix,
            self.path_builder.namespace,
            user_id,
            notification_id
        );
        
        self.doc_store.get::<NotificationDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
            .map(|opt| opt.map(|(doc, _)| doc))
    }
    
    async fn get_index(&self, user_id: UserId) -> Result<UserNotificationIndex, RepositoryError> {
        let key = self.path_builder.user_notifications_index(user_id);
        
        self.doc_store.get::<UserNotificationIndex>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
            .map(|opt| opt.map(|(doc, _)| doc).unwrap_or_else(|| UserNotificationIndex::new(user_id)))
    }
    
    async fn save_index(&self, index: &UserNotificationIndex) -> Result<(), RepositoryError> {
        let key = self.path_builder.user_notifications_index(index.user_id);
        
        self.doc_store.put(&key, index, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn list(&self, user_id: UserId, unread_only: bool, offset: usize, limit: usize) -> Result<Vec<NotificationRef>, RepositoryError> {
        let index = self.get_index(user_id).await?;
        
        let mut notifications: Vec<_> = if unread_only {
            index.notifications.into_iter().filter(|n| !n.read).collect()
        } else {
            index.notifications
        };
        
        // Sort by created_at desc
        notifications.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
        // Apply pagination
        let paginated: Vec<_> = notifications.into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        
        Ok(paginated)
    }
    
    async fn count_unread(&self, user_id: UserId) -> Result<u32, RepositoryError> {
        let index = self.get_index(user_id).await?;
        Ok(index.unread_count)
    }
    
    async fn mark_read(&self, user_id: UserId, notification_id: Uuid) -> Result<(), RepositoryError> {
        let mut index = self.get_index(user_id).await?;
        index.mark_read(notification_id);
        self.save_index(&index).await
    }
    
    async fn delete(&self, user_id: UserId, notification_id: Uuid) -> Result<(), RepositoryError> {
        let mut index = self.get_index(user_id).await?;
        index.remove_notification(notification_id);
        self.save_index(&index).await
    }
}
