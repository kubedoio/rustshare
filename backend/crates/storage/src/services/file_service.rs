//! File Service V2 Implementation

use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::user_bucket::{UserBucketStore, UserId};
use crate::BlobStore;

use rustshare_core::domain::{File, FileVersion};
use rustshare_core::services::FileError;

use super::indexes::UserBucketIndexes;
use super::models::*;
use super::paths::UserBucketPaths;

/// File service using per-user bucket storage
pub struct FileServiceV2 {
    user_buckets: Arc<dyn UserBucketStore>,
    blob_store: Arc<dyn BlobStore>,
    indexes: Arc<UserBucketIndexes>,
}

impl FileServiceV2 {
    /// Create a new file service
    pub fn new(
        user_buckets: Arc<dyn UserBucketStore>,
        blob_store: Arc<dyn BlobStore>,
        indexes: Arc<UserBucketIndexes>,
    ) -> Self {
        Self {
            user_buckets,
            blob_store,
            indexes,
        }
    }

    /// Upload a new file
    pub async fn upload_file(
        &self,
        owner_id: UserId,
        name: String,
        parent_id: Option<Uuid>,
        content: Bytes,
        mime_type: String,
    ) -> Result<File, FileError> {
        // Validate file name
        Self::validate_file_name(&name)?;

        // Calculate content hash
        let content_hash = Self::calculate_sha256(&content);
        let size = content.len() as i64;

        // Compute path
        let path = if let Some(folder_id) = parent_id {
            let folder_doc = self.get_folder_doc(owner_id, folder_id).await?;
            format!("{}/{}", folder_doc.path, name)
        } else {
            format!("/{}", name)
        };

        // Store blob (content-addressed)
        let blob_key = self.blob_store.content_key(&content_hash);
        if !self.blob_store.exists(&blob_key).await.map_err(|e| FileError::Storage(e.to_string()))? {
            self.blob_store
                .put(&blob_key, content)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        }

        // Create file document
        let file_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();

        let file_doc = FileDocV2 {
            schema_version: 2,
            id: file_id,
            owner_id,
            parent_folder_id: parent_id,
            name: name.clone(),
            path: path.clone(),
            current_version_id: version_id,
            version_number: 1,
            size,
            mime_type: mime_type.clone(),
            content_hash: content_hash.clone(),
            deleted: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Create version document
        let version_doc = FileVersionDocV2 {
            schema_version: 2,
            id: version_id,
            file_id,
            version_number: 1,
            size,
            content_hash: content_hash.clone(),
            storage_key: format!("blobs/{}", content_hash),
            created_by: owner_id,
            created_at: Utc::now(),
        };

        // Store documents
        let paths = UserBucketPaths::new(owner_id);

        self.user_buckets
            .put_object(
                owner_id,
                &paths.file(file_id),
                Bytes::from(serde_json::to_vec(&file_doc).unwrap()),
            )
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        self.user_buckets
            .put_object(
                owner_id,
                &paths.file_version(file_id, version_id),
                Bytes::from(serde_json::to_vec(&version_doc).unwrap()),
            )
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        // Update folder children index
        if let Some(folder_id) = parent_id {
            self.indexes
                .folder_children
                .add_file(owner_id, folder_id, &file_doc)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        } else {
            // Update user roots index
            self.indexes
                .user_roots
                .add_root_file(owner_id, file_id)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        }

        // Convert to domain model
        Ok(file_doc.to_domain())
    }

    /// Get file by ID
    pub async fn get_file(&self, user_id: UserId, file_id: Uuid) -> Result<File, FileError> {
        let paths = UserBucketPaths::new(user_id);

        let data = self
            .user_buckets
            .get_object(user_id, &paths.file(file_id))
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        match data {
            Some(data) => {
                let doc: FileDocV2 = serde_json::from_slice(&data)
                    .map_err(|e| FileError::Storage(format!("Invalid file document: {}", e)))?;

                if doc.deleted {
                    return Err(FileError::NotFound(file_id));
                }

                Ok(doc.to_domain())
            }
            None => Err(FileError::NotFound(file_id)),
        }
    }

    /// List files in a folder
    pub async fn list_files(
        &self,
        user_id: UserId,
        folder_id: Option<Uuid>,
    ) -> Result<Vec<File>, FileError> {
        if let Some(folder_id) = folder_id {
            // Use folder children index
            let children = self
                .indexes
                .folder_children
                .list_files(user_id, folder_id)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;

            let mut files = Vec::new();
            for child in children {
                if !child.deleted {
                    if let Ok(file) = self.get_file(user_id, child.id).await {
                        files.push(file);
                    }
                }
            }
            Ok(files)
        } else {
            // List root files using user roots index
            let root_ids = self
                .indexes
                .user_roots
                .list_root_files(user_id)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;

            let mut files = Vec::new();
            for file_id in root_ids {
                if let Ok(file) = self.get_file(user_id, file_id).await {
                    if !file.deleted {
                        files.push(file);
                    }
                }
            }
            Ok(files)
        }
    }

    /// Rename a file
    pub async fn rename_file(
        &self,
        user_id: UserId,
        file_id: Uuid,
        new_name: String,
    ) -> Result<File, FileError> {
        Self::validate_file_name(&new_name)?;

        let paths = UserBucketPaths::new(user_id);

        // Load existing file
        let data = self
            .user_buckets
            .get_object(user_id, &paths.file(file_id))
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?
            .ok_or(FileError::NotFound(file_id))?;

        let mut doc: FileDocV2 = serde_json::from_slice(&data)
            .map_err(|e| FileError::Storage(format!("Invalid file document: {}", e)))?;

        if doc.deleted {
            return Err(FileError::NotFound(file_id));
        }

        if doc.owner_id != user_id {
            return Err(FileError::PermissionDenied {
                file_id,
                user_id,
            });
        }

        // Update name and path
        doc.name = new_name.clone();

        // Compute new path
        doc.path = if let Some(parent_id) = doc.parent_folder_id {
            let folder_doc = self.get_folder_doc(user_id, parent_id).await?;
            format!("{}/{}", folder_doc.path, new_name)
        } else {
            format!("/{}", new_name)
        };

        doc.updated_at = Utc::now();

        // Save updated document
        self.user_buckets
            .put_object(
                user_id,
                &paths.file(file_id),
                Bytes::from(serde_json::to_vec(&doc).unwrap()),
            )
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        // Update folder children index
        if let Some(parent_id) = doc.parent_folder_id {
            self.indexes
                .folder_children
                .update_file(user_id, parent_id, &doc)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        }

        Ok(doc.to_domain())
    }

    /// Move a file
    pub async fn move_file(
        &self,
        user_id: UserId,
        file_id: Uuid,
        target_folder_id: Option<Uuid>,
    ) -> Result<File, FileError> {
        let paths = UserBucketPaths::new(user_id);

        // Load existing file
        let data = self
            .user_buckets
            .get_object(user_id, &paths.file(file_id))
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?
            .ok_or(FileError::NotFound(file_id))?;

        let mut doc: FileDocV2 = serde_json::from_slice(&data)
            .map_err(|e| FileError::Storage(format!("Invalid file document: {}", e)))?;

        if doc.deleted {
            return Err(FileError::NotFound(file_id));
        }

        if doc.owner_id != user_id {
            return Err(FileError::PermissionDenied { file_id, user_id });
        }

        let old_parent_id = doc.parent_folder_id;

        // Verify target folder exists if specified
        if let Some(target_id) = target_folder_id {
            let _ = self.get_folder_doc(user_id, target_id).await?;
        }

        // Update parent and path
        doc.parent_folder_id = target_folder_id;
        doc.path = if let Some(folder_id) = target_folder_id {
            let folder_doc = self.get_folder_doc(user_id, folder_id).await?;
            format!("{}/{}", folder_doc.path, doc.name)
        } else {
            format!("/{}", doc.name)
        };

        doc.updated_at = Utc::now();

        // Save updated document
        self.user_buckets
            .put_object(
                user_id,
                &paths.file(file_id),
                Bytes::from(serde_json::to_vec(&doc).unwrap()),
            )
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        // Update folder children indexes
        if let Some(old_parent) = old_parent_id {
            self.indexes
                .folder_children
                .remove_child(user_id, old_parent, file_id)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        }

        if let Some(new_parent) = target_folder_id {
            self.indexes
                .folder_children
                .add_file(user_id, new_parent, &doc)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        }

        Ok(doc.to_domain())
    }

    /// Delete a file
    pub async fn delete_file(&self, user_id: UserId, file_id: Uuid) -> Result<(), FileError> {
        let paths = UserBucketPaths::new(user_id);

        // Load existing file
        let data = self
            .user_buckets
            .get_object(user_id, &paths.file(file_id))
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?
            .ok_or(FileError::NotFound(file_id))?;

        let mut doc: FileDocV2 = serde_json::from_slice(&data)
            .map_err(|e| FileError::Storage(format!("Invalid file document: {}", e)))?;

        if doc.deleted {
            return Err(FileError::NotFound(file_id));
        }

        if doc.owner_id != user_id {
            return Err(FileError::PermissionDenied { file_id, user_id });
        }

        // Create tombstone
        let tombstone = TombstoneDocV2::from_file(&doc, user_id);

        // Mark file as deleted
        doc.deleted = true;
        doc.updated_at = Utc::now();

        // Save tombstone and updated file
        self.user_buckets
            .put_object(
                user_id,
                &paths.file_tombstone(file_id),
                Bytes::from(serde_json::to_vec(&tombstone).unwrap()),
            )
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        self.user_buckets
            .put_object(
                user_id,
                &paths.file(file_id),
                Bytes::from(serde_json::to_vec(&doc).unwrap()),
            )
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        // Update folder children index
        if let Some(parent_id) = doc.parent_folder_id {
            self.indexes
                .folder_children
                .mark_deleted(user_id, parent_id, file_id)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    /// Restore a file from tombstone
    pub async fn restore_file(&self, user_id: UserId, file_id: Uuid) -> Result<File, FileError> {
        let paths = UserBucketPaths::new(user_id);

        // Load tombstone
        let tombstone_data = self
            .user_buckets
            .get_object(user_id, &paths.file_tombstone(file_id))
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?
            .ok_or(FileError::NotFound(file_id))?;

        let tombstone: TombstoneDocV2 = serde_json::from_slice(&tombstone_data)
            .map_err(|e| FileError::Storage(format!("Invalid tombstone: {}", e)))?;

        // Restore original document
        let mut doc: FileDocV2 = serde_json::from_value(tombstone.original_doc.clone())
            .map_err(|e| FileError::Storage(format!("Invalid restore data: {}", e)))?;

        doc.deleted = false;
        doc.updated_at = Utc::now();

        // Save restored file
        self.user_buckets
            .put_object(
                user_id,
                &paths.file(file_id),
                Bytes::from(serde_json::to_vec(&doc).unwrap()),
            )
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        // Remove tombstone
        self.user_buckets
            .delete_object(user_id, &paths.file_tombstone(file_id))
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        // Update folder children index
        if let Some(parent_id) = doc.parent_folder_id {
            self.indexes
                .folder_children
                .unmark_deleted(user_id, parent_id, file_id)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        }

        Ok(doc.to_domain())
    }

    /// Update file content
    pub async fn update_file(
        &self,
        user_id: UserId,
        file_id: Uuid,
        expected_version: i32,
        content: Bytes,
    ) -> Result<File, FileError> {
        let paths = UserBucketPaths::new(user_id);

        // Load existing file
        let data = self
            .user_buckets
            .get_object(user_id, &paths.file(file_id))
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?
            .ok_or(FileError::NotFound(file_id))?;

        let mut doc: FileDocV2 = serde_json::from_slice(&data)
            .map_err(|e| FileError::Storage(format!("Invalid file document: {}", e)))?;

        if doc.deleted {
            return Err(FileError::NotFound(file_id));
        }

        if doc.owner_id != user_id {
            return Err(FileError::PermissionDenied { file_id, user_id });
        }

        // Check version
        if doc.version_number != expected_version {
            return Err(FileError::VersionConflict {
                expected: expected_version,
                actual: doc.version_number,
                current_modified_by: doc.owner_id.to_string(),
                current_modified_at: doc.updated_at.to_rfc3339(),
            });
        }

        // Calculate new content hash
        let content_hash = Self::calculate_sha256(&content);
        let size = content.len() as i64;

        // Store blob
        let blob_key = self.blob_store.content_key(&content_hash);
        if !self.blob_store.exists(&blob_key).await.map_err(|e| FileError::Storage(e.to_string()))? {
            self.blob_store
                .put(&blob_key, content)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        }

        // Create new version
        let version_id = Uuid::new_v4();
        let version_doc = FileVersionDocV2 {
            schema_version: 2,
            id: version_id,
            file_id,
            version_number: doc.version_number + 1,
            size,
            content_hash: content_hash.clone(),
            storage_key: format!("blobs/{}", content_hash),
            created_by: user_id,
            created_at: Utc::now(),
        };

        // Update file document
        doc.current_version_id = version_id;
        doc.version_number += 1;
        doc.size = size;
        doc.content_hash = content_hash;
        doc.updated_at = Utc::now();

        // Save documents
        self.user_buckets
            .put_object(
                user_id,
                &paths.file(file_id),
                Bytes::from(serde_json::to_vec(&doc).unwrap()),
            )
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        self.user_buckets
            .put_object(
                user_id,
                &paths.file_version(file_id, version_id),
                Bytes::from(serde_json::to_vec(&version_doc).unwrap()),
            )
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        // Update folder children index
        if let Some(parent_id) = doc.parent_folder_id {
            self.indexes
                .folder_children
                .update_file(user_id, parent_id, &doc)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        }

        Ok(doc.to_domain())
    }

    /// List file versions
    pub async fn list_versions(
        &self,
        user_id: UserId,
        file_id: Uuid,
    ) -> Result<Vec<FileVersion>, FileError> {
        let paths = UserBucketPaths::new(user_id);

        // List all version documents for this file
        let prefix = format!("owned/file_versions/{}/", file_id);
        let version_keys = self
            .user_buckets
            .list_objects(user_id, &prefix)
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        let mut versions = Vec::new();
        for key in version_keys {
            if let Some(data) = self
                .user_buckets
                .get_object(user_id, &key)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?
            {
                let doc: FileVersionDocV2 = serde_json::from_slice(&data)
                    .map_err(|e| FileError::Storage(format!("Invalid version document: {}", e)))?;
                versions.push(doc.to_domain());
            }
        }

        // Sort by version number descending
        versions.sort_by(|a, b| b.version_number.cmp(&a.version_number));

        Ok(versions)
    }

    // Helper methods

    fn validate_file_name(name: &str) -> Result<(), FileError> {
        if name.is_empty() {
            return Err(FileError::InvalidName(
                "File name cannot be empty".to_string(),
            ));
        }

        if name.contains('/') {
            return Err(FileError::InvalidName(
                "File name cannot contain forward slash (/)".to_string(),
            ));
        }

        if name.contains('\0') {
            return Err(FileError::InvalidName(
                "File name cannot contain null character".to_string(),
            ));
        }

        Ok(())
    }

    fn calculate_sha256(content: &Bytes) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        format!("{:x}", hasher.finalize())
    }

    async fn get_folder_doc(
        &self,
        user_id: UserId,
        folder_id: Uuid,
    ) -> Result<FolderDocV2, FileError> {
        let paths = UserBucketPaths::new(user_id);

        let data = self
            .user_buckets
            .get_object(user_id, &paths.folder(folder_id))
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?
            .ok_or(FileError::FolderNotFound(folder_id))?;

        let doc: FolderDocV2 = serde_json::from_slice(&data)
            .map_err(|e| FileError::Storage(format!("Invalid folder document: {}", e)))?;

        Ok(doc)
    }
}
