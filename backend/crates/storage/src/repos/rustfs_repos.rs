//! RustFS-backed repository implementations

use async_trait::async_trait;
use rustshare_core::domain::{FileId, FolderId, ShareId, UserId};
use std::sync::Arc;
use uuid::Uuid;

use super::{RepositoryError, *};
use crate::metadata_v2::{
    EventLogStore, FolderChildrenIndex, MetadataDocumentStore, MetadataDocumentStoreExt,
    PutOptions, RuntimeMetadataCache,
};
use crate::metadata_v2::schemas::*;
use crate::repos::PathBuilder;

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
        
        let key = self.path_builder.folder_path(id);
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
        let key = self.path_builder.folder_path(folder.id);
        
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
        let key = self.path_builder.folder_path(folder.id);
        
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
        let tombstone_key = self.path_builder.tombstone_path("folders", id);
        
        self.doc_store.put(&tombstone_key, &tombstone, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Mark as deleted in place (or we could move it)
        let mut deleted_folder = folder.clone();
        deleted_folder.deleted = true;
        deleted_folder.bump_version();
        
        let folder_key = self.path_builder.folder_path(id);
        self.doc_store.put(&folder_key, &deleted_folder, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.on_folder_deleted(&folder);
        }
        
        Ok(())
    }
    
    async fn hard_delete(&self, id: FolderId) -> Result<(), RepositoryError> {
        let key = self.path_builder.folder_path(id);
        
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
        let prefix = format!("{}/{}/meta/folders/", self.path_builder.base_prefix(), self.path_builder.namespace());
        
        let keys = self.doc_store.list_prefix(&prefix).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        let mut descendants = Vec::new();
        let mut stack = vec![folder_id];
        
        while let Some(current_id) = stack.pop() {
            let key = self.path_builder.folder_path(current_id);
            
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
        let prefix = format!("{}/{}/meta/folders/", self.path_builder.base_prefix(), self.path_builder.namespace());
        
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
            let index_key = self.path_builder.folder_children_index_path(parent_id);
            
            if let Some((index, _)) = self.doc_store.get::<FolderChildrenIndex>(&index_key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                
                return Ok(index.children.iter().any(|c| {
                    c.name == name && c.kind == "folder" && !c.deleted
                }));
            }
        }
        
        // Fall back to scanning
        let prefix = format!("{}/{}/meta/folders/", self.path_builder.base_prefix(), self.path_builder.namespace());
        
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

    async fn batch_update(&self, folders: &[FolderDocument]) -> Result<(), RepositoryError> {
        // Update all folders in parallel for efficiency
        let mut results = Vec::new();
        
        for folder in folders {
            let key = self.path_builder.folder_path(folder.id);
            let result = self.doc_store.put(&key, folder, PutOptions::default()).await;
            results.push((folder.id, result));
            
            // Update cache for each folder
            if let Some(ref cache) = self.cache {
                cache.put_folder(folder.clone());
            }
        }
        
        // Check for any errors
        let errors: Vec<_> = results
            .into_iter()
            .filter_map(|(id, result)| result.err().map(|e| (id, e)))
            .collect();
        
        if !errors.is_empty() {
            return Err(RepositoryError::StorageError(
                format!("Failed to update {} folders: {:?}", errors.len(), errors)
            ));
        }
        
        Ok(())
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
        
        let key = self.path_builder.file_path(id);
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
        let key = self.path_builder.file_path(file.id);
        
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
        let key = self.path_builder.file_path(file.id);
        
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
        let tombstone_key = self.path_builder.tombstone_path("files", id);
        
        self.doc_store.put(&tombstone_key, &tombstone, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Mark as deleted
        let mut deleted_file = file.clone();
        deleted_file.deleted = true;
        deleted_file.bump_version();
        
        let file_key = self.path_builder.file_path(id);
        self.doc_store.put(&file_key, &deleted_file, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.on_file_deleted(id, file.parent_id);
        }
        
        Ok(())
    }
    
    async fn hard_delete(&self, id: FileId) -> Result<(), RepositoryError> {
        let key = self.path_builder.file_path(id);
        
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
            let index_key = self.path_builder.folder_children_index_path(parent_id);
            
            if let Some((index, _)) = self.doc_store.get::<FolderChildrenIndex>(&index_key).await
                .map_err(|e| RepositoryError::StorageError(e.to_string()))? {
                
                return Ok(index.children.iter().any(|c| {
                    c.name == name && c.kind == "file" && !c.deleted
                }));
            }
        }
        
        // Fall back to scanning
        let prefix = format!("{}/{}/meta/files/", self.path_builder.base_prefix(), self.path_builder.namespace());
        
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
        let prefix = format!("{}/{}/meta/file_versions/", self.path_builder.base_prefix(), self.path_builder.namespace());
        
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
            self.path_builder.base_prefix(), self.path_builder.namespace(), file_id
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
        let key = self.path_builder.file_version_path(version.file_id, version.id);
        
        self.doc_store.put(&key, version, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn list_by_file(&self, file_id: FileId) -> Result<Vec<FileVersionDocument>, RepositoryError> {
        let prefix = format!(
            "{}/{}/meta/file_versions/{}/",
            self.path_builder.base_prefix(), self.path_builder.namespace(), file_id
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
        
        let key = self.path_builder.share_path(id);
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
        let prefix = format!("{}/{}/meta/shares/", self.path_builder.base_prefix(), self.path_builder.namespace());
        
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
        let key = self.path_builder.share_path(share.id);
        
        self.doc_store.put(&key, share, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        // Update cache
        if let Some(ref cache) = self.cache {
            cache.on_share_created(share);
        }
        
        Ok(())
    }
    
    async fn update(&self, share: &ShareDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.share_path(share.id);
        
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
        let key = self.path_builder.share_path(id);
        
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
        let prefix = format!("{}/{}/meta/shares/", self.path_builder.base_prefix(), self.path_builder.namespace());
        
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
        let prefix = format!("{}/{}/meta/shares/", self.path_builder.base_prefix(), self.path_builder.namespace());
        
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
        let prefix = format!("{}/{}/meta/shares/", self.path_builder.base_prefix(), self.path_builder.namespace());
        
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
        
        let key = self.path_builder.folder_children_index_path(folder_id);
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
        let key = self.path_builder.folder_children_index_path(index.folder_id);
        
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
        let key = self.path_builder.tombstone_path(resource_type, resource_id);
        
        self.doc_store.get::<TombstoneDocument>(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))
            .map(|opt| opt.map(|(doc, _)| doc))
    }
    
    async fn create(&self, tombstone: &TombstoneDocument) -> Result<(), RepositoryError> {
        let key = self.path_builder.tombstone_path(&tombstone.resource_type, tombstone.resource_id);
        
        self.doc_store.put(&key, tombstone, PutOptions::default()).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn list_by_user(&self, user_id: UserId) -> Result<Vec<TombstoneDocument>, RepositoryError> {
        let prefix = format!("{}/{}/meta/tombstones/", self.path_builder.base_prefix(), self.path_builder.namespace());
        
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
        let key = self.path_builder.tombstone_path(resource_type, resource_id);
        
        self.doc_store.delete(&key).await
            .map_err(|e| RepositoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
}
