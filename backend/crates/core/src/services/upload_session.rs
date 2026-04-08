//! Upload session types for resumable file uploads
//!
//! This module provides types for managing chunked upload sessions
//! that support resumable/background file transfers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of an upload session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadSessionStatus {
    /// Session created, waiting for chunks
    Pending,
    /// Upload in progress (chunks being received)
    InProgress,
    /// All chunks received, being assembled
    Completed,
    /// Upload aborted by user or system
    Aborted,
}

impl UploadSessionStatus {
    /// Check if the session is still active (can receive chunks)
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::InProgress)
    }

    /// Check if the session is complete
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Completed)
    }
}

/// Information about a single chunk in an upload session
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkInfo {
    /// Session ID this chunk belongs to
    pub session_id: Uuid,
    /// Index of this chunk (0-based)
    pub chunk_index: u32,
    /// SHA-256 hash of the chunk content for verification
    pub chunk_hash: String,
    /// Size of the chunk in bytes
    pub size: u64,
    /// When the chunk was received
    pub received_at: DateTime<Utc>,
}

impl ChunkInfo {
    /// Create a new chunk info
    pub fn new(session_id: Uuid, chunk_index: u32, chunk_hash: String, size: u64) -> Self {
        Self {
            session_id,
            chunk_index,
            chunk_hash,
            size,
            received_at: Utc::now(),
        }
    }
}

/// Upload session for resumable file transfers
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadSession {
    /// Schema version for migration support
    pub schema_version: u32,
    /// Unique session identifier
    pub id: Uuid,
    /// Tenant ID for multi-tenancy
    pub tenant_id: Uuid,
    /// User who initiated the upload
    pub owner_id: Uuid,
    /// Target folder ID (None for root)
    pub folder_id: Option<Uuid>,
    /// Final file name
    pub file_name: String,
    /// MIME type of the file
    pub mime_type: String,
    /// Total file size in bytes
    pub total_size: u64,
    /// Chunk size in bytes (e.g., 5MB)
    pub chunk_size: u64,
    /// Total bytes uploaded so far
    pub uploaded_bytes: u64,
    /// Bitmask of received chunk indices (bit i = chunk i received)
    pub chunks_received: Vec<u32>,
    /// Current session status
    pub status: UploadSessionStatus,
    /// SHA-256 hash of the complete file (for verification on completion)
    pub file_hash: Option<String>,
    /// When the session was created
    pub created_at: DateTime<Utc>,
    /// When the session expires (auto-cleanup if not completed)
    pub expires_at: DateTime<Utc>,
    /// When the session was completed (if applicable)
    pub completed_at: Option<DateTime<Utc>>,
    /// ID of the created file (set on completion)
    pub file_id: Option<Uuid>,
    /// Document version for optimistic concurrency
    pub version: u64,
}

impl UploadSession {
    /// Schema version for upload sessions
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    /// Default session expiration time (24 hours)
    pub const DEFAULT_EXPIRATION_HOURS: i64 = 24;

