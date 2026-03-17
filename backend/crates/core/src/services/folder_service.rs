//! FolderService for folder management operations.
//!
//! This service handles folder operations including:
//! - Folder creation and validation
//! - Folder hierarchy management
//! - Folder tree navigation
//! - Event sourcing via EventStore
//! - Metadata persistence via MetadataStore

use anyhow::Result;
use std::sync::Arc;

use crate::domain::{File, Folder, FolderContents, FolderTree, FolderId, UserId};
use crate::events::{AggregateType, Event, EventType, FolderCreatedPayload};
use crate::services::FolderError;

/// Trait for event store operations needed by FolderService.
///
/// This trait abstracts the event store to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait EventStoreOps: Send + Sync {
    /// Append an event to the event store.
    async fn append(&self, event: &Event) -> Result<()>;
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
    async fn list_folders(&self, parent_id: Option<FolderId>, owner_id: UserId) -> Result<Vec<Folder>>;

    /// Find all descendant folders of a given folder using recursive CTE.
    async fn find_descendant_folders(&self, folder_id: FolderId) -> Result<Vec<Folder>>;

    /// List files with optional parent filter.
    async fn list_files(&self, parent_id: Option<FolderId>, owner_id: UserId) -> Result<Vec<File>>;
}

/// FolderService manages folder operations with event sourcing.
///
/// Generic over EventStore (E) and MetadataStore (M) implementations
/// to support both production and test environments.
pub struct FolderService<E, M>
where
    E: EventStoreOps,
    M: MetadataStoreOps,
{
    event_store: Arc<E>,
    metadata_store: Arc<M>,
}

