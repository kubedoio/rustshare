//! Index Managers for V2 Service Layer
//!
//! These indexes are DERIVED STATE stored in user buckets for fast lookup.
//! They can be rebuilt at any time by scanning the canonical documents.

use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use uuid::Uuid;

use crate::{UserBucketStore, UserId};

use super::models::*;
use super::paths::UserBucketPaths;

/// Collection of all user bucket indexes
#[derive(Clone)]
pub struct UserBucketIndexes {
    pub folder_children: Arc<FolderChildrenIndexManager>,
    pub user_roots: Arc<UserRootsIndexManager>,
    pub favourites: Arc<FavouritesIndexManager>,
    pub shared_with_me: Arc<SharedWithMeIndexManager>,
}

impl UserBucketIndexes {
    /// Create all index managers
    pub fn new(user_buckets: Arc<dyn UserBucketStore>) -> Self {
        Self {
            folder_children: Arc::new(FolderChildrenIndexManager::new(user_buckets.clone())),
            user_roots: Arc::new(UserRootsIndexManager::new(user_buckets.clone())),
            favourites: Arc::new(FavouritesIndexManager::new(user_buckets.clone())),
            shared_with_me: Arc::new(SharedWithMeIndexManager::new(user_buckets)),
        }
    }
}

/// Manages folder children indexes for O(1) listing without scanning
pub struct FolderChildrenIndexManager {
    user_buckets: Arc<dyn UserBucketStore>,
}

impl FolderChildrenIndexManager {
    /// Create a new folder children index manager
    pub fn new(user_buckets: Arc<dyn UserBucketStore>) -> Self {
        Self { user_buckets }
    }

    /// Load the folder children index for a folder
    async fn load(&self, user_id: UserId, folder_id: Uuid) -> Result<FolderChildrenIndex> {
        let paths = UserBucketPaths::new(user_id);
        
        match self.user_buckets.get_object(user_id, &paths.folder_children_index(folder_id)).await? {
            Some(data) => {
                let index: FolderChildrenIndex = serde_json::from_slice(&data)?;
                Ok(index)
            }
            None => Ok(FolderChildrenIndex::new(folder_id)),
        }
    }

    /// Save the folder children index
    async fn save(&self, user_id: UserId, index: &FolderChildrenIndex) -> Result<()> {
        let paths = UserBucketPaths::new(user_id);
        let data = Bytes::from(serde_json::to_vec(index)?);
        self.user_buckets.put_object(user_id, &paths.folder_children_index(index.folder_id), data).await?;
        Ok(())
    }

