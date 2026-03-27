//! Favourite Service V2 Implementation
//!
//! Favourites are user preference state stored in the user's own bucket.
//! Adding/removing favourites never modifies the owner's resource documents.
//!
//! This service supports favouriting:
//! - Owned files and folders
//! - Received files and folders (shared with the user)

use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use rustshare_core::services::FileError;
use rustshare_core::services::FolderError;
use crate::{
    CrossBucketReader, PortableStorageLocator, UserBucketStore, UserId,
};

use super::indexes::UserBucketIndexes;
use super::models::*;
use super::paths::UserBucketPaths;

/// Favourite entry with resource details
#[derive(Debug, Clone)]
pub struct FavouriteDetail {
    pub resource_id: Uuid,
    pub resource_type: FavouriteResourceType,
    pub name: String,
    pub owner_id: UserId,
    pub added_at: chrono::DateTime<chrono::Utc>,
    pub is_owned: bool,
}

/// Favourite service using per-user bucket storage
pub struct FavouriteServiceV2 {
    user_buckets: Arc<dyn UserBucketStore>,
    cross_bucket_reader: Arc<dyn CrossBucketReader>,
    indexes: Arc<UserBucketIndexes>,
}

impl FavouriteServiceV2 {
    /// Create a new favourite service
    pub fn new(
        user_buckets: Arc<dyn UserBucketStore>,
        cross_bucket_reader: Arc<dyn CrossBucketReader>,
        indexes: Arc<UserBucketIndexes>,
    ) -> Self {
        Self {
            user_buckets,
            cross_bucket_reader,
            indexes,
        }
    }