impl<E, M> FolderService<E, M>
where
    E: EventStoreOps,
    M: MetadataStoreOps,
{
    /// Create a new FolderService instance.
    pub fn new(event_store: Arc<E>, metadata_store: Arc<M>) -> Self {
        Self {
            event_store,
            metadata_store,
        }
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
    ) -> Result<Folder, FolderError> {
        // Validate folder name
        self.validate_folder_name(&name)?;

        // Construct path based on parent folder
        let path = if let Some(parent_id) = parent_folder_id {
            // Verify parent folder exists and user has access
            let parent = self
                .metadata_store
                .find_folder_by_id(parent_id)
                .await
                .map_err(|e| FolderError::Database(sqlx::Error::Protocol(e.to_string())))?
                .ok_or(FolderError::ParentFolderNotFound(parent_id))?;

            // Verify ownership
            if parent.owner_id != owner_id {
                return Err(FolderError::PermissionDenied {
                    folder_id: parent_id,
                    user_id: owner_id,
                });
            }

            // Construct path: parent_path + "/" + name
            format!("{}/{}", parent.path.trim_end_matches('/'), name)
        } else {
            // Root folder
            format!("/{}", name)
        };

        // Create the folder domain object
        let folder = if let Some(parent_id) = parent_folder_id {
            Folder::new_child(name.clone(), path.clone(), parent_id, owner_id)
        } else {
            // Root-level folder (no parent)
            use uuid::Uuid;
            use chrono::Utc;
            Folder {
                id: Uuid::new_v4(),
                name: name.clone(),
                path: path.clone(),
                parent_folder_id: None,
                owner_id,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }
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
            serde_json::to_value(payload).map_err(|e| FolderError::Database(sqlx::Error::Decode(Box::new(e))))?,
            owner_id,
        );

        self.event_store
            .append(&event)
            .await
            .map_err(|e| FolderError::Database(sqlx::Error::Protocol(format!("Failed to append event: {}", e))))?;

        // Insert into metadata store
        self.metadata_store
            .create_folder(&folder)
            .await
            .map_err(|e| FolderError::Database(sqlx::Error::Protocol(e.to_string())))?;

        Ok(folder)
    }

    /// Get a folder by ID.
    ///
    /// Verifies that the folder exists and that the user has permission to access it.
    pub async fn get_folder(&self, folder_id: FolderId, user_id: UserId) -> Result<Folder, FolderError> {
        let folder = self
            .metadata_store
            .find_folder_by_id(folder_id)
            .await
            .map_err(|e| FolderError::Database(sqlx::Error::Protocol(e.to_string())))?
            .ok_or(FolderError::NotFound(folder_id))?;

        // Verify ownership
        if folder.owner_id != user_id {
            return Err(FolderError::PermissionDenied {
                folder_id,
                user_id,
            });
        }

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

        // Get files in this folder
        let files = self
            .metadata_store
            .list_files(Some(folder.id), user_id)
            .await
            .map_err(|e| FolderError::Database(sqlx::Error::Protocol(e.to_string())))?;

        // Get subfolders in this folder
        let folders = self
            .metadata_store
            .list_folders(Some(folder.id), user_id)
            .await
            .map_err(|e| FolderError::Database(sqlx::Error::Protocol(e.to_string())))?;

        Ok(FolderContents::with_contents(files, folders))
    }

    /// Get a recursive tree structure of a folder and all its descendants.
    ///
    /// Returns a FolderTree containing the folder, all its files, and recursively
    /// all its subfolders with their files.
    pub async fn get_tree(&self, folder_id: FolderId, user_id: UserId) -> Result<FolderTree, FolderError> {
        // Verify folder exists and user has access
        let folder = self.get_folder(folder_id, user_id).await?;

        // Build the tree recursively
        self.build_tree_internal(folder, user_id).await
    }

    /// Internal recursive method to build folder tree.
    ///
    /// This is a separate method to allow for Box::pin recursion.
    async fn build_tree_internal(&self, folder: Folder, user_id: UserId) -> Result<FolderTree, FolderError> {
        // Get files in this folder
        let files = self
            .metadata_store
            .list_files(Some(folder.id), user_id)
            .await
            .map_err(|e| FolderError::Database(sqlx::Error::Protocol(e.to_string())))?;

        // Get immediate subfolders
        let subfolders = self
            .metadata_store
            .list_folders(Some(folder.id), user_id)
            .await
            .map_err(|e| FolderError::Database(sqlx::Error::Protocol(e.to_string())))?;

        // Recursively build trees for each subfolder
        let mut subfolder_trees = Vec::new();
        for subfolder in subfolders {
            let subtree = Box::pin(self.build_tree_internal(subfolder, user_id)).await?;
            subfolder_trees.push(subtree);
        }

        Ok(FolderTree::with_contents(folder, files, subfolder_trees))
    }

    /// Validate folder name.
    ///
    /// Ensures the name is not empty and does not contain illegal characters.
    fn validate_folder_name(&self, name: &str) -> Result<(), FolderError> {
        if name.is_empty() {
            return Err(FolderError::InvalidName("Folder name cannot be empty".to_string()));
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
        async fn append(&self, event: &Event) -> Result<()> {
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
            self.folders.lock().unwrap().insert(folder.id, folder.clone());
            Ok(())
        }

        async fn find_folder_by_id(&self, id: FolderId) -> Result<Option<Folder>> {
            Ok(self.folders.lock().unwrap().get(&id).cloned())
        }

        async fn update_folder(&self, folder: &Folder) -> Result<()> {
            self.folders.lock().unwrap().insert(folder.id, folder.clone());
            Ok(())
        }

        async fn delete_folder(&self, id: FolderId) -> Result<()> {
            self.folders.lock().unwrap().remove(&id);
            Ok(())
        }

        async fn list_folders(&self, parent_id: Option<FolderId>, owner_id: UserId) -> Result<Vec<Folder>> {
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

        async fn list_files(&self, parent_id: Option<FolderId>, _owner_id: UserId) -> Result<Vec<File>> {
            let files = self.files.lock().unwrap();
            Ok(files.get(&parent_id.unwrap_or_default()).cloned().unwrap_or_default())
        }
    }

    #[tokio::test]
    async fn test_create_folder_success() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(event_store.clone(), metadata_store.clone());

        let owner_id = Uuid::new_v4();
        let folder = service.create_folder("Documents".to_string(), None, owner_id).await.unwrap();

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
        let service = FolderService::new(event_store.clone(), metadata_store.clone());

        let owner_id = Uuid::new_v4();

        // Create parent folder
        let parent = service.create_folder("Documents".to_string(), None, owner_id).await.unwrap();

        // Create subfolder
        let subfolder = service
            .create_folder("Work".to_string(), Some(parent.id), owner_id)
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
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();
        let result = service.create_folder("".to_string(), None, owner_id).await;

        assert!(matches!(result, Err(FolderError::InvalidName(_))));
    }

    #[tokio::test]
    async fn test_create_folder_invalid_name_with_slash() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();
        let result = service.create_folder("Work/Projects".to_string(), None, owner_id).await;

        assert!(matches!(result, Err(FolderError::InvalidName(_))));
    }

    #[tokio::test]
    async fn test_create_folder_parent_not_found() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();
        let non_existent_parent_id = Uuid::new_v4();
        let result = service
            .create_folder("Work".to_string(), Some(non_existent_parent_id), owner_id)
            .await;

        assert!(matches!(result, Err(FolderError::ParentFolderNotFound(_))));
    }

    #[tokio::test]
    async fn test_create_folder_permission_denied() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(event_store, metadata_store.clone());

        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();

        // Create parent folder as owner
        let parent = service.create_folder("Documents".to_string(), None, owner_id).await.unwrap();

        // Try to create subfolder as different user
        let result = service
            .create_folder("Work".to_string(), Some(parent.id), other_user_id)
            .await;

        assert!(matches!(result, Err(FolderError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_get_folder_success() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();
        let created = service.create_folder("Documents".to_string(), None, owner_id).await.unwrap();

        let retrieved = service.get_folder(created.id, owner_id).await.unwrap();

        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.name, "Documents");
        assert_eq!(retrieved.owner_id, owner_id);
    }

    #[tokio::test]
    async fn test_get_folder_not_found() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();
        let non_existent_id = Uuid::new_v4();
        let result = service.get_folder(non_existent_id, owner_id).await;

        assert!(matches!(result, Err(FolderError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_get_folder_permission_denied() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let created = service.create_folder("Documents".to_string(), None, owner_id).await.unwrap();

        let result = service.get_folder(created.id, other_user_id).await;

        assert!(matches!(result, Err(FolderError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_list_contents_empty_folder() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();
        let folder = service.create_folder("Documents".to_string(), None, owner_id).await.unwrap();

        let contents = service.list_contents(folder.id, owner_id).await.unwrap();

        assert_eq!(contents.files.len(), 0);
        assert_eq!(contents.folders.len(), 0);
    }

    #[tokio::test]
    async fn test_list_contents_with_subfolders() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();

        // Create parent folder
        let parent = service.create_folder("Documents".to_string(), None, owner_id).await.unwrap();

        // Create subfolders
        let _subfolder1 = service
            .create_folder("Work".to_string(), Some(parent.id), owner_id)
            .await
            .unwrap();
        let _subfolder2 = service
            .create_folder("Personal".to_string(), Some(parent.id), owner_id)
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
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let folder = service.create_folder("Documents".to_string(), None, owner_id).await.unwrap();

        let result = service.list_contents(folder.id, other_user_id).await;

        assert!(matches!(result, Err(FolderError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_get_tree_single_folder() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();
        let folder = service.create_folder("Documents".to_string(), None, owner_id).await.unwrap();

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
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();

        // Create folder hierarchy:
        // Documents/
        //   Work/
        //     Projects/
        //   Personal/
        let root = service.create_folder("Documents".to_string(), None, owner_id).await.unwrap();
        let work = service
            .create_folder("Work".to_string(), Some(root.id), owner_id)
            .await
            .unwrap();
        let _projects = service
            .create_folder("Projects".to_string(), Some(work.id), owner_id)
            .await
            .unwrap();
        let _personal = service
            .create_folder("Personal".to_string(), Some(root.id), owner_id)
            .await
            .unwrap();

        let tree = service.get_tree(root.id, owner_id).await.unwrap();

        assert_eq!(tree.folder.name, "Documents");
        assert_eq!(tree.subfolders.len(), 2);

        // Check Work subfolder
        let work_tree = tree.subfolders.iter().find(|t| t.folder.name == "Work").unwrap();
        assert_eq!(work_tree.subfolders.len(), 1);
        assert_eq!(work_tree.subfolders[0].folder.name, "Projects");

        // Check Personal subfolder
        let personal_tree = tree.subfolders.iter().find(|t| t.folder.name == "Personal").unwrap();
        assert_eq!(personal_tree.subfolders.len(), 0);
    }

    #[tokio::test]
    async fn test_get_tree_permission_denied() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let service = FolderService::new(event_store, metadata_store);

        let owner_id = Uuid::new_v4();
        let other_user_id = Uuid::new_v4();
        let folder = service.create_folder("Documents".to_string(), None, owner_id).await.unwrap();

        let result = service.get_tree(folder.id, other_user_id).await;

        assert!(matches!(result, Err(FolderError::PermissionDenied { .. })));
    }
}
