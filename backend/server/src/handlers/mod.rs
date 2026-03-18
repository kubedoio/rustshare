//! HTTP request handlers for RustShare API endpoints.

mod extractors;
mod files;
mod folders;
mod sync;

pub use extractors::AuthenticatedUser;
pub use files::{
    upload_file, get_file, download_file, delete_file,
    update_file, get_file_versions, restore_file_version,
    move_file, rename_file,
};
pub use folders::{
    create_folder, get_folder, delete_folder,
    get_folder_contents, get_folder_tree,
    move_folder, rename_folder,
};
pub use sync::sync_handler;

use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use rustshare_core::services::{FileError, FolderError};
use serde::Serialize;

/// Standard error response format.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl ErrorResponse {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            details: None,
        }
    }

    pub fn with_details(error: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            details: Some(details.into()),
        }
    }
}

/// Map FileError to HTTP response.
pub fn file_error_response(err: FileError) -> Response {
    let (status, message) = match err {
        FileError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        FileError::PermissionDenied { .. } => (StatusCode::FORBIDDEN, err.to_string()),
        FileError::VersionConflict { .. } => (StatusCode::CONFLICT, err.to_string()),
        FileError::ParentFolderNotFound(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        FileError::QuotaExceeded { .. } => (StatusCode::FORBIDDEN, err.to_string()),
        FileError::InvalidName(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        FileError::VersionNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        FileError::Database(_) | FileError::Storage(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        }
    };

    (status, Json(ErrorResponse::new(message))).into_response()
}

/// Map FolderError to HTTP response.
pub fn folder_error_response(err: FolderError) -> Response {
    let (status, message) = match err {
        FolderError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        FolderError::PermissionDenied { .. } => (StatusCode::FORBIDDEN, err.to_string()),
        FolderError::ParentFolderNotFound(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        FolderError::CircularReference { .. } => (StatusCode::BAD_REQUEST, err.to_string()),
        FolderError::DuplicateName { .. } => (StatusCode::CONFLICT, err.to_string()),
        FolderError::InvalidName(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        FolderError::CannotDeleteRoot(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        FolderError::Database(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        }
    };

    (status, Json(ErrorResponse::new(message))).into_response()
}
