//! Repository layer for metadata operations
//!
//! This module provides repository implementations for the new metadata_v2 system.
//! It includes:
//! - Repository traits defining the contract
//! - RustFS-backed implementations
//! - Dual-write adapters for migration
//! - Factory for backend selection

pub mod traits;
pub mod rustfs_repos;
pub mod dual_write;
pub mod factory;
pub mod user;
pub mod notification;
pub mod job;
pub mod path_builder;
pub mod search;
pub mod sync;
pub mod upload_session;

pub use traits::*;
pub use rustfs_repos::*;
pub use dual_write::*;
pub use factory::*;
pub use user::*;
pub use notification::*;
pub use job::*;
pub use path_builder::PathBuilder;
pub use search::*;
pub use upload_session::*;

use thiserror::Error;

/// Repository error types
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
    
    #[error("Dual-write mismatch: {0}")]
    DualWriteMismatch(String),
    
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl RepositoryError {
    /// Check if this error indicates a not-found condition
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }
    
    /// Check if this error indicates a concurrency conflict
    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::ConcurrencyConflict(_))
    }
}
