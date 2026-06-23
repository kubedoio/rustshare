//! Repository layer for storage-backed operations.
//!
//! Currently only the upload-session repository is wired into the live server;
//! the remaining modules are kept minimal.

pub mod path_builder;
pub mod share_notification;
pub mod sync;
pub mod upload_session;

pub use path_builder::PathBuilder;
pub use share_notification::{ShareNotificationRepo, ShareNotificationRepoImpl};
pub use upload_session::RustFsUploadSessionRepository;

use thiserror::Error;

/// Repository error types.
#[derive(Error, Debug)]
pub enum RepositoryError {
    #[error("Entity not found: {0}")]
    NotFound(String),

    #[error("Entity already exists: {0}")]
    AlreadyExists(String),

    #[error("Concurrency conflict: {0}")]
    ConcurrencyConflict(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Coordination error: {0}")]
    CoordinationError(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl RepositoryError {
    /// Check if this error indicates a not-found condition.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }

    /// Check if this error indicates a concurrency conflict.
    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::ConcurrencyConflict(_))
    }
}
