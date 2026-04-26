//! FolderService for folder management operations.
//!
//! This service handles folder operations including:
//! - Folder creation and validation
//! - Folder hierarchy management
//! - Folder tree navigation
//! - Event sourcing via EventStore
//! - Metadata persistence via MetadataStore
//! - Ancestor chain management for permission resolution

use anyhow::Result;
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::{File, Folder, FolderContents, FolderId, FolderTree, SharePermissions, UserId};
use crate::events::{
    AggregateType, Event, EventBroadcaster, EventType, FolderCreatedPayload, FolderDeletedPayload,
    FolderMovedPayload, FolderRenamedPayload,
};
use crate::services::FolderError;
use crate::services::{PermissionResolver, PermissionResolverOps};

/// Trait for event store operations needed by FolderService.
///
/// This trait abstracts the event store to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait EventStoreOps: Send + Sync {
    /// Append an event to the event store.
    async fn append(&self, event: &Event, broadcaster: &EventBroadcaster) -> Result<()>;
}

/// Trait for metadata store operations needed by FolderService.
///
/// This trait abstracts the metadata store to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait MetadataStoreOps: Send + Sync {
    /// Create a folder in the metadata store.
    async fn create_folder(&self, folder: &Folder) -> Result<()>;

    /// Find a folder by ID.
    async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>>;

    /// Update a folder in the metadata store.
    async fn update_folder(&self, folder: &Folder) -> Result<()>;

    /// Delete a folder from the metadata store.
    async fn delete_folder(&self, id: FolderId) -> Result<()>;

    /// List folders with optional parent filter.
    async fn list_folders(
        &self,
        parent_id: Option<FolderId>,
        owner_id: UserId,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<Folder>>;

    /// List folders by parent regardless of owner.
    async fn list_folders_by_parent(
        &self,
        parent_id: Option<FolderId>,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<Folder>>;

    /// Find all descendant folders of a given folder using recursive CTE.
    async fn find_descendant_folders(&self, folder_id: FolderId) -> Result<Vec<Folder>>;

    /// List files with optional parent filter.
    async fn list_files(
        &self,
        parent_id: Option<FolderId>,
        owner_id: UserId,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<File>>;

    /// List files by parent regardless of owner.
    async fn list_files_by_parent(
        &self,
        parent_id: Option<FolderId>,
        tenant_id: uuid::Uuid,
    ) -> Result<Vec<File>>;
}

/// FolderService manages folder operations with event sourcing.
pub struct FolderService<E, M, P>
where
    E: EventStoreOps,
    M: MetadataStoreOps,
    P: PermissionResolverOps,
{
    event_store: Arc<E>,
    metadata_store: Arc<M>,
    broadcaster: Arc<EventBroadcaster>,
    permission_resolver: Arc<PermissionResolver<P>>,
}

impl<E, M, P> FolderService<E, M, P>
where
    E: EventStoreOps,
    M: MetadataStoreOps,
    P: PermissionResolverOps,
{
    /// Create a new FolderService instance.
    pub fn new(
        event_store: Arc<E>,
        metadata_store: Arc<M>,
        broadcaster: Arc<EventBroadcaster>,
        permission_resolver: Arc<PermissionResolver<P>>,
    ) -> Self {
        Self {
            event_store,
            metadata_store,
            broadcaster,
            permission_resolver,
        }
    }
    async fn require_folder_permission(
        &self,
        user_id: UserId,
        folder_id: FolderId,
        required: SharePermissions,
    ) -> Result<(), FolderError> {
        let has = self
            .permission_resolver
            .check_folder_permission(user_id, folder_id, required)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?;
        if !has {
            return Err(FolderError::PermissionDenied { folder_id, user_id });
        }
        Ok(())
    }


    /// Create a new folder.
    ///
    /// Validates the folder name, checks that the parent folder exists (if specified),
    /// constructs the path, emits a FolderCreated event, and inserts into the database.
    pub async fn create_folder(
        &self,
        name: String,
        parent_folder_id: Option<FolderId>,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, FolderError> {
        // Validate folder name
        self.validate_folder_name(&name)?;

        // Verify parent folder if specified, and construct path and ancestor_ids
        let (_path, folder) = if let Some(parent_id) = parent_folder_id {
            // Verify parent folder exists and user has access
            let parent = self
                .metadata_store
                .find_folder_by_id(parent_id)
                .await
                .map_err(|e| FolderError::Database(e.to_string()))?
                .ok_or(FolderError::ParentFolderNotFound(parent_id))?;

            // Verify permissions: user must own the folder or have Edit permission
            self.require_folder_permission(owner_id, parent_id, SharePermissions::Edit).await?;

            // Construct path: parent_path + "/" + name
            let path = format!("{}/{}", parent.path.trim_end_matches('/'), name);

            // Create folder with proper ancestor_ids from parent
            let parent_ancestors = parent.ancestor_ids.as_deref();
            let folder = Folder::new_child_with_ancestors(
                name.clone(),
                path.clone(),
                parent_id,
                parent_ancestors,
                owner_id,
                tenant_id,
            );

            (path, folder)
        } else {
            // Root folder with user-provided name
            let folder = Folder::new_root_with_name(name, owner_id, tenant_id);
            let path = folder.path.clone();
            (path, folder)
        };

        // Emit FolderCreated event
        let payload = FolderCreatedPayload {
            folder_id: folder.id,
            name: folder.name.clone(),
            path: folder.path.clone(),
            parent_folder_id: folder.parent_folder_id,
            owner_id: folder.owner_id,
        };

        let event = Event::new(
            EventType::FolderCreated,
            folder.id,
            AggregateType::Folder,
            serde_json::to_value(payload)
                .map_err(|e| FolderError::Database(e.to_string()))?,
            owner_id,
        );

        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|e| FolderError::Database(format!("Failed to append event: {}", e)))?;

        // Insert into metadata store
        self.metadata_store
            .create_folder(&folder)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?;

        Ok(folder)
    }

    /// Get a folder by ID.
    ///
    /// Verifies that the folder exists and that the user has permission to access it.
    pub async fn get_folder(
        &self,
        folder_id: FolderId,
        user_id: UserId,
    ) -> Result<Folder, FolderError> {
        // 1. Check permissions first using the resolver
        self.require_folder_permission(user_id, folder_id, SharePermissions::View).await?;

        // 2. Find folder by ID
        let folder = self
            .metadata_store
            .find_folder_by_id(folder_id)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?
            .ok_or(FolderError::NotFound(folder_id))?;

        Ok(folder)
    }

    /// List the contents of a folder (immediate children only).
    ///
    /// Returns a FolderContents structure containing the files and immediate subfolders
    /// in the specified folder. Does not recurse into subdirectories.
    pub async fn list_contents(
        &self,
        folder_id: FolderId,
        user_id: UserId,
    ) -> Result<FolderContents, FolderError> {
        // Verify folder exists and user has access
        let folder = self.get_folder(folder_id, user_id).await?;

        // Get files in this folder (filter by folder owner, not current user)
        let files = self
            .metadata_store
            .list_files_by_parent(Some(folder.id), folder.tenant_id)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?;

        // Get subfolders in this folder (filter by folder owner, not current user)
        let folders = self
            .metadata_store
            .list_folders_by_parent(Some(folder.id), folder.tenant_id)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?;

        Ok(FolderContents::with_contents(files, folders))
    }

    /// Get a recursive tree structure of a folder and all its descendants.
    ///
    /// Returns a FolderTree containing the folder, all its files, and recursively
    /// all its subfolders with their files.
    pub async fn get_tree(
        &self,
        folder_id: FolderId,
        user_id: UserId,
    ) -> Result<FolderTree, FolderError> {
        // Verify folder exists and user has access
        let folder = self.get_folder(folder_id, user_id).await?;

        // Build the tree recursively
        self.build_tree_internal(folder, user_id).await
    }

    /// Internal recursive method to build folder tree.
    ///
    /// This is a separate method to allow for Box::pin recursion.
    async fn build_tree_internal(
        &self,
        folder: Folder,
        user_id: UserId,
    ) -> Result<FolderTree, FolderError> {
        // Get files in this folder
        let files = self
            .metadata_store
            .list_files_by_parent(Some(folder.id), folder.tenant_id)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?;

        // Get immediate subfolders
        let subfolders = self
            .metadata_store
            .list_folders_by_parent(Some(folder.id), folder.tenant_id)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?;

        // Recursively build trees for each subfolder
        let mut subfolder_trees = Vec::new();
        for subfolder in subfolders {
            let subtree = Box::pin(self.build_tree_internal(subfolder, user_id)).await?;
            subfolder_trees.push(subtree);
        }

        Ok(FolderTree::with_contents(folder, files, subfolder_trees))
    }

    /// Rename a folder.
    ///
    /// Updates the folder name and path, and recursively updates all descendant paths.
    pub async fn rename_folder(
        &self,
        folder_id: FolderId,
        new_name: String,
        user_id: UserId,
    ) -> Result<Folder, FolderError> {
        // Validate new name
        self.validate_folder_name(&new_name)?;

        // Get and verify access
        let mut folder = self.get_folder(folder_id, user_id).await?;

        // Verify Edit permission
        self.require_folder_permission(user_id, folder_id, SharePermissions::Edit).await?;

        // Check if name is actually changing
        if folder.name == new_name {
            return Ok(folder);
        }

        let old_name = folder.name.clone();
        let old_path = folder.path.clone();

        // Calculate new path
        let new_path = if let Some(parent_id) = folder.parent_folder_id {
            let parent = self
                .metadata_store
                .find_folder_by_id(parent_id)
                .await
                .map_err(|e| FolderError::Database(e.to_string()))?
                .ok_or(FolderError::ParentFolderNotFound(parent_id))?;
            format!("{}/{}", parent.path.trim_end_matches('/'), new_name)
        } else {
            format!("/{}", new_name)
        };

        // Update folder
        folder.name = new_name.clone();
        folder.path = new_path.clone();
        folder.updated_at = chrono::Utc::now();

        // Update descendants' paths and ancestor_ids
        self.update_descendant_paths_and_ancestors(folder_id, &old_path, &new_path, user_id)
            .await?;

        // Emit FolderRenamed event
        let payload = FolderRenamedPayload {
            folder_id,
            old_name,
            new_name,
            old_path,
            new_path,
            renamed_by: user_id,
        };

        let event = Event::new(
            EventType::FolderRenamed,
            folder_id,
            AggregateType::Folder,
            serde_json::to_value(payload)
                .map_err(|e| FolderError::Database(e.to_string()))?,
            user_id,
        );

        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|e| FolderError::Database(format!("Failed to append event: {}", e)))?;

        // Update in metadata store
        self.metadata_store
            .update_folder(&folder)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?;

        Ok(folder)
    }

    /// Move a folder to a new parent.
    ///
    /// Checks for circular references and updates paths for the folder and all descendants.
    pub async fn move_folder(
        &self,
        folder_id: FolderId,
        new_parent_id: Option<FolderId>,
        user_id: UserId,
    ) -> Result<Folder, FolderError> {
        // Get and verify access
        let mut folder = self.get_folder(folder_id, user_id).await?;

        // Verify Edit permission
        self.require_folder_permission(user_id, folder_id, SharePermissions::Edit).await?;

        // Check if parent is actually changing
        if folder.parent_folder_id == new_parent_id {
            return Ok(folder);
        }

        let old_parent_id = folder.parent_folder_id;
        let old_path = folder.path.clone();

        // Verify new parent exists and check for circular reference
        let (new_path, new_parent_ancestors) = if let Some(parent_id) = new_parent_id {
            // Check if new parent exists and user owns it
            let parent = self.get_folder(parent_id, user_id).await?;

            // Check for circular reference using ancestor_ids if available
            let would_create_cycle = if let Some(ref parent_ancestors) = parent.ancestor_ids {
                // Fast path: check if folder_id is in parent's ancestors
                parent_ancestors.contains(&folder_id)
            } else {
                // Slow path: check descendants
                let descendants = self
                    .metadata_store
                    .find_descendant_folders(folder_id)
                    .await
                    .map_err(|e| FolderError::Database(e.to_string()))?;
                descendants.iter().any(|d| d.id == parent_id)
            };

            if would_create_cycle {
                return Err(FolderError::CircularReference {
                    folder_id,
                    target_id: parent_id,
                });
            }

            // Calculate new path
            let new_path = format!("{}/{}", parent.path.trim_end_matches('/'), folder.name);
            (new_path, parent.ancestor_ids.clone())
        } else {
            // Moving to root
            (format!("/{}", folder.name), Some(Vec::new()))
        };

        // Update folder
        folder.parent_folder_id = new_parent_id;
        folder.path = new_path.clone();
        folder.updated_at = chrono::Utc::now();

        // Update ancestor_ids for the moved folder
        let new_ancestor_ids = if let Some(parent_id) = new_parent_id {
            let mut ancestors = new_parent_ancestors.unwrap_or_default();
            ancestors.push(parent_id);
            Some(ancestors)
        } else {
            Some(Vec::new()) // Root folder has no ancestors
        };
        folder.ancestor_ids = new_ancestor_ids;

        // Update descendants' paths
        self.update_descendant_paths_and_ancestors(folder_id, &old_path, &new_path, user_id)
            .await?;

        // Emit FolderMoved event
        let payload = FolderMovedPayload {
            folder_id,
            old_parent_folder_id: old_parent_id,
            new_parent_folder_id: new_parent_id,
            old_path,
            new_path,
            moved_by: user_id,
        };

        let event = Event::new(
            EventType::FolderMoved,
            folder_id,
            AggregateType::Folder,
            serde_json::to_value(payload)
                .map_err(|e| FolderError::Database(e.to_string()))?,
            user_id,
        );

        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|e| FolderError::Database(format!("Failed to append event: {}", e)))?;

        // Update in metadata store
        self.metadata_store
            .update_folder(&folder)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?;

        Ok(folder)
    }

    /// Delete a folder and all its contents recursively.
    ///
    /// Emits FolderDeleted events for all deleted folders (in reverse tree order).
    pub async fn delete_folder(
        &self,
        folder_id: FolderId,
        user_id: UserId,
    ) -> Result<(), FolderError> {
        // Get and verify access
        let folder = self.get_folder(folder_id, user_id).await?;

        // Verify Admin permission for deletion
        self.require_folder_permission(user_id, folder_id, SharePermissions::Admin).await?;

        // Check if it's the system root folder (nil UUID) - only protect system folders
        // User-created root folders should be deletable even if named "Root"
        if folder.id.as_u128() == 0 {
            return Err(FolderError::CannotDeleteRoot(folder_id));
        }

        // Get all descendant folders (including this folder)
        let descendants = self
            .metadata_store
            .find_descendant_folders(folder_id)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?;

        // Delete in reverse order (children before parents) to maintain referential integrity
        for descendant in descendants.iter().rev() {
            // Emit FolderDeleted event for each folder
            let payload = FolderDeletedPayload {
                folder_id: descendant.id,
                name: descendant.name.clone(),
                path: descendant.path.clone(),
                deleted_by: user_id,
            };

            let event = Event::new(
                EventType::FolderDeleted,
                descendant.id,
                AggregateType::Folder,
                serde_json::to_value(payload)
                    .map_err(|e| FolderError::Database(e.to_string()))?,
                user_id,
            );

            self.event_store
                .append(&event, &self.broadcaster)
                .await
                .map_err(|e| FolderError::Database(format!("Failed to append event: {}", e)))?;

            // Delete from metadata store
            self.metadata_store
                .delete_folder(descendant.id)
                .await
                .map_err(|e| FolderError::Database(e.to_string()))?;
        }

        Ok(())
    }

    /// Helper method to update paths of all descendant folders.
    ///
    /// Used when renaming or moving a folder to ensure all descendant paths remain consistent.
    /// Helper method to update paths and ancestor_ids of all descendant folders.
    ///
    /// Used when moving a folder to ensure all descendant paths and ancestor_ids remain consistent.
    /// This updates the ancestor_ids by replacing the old folder_id prefix with the new ancestor chain.
    async fn update_descendant_paths_and_ancestors(
        &self,
        folder_id: FolderId,
        old_path: &str,
        new_path: &str,
        _user_id: UserId,
    ) -> Result<(), FolderError> {
        // Get all descendants (excluding the folder itself)
        let all_descendants = self
            .metadata_store
            .find_descendant_folders(folder_id)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?;

        // Filter to get only descendants, not the folder itself
        let descendants: Vec<_> = all_descendants
            .into_iter()
            .filter(|d| d.id != folder_id)
            .collect();

        // Get the moved folder's new ancestor_ids to compute descendants' new ancestors
        let moved_folder = self
            .metadata_store
            .find_folder_by_id(folder_id)
            .await
            .map_err(|e| FolderError::Database(e.to_string()))?
            .ok_or(FolderError::NotFound(folder_id))?;

        let new_moved_folder_ancestors = moved_folder.ancestor_ids.clone().unwrap_or_default();

        // Update each descendant's path and ancestor_ids
        for mut descendant in descendants {
            let mut needs_update = false;

            // Update path: replace the old path prefix with the new path
            if descendant.path.starts_with(old_path) {
                descendant.path = descendant.path.replace(old_path, new_path);
                needs_update = true;
            }

            // Update ancestor_ids: rebuild from the moved folder's new ancestors
            if let Some(ref old_ancestor_ids) = descendant.ancestor_ids {
                // Find where folder_id appears in the old ancestor chain
                if let Some(pos) = old_ancestor_ids.iter().position(|&id| id == folder_id) {
                    // Build new ancestors: moved_folder's ancestors + moved_folder + rest of old chain after folder_id
                    let mut new_ancestors = new_moved_folder_ancestors.clone();
                    new_ancestors.push(folder_id);
                    // Add everything after folder_id from the old chain
                    if pos + 1 < old_ancestor_ids.len() {
                        new_ancestors.extend(&old_ancestor_ids[pos + 1..]);
                    }
                    descendant.ancestor_ids = Some(new_ancestors);
                    needs_update = true;
                }
            }

            if needs_update {
                descendant.updated_at = chrono::Utc::now();
                self.metadata_store
                    .update_folder(&descendant)
                    .await
                    .map_err(|e| FolderError::Database(e.to_string()))?;
            }
        }

        Ok(())
    }

    /// Validate folder name.
    ///
    /// Ensures the name is not empty and does not contain illegal characters.
    fn validate_folder_name(&self, name: &str) -> Result<(), FolderError> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Folder;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    // Mock EventStore for testing
    struct MockEventStore {
        events: Mutex<Vec<Event>>,
    }

    impl MockEventStore {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }
    }

    impl EventStoreOps for MockEventStore {
        async fn append(&self, event: &Event, _broadcaster: &EventBroadcaster) -> Result<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    // Mock MetadataStore for testing
    struct MockMetadataStore {
        folders: Mutex<HashMap<FolderId, Folder>>,
        files: Mutex<HashMap<FolderId, Vec<File>>>,
    }

    impl MockMetadataStore {
        fn new() -> Self {
            Self {
                folders: Mutex::new(HashMap::new()),
                files: Mutex::new(HashMap::new()),
            }
        }
    }

    impl MetadataStoreOps for MockMetadataStore {
        async fn create_folder(&self, folder: &Folder) -> Result<()> {
            self.folders
                .lock()
                .unwrap()
                .insert(folder.id, folder.clone());
            Ok(())
        }

        async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>> {
            Ok(self.folders.lock().unwrap().get(&id).cloned())
        }

        async fn update_folder(&self, folder: &Folder) -> Result<()> {
            self.folders
                .lock()
                .unwrap()
                .insert(folder.id, folder.clone());
            Ok(())
        }

        async fn delete_folder(&self, id: FolderId) -> Result<()> {
            self.folders.lock().unwrap().remove(&id);
            Ok(())
        }

        async fn list_folders(
            &self,
            parent_id: Option<FolderId>,
            owner_id: UserId,
            _tenant_id: uuid::Uuid,
        ) -> Result<Vec<Folder>> {
            let folders = self.folders.lock().unwrap();
            let result: Vec<Folder> = folders
                .values()
                .filter(|f| f.owner_id == owner_id && f.parent_folder_id == parent_id)
                .cloned()
                .collect();
            Ok(result)
        }

        async fn find_descendant_folders(&self, folder_id: FolderId) -> Result<Vec<Folder>> {
            let folders = self.folders.lock().unwrap();
            let mut result = Vec::new();
            let mut to_process = vec![folder_id];

            while let Some(current_id) = to_process.pop() {
                if let Some(folder) = folders.get(&current_id) {
                    result.push(folder.clone());
                    // Find children
                    for f in folders.values() {
                        if f.parent_folder_id == Some(current_id) {
                            to_process.push(f.id);
                        }
                    }
                }
            }

            Ok(result)
        }

        async fn list_files(
            &self,
            parent_id: Option<FolderId>,
            _owner_id: UserId,
            _tenant_id: uuid::Uuid,
        ) -> Result<Vec<File>> {
            let files = self.files.lock().unwrap();
            Ok(files
                .get(&parent_id.unwrap_or_default())
                .cloned()
                .unwrap_or_default())
        }
    }

    #[tokio::test]
    async fn test_create_folder_success() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store.clone(),
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let folder = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        assert_eq!(folder.name, "Documents");
        assert_eq!(folder.path, "/Documents");
        assert_eq!(folder.parent_folder_id, None);
        assert_eq!(folder.owner_id, owner_id);

        // Verify event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FolderCreated);
    }

    #[tokio::test]
    async fn test_create_subfolder_success() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store.clone(),
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create parent folder
        let parent = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Create subfolder
        let subfolder = service
            .create_folder(
                "Work".to_string(),
                Some(parent.id),
                owner_id,
                Uuid::new_v4(),
            )
            .await
            .unwrap();

        assert_eq!(subfolder.name, "Work");
        assert_eq!(subfolder.path, "/Documents/Work");
        assert_eq!(subfolder.parent_folder_id, Some(parent.id));
        assert_eq!(subfolder.owner_id, owner_id);
    }

    #[tokio::test]
    async fn test_create_folder_invalid_name_empty() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let result = service
            .create_folder("".to_string(), None, owner_id, Uuid::new_v4())
            .await;

        assert!(matches!(result, Err(FolderError::InvalidName(_))));
    }

    #[tokio::test]
    async fn test_create_folder_invalid_name_with_slash() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let result = service
            .create_folder("Work/Projects".to_string(), None, owner_id, Uuid::new_v4())
            .await;

        assert!(matches!(result, Err(FolderError::InvalidName(_))));
    }

    #[tokio::test]
    async fn test_create_folder_parent_not_found() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let non_existent_parent_id = Uuid::new_v4();
        let result = service
            .create_folder(
                "Work".to_string(),
                Some(non_existent_parent_id),
                owner_id,
                Uuid::new_v4(),
            )
            .await;

        assert!(matches!(result, Err(FolderError::ParentFolderNotFound(_))));
    }

    #[tokio::test]
    async fn test_create_folder_permission_denied() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();

        // Create parent folder as owner
        let parent = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Try to create subfolder as different user
        let result = service
            .create_folder(
                "Work".to_string(),
                Some(parent.id),
                other_user_id,
                Uuid::new_v4(),
            )
            .await;

        assert!(matches!(result, Err(FolderError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_get_folder_success() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let created = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        let retrieved = service.get_folder(created.id, owner_id).await.unwrap();

        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.name, "Documents");
        assert_eq!(retrieved.owner_id, owner_id);
    }

    #[tokio::test]
    async fn test_get_folder_not_found() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let non_existent_id = Uuid::new_v4();
        let result = service.get_folder(non_existent_id, owner_id).await;

        assert!(matches!(result, Err(FolderError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_get_folder_permission_denied() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let created = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        let result = service.get_folder(created.id, other_user_id).await;

        assert!(matches!(result, Err(FolderError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_list_contents_empty_folder() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let folder = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        let contents = service.list_contents(folder.id, owner_id).await.unwrap();

        assert_eq!(contents.files.len(), 0);
        assert_eq!(contents.folders.len(), 0);
    }

    #[tokio::test]
    async fn test_list_contents_with_subfolders() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create parent folder
        let parent = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Create subfolders
        let _subfolder1 = service
            .create_folder(
                "Work".to_string(),
                Some(parent.id),
                owner_id,
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        let _subfolder2 = service
            .create_folder(
                "Personal".to_string(),
                Some(parent.id),
                owner_id,
                Uuid::new_v4(),
            )
            .await
            .unwrap();

        let contents = service.list_contents(parent.id, owner_id).await.unwrap();

        assert_eq!(contents.files.len(), 0);
        assert_eq!(contents.folders.len(), 2);
        let folder_names: Vec<&str> = contents.folders.iter().map(|f| f.name.as_str()).collect();
        assert!(folder_names.contains(&"Work"));
        assert!(folder_names.contains(&"Personal"));
    }

    #[tokio::test]
    async fn test_list_contents_permission_denied() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let folder = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        let result = service.list_contents(folder.id, other_user_id).await;

        assert!(matches!(result, Err(FolderError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_get_tree_single_folder() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let folder = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        let tree = service.get_tree(folder.id, owner_id).await.unwrap();

        assert_eq!(tree.folder.id, folder.id);
        assert_eq!(tree.folder.name, "Documents");
        assert_eq!(tree.files.len(), 0);
        assert_eq!(tree.subfolders.len(), 0);
    }

    #[tokio::test]
    async fn test_get_tree_with_subfolders() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create folder hierarchy:
        // Documents/
        //   Work/
        //     Projects/
        //   Personal/
        let root = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let work = service
            .create_folder("Work".to_string(), Some(root.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let _projects = service
            .create_folder(
                "Projects".to_string(),
                Some(work.id),
                owner_id,
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        let _personal = service
            .create_folder(
                "Personal".to_string(),
                Some(root.id),
                owner_id,
                Uuid::new_v4(),
            )
            .await
            .unwrap();

        let tree = service.get_tree(root.id, owner_id).await.unwrap();

        assert_eq!(tree.folder.name, "Documents");
        assert_eq!(tree.subfolders.len(), 2);

        // Check Work subfolder
        let work_tree = tree
            .subfolders
            .iter()
            .find(|t| t.folder.name == "Work")
            .unwrap();
        assert_eq!(work_tree.subfolders.len(), 1);
        assert_eq!(work_tree.subfolders[0].folder.name, "Projects");

        // Check Personal subfolder
        let personal_tree = tree
            .subfolders
            .iter()
            .find(|t| t.folder.name == "Personal")
            .unwrap();
        assert_eq!(personal_tree.subfolders.len(), 0);
    }

    #[tokio::test]
    async fn test_get_tree_permission_denied() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let folder = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        let result = service.get_tree(folder.id, other_user_id).await;

        assert!(matches!(result, Err(FolderError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_rename_folder_success() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store.clone(),
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let folder = service
            .create_folder("OldName".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        let renamed = service
            .rename_folder(folder.id, "NewName".to_string(), owner_id)
            .await
            .unwrap();

        assert_eq!(renamed.name, "NewName");
        assert_eq!(renamed.path, "/NewName");
        assert_eq!(renamed.id, folder.id);

        // Verify event was emitted
        let events = event_store.events.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| e.event_type == EventType::FolderRenamed));
    }

    #[tokio::test]
    async fn test_rename_folder_updates_descendant_paths() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create hierarchy: Documents/Work/Projects
        let docs = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let work = service
            .create_folder("Work".to_string(), Some(docs.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let projects = service
            .create_folder(
                "Projects".to_string(),
                Some(work.id),
                owner_id,
                Uuid::new_v4(),
            )
            .await
            .unwrap();

        // Rename Documents to Files
        service
            .rename_folder(docs.id, "Files".to_string(), owner_id)
            .await
            .unwrap();

        // Verify descendant paths were updated
        let updated_work = service.get_folder(work.id, owner_id).await.unwrap();
        let updated_projects = service.get_folder(projects.id, owner_id).await.unwrap();

        assert_eq!(updated_work.path, "/Files/Work");
        assert_eq!(updated_projects.path, "/Files/Work/Projects");
    }

    #[tokio::test]
    async fn test_rename_folder_invalid_name() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let folder = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        let result = service
            .rename_folder(folder.id, "Invalid/Name".to_string(), owner_id)
            .await;

        assert!(matches!(result, Err(FolderError::InvalidName(_))));
    }

    #[tokio::test]
    async fn test_rename_folder_no_change() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store.clone(),
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let folder = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Get initial event count
        let initial_event_count = event_store.events.lock().unwrap().len();

        // Rename to same name
        let renamed = service
            .rename_folder(folder.id, "Documents".to_string(), owner_id)
            .await
            .unwrap();

        assert_eq!(renamed.name, "Documents");

        // No new event should be emitted
        let final_event_count = event_store.events.lock().unwrap().len();
        assert_eq!(initial_event_count, final_event_count);
    }

    #[tokio::test]
    async fn test_move_folder_success() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store.clone(),
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create folders at root level
        let docs = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let projects = service
            .create_folder("Projects".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Move Projects into Documents
        let moved = service
            .move_folder(projects.id, Some(docs.id), owner_id)
            .await
            .unwrap();

        assert_eq!(moved.parent_folder_id, Some(docs.id));
        assert_eq!(moved.path, "/Documents/Projects");

        // Verify event was emitted
        let events = event_store.events.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| e.event_type == EventType::FolderMoved));
    }

    #[tokio::test]
    async fn test_move_folder_circular_reference() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create hierarchy: Documents/Work
        let docs = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let work = service
            .create_folder("Work".to_string(), Some(docs.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Try to move Documents into Work (circular reference)
        let result = service.move_folder(docs.id, Some(work.id), owner_id).await;

        assert!(matches!(result, Err(FolderError::CircularReference { .. })));
    }

    #[tokio::test]
    async fn test_move_folder_updates_descendant_paths() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create hierarchy: Documents, Work/Projects
        let docs = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let work = service
            .create_folder("Work".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let projects = service
            .create_folder(
                "Projects".to_string(),
                Some(work.id),
                owner_id,
                Uuid::new_v4(),
            )
            .await
            .unwrap();

        // Move Work into Documents
        service
            .move_folder(work.id, Some(docs.id), owner_id)
            .await
            .unwrap();

        // Verify descendant paths were updated
        let updated_work = service.get_folder(work.id, owner_id).await.unwrap();
        let updated_projects = service.get_folder(projects.id, owner_id).await.unwrap();

        assert_eq!(updated_work.path, "/Documents/Work");
        assert_eq!(updated_projects.path, "/Documents/Work/Projects");
    }

    #[tokio::test]
    async fn test_move_folder_no_change() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store.clone(),
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        let docs = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let work = service
            .create_folder("Work".to_string(), Some(docs.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Get initial event count
        let initial_event_count = event_store.events.lock().unwrap().len();

        // Move to same parent
        let result = service
            .move_folder(work.id, Some(docs.id), owner_id)
            .await
            .unwrap();

        assert_eq!(result.parent_folder_id, Some(docs.id));

        // No new event should be emitted
        let final_event_count = event_store.events.lock().unwrap().len();
        assert_eq!(initial_event_count, final_event_count);
    }

    #[tokio::test]
    async fn test_delete_folder_empty() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store.clone(),
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let folder = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        service.delete_folder(folder.id, owner_id).await.unwrap();

        // Verify folder no longer exists
        let result = service.get_folder(folder.id, owner_id).await;
        assert!(matches!(result, Err(FolderError::NotFound(_))));

        // Verify event was emitted
        let events = event_store.events.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| e.event_type == EventType::FolderDeleted));
    }

    #[tokio::test]
    async fn test_delete_folder_with_descendants() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store.clone(),
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create hierarchy: Documents/Work/Projects
        let docs = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let work = service
            .create_folder("Work".to_string(), Some(docs.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let projects = service
            .create_folder(
                "Projects".to_string(),
                Some(work.id),
                owner_id,
                Uuid::new_v4(),
            )
            .await
            .unwrap();

        // Delete Documents (should cascade delete Work and Projects)
        service.delete_folder(docs.id, owner_id).await.unwrap();

        // Verify all folders are deleted
        assert!(matches!(
            service.get_folder(docs.id, owner_id).await,
            Err(FolderError::NotFound(_))
        ));
        assert!(matches!(
            service.get_folder(work.id, owner_id).await,
            Err(FolderError::NotFound(_))
        ));
        assert!(matches!(
            service.get_folder(projects.id, owner_id).await,
            Err(FolderError::NotFound(_))
        ));

        // Verify events were emitted for all folders (3 FolderDeleted events)
        let events = event_store.events.lock().unwrap();
        let delete_events: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::FolderDeleted)
            .collect();
        assert_eq!(delete_events.len(), 3);
    }

    #[tokio::test]
    async fn test_delete_folder_permission_denied() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let folder = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        let result = service.delete_folder(folder.id, other_user_id).await;

        assert!(matches!(result, Err(FolderError::PermissionDenied { .. })));
    }

    // =======================================================================
    // Ancestor ID Tests
    // =======================================================================

    #[tokio::test]
    async fn test_create_folder_ancestor_ids_root() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create root-level folder
        let folder = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Root folder should have empty ancestor_ids
        assert_eq!(folder.ancestor_ids, Some(Vec::new()));

        // Verify by fetching from store
        let stored = metadata_store
            .find_folder_by_id(folder.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.ancestor_ids, Some(Vec::new()));
    }

    #[tokio::test]
    async fn test_create_folder_ancestor_ids_nested() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create hierarchy: Documents/Work/Projects
        let docs = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let work = service
            .create_folder("Work".to_string(), Some(docs.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let projects = service
            .create_folder(
                "Projects".to_string(),
                Some(work.id),
                owner_id,
                Uuid::new_v4(),
            )
            .await
            .unwrap();

        // Verify ancestor_ids
        assert_eq!(docs.ancestor_ids, Some(Vec::new()));
        assert_eq!(work.ancestor_ids, Some(vec![docs.id]));
        assert_eq!(projects.ancestor_ids, Some(vec![docs.id, work.id]));

        // Verify by fetching from store
        let stored_docs = metadata_store
            .find_folder_by_id(docs.id)
            .await
            .unwrap()
            .unwrap();
        let stored_work = metadata_store
            .find_folder_by_id(work.id)
            .await
            .unwrap()
            .unwrap();
        let stored_projects = metadata_store
            .find_folder_by_id(projects.id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(stored_docs.ancestor_ids, Some(Vec::new()));
        assert_eq!(stored_work.ancestor_ids, Some(vec![docs.id]));
        assert_eq!(stored_projects.ancestor_ids, Some(vec![docs.id, work.id]));
    }

    #[tokio::test]
    async fn test_move_folder_updates_ancestor_ids() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create two separate hierarchies:
        // Source:      Destination:
        // Work/        Archive/
        //   Projects/
        let work = service
            .create_folder("Work".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let projects = service
            .create_folder(
                "Projects".to_string(),
                Some(work.id),
                owner_id,
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        let archive = service
            .create_folder("Archive".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Verify initial ancestor_ids
        assert_eq!(work.ancestor_ids, Some(Vec::new()));
        assert_eq!(projects.ancestor_ids, Some(vec![work.id]));
        assert_eq!(archive.ancestor_ids, Some(Vec::new()));

        // Move Work into Archive
        service
            .move_folder(work.id, Some(archive.id), owner_id)
            .await
            .unwrap();

        // Verify updated ancestor_ids
        let updated_work = metadata_store
            .find_folder_by_id(work.id)
            .await
            .unwrap()
            .unwrap();
        let updated_projects = metadata_store
            .find_folder_by_id(projects.id)
            .await
            .unwrap()
            .unwrap();

        // Work should now have Archive as ancestor
        assert_eq!(updated_work.ancestor_ids, Some(vec![archive.id]));
        // Projects should now have Archive and Work as ancestors
        assert_eq!(
            updated_projects.ancestor_ids,
            Some(vec![archive.id, work.id])
        );
    }

    #[tokio::test]
    async fn test_move_folder_to_root_updates_ancestor_ids() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(
            event_store,
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
        );

        let owner_id = Uuid::new_v4();

        // Create hierarchy: Documents/Work
        let docs = service
            .create_folder("Documents".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let work = service
            .create_folder("Work".to_string(), Some(docs.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Verify initial ancestor_ids
        assert_eq!(work.ancestor_ids, Some(vec![docs.id]));

        // Move Work to root (no parent)
        service.move_folder(work.id, None, owner_id).await.unwrap();

        // Verify updated ancestor_ids
        let updated_work = metadata_store
            .find_folder_by_id(work.id)
            .await
            .unwrap()
            .unwrap();

        // Work should now have empty ancestors (root folder)
        assert_eq!(updated_work.ancestor_ids, Some(Vec::new()));
    }

    struct MockPermissionOps;

    impl PermissionResolverOps for MockPermissionOps {
        async fn find_user_share(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<FolderId>,
            _recipient_user_id: UserId,
        ) -> Result<Option<Share>> {
            Ok(None)
        }

        async fn find_group_shares(
            &self,
            _file_id: Option<FileId>,
            _folder_id: Option<FolderId>,
            _group_ids: &[Uuid],
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }

        async fn find_user_shares_for_folders(
            &self,
            _folder_ids: &[Uuid],
            _recipient_user_id: UserId,
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }

        async fn find_group_shares_for_folders(
            &self,
            _folder_ids: &[Uuid],
            _group_ids: &[Uuid],
        ) -> Result<Vec<Share>> {
            Ok(Vec::new())
        }

        async fn find_file_by_id(&self, _id: FileId) -> Result<Option<File>> {
            Ok(None)
        }

        async fn find_folder_by_id(&self, _id: FolderId) -> Result<Option<Folder>> {
            Ok(None)
        }

        async fn get_user_group_ids(&self, _user_id: UserId) -> Result<Vec<Uuid>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn test_move_folder_circular_with_ancestor_ids() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let permission_ops = Arc::new(MockPermissionOps);
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops));

        let service = FolderService::new(
            event_store,
            metadata_store,
            Arc::new(EventBroadcaster::new(100)),
            permission_resolver,
        );

        let owner_id = Uuid::new_v4();

        // Create hierarchy with proper ancestor_ids: A/B/C
        let folder_a = service
            .create_folder("A".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let folder_b = service
            .create_folder("B".to_string(), Some(folder_a.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let folder_c = service
            .create_folder("C".to_string(), Some(folder_b.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Verify ancestor_ids are set correctly
        assert_eq!(folder_a.ancestor_ids, Some(Vec::new()));
        assert_eq!(folder_b.ancestor_ids, Some(vec![folder_a.id]));
        assert_eq!(folder_c.ancestor_ids, Some(vec![folder_a.id, folder_b.id]));

        // Try to move A into C - this should be detected as circular
        // because A is in C's ancestor_ids [A, B]
        let result = service
            .move_folder(folder_a.id, Some(folder_c.id), owner_id)
            .await;

        assert!(matches!(result, Err(FolderError::CircularReference { .. })));
    }

    #[tokio::test]
    async fn test_deep_nesting_ancestor_ids() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let permission_ops = Arc::new(MockPermissionOps);
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops));

        let service = FolderService::new(
            event_store,
            metadata_store.clone(),
            Arc::new(EventBroadcaster::new(100)),
            permission_resolver,
        );

        let owner_id = Uuid::new_v4();

        // Create deeply nested hierarchy: A/B/C/D/E
        let folder_a = service
            .create_folder("A".to_string(), None, owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let folder_b = service
            .create_folder("B".to_string(), Some(folder_a.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let folder_c = service
            .create_folder("C".to_string(), Some(folder_b.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let folder_d = service
            .create_folder("D".to_string(), Some(folder_c.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();
        let folder_e = service
            .create_folder("E".to_string(), Some(folder_d.id), owner_id, Uuid::new_v4())
            .await
            .unwrap();

        // Verify ancestor_ids at each level
        assert_eq!(folder_a.ancestor_ids, Some(Vec::new()));
        assert_eq!(folder_b.ancestor_ids, Some(vec![folder_a.id]));
        assert_eq!(folder_c.ancestor_ids, Some(vec![folder_a.id, folder_b.id]));
        assert_eq!(
            folder_d.ancestor_ids,
            Some(vec![folder_a.id, folder_b.id, folder_c.id])
        );
        assert_eq!(
            folder_e.ancestor_ids,
            Some(vec![folder_a.id, folder_b.id, folder_c.id, folder_d.id])
        );

        // Move C (with D and E) to root
        service
            .move_folder(folder_c.id, None, owner_id)
            .await
            .unwrap();

        // Verify ancestor_ids were updated for C and all descendants
        let updated_c = metadata_store
            .find_folder_by_id(folder_c.id)
            .await
            .unwrap()
            .unwrap();
        let updated_d = metadata_store
            .find_folder_by_id(folder_d.id)
            .await
            .unwrap()
            .unwrap();
        let updated_e = metadata_store
            .find_folder_by_id(folder_e.id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated_c.ancestor_ids, Some(Vec::new()));
        assert_eq!(updated_d.ancestor_ids, Some(vec![folder_c.id]));
        assert_eq!(updated_e.ancestor_ids, Some(vec![folder_c.id, folder_d.id]));
    }
}