    /// Add a file to the folder's children index
    pub async fn add_file(&self, user_id: UserId, folder_id: Uuid, file: &FileDocumentV2) -> Result<()> {
        let mut index = self.load(user_id, folder_id).await?;
        index.add_file(file.id, file.name.clone());
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// Add a folder to the folder's children index
    pub async fn add_folder(&self, user_id: UserId, parent_id: Uuid, folder: &FolderDocV2) -> Result<()> {
        let mut index = self.load(user_id, parent_id).await?;
        index.add_folder(folder.id, folder.name.clone());
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// Update a file in the index
    pub async fn update_file(&self, user_id: UserId, folder_id: Uuid, file: &FileDocumentV2) -> Result<()> {
        let mut index = self.load(user_id, folder_id).await?;
        // Remove old entry if exists
        index.files.retain(|f| f.id != file.id);
        // Add updated entry
        index.add_file(file.id, file.name.clone());
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// Mark a child as deleted
    pub async fn mark_deleted(&self, user_id: UserId, folder_id: Uuid, child_id: Uuid) -> Result<()> {
        let mut index = self.load(user_id, folder_id).await?;
        index.mark_deleted(child_id);
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// Unmark a child as deleted (restore)
    pub async fn unmark_deleted(&self, user_id: UserId, folder_id: Uuid, child_id: Uuid) -> Result<()> {
        let mut index = self.load(user_id, folder_id).await?;
        if let Some(file) = index.files.iter_mut().find(|f| f.id == child_id) {
            file.deleted = false;
        }
        if let Some(folder) = index.folders.iter_mut().find(|f| f.id == child_id) {
            folder.deleted = false;
        }
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// Remove a child from the index (permanent deletion)
    pub async fn remove_child(&self, user_id: UserId, folder_id: Uuid, child_id: Uuid) -> Result<()> {
        let mut index = self.load(user_id, folder_id).await?;
        index.remove(child_id);
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// List files in a folder
    pub async fn list_files(&self, user_id: UserId, folder_id: Uuid) -> Result<Vec<FolderChildRef>> {
        let index = self.load(user_id, folder_id).await?;
        Ok(index.files)
    }

    /// List folders in a folder
    pub async fn list_folders(&self, user_id: UserId, folder_id: Uuid) -> Result<Vec<FolderChildRef>> {
        let index = self.load(user_id, folder_id).await?;
        Ok(index.folders)
    }

    /// Check if a name is already used in the folder
    pub async fn name_exists(&self, user_id: UserId, folder_id: Uuid, name: &str, exclude_id: Option<Uuid>) -> Result<bool> {
        let index = self.load(user_id, folder_id).await?;
        
        let file_exists = index.files.iter().any(|f| {
            f.name == name && !f.deleted && exclude_id.map_or(true, |id| f.id != id)
        });
        
        let folder_exists = index.folders.iter().any(|f| {
            f.name == name && !f.deleted && exclude_id.map_or(true, |id| f.id != id)
        });
        
        Ok(file_exists || folder_exists)
    }
}

/// Manages user roots index for O(1) root listing
pub struct UserRootsIndexManager {
    user_buckets: Arc<dyn UserBucketStore>,
}

impl UserRootsIndexManager {
    /// Create a new user roots index manager
    pub fn new(user_buckets: Arc<dyn UserBucketStore>) -> Self {
        Self { user_buckets }
    }

    /// Load the user roots index
    async fn load(&self, user_id: UserId) -> Result<UserRootsIndex> {
        let paths = UserBucketPaths::new(user_id);
        
        match self.user_buckets.get_object(user_id, &paths.roots_index()).await? {
            Some(data) => {
                let index: UserRootsIndex = serde_json::from_slice(&data)?;
                Ok(index)
            }
            None => Ok(UserRootsIndex::new()),
        }
    }

    /// Save the user roots index
    async fn save(&self, user_id: UserId, index: &UserRootsIndex) -> Result<()> {
        let paths = UserBucketPaths::new(user_id);
        let data = Bytes::from(serde_json::to_vec(index)?);
        self.user_buckets.put_object(user_id, &paths.roots_index(), data).await?;
        Ok(())
    }

    /// Add a root file
    pub async fn add_root_file(&self, user_id: UserId, file_id: Uuid) -> Result<()> {
        let mut index = self.load(user_id).await?;
        index.add_file(file_id);
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// Add a root folder
    pub async fn add_root_folder(&self, user_id: UserId, folder_id: Uuid) -> Result<()> {
        let mut index = self.load(user_id).await?;
        index.add_folder(folder_id);
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// Remove a root file
    pub async fn remove_root_file(&self, user_id: UserId, file_id: Uuid) -> Result<()> {
        let mut index = self.load(user_id).await?;
        index.remove_file(file_id);
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// Remove a root folder
    pub async fn remove_root_folder(&self, user_id: UserId, folder_id: Uuid) -> Result<()> {
        let mut index = self.load(user_id).await?;
        index.remove_folder(folder_id);
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// List root files
    pub async fn list_root_files(&self, user_id: UserId) -> Result<Vec<Uuid>> {
        let index = self.load(user_id).await?;
        Ok(index.root_files)
    }

    /// List root folders
    pub async fn list_root_folders(&self, user_id: UserId) -> Result<Vec<Uuid>> {
        let index = self.load(user_id).await?;
        Ok(index.root_folders)
    }
}

/// Manages favourites index
pub struct FavouritesIndexManager {
    user_buckets: Arc<dyn UserBucketStore>,
}

impl FavouritesIndexManager {
    /// Create a new favourites index manager
    pub fn new(user_buckets: Arc<dyn UserBucketStore>) -> Self {
        Self { user_buckets }
    }

    /// Load the favourites index
    async fn load(&self, user_id: UserId) -> Result<FavouritesIndex> {
        let paths = UserBucketPaths::new(user_id);
        
        match self.user_buckets.get_object(user_id, &paths.favourites_index()).await? {
            Some(data) => {
                let index: FavouritesIndex = serde_json::from_slice(&data)?;
                Ok(index)
            }
            None => Ok(FavouritesIndex::new()),
        }
    }

    /// Save the favourites index
    async fn save(&self, user_id: UserId, index: &FavouritesIndex) -> Result<()> {
        let paths = UserBucketPaths::new(user_id);
        let data = Bytes::from(serde_json::to_vec(index)?);
        self.user_buckets.put_object(user_id, &paths.favourites_index(), data).await?;
        Ok(())
    }

    /// Add a favourite
    pub async fn add(&self, user_id: UserId, resource_id: Uuid, resource_type: FavouriteResourceType) -> Result<()> {
        let mut index = self.load(user_id).await?;
        index.add(resource_id, resource_type);
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// Remove a favourite
    pub async fn remove(&self, user_id: UserId, resource_id: Uuid) -> Result<bool> {
        let mut index = self.load(user_id).await?;
        let removed = index.remove(resource_id);
        self.save(user_id, &index).await?;
        Ok(removed)
    }

    /// Check if a resource is favourited
    pub async fn contains(&self, user_id: UserId, resource_id: Uuid) -> Result<bool> {
        let index = self.load(user_id).await?;
        Ok(index.contains(resource_id))
    }

    /// Get all favourites
    pub async fn list(&self, user_id: UserId) -> Result<Vec<FavouriteEntry>> {
        let index = self.load(user_id).await?;
        Ok(index.entries)
    }
}

/// Manages shared with me index
pub struct SharedWithMeIndexManager {
    user_buckets: Arc<dyn UserBucketStore>,
}

impl SharedWithMeIndexManager {
    /// Create a new shared with me index manager
    pub fn new(user_buckets: Arc<dyn UserBucketStore>) -> Self {
        Self { user_buckets }
    }

    /// Load the shared with me index
    async fn load(&self, user_id: UserId) -> Result<SharedWithMeIndex> {
        let paths = UserBucketPaths::new(user_id);
        
        match self.user_buckets.get_object(user_id, &paths.shared_with_me_index()).await? {
            Some(data) => {
                let index: SharedWithMeIndex = serde_json::from_slice(&data)?;
                Ok(index)
            }
            None => Ok(SharedWithMeIndex::new()),
        }
    }

    /// Save the shared with me index
    async fn save(&self, user_id: UserId, index: &SharedWithMeIndex) -> Result<()> {
        let paths = UserBucketPaths::new(user_id);
        let data = Bytes::from(serde_json::to_vec(index)?);
        self.user_buckets.put_object(user_id, &paths.shared_with_me_index(), data).await?;
        Ok(())
    }

    /// Add or update a shared entry
    pub async fn upsert(&self, user_id: UserId, entry: SharedWithMeEntry) -> Result<()> {
        let mut index = self.load(user_id).await?;
        index.upsert(entry);
        self.save(user_id, &index).await?;
        Ok(())
    }

    /// Remove a shared entry
    pub async fn remove(&self, user_id: UserId, share_id: Uuid) -> Result<bool> {
        let mut index = self.load(user_id).await?;
        let removed = index.remove(share_id);
        self.save(user_id, &index).await?;
        Ok(removed)
    }

    /// Get all shared entries
    pub async fn list(&self, user_id: UserId) -> Result<Vec<SharedWithMeEntry>> {
        let index = self.load(user_id).await?;
        Ok(index.entries)
    }
}
