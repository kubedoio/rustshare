//! Upload service for resumable file uploads
//!
//! This service provides business logic for managing upload sessions,
//! handling chunked uploads, and assembling files on completion.

use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::domain::{File, FolderId, UserId};
use crate::events::{
    AggregateType, Event, EventBroadcaster, EventType, FileModifiedPayload, FileUploadedPayload,
};
use crate::services::upload_session::*;
use crate::services::FileError;

/// Errors that can occur during upload operations
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error("Upload session not found: {0}")]
    SessionNotFound(Uuid),

    #[error("Upload session expired: {0}")]
    SessionExpired(Uuid),

    #[error("Upload session already completed: {0}")]
    SessionAlreadyCompleted(Uuid),

    #[error("Upload session aborted: {0}")]
    SessionAborted(Uuid),

    #[error("Chunk index out of range: {index} (total: {total})")]
    ChunkIndexOutOfRange { index: u32, total: u32 },

    #[error("Chunk already received: {0}")]
    ChunkAlreadyReceived(u32),

    #[error("Chunk hash verification failed")]
    ChunkHashVerificationFailed,

    #[error("File hash verification failed")]
    FileHashVerificationFailed,

    #[error("Invalid chunk size: expected {expected}, got {actual}")]
    InvalidChunkSize { expected: u64, actual: u64 },

    #[error("Permission denied: user {user_id} cannot access session {session_id}")]
    PermissionDenied { user_id: UserId, session_id: Uuid },

    #[error("Parent folder not found: {0}")]
    ParentFolderNotFound(FolderId),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("File name error: {0}")]
    InvalidFileName(String),
}

impl From<UploadError> for FileError {
    fn from(err: UploadError) -> Self {
        match err {
            UploadError::ParentFolderNotFound(id) => FileError::ParentFolderNotFound(id),
            UploadError::PermissionDenied { session_id, .. } => FileError::PermissionDenied {
                file_id: session_id,
                user_id: Uuid::nil(),
            },
            UploadError::InvalidFileName(msg) => FileError::InvalidName(msg),
            UploadError::Storage(msg) => FileError::Storage(msg),
            UploadError::Database(msg) => FileError::Storage(msg),
            _ => FileError::Storage(err.to_string()),
        }
    }
}

/// Repository trait for upload session operations
#[async_trait::async_trait]
pub trait UploadSessionRepository: Send + Sync {
    /// Create a new upload session
    async fn create_session(&self, session: &UploadSession) -> Result<(), UploadError>;

    /// Get an upload session by ID
    async fn get_session(&self, id: Uuid) -> Result<Option<UploadSession>, UploadError>;

    /// Update an upload session
    async fn update_session(&self, session: &UploadSession) -> Result<(), UploadError>;

    /// Update chunk received status for a session
    async fn update_chunk_received(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        chunk_hash: &str,
        size: u64,
    ) -> Result<(), UploadError>;

    /// Get chunk info for a session
    async fn get_chunk_info(
        &self,
        session_id: Uuid,
        chunk_index: u32,
    ) -> Result<Option<ChunkInfo>, UploadError>;

    /// Mark session as completed
    async fn complete_session(&self, session_id: Uuid, file_id: Uuid) -> Result<(), UploadError>;

    /// Mark session as aborted
    async fn abort_session(&self, session_id: Uuid) -> Result<(), UploadError>;

    /// Delete a session and its chunks
    async fn delete_session(&self, session_id: Uuid) -> Result<(), UploadError>;

    /// List expired sessions
    async fn list_expired_sessions(
        &self,
        before: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<UploadSession>, UploadError>;

    /// List sessions for a user
    async fn list_user_sessions(&self, user_id: UserId) -> Result<Vec<UploadSession>, UploadError>;
}

/// Source of chunk data for an upload.
enum ChunkSource {
    Bytes(Bytes),
    Path(std::path::PathBuf),
}

/// Object store operations for upload service
#[async_trait::async_trait]
pub trait UploadObjectStore: Send + Sync {
    /// Store a chunk from in-memory bytes.
    async fn put_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        data: Bytes,
    ) -> Result<(), UploadError>;

