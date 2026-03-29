//! HTTP request handlers for RustShare API endpoints.

pub mod admin;
pub mod ai;
pub mod auth;
// TODO: Fix chat_integration compilation errors
// mod chat_integration;
pub mod device_auth;
pub mod devices;
mod extractors;
mod files;
mod folders;
mod notifications;
mod profile;
mod public_shares;
// mod upload;
pub mod scim;
pub mod scim_v2;
// TODO: Fix search_service compilation errors
// mod search;
mod shares;
mod sync;
mod user_shares;
mod users;

pub use extractors::{AdminUser, AuthenticatedSession, AuthenticatedUser, ShareSessionAuth};
pub use files::{
    delete_file, download_file, download_file_content, get_file, get_file_thumbnail, get_file_versions, list_deleted_items,
    list_files, list_starred_items, move_file, permanently_delete_file, preview_file, rename_file,
    restore_file_from_trash, restore_file_version, toggle_file_star, update_file, upload_file,
};
pub use folders::{
    create_folder, delete_folder, get_folder, get_folder_contents, get_folder_tree,
    get_root_contents, move_folder, permanently_delete_folder, rename_folder,
    restore_folder_from_trash, toggle_folder_star,
};
pub use notifications::{
    count_unread_notifications, delete_notification, list_notifications, mark_notification_read,
};
pub use profile::{get_profile, update_profile};
pub use public_shares::{
    create_session, download_shared_file, download_shared_folder_file, get_share_info,
    get_shared_folder_contents, upload_shared_folder_file,
};
pub use shares::{
    create_public_file_share, create_public_folder_share, get_share_access_log,
    list_public_file_shares, list_public_folder_shares, list_user_shares, revoke_share,
    OwnedShareResponse, ShareAccessLogQuery, ShareAccessLogResponse,
};
pub use sync::{sync_handler, get_sync_cursor, get_sync_delta};
// pub use upload::{
//     abort_upload_session, complete_upload, create_upload_session, get_upload_session_status,
//     list_upload_sessions, upload_chunk, upload_error_response,
//     CompleteUploadResponseBody, CreateUploadSessionResponse, UploadChunkResponse, UploadSessionStatusResponse,
// };
pub use user_shares::{
    create_file_share, create_folder_share, list_file_recipients, list_folder_recipients,
    list_received_shares, remove_recipient, update_recipient_permission,
};
pub use ai::{
    ask_question, semantic_search, summarize_file, AskQuestionRequest, AskQuestionResponse,
    SemanticSearchRequest, SemanticSearchResponse, SummarizeRequest, SummarizeResponse,
};
pub use auth::{
    ensure_optional_seed_user, log_user_security_event, login, logout, LoginRequest, LoginResponse,
    UserResponse,
};
// TODO: Fix chat_integration compilation errors
// pub use chat_integration::{
//     dispatch_webhooks, list_chat_webhooks, receive_chat_event, register_chat_webhook,
//     unfurl_link, unfurl_link_public, DispatchWebhookResponse, UnfurlLinkRequest, UnfurlLinkResponse,
//     WebhookDispatchResult, WebhookListResponse,
// };
// pub use search::search;
pub use users::{
    delete_avatar, delete_user_session, get_avatar, get_user_profile,
    list_user_security_events, list_user_sessions, update_user_password, update_user_theme,
    upload_avatar,
};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
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
}

/// Map FileError to HTTP response.
pub fn file_error_response(err: FileError) -> Response {
    let (status, message) = match err {
        FileError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        FileError::PermissionDenied { .. } => (StatusCode::FORBIDDEN, err.to_string()),
        FileError::VersionConflict { .. } => (StatusCode::CONFLICT, err.to_string()),
        FileError::ParentFolderNotFound(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        FileError::FolderNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        FileError::QuotaExceeded { .. } => (StatusCode::FORBIDDEN, err.to_string()),
        FileError::InvalidName(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        FileError::VersionNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        FileError::Database(_) | FileError::Storage(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        ),
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
        FolderError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        ),
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
        ShareError::Database(_) | ShareError::PasswordHash(_) | ShareError::Jwt(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        ),
    };

    (status, Json(ErrorResponse::new(message))).into_response()
}
