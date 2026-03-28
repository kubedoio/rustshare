//! Folder Service V2 Implementation

use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use chrono::Utc;
use uuid::Uuid;

use crate::user_bucket::{UserBucketStore, UserId};

use rustshare_core::domain::Folder;
use rustshare_core::services::FolderError;

use super::indexes::UserBucketIndexes;
use super::models::*;
use super::paths::UserBucketPaths;

/// Folder service using per-user bucket storage
pub struct FolderServiceV2 {
    user_buckets: Arc<dyn UserBucketStore>,
    indexes: Arc<UserBucketIndexes>,
}

impl FolderServiceV2 {
    /// Create a new folder service
    pub fn new(
        user_buckets: Arc<dyn UserBucketStore>,
        indexes: Arc<UserBucketIndexes>,
    ) -> Self {
        Self {
            user_buckets,
            indexes,
        }
    }

    /// Create a new folder
    pub async fn create_folder(
        &self,
        owner_id: UserId,
        name: String,
        parent_id: Option<Uuid>,
    ) -> Result<Folder, FolderError> {
        // Validate folder name
        Self::validate_folder_name(&name)?;

        // Compute path
        let path = if let Some(folder_id) = parent_id {
            let parent_doc = self.get_folder_doc(owner_id, folder_id).await?;
            if parent_doc.path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", parent_doc.path, name)
            }
        } else {
            format!("/{}", name)
        };

        // Check for duplicate names
        if let Some(parent_id) = parent_id {
            if self.indexes.folder_children.name_exists(owner_id, parent_id, &name, None).await
                .map_err(|e| FolderError::Storage(e.to_string()))? {
                return Err(FolderError::DuplicateName(name));
            }
        } else {
            // Check root level
            let root_folders = self.indexes.user_roots.list_root_folders(owner_id).await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
            for folder_id in root_folders {
                if let Ok(existing) = self.get_folder_doc(owner_id, folder_id).await {
                    if existing.name == name {
                        return Err(FolderError::DuplicateName(name));
                    }
                }
            }
        }

        // Create folder document
        let folder_id = Uuid::new_v4();

