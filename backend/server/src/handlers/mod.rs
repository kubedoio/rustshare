//! HTTP request handlers for RustShare API endpoints.

pub mod admin;
pub mod ai;
pub mod auth;
pub mod brainstorming;
pub mod chat_integration;
pub mod collab;
pub mod decisions;
pub mod device_auth;
pub mod devices;
pub mod extractors;
pub mod features;
pub mod files;
pub mod folders;
pub mod groups;
pub mod health;
pub mod invites;
pub mod kanban;
pub mod mail;
pub mod meetings;
pub mod modules;
pub mod notes;
pub mod notifications;
pub mod profile;
pub mod public_shares;
pub mod scim;
pub mod scim_v2;
pub mod shares;
pub mod standups;
pub mod sync;
pub mod trash;
pub mod upload;
pub mod user_shares;
pub mod users;
pub mod validated_json;
pub mod vault_sync;
pub mod workspace_surface;
pub mod ws_auth;

pub use brainstorming::{
    create_brainstorm_board, delete_brainstorm_board, get_brainstorm_board,
    get_brainstorm_board_source, list_brainstorm_boards, save_brainstorm_board_source,
    update_brainstorm_board_preview,
};
pub use collab::collab_handler;
pub use extractors::{AdminUser, AuthenticatedSession, AuthenticatedUser, ShareSessionAuth};
pub use files::{
    delete_file, download_file, download_file_content, edit_file, get_file, get_file_thumbnail,
    get_file_versions, list_deleted_items, list_files, list_starred_items, move_file,
    permanently_delete_file, preview_file, rename_file, restore_file_from_trash,
    restore_file_version, set_file_color, toggle_file_star, update_file, upload_file,
};
pub use folders::{
    create_folder, delete_folder, download_folder, get_folder, get_folder_contents,
    get_folder_tree, get_root_contents, move_folder, permanently_delete_folder, rename_folder,
    restore_folder_from_trash, toggle_folder_star,
};
pub use health::readiness_check;
pub use kanban::{
    add_card_attachment, add_card_label, archive_board, archive_card, assign_card_member,
    create_board, create_card, create_checklist, create_checklist_item, create_label, delete_card,
    delete_card_attachment, delete_checklist, delete_checklist_item, delete_label,
    get_assignable_users, get_board, get_card, get_card_detail, list_boards, list_cards, move_card,
    remove_card_label, toggle_checklist_item, unassign_card_member, update_board, update_card,
    update_card_description, update_label,
};
pub use notifications::{
    count_unread_notifications, delete_notification, list_activity, list_notifications,
    mark_notification_read,
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
pub use validated_json::ValidatedJson;

pub use ai::{ask_question, semantic_search, summarize_file};
pub use auth::{ensure_optional_seed_user, login, logout};
pub use decisions::{
    create_decision, delete_decision, duplicate_decision, get_decision, list_decisions,
    move_decision, rename_decision, update_decision,
};
pub use features::get_features;
pub use groups::{
    create_file_group_share, create_folder_group_share, get_my_group, list_file_group_shares,
    list_folder_group_shares, list_my_groups, revoke_group_share, update_group_share_permission,
};
pub use invites::{accept_invite, create_invite, get_invite};
pub use meetings::{
    create_meeting, delete_meeting, duplicate_meeting, get_meeting, list_meetings, move_meeting,
    update_meeting,
};
pub use modules::{create_from_template, get_module, get_module_summary, list_enabled_modules};
pub use notes::{
    create_note, delete_note, duplicate_note, get_note, get_public_note, list_notes,
    list_recent_notes, move_note, rename_note, resolve_conflict, save_note, toggle_visibility,
};
pub use standups::{
    create_standup, delete_standup, duplicate_standup, get_standup, list_standups, move_standup,
    update_standup,
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
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

/// Standard pagination query parameters.
#[derive(Deserialize, Debug, Clone, utoipa::ToSchema)]
pub struct PaginationQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 {
    1
}

fn default_per_page() -> u32 {
    50
}

impl PaginationQuery {
    pub fn limit(&self) -> i64 {
        self.per_page.clamp(1, 100) as i64
    }

    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) as i64) * self.limit()
    }
}