    /// Create a new upload session
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        tenant_id: Uuid,
        owner_id: Uuid,
        folder_id: Option<Uuid>,
        file_name: String,
        mime_type: String,
        total_size: u64,
        chunk_size: u64,
        file_hash: Option<String>,
    ) -> Self {
        let now = Utc::now();
        let total_chunks = Self::calculate_total_chunks(total_size, chunk_size);

        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            id,
            tenant_id,
            owner_id,
            folder_id,
            file_name,
            mime_type,
            total_size,
            chunk_size,
            uploaded_bytes: 0,
            chunks_received: vec![0; (total_chunks as usize + 31) / 32],
            status: UploadSessionStatus::Pending,
            file_hash,
            created_at: now,
            expires_at: now + chrono::Duration::hours(Self::DEFAULT_EXPIRATION_HOURS),
            completed_at: None,
            file_id: None,
            version: 1,
        }
    }

    /// Calculate the total number of chunks needed
    pub fn calculate_total_chunks(total_size: u64, chunk_size: u64) -> u32 {
        if total_size == 0 {
            1
        } else {
            ((total_size + chunk_size - 1) / chunk_size) as u32
        }
    }

    /// Get the total number of chunks for this session
    pub fn total_chunks(&self) -> u32 {
        Self::calculate_total_chunks(self.total_size, self.chunk_size)
    }

    /// Check if a specific chunk has been received
    pub fn has_chunk(&self, chunk_index: u32) -> bool {
        let bucket = chunk_index as usize / 32;
        let bit = chunk_index % 32;
        bucket < self.chunks_received.len() && (self.chunks_received[bucket] & (1 << bit)) != 0
    }

    /// Mark a chunk as received
    pub fn mark_chunk_received(&mut self, chunk_index: u32) {
        let bucket = chunk_index as usize / 32;
        let bit = chunk_index % 32;
        if bucket < self.chunks_received.len() {
            self.chunks_received[bucket] |= 1 << bit;
        }
    }

    /// Get a list of missing chunk indices
    pub fn missing_chunks(&self) -> Vec<u32> {
        let total = self.total_chunks();
        (0..total).filter(|&i| !self.has_chunk(i)).collect()
    }

    /// Get a list of received chunk indices
    pub fn received_chunks(&self) -> Vec<u32> {
        let total = self.total_chunks();
        (0..total).filter(|&i| self.has_chunk(i)).collect()
    }

    /// Check if all chunks have been received
    pub fn is_complete(&self) -> bool {
        self.total_chunks() == self.received_chunks().len() as u32
    }

    /// Update uploaded bytes count
    pub fn add_uploaded_bytes(&mut self, bytes: u64) {
        self.uploaded_bytes = (self.uploaded_bytes + bytes).min(self.total_size);
    }

    /// Mark the session as in progress
    pub fn mark_in_progress(&mut self) {
        if self.status == UploadSessionStatus::Pending {
            self.status = UploadSessionStatus::InProgress;
        }
        self.version += 1;
    }

    /// Mark the session as completed
    pub fn mark_completed(&mut self, file_id: Uuid) {
        self.status = UploadSessionStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.file_id = Some(file_id);
        self.uploaded_bytes = self.total_size;
        self.version += 1;
    }

    /// Mark the session as aborted
    pub fn mark_aborted(&mut self) {
        self.status = UploadSessionStatus::Aborted;
        self.version += 1;
    }

    /// Check if the session has expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Get the progress percentage (0-100)
    pub fn progress_percent(&self) -> u8 {
        if self.total_size == 0 {
            return 100;
        }
        ((self.uploaded_bytes * 100) / self.total_size) as u8
    }

    /// Get the storage path for a chunk
    pub fn chunk_storage_path(&self, chunk_index: u32) -> String {
        format!("temp/uploads/{}/{}", self.id, chunk_index)
    }

    /// Bump version on mutation
    pub fn bump_version(&mut self) {
        self.version += 1;
    }
}

/// Request to create a new upload session
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSessionRequest {
    /// Target folder ID (None for root)
    pub folder_id: Option<Uuid>,
    /// File name
    pub file_name: String,
    /// MIME type
    pub mime_type: String,
    /// Total file size in bytes
    pub total_size: u64,
    /// Chunk size in bytes (optional, defaults to 5MB)
    #[serde(default = "default_chunk_size")]
    pub chunk_size: u64,
    /// Expected SHA-256 hash of the complete file (optional)
    pub file_hash: Option<String>,
}

fn default_chunk_size() -> u64 {
    5 * 1024 * 1024 // 5MB default
}

/// Response for session creation
#[derive(Debug, Clone, Serialize)]
pub struct CreateSessionResponse {
    /// Session ID
    pub session_id: Uuid,
    /// Total number of chunks expected
    pub total_chunks: u32,
    /// Chunk size in bytes
    pub chunk_size: u64,
    /// Session expiration time
    pub expires_at: DateTime<Utc>,
}