        let folder_doc = FolderDocV2 {
            schema_version: SCHEMA_VERSION,
            id: folder_id,
            owner_id,
            parent_folder_id: parent_id,
            name: name.clone(),
            path: path.clone(),
            deleted: false,
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Store document
        let paths = UserBucketPaths::new(owner_id);

        self.user_buckets
            .put_object(
                owner_id,
                &paths.folder(folder_id),
                Bytes::from(serde_json::to_vec(&folder_doc).unwrap()),
            )
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?;

        // Update indexes
        if let Some(parent_id) = parent_id {
            self.indexes
                .folder_children
                .add_folder(owner_id, parent_id, &folder_doc)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
        } else {
            // Update user roots index
            self.indexes
                .user_roots
                .add_root_folder(owner_id, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
        }

        // Create empty children index for the new folder
        let children_index = FolderChildrenIndex::new(folder_id);
        let data = Bytes::from(serde_json::to_vec(&children_index).unwrap());
        self.user_buckets
            .put_object(owner_id, &paths.folder_children_index(folder_id), data)
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?;

        Ok(folder_doc.to_domain())
    }

    /// Get folder by ID
    pub async fn get_folder(&self, user_id: UserId, folder_id: Uuid) -> Result<Folder, FolderError> {
        let paths = UserBucketPaths::new(user_id);

        let data = self
            .user_buckets
            .get_object(user_id, &paths.folder(folder_id))
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?;

        match data {
            Some(data) => {
                let doc: FolderDocV2 = serde_json::from_slice(&data)
                    .map_err(|e| FolderError::Storage(format!("Invalid folder document: {}", e)))?;

                if doc.deleted {
                    return Err(FolderError::NotFound(folder_id));
                }

                Ok(doc.to_domain())
            }
            None => Err(FolderError::NotFound(folder_id)),
        }
    }

    /// List folders in a parent folder
    pub async fn list_folders(
        &self,
        user_id: UserId,
        parent_id: Option<Uuid>,
    ) -> Result<Vec<Folder>, FolderError> {
        if let Some(folder_id) = parent_id {
            // Use folder children index
            let children = self
                .indexes
                .folder_children
                .list_folders(user_id, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;

            let mut folders = Vec::new();
            for child in children {
                if !child.deleted {
                    if let Ok(folder) = self.get_folder(user_id, child.id).await {
                        folders.push(folder);
                    }
                }
            }
            Ok(folders)
        } else {
            // List root folders using user roots index
            let root_ids = self
                .indexes
                .user_roots
                .list_root_folders(user_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;

            let mut folders = Vec::new();
            for folder_id in root_ids {
                if let Ok(folder) = self.get_folder(user_id, folder_id).await {
                    if !folder.deleted {
                        folders.push(folder);
                    }
                }
            }
            Ok(folders)
        }
    }

    /// Rename a folder
    pub async fn rename_folder(
        &self,
        user_id: UserId,
        folder_id: Uuid,
        new_name: String,
    ) -> Result<Folder, FolderError> {
        Self::validate_folder_name(&new_name)?;

        let paths = UserBucketPaths::new(user_id);

        // Load existing folder
        let data = self
            .user_buckets
            .get_object(user_id, &paths.folder(folder_id))
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?
            .ok_or(FolderError::NotFound(folder_id))?;

        let mut doc: FolderDocV2 = serde_json::from_slice(&data)
            .map_err(|e| FolderError::Storage(format!("Invalid folder document: {}", e)))?;

        if doc.deleted {
            return Err(FolderError::NotFound(folder_id));
        }

        if doc.owner_id != user_id {
            return Err(FolderError::PermissionDenied { folder_id, user_id });
        }

        // Update name and path
        doc.name = new_name.clone();

        // Compute new path
        doc.path = if let Some(parent_id) = doc.parent_folder_id {
            let parent_doc = self.get_folder_doc(user_id, parent_id).await?;
            if parent_doc.path == "/" {
                format!("/{}", new_name)
            } else {
                format!("{}/{}", parent_doc.path, new_name)
            }
        } else {
            format!("/{}", new_name)
        };

        doc.updated_at = Utc::now();

        // Save updated document
        self.user_buckets
            .put_object(
                user_id,
                &paths.folder(folder_id),
                Bytes::from(serde_json::to_vec(&doc).unwrap()),
            )
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?;

        // Update parent folder children index
        if let Some(parent_id) = doc.parent_folder_id {
            self.indexes
                .folder_children
                .remove_child(user_id, parent_id, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
            
            let folder_ref = FolderChildRef {
                id: folder_id,
                name: new_name,
                resource_type: FolderChildType::Folder,
                deleted: false,
            };
            let mut index = self.load_children_index(user_id, parent_id).await?;
            index.folders.push(folder_ref);
            self.save_children_index(user_id, &index).await?;
        }

        // Update child paths recursively
        self.update_child_paths(user_id, folder_id, &doc.path).await?;

        Ok(doc.to_domain())
    }

    /// Move a folder
    pub async fn move_folder(
        &self,
        user_id: UserId,
        folder_id: Uuid,
        target_parent_id: Option<Uuid>,
    ) -> Result<Folder, FolderError> {
        // Prevent moving a folder into itself or its descendants
        if let Some(target_id) = target_parent_id {
            if target_id == folder_id {
                return Err(FolderError::InvalidMove {
                    folder_id,
                    reason: "Cannot move a folder into itself".to_string(),
                });
            }
            if self.is_descendant(user_id, folder_id, target_id).await? {
                return Err(FolderError::InvalidMove {
                    folder_id,
                    reason: "Cannot move a folder into its own descendant".to_string(),
                });
            }
        }

        let paths = UserBucketPaths::new(user_id);

        // Load existing folder
        let data = self
            .user_buckets
            .get_object(user_id, &paths.folder(folder_id))
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?
            .ok_or(FolderError::NotFound(folder_id))?;

        let mut doc: FolderDocV2 = serde_json::from_slice(&data)
            .map_err(|e| FolderError::Storage(format!("Invalid folder document: {}", e)))?;

        if doc.deleted {
            return Err(FolderError::NotFound(folder_id));
        }

        if doc.owner_id != user_id {
            return Err(FolderError::PermissionDenied { folder_id, user_id });
        }

        let old_parent_id = doc.parent_folder_id;

        // Verify target folder exists if specified
        if let Some(target_id) = target_parent_id {
            let _ = self.get_folder_doc(user_id, target_id).await?;
        }

        // Update parent and path
        doc.parent_folder_id = target_parent_id;
        doc.path = if let Some(parent_id) = target_parent_id {
            let parent_doc = self.get_folder_doc(user_id, parent_id).await?;
            if parent_doc.path == "/" {
                format!("/{}", doc.name)
            } else {
                format!("{}/{}", parent_doc.path, doc.name)
            }
        } else {
            format!("/{}", doc.name)
        };

        doc.updated_at = Utc::now();

        // Save updated document
        self.user_buckets
            .put_object(
                user_id,
                &paths.folder(folder_id),
                Bytes::from(serde_json::to_vec(&doc).unwrap()),
            )
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?;

        // Update indexes
        if let Some(old_parent) = old_parent_id {
            self.indexes
                .folder_children
                .remove_child(user_id, old_parent, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
        } else {
            self.indexes
                .user_roots
                .remove_root_folder(user_id, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
        }

        if let Some(new_parent) = target_parent_id {
            self.indexes
                .folder_children
                .add_folder(user_id, new_parent, &doc)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
        } else {
            self.indexes
                .user_roots
                .add_root_folder(user_id, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
        }

        // Update child paths recursively
        self.update_child_paths(user_id, folder_id, &doc.path).await?;

        Ok(doc.to_domain())
    }

    /// Delete a folder (soft delete)
    pub async fn delete_folder(&self, user_id: UserId, folder_id: Uuid) -> Result<(), FolderError> {
        let paths = UserBucketPaths::new(user_id);

        // Load existing folder
        let data = self
            .user_buckets
            .get_object(user_id, &paths.folder(folder_id))
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?
            .ok_or(FolderError::NotFound(folder_id))?;

        let mut doc: FolderDocV2 = serde_json::from_slice(&data)
            .map_err(|e| FolderError::Storage(format!("Invalid folder document: {}", e)))?;

        if doc.deleted {
            return Err(FolderError::NotFound(folder_id));
        }

        if doc.owner_id != user_id {
            return Err(FolderError::PermissionDenied { folder_id, user_id });
        }

        // Check if folder is empty
        let children_index = self.load_children_index(user_id, folder_id).await?;
        if children_index.active_count() > 0 {
            return Err(FolderError::NotEmpty(folder_id));
        }

        // Create tombstone
        let tombstone = TombstoneDocV2::from_folder(&doc, user_id);

        // Mark folder as deleted
        doc.deleted = true;
        doc.updated_at = Utc::now();

        // Save tombstone and updated folder
        self.user_buckets
            .put_object(
                user_id,
                &paths.folder_tombstone(folder_id),
                Bytes::from(serde_json::to_vec(&tombstone).unwrap()),
            )
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?;

        self.user_buckets
            .put_object(
                user_id,
                &paths.folder(folder_id),
                Bytes::from(serde_json::to_vec(&doc).unwrap()),
            )
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?;

        // Update parent folder children index
        if let Some(parent_id) = doc.parent_folder_id {
            self.indexes
                .folder_children
                .mark_deleted(user_id, parent_id, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
        } else {
            self.indexes
                .user_roots
                .remove_root_folder(user_id, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    /// Restore a folder from tombstone
    pub async fn restore_folder(&self, user_id: UserId, folder_id: Uuid) -> Result<Folder, FolderError> {
        let paths = UserBucketPaths::new(user_id);

        // Load tombstone
        let tombstone_data = self
            .user_buckets
            .get_object(user_id, &paths.folder_tombstone(folder_id))
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?
            .ok_or(FolderError::NotFound(folder_id))?;

        let tombstone: TombstoneDocV2 = serde_json::from_slice(&tombstone_data)
            .map_err(|e| FolderError::Storage(format!("Invalid tombstone: {}", e)))?;

        // Restore original document
        let mut doc: FolderDocV2 = serde_json::from_value(tombstone.original_doc.clone())
            .map_err(|e| FolderError::Storage(format!("Invalid restore data: {}", e)))?;

        doc.deleted = false;
        doc.updated_at = Utc::now();

        // Check if parent still exists
        if let Some(parent_id) = doc.parent_folder_id {
            match self.get_folder_doc(user_id, parent_id).await {
                Ok(_) => {}
                Err(_) => {
                    // Parent no longer exists, move to root
                    doc.parent_folder_id = None;
                    doc.path = format!("/{}", doc.name);
                }
            }
        }

        // Save restored folder
        self.user_buckets
            .put_object(
                user_id,
                &paths.folder(folder_id),
                Bytes::from(serde_json::to_vec(&doc).unwrap()),
            )
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?;

        // Remove tombstone
        self.user_buckets
            .delete_object(user_id, &paths.folder_tombstone(folder_id))
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?;

        // Update indexes
        if let Some(parent_id) = doc.parent_folder_id {
            self.indexes
                .folder_children
                .unmark_deleted(user_id, parent_id, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
        } else {
            self.indexes
                .user_roots
                .add_root_folder(user_id, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;
        }

        Ok(doc.to_domain())
    }

    /// Get folder contents (files and folders)
    pub async fn get_contents(
        &self,
        user_id: UserId,
        folder_id: Option<Uuid>,
    ) -> Result<(Vec<FolderDocV2>, Vec<FileDocV2>), FolderError> {
        let paths = UserBucketPaths::new(user_id);

        if let Some(folder_id) = folder_id {
            // Get children from index
            let files = self
                .indexes
                .folder_children
                .list_files(user_id, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;

            let folders = self
                .indexes
                .folder_children
                .list_folders(user_id, folder_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;

            // Load folder documents
            let mut folder_docs = Vec::new();
            for child in folders {
                if !child.deleted {
                    if let Some(data) = self
                        .user_buckets
                        .get_object(user_id, &paths.folder(child.id))
                        .await
                        .map_err(|e| FolderError::Storage(e.to_string()))?
                    {
                        if let Ok(doc) = serde_json::from_slice::<FolderDocV2>(&data) {
                            if !doc.deleted {
                                folder_docs.push(doc);
                            }
                        }
                    }
                }
            }

            // Load file documents
            let mut file_docs = Vec::new();
            for child in files {
                if !child.deleted {
                    if let Some(data) = self
                        .user_buckets
                        .get_object(user_id, &format!("owned/files/{}.json", child.id))
                        .await
                        .map_err(|e| FolderError::Storage(e.to_string()))?
                    {
                        if let Ok(doc) = serde_json::from_slice::<FileDocV2>(&data) {
                            if !doc.deleted {
                                file_docs.push(doc);
                            }
                        }
                    }
                }
            }

            Ok((folder_docs, file_docs))
        } else {
            // Get root contents
            let root_folders = self
                .indexes
                .user_roots
                .list_root_folders(user_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;

            let root_files = self
                .indexes
                .user_roots
                .list_root_files(user_id)
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;

            // Load folder documents
            let mut folder_docs = Vec::new();
            for folder_id in root_folders {
                if let Some(data) = self
                    .user_buckets
                    .get_object(user_id, &paths.folder(folder_id))
                    .await
                    .map_err(|e| FolderError::Storage(e.to_string()))?
                {
                    if let Ok(doc) = serde_json::from_slice::<FolderDocV2>(&data) {
                        if !doc.deleted {
                            folder_docs.push(doc);
                        }
                    }
                }
            }

            // Load file documents
            let mut file_docs = Vec::new();
            for file_id in root_files {
                if let Some(data) = self
                    .user_buckets
                    .get_object(user_id, &format!("owned/files/{}.json", file_id))
                    .await
                    .map_err(|e| FolderError::Storage(e.to_string()))?
                {
                    if let Ok(doc) = serde_json::from_slice::<FileDocV2>(&data) {
                        if !doc.deleted {
                            file_docs.push(doc);
                        }
                    }
                }
            }

            Ok((folder_docs, file_docs))
        }
    }

    /// List children (files and folders) for a folder
    /// Convenience method that returns domain types instead of doc types
    pub async fn list_children(
        &self,
        user_id: UserId,
        folder_id: Uuid,
    ) -> Result<(Vec<Folder>, Vec<rustshare_core::domain::File>), FolderError> {
        let (folder_docs, file_docs) = self.get_contents(user_id, Some(folder_id)).await?;
        
        let folders = folder_docs.into_iter()
            .map(|doc| doc.to_domain())
            .collect();
        
        let files = file_docs.into_iter()
            .map(|doc| doc.to_domain())
            .collect();
        
        Ok((folders, files))
    }

    // Helper methods

    fn validate_folder_name(name: &str) -> Result<(), FolderError> {
        if name.is_empty() {
            return Err(FolderError::InvalidName(
                "Folder name cannot be empty".to_string(),
            ));
        }

        if name.contains('/') {
            return Err(FolderError::InvalidName(
                "Folder name cannot contain forward slash (/)".to_string(),
            ));
        }

        if name.contains('\0') {
            return Err(FolderError::InvalidName(
                "Folder name cannot contain null character".to_string(),
            ));
        }

        Ok(())
    }

    async fn get_folder_doc(
        &self,
        user_id: UserId,
        folder_id: Uuid,
    ) -> Result<FolderDocV2, FolderError> {
        let paths = UserBucketPaths::new(user_id);

        let data = self
            .user_buckets
            .get_object(user_id, &paths.folder(folder_id))
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?
            .ok_or(FolderError::NotFound(folder_id))?;

        let doc: FolderDocV2 = serde_json::from_slice(&data)
            .map_err(|e| FolderError::Storage(format!("Invalid folder document: {}", e)))?;

        Ok(doc)
    }

    async fn load_children_index(
        &self,
        user_id: UserId,
        folder_id: Uuid,
    ) -> Result<FolderChildrenIndex, FolderError> {
        let paths = UserBucketPaths::new(user_id);
        
        match self
            .user_buckets
            .get_object(user_id, &paths.folder_children_index(folder_id))
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?
        {
            Some(data) => {
                let index: FolderChildrenIndex = serde_json::from_slice(&data)
                    .map_err(|e| FolderError::Storage(format!("Invalid children index: {}", e)))?;
                Ok(index)
            }
            None => Ok(FolderChildrenIndex::new(folder_id)),
        }
    }

    async fn save_children_index(
        &self,
        user_id: UserId,
        index: &FolderChildrenIndex,
    ) -> Result<(), FolderError> {
        let paths = UserBucketPaths::new(user_id);
        let data = Bytes::from(serde_json::to_vec(index).unwrap());
        self.user_buckets
            .put_object(user_id, &paths.folder_children_index(index.folder_id), data)
            .await
            .map_err(|e| FolderError::Storage(e.to_string()))?;
        Ok(())
    }

    fn update_child_paths<'a>(
        &'a self,
        user_id: UserId,
        folder_id: Uuid,
        new_path: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), FolderError>> + Send + 'a>> {
        Box::pin(async move {
            let paths = UserBucketPaths::new(user_id);
            let children_index = self.load_children_index(user_id, folder_id).await?;

            // Update child folders
            for folder_ref in &children_index.folders {
                if folder_ref.deleted {
                    continue;
                }

                let folder_data = self
                    .user_buckets
                    .get_object(user_id, &paths.folder(folder_ref.id))
                    .await
                    .map_err(|e| FolderError::Storage(e.to_string()))?;

                if let Some(data) = folder_data {
                    let mut child_doc: FolderDocV2 = serde_json::from_slice(&data)
                        .map_err(|e| FolderError::Storage(format!("Invalid folder document: {}", e)))?;

                    child_doc.path = format!("{}/{}", new_path, child_doc.name);
                    child_doc.updated_at = Utc::now();

                    self.user_buckets
                        .put_object(
                            user_id,
                            &paths.folder(folder_ref.id),
                            Bytes::from(serde_json::to_vec(&child_doc).unwrap()),
                        )
                        .await
                        .map_err(|e| FolderError::Storage(e.to_string()))?;

                    // Recursively update this folder's children
                    self.update_child_paths(user_id, folder_ref.id, &child_doc.path).await?;
                }
            }

            // Update child files
            for file_ref in &children_index.files {
                if file_ref.deleted {
                    continue;
                }

            let file_data = self
                .user_buckets
                .get_object(user_id, &format!("owned/files/{}.json", file_ref.id))
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;

            if let Some(data) = file_data {
                let mut child_doc: FileDocV2 = serde_json::from_slice(&data)
                    .map_err(|e| FolderError::Storage(format!("Invalid file document: {}", e)))?;

                child_doc.path = format!("{}/{}", new_path, child_doc.name);
                child_doc.updated_at = Utc::now();

                self.user_buckets
                    .put_object(
                        user_id,
                        &format!("owned/files/{}.json", file_ref.id),
                        Bytes::from(serde_json::to_vec(&child_doc).unwrap()),
                    )
                    .await
                    .map_err(|e| FolderError::Storage(e.to_string()))?;
            }
        }

        Ok(())
        })
    }

    async fn is_descendant(
        &self,
        user_id: UserId,
        potential_ancestor: Uuid,
        potential_descendant: Uuid,
    ) -> Result<bool, FolderError> {
        let mut current = potential_descendant;
        let paths = UserBucketPaths::new(user_id);

        loop {
            let data = self
                .user_buckets
                .get_object(user_id, &paths.folder(current))
                .await
                .map_err(|e| FolderError::Storage(e.to_string()))?;

            if let Some(data) = data {
                let doc: FolderDocV2 = serde_json::from_slice(&data)
                    .map_err(|e| FolderError::Storage(format!("Invalid folder document: {}", e)))?;

                match doc.parent_folder_id {
                    Some(parent_id) => {
                        if parent_id == potential_ancestor {
                            return Ok(true);
                        }
                        current = parent_id;
                    }
                    None => return Ok(false),
                }
            } else {
                return Ok(false);
            }
        }
    }
}

/// Constant from models module
const SCHEMA_VERSION: u32 = 2;