/// Maximum authenticated upload size in bytes.
///
/// Defaults to 5000 MB and can be overridden with `MAX_UPLOAD_SIZE_MB`.
/// The value is parsed once on first use and converted from megabytes to bytes.
/// Values above 50 GB are clamped and logged to prevent a malformed or
/// malicious environment variable from disabling size limits.
pub fn max_upload_size_bytes() -> usize {
    static MAX: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *MAX.get_or_init(|| match std::env::var("MAX_UPLOAD_SIZE_MB") {
        Ok(value) => match value.parse::<usize>() {
            Ok(mb) => {
                const MAX_MB: usize = 50 * 1024; // 50 GB in MB
                let clamped_mb = mb.min(MAX_MB);
                if clamped_mb != mb {
                    tracing::warn!(
                        value = %value,
                        max_mb = %MAX_MB,
                        "MAX_UPLOAD_SIZE_MB exceeds the maximum allowed size; clamping to 50 GB"
                    );
                }
                clamped_mb.saturating_mul(1024 * 1024)
            }
            Err(_) => {
                tracing::warn!(
                    value = %value,
                    "MAX_UPLOAD_SIZE_MB is malformed; using default 5000 MB"
                );
                5000 * 1024 * 1024
            }
        },
        Err(_) => 5000 * 1024 * 1024,
    })
}

/// Stream a multipart field to a temporary file and return the temp file plus size.
/// Enforces a per-field size limit during streaming to prevent OOM.
pub(super) async fn stream_multipart_field_to_temp_file(
    field: &mut axum::extract::multipart::Field<'_>,
    max_size: usize,
) -> Result<(tempfile::NamedTempFile, usize), AppError> {
    let temp_file = tokio::task::spawn_blocking(tempfile::NamedTempFile::new)
        .await
        .map_err(|e| AppError::internal(format!("Failed to create temp file: {e}")))?
        .map_err(|e| AppError::internal(format!("Failed to create temp file: {e}")))?;

    let mut async_file = tokio::fs::File::from_std(
        temp_file
            .reopen()
            .map_err(|e| AppError::internal(format!("Failed to reopen temp file: {e}")))?,
    );

    let mut total_size: usize = 0;

    while let Some(chunk) = field.chunk().await.map_err(|e| {
        tracing::error!("Failed to read chunk: {e}");
        AppError::internal(format!("Failed to read chunk: {e}"))
    })? {
        total_size += chunk.len();
        if total_size > max_size {
            return Err(AppError::payload_too_large(format!(
                "File size exceeds maximum allowed {max_size} bytes"
            )));
        }
        async_file.write_all(&chunk).await.map_err(|e| {
            tracing::error!("Failed to write to temp file: {e}");
            AppError::internal(format!("Failed to write to temp file: {e}"))
        })?;
    }

    async_file
        .flush()
        .await
        .map_err(|e| AppError::internal(format!("Failed to flush temp file: {e}")))?;

    Ok((temp_file, total_size))
}

use rustshare_core::services::{
    AiError, FileError, FolderError, NotificationError, ShareError, UploadError, VaultSyncError,
};
use serde::Serialize;

/// Standard error response format.
#[derive(Debug, Serialize, utoipa::ToSchema)]
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

// ============================================================================
// Unified application error type
// ============================================================================

/// Unified error type for all HTTP handlers.
///
/// Eliminates the `Result<T, Response>` anti-pattern by providing a single
/// error enum that implements `IntoResponse`. Domain errors convert via `From`
/// impls, allowing natural `?` propagation instead of `.map_err(foo_error_response)?`.
#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Unauthorized,
    Forbidden(String),
    Conflict(String),
    VaultConflict {
        client_rev: i64,
        current_rev: i64,
        server_sha256: Option<String>,
    },
    Gone(String),
    UnsupportedMediaType(String),
    PayloadTooLarge(String),
    TooManyRequests,
    BadGateway(String),
    ServiceUnavailable(String),
    Internal(String),
}

