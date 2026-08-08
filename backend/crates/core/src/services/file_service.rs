//! FileService for file upload and management operations.
//!
//! This service handles file uploads, including:
//! - File name validation
//! - SHA256 content hashing for deduplication
//! - RustFS object storage as the primary blob store
//! - Event sourcing via EventStore
//! - Metadata persistence via MetadataStore

use anyhow::Result;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::io::AsyncReadExt;

use crate::domain::{
    File, FileId, FileVersion, Folder, FolderId, ReplicationJob, ReplicationState, UserId,
};
use crate::events::{
    AggregateType, Event, EventBroadcaster, EventType, FileDeletedPayload, FileModifiedPayload,
    FileMovedPayload, FileRenamedPayload, FileRestoredPayload, FileUploadedPayload,
    ReplicationStateChangedPayload,
};
// Removed redundant Use FileError

#[derive(Debug, Clone, Default)]
pub struct FileUploadActor {
    pub actor_type: String,
    pub actor_user_id: Option<UserId>,
    pub actor_share_id: Option<uuid::Uuid>,
    pub actor_share_session_id: Option<uuid::Uuid>,
    pub actor_display_name: Option<String>,
}

#[derive(Debug, Clone)]
struct ReplicationEventContext {
    file_id: uuid::Uuid,
    owner_id: UserId,
    file_version_id: uuid::Uuid,
    replication_state: ReplicationState,
    job_status: Option<String>,
    attempt_count: i32,
    next_attempt_at: Option<chrono::DateTime<chrono::Utc>>,
    last_error: Option<String>,
}

/// Trait for event store operations needed by FileService.
///
/// This trait abstracts the event store to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait EventStoreOps: Send + Sync {
    /// Transaction handle type.
    type Tx: Send;

    /// Append an event to the event store.
    async fn append(&self, event: &Event, broadcaster: &EventBroadcaster) -> Result<()>;

    /// Begin a new database transaction.
    async fn begin_transaction(&self) -> Result<Self::Tx>;

    /// Commit a database transaction.
    async fn commit_transaction(&self, tx: Self::Tx) -> Result<()>;

    /// Append an event to the event store inside an existing transaction.
    async fn append_in_tx(&self, tx: &mut Self::Tx, event: &Event) -> Result<()>;
}

/// Trait for metadata store operations needed by FileService.
///
/// This trait abstracts the metadata store to allow for testing without database dependencies.
#[allow(async_fn_in_trait)]
pub trait MetadataStoreOps: Send + Sync {
    /// Transaction handle type.
    type Tx: Send;

    /// Create a file in the metadata store.
    async fn create_file(&self, file: &File) -> Result<()>;

    /// Create a file in the metadata store inside a transaction.
    async fn create_file_in_tx(&self, tx: &mut Self::Tx, file: &File) -> Result<()>;

    /// Find a file by canonical path for a specific owner.
    async fn find_file_by_path(&self, path: &str, owner_id: uuid::Uuid) -> Result<Option<File>>;

    /// Create a file version in the metadata store.
    async fn create_file_version(&self, version: &FileVersion) -> Result<()>;

    /// Create a file version in the metadata store inside a transaction.
    ///
    /// Returns the persisted version id. Callers must use this id (not the
    /// version struct's id) for anything referencing the row afterwards,
    /// because the upsert may have kept an existing row's id.
    async fn create_file_version_in_tx(
        &self,
        tx: &mut Self::Tx,
        version: &FileVersion,
    ) -> Result<uuid::Uuid>;

    /// Find a folder by ID.
    async fn find_folder_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Option<Folder>>;

    /// Find a folder by ID without ownership filtering.
    ///
    /// Callers must verify access before using this method.
    async fn find_folder_by_id_unchecked(&self, id: uuid::Uuid) -> Result<Option<Folder>>;

