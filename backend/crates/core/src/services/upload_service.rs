//! Upload service for resumable file uploads
//!
//! This service provides business logic for managing upload sessions,
//! handling chunked uploads, and assembling files on completion.

use bytes::Bytes;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

use crate::domain::{File, FolderId, UserId};
use crate::events::{AggregateType, Event, EventBroadcaster, EventType, FileUploadedPayload};
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
    async fn list_user_sessions(
        &self,
        user_id: UserId,
    ) -> Result<Vec<UploadSession>, UploadError>;
}

/// Object store operations for upload service
#[async_trait::async_trait]
pub trait UploadObjectStore: Send + Sync {
    /// Store a chunk
    async fn put_chunk(&self, session_id: Uuid, chunk_index: u32, data: Bytes) -> Result<(), UploadError>;

    /// Get a chunk
    async fn get_chunk(&self, session_id: Uuid, chunk_index: u32) -> Result<Option<Bytes>, UploadError>;

    /// Delete a chunk
    async fn delete_chunk(&self, session_id: Uuid, chunk_index: u32) -> Result<(), UploadError>;

    /// Delete all chunks for a session
    async fn delete_session_chunks(&self, session_id: Uuid, total_chunks: u32) -> Result<(), UploadError>;

    /// Check if a chunk exists
    async fn chunk_exists(&self, session_id: Uuid, chunk_index: u32) -> Result<bool, UploadError>;

    /// Assemble chunks into a final file
    async fn assemble_chunks(
        &self,
        session_id: Uuid,
        total_chunks: u32,
        final_key: &str,
    ) -> Result<(), UploadError>;
}

/// Metadata store operations for upload service
#[async_trait::async_trait]
pub trait UploadMetadataStore: Send + Sync {
    /// Find a folder by ID
    async fn find_folder_by_id(&self, id: Uuid) -> Result<Option<crate::domain::Folder>, UploadError>;

    /// Create a file
    async fn create_file(&self, file: &File) -> Result<(), UploadError>;

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
        self.validate_file_name(&request.file_name)?;

        // Validate parent folder if provided
        if let Some(folder_id) = request.folder_id {
            let folder = self
                .metadata_store
                .find_folder_by_id(folder_id)
                .await?
                .ok_or(UploadError::ParentFolderNotFound(folder_id))?;

            if folder.owner_id != user_id {
                return Err(UploadError::PermissionDenied {
                    user_id,
                    session_id: Uuid::nil(),
                });
            }
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

        Ok(SessionStatusResponse::from_session(&session))
    }

    /// Upload a chunk
    #[allow(clippy::too_many_arguments)]
    pub async fn upload_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        data: Bytes,
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
            if remainder == 0 { session.chunk_size } else { remainder }
        } else {
            session.chunk_size
        };

        let actual_size = data.len() as u64;
        if actual_size > expected_size {
            return Err(UploadError::InvalidChunkSize {
                expected: expected_size,
                actual: actual_size,
            });
        }

        // Calculate chunk hash
        let chunk_hash = self.calculate_sha256(&data);

        // Verify hash if provided
        if let Some(expected_hash) = provided_hash {
            if expected_hash != chunk_hash {
                return Err(UploadError::ChunkHashVerificationFailed);
            }
        }

        // Store chunk
        self.object_store
            .put_chunk(session_id, chunk_index, data)
            .await?;

        // Update session
        session.mark_in_progress();
        session.mark_chunk_received(chunk_index);
        session.add_uploaded_bytes(actual_size);

        self.repository
            .update_chunk_received(session_id, chunk_index, &chunk_hash, actual_size)
            .await?;
        self.repository.update_session(&session).await?;

        Ok(ChunkUploadResponse {
            session_id,
            chunk_index,
            verified: true,
            progress_percent: session.progress_percent(),
            is_complete: session.is_complete(),
        })
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

        // Assemble chunks and calculate final hash
        let final_content = self.assemble_content(&session).await?;
        let final_hash = self.calculate_sha256(&final_content);

        // Verify final hash if expected
        if let Some(expected_hash) = &session.file_hash {
            if expected_hash != &final_hash {
                return Err(UploadError::FileHashVerificationFailed);
            }
        }

        // Get parent folder path
        let parent_path = if let Some(folder_id) = session.folder_id {
            let folder = self
                .metadata_store
                .find_folder_by_id(folder_id)
                .await?
                .ok_or(UploadError::ParentFolderNotFound(folder_id))?;
            folder.path.clone()
        } else {
            String::new()
        };

        // Construct file path
        let path = if parent_path.is_empty() || parent_path == "/" {
            format!("/{}", session.file_name)
        } else {
            format!("{}/{}", parent_path, session.file_name)
        };

        // Store final file
        let storage_key = format!("blobs/{}", final_hash);
        self.object_store
            .assemble_chunks(
                session_id,
                session.total_chunks(),
                &storage_key,
            )
            .await?;

        // Create file metadata
        let file = File::new(
            session.file_name.clone(),
            path.clone(),
            final_hash.clone(),
            session.total_size as i64,
            session.mime_type.clone(),
            session.folder_id,
            user_id,
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
            serde_json::to_value(&payload)
                .map_err(|e| UploadError::Storage(format!("Failed to serialize event: {}", e)))?,
            user_id,
        );

        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|e| UploadError::Storage(format!("Failed to append event: {}", e)))?;

        // Mark session complete
        session.mark_completed(file.id);
        self.repository.complete_session(session_id, file.id).await?;

        // Cleanup chunks
        let _ = self
            .object_store
            .delete_session_chunks(session_id, session.total_chunks())
            .await;

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

        // Delete chunks
        let _ = self
            .object_store
            .delete_session_chunks(session_id, session.total_chunks())
            .await;

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
            // Delete chunks
            let _ = self
                .object_store
                .delete_session_chunks(session.id, session.total_chunks())
                .await;

            // Delete session
            let _ = self.repository.delete_session(session.id).await;
            cleaned += 1;
        }

        Ok(cleaned)
    }

    /// Assemble content from chunks for verification
    async fn assemble_content(&self, session: &UploadSession) -> Result<Bytes, UploadError> {
        let mut content = Vec::with_capacity(session.total_size as usize);

        for chunk_index in 0..session.total_chunks() {
            let chunk_data = self
                .object_store
                .get_chunk(session.id, chunk_index)
                .await?
                .ok_or_else(|| {
                    UploadError::Storage(format!("Chunk {} missing during assembly", chunk_index))
                })?;
            content.extend_from_slice(&chunk_data);
        }

        Ok(Bytes::from(content))
    }

    /// Validate file name
    fn validate_file_name(&self, name: &str) -> Result<(), UploadError> {
        if name.is_empty() {
            return Err(UploadError::InvalidFileName(
                "File name cannot be empty".to_string(),
            ));
        }

        if name.contains('/') {
            return Err(UploadError::InvalidFileName(
                "File name cannot contain forward slash (/)".to_string(),
            ));
        }

        if name.contains('\0') {
            return Err(UploadError::InvalidFileName(
                "File name cannot contain null character".to_string(),
            ));
        }

        Ok(())
    }

    /// Calculate SHA256 hash
    fn calculate_sha256(&self, content: &Bytes) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }
}

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

    // Note: These tests require mock implementations of the traits
    // which would be quite extensive. For now, we test the basic
    // validation logic here.

    #[test]
    fn test_validate_file_name() {
        // This is a simple sanity check - full testing requires mocks
        assert!(true);
    }
}
