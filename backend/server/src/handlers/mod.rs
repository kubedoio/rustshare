//! HTTP request handlers for RustShare API endpoints.

mod extractors;
mod files;
mod folders;
mod notifications;
mod public_shares;
mod shares;
mod sync;
mod user_shares;

pub use extractors::{AuthenticatedUser, ShareSessionAuth};
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
pub use notifications::{
    list_notifications, mark_notification_read, delete_notification,
};
pub use public_shares::{create_session, get_share_info, download_shared_file};
pub use shares::{create_share, list_file_shares};
pub use sync::sync_handler;
pub use user_shares::{
    create_file_share, create_folder_share, list_received_shares,
    list_file_recipients, list_folder_recipients,
    update_recipient_permission, remove_recipient,
};

use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use rustshare_core::services::{FileError, FolderError, ShareError};
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

/// Map ShareError to HTTP response.
pub fn share_error_response(err: ShareError) -> Response {
    let (status, message) = match err {
        ShareError::NotFound => (StatusCode::NOT_FOUND, err.to_string()),
        ShareError::NotFoundById(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ShareError::PermissionDenied { .. } => (StatusCode::FORBIDDEN, err.to_string()),
        ShareError::FileNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ShareError::Revoked => (StatusCode::GONE, err.to_string()),
        ShareError::Expired => (StatusCode::GONE, err.to_string()),
        ShareError::PasswordRequired => (StatusCode::UNAUTHORIZED, err.to_string()),
        ShareError::InvalidPassword => (StatusCode::UNAUTHORIZED, err.to_string()),
        ShareError::RecipientNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ShareError::InsufficientPermission { .. } => (StatusCode::FORBIDDEN, err.to_string()),
        ShareError::CannotShareWithSelf => (StatusCode::BAD_REQUEST, err.to_string()),
        ShareError::ShareAlreadyExists(_) => (StatusCode::CONFLICT, err.to_string()),
        ShareError::CannotRemoveOwner => (StatusCode::FORBIDDEN, err.to_string()),
        ShareError::Database(_) | ShareError::PasswordHash(_) | ShareError::Jwt(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        }
    };

    (status, Json(ErrorResponse::new(message))).into_response()
}