    /// Add a resource to favourites (idempotent)
    ///
    /// This operation only modifies the user's own favourites index.
    /// It never modifies the owner's resource document.
    pub async fn add_favourite(
        &self,
        user_id: UserId,
        resource_id: Uuid,
        resource_type: FavouriteResourceType,
    ) -> Result<(), FavouriteError> {
        // Verify the resource exists and user has access
        self.verify_resource_access(user_id, resource_id, resource_type).await?;

        // Add to favourites index
        self.indexes
            .favourites
            .add(user_id, resource_id, resource_type)
            .await
            .map_err(|e| FavouriteError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Remove a resource from favourites
    ///
    /// This operation only modifies the user's own favourites index.
    /// It never modifies the owner's resource document.
    pub async fn remove_favourite(
        &self,
        user_id: UserId,
        resource_id: Uuid,
    ) -> Result<bool, FavouriteError> {
        let removed = self
            .indexes
            .favourites
            .remove(user_id, resource_id)
            .await
            .map_err(|e| FavouriteError::Storage(e.to_string()))?;

        Ok(removed)
    }

    /// Check if a resource is favourited
    pub async fn is_favourite(
        &self,
        user_id: UserId,
        resource_id: Uuid,
    ) -> Result<bool, FavouriteError> {
        let is_fav = self
            .indexes
            .favourites
            .contains(user_id, resource_id)
            .await
            .map_err(|e| FavouriteError::Storage(e.to_string()))?;

        Ok(is_fav)
    }

    /// List all favourites for a user
    pub async fn list_favourites(
        &self,
        user_id: UserId,
    ) -> Result<Vec<FavouriteEntry>, FavouriteError> {
        let entries = self
            .indexes
            .favourites
            .list(user_id)
            .await
            .map_err(|e| FavouriteError::Storage(e.to_string()))?;

        Ok(entries)
    }

    /// List favourites with resource details
    /// 
    /// This method enriches the favourite entries with resource information
    /// by reading from the appropriate buckets.
    pub async fn list_favourite_details(
        &self,
        user_id: UserId,
    ) -> Result<Vec<FavouriteDetail>, FavouriteError> {
        let entries = self.list_favourites(user_id).await?;
        let mut details = Vec::new();

        for entry in entries {
            match self.get_resource_info(user_id, entry.resource_id, entry.resource_type).await {
                Ok(Some((name, owner_id, is_owned))) => {
                    details.push(FavouriteDetail {
                        resource_id: entry.resource_id,
                        resource_type: entry.resource_type,
                        name,
                        owner_id,
                        added_at: entry.added_at,
                        is_owned,
                    });
                }
                Ok(None) => {
                    // Resource no longer exists or user lost access
                    // Optionally remove from favourites here
                }
                Err(_) => {
                    // Error fetching resource info, skip this entry
                }
            }
        }

        Ok(details)
    }

    /// Toggle favourite status (add if not present, remove if present)
    pub async fn toggle_favourite(
        &self,
        user_id: UserId,
        resource_id: Uuid,
        resource_type: FavouriteResourceType,
    ) -> Result<bool, FavouriteError> {
        let is_favourite = self.is_favourite(user_id, resource_id).await?;

        if is_favourite {
            self.remove_favourite(user_id, resource_id).await?;
            Ok(false) // Now not favourited
        } else {
            self.add_favourite(user_id, resource_id, resource_type).await?;
            Ok(true) // Now favourited
        }
    }

    // Helper methods

    /// Verify that a user has access to a resource
    async fn verify_resource_access(
        &self,
        user_id: UserId,
        resource_id: Uuid,
        resource_type: FavouriteResourceType,
    ) -> Result<(), FavouriteError> {
        match resource_type {
            FavouriteResourceType::OwnedFile => {
                let paths = UserBucketPaths::new(user_id);
                let exists = self
                    .user_buckets
                    .get_object(user_id, &paths.file(resource_id))
                    .await
                    .map_err(|e| FavouriteError::Storage(e.to_string()))?
                    .is_some();

                if !exists {
                    return Err(FavouriteError::ResourceNotFound(resource_id));
                }
                Ok(())
            }
            FavouriteResourceType::OwnedFolder => {
                let paths = UserBucketPaths::new(user_id);
                let exists = self
                    .user_buckets
                    .get_object(user_id, &paths.folder(resource_id))
                    .await
                    .map_err(|e| FavouriteError::Storage(e.to_string()))?
                    .is_some();

                if !exists {
                    return Err(FavouriteError::ResourceNotFound(resource_id));
                }
                Ok(())
            }
            FavouriteResourceType::ReceivedFile | FavouriteResourceType::ReceivedFolder => {
                // For received resources, check the received shares
                let paths = UserBucketPaths::new(user_id);
                let share_keys = self
                    .user_buckets
                    .list_objects(user_id, &paths.received_shares_prefix())
                    .await
                    .map_err(|e| FavouriteError::Storage(e.to_string()))?;

                for key in share_keys {
                    if let Some(data) = self
                        .user_buckets
                        .get_object(user_id, &key)
                        .await
                        .map_err(|e| FavouriteError::Storage(e.to_string()))?
                    {
                        if let Ok(share) = serde_json::from_slice::<ReceivedShareDocV2>(&data) {
                            if share.resource_locator.resource_id == resource_id {
                                return Ok(());
                            }
                        }
                    }
                }

                Err(FavouriteError::ResourceNotFound(resource_id))
            }
        }
    }

    /// Get resource information (name, owner_id, is_owned)
    async fn get_resource_info(
        &self,
        user_id: UserId,
        resource_id: Uuid,
        resource_type: FavouriteResourceType,
    ) -> Result<Option<(String, UserId, bool)>, FavouriteError> {
        match resource_type {
            FavouriteResourceType::OwnedFile => {
                let paths = UserBucketPaths::new(user_id);
                if let Some(data) = self
                    .user_buckets
                    .get_object(user_id, &paths.file(resource_id))
                    .await
                    .map_err(|e| FavouriteError::Storage(e.to_string()))?
                {
                    if let Ok(file) = serde_json::from_slice::<FileDocV2>(&data) {
                        if !file.deleted {
                            return Ok(Some((file.name, user_id, true)));
                        }
                    }
                }
                Ok(None)
            }
            FavouriteResourceType::OwnedFolder => {
                let paths = UserBucketPaths::new(user_id);
                if let Some(data) = self
                    .user_buckets
                    .get_object(user_id, &paths.folder(resource_id))
                    .await
                    .map_err(|e| FavouriteError::Storage(e.to_string()))?
                {
                    if let Ok(folder) = serde_json::from_slice::<FolderDocV2>(&data) {
                        if !folder.deleted {
                            return Ok(Some((folder.name, user_id, true)));
                        }
                    }
                }
                Ok(None)
            }
            FavouriteResourceType::ReceivedFile | FavouriteResourceType::ReceivedFolder => {
                // Find the share and use the locator to get resource info
                let paths = UserBucketPaths::new(user_id);
                let share_keys = self
                    .user_buckets
                    .list_objects(user_id, &paths.received_shares_prefix())
                    .await
                    .map_err(|e| FavouriteError::Storage(e.to_string()))?;

                for key in share_keys {
                    if let Some(data) = self
                        .user_buckets
                        .get_object(user_id, &key)
                        .await
                        .map_err(|e| FavouriteError::Storage(e.to_string()))?
                    {
                        if let Ok(share) = serde_json::from_slice::<ReceivedShareDocV2>(&data) {
                            if share.resource_locator.resource_id == resource_id {
                                // Use cross-bucket reader to get resource info
                                match resource_type {
                                    FavouriteResourceType::ReceivedFile => {
                                        if let Some(data) = self
                                            .cross_bucket_reader
                                            .read_with_locator(&share.resource_locator)
                                            .await
                                            .map_err(|e| FavouriteError::Storage(e.to_string()))?
                                        {
                                            if let Ok(file) = serde_json::from_slice::<FileDocV2>(&data) {
                                                if !file.deleted {
                                                    return Ok(Some((file.name, share.shared_by, false)));
                                                }
                                            }
                                        }
                                    }
                                    FavouriteResourceType::ReceivedFolder => {
                                        if let Some(data) = self
                                            .cross_bucket_reader
                                            .read_with_locator(&share.resource_locator)
                                            .await
                                            .map_err(|e| FavouriteError::Storage(e.to_string()))?
                                        {
                                            if let Ok(folder) = serde_json::from_slice::<FolderDocV2>(&data) {
                                                if !folder.deleted {
                                                    return Ok(Some((folder.name, share.shared_by, false)));
                                                }
                                            }
                                        }
                                    }
                                    _ => unreachable!(),
                                }
                                break;
                            }
                        }
                    }
                }
                Ok(None)
            }
        }
    }
}

/// Favourite-specific errors
#[derive(Debug, thiserror::Error)]
pub enum FavouriteError {
    #[error("Resource not found: {0}")]
    ResourceNotFound(Uuid),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Invalid resource type for operation")]
    InvalidResourceType,
}

impl From<FileError> for FavouriteError {
    fn from(err: FileError) -> Self {
        match err {
            FileError::NotFound(id) => FavouriteError::ResourceNotFound(id),
            FileError::Storage(msg) => FavouriteError::Storage(msg),
            _ => FavouriteError::Storage(err.to_string()),
        }
    }
}

impl From<FolderError> for FavouriteError {
    fn from(err: FolderError) -> Self {
        match err {
            FolderError::NotFound(id) => FavouriteError::ResourceNotFound(id),
            FolderError::Storage(msg) => FavouriteError::Storage(msg),
            _ => FavouriteError::Storage(err.to_string()),
        }
    }
}