/// Response for session status query
#[derive(Debug, Clone, Serialize)]
pub struct SessionStatusResponse {
    /// Session ID
    pub session_id: Uuid,
    /// Current status
    pub status: UploadSessionStatus,
    /// Total file size
    pub total_size: u64,
    /// Bytes uploaded so far
    pub uploaded_bytes: u64,
    /// Progress percentage (0-100)
    pub progress_percent: u8,
    /// Total number of chunks
    pub total_chunks: u32,
    /// List of received chunk indices
    pub received_chunks: Vec<u32>,
    /// List of missing chunk indices
    pub missing_chunks: Vec<u32>,
    /// Whether the session is expired
    pub is_expired: bool,
    /// Session expiration time
    pub expires_at: DateTime<Utc>,
    /// File ID (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<Uuid>,
}

impl SessionStatusResponse {
    /// Create a status response from an upload session
    pub fn from_session(session: &UploadSession) -> Self {
        Self {
            session_id: session.id,
            status: session.status,
            total_size: session.total_size,
            uploaded_bytes: session.uploaded_bytes,
            progress_percent: session.progress_percent(),
            total_chunks: session.total_chunks(),
            received_chunks: session.received_chunks(),
            missing_chunks: session.missing_chunks(),
            is_expired: session.is_expired(),
            expires_at: session.expires_at,
            file_id: session.file_id,
        }
    }
}

/// Response for chunk upload
#[derive(Debug, Clone, Serialize)]
pub struct ChunkUploadResponse {
    /// Session ID
    pub session_id: Uuid,
    /// Chunk index that was uploaded
    pub chunk_index: u32,
    /// Whether the chunk was verified successfully
    pub verified: bool,
    /// Current progress percentage
    pub progress_percent: u8,
    /// Whether all chunks are now received
    pub is_complete: bool,
}

/// Response for session completion
#[derive(Debug, Clone, Serialize)]
pub struct CompleteUploadResponse {
    /// Session ID
    pub session_id: Uuid,
    /// Created file ID
    pub file_id: Uuid,
    /// File name
    pub file_name: String,
    /// File size
    pub file_size: u64,
    /// Content hash (SHA-256)
    pub content_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upload_session_chunk_tracking() {
        let session = UploadSession::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
            "test.pdf".to_string(),
            "application/pdf".to_string(),
            10 * 1024 * 1024, // 10MB
            5 * 1024 * 1024,  // 5MB chunks
            None,
        );

        // Should have 2 chunks for 10MB with 5MB chunk size
        assert_eq!(session.total_chunks(), 2);

        // Initially no chunks received
        assert!(!session.has_chunk(0));
        assert!(!session.has_chunk(1));
        assert_eq!(session.received_chunks(), Vec::<u32>::new());
        assert_eq!(session.missing_chunks(), vec![0, 1]);

        // Mark first chunk received
        let mut session = session;
        session.mark_chunk_received(0);
        assert!(session.has_chunk(0));
        assert!(!session.has_chunk(1));
        assert_eq!(session.received_chunks(), vec![0]);
        assert_eq!(session.missing_chunks(), vec![1]);

        // Mark second chunk received
        session.mark_chunk_received(1);
        assert!(session.has_chunk(0));
        assert!(session.has_chunk(1));
        assert!(session.is_complete());
    }

    #[test]
    fn test_upload_session_progress() {
        let mut session = UploadSession::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
            "test.pdf".to_string(),
            "application/pdf".to_string(),
            100,
            10,
            None,
        );

        assert_eq!(session.progress_percent(), 0);

        session.add_uploaded_bytes(50);
        assert_eq!(session.progress_percent(), 50);

        session.add_uploaded_bytes(60); // Would exceed total
        assert_eq!(session.progress_percent(), 100);
        assert_eq!(session.uploaded_bytes, 100);
    }

    #[test]
    fn test_session_status_is_active() {
        assert!(UploadSessionStatus::Pending.is_active());
        assert!(UploadSessionStatus::InProgress.is_active());
        assert!(!UploadSessionStatus::Completed.is_active());
        assert!(!UploadSessionStatus::Aborted.is_active());
    }

    #[test]
    fn test_chunk_storage_path() {
        let session = UploadSession::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
            "test.pdf".to_string(),
            "application/pdf".to_string(),
            100,
            10,
            None,
        );

        let path = session.chunk_storage_path(5);
        assert!(path.contains(&session.id.to_string()));
        assert!(path.contains("temp/uploads/"));
        assert!(path.ends_with("/5"));
    }
}