    /// Store a chunk by streaming from a local file path.
    ///
    /// This avoids loading the chunk into memory and is used by HTTP handlers
    /// that buffer chunks to disk.
    async fn put_chunk_from_path(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        path: &std::path::Path,
    ) -> Result<(), UploadError>;

    /// Get a chunk
    async fn get_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
    ) -> Result<Option<Bytes>, UploadError>;

    /// Delete a chunk
    async fn delete_chunk(&self, session_id: Uuid, chunk_index: u32) -> Result<(), UploadError>;

    /// Delete all chunks for a session
    async fn delete_session_chunks(
        &self,
        session_id: Uuid,
        total_chunks: u32,
    ) -> Result<(), UploadError>;

    /// Check if a chunk exists
    async fn chunk_exists(&self, session_id: Uuid, chunk_index: u32) -> Result<bool, UploadError>;

    /// Assemble chunks into a content-addressed final file and return the final SHA-256 hash.
    async fn assemble_chunks_to_prefix(
        &self,
        session_id: Uuid,
        total_chunks: u32,
        final_key_prefix: &str,
    ) -> Result<String, UploadError>;
}

/// Metadata store operations for upload service
#[async_trait::async_trait]
pub trait UploadMetadataStore: Send + Sync {
    /// Find a folder by ID (owner-filtered)
    async fn find_folder_by_id(
        &self,
        id: Uuid,
        owner_id: UserId,
    ) -> Result<Option<crate::domain::Folder>, UploadError>;

    /// Find a folder by ID without owner filtering.
    /// Callers must verify access before using this method.
    async fn find_folder_by_id_unchecked(
        &self,
        id: Uuid,
    ) -> Result<Option<crate::domain::Folder>, UploadError>;

    /// Find a file by canonical path for an owner
    async fn find_file_by_path(
        &self,
        path: &str,
        owner_id: Uuid,
    ) -> Result<Option<File>, UploadError>;

    /// Create a file
    async fn create_file(&self, file: &File) -> Result<(), UploadError>;

    /// Update an existing file
    async fn update_file(&self, file: &File) -> Result<(), UploadError>;

    /// Create a file version
    async fn create_file_version(
        &self,
        file: &File,
        version: &crate::domain::FileVersion,
    ) -> Result<(), UploadError>;
}

/// Upload service for managing resumable uploads
pub struct UploadService<R, O, M, E>
where
    R: UploadSessionRepository,
    O: UploadObjectStore,
    M: UploadMetadataStore,
    E: crate::services::file_service::EventStoreOps,
{
    repository: Arc<R>,
    object_store: Arc<O>,
    metadata_store: Arc<M>,
    event_store: Arc<E>,
    broadcaster: Arc<EventBroadcaster>,
}

