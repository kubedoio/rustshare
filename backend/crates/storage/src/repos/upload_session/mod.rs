//! Upload session repository for resumable file uploads
//!
//! This module provides repository implementations for managing
//! upload sessions and their associated chunks.

use rustshare_core::services::UploadError;
use uuid::Uuid;

pub mod rustfs;

pub use rustfs::RustFsUploadSessionRepository;

/// Conversion between repository errors and upload errors
impl From<super::RepositoryError> for UploadError {
    fn from(err: super::RepositoryError) -> Self {
        match err {
            super::RepositoryError::NotFound(msg) => {
                UploadError::SessionNotFound(extract_uuid_from_msg(&msg).unwrap_or(Uuid::nil()))
            }
            super::RepositoryError::AlreadyExists(_) => {
                UploadError::Storage("Session already exists".to_string())
            }
            super::RepositoryError::ConcurrencyConflict(_) => {
                UploadError::Storage("Concurrency conflict".to_string())
            }
            super::RepositoryError::PermissionDenied(_) => UploadError::PermissionDenied {
                user_id: Uuid::nil(),
                session_id: Uuid::nil(),
            },
            super::RepositoryError::ValidationError(msg) => UploadError::InvalidFileName(msg),
            super::RepositoryError::StorageError(msg)
            | super::RepositoryError::CoordinationError(msg) => UploadError::Storage(msg),
            super::RepositoryError::DualWriteMismatch(msg) => {
                UploadError::Storage(format!("Dual-write error: {}", msg))
            }
            super::RepositoryError::Other(e) => UploadError::Storage(e.to_string()),
        }
    }
}

/// Helper to extract UUID from error message (best effort)
fn extract_uuid_from_msg(msg: &str) -> Option<Uuid> {
    // Try to find a UUID pattern in the message
    for word in msg.split_whitespace() {
        if let Ok(uuid) = Uuid::parse_str(word) {
            return Some(uuid);
        }
    }
    None
}
