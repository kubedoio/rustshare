//! Centralized error-to-HTTP response mapping for share operations.

use axum::{http::StatusCode, Json};
use rustshare_core::services::ShareError;

/// Convert a ShareError to an HTTP response.
pub fn share_error_to_response(err: ShareError) -> (StatusCode, Json<serde_json::Value>) {
    match err {
        ShareError::ShareNotFound(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Share not found" })),
        ),
        ShareError::FileNotFound(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "File not found" })),
        ),
        ShareError::FolderNotFound(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Folder not found" })),
        ),
        ShareError::GroupNotFound(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Group not found" })),
        ),
        ShareError::CrossTenantSharingNotAllowed => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "Cross-tenant sharing not allowed" })),
        ),
        ShareError::NotGroupMember(_) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "You must be a group member to share" })),
        ),
        ShareError::GroupShareAlreadyExists => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Group already has access" })),
        ),
        ShareError::InsufficientPermission { .. } => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "Admin permission required" })),
        ),
        ShareError::PermissionDenied { .. } => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "Permission denied" })),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to process share operation" })),
        ),
    }
}