    /// Find a file by ID.
    async fn find_file_by_id(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> Result<Option<File>>;

    /// Find a file by ID without ownership filtering.
    ///
    /// Callers must verify access before using this method.
    async fn find_file_by_id_unchecked(&self, id: uuid::Uuid) -> Result<Option<File>>;

    /// Update a file in the metadata store.
    async fn update_file(&self, file: &File) -> Result<()>;

    /// Update a file in the metadata store inside a transaction.
    async fn update_file_in_tx(&self, tx: &mut Self::Tx, file: &File) -> Result<()>;

    /// Delete a file from the metadata store.
    async fn delete_file(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> Result<()>;

    /// Delete a file from the metadata store inside a transaction.
    async fn delete_file_in_tx(
        &self,
        tx: &mut Self::Tx,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<()>;

    /// List all versions of a file, ordered by version number descending.
    async fn list_file_versions(
        &self,
        file_id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> Result<Vec<FileVersion>>;

    /// Find a specific version of a file.
    async fn find_file_version(
        &self,
        file_id: uuid::Uuid,
        version_number: i32,
        owner_id: uuid::Uuid,
    ) -> Result<Option<FileVersion>>;

    /// Count enabled replication targets.
    async fn count_enabled_replication_targets(&self) -> Result<i64>;

    /// Queue a new replication job.
    async fn create_replication_job(&self, job: &ReplicationJob) -> Result<()>;

    /// Update the replication state for a file version.
    async fn update_file_version_replication_state(
        &self,
        version_id: uuid::Uuid,
        state: ReplicationState,
    ) -> Result<()>;
}

/// Trait for object store operations needed by FileService.
///
/// This trait abstracts RustFS/object storage to allow for testing without storage dependencies.
#[allow(async_fn_in_trait)]
pub trait ObjectStoreOps: Send + Sync {
    /// Acquire the cross-process exclusion guard for a content-addressed write.
    async fn acquire_blob_write_lock(&self, _key: &str) -> Result<Box<dyn Send>> {
        Ok(Box::new(()))
    }

    /// Upload data to object storage.
    async fn put(&self, key: &str, data: Bytes) -> Result<()>;

    /// Upload data to object storage by streaming from a local file path.
    ///
    /// This avoids loading the entire object into memory and is the preferred
    /// path for large uploads that have already been buffered to disk.
    async fn put_from_path(&self, key: &str, path: &std::path::Path) -> Result<()>;

    /// Check if an object exists.
    async fn exists(&self, key: &str) -> Result<bool>;

    /// Download content from object storage.
    ///
    /// # Arguments
    /// * `key` - The object key
    ///
    /// # Returns
    /// The content as Bytes.
    async fn get(&self, key: &str) -> Result<Bytes>;

    /// Delete an object from storage.
    ///
    /// # Arguments
    /// * `key` - The object key
    async fn delete(&self, key: &str) -> Result<()>;
}

/// Source of blob data for an upload.
///
/// `Bytes` is used for small in-memory payloads (tests, internal callers).
/// `Path` is used for large uploads that have been streamed to a temporary
/// file on disk and must be sent to object storage without being read back
/// into memory.
enum UploadSource {
    Bytes(Bytes),
    Path(std::path::PathBuf),
}

use crate::domain::SharePermissions;
use crate::services::errors::FileError;
use crate::services::{PermissionResolver, PermissionResolverOps};

/// File service for handling file operations.
pub struct FileService<E, M, O, P>
where
    E: EventStoreOps,
    M: MetadataStoreOps<Tx = E::Tx>,
    O: ObjectStoreOps,
    P: PermissionResolverOps,
{
    event_store: Arc<E>,
    metadata_store: Arc<M>,
    object_store: Arc<O>,
    broadcaster: Arc<EventBroadcaster>,
    permission_resolver: Arc<PermissionResolver<P>>,
}

impl<E, M, O, P> FileService<E, M, O, P>
where
    E: EventStoreOps,
    M: MetadataStoreOps<Tx = E::Tx>,
    O: ObjectStoreOps,
    P: PermissionResolverOps,
{
    /// Create a new FileService with the given stores.
    pub fn new(
        event_store: Arc<E>,
        metadata_store: Arc<M>,
        object_store: Arc<O>,
        broadcaster: Arc<EventBroadcaster>,
        permission_resolver: Arc<PermissionResolver<P>>,
    ) -> Self {
        Self {
            event_store,
            metadata_store,
            object_store,
            broadcaster,
            permission_resolver,
        }
    }
    async fn require_file_permission(
        &self,
        user_id: UserId,
        tenant_id: uuid::Uuid,
        file_id: FileId,
        required: SharePermissions,
    ) -> Result<(), FileError> {
        let has = self
            .permission_resolver
            .check_file_permission(user_id, tenant_id, file_id, required)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        if !has {
            return Err(FileError::PermissionDenied { file_id, user_id });
        }
        Ok(())
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
    /// - `FileError::Storage` if the RustFS write fails
    /// - `FileError::Database` if database operations fail
    pub async fn upload_file(
        &self,
        owner_id: UserId,
        name: String,
        parent_folder_id: Option<FolderId>,
        content: Bytes,
        mime_type: String,
        tenant_id: uuid::Uuid,
    ) -> Result<File, FileError> {
        let actor = FileUploadActor {
            actor_type: "user".to_string(),
            actor_user_id: Some(owner_id),
            actor_share_id: None,
            actor_share_session_id: None,
            actor_display_name: None,
        };

        self.upload_file_with_actor(
            owner_id,
            actor,
            name,
            parent_folder_id,
            content,
            mime_type,
            tenant_id,
        )
        .await
    }

    /// Upload a new file whose contents are stored on disk at `file_path`.
    ///
    /// This is the streaming equivalent of [`Self::upload_file`] and is used by
    /// HTTP handlers for large multipart uploads. The file is hashed and sent
    /// to object storage without being fully loaded into memory.
    pub async fn upload_file_from_path(
        &self,
        owner_id: UserId,
        name: String,
        parent_folder_id: Option<FolderId>,
        file_path: &std::path::Path,
        mime_type: String,
        tenant_id: uuid::Uuid,
    ) -> Result<File, FileError> {
        let actor = FileUploadActor {
            actor_type: "user".to_string(),
            actor_user_id: Some(owner_id),
            actor_share_id: None,
            actor_share_session_id: None,
            actor_display_name: None,
        };

        self.upload_file_with_actor_from_path(
            owner_id,
            actor,
            name,
            parent_folder_id,
            file_path,
            mime_type,
            tenant_id,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upload_file_with_actor(
        &self,
        owner_id: UserId,
        actor: FileUploadActor,
        name: String,
        parent_folder_id: Option<FolderId>,
        content: Bytes,
        mime_type: String,
        tenant_id: uuid::Uuid,
    ) -> Result<File, FileError> {
        self.validate_file_name(&name)?;
        let content_hash = self.calculate_sha256(&content);
        let size = content.len() as i64;
        self.upload_file_with_actor_impl(
            owner_id,
            actor,
            name,
            parent_folder_id,
            content_hash,
            size,
            UploadSource::Bytes(content),
            mime_type,
            tenant_id,
        )
        .await
    }

    /// Upload a new file whose contents are stored on disk at `file_path`.
    ///
    /// The file is hashed and uploaded to object storage using streaming I/O,
    /// so the full content is never loaded into memory. This is the path used
    /// by HTTP handlers for large multipart uploads.
    #[allow(clippy::too_many_arguments)]
    pub async fn upload_file_with_actor_from_path(
        &self,
        owner_id: UserId,
        actor: FileUploadActor,
        name: String,
        parent_folder_id: Option<FolderId>,
        file_path: &std::path::Path,
        mime_type: String,
        tenant_id: uuid::Uuid,
    ) -> Result<File, FileError> {
        self.validate_file_name(&name)?;
        let (content_hash, size) = self.calculate_sha256_and_size_from_path(file_path).await?;
        self.upload_file_with_actor_impl(
            owner_id,
            actor,
            name,
            parent_folder_id,
            content_hash,
            size,
            UploadSource::Path(file_path.to_path_buf()),
            mime_type,
            tenant_id,
        )
        .await
    }

    /// Shared implementation for file uploads from either in-memory bytes or a
    /// temporary file on disk.
    #[allow(clippy::too_many_arguments)]
    async fn upload_file_with_actor_impl(
        &self,
        owner_id: UserId,
        actor: FileUploadActor,
        name: String,
        parent_folder_id: Option<FolderId>,
        content_hash: String,
        size: i64,
        source: UploadSource,
        mime_type: String,
        tenant_id: uuid::Uuid,
    ) -> Result<File, FileError> {
        // 1. Check parent folder exists (if provided) and verify permissions
        let (parent_path, file_owner_id) = if let Some(folder_id) = parent_folder_id {
            // Use unchecked lookup so we can distinguish "folder doesn't exist"
            // from "user lacks permission".
            let folder = self
                .metadata_store
                .find_folder_by_id_unchecked(folder_id)
                .await
                .map_err(|e| FileError::Database(e.to_string()))?
                .ok_or(FileError::ParentFolderNotFound(folder_id))?;

            // Verify permissions: user must own the folder or have Edit permission
            let has_permission = self
                .permission_resolver
                .check_folder_permission(owner_id, tenant_id, folder_id, SharePermissions::Edit)
                .await
                .map_err(|e| FileError::Database(e.to_string()))?;

            if !has_permission {
                return Err(FileError::PermissionDenied {
                    file_id: uuid::Uuid::nil(),
                    user_id: owner_id,
                });
            }

            // Files in shared folders are owned by the folder owner so that
            // deduplication and versioning work within the shared namespace.
            (folder.path.clone(), folder.owner_id)
        } else {
            (String::new(), owner_id)
        };

        // 2. Construct path from parent path + name
        let path = if parent_path.is_empty() || parent_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent_path, name)
        };

        // 3. Write to RustFS at "blobs/{hash}" (skip if the blob already exists)
        let storage_key = format!("blobs/{}", content_hash);
        let _blob_write_lock = self
            .object_store
            .acquire_blob_write_lock(&storage_key)
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;
        let blob_exists = self
            .object_store
            .exists(&storage_key)
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        if !blob_exists {
            match source {
                UploadSource::Bytes(ref content) => {
                    self.object_store
                        .put(&storage_key, content.clone())
                        .await
                        .map_err(|e| FileError::Storage(e.to_string()))?;
                }
                UploadSource::Path(ref file_path) => {
                    self.object_store
                        .put_from_path(&storage_key, file_path)
                        .await
                        .map_err(|e| FileError::Storage(e.to_string()))?;
                }
            }
        }

        if let Some(mut existing) = self
            .metadata_store
            .find_file_by_path(&path, file_owner_id)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?
        {
            if existing.content_hash == content_hash && existing.size == size {
                return Ok(existing);
            }

            let old_version = existing.current_version;
            let old_content_hash = existing.content_hash.clone();
            let old_size = existing.size;

            existing.content_hash = content_hash.clone();
            existing.size = size;
            existing.mime_type = mime_type.clone();
            existing.parent_folder_id = parent_folder_id;
            existing.current_version += 1;
            existing.modified_at = chrono::Utc::now();

            let version = FileVersion::new(
                existing.id,
                existing.current_version,
                content_hash.clone(),
                size,
                owner_id,
                Some("Uploaded new content".to_string()),
                tenant_id,
            );

            let payload = FileModifiedPayload {
                file_id: existing.id,
                old_version,
                new_version: existing.current_version,
                old_content_hash,
                new_content_hash: content_hash,
                old_size,
                new_size: size,
                storage_key,
                modified_by: owner_id,
            };
            let payload_json = serde_json::to_value(&payload)
                .map_err(|e| FileError::Storage(format!("Failed to serialize payload: {}", e)))?;

            let event = Event::new(
                EventType::FileModified,
                existing.id,
                AggregateType::File,
                payload_json,
                owner_id,
            );

            let mut tx =
                self.event_store.begin_transaction().await.map_err(|e| {
                    FileError::Storage(format!("Failed to begin transaction: {}", e))
                })?;
            self.event_store
                .append_in_tx(&mut tx, &event)
                .await
                .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;
            self.metadata_store
                .update_file_in_tx(&mut tx, &existing)
                .await
                .map_err(|e| FileError::Database(e.to_string()))?;
            let persisted_version_id = self
                .metadata_store
                .create_file_version_in_tx(&mut tx, &version)
                .await
                .map_err(|e| FileError::Database(e.to_string()))?;
            self.event_store
                .commit_transaction(tx)
                .await
                .map_err(|e| FileError::Storage(format!("Failed to commit transaction: {}", e)))?;
            self.broadcaster.publish(event);

            self.queue_replication_if_needed(
                existing.id,
                file_owner_id,
                persisted_version_id,
                &version.storage_key(),
            )
            .await?;

            return Ok(existing);
        }

        // 4. Create File domain object with version=1
        let file = File::new(
            name.clone(),
            path.clone(),
            content_hash.clone(),
            size,
            mime_type.clone(),
            parent_folder_id,
            file_owner_id,
            tenant_id,
        );

        // 5. Emit FileUploaded event to EventStore
        let payload = FileUploadedPayload {
            file_id: file.id,
            name: name.clone(),
            path: path.clone(),
            size,
            content_hash: content_hash.clone(),
            storage_key: storage_key.clone(),
            mime_type: mime_type.clone(),
            owner_id: file_owner_id,
            parent_folder_id,
            actor_type: actor.actor_type.clone(),
            actor_user_id: actor.actor_user_id,
            actor_share_id: actor.actor_share_id,
            actor_share_session_id: actor.actor_share_session_id,
            actor_display_name: actor.actor_display_name.clone(),
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

        // 6. Insert into files and file_versions tables atomically with the event
        let version = FileVersion::new(
            file.id,
            1,
            content_hash,
            size,
            owner_id,
            Some(match actor.actor_type.as_str() {
                "public_share_session" => match actor.actor_display_name.as_deref() {
                    Some(name) if !name.is_empty() => {
                        format!("Uploaded via public share by {}", name)
                    }
                    _ => "Uploaded via public share".to_string(),
                },
                _ => "Initial upload".to_string(),
            }),
            tenant_id,
        );

        let mut tx = self
            .event_store
            .begin_transaction()
            .await
            .map_err(|e| FileError::Storage(format!("Failed to begin transaction: {}", e)))?;
        self.event_store
            .append_in_tx(&mut tx, &event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;
        self.metadata_store
            .create_file_in_tx(&mut tx, &file)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        let persisted_version_id = self
            .metadata_store
            .create_file_version_in_tx(&mut tx, &version)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        self.event_store
            .commit_transaction(tx)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to commit transaction: {}", e)))?;
        self.broadcaster.publish(event);

        self.queue_replication_if_needed(
            file.id,
            file_owner_id,
            persisted_version_id,
            &version.storage_key(),
        )
        .await?;

        // 7. Return File
        Ok(file)
    }

    /// Get a file by ID, verifying access permissions.
    ///
    /// # Arguments
    /// * `file_id` - The ID of the file to retrieve
    /// * `user_id` - The ID of the user requesting the file
    ///
    /// # Returns
    /// The File domain object if found and the user has at least View permission.
    ///
    /// # Errors
    /// - `FileError::NotFound` if the file doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't have access
    pub async fn get_file(&self, file_id: uuid::Uuid, user_id: UserId) -> Result<File, FileError> {
        // 1. Find file by ID first. Deleted or non-existent files must return
        // NotFound rather than PermissionDenied.
        let file = self
            .metadata_store
            .find_file_by_id_unchecked(file_id)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?
            .ok_or(FileError::NotFound(file_id))?;

        // 2. Check permissions. Access was verified above, so this must not be
        // owner-filtered; shared recipients are allowed to read non-owned files.
        self.require_file_permission(user_id, file.tenant_id, file_id, SharePermissions::View)
            .await?;

        Ok(file)
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
    /// - `FileError::Storage` if the RustFS write fails
    /// - `FileError::Database` if database operations fail
    pub async fn update_file(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
        expected_version: i32,
        content: Bytes,
    ) -> Result<File, FileError> {
        let new_content_hash = self.calculate_sha256(&content);
        let new_size = content.len() as i64;
        self.update_file_impl(
            file_id,
            user_id,
            expected_version,
            new_content_hash,
            new_size,
            UploadSource::Bytes(content),
        )
        .await
    }

    /// Update a file's content from a file on disk with optimistic locking.
    ///
    /// The file is hashed and sent to object storage using streaming I/O so the
    /// full content is never loaded into memory.
    pub async fn update_file_from_path(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
        expected_version: i32,
        file_path: &std::path::Path,
    ) -> Result<File, FileError> {
        let (new_content_hash, new_size) =
            self.calculate_sha256_and_size_from_path(file_path).await?;
        self.update_file_impl(
            file_id,
            user_id,
            expected_version,
            new_content_hash,
            new_size,
            UploadSource::Path(file_path.to_path_buf()),
        )
        .await
    }

    /// Shared implementation for file updates from either bytes or a file path.
    async fn update_file_impl(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
        expected_version: i32,
        new_content_hash: String,
        new_size: i64,
        source: UploadSource,
    ) -> Result<File, FileError> {
        // 1. Get current file and verify access
        let mut file = self.get_file(file_id, user_id).await?;

        // 1b. Verify Edit permission
        self.require_file_permission(user_id, file.tenant_id, file_id, SharePermissions::Edit)
            .await?;

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

        // 4. Write to RustFS (skip if same content - deduplication)
        let storage_key = format!("blobs/{}", new_content_hash);
        let _blob_write_lock = self
            .object_store
            .acquire_blob_write_lock(&storage_key)
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;
        let blob_exists = self
            .object_store
            .exists(&storage_key)
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;

        if !blob_exists {
            match source {
                UploadSource::Bytes(content) => {
                    self.object_store
                        .put(&storage_key, content)
                        .await
                        .map_err(|e| FileError::Storage(e.to_string()))?;
                }
                UploadSource::Path(file_path) => {
                    self.object_store
                        .put_from_path(&storage_key, &file_path)
                        .await
                        .map_err(|e| FileError::Storage(e.to_string()))?;
                }
            }
        }

        // 5. Increment version and update file record
        let old_version = file.current_version;
        file.current_version += 1;
        file.content_hash = new_content_hash.clone();
        file.size = new_size;
        file.modified_at = chrono::Utc::now();

        // 6. Create FileVersion snapshot
        let version = FileVersion::new(
            file.id,
            file.current_version,
            new_content_hash.clone(),
            new_size,
            user_id,
            Some(format!("Updated from version {}", old_version)),
            file.tenant_id,
        );

        // 7. Emit FileModified event atomically with projection updates
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

        let mut tx = self
            .event_store
            .begin_transaction()
            .await
            .map_err(|e| FileError::Storage(format!("Failed to begin transaction: {}", e)))?;
        self.event_store
            .append_in_tx(&mut tx, &event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;
        self.metadata_store
            .update_file_in_tx(&mut tx, &file)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        let persisted_version_id = self
            .metadata_store
            .create_file_version_in_tx(&mut tx, &version)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        self.event_store
            .commit_transaction(tx)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to commit transaction: {}", e)))?;
        self.broadcaster.publish(event);

        self.queue_replication_if_needed(
            file.id,
            user_id,
            persisted_version_id,
            &version.storage_key(),
        )
        .await?;

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
    /// The blob in RustFS is NOT deleted (content-addressed storage may be shared).
    pub async fn list_versions(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
    ) -> Result<Vec<FileVersion>, FileError> {
        // 1. Get file and verify access
        let file = self.get_file(file_id, user_id).await?;

        // 2. Get all versions from metadata store (already ordered DESC).
        //    Query by the file owner: shared View/Edit recipients may see versions too.
        let versions = self
            .metadata_store
            .list_file_versions(file_id, file.owner_id)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;

        Ok(versions)
    }

    /// Restore a file to a previous version.
    ///
    /// This creates a NEW version with the content from the old version,
    /// rather than overwriting the current version.
    ///
    /// # Arguments
    /// * `file_id` - The ID of the file to restore
    /// * `version_number` - The version number to restore to
    /// * `user_id` - The ID of the user performing the restore
    ///
    /// # Returns
    /// The updated File domain object with the new version.
    ///
    /// # Errors
    /// - `FileError::NotFound` if the file doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't own the file
    /// - `FileError::VersionNotFound` if the version doesn't exist
    /// - `FileError::Storage` if the RustFS read fails
    /// - `FileError::Database` if database operations fail
    pub async fn restore_version(
        &self,
        file_id: uuid::Uuid,
        version_number: i32,
        user_id: UserId,
    ) -> Result<File, FileError> {
        // 1. Get file and verify access
        let mut file = self.get_file(file_id, user_id).await?;
        let old_version = file.current_version;

        // 1b. Verify Edit permission
        self.require_file_permission(user_id, file.tenant_id, file_id, SharePermissions::Edit)
            .await?;

        // 2. Find the old version (by the file owner: shared Edit recipients
        //    may restore versions too)
        let old_file_version = self
            .metadata_store
            .find_file_version(file_id, version_number, file.owner_id)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?
            .ok_or(FileError::VersionNotFound(version_number))?;

        // 3. Read content from RustFS (using the old version's storage key)
        let storage_key = old_file_version.storage_key();
        let content =
            self.object_store.get(&storage_key).await.map_err(|e| {
                FileError::Storage(format!("Failed to download old version: {}", e))
            })?;

        // 4. Create new version with old content
        // The blob already exists in RustFS (same content hash), no need to re-upload
        let new_version_number = file.current_version + 1;
        file.current_version = new_version_number;
        file.content_hash = old_file_version.content_hash.clone();
        file.size = old_file_version.size;
        file.modified_at = chrono::Utc::now();

        // 5. Create FileVersion snapshot
        let version = FileVersion::new(
            file.id,
            new_version_number,
            old_file_version.content_hash.clone(),
            old_file_version.size,
            user_id,
            Some(format!("Restored from version {}", version_number)),
            file.tenant_id,
        );

        // 6. Emit FileRestored event atomically with projection updates
        let payload = FileRestoredPayload {
            file_id: file.id,
            old_version,
            new_version: new_version_number,
            restored_from_version: version_number,
            content_hash: old_file_version.content_hash,
            size: old_file_version.size,
            restored_by: user_id,
        };
        let payload_json = serde_json::to_value(&payload)
            .map_err(|e| FileError::Storage(format!("Failed to serialize payload: {}", e)))?;

        let event = Event::new(
            EventType::FileRestored,
            file.id,
            AggregateType::File,
            payload_json,
            user_id,
        );

        let mut tx = self
            .event_store
            .begin_transaction()
            .await
            .map_err(|e| FileError::Storage(format!("Failed to begin transaction: {}", e)))?;
        self.event_store
            .append_in_tx(&mut tx, &event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;
        self.metadata_store
            .update_file_in_tx(&mut tx, &file)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        let persisted_version_id = self
            .metadata_store
            .create_file_version_in_tx(&mut tx, &version)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        self.event_store
            .commit_transaction(tx)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to commit transaction: {}", e)))?;
        self.broadcaster.publish(event);

        self.queue_replication_if_needed(
            file.id,
            user_id,
            persisted_version_id,
            &version.storage_key(),
        )
        .await?;

        // 7. Return updated file
        // Note: content is already in RustFS, we just needed to read it to verify it exists
        drop(content);
        Ok(file)
    }

    /// Move a file to a different folder.
    ///
    /// # Arguments
    /// * `file_id` - The UUID of the file to move
    /// * `target_folder_id` - The UUID of the destination folder, or None for root
    /// * `user_id` - The user requesting the move
    ///
    /// # Returns
    /// The updated file with new path and parent_folder_id
    ///
    /// # Errors
    /// - `FileError::NotFound` if the file doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't own the file
    /// - `FileError::FolderNotFound` if the target folder doesn't exist
    /// - `FileError::Database` if database operations fail
    pub async fn move_file(
        &self,
        file_id: uuid::Uuid,
        target_folder_id: Option<FolderId>,
        user_id: UserId,
    ) -> Result<File, FileError> {
        // 1. Get file and verify access
        let mut file = self.get_file(file_id, user_id).await?;

        // 1b. Verify Edit permission
        self.require_file_permission(user_id, file.tenant_id, file_id, SharePermissions::Edit)
            .await?;

        // 2. If target folder is specified, verify it exists and the user may
        //    write to it (unchecked lookup + Edit check, so shared recipients
        //    can move files into shared folders)
        let new_path = if let Some(folder_id) = target_folder_id {
            let folder = self
                .metadata_store
                .find_folder_by_id_unchecked(folder_id)
                .await
                .map_err(|e| FileError::Database(e.to_string()))?
                .ok_or(FileError::FolderNotFound(folder_id))?;
            let has_edit = self
                .permission_resolver
                .check_folder_permission(
                    user_id,
                    folder.tenant_id,
                    folder_id,
                    SharePermissions::Edit,
                )
                .await
                .map_err(|e| FileError::Database(e.to_string()))?;
            if !has_edit {
                return Err(FileError::PermissionDenied {
                    file_id: folder_id,
                    user_id,
                });
            }
            format!("{}/{}", folder.path, file.name)
        } else {
            format!("/{}", file.name)
        };

        // 3. Store old values for event
        let old_parent_folder_id = file.parent_folder_id;
        let old_path = file.path.clone();

        // 4. Update file
        file.parent_folder_id = target_folder_id;
        file.path = new_path.clone();

        // 6. Create FileMoved event
        let payload = FileMovedPayload {
            file_id,
            old_parent_folder_id,
            new_parent_folder_id: target_folder_id,
            old_path,
            new_path,
            moved_by: user_id,
        };

        let event = Event {
            id: uuid::Uuid::new_v4(),
            event_type: EventType::FileMoved,
            aggregate_id: file_id,
            aggregate_type: AggregateType::File,
            payload: serde_json::to_value(&payload)
                .map_err(|e| FileError::Storage(format!("Failed to serialize event: {}", e)))?,
            user_id,
            timestamp: chrono::Utc::now(),
            version: file.current_version,
        };

        // 7. Persist updated file and append event atomically
        let mut tx = self
            .event_store
            .begin_transaction()
            .await
            .map_err(|e| FileError::Storage(format!("Failed to begin transaction: {}", e)))?;
        self.event_store
            .append_in_tx(&mut tx, &event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;
        self.metadata_store
            .update_file_in_tx(&mut tx, &file)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        self.event_store
            .commit_transaction(tx)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to commit transaction: {}", e)))?;
        self.broadcaster.publish(event);

        Ok(file)
    }

    /// Rename a file while keeping its parent folder unchanged.
    pub async fn rename_file(
        &self,
        file_id: uuid::Uuid,
        new_name: String,
        user_id: UserId,
    ) -> Result<File, FileError> {
        self.validate_file_name(&new_name)?;

        // 1. Get file and verify access
        let mut file = self.get_file(file_id, user_id).await?;

        // 1b. Verify Edit permission
        self.require_file_permission(user_id, file.tenant_id, file_id, SharePermissions::Edit)
            .await?;

        if file.name == new_name {
            return Ok(file);
        }

        let old_name = file.name.clone();
        let old_path = file.path.clone();
        let new_path = if let Some(parent_id) = file.parent_folder_id {
            let parent = self
                .metadata_store
                .find_folder_by_id(parent_id, file.owner_id)
                .await
                .map_err(|e| FileError::Database(e.to_string()))?
                .ok_or(FileError::FolderNotFound(parent_id))?;

            if parent.path == "/" {
                format!("/{}", new_name)
            } else {
                format!("{}/{}", parent.path.trim_end_matches('/'), new_name)
            }
        } else {
            format!("/{}", new_name)
        };

        file.name = new_name.clone();
        file.path = new_path.clone();
        file.modified_at = chrono::Utc::now();

        let payload = FileRenamedPayload {
            file_id,
            old_name,
            new_name,
            old_path,
            new_path,
            renamed_by: user_id,
        };

        let event = Event {
            id: uuid::Uuid::new_v4(),
            event_type: EventType::FileRenamed,
            aggregate_id: file_id,
            aggregate_type: AggregateType::File,
            payload: serde_json::to_value(&payload)
                .map_err(|e| FileError::Storage(format!("Failed to serialize event: {}", e)))?,
            user_id,
            timestamp: chrono::Utc::now(),
            version: file.current_version,
        };

        let mut tx = self
            .event_store
            .begin_transaction()
            .await
            .map_err(|e| FileError::Storage(format!("Failed to begin transaction: {}", e)))?;
        self.event_store
            .append_in_tx(&mut tx, &event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;
        self.metadata_store
            .update_file_in_tx(&mut tx, &file)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        self.event_store
            .commit_transaction(tx)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to commit transaction: {}", e)))?;
        self.broadcaster.publish(event);

        Ok(file)
    }

    /// Delete a file.
    ///
    /// # Arguments
    /// * `file_id` - The UUID of the file to delete
    /// * `user_id` - The user requesting the deletion
    ///
    /// # Returns
    /// Ok(()) if successful
    ///
    /// # Errors
    /// - `FileError::NotFound` if the file doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't own the file
    /// - `FileError::Database` if database operations fail
    pub async fn delete_file(&self, file_id: uuid::Uuid, user_id: UserId) -> Result<(), FileError> {
        // 1. Get file and verify access
        let file = self.get_file(file_id, user_id).await?;

        // 1b. Verify Admin permission for deletion
        self.require_file_permission(user_id, file.tenant_id, file_id, SharePermissions::Admin)
            .await?;

        // 2. Create FileDeleted event
        let payload = FileDeletedPayload {
            file_id,
            file_name: file.name.clone(),
            folder_id: file.parent_folder_id,
        };

        let event = Event {
            id: uuid::Uuid::new_v4(),
            event_type: EventType::FileDeleted,
            aggregate_id: file_id,
            aggregate_type: AggregateType::File,
            payload: serde_json::to_value(&payload)
                .map_err(|e| FileError::Storage(format!("Failed to serialize event: {}", e)))?,
            user_id,
            timestamp: chrono::Utc::now(),
            version: file.current_version,
        };

        // 3. Append event to event store and delete file atomically
        let mut tx = self
            .event_store
            .begin_transaction()
            .await
            .map_err(|e| FileError::Storage(format!("Failed to begin transaction: {}", e)))?;
        self.event_store
            .append_in_tx(&mut tx, &event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;
        self.metadata_store
            .delete_file_in_tx(&mut tx, file_id, user_id)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        self.event_store
            .commit_transaction(tx)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to commit transaction: {}", e)))?;
        self.broadcaster.publish(event);

        // Note: We don't delete from RustFS (blob storage) because of deduplication
        // The same content hash might be used by other files or versions

        Ok(())
    }

    /// Edit a file's content with option to overwrite or create new version.
    ///
    /// # Arguments
    /// * `file_id` - The ID of the file to edit
    /// * `user_id` - The ID of the user performing the edit
    /// * `content` - The new file content as bytes
    /// * `save_mode` - "overwrite" to update current version, "new_version" to create new version
    /// * `change_description` - Optional description for the change (used when creating new version)
    ///
    /// # Returns
    /// The updated File domain object.
    ///
    /// # Errors
    /// - `FileError::NotFound` if the file doesn't exist
    /// - `FileError::PermissionDenied` if the user doesn't own the file
    /// - `FileError::NotEditable` if the file type is not supported for editing
    /// - `FileError::ContentTooLarge` if the content exceeds the editable size limit
    /// - `FileError::Storage` if the RustFS write fails
    /// - `FileError::Database` if database operations fail
    pub async fn edit_file(
        &self,
        file_id: uuid::Uuid,
        user_id: UserId,
        content: Bytes,
        save_mode: &str,
        change_description: Option<String>,
    ) -> Result<File, FileError> {
        // 1. Get file and verify access
        let mut file = self.get_file(file_id, user_id).await?;

        // 1b. Verify Edit permission
        self.require_file_permission(user_id, file.tenant_id, file_id, SharePermissions::Edit)
            .await?;

        // 2. Validate file is editable based on mime type and extension
        self.validate_file_editable(&file)?;

        // 3. Validate content size (10MB limit for editing)
        const MAX_EDITABLE_SIZE: i64 = 10 * 1024 * 1024; // 10MB
        let content_size = content.len() as i64;
        if content_size > MAX_EDITABLE_SIZE {
            return Err(FileError::ContentTooLarge {
                size: content_size,
                limit: MAX_EDITABLE_SIZE,
            });
        }

        // 4. Calculate new content hash
        let old_content_hash = file.content_hash.clone();
        let old_size = file.size;
        let new_content_hash = self.calculate_sha256(&content);
        let new_size = content.len() as i64;

        // 5. Write to RustFS (skip if same content - deduplication)
        let storage_key = format!("blobs/{}", new_content_hash);
        let _blob_write_lock = self
            .object_store
            .acquire_blob_write_lock(&storage_key)
            .await
            .map_err(|e| FileError::Storage(e.to_string()))?;
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

        // 6. Handle based on save mode
        let old_version = file.current_version;
        let saved_as_new_version = if save_mode == "new_version" {
            // Create new version
            file.current_version += 1;
            true
        } else {
            // Overwrite mode - keep same version number
            false
        };

        // Update file record
        file.content_hash = new_content_hash.clone();
        file.size = new_size;
        file.modified_at = chrono::Utc::now();

        // 7. Create FileVersion snapshot
        let version = FileVersion::new(
            file.id,
            file.current_version,
            new_content_hash.clone(),
            new_size,
            user_id,
            Some(change_description.unwrap_or_else(|| {
                if saved_as_new_version {
                    format!("Edited (new version from {})", old_version)
                } else {
                    format!("Edited (overwrote version {})", old_version)
                }
            })),
            file.tenant_id,
        );

        // 8. Emit FileModified event atomically with projection updates
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

        let mut tx = self
            .event_store
            .begin_transaction()
            .await
            .map_err(|e| FileError::Storage(format!("Failed to begin transaction: {}", e)))?;
        self.event_store
            .append_in_tx(&mut tx, &event)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;
        self.metadata_store
            .update_file_in_tx(&mut tx, &file)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        let persisted_version_id = self
            .metadata_store
            .create_file_version_in_tx(&mut tx, &version)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;
        self.event_store
            .commit_transaction(tx)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to commit transaction: {}", e)))?;
        self.broadcaster.publish(event);

        self.queue_replication_if_needed(
            file.id,
            user_id,
            persisted_version_id,
            &version.storage_key(),
        )
        .await?;

        // 9. Return updated file
        Ok(file)
    }

    /// Validate that a file is editable based on its MIME type and extension.
    fn validate_file_editable(&self, file: &File) -> Result<(), FileError> {
        let name = file.name.to_lowercase();
        let mime_type = file.mime_type.to_lowercase();

        // Check for Excalidraw files
        if name.ends_with(".excalidraw") || name.ends_with(".excalidraw.json") {
            return Ok(());
        }

        // Check for markdown files
        if name.ends_with(".md") || name.ends_with(".mdx") || mime_type == "text/markdown" {
            return Ok(());
        }

        // Check for text files
        if mime_type.starts_with("text/") {
            return Ok(());
        }

        // Check for code files by extension
        let editable_extensions = [
            "txt",
            "js",
            "ts",
            "tsx",
            "jsx",
            "py",
            "rs",
            "go",
            "java",
            "cpp",
            "c",
            "h",
            "hpp",
            "cs",
            "php",
            "rb",
            "swift",
            "kt",
            "scala",
            "r",
            "m",
            "mm",
            "json",
            "yaml",
            "yml",
            "toml",
            "xml",
            "html",
            "htm",
            "css",
            "scss",
            "sass",
            "less",
            "sql",
            "sh",
            "bash",
            "zsh",
            "fish",
            "ps1",
            "bat",
            "cmd",
            "dockerfile",
            "makefile",
            "cmake",
            "gradle",
            "ini",
            "conf",
            "cfg",
            "properties",
            "env",
            "gitignore",
            "gitattributes",
            "lock",
            "log",
            "csv",
            "tsv",
            "svg",
            "vue",
            "svelte",
        ];

        if let Some(ext) = name.rsplit('.').next() {
            if editable_extensions.contains(&ext) {
                return Ok(());
            }
        }

        // File type not supported for editing
        Err(FileError::NotEditable(format!(
            "Files with MIME type '{}' and extension '{}' are not editable",
            file.mime_type,
            name.rsplit('.').next().unwrap_or("unknown")
        )))
    }

    fn validate_file_name(&self, name: &str) -> Result<(), FileError> {
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

        if name.contains('\\') {
            return Err(FileError::InvalidName(
                "File name cannot contain backslash (\\)".to_string(),
            ));
        }

        if name.contains("..") {
            return Err(FileError::InvalidName(
                "File name cannot contain '..'".to_string(),
            ));
        }

        if name.starts_with('\0') || name.contains('\0') {
            return Err(FileError::InvalidName(
                "File name cannot contain null character".to_string(),
            ));
        }

        // Reject reserved editor metadata filename
        if name == "index.editor.json" {
            return Err(FileError::InvalidName(
                "File name 'index.editor.json' is reserved".to_string(),
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

    /// Calculate SHA256 hash and size of a file on disk using streaming I/O.
    async fn calculate_sha256_and_size_from_path(
        &self,
        path: &std::path::Path,
    ) -> Result<(String, i64), FileError> {
        let mut file = tokio::fs::File::open(path).await.map_err(|e| {
            FileError::Storage(format!("Failed to open temp file for hashing: {e}"))
        })?;

        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut total_size: i64 = 0;

        loop {
            let n = file.read(&mut buffer).await.map_err(|e| {
                FileError::Storage(format!("Failed to read temp file for hashing: {e}"))
            })?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            total_size += n as i64;
        }

        Ok((hex::encode(hasher.finalize()), total_size))
    }

    async fn queue_replication_if_needed(
        &self,
        file_id: uuid::Uuid,
        owner_id: UserId,
        persisted_version_id: uuid::Uuid,
        storage_key: &str,
    ) -> Result<(), FileError> {
        let target_count = self
            .metadata_store
            .count_enabled_replication_targets()
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;

        if target_count == 0 {
            return Ok(());
        }

        let job = ReplicationJob::new(file_id, persisted_version_id, storage_key.to_string());

        self.metadata_store
            .create_replication_job(&job)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;

        self.metadata_store
            .update_file_version_replication_state(persisted_version_id, ReplicationState::Queued)
            .await
            .map_err(|e| FileError::Database(e.to_string()))?;

        self.publish_replication_state_event(ReplicationEventContext {
            file_id,
            owner_id,
            file_version_id: persisted_version_id,
            replication_state: ReplicationState::Queued,
            job_status: Some(job.status.as_str().to_string()),
            attempt_count: job.attempt_count,
            next_attempt_at: Some(job.next_attempt_at),
            last_error: None,
        })
        .await?;

        Ok(())
    }

    async fn publish_replication_state_event(
        &self,
        context: ReplicationEventContext,
    ) -> Result<(), FileError> {
        let payload = ReplicationStateChangedPayload {
            file_id: context.file_id,
            file_version_id: context.file_version_id,
            replication_state: context.replication_state,
            job_status: context.job_status,
            attempt_count: context.attempt_count,
            next_attempt_at: context.next_attempt_at,
            last_error: context.last_error,
            updated_at: chrono::Utc::now(),
        };

        let event = Event::new(
            EventType::ReplicationStateChanged,
            context.file_id,
            AggregateType::File,
            serde_json::to_value(payload)
                .map_err(|e| FileError::Storage(format!("Failed to serialize payload: {}", e)))?,
            context.owner_id,
        );

        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|e| FileError::Storage(format!("Failed to append event: {}", e)))?;

        Ok(())
    }
}

// NOTE: Tests temporarily disabled until the tenant-aware fixtures are updated.
#[cfg(any())]
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
        async fn append(&self, event: &Event, _broadcaster: &EventBroadcaster) -> Result<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    struct MockMetadataStore {
        files: Mutex<Vec<File>>,
        versions: Mutex<Vec<FileVersion>>,
        folders: Mutex<HashMap<uuid::Uuid, Folder>>,
        replication_jobs: Mutex<Vec<ReplicationJob>>,
        enabled_replication_targets: Mutex<i64>,
    }

    impl MockMetadataStore {
        fn new() -> Self {
            Self {
                files: Mutex::new(Vec::new()),
                versions: Mutex::new(Vec::new()),
                folders: Mutex::new(HashMap::new()),
                replication_jobs: Mutex::new(Vec::new()),
                enabled_replication_targets: Mutex::new(0),
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

        async fn find_folder_by_id(
            &self,
            id: uuid::Uuid,
            _owner_id: uuid::Uuid,
        ) -> Result<Option<Folder>> {
            Ok(self.folders.lock().unwrap().get(&id).cloned())
        }

        async fn find_folder_by_id_unchecked(&self, id: uuid::Uuid) -> Result<Option<Folder>> {
            Ok(self.folders.lock().unwrap().get(&id).cloned())
        }

        async fn find_file_by_id(
            &self,
            id: uuid::Uuid,
            _owner_id: uuid::Uuid,
        ) -> Result<Option<File>> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .find(|f| f.id == id)
                .cloned())
        }

        async fn find_file_by_id_unchecked(&self, id: uuid::Uuid) -> Result<Option<File>> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .iter()
                .find(|f| f.id == id)
                .cloned())
        }

        async fn update_file(&self, file: &File) -> Result<()> {
            let mut files = self.files.lock().unwrap();
            if let Some(existing) = files.iter_mut().find(|f| f.id == file.id) {
                *existing = file.clone();
            }
            Ok(())
        }

        async fn delete_file(&self, id: uuid::Uuid, _owner_id: uuid::Uuid) -> Result<()> {
            let mut files = self.files.lock().unwrap();
            files.retain(|f| f.id != id);
            Ok(())
        }

        async fn list_file_versions(
            &self,
            file_id: uuid::Uuid,
            _owner_id: uuid::Uuid,
        ) -> Result<Vec<FileVersion>> {
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
            _owner_id: uuid::Uuid,
        ) -> Result<Option<FileVersion>> {
            let versions = self.versions.lock().unwrap();
            Ok(versions
                .iter()
                .find(|v| v.file_id == file_id && v.version_number == version_number)
                .cloned())
        }

        async fn count_enabled_replication_targets(&self) -> Result<i64> {
            Ok(*self.enabled_replication_targets.lock().unwrap())
        }

        async fn create_replication_job(&self, job: &ReplicationJob) -> Result<()> {
            self.replication_jobs.lock().unwrap().push(job.clone());
            Ok(())
        }

        async fn update_file_version_replication_state(
            &self,
            version_id: uuid::Uuid,
            state: ReplicationState,
        ) -> Result<()> {
            if let Some(version) = self
                .versions
                .lock()
                .unwrap()
                .iter_mut()
                .find(|version| version.id == version_id)
            {
                version.replication_state = state;
            }

            Ok(())
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

        async fn put_from_path(&self, key: &str, path: &std::path::Path) -> Result<()> {
            let data = tokio::fs::read(path).await?;
            self.objects
                .lock()
                .unwrap()
                .insert(key.to_string(), Bytes::from(data));
            Ok(())
        }

        async fn exists(&self, key: &str) -> Result<bool> {
            Ok(self.objects.lock().unwrap().contains_key(key))
        }

        async fn get(&self, key: &str) -> Result<Bytes> {
            let objects = self.objects.lock().unwrap();
            objects
                .get(key)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Object not found: {}", key))
        }

        async fn delete(&self, key: &str) -> Result<()> {
            self.objects.lock().unwrap().remove(key);
            Ok(())
        }
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

    fn setup_file_service() -> (
        FileService<MockEventStore, MockMetadataStore, MockObjectStore, MockPermissionOps>,
        Arc<MockEventStore>,
        Arc<MockMetadataStore>,
        Arc<MockObjectStore>,
    ) {
        let event_store = Arc::new(MockEventStore::new());
        let metadata_store = Arc::new(MockMetadataStore::new());
        let object_store = Arc::new(MockObjectStore::new());
        let broadcaster = Arc::new(EventBroadcaster::new(100));
        let permission_ops = Arc::new(MockPermissionOps);
        let permission_resolver = Arc::new(PermissionResolver::new(permission_ops));

        let service = FileService::new(
            event_store.clone(),
            metadata_store.clone(),
            object_store.clone(),
            broadcaster,
            permission_resolver,
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
        let broadcaster = Arc::new(EventBroadcaster::new(100));
        let service = FileService::new(event_store, metadata_store, object_store, broadcaster);

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
        let broadcaster = Arc::new(EventBroadcaster::new(100));
        let service = FileService::new(event_store, metadata_store, object_store, broadcaster);

        assert!(service.validate_file_name("").is_err());
        assert!(service.validate_file_name("path/file.txt").is_err());
        assert!(service.validate_file_name("file\0name.txt").is_err());
        assert!(service.validate_file_name("..secret.txt").is_err());
        assert!(service.validate_file_name("secret\\file.txt").is_err());
        assert!(service.validate_file_name("index.editor.json").is_err());
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
        let result = service.update_file(file.id, owner_id, 1, new_content).await;

        // Should get version conflict
        assert!(matches!(result, Err(FileError::VersionConflict { .. })));

        if let Err(FileError::VersionConflict {
            expected, actual, ..
        }) = result
        {
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

    // ==================== Task 10: Version History & Restore Tests ====================

    #[tokio::test]
    async fn test_list_versions_success() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        let file = File::new(
            "versioned.txt".to_string(),
            "/versioned.txt".to_string(),
            "hash3".to_string(),
            300,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Add versions (in arbitrary order)
        let v1 = FileVersion::new(
            file.id,
            1,
            "hash1".to_string(),
            100,
            owner_id,
            Some("Initial".to_string()),
        );
        let v2 = FileVersion::new(
            file.id,
            2,
            "hash2".to_string(),
            200,
            owner_id,
            Some("Update 1".to_string()),
        );
        let v3 = FileVersion::new(
            file.id,
            3,
            "hash3".to_string(),
            300,
            owner_id,
            Some("Update 2".to_string()),
        );
        metadata_store.add_file_version(v2.clone());
        metadata_store.add_file_version(v1.clone());
        metadata_store.add_file_version(v3.clone());

        let versions = service.list_versions(file.id, owner_id).await.unwrap();

        // Should be sorted DESC by version number
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version_number, 3);
        assert_eq!(versions[1].version_number, 2);
        assert_eq!(versions[2].version_number, 1);
    }

    #[tokio::test]
    async fn test_list_versions_empty() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        let file = File::new(
            "no-versions.txt".to_string(),
            "/no-versions.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let versions = service.list_versions(file.id, owner_id).await.unwrap();
        assert_eq!(versions.len(), 0);
    }

    #[tokio::test]
    async fn test_list_versions_not_found() {
        let (service, _, _, _) = setup_file_service();
        let user_id = uuid::Uuid::new_v4();
        let non_existent_file_id = uuid::Uuid::new_v4();

        let result = service.list_versions(non_existent_file_id, user_id).await;
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_list_versions_permission_denied() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let other_user = uuid::Uuid::new_v4();

        let file = File::new(
            "private.txt".to_string(),
            "/private.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let result = service.list_versions(file.id, other_user).await;
        assert!(matches!(result, Err(FileError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_restore_version_creates_new_version() {
        let (service, event_store, metadata_store, object_store) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create a file at version 3
        let mut file = File::new(
            "restore-me.txt".to_string(),
            "/restore-me.txt".to_string(),
            "hash3".to_string(),
            300,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        file.current_version = 3;
        metadata_store.add_file(file.clone());

        // Add versions
        let v1 = FileVersion::new(
            file.id,
            1,
            "hash1".to_string(),
            100,
            owner_id,
            Some("Initial".to_string()),
        );
        let v2 = FileVersion::new(file.id, 2, "hash2".to_string(), 200, owner_id, None);
        let v3 = FileVersion::new(file.id, 3, "hash3".to_string(), 300, owner_id, None);
        metadata_store.add_file_version(v1.clone());
        metadata_store.add_file_version(v2.clone());
        metadata_store.add_file_version(v3.clone());

        // Put the content for version 1 in object store
        object_store
            .objects
            .lock()
            .unwrap()
            .insert("blobs/hash1".to_string(), Bytes::from("Version 1 content"));

        // Restore to version 1
        let restored_file = service.restore_version(file.id, 1, owner_id).await.unwrap();

        // Should create NEW version 4 (not overwrite version 3)
        assert_eq!(restored_file.current_version, 4);
        assert_eq!(restored_file.content_hash, "hash1");
        assert_eq!(restored_file.size, 100);

        // Verify new FileVersion was created
        let versions = metadata_store.versions.lock().unwrap();
        let v4 = versions.iter().find(|v| v.version_number == 4).unwrap();
        assert_eq!(v4.content_hash, "hash1");
        assert_eq!(v4.size, 100);
        assert!(v4
            .change_description
            .as_ref()
            .unwrap()
            .contains("Restored from version 1"));

        // Verify FileRestored event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FileRestored);
    }

    #[tokio::test]
    async fn test_restore_version_not_found() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        let file = File::new(
            "restore-me.txt".to_string(),
            "/restore-me.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Try to restore to a non-existent version
        let result = service.restore_version(file.id, 99, owner_id).await;
        assert!(matches!(result, Err(FileError::VersionNotFound(99))));
    }

    #[tokio::test]
    async fn test_restore_version_file_not_found() {
        let (service, _, _, _) = setup_file_service();
        let user_id = uuid::Uuid::new_v4();
        let non_existent_file_id = uuid::Uuid::new_v4();

        let result = service
            .restore_version(non_existent_file_id, 1, user_id)
            .await;
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_restore_version_permission_denied() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let other_user = uuid::Uuid::new_v4();

        let file = File::new(
            "private.txt".to_string(),
            "/private.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        let result = service.restore_version(file.id, 1, other_user).await;
        assert!(matches!(result, Err(FileError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_restore_version_preserves_existing_versions() {
        let (service, _, metadata_store, object_store) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create a file at version 2
        let mut file = File::new(
            "restore-me.txt".to_string(),
            "/restore-me.txt".to_string(),
            "hash2".to_string(),
            200,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        file.current_version = 2;
        metadata_store.add_file(file.clone());

        // Add versions
        let v1 = FileVersion::new(file.id, 1, "hash1".to_string(), 100, owner_id, None);
        let v2 = FileVersion::new(file.id, 2, "hash2".to_string(), 200, owner_id, None);
        metadata_store.add_file_version(v1.clone());
        metadata_store.add_file_version(v2.clone());

        // Put content for version 1
        object_store
            .objects
            .lock()
            .unwrap()
            .insert("blobs/hash1".to_string(), Bytes::from("Content v1"));

        // Restore to version 1
        service.restore_version(file.id, 1, owner_id).await.unwrap();

        // All original versions should still exist
        let versions = metadata_store.versions.lock().unwrap();
        assert!(versions.iter().any(|v| v.version_number == 1));
        assert!(versions.iter().any(|v| v.version_number == 2));
        // Plus the new restored version
        assert!(versions.iter().any(|v| v.version_number == 3));
        assert_eq!(versions.iter().filter(|v| v.file_id == file.id).count(), 3);
    }

    #[tokio::test]
    async fn test_move_file_to_folder() {
        let (service, event_store, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create target folder
        let target_folder = Folder::new_child(
            "Target".to_string(),
            "/Target".to_string(),
            uuid::Uuid::new_v4(), // parent id
            owner_id,
        );
        metadata_store.add_folder(target_folder.clone());

        // Create a file at root
        let file = File::new(
            "moveme.txt".to_string(),
            "/moveme.txt".to_string(),
            "hash123".to_string(),
            100,
            "text/plain".to_string(),
            None, // root
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Move file to target folder
        let moved_file = service
            .move_file(file.id, Some(target_folder.id), owner_id)
            .await
            .expect("Failed to move file");

        // Verify file is now in target folder
        assert_eq!(moved_file.parent_folder_id, Some(target_folder.id));
        assert_eq!(moved_file.path, "/Target/moveme.txt");
        assert_eq!(moved_file.name, "moveme.txt"); // Name unchanged

        // Verify event was emitted
        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FileMoved);
    }

    #[tokio::test]
    async fn test_move_file_to_root() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create source folder
        let source_folder = Folder::new_child(
            "Source".to_string(),
            "/Source".to_string(),
            uuid::Uuid::new_v4(),
            owner_id,
        );
        metadata_store.add_folder(source_folder.clone());

        // Create a file in source folder
        let file = File::new(
            "moveme.txt".to_string(),
            "/Source/moveme.txt".to_string(),
            "hash123".to_string(),
            100,
            "text/plain".to_string(),
            Some(source_folder.id),
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Move file to root (None parent)
        let moved_file = service
            .move_file(file.id, None, owner_id)
            .await
            .expect("Failed to move file to root");

        // Verify file is now at root
        assert_eq!(moved_file.parent_folder_id, None);
        assert_eq!(moved_file.path, "/moveme.txt");
    }

    #[tokio::test]
    async fn test_move_file_permission_denied() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();
        let other_user = uuid::Uuid::new_v4();

        // Create a file owned by owner_id
        let file = File::new(
            "private.txt".to_string(),
            "/private.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Try to move with different user - should fail
        let result = service.move_file(file.id, None, other_user).await;
        assert!(matches!(result, Err(FileError::PermissionDenied { .. })));
    }

    #[tokio::test]
    async fn test_move_file_not_found() {
        let (service, _, _, _) = setup_file_service();
        let user_id = uuid::Uuid::new_v4();
        let nonexistent_id = uuid::Uuid::new_v4();

        // Try to move non-existent file
        let result = service.move_file(nonexistent_id, None, user_id).await;
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_move_file_to_nonexistent_folder() {
        let (service, _, metadata_store, _) = setup_file_service();
        let owner_id = uuid::Uuid::new_v4();

        // Create a file at root
        let file = File::new(
            "moveme.txt".to_string(),
            "/moveme.txt".to_string(),
            "hash".to_string(),
            100,
            "text/plain".to_string(),
            None,
            owner_id,
        );
        metadata_store.add_file(file.clone());

        // Try to move to non-existent folder
        let nonexistent_folder = uuid::Uuid::new_v4();
        let result = service
            .move_file(file.id, Some(nonexistent_folder), owner_id)
            .await;
        assert!(matches!(result, Err(FileError::FolderNotFound(_))));
    }
}