impl AppError {
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::Forbidden(msg.into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }
    pub fn gone(msg: impl Into<String>) -> Self {
        Self::Gone(msg.into())
    }
    pub fn unsupported_media_type(msg: impl Into<String>) -> Self {
        Self::UnsupportedMediaType(msg.into())
    }
    pub fn payload_too_large(msg: impl Into<String>) -> Self {
        Self::PayloadTooLarge(msg.into())
    }
    pub fn bad_gateway(msg: impl Into<String>) -> Self {
        Self::BadGateway(msg.into())
    }
    pub fn service_unavailable(msg: impl Into<String>) -> Self {
        Self::ServiceUnavailable(msg.into())
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            AppError::VaultConflict {
                client_rev,
                current_rev,
                server_sha256,
            } => {
                let body = serde_json::json!({
                    "error": "conflict",
                    "message": "Conflict detected",
                    "client_rev": client_rev,
                    "current_rev": current_rev,
                    "server_sha256": server_sha256,
                    "resolution": "create_conflict_copy",
                });
                return (StatusCode::CONFLICT, Json(body)).into_response();
            }
            AppError::Gone(msg) => (StatusCode::GONE, msg),
            AppError::UnsupportedMediaType(msg) => (StatusCode::UNSUPPORTED_MEDIA_TYPE, msg),
            AppError::PayloadTooLarge(msg) => (StatusCode::PAYLOAD_TOO_LARGE, msg),
            AppError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "Too many requests. Please try again later.".to_string(),
            ),
            AppError::BadGateway(msg) => (StatusCode::BAD_GATEWAY, msg),
            AppError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        };
        (status, Json(ErrorResponse::new(message))).into_response()
    }
}

// -- From domain errors -----------------------------------------------------