impl<R, O, M, E> UploadService<R, O, M, E>
where
    R: UploadSessionRepository,
    O: UploadObjectStore,
    M: UploadMetadataStore,
    E: crate::services::file_service::EventStoreOps,
{
    /// Create a new upload service
    pub fn new(
        repository: Arc<R>,
        object_store: Arc<O>,
        metadata_store: Arc<M>,
        event_store: Arc<E>,
        broadcaster: Arc<EventBroadcaster>,
    ) -> Self {
        Self {
            repository,
            object_store,
            metadata_store,
            event_store,
            broadcaster,
        }
    }

    /// Create a new upload session
    pub async fn create_session(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
        request: CreateSessionRequest,
    ) -> Result<CreateSessionResponse, UploadError> {
        // Validate file name
        crate::validation::validate_name(&request.file_name)
            .map_err(UploadError::InvalidFileName)?;

        // Validate parent folder if provided
        if let Some(folder_id) = request.folder_id {
            self.metadata_store
                .find_folder_by_id_unchecked(folder_id)
                .await?
                .ok_or(UploadError::ParentFolderNotFound(folder_id))?;
            // Permission check is the responsibility of the caller (handler layer)
            // so that shared folder uploads are supported.
        }

        // Validate chunk size (min 1MB, max 100MB)
        let chunk_size = request.chunk_size.clamp(1024 * 1024, 100 * 1024 * 1024);

        let session_id = Uuid::new_v4();
        let session = UploadSession::new(
            session_id,
            tenant_id,
            user_id,
            request.folder_id,
            request.file_name,
            request.mime_type,
            request.total_size,
            chunk_size,
            request.file_hash,
        );

        let total_chunks = session.total_chunks();
        let expires_at = session.expires_at;

        self.repository.create_session(&session).await?;

        Ok(CreateSessionResponse {
            session_id,
            total_chunks,
            chunk_size,
            expires_at,
        })
    }

    /// Get session status for resuming uploads
    pub async fn get_session_status(
        &self,
        session_id: Uuid,
        user_id: UserId,
    ) -> Result<SessionStatusResponse, UploadError> {
        let session = self
            .repository
            .get_session(session_id)
            .await?
            .ok_or(UploadError::SessionNotFound(session_id))?;

        // Verify ownership
        if session.owner_id != user_id {
            return Err(UploadError::PermissionDenied {
                user_id,
                session_id,
            });
        }

        if session.status == UploadSessionStatus::Aborted {
            return Err(UploadError::SessionAborted(session_id));
        }

        Ok(SessionStatusResponse::from_session(&session))
    }

    /// Return the target folder for an upload session after verifying ownership.
    pub async fn get_session_target_folder(
        &self,
        session_id: Uuid,
        user_id: UserId,
    ) -> Result<Option<FolderId>, UploadError> {
        let session = self
            .repository
            .get_session(session_id)
            .await?
            .ok_or(UploadError::SessionNotFound(session_id))?;

        if session.owner_id != user_id {
            return Err(UploadError::PermissionDenied {
                user_id,
                session_id,
            });
        }

        Ok(session.folder_id)
    }

    /// Upload a chunk from in-memory bytes.
    #[allow(clippy::too_many_arguments)]
    pub async fn upload_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        data: Bytes,
        provided_hash: Option<String>,
        user_id: UserId,
    ) -> Result<ChunkUploadResponse, UploadError> {
        let actual_size = data.len() as u64;
        let chunk_hash = crate::validation::calculate_sha256(&data);
        self.upload_chunk_impl(
            session_id,
            chunk_index,
            actual_size,
            chunk_hash,
            ChunkSource::Bytes(data),
            provided_hash,
            user_id,
        )
        .await
    }

    /// Upload a chunk whose contents are stored on disk at `chunk_path`.
    ///
    /// The chunk is hashed and stored without being fully loaded into memory.
    pub async fn upload_chunk_from_path(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        chunk_path: &std::path::Path,
        provided_hash: Option<String>,
        user_id: UserId,
    ) -> Result<ChunkUploadResponse, UploadError> {
        let (chunk_hash, actual_size) =
            Self::calculate_sha256_and_size_from_path(chunk_path).await?;
        self.upload_chunk_impl(
            session_id,
            chunk_index,
            actual_size,
            chunk_hash,
            ChunkSource::Path(chunk_path.to_path_buf()),
            provided_hash,
            user_id,
        )
        .await
    }

    /// Shared implementation for chunk uploads from either bytes or a file path.
    #[allow(clippy::too_many_arguments)]
    async fn upload_chunk_impl(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        actual_size: u64,
        chunk_hash: String,
        source: ChunkSource,
        provided_hash: Option<String>,
        user_id: UserId,
    ) -> Result<ChunkUploadResponse, UploadError> {
        let mut session = self
            .repository
            .get_session(session_id)
            .await?
            .ok_or(UploadError::SessionNotFound(session_id))?;

        // Verify ownership
        if session.owner_id != user_id {
            return Err(UploadError::PermissionDenied {
                user_id,
                session_id,
            });
        }

        // Check session state
        if session.is_expired() {
            return Err(UploadError::SessionExpired(session_id));
        }

        if session.status == UploadSessionStatus::Aborted {
            return Err(UploadError::SessionAborted(session_id));
        }

        if session.status == UploadSessionStatus::Completed {
            return Err(UploadError::SessionAlreadyCompleted(session_id));
        }

        // Validate chunk index
        let total_chunks = session.total_chunks();
        if chunk_index >= total_chunks {
            return Err(UploadError::ChunkIndexOutOfRange {
                index: chunk_index,
                total: total_chunks,
            });
        }

        // Check if chunk already received (idempotent - return success)
        if session.has_chunk(chunk_index) {
            return Ok(ChunkUploadResponse {
                session_id,
                chunk_index,
                verified: true,
                progress_percent: session.progress_percent(),
                is_complete: session.is_complete(),
            });
        }

        // Validate chunk size (last chunk may be smaller)
        let expected_size = if chunk_index == total_chunks - 1 {
            // Last chunk
            let remainder = session.total_size % session.chunk_size;
            if remainder == 0 {
                session.chunk_size
            } else {
                remainder
            }
        } else {
            session.chunk_size
        };

        if actual_size > expected_size {
            return Err(UploadError::InvalidChunkSize {
                expected: expected_size,
                actual: actual_size,
            });
        }

        // Verify hash if provided
        if let Some(expected_hash) = provided_hash {
            if expected_hash != chunk_hash {
                return Err(UploadError::ChunkHashVerificationFailed);
            }
        }

        let store_result = match source {
            ChunkSource::Bytes(data) => {
                self.object_store
                    .put_chunk(session_id, chunk_index, data)
                    .await
            }
            ChunkSource::Path(path) => {
                self.object_store
                    .put_chunk_from_path(session_id, chunk_index, &path)
                    .await
            }
        };

        if let Err(err) = store_result {
            if matches!(err, UploadError::ChunkAlreadyReceived(_)) {
                return Ok(ChunkUploadResponse {
                    session_id,
                    chunk_index,
                    verified: true,
                    progress_percent: session.progress_percent(),
                    is_complete: session.is_complete(),
                });
            }
            return Err(err);
        }

        // Update session
        session.mark_in_progress();
        session.mark_chunk_received(chunk_index);
        session.add_uploaded_bytes(actual_size);

        if let Err(err) = self
            .repository
            .update_chunk_received(session_id, chunk_index, &chunk_hash, actual_size)
            .await
        {
            if matches!(err, UploadError::ChunkAlreadyReceived(_)) {
                return Ok(ChunkUploadResponse {
                    session_id,
                    chunk_index,
                    verified: true,
                    progress_percent: session.progress_percent(),
                    is_complete: session.is_complete(),
                });
            }
            return Err(err);
        }
        self.repository.update_session(&session).await?;

        Ok(ChunkUploadResponse {
            session_id,
            chunk_index,
            verified: true,
            progress_percent: session.progress_percent(),
            is_complete: session.is_complete(),
        })
    }

    /// Calculate SHA256 hash and size of a chunk file using streaming I/O.
    async fn calculate_sha256_and_size_from_path(
        path: &std::path::Path,
    ) -> Result<(String, u64), UploadError> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| UploadError::Storage(format!("Failed to open chunk file: {e}")))?;

        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut total_size: u64 = 0;

        loop {
            let n = file
                .read(&mut buffer)
                .await
                .map_err(|e| UploadError::Storage(format!("Failed to read chunk file: {e}")))?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            total_size += n as u64;
        }

        Ok((hex::encode(hasher.finalize()), total_size))
    }

    /// Complete the upload and assemble the file
    pub async fn complete_upload(
        &self,
        session_id: Uuid,
        user_id: UserId,
    ) -> Result<CompleteUploadResponse, UploadError> {
        let mut session = self
            .repository
            .get_session(session_id)
            .await?
            .ok_or(UploadError::SessionNotFound(session_id))?;

        // Verify ownership
        if session.owner_id != user_id {
            return Err(UploadError::PermissionDenied {
                user_id,
                session_id,
            });
        }

        // Check session state
        if session.is_expired() {
            return Err(UploadError::SessionExpired(session_id));
        }

        if session.status == UploadSessionStatus::Aborted {
            return Err(UploadError::SessionAborted(session_id));
        }

        if session.status == UploadSessionStatus::Completed {
            // Already completed, return existing file info
            if let Some(file_id) = session.file_id {
                return Ok(CompleteUploadResponse {
                    session_id,
                    file_id,
                    file_name: session.file_name.clone(),
                    file_size: session.total_size,
                    content_hash: session.file_hash.clone().unwrap_or_default(),
                });
            }
        }

        // Verify all chunks received
        if !session.is_complete() {
            let missing = session.missing_chunks();
            return Err(UploadError::Storage(format!(
                "Upload incomplete. Missing {} chunks",
                missing.len()
            )));
        }

        // Assemble chunks into the final object without materializing the full
        // file in memory. The object store computes the final SHA-256 while
        // streaming the chunks.
        let final_hash = self
            .object_store
            .assemble_chunks_to_prefix(session_id, session.total_chunks(), "blobs/")
            .await?;

        // Verify final hash if expected
        if let Some(expected_hash) = &session.file_hash {
            if expected_hash != &final_hash {
                return Err(UploadError::FileHashVerificationFailed);
            }
        }

        // Get parent folder path and determine file owner
        let (parent_path, file_owner_id) = if let Some(folder_id) = session.folder_id {
            let folder = self
                .metadata_store
                .find_folder_by_id_unchecked(folder_id)
                .await?
                .ok_or(UploadError::ParentFolderNotFound(folder_id))?;
            // Files in shared folders are owned by the folder owner so that
            // deduplication and versioning work within the shared namespace.
            (folder.path.clone(), folder.owner_id)
        } else {
            (String::new(), user_id)
        };

        // Construct file path
        let path = if parent_path.is_empty() || parent_path == "/" {
            format!("/{}", session.file_name)
        } else {
            format!("{}/{}", parent_path, session.file_name)
        };

        let storage_key = format!("blobs/{}", final_hash);

        let file = if let Some(mut existing) = self
            .metadata_store
            .find_file_by_path(&path, file_owner_id)
            .await?
        {
            if existing.content_hash == final_hash && existing.size == session.total_size as i64 {
                existing
            } else {
                let old_version = existing.current_version;
                let old_content_hash = existing.content_hash.clone();
                let old_size = existing.size;

                existing.content_hash = final_hash.clone();
                existing.size = session.total_size as i64;
                existing.mime_type = session.mime_type.clone();
                existing.parent_folder_id = session.folder_id;
                existing.current_version += 1;
                existing.modified_at = chrono::Utc::now();

                self.metadata_store.update_file(&existing).await?;

                let version = crate::domain::FileVersion::new(
                    existing.id,
                    existing.current_version,
                    final_hash.clone(),
                    session.total_size as i64,
                    user_id,
                    Some("Uploaded via resumable session".to_string()),
                    session.tenant_id,
                );
                self.metadata_store
                    .create_file_version(&existing, &version)
                    .await?;

                let payload = FileModifiedPayload {
                    file_id: existing.id,
                    old_version,
                    new_version: existing.current_version,
                    old_content_hash,
                    new_content_hash: final_hash.clone(),
                    old_size,
                    new_size: session.total_size as i64,
                    storage_key: storage_key.clone(),
                    modified_by: user_id,
                };

                let event = Event::new(
                    EventType::FileModified,
                    existing.id,
                    AggregateType::File,
                    serde_json::to_value(&payload).map_err(|e| {
                        UploadError::Storage(format!("Failed to serialize event: {}", e))
                    })?,
                    user_id,
                );

                self.event_store
                    .append(&event, &self.broadcaster)
                    .await
                    .map_err(|e| UploadError::Storage(format!("Failed to append event: {}", e)))?;

                existing
            }
        } else {
            // Create file metadata
            let file = File::new(
                session.file_name.clone(),
                path.clone(),
                final_hash.clone(),
                session.total_size as i64,
                session.mime_type.clone(),
                session.folder_id,
                file_owner_id,
                session.tenant_id,
            );

            // Persist file
            self.metadata_store.create_file(&file).await?;

            // Create initial version
            let version = crate::domain::FileVersion::new(
                file.id,
                1,
                final_hash.clone(),
                session.total_size as i64,
                user_id,
                Some("Uploaded via resumable session".to_string()),
                session.tenant_id,
            );
            self.metadata_store
                .create_file_version(&file, &version)
                .await?;

            // Emit event
            let payload = FileUploadedPayload {
                file_id: file.id,
                name: session.file_name.clone(),
                path: path.clone(),
                size: session.total_size as i64,
                content_hash: final_hash.clone(),
                storage_key: storage_key.clone(),
                mime_type: session.mime_type.clone(),
                owner_id: user_id,
                parent_folder_id: session.folder_id,
                actor_type: "user".to_string(),
                actor_user_id: Some(user_id),
                actor_share_id: None,
                actor_share_session_id: None,
                actor_display_name: None,
            };

            let event = Event::new(
                EventType::FileUploaded,
                file.id,
                AggregateType::File,
                serde_json::to_value(&payload).map_err(|e| {
                    UploadError::Storage(format!("Failed to serialize event: {}", e))
                })?,
                user_id,
            );

            self.event_store
                .append(&event, &self.broadcaster)
                .await
                .map_err(|e| UploadError::Storage(format!("Failed to append event: {}", e)))?;

            file
        };

        // Mark session complete
        session.mark_completed(file.id);
        self.repository
            .complete_session(session_id, file.id)
            .await?;

        // Cleanup chunks - best effort, log but don't fail if cleanup fails
        if let Err(e) = self
            .object_store
            .delete_session_chunks(session_id, session.total_chunks())
            .await
        {
            tracing::warn!(session_id = %session_id, error = %e, "failed to cleanup upload chunks");
        }

        Ok(CompleteUploadResponse {
            session_id,
            file_id: file.id,
            file_name: session.file_name,
            file_size: session.total_size,
            content_hash: final_hash,
        })
    }

    /// Abort an upload session
    pub async fn abort_session(
        &self,
        session_id: Uuid,
        user_id: UserId,
    ) -> Result<(), UploadError> {
        let session = self
            .repository
            .get_session(session_id)
            .await?
            .ok_or(UploadError::SessionNotFound(session_id))?;

        // Verify ownership
        if session.owner_id != user_id {
            return Err(UploadError::PermissionDenied {
                user_id,
                session_id,
            });
        }

        if session.status == UploadSessionStatus::Completed {
            return Err(UploadError::SessionAlreadyCompleted(session_id));
        }

        // Delete chunks - best effort during abort
        if let Err(e) = self
            .object_store
            .delete_session_chunks(session_id, session.total_chunks())
            .await
        {
            tracing::warn!(session_id = %session_id, error = %e, "failed to delete chunks during abort");
        }

        // Mark as aborted
        self.repository.abort_session(session_id).await?;

        Ok(())
    }

    /// List active upload sessions for a user
    pub async fn list_user_sessions(
        &self,
        user_id: UserId,
    ) -> Result<Vec<SessionStatusResponse>, UploadError> {
        let sessions = self.repository.list_user_sessions(user_id).await?;
        Ok(sessions
            .into_iter()
            .map(|s| SessionStatusResponse::from_session(&s))
            .collect())
    }

    /// Cleanup expired sessions
    pub async fn cleanup_expired_sessions(&self) -> Result<u32, UploadError> {
        let expired = self
            .repository
            .list_expired_sessions(chrono::Utc::now())
            .await?;

        let mut cleaned = 0;
        for session in expired {
            // Delete chunks - best effort during cleanup
            if let Err(e) = self
                .object_store
                .delete_session_chunks(session.id, session.total_chunks())
                .await
            {
                tracing::warn!(session_id = %session.id, error = %e, "failed to delete chunks during cleanup");
            }

            // Delete session
            if let Err(e) = self.repository.delete_session(session.id).await {
                tracing::warn!(session_id = %session.id, error = %e, "failed to delete expired session");
            }
            cleaned += 1;
        }

        Ok(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileVersion, Folder};
    use crate::services::file_service::EventStoreOps;
    use bytes::Bytes;
    use chrono::Utc;
    use std::sync::Mutex;

    struct MockUploadRepo {
        session: Mutex<Option<UploadSession>>,
        completed: Mutex<Vec<(Uuid, Uuid)>>,
    }

    impl MockUploadRepo {
        fn new(session: UploadSession) -> Self {
            Self {
                session: Mutex::new(Some(session)),
                completed: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl UploadSessionRepository for MockUploadRepo {
        async fn create_session(&self, _session: &UploadSession) -> Result<(), UploadError> {
            unreachable!()
        }

        async fn get_session(&self, _id: Uuid) -> Result<Option<UploadSession>, UploadError> {
            Ok(self.session.lock().unwrap().clone())
        }

        async fn update_session(&self, session: &UploadSession) -> Result<(), UploadError> {
            *self.session.lock().unwrap() = Some(session.clone());
            Ok(())
        }

        async fn update_chunk_received(
            &self,
            _session_id: Uuid,
            _chunk_index: u32,
            _chunk_hash: &str,
            _size: u64,
        ) -> Result<(), UploadError> {
            unreachable!()
        }

        async fn get_chunk_info(
            &self,
            _session_id: Uuid,
            _chunk_index: u32,
        ) -> Result<Option<ChunkInfo>, UploadError> {
            unreachable!()
        }

        async fn complete_session(
            &self,
            session_id: Uuid,
            file_id: Uuid,
        ) -> Result<(), UploadError> {
            self.completed.lock().unwrap().push((session_id, file_id));
            if let Some(session) = self.session.lock().unwrap().as_mut() {
                session.file_id = Some(file_id);
                session.status = UploadSessionStatus::Completed;
            }
            Ok(())
        }

        async fn abort_session(&self, _session_id: Uuid) -> Result<(), UploadError> {
            unreachable!()
        }

        async fn delete_session(&self, _session_id: Uuid) -> Result<(), UploadError> {
            Ok(())
        }

        async fn list_expired_sessions(
            &self,
            _before: chrono::DateTime<chrono::Utc>,
        ) -> Result<Vec<UploadSession>, UploadError> {
            Ok(Vec::new())
        }

        async fn list_user_sessions(
            &self,
            _user_id: UserId,
        ) -> Result<Vec<UploadSession>, UploadError> {
            Ok(Vec::new())
        }
    }

    struct MockUploadObjectStore {
        assembled: Mutex<Vec<(Uuid, u32, String)>>,
    }

    impl MockUploadObjectStore {
        fn new() -> Self {
            Self {
                assembled: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl UploadObjectStore for MockUploadObjectStore {
        async fn put_chunk(
            &self,
            _session_id: Uuid,
            _chunk_index: u32,
            _data: Bytes,
        ) -> Result<(), UploadError> {
            unreachable!()
        }

        async fn put_chunk_from_path(
            &self,
            _session_id: Uuid,
            _chunk_index: u32,
            _path: &std::path::Path,
        ) -> Result<(), UploadError> {
            unreachable!()
        }

        async fn get_chunk(
            &self,
            _session_id: Uuid,
            _chunk_index: u32,
        ) -> Result<Option<Bytes>, UploadError> {
            Ok(Some(Bytes::new()))
        }

        async fn delete_chunk(
            &self,
            _session_id: Uuid,
            _chunk_index: u32,
        ) -> Result<(), UploadError> {
            Ok(())
        }

        async fn delete_session_chunks(
            &self,
            _session_id: Uuid,
            _total_chunks: u32,
        ) -> Result<(), UploadError> {
            Ok(())
        }

        async fn chunk_exists(
            &self,
            _session_id: Uuid,
            _chunk_index: u32,
        ) -> Result<bool, UploadError> {
            Ok(true)
        }

        async fn assemble_chunks_to_prefix(
            &self,
            session_id: Uuid,
            total_chunks: u32,
            final_key_prefix: &str,
        ) -> Result<String, UploadError> {
            self.assembled.lock().unwrap().push((
                session_id,
                total_chunks,
                final_key_prefix.to_string(),
            ));
            Ok(crate::validation::calculate_sha256(&Bytes::new()))
        }
    }

    struct MockUploadMetadataStore {
        existing_file: Mutex<Option<File>>,
        created_files: Mutex<Vec<File>>,
        updated_files: Mutex<Vec<File>>,
        created_versions: Mutex<Vec<FileVersion>>,
    }

    impl MockUploadMetadataStore {
        fn new(existing_file: Option<File>) -> Self {
            Self {
                existing_file: Mutex::new(existing_file),
                created_files: Mutex::new(Vec::new()),
                updated_files: Mutex::new(Vec::new()),
                created_versions: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl UploadMetadataStore for MockUploadMetadataStore {
        async fn find_folder_by_id(
            &self,
            _id: Uuid,
            _owner_id: UserId,
        ) -> Result<Option<Folder>, UploadError> {
            Ok(None)
        }

        async fn find_folder_by_id_unchecked(
            &self,
            _id: Uuid,
        ) -> Result<Option<Folder>, UploadError> {
            Ok(None)
        }

        async fn find_file_by_path(
            &self,
            _path: &str,
            _owner_id: Uuid,
        ) -> Result<Option<File>, UploadError> {
            Ok(self.existing_file.lock().unwrap().clone())
        }

        async fn create_file(&self, file: &File) -> Result<(), UploadError> {
            self.created_files.lock().unwrap().push(file.clone());
            Ok(())
        }

        async fn update_file(&self, file: &File) -> Result<(), UploadError> {
            self.updated_files.lock().unwrap().push(file.clone());
            *self.existing_file.lock().unwrap() = Some(file.clone());
            Ok(())
        }

        async fn create_file_version(
            &self,
            _file: &File,
            version: &FileVersion,
        ) -> Result<(), UploadError> {
            self.created_versions.lock().unwrap().push(version.clone());
            Ok(())
        }
    }

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
        type Tx = ();

        async fn append(
            &self,
            event: &Event,
            _broadcaster: &EventBroadcaster,
        ) -> anyhow::Result<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }

        async fn begin_transaction(&self) -> anyhow::Result<Self::Tx> {
            Ok(())
        }

        async fn commit_transaction(&self, _tx: Self::Tx) -> anyhow::Result<()> {
            Ok(())
        }

        async fn append_in_tx(&self, _tx: &mut Self::Tx, event: &Event) -> anyhow::Result<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    #[test]
    fn test_validate_file_name() {
        // Placeholder for future validation tests
    }

    #[tokio::test]
    async fn complete_upload_updates_existing_file_instead_of_creating_duplicate() {
        let owner_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        let mut session = UploadSession::new(
            session_id,
            tenant_id,
            owner_id,
            None,
            "note1.md".to_string(),
            "text/markdown".to_string(),
            0,
            1024 * 1024,
            None,
        );
        session.status = UploadSessionStatus::InProgress;
        session.mark_chunk_received(0);
        session.uploaded_bytes = 0;

        let existing_file = File {
            id: Uuid::new_v4(),
            name: "note1.md".to_string(),
            path: "/note1.md".to_string(),
            content_hash: "old-hash".to_string(),
            size: 4,
            mime_type: "text/markdown".to_string(),
            parent_folder_id: None,
            owner_id,
            current_version: 1,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            starred_at: None,
            deleted_at: None,
            tenant_id,
        };

        let repo = Arc::new(MockUploadRepo::new(session));
        let object_store = Arc::new(MockUploadObjectStore::new());
        let metadata_store = Arc::new(MockUploadMetadataStore::new(Some(existing_file.clone())));
        let event_store = Arc::new(MockEventStore::new());
        let broadcaster = Arc::new(EventBroadcaster::new(16));

        let service = UploadService::new(
            repo.clone(),
            object_store,
            metadata_store.clone(),
            event_store.clone(),
            broadcaster,
        );

        let response = service.complete_upload(session_id, owner_id).await.unwrap();

        assert_eq!(response.file_id, existing_file.id);
        assert!(metadata_store.created_files.lock().unwrap().is_empty());
        assert_eq!(metadata_store.updated_files.lock().unwrap().len(), 1);
        assert_eq!(metadata_store.created_versions.lock().unwrap().len(), 1);
        assert_eq!(
            repo.completed.lock().unwrap().as_slice(),
            &[(session_id, existing_file.id)]
        );

        let updated = metadata_store.updated_files.lock().unwrap()[0].clone();
        assert_eq!(updated.id, existing_file.id);
        assert_eq!(updated.path, "/note1.md");
        assert_eq!(updated.current_version, 2);

        let events = event_store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::FileModified);
        let payload = events[0].payload.clone();
        assert_eq!(
            payload["file_id"].as_str(),
            Some(existing_file.id.to_string().as_str())
        );
    }
}
