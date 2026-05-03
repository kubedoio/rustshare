//! HTTP request handlers for RustShare API endpoints.

pub mod admin;
pub mod ai;
pub mod auth;
pub mod device_auth;
pub mod devices;
mod extractors;
pub mod features;
mod brainstorming;
mod decisions;
mod files;
mod folders;
mod groups;
mod kanban;
pub mod invites;
mod meetings;
mod modules;
mod notes;
mod notifications;
mod profile;
mod public_shares;
pub mod scim;
pub mod scim_v2;
mod shares;
mod sync;
mod trash;
pub mod upload;
mod user_shares;
mod users;
mod workspace_surface;

pub use extractors::{AdminUser, AuthenticatedSession, AuthenticatedUser, ShareSessionAuth};
pub use files::{
    delete_file, download_file, download_file_content, edit_file, get_file, get_file_thumbnail,
    get_file_versions, list_deleted_items, list_files, list_starred_items, move_file,
    permanently_delete_file, preview_file, rename_file, restore_file_from_trash,
    restore_file_version, toggle_file_star, update_file, upload_file,
};
pub use folders::{
    create_folder, delete_folder, get_folder, get_folder_contents, get_folder_tree,
    get_root_contents, move_folder, permanently_delete_folder, rename_folder,
    restore_folder_from_trash, toggle_folder_star,
};
pub use brainstorming::{
    create_brainstorm_board, delete_brainstorm_board, get_brainstorm_board,
    get_brainstorm_board_source, list_brainstorm_boards, save_brainstorm_board_source,
    update_brainstorm_board_preview,
};
pub use kanban::{
    add_card_attachment, add_card_label, archive_board, archive_card, assign_card_member,
    create_board, create_card, create_checklist, create_checklist_item, create_label, delete_card,
    delete_checklist, delete_checklist_item, delete_card_attachment, delete_label,
    get_assignable_users, get_board, get_card, get_card_detail, list_boards, list_cards, move_card,
    remove_card_label, toggle_checklist_item, unassign_card_member, update_board, update_card,
    update_card_description, update_label,
};
pub use notifications::{
    count_unread_notifications, delete_notification, list_notifications, mark_notification_read,
};
pub use profile::{get_profile, update_profile, update_trash_retention};
pub use public_shares::{
    create_session, download_shared_file, download_shared_folder_file, get_share_info,
    get_shared_folder_contents, upload_shared_folder_file,
};
pub use shares::{
    create_public_file_share, create_public_folder_share, get_share_access_log,
    list_public_file_shares, list_public_folder_shares, list_user_shares, revoke_share,
};
pub use sync::{get_sync_cursor, get_sync_delta, sync_handler};
pub use trash::{empty_trash, get_trash_summary};

pub use ai::{ask_question, semantic_search, summarize_file};
pub use auth::{ensure_optional_seed_user, login, logout};
pub use features::get_features;
pub use groups::{
    create_file_group_share, create_folder_group_share, get_my_group, list_file_group_shares,
    list_folder_group_shares, list_my_groups, revoke_group_share, update_group_share_permission,
};
pub use invites::{accept_invite, create_invite, get_invite};
pub use modules::{create_from_template, get_module, get_module_summary, list_enabled_modules};
pub use decisions::{create_decision, get_decision, list_decisions, update_decision};
pub use meetings::{create_meeting, get_meeting, list_meetings, update_meeting};
pub use notes::{
    create_note, delete_note, get_note, get_public_note, list_notes, list_recent_notes, move_note,
    rename_note, save_note, toggle_visibility,
};
pub use user_shares::{
    create_file_share, create_folder_share, get_user_shared_folder_contents,
    get_user_shared_folder_tree, list_file_recipients, list_folder_recipients,
    list_received_shares, remove_recipient, update_recipient_permission,
};
pub use users::{
    delete_avatar, delete_user_session, get_avatar, get_dashboard_config, get_user_profile,
    list_user_module_preferences, list_user_security_events, list_user_sessions,
    update_dashboard_config, update_user_module_preference, update_user_password,
    update_user_theme, upload_avatar,
};
pub use workspace_surface::get_workspace_surface;

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
        FileError::NotEditable(_) => (StatusCode::UNSUPPORTED_MEDIA_TYPE, err.to_string()),
        FileError::ContentTooLarge { .. } => (StatusCode::PAYLOAD_TOO_LARGE, err.to_string()),
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
        ShareError::ShareNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ShareError::ShareNotFoundByToken(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ShareError::FileNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ShareError::FolderNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ShareError::PermissionDenied { .. } => (StatusCode::FORBIDDEN, err.to_string()),
        ShareError::Revoked => (StatusCode::GONE, err.to_string()),
        ShareError::Expired => (StatusCode::GONE, err.to_string()),
        ShareError::PasswordRequired => (StatusCode::UNAUTHORIZED, err.to_string()),
        ShareError::InvalidPassword => (StatusCode::UNAUTHORIZED, err.to_string()),
        ShareError::RecipientNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ShareError::InsufficientPermission { .. } => (StatusCode::FORBIDDEN, err.to_string()),
        ShareError::CannotShareWithSelf => (StatusCode::BAD_REQUEST, err.to_string()),
        ShareError::ShareAlreadyExists(_) => (StatusCode::CONFLICT, err.to_string()),
        ShareError::CannotRemoveOwner => (StatusCode::FORBIDDEN, err.to_string()),
        ShareError::InvalidState(_) => (StatusCode::CONFLICT, err.to_string()),
        ShareError::GroupNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        ShareError::NotGroupMember(_) => (StatusCode::FORBIDDEN, err.to_string()),
        ShareError::GroupShareAlreadyExists => (StatusCode::CONFLICT, err.to_string()),
        ShareError::InvalidRecipientVisibility(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        ShareError::CrossTenantSharingNotAllowed => (StatusCode::FORBIDDEN, err.to_string()),
        ShareError::Database(_) | ShareError::PasswordHash(_) | ShareError::Jwt(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        ),
    };

    (status, Json(ErrorResponse::new(message))).into_response()
}

/// Helper function to generate internal server error response.
pub fn internal_error_response() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new("Internal server error")),
    )
        .into_response()
}