impl From<FileError> for AppError {
    fn from(err: FileError) -> Self {
        match err {
            FileError::NotFound(_)
            | FileError::FolderNotFound(_)
            | FileError::VersionNotFound(_) => AppError::NotFound(err.to_string()),
            FileError::PermissionDenied { .. } | FileError::QuotaExceeded { .. } => {
                AppError::Forbidden(err.to_string())
            }
            FileError::VersionConflict { .. } => AppError::Conflict(err.to_string()),
            FileError::ParentFolderNotFound(_) | FileError::InvalidName(_) => {
                AppError::BadRequest(err.to_string())
            }
            FileError::NotEditable(_) => AppError::UnsupportedMediaType(err.to_string()),
            FileError::ContentTooLarge { .. } => AppError::PayloadTooLarge(err.to_string()),
            FileError::Database(_) | FileError::Storage(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<FolderError> for AppError {
    fn from(err: FolderError) -> Self {
        match err {
            FolderError::NotFound(_) => AppError::NotFound(err.to_string()),
            FolderError::PermissionDenied { .. } => AppError::Forbidden(err.to_string()),
            FolderError::ParentFolderNotFound(_)
            | FolderError::CircularReference { .. }
            | FolderError::InvalidName(_)
            | FolderError::CannotDeleteRoot(_) => AppError::BadRequest(err.to_string()),
            FolderError::DuplicateName { .. } => AppError::Conflict(err.to_string()),
            FolderError::Database(_) => AppError::Internal("Internal server error".to_string()),
        }
    }
}

impl From<ShareError> for AppError {
    fn from(err: ShareError) -> Self {
        match err {
            ShareError::ShareNotFound(_)
            | ShareError::ShareNotFoundByToken(_)
            | ShareError::FileNotFound(_)
            | ShareError::FolderNotFound(_)
            | ShareError::RecipientNotFound(_)
            | ShareError::GroupNotFound(_) => AppError::NotFound(err.to_string()),
            ShareError::PermissionDenied { .. }
            | ShareError::InsufficientPermission { .. }
            | ShareError::CannotRemoveOwner
            | ShareError::CrossTenantSharingNotAllowed
            | ShareError::NotGroupMember(_) => AppError::Forbidden(err.to_string()),
            ShareError::Revoked | ShareError::Expired => AppError::Gone(err.to_string()),
            ShareError::PasswordRequired | ShareError::InvalidPassword => AppError::Unauthorized,
            ShareError::CannotShareWithSelf
            | ShareError::InvalidState(_)
            | ShareError::InvalidRecipientVisibility(_) => AppError::BadRequest(err.to_string()),
            ShareError::ShareAlreadyExists(_) | ShareError::GroupShareAlreadyExists => {
                AppError::Conflict(err.to_string())
            }
            ShareError::Database(_) | ShareError::PasswordHash(_) | ShareError::Jwt(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<UploadError> for AppError {
    fn from(err: UploadError) -> Self {
        match err {
            UploadError::SessionNotFound(_) => AppError::NotFound(err.to_string()),
            UploadError::SessionExpired(_) => AppError::Gone(err.to_string()),
            UploadError::SessionAlreadyCompleted(_) | UploadError::SessionAborted(_) => {
                AppError::Conflict(err.to_string())
            }
            UploadError::ChunkIndexOutOfRange { .. }
            | UploadError::ChunkAlreadyReceived(_)
            | UploadError::InvalidChunkSize { .. }
            | UploadError::InvalidFileName(_) => AppError::BadRequest(err.to_string()),
            UploadError::ChunkHashVerificationFailed | UploadError::FileHashVerificationFailed => {
                AppError::BadRequest(err.to_string())
            }
            UploadError::PermissionDenied { .. } => AppError::Forbidden(err.to_string()),
            UploadError::ParentFolderNotFound(_) => AppError::BadRequest(err.to_string()),
            UploadError::Storage(_) | UploadError::Database(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<NotificationError> for AppError {
    fn from(err: NotificationError) -> Self {
        match err {
            NotificationError::NotFound | NotificationError::NotFoundById(_) => {
                AppError::NotFound(err.to_string())
            }
            NotificationError::NotOwned { .. } => AppError::Forbidden(err.to_string()),
            NotificationError::Database(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<AiError> for AppError {
    fn from(err: AiError) -> Self {
        match err {
            AiError::PermissionDenied { .. } => AppError::Forbidden(err.to_string()),
            AiError::FileNotFound(_) => AppError::NotFound(err.to_string()),
            AiError::ContentNotExtractable(_) => AppError::UnsupportedMediaType(err.to_string()),
            AiError::RateLimitExceeded => AppError::TooManyRequests,
            AiError::InvalidQuery(_) => AppError::BadRequest(err.to_string()),
            AiError::Internal(_) => AppError::Internal("Internal server error".to_string()),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!("Database error: {:?}", err);
        AppError::Internal("Internal server error".to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!("Internal error: {:?}", err);
        AppError::Internal("Internal server error".to_string())
    }
}

// -- Legacy auth error compatibility -----------------------------------------

// -- Server-local domain errors ------------------------------------------------

impl From<crate::services::note_service::NoteError> for AppError {
    fn from(err: crate::services::note_service::NoteError) -> Self {
        use crate::services::note_service::NoteError;
        match err {
            NoteError::NotFound(_) => AppError::NotFound(err.to_string()),
            NoteError::PermissionDenied => AppError::Forbidden(err.to_string()),
            NoteError::InvalidName(_) => AppError::BadRequest(err.to_string()),
            NoteError::Database(_) | NoteError::Storage(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<crate::services::standup_service::StandupError> for AppError {
    fn from(err: crate::services::standup_service::StandupError) -> Self {
        use crate::services::standup_service::StandupError;
        match err {
            StandupError::NotFound(_) => AppError::NotFound(err.to_string()),
            StandupError::PermissionDenied => AppError::Forbidden(err.to_string()),
            StandupError::InvalidData(_) => AppError::BadRequest(err.to_string()),
            StandupError::Database(_) | StandupError::Storage(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<crate::services::meeting_service::MeetingError> for AppError {
    fn from(err: crate::services::meeting_service::MeetingError) -> Self {
        use crate::services::meeting_service::MeetingError;
        match err {
            MeetingError::NotFound(_) => AppError::NotFound(err.to_string()),
            MeetingError::PermissionDenied => AppError::Forbidden(err.to_string()),
            MeetingError::InvalidData(_) => AppError::BadRequest(err.to_string()),
            MeetingError::Database(_) | MeetingError::Storage(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<crate::services::decision_service::DecisionError> for AppError {
    fn from(err: crate::services::decision_service::DecisionError) -> Self {
        use crate::services::decision_service::DecisionError;
        match err {
            DecisionError::NotFound(_) => AppError::NotFound(err.to_string()),
            DecisionError::PermissionDenied => AppError::Forbidden(err.to_string()),
            DecisionError::InvalidData(_) => AppError::BadRequest(err.to_string()),
            DecisionError::Database(_) | DecisionError::Storage(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<crate::services::module_service::ModuleError> for AppError {
    fn from(err: crate::services::module_service::ModuleError) -> Self {
        use crate::services::module_service::ModuleError;
        match err {
            ModuleError::NotFound(_) => AppError::NotFound(err.to_string()),
            ModuleError::AlreadyExists(_) => AppError::Conflict(err.to_string()),
            ModuleError::PermissionDenied => AppError::Forbidden(err.to_string()),
            ModuleError::InvalidName(_) | ModuleError::InvalidData(_) => {
                AppError::BadRequest(err.to_string())
            }
            ModuleError::Storage(_) | ModuleError::Database(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<crate::services::brainstorming_service::BrainstormError> for AppError {
    fn from(err: crate::services::brainstorming_service::BrainstormError) -> Self {
        use crate::services::brainstorming_service::BrainstormError;
        match err {
            BrainstormError::BoardNotFound => AppError::NotFound(err.to_string()),
            BrainstormError::PermissionDenied => AppError::Forbidden(err.to_string()),
            BrainstormError::InvalidName(_)
            | BrainstormError::InvalidSlug(_)
            | BrainstormError::InvalidData(_) => AppError::BadRequest(err.to_string()),
            BrainstormError::Database(_) | BrainstormError::Storage(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<crate::services::kanban_service::KanbanError> for AppError {
    fn from(err: crate::services::kanban_service::KanbanError) -> Self {
        use crate::services::kanban_service::KanbanError;
        match err {
            KanbanError::BoardNotFound
            | KanbanError::CardNotFound
            | KanbanError::ColumnNotFound(_)
            | KanbanError::NotFound(_) => AppError::NotFound(err.to_string()),
            KanbanError::PermissionDenied => AppError::Forbidden(err.to_string()),
            KanbanError::InvalidName(_) | KanbanError::InvalidData(_) => {
                AppError::BadRequest(err.to_string())
            }
            KanbanError::Database(_) | KanbanError::Storage(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<crate::services::template_service::TemplateError> for AppError {
    fn from(err: crate::services::template_service::TemplateError) -> Self {
        use crate::services::template_service::TemplateError;
        match err {
            TemplateError::NotFound(_) => AppError::NotFound(err.to_string()),
            TemplateError::AlreadyExists(_) => AppError::Conflict(err.to_string()),
            TemplateError::ModuleNotFound(_) => AppError::NotFound(err.to_string()),
            TemplateError::PermissionDenied => AppError::Forbidden(err.to_string()),
            TemplateError::InvalidData(_) => AppError::BadRequest(err.to_string()),
            TemplateError::Storage(_) | TemplateError::Database(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

impl From<VaultSyncError> for AppError {
    fn from(err: VaultSyncError) -> Self {
        match err {
            VaultSyncError::VaultNotFound(_)
            | VaultSyncError::FileNotFound(_)
            | VaultSyncError::DeviceNotFound(_) => AppError::NotFound(err.to_string()),
            // Security note: returning server_sha256 in 409 responses enables client-side
            // deduplication but acts as a confirmation oracle. This is an accepted MVP trade-off.
            VaultSyncError::Conflict {
                client_rev,
                current_rev,
                server_sha256,
            } => AppError::VaultConflict {
                client_rev,
                current_rev,
                server_sha256,
            },
            VaultSyncError::TombstoneConflict
            | VaultSyncError::VaultAlreadyExists(_)
            | VaultSyncError::FileAlreadyExists(_) => AppError::Conflict(err.to_string()),
            VaultSyncError::ManifestTooLarge { .. } => AppError::PayloadTooLarge(err.to_string()),
            VaultSyncError::InvalidPath(_) => AppError::BadRequest(err.to_string()),
            VaultSyncError::InvalidName(_) => AppError::BadRequest(err.to_string()),
            VaultSyncError::Unauthorized | VaultSyncError::DeviceRevoked => {
                AppError::Forbidden(err.to_string())
            }
            VaultSyncError::Database(ref msg) | VaultSyncError::Storage(ref msg) => {
                tracing::error!("Vault sync internal error: {}", msg);
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}

// ============================================================================
// Legacy helpers (kept for backward compatibility during transition)
// ============================================================================

// TODO: Remove once all callers are confirmed migrated to AppError.
