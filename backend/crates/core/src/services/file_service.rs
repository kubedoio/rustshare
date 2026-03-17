//! FileService for file upload and management operations.
//!
//! This service handles file uploads, including:
//! - File name validation
//! - SHA256 content hashing for deduplication
//! - S3 object storage
//! - Event sourcing via EventStore
//! - Metadata persistence via MetadataStore

use anyhow::Result;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::domain::{File, FileVersion, Folder, FolderId, UserId};
use crate::events::{
    AggregateType, Event, EventType, FileDeletedPayload, FileModifiedPayload, FileMovedPayload,
    FileRenamedPayload, FileUploadedPayload,
};
use crate::services::FileError;

/// Trait for event store operations needed by FileService.
///
/// This trait abstracts the event store to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait EventStoreOps: Send + Sync {
    /// Append an event to the event store.
    async fn append(&self, event: &Event) -> Result<()>;
}

/// Trait for metadata store operations needed by FileService.
///
/// This trait abstracts the metadata store to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait MetadataStoreOps: Send + Sync {
    /// Create a file in the metadata store.
    async fn create_file(&self, file: &File) -> Result<()>;

    /// Create a file version in the metadata store.
    async fn create_file_version(&self, version: &FileVersion) -> Result<()>;

    /// Find a folder by ID.
    async fn find_folder_by_id(&self, id: uuid::Uuid) -> Result<Option<Folder>>;

    /// Find a file by ID.
    async fn find_file_by_id(&self, id: uuid::Uuid) -> Result<Option<File>>;

    /// Update a file in the metadata store.
    async fn update_file(&self, file: &File) -> Result<()>;

    /// Delete a file from the metadata store.
    async fn delete_file(&self, id: uuid::Uuid) -> Result<()>;

    /// List all versions of a file, ordered by version number descending.
    async fn list_file_versions(&self, file_id: uuid::Uuid) -> Result<Vec<FileVersion>>;

    /// Find a specific version of a file.
    async fn find_file_version(
        &self,
        file_id: uuid::Uuid,
        version_number: i32,
    ) -> Result<Option<FileVersion>>;
}

/// Trait for object store operations needed by FileService.
///
/// This trait abstracts S3/object storage to allow for testing without S3 dependencies.
#[allow(async_fn_in_trait)]
pub trait ObjectStoreOps: Send + Sync {
    /// Upload data to object storage.
    async fn put(&self, key: &str, data: Bytes) -> Result<()>;

    /// Check if an object exists.
    async fn exists(&self, key: &str) -> Result<bool>;

    /// Get a presigned URL for downloading an object.
    ///
    /// # Arguments
    /// * `key` - The object key
    /// * `expiry_secs` - URL expiration time in seconds
    ///
    /// # Returns
    /// A presigned URL string valid for the specified duration.
    async fn get_presigned_url(&self, key: &str, expiry_secs: u64) -> Result<String>;

    /// Download content from object storage.
    ///
    /// # Arguments
    /// * `key` - The object key
    ///
    /// # Returns
    /// The content as Bytes.
    async fn get(&self, key: &str) -> Result<Bytes>;
}

/// File service for handling file operations.
pub struct FileService<E, M, O>
where
    E: EventStoreOps,
    M: MetadataStoreOps,
    O: ObjectStoreOps,
{
    event_store: Arc<E>,
    metadata_store: Arc<M>,
    object_store: Arc<O>,
}

