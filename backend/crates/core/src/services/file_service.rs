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
use crate::events::{AggregateType, Event, EventType, FileUploadedPayload};
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

    /// Validate a file name.
    ///
    /// A valid file name must:
    /// - Not be empty
    /// - Not contain forward slash (/)
    /// - Not contain null character (\0)
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
}

// Integration test (requires DB + S3)
#[cfg(test)]
mod integration_tests {
    #[tokio::test]
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