impl<E, M, O> FileService<E, M, O>
where
    E: EventStoreOps,
    M: MetadataStoreOps,
    O: ObjectStoreOps,
{
    /// Create a new FileService with the given stores.
    pub fn new(event_store: Arc<E>, metadata_store: Arc<M>, object_store: Arc<O>) -> Self {
        Self {
            event_store,
            metadata_store,
            object_store,
        }
    }

    /// Upload a new file.
    ///
    /// # Arguments
    /// * `owner_id` - The user uploading the file
    /// * `name` - The file name (must not contain `/` or `\0`, must not be empty)
    /// * `parent_folder_id` - Optional parent folder (None for root)
    /// * `content` - The file content as bytes
    /// * `mime_type` - The MIME type of the file
    ///
    /// # Returns
    /// The created File domain object with version 1.
    ///
    /// # Errors
    /// - `FileError::InvalidName` if the file name is invalid
    /// - `FileError::ParentFolderNotFound` if the parent folder doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't own the parent folder
    /// - `FileError::Storage` if S3 upload fails
    /// - `FileError::Database` if database operations fail
    pub async fn upload_file(
        &self,
        owner_id: UserId,
        name: String,
        parent_folder_id: Option<FolderId>,
        content: Bytes,
        mime_type: String,
    ) -> Result<File, FileError> {
        // 1. Validate file name
        self.validate_file_name(&name)?;

        // 2. Calculate SHA256 hash of content
        let content_hash = self.calculate_sha256(&content);

        // 3. Check parent folder exists (if provided) and verify ownership
        let parent_path = if let Some(folder_id) = parent_folder_id {
            let folder = self
                .metadata_store
                .find_folder_by_id(folder_id)
                .await
                .map_err(|e| FileError::Database(sqlx::Error::Protocol(e.to_string())))?
                .ok_or(FileError::ParentFolderNotFound(folder_id))?;

            // Verify the user owns this folder
            if folder.owner_id != owner_id {
                return Err(FileError::PermissionDenied {
                    file_id: uuid::Uuid::nil(), // No file yet
                    user_id: owner_id,
                });
            }

            folder.path.clone()
        } else {
            String::new()
        };

        // 4. Construct path from parent path + name
        let path = if parent_path.is_empty() || parent_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent_path, name)
        };

        // 5. Upload to S3 at "blobs/{hash}" (skip if exists - deduplication)
        let storage_key = format!("blobs/{}", content_hash);
        let blob_exists = self
            .object_store
            .exists(&storage_key)
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        if !blob_exists {
            self.object_store
                .put(&storage_key, content.clone())
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        }

        // 6. Create File domain object with version=1
        let size = content.len() as i64;
        let file = File::new(
            name.clone(),
            path.clone(),
            content_hash.clone(),
            size,
            mime_type.clone(),
            parent_folder_id,
            owner_id,
        );

        // 7. Emit FileUploaded event to EventStore
        let payload = FileUploadedPayload {
            file_id: file.id,
            name: name.clone(),
            path: path.clone(),
            size,
            content_hash: content_hash.clone(),
            storage_key: storage_key.clone(),
            mime_type: mime_type.clone(),
            owner_id,
            parent_folder_id,
        };
        let payload_json = serde_json::to_value(&payload)
            .map_err(|e| FileError::Storage(format!("Failed to serialize payload: {}", e)))?;

        let event = Event::new(
            EventType::FileUploaded,
            file.id,
            AggregateType::File,
            payload_json,
            owner_id,
        );

        self.event_store
            .append(&event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;

        // 8. Insert into files and file_versions tables
        self.metadata_store
            .create_file(&file)
            .await
            .map_err(|e| FileError::Database(sqlx::Error::Protocol(e.to_string())))?;

        // Create version 1 entry
        let version = FileVersion::new(
            file.id,
            1,
            content_hash,
            size,
            owner_id,
            Some("Initial upload".to_string()),
        );

        self.metadata_store
            .create_file_version(&version)
            .await
            .map_err(|e| FileError::Database(sqlx::Error::Protocol(e.to_string())))?;

        // 9. Return File
        Ok(file)
    }

    /// Get a file by ID, verifying ownership.
    ///
    /// # Arguments
    /// * `file_id` - The ID of the file to retrieve
    /// * `user_id` - The ID of the user requesting the file
    ///
    /// # Returns
    /// The File domain object if found and owned by the user.
    ///
    /// # Errors
    /// - `FileError::NotFound` if the file doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't own the file
    pub async fn get_file(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
    ) -> Result<File, FileError> {
        // Find file by ID
        let file = self
            .metadata_store
            .find_file_by_id(file_id)
            .await
            .map_err(|e| FileError::Database(sqlx::Error::Protocol(e.to_string())))?
            .ok_or(FileError::NotFound(file_id))?;

        // Verify ownership
        if file.owner_id != user_id {
            return Err(FileError::PermissionDenied { file_id, user_id });
        }

        Ok(file)
    }

    /// Get a presigned download URL for a file.
    ///
    /// # Arguments
    /// * `file_id` - The ID of the file to download
    /// * `user_id` - The ID of the user requesting the download
    ///
    /// # Returns
    /// A presigned S3 URL valid for 1 hour.
    ///
    /// # Errors
    /// - `FileError::NotFound` if the file doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't own the file
    /// - `FileError::Storage` if URL generation fails
    pub async fn get_download_url(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
    ) -> Result<String, FileError> {
        // Use get_file for permission check
        let file = self.get_file(file_id, user_id).await?;

        // Generate presigned URL (1 hour = 3600 seconds)
        let storage_key = file.storage_key();
        let url = self
            .object_store
            .get_presigned_url(&storage_key, 3600)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to generate presigned URL: {}", e)))?;

        Ok(url)
    }

    /// Update a file's content with optimistic locking.
    ///
    /// # Arguments
    /// * `file_id` - The ID of the file to update
    /// * `user_id` - The ID of the user performing the update
    /// * `expected_version` - The version the client expects the file to be at (optimistic lock)
    /// * `content` - The new file content
    ///
    /// # Returns
    /// The updated File domain object with incremented version.
    ///
    /// # Errors
    /// - `FileError::NotFound` if the file doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't own the file
    /// - `FileError::VersionConflict` if current_version != expected_version
    /// - `FileError::Storage` if S3 upload fails
    /// - `FileError::Database` if database operations fail
    pub async fn update_file(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
        expected_version: i32,
        content: Bytes,
    ) -> Result<File, FileError> {
        // 1. Get current file and verify ownership
        let mut file = self.get_file(file_id, user_id).await?;

        // 2. Check optimistic lock (current_version == expected_version)
        if file.current_version != expected_version {
            return Err(FileError::VersionConflict {
                expected: expected_version,
                actual: file.current_version,
                current_modified_by: file.owner_id.to_string(),
                current_modified_at: file.modified_at.to_rfc3339(),
            });
        }

        // 3. Calculate new content hash
        let old_content_hash = file.content_hash.clone();
        let old_size = file.size;
        let new_content_hash = self.calculate_sha256(&content);
        let new_size = content.len() as i64;

        // 4. Upload to S3 (skip if same content - deduplication)
        let storage_key = format!("blobs/{}", new_content_hash);
        let blob_exists = self
            .object_store
            .exists(&storage_key)
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        if !blob_exists {
            self.object_store
                .put(&storage_key, content)
                .await
                .map_err(|e| FileError::Storage(e.to_string()))?;
        }

        // 5. Increment version and update file record
        let old_version = file.current_version;
        file.current_version += 1;
        file.content_hash = new_content_hash.clone();
        file.size = new_size;
        file.modified_at = chrono::Utc::now();

        self.metadata_store
            .update_file(&file)
            .await
            .map_err(|e| FileError::Database(sqlx::Error::Protocol(e.to_string())))?;

        // 6. Create FileVersion snapshot
        let version = FileVersion::new(
            file.id,
            file.current_version,
            new_content_hash.clone(),
            new_size,
            user_id,
            Some(format!("Updated from version {}", old_version)),
        );

        self.metadata_store
            .create_file_version(&version)
            .await
            .map_err(|e| FileError::Database(sqlx::Error::Protocol(e.to_string())))?;

        // 7. Emit FileModified event
        let payload = FileModifiedPayload {
            file_id: file.id,
            old_version,
            new_version: file.current_version,
            old_content_hash,
            new_content_hash,
            old_size,
            new_size,
            storage_key,
            modified_by: user_id,
        };
        let payload_json = serde_json::to_value(&payload)
            .map_err(|e| FileError::Storage(format!("Failed to serialize payload: {}", e)))?;

        let event = Event::new(
            EventType::FileModified,
            file.id,
            AggregateType::File,
            payload_json,
            user_id,
        );

        self.event_store
            .append(&event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;

        // 8. Return updated file
        Ok(file)
    }

    /// Delete a file.
    ///
    /// # Arguments
    /// * `file_id` - The ID of the file to delete
    /// * `user_id` - The ID of the user performing the deletion
    ///
    /// # Returns
    /// Ok(()) if the file was deleted successfully.
    ///
    /// # Errors
    /// - `FileError::NotFound` if the file doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't own the file
    /// - `FileError::Database` if database operations fail
    ///
    /// # Note
    /// The blob in S3 is NOT deleted (content-addressed storage may be shared).
    pub async fn delete_file(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
    ) -> Result<(), FileError> {
        // 1. Get file and verify ownership
        let file = self.get_file(file_id, user_id).await?;

        // 2. Emit FileDeleted event
        let payload = FileDeletedPayload {
            file_id: file.id,
            name: file.name.clone(),
            path: file.path.clone(),
            deleted_by: user_id,
        };
        let payload_json = serde_json::to_value(&payload)
            .map_err(|e| FileError::Storage(format!("Failed to serialize payload: {}", e)))?;

        let event = Event::new(
            EventType::FileDeleted,
            file.id,
            AggregateType::File,
            payload_json,
            user_id,
        );

        self.event_store
            .append(&event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;

        // 3. Delete from metadata store
        self.metadata_store
            .delete_file(file_id)
            .await
            .map_err(|e| FileError::Database(sqlx::Error::Protocol(e.to_string())))?;

        Ok(())
    }

    /// Move a file to a different folder.
    ///
    /// # Arguments
    /// * `file_id` - The ID of the file to move
    /// * `new_parent_id` - The ID of the new parent folder (None for root)
    /// * `user_id` - The ID of the user performing the move
    ///
    /// # Returns
    /// The updated File domain object.
    ///
    /// # Errors
    /// - `FileError::NotFound` if the file doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't own the file or new parent
    /// - `FileError::ParentFolderNotFound` if the new parent folder doesn't exist
    /// - `FileError::Database` if database operations fail
    pub async fn move_file(
        &self,
        file_id: uuid::Uuid,
        new_parent_id: Option<FolderId>,
        user_id: UserId,
    ) -> Result<File, FileError> {
        // 1. Get file and verify ownership
        let mut file = self.get_file(file_id, user_id).await?;
        let old_parent_folder_id = file.parent_folder_id;
        let old_path = file.path.clone();

        // 2. Check new parent exists and verify ownership (if not root)
        let new_parent_path = if let Some(parent_id) = new_parent_id {
            let parent = self
                .metadata_store
                .find_folder_by_id(parent_id)
                .await
                .map_err(|e| FileError::Database(sqlx::Error::Protocol(e.to_string())))?
                .ok_or(FileError::ParentFolderNotFound(parent_id))?;

            // Verify the user owns the new parent folder
            if parent.owner_id != user_id {
                return Err(FileError::PermissionDenied {
                    file_id: parent_id, // Using parent_id here to indicate the folder permission issue
                    user_id,
                });
            }

            parent.path.clone()
        } else {
            String::new()
        };

        // 3. Compute new path
        let new_path = if new_parent_path.is_empty() || new_parent_path == "/" {
            format!("/{}", file.name)
        } else {
            format!("{}/{}", new_parent_path, file.name)
        };

        // 4. Update file
        file.parent_folder_id = new_parent_id;
        file.path = new_path.clone();
        file.modified_at = chrono::Utc::now();

        self.metadata_store
            .update_file(&file)
            .await
            .map_err(|e| FileError::Database(sqlx::Error::Protocol(e.to_string())))?;

        // 5. Emit FileMoved event
        let payload = FileMovedPayload {
            file_id: file.id,
            old_parent_folder_id,
            new_parent_folder_id: new_parent_id,
            old_path,
            new_path,
            moved_by: user_id,
        };
        let payload_json = serde_json::to_value(&payload)
            .map_err(|e| FileError::Storage(format!("Failed to serialize payload: {}", e)))?;

        let event = Event::new(
            EventType::FileMoved,
            file.id,
            AggregateType::File,
            payload_json,
            user_id,
        );

        self.event_store
            .append(&event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;

        // 6. Return updated file
        Ok(file)
    }

    /// Rename a file.
    ///
    /// # Arguments
    /// * `file_id` - The ID of the file to rename
    /// * `new_name` - The new name for the file
    /// * `user_id` - The ID of the user performing the rename
    ///
    /// # Returns
    /// The updated File domain object.
    ///
    /// # Errors
    /// - `FileError::NotFound` if the file doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't own the file
    /// - `FileError::InvalidName` if the new name is invalid
    /// - `FileError::Database` if database operations fail
    pub async fn rename_file(
        &self,
        file_id: uuid::Uuid,
        new_name: String,
        user_id: UserId,
    ) -> Result<File, FileError> {
        // 1. Validate new name
        self.validate_file_name(&new_name)?;

        // 2. Get file and verify ownership
        let mut file = self.get_file(file_id, user_id).await?;
        let old_name = file.name.clone();
        let old_path = file.path.clone();

        // 3. Compute new path (replace last segment)
        let new_path = if let Some(parent_path_end) = old_path.rfind('/') {
            let parent_path = &old_path[..parent_path_end];
            if parent_path.is_empty() {
                format!("/{}", new_name)
            } else {
                format!("{}/{}", parent_path, new_name)
            }
        } else {
            format!("/{}", new_name)
        };

        // 4. Update file
        file.name = new_name.clone();
        file.path = new_path.clone();
        file.modified_at = chrono::Utc::now();

        self.metadata_store
            .update_file(&file)
            .await
            .map_err(|e| FileError::Database(sqlx::Error::Protocol(e.to_string())))?;

        // 5. Emit FileRenamed event
        let payload = FileRenamedPayload {
            file_id: file.id,
            old_name,
            new_name,
            old_path,
            new_path,
            renamed_by: user_id,
        };
        let payload_json = serde_json::to_value(&payload)
            .map_err(|e| FileError::Storage(format!("Failed to serialize payload: {}", e)))?;

        let event = Event::new(
            EventType::FileRenamed,
            file.id,
            AggregateType::File,
            payload_json,
            user_id,
        );

        self.event_store
            .append(&event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;

        // 6. Return updated file
        Ok(file)
    }

    fn validate_file_name(&self, name: &str) -> Result<(), FileError> {
        if name.is_empty() {
            return Err(FileError::InvalidName("File name cannot be empty".to_string()));
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

    /// Calculate SHA256 hash of content and return as hex string.
    fn calculate_sha256(&self, content: &Bytes) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let result = hasher.finalize();
        hex::encode(result)
    }
}

// Helper for hex encoding (avoiding an extra dependency)
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // Mock implementations for testing

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

    struct MockMetadataStore {
        files: Mutex<Vec<File>>,
        versions: Mutex<Vec<FileVersion>>,
        folders: Mutex<HashMap<uuid::Uuid, Folder>>,
    }

    impl MockMetadataStore {
        fn new() -> Self {
            Self {
                files: Mutex::new(Vec::new()),
                versions: Mutex::new(Vec::new()),
                folders: Mutex::new(HashMap::new()),
            }
        }

        fn add_folder(&self, folder: Folder) {
            self.folders.lock().unwrap().insert(folder.id, folder);
        }

        fn add_file(&self, file: File) {
            self.files.lock().unwrap().push(file);
        }

        fn add_file_version(&self, version: FileVersion) {
            self.versions.lock().unwrap().push(version);
        }
    }

    impl MetadataStoreOps for MockMetadataStore {
        async fn create_file(&self, file: &File) -> Result<()> {
            self.files.lock().unwrap().push(file.clone());
            Ok(())
        }

        async fn create_file_version(&self, version: &FileVersion) -> Result<()> {
            self.versions.lock().unwrap().push(version.clone());
            Ok(())
        }

        async fn find_folder_by_id(&self, id: uuid::Uuid) -> Result<Option<Folder>> {
            Ok(self.folders.lock().unwrap().get(&id).cloned())
        }

        async fn find_file_by_id(&self, id: uuid::Uuid) -> Result<Option<File>> {
            Ok(self.files.lock().unwrap().iter().find(|f| f.id == id).cloned())
        }

        async fn update_file(&self, file: &File) -> Result<()> {
            let mut files = self.files.lock().unwrap();
            if let Some(existing) = files.iter_mut().find(|f| f.id == file.id) {
                *existing = file.clone();
            }
            Ok(())
        }

        async fn delete_file(&self, id: uuid::Uuid) -> Result<()> {
            let mut files = self.files.lock().unwrap();
            files.retain(|f| f.id != id);
            Ok(())
        }

        async fn list_file_versions(&self, file_id: uuid::Uuid) -> Result<Vec<FileVersion>> {
            let versions = self.versions.lock().unwrap();
            let mut result: Vec<_> = versions
                .iter()
                .filter(|v| v.file_id == file_id)
                .cloned()
                .collect();
            // Sort by version number descending
            result.sort_by(|a, b| b.version_number.cmp(&a.version_number));
            Ok(result)
        }

        async fn find_file_version(
            &self,
            file_id: uuid::Uuid,
            version_number: i32,
        ) -> Result<Option<FileVersion>> {
            let versions = self.versions.lock().unwrap();
            Ok(versions
                .iter()
                .find(|v| v.file_id == file_id && v.version_number == version_number)
                .cloned())
        }
    }

    struct MockObjectStore {
        objects: Mutex<HashMap<String, Bytes>>,
    }

    impl MockObjectStore {
        fn new() -> Self {
            Self {
                objects: Mutex::new(HashMap::new()),
            }
        }
    }

    impl ObjectStoreOps for MockObjectStore {
        async fn put(&self, key: &str, data: Bytes) -> Result<()> {
            self.objects.lock().unwrap().insert(key.to_string(), data);
            Ok(())
        }

        async fn exists(&self, key: &str) -> Result<bool> {
            Ok(self.objects.lock().unwrap().contains_key(key))
        }

        async fn get_presigned_url(&self, key: &str, expiry_secs: u64) -> Result<String> {
            // Mock presigned URL generation
            Ok(format!("https://mock-s3.example.com/{}?expiry={}", key, expiry_secs))
        }

        async fn get(&self, key: &str) -> Result<Bytes> {
            let objects = self.objects.lock().unwrap();
            objects
                .get(key)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Object not found: {}", key))
        }
    }

    fn setup_file_service() -> (
        FileService<MockEventStore, MockMetadataStore, MockObjectStore>,
        Arc<MockEventStore>,
        Arc<MockMetadataStore>,
        Arc<MockObjectStore>,
    ) {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let object_store = Arc::new(MockObjectStore::new());

        let service = FileService::new(
            event_store.clone(),
            metadata_store.clone(),
            object_store.clone(),
        );

        (service, event_store, metadata_store, object_store)
    }

    #[tokio::test]
    async fn test_upload_file() {
        let (service, event_store, metadata_store, object_store) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let content = Bytes::from("Hello World");

        let file = service
            .upload_file(
                owner_id,
                "test.txt".into(),
                None, // root folder
                content.clone(),
                "text/plain".into(),
            )
            .await
            .unwrap();

        assert_eq!(file.name, "test.txt");
        assert_eq!(file.path, "/test.txt");
        assert_eq!(file.size, 11);
        assert_eq!(file.current_version, 1);
        assert_eq!(file.owner_id, owner_id);
        assert_eq!(file.mime_type, "text/plain");

        // Verify event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FileUploaded);

        // Verify file was created in metadata store
        let files = metadata_store.files.lock().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].id, file.id);

        // Verify version was created
        let versions = metadata_store.versions.lock().unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version_number, 1);
        assert_eq!(versions[0].file_id, file.id);

        // Verify blob was uploaded to object store
        let objects = object_store.objects.lock().unwrap();
        assert_eq!(objects.len(), 1);
        assert!(objects.contains_key(&format!("blobs/{}", file.content_hash)));
    }

    #[tokio::test]
    async fn test_upload_file_to_folder() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create a folder
        let folder = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            uuid::Uuid::new_v4(), // parent (doesn't matter for this test)
            owner_id,
        );
        metadata_store.add_folder(folder.clone());

        let content = Bytes::from("Test content");
        let file = service
            .upload_file(
                owner_id,
                "report.pdf".into(),
                Some(folder.id),
                content,
                "application/pdf".into(),
            )
            .await
            .unwrap();

        assert_eq!(file.name, "report.pdf");
        assert_eq!(file.path, "/Documents/report.pdf");
        assert_eq!(file.parent_folder_id, Some(folder.id));
    }

    #[tokio::test]
    async fn test_upload_file_invalid_name_empty() {
        let (service, _, _, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let content = Bytes::from("content");

        let result = service
            .upload_file(owner_id, "".into(), None, content, "text/plain".into())
            .await;

        assert!(matches!(result, Err(FileError::InvalidName(_))));
    }

    #[tokio::test]
    async fn test_upload_file_invalid_name_slash() {
        let (service, _, _, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let content = Bytes::from("content");

        let result = service
            .upload_file(
                owner_id,
                "path/to/file.txt".into(),
                None,
                content,
                "text/plain".into(),
            )
            .await;

        assert!(matches!(result, Err(FileError::InvalidName(_))));
    }

    #[tokio::test]
    async fn test_upload_file_invalid_name_null() {
        let (service, _, _, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let content = Bytes::from("content");

        let result = service
            .upload_file(
                owner_id,
                "file\0name.txt".into(),
                None,
                content,
                "text/plain".into(),
            )
            .await;

        assert!(matches!(result, Err(FileError::InvalidName(_))));
    }

    #[tokio::test]
    async fn test_upload_file_parent_folder_not_found() {
        let (service, _, _, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let content = Bytes::from("content");
        let non_existent_folder = uuid::Uuid::new_v4();

        let result = service
            .upload_file(
                owner_id,
                "test.txt".into(),
                Some(non_existent_folder),
                content,
                "text/plain".into(),
            )
            .await;

        assert!(matches!(result, Err(FileError::ParentFolderNotFound(_))));
    }

    #[tokio::test]
    async fn test_upload_file_permission_denied() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let other_user = uuid::Uuid::new_v4();

        // Create a folder owned by a different user
        let folder = Folder::new_child(
            "OtherUserFolder".to_string(),
            "/OtherUserFolder".to_string(),
            uuid::Uuid::new_v4(),
            other_user,
        );
        metadata_store.add_folder(folder.clone());

        let content = Bytes::from("content");
        let result = service
            .upload_file(
                owner_id,
                "test.txt".into(),
                Some(folder.id),
                content,
                "text/plain".into(),
            )
            .await;

        assert!(matches!(result, Err(FileError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_upload_file_deduplication() {
        let (service, _, _, object_store) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let content = Bytes::from("Duplicate content");

        // Upload first file
        let file1 = service
            .upload_file(
                owner_id,
                "file1.txt".into(),
                None,
                content.clone(),
                "text/plain".into(),
            )
            .await
            .unwrap();

        // Upload second file with same content
        let file2 = service
            .upload_file(
                owner_id,
                "file2.txt".into(),
                None,
                content.clone(),
                "text/plain".into(),
            )
            .await
            .unwrap();

        // Both files should have the same content hash
        assert_eq!(file1.content_hash, file2.content_hash);

        // Only one blob should exist in object store (deduplication)
        let objects = object_store.objects.lock().unwrap();
        assert_eq!(objects.len(), 1);
    }

    #[tokio::test]
    async fn test_sha256_hash_calculation() {
        let (service, _, _, _) = setup_file_service();

        // Known SHA256 hash for "Hello World"
        // SHA256("Hello World") = a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e
        let content = Bytes::from("Hello World");
        let hash = service.calculate_sha256(&content);

        assert_eq!(
            hash,
            "a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e"
        );
    }

    #[test]
    fn test_validate_file_name_valid() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let object_store = Arc::new(MockObjectStore::new());
        let service = FileService::new(event_store, metadata_store, object_store);

        assert!(service.validate_file_name("test.txt").is_ok());
        assert!(service.validate_file_name("my-file.pdf").is_ok());
        assert!(service.validate_file_name("file with spaces.doc").is_ok());
        assert!(service.validate_file_name("file.name.ext").is_ok());
    }

    #[test]
    fn test_validate_file_name_invalid() {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let object_store = Arc::new(MockObjectStore::new());
        let service = FileService::new(event_store, metadata_store, object_store);

        assert!(service.validate_file_name("").is_err());
        assert!(service.validate_file_name("path/file.txt").is_err());
        assert!(service.validate_file_name("file\0name.txt").is_err());
    }

    // Tests for get_file

    #[tokio::test]
    async fn test_get_file_success() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create a file owned by the user
        let file = File::new(
            "test.txt".to_string(),
            "/test.txt".to_string(),
            "abc123".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let result = service.get_file(file.id, owner_id).await;
        assert!(result.is_ok());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.id, file.id);
        assert_eq!(retrieved.name, "test.txt");
        assert_eq!(retrieved.owner_id, owner_id);
    }

    #[tokio::test]
    async fn test_get_file_not_found() {
        let (service, _, _, _) = setup_file_service();
        let user_id = uuid::Uuid::new_v4();
        let non_existent_file_id = uuid::Uuid::new_v4();

        let result = service.get_file(non_existent_file_id, user_id).await;
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_get_file_permission_denied() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let other_user = uuid::Uuid::new_v4();

        // Create a file owned by owner_id
        let file = File::new(
            "private.txt".to_string(),
            "/private.txt".to_string(),
            "def456".to_string(),
            200,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Try to access as different user
        let result = service.get_file(file.id, other_user).await;
        assert!(matches!(result, Err(FileError::PermissionDenied { .. })));

        // Verify the error contains the correct IDs
        if let Err(FileError::PermissionDenied { file_id, user_id }) = result {
            assert_eq!(file_id, file.id);
            assert_eq!(user_id, other_user);
        }
    }

    // Tests for get_download_url

    #[tokio::test]
    async fn test_get_download_url_success() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        let file = File::new(
            "download.pdf".to_string(),
            "/download.pdf".to_string(),
            "hash789".to_string(),
            500,
            "application/pdf".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let result = service.get_download_url(file.id, owner_id).await;
        assert!(result.is_ok());

        let url = result.unwrap();
        // Verify URL contains the storage key and expiry
        assert!(url.contains("blobs/hash789"));
        assert!(url.contains("expiry=3600"));
    }

    #[tokio::test]
    async fn test_get_download_url_not_found() {
        let (service, _, _, _) = setup_file_service();
        let user_id = uuid::Uuid::new_v4();
        let non_existent_file_id = uuid::Uuid::new_v4();

        let result = service.get_download_url(non_existent_file_id, user_id).await;
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_get_download_url_permission_denied() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let other_user = uuid::Uuid::new_v4();

        let file = File::new(
            "secret.docx".to_string(),
            "/secret.docx".to_string(),
            "secrethash".to_string(),
            1000,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Try to get download URL as different user
        let result = service.get_download_url(file.id, other_user).await;
        assert!(matches!(result, Err(FileError::PermissionDenied { .. })));
    }

    // Tests for update_file with optimistic locking

    #[tokio::test]
    async fn test_update_file_success() {
        let (service, event_store, metadata_store, object_store) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create initial file
        let file = File::new(
            "update-me.txt".to_string(),
            "/update-me.txt".to_string(),
            "originalhash123".to_string(),
            50,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Update with correct expected version
        let new_content = Bytes::from("Updated content here");
        let updated = service
            .update_file(file.id, owner_id, 1, new_content.clone())
            .await
            .unwrap();

        // Verify updated file
        assert_eq!(updated.id, file.id);
        assert_eq!(updated.current_version, 2);
        assert_eq!(updated.size, new_content.len() as i64);
        assert_ne!(updated.content_hash, "originalhash123");

        // Verify event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FileModified);

        // Verify version was created
        let versions = metadata_store.versions.lock().unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version_number, 2);
        assert_eq!(versions[0].file_id, file.id);

        // Verify blob was uploaded
        let objects = object_store.objects.lock().unwrap();
        assert!(objects.contains_key(&format!("blobs/{}", updated.content_hash)));
    }

    #[tokio::test]
    async fn test_update_file_version_conflict() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create file at version 1
        let file = File::new(
            "conflict.txt".to_string(),
            "/conflict.txt".to_string(),
            "hash123".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Simulate another update happened (version is now 2)
        {
            let mut files = metadata_store.files.lock().unwrap();
            if let Some(f) = files.iter_mut().find(|f| f.id == file.id) {
                f.current_version = 2;
                f.content_hash = "newer_hash".to_string();
            }
        }

        // Try to update expecting version 1 (stale)
        let new_content = Bytes::from("My update");
        let result = service
            .update_file(file.id, owner_id, 1, new_content)
            .await;

        // Should get version conflict
        assert!(matches!(result, Err(FileError::VersionConflict { .. })));

        if let Err(FileError::VersionConflict { expected, actual, .. }) = result {
            assert_eq!(expected, 1);
            assert_eq!(actual, 2);
        }
    }

    #[tokio::test]
    async fn test_update_file_not_found() {
        let (service, _, _, _) = setup_file_service();
        let user_id = uuid::Uuid::new_v4();
        let non_existent_file_id = uuid::Uuid::new_v4();

        let content = Bytes::from("content");
        let result = service
            .update_file(non_existent_file_id, user_id, 1, content)
            .await;

        assert!(matches!(result, Err(FileError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_update_file_permission_denied() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let other_user = uuid::Uuid::new_v4();

        let file = File::new(
            "not-yours.txt".to_string(),
            "/not-yours.txt".to_string(),
            "ownerhash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Try to update as different user
        let content = Bytes::from("hacked content");
        let result = service.update_file(file.id, other_user, 1, content).await;

        assert!(matches!(result, Err(FileError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_update_file_deduplication() {
        let (service, _, metadata_store, object_store) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create initial file
        let file = File::new(
            "dedupe.txt".to_string(),
            "/dedupe.txt".to_string(),
            "initialhash".to_string(),
            50,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Upload some content first
        let content = Bytes::from("Deduplication test content");
        let hash = service.calculate_sha256(&content);
        object_store
            .objects
            .lock()
            .unwrap()
            .insert(format!("blobs/{}", hash), content.clone());

        // Update file with same content that already exists in object store
        let updated = service
            .update_file(file.id, owner_id, 1, content.clone())
            .await
            .unwrap();

        // Should still succeed and use the existing blob
        assert_eq!(updated.content_hash, hash);
        assert_eq!(updated.current_version, 2);

        // Only one blob with this hash should exist
        let objects = object_store.objects.lock().unwrap();
        let matching: Vec<_> = objects.keys().filter(|k| k.contains(&hash)).collect();
        assert_eq!(matching.len(), 1);
    }

    // ==================== Task 9: Delete, Move, Rename Tests ====================

    #[tokio::test]
    async fn test_delete_file_success() {
        let (service, event_store, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create a file
        let file = File::new(
            "delete-me.txt".to_string(),
            "/delete-me.txt".to_string(),
            "deletehash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Delete the file
        let result = service.delete_file(file.id, owner_id).await;
        assert!(result.is_ok());

        // Verify file is deleted from metadata store
        let files = metadata_store.files.lock().unwrap();
        assert!(files.iter().find(|f| f.id == file.id).is_none());

        // Verify FileDeleted event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FileDeleted);
    }

    #[tokio::test]
    async fn test_delete_file_not_found() {
        let (service, _, _, _) = setup_file_service();
        let user_id = uuid::Uuid::new_v4();
        let non_existent_file_id = uuid::Uuid::new_v4();

        let result = service.delete_file(non_existent_file_id, user_id).await;
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_delete_file_permission_denied() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let other_user = uuid::Uuid::new_v4();

        let file = File::new(
            "not-yours.txt".to_string(),
            "/not-yours.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let result = service.delete_file(file.id, other_user).await;
        assert!(matches!(result, Err(FileError::PermissionDenied { .. })));

        // File should still exist
        let files = metadata_store.files.lock().unwrap();
        assert!(files.iter().find(|f| f.id == file.id).is_some());
    }

    #[tokio::test]
    async fn test_move_file_success() {
        let (service, event_store, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create a folder
        let folder = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            uuid::Uuid::new_v4(),
            owner_id,
        );
        metadata_store.add_folder(folder.clone());

        // Create a file in root
        let file = File::new(
            "move-me.txt".to_string(),
            "/move-me.txt".to_string(),
            "movehash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Move file to folder
        let moved_file = service.move_file(file.id, Some(folder.id), owner_id).await.unwrap();

        assert_eq!(moved_file.parent_folder_id, Some(folder.id));
        assert_eq!(moved_file.path, "/Documents/move-me.txt");
        assert_eq!(moved_file.name, "move-me.txt"); // Name stays the same

        // Verify FileMoved event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FileMoved);
    }

    #[tokio::test]
    async fn test_move_file_to_root() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create a folder
        let folder = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            uuid::Uuid::new_v4(),
            owner_id,
        );
        metadata_store.add_folder(folder.clone());

        // Create a file in the folder
        let file = File::new(
            "move-me.txt".to_string(),
            "/Documents/move-me.txt".to_string(),
            "movehash".to_string(),
            100,
            "text/plain".to_string(),
            Some(folder.id),
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Move file to root (None)
        let moved_file = service.move_file(file.id, None, owner_id).await.unwrap();

        assert_eq!(moved_file.parent_folder_id, None);
        assert_eq!(moved_file.path, "/move-me.txt");
    }

    #[tokio::test]
    async fn test_move_file_not_found() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        let folder = Folder::new_child(
            "Folder".to_string(),
            "/Folder".to_string(),
            uuid::Uuid::new_v4(),
            owner_id,
        );
        metadata_store.add_folder(folder.clone());

        let non_existent_file_id = uuid::Uuid::new_v4();
        let result = service.move_file(non_existent_file_id, Some(folder.id), owner_id).await;
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_move_file_parent_not_found() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        let file = File::new(
            "move-me.txt".to_string(),
            "/move-me.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let non_existent_folder_id = uuid::Uuid::new_v4();
        let result = service.move_file(file.id, Some(non_existent_folder_id), owner_id).await;
        assert!(matches!(result, Err(FileError::ParentFolderNotFound(_))));
    }

    #[tokio::test]
    async fn test_move_file_permission_denied_on_folder() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let other_user = uuid::Uuid::new_v4();

        // Create a folder owned by different user
        let folder = Folder::new_child(
            "OtherFolder".to_string(),
            "/OtherFolder".to_string(),
            uuid::Uuid::new_v4(),
            other_user,
        );
        metadata_store.add_folder(folder.clone());

        // Create a file owned by owner_id
        let file = File::new(
            "move-me.txt".to_string(),
            "/move-me.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Try to move to folder owned by other user
        let result = service.move_file(file.id, Some(folder.id), owner_id).await;
        assert!(matches!(result, Err(FileError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_rename_file_success() {
        let (service, event_store, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        let file = File::new(
            "old-name.txt".to_string(),
            "/old-name.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let renamed_file = service.rename_file(file.id, "new-name.txt".to_string(), owner_id).await.unwrap();

        assert_eq!(renamed_file.name, "new-name.txt");
        assert_eq!(renamed_file.path, "/new-name.txt");
        assert_eq!(renamed_file.parent_folder_id, None); // Parent unchanged

        // Verify FileRenamed event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FileRenamed);
    }

    #[tokio::test]
    async fn test_rename_file_in_folder() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        let folder = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            uuid::Uuid::new_v4(),
            owner_id,
        );
        metadata_store.add_folder(folder.clone());

        let file = File::new(
            "old-name.txt".to_string(),
            "/Documents/old-name.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            Some(folder.id),
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let renamed_file = service.rename_file(file.id, "new-name.txt".to_string(), owner_id).await.unwrap();

        assert_eq!(renamed_file.name, "new-name.txt");
        assert_eq!(renamed_file.path, "/Documents/new-name.txt");
        assert_eq!(renamed_file.parent_folder_id, Some(folder.id));
    }

    #[tokio::test]
    async fn test_rename_file_invalid_name_empty() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        let file = File::new(
            "valid.txt".to_string(),
            "/valid.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let result = service.rename_file(file.id, "".to_string(), owner_id).await;
        assert!(matches!(result, Err(FileError::InvalidName(_))));
    }

    #[tokio::test]
    async fn test_rename_file_invalid_name_slash() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        let file = File::new(
            "valid.txt".to_string(),
            "/valid.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let result = service.rename_file(file.id, "path/to/file.txt".to_string(), owner_id).await;
        assert!(matches!(result, Err(FileError::InvalidName(_))));
    }

    #[tokio::test]
    async fn test_rename_file_not_found() {
        let (service, _, _, _) = setup_file_service();
        let user_id = uuid::Uuid::new_v4();
        let non_existent_file_id = uuid::Uuid::new_v4();

        let result = service.rename_file(non_existent_file_id, "new.txt".to_string(), user_id).await;
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_rename_file_permission_denied() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let other_user = uuid::Uuid::new_v4();

        let file = File::new(
            "owned.txt".to_string(),
            "/owned.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let result = service.rename_file(file.id, "hacked.txt".to_string(), other_user).await;
        assert!(matches!(result, Err(FileError::PermissionDenied { .. })));
    }

    // ==================== Task 10: Version History & Restore Tests ====================









}

// Integration test (requires DB + S3)
#[cfg(test)]
mod integration_tests {
    #[ignore] // Requires DB + S3
    async fn test_upload_file_integration() {
        // This test requires:
        // 1. Running PostgreSQL database with schema
        // 2. Running RustFS/S3 compatible storage
        //
        // To run:
        // docker-compose up -d postgres rustfs
        // cargo test test_upload_file_integration -- --ignored
        //
        // The actual implementation would use:
        // - rustshare_storage::EventStore
        // - rustshare_storage::MetadataStore
        // - rustshare_storage::ObjectStore
        //
        // For now, this is a placeholder that documents the expected behavior.
        println!("Integration test placeholder - requires DB + S3 setup");
    }
}
