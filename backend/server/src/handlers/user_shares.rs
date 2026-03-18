//! HTTP handlers for user share operations.
//!
//! This module implements endpoints for sharing files and folders with specific users,
//! managing share permissions, and listing shared resources.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::domain::{SharePermissions, ShareRecipient};

use crate::AppState;
use super::{AuthenticatedUser, share_error_response};

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Request to create a file share with a specific user.
#[derive(Debug, Deserialize)]
pub struct CreateFileShareRequest {
    /// Email of the recipient user.
    pub recipient_email: String,
    /// Permission level to grant.
    pub permission: SharePermissions,
}

/// Request to create a folder share with a specific user.
#[derive(Debug, Deserialize)]
pub struct CreateFolderShareRequest {
    /// Email of the recipient user.
    pub recipient_email: String,
    /// Permission level to grant.
    pub permission: SharePermissions,
}

/// Response for a created or updated share.
#[derive(Debug, Serialize)]
pub struct UserShareResponse {
    pub share_id: Uuid,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub recipient_email: String,
    pub permission: SharePermissions,
    pub created_at: String,
}

/// Response for a received share.
#[derive(Debug, Serialize)]
pub struct ReceivedShareResponse {
    pub share_id: Uuid,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub permission: SharePermissions,
    pub shared_by: Uuid,
    pub created_at: String,
}

/// Response for a share recipient.
#[derive(Debug, Serialize)]
pub struct ShareRecipientResponse {
    pub share_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub permission: SharePermissions,
    pub added_at: String,
    pub added_by: Uuid,
}

/// Request to update a recipient's permission.
#[derive(Debug, Deserialize)]
pub struct UpdatePermissionRequest {
    pub permission: SharePermissions,
}

/// Query parameters for listing received shares.
#[derive(Debug, Deserialize)]
pub struct ListReceivedSharesQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

// ============================================================================
// 1. POST /api/files/{id}/share - Create file share
// ============================================================================

/// Create a share for a file with a specific user.
///
/// POST /api/files/{id}/share
///
/// Creates a user-specific share (not a public share link).
/// If a share already exists for this recipient, updates the permission level.
pub async fn create_file_share(
    State(state): State<AppState>,
    Path(file_id): Path<Uuid>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateFileShareRequest>,
) -> Result<Response, Response> {
    let share = state
        .user_share_service
        .create_file_share(
            file_id,
            &req.recipient_email,
            req.permission,
            auth.user_id,
        )
        .await
        .map_err(share_error_response)?;

    // Get recipient email for response (it was validated in the service)
    let response = UserShareResponse {
        share_id: share.id,
        resource_id: file_id,
        resource_type: "file".to_string(),
        recipient_email: req.recipient_email,
        permission: share.permissions,
        created_at: share.created_at.to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(response)).into_response())
}

// ============================================================================
// 2. POST /api/folders/{id}/share - Create folder share
// ============================================================================

/// Create a share for a folder with a specific user.
///
/// POST /api/folders/{id}/share
///
/// Creates a user-specific share (not a public share link).
/// If a share already exists for this recipient, updates the permission level.
pub async fn create_folder_share(
    State(state): State<AppState>,
    Path(folder_id): Path<Uuid>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateFolderShareRequest>,
) -> Result<Response, Response> {
    let share = state
        .user_share_service
        .create_folder_share(
            folder_id,
            &req.recipient_email,
            req.permission,
            auth.user_id,
        )
        .await
        .map_err(share_error_response)?;

    let response = UserShareResponse {
        share_id: share.id,
        resource_id: folder_id,
        resource_type: "folder".to_string(),
        recipient_email: req.recipient_email,
        permission: share.permissions,
        created_at: share.created_at.to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(response)).into_response())
}

// ============================================================================
// 3. GET /api/shares/received - List received shares
// ============================================================================

/// List shares received by the authenticated user.
///
/// GET /api/shares/received?limit=50&offset=0
///
/// Returns paginated list of files and folders shared with the user.
pub async fn list_received_shares(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ListReceivedSharesQuery>,
) -> Result<Response, Response> {
    let shares = state
        .user_share_service
        .list_received_shares(auth.user_id, query.limit, query.offset)
        .await
        .map_err(share_error_response)?;

    let response: Vec<ReceivedShareResponse> = shares
        .into_iter()
        .map(|s| {
            let resource_id = s.file_id.or(s.folder_id).unwrap_or_else(Uuid::nil);
            let resource_type = if s.file_id.is_some() {
                "file"
            } else {
                "folder"
            };

            ReceivedShareResponse {
                share_id: s.id,
                resource_id,
                resource_type: resource_type.to_string(),
                permission: s.permissions,
                shared_by: s.created_by,
                created_at: s.created_at.to_rfc3339(),
            }
        })
        .collect();

    Ok(Json(response).into_response())
}

// ============================================================================
// 4. GET /api/files/{id}/recipients - List file recipients (Admin only)
// ============================================================================

/// List all recipients of a shared file.
///
/// GET /api/files/{id}/recipients
///
/// Requires Admin permission on the file.
/// Returns list of users who have access to the file and their permission levels.
pub async fn list_file_recipients(
    State(mut state): State<AppState>,
    Path(file_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    let recipients = state
        .user_share_service
        .list_recipients(Some(file_id), None, auth.user_id)
        .await
        .map_err(share_error_response)?;

    let response: Vec<ShareRecipientResponse> = recipients
        .into_iter()
        .map(|r| ShareRecipientResponse {
            share_id: r.share_id,
            user_id: r.user_id,
            email: r.email,
            permission: r.permission,
            added_at: r.added_at.to_rfc3339(),
            added_by: r.added_by,
        })
        .collect();

    Ok(Json(response).into_response())
}

// ============================================================================
// 5. GET /api/folders/{id}/recipients - List folder recipients (Admin only)
// ============================================================================

/// List all recipients of a shared folder.
///
/// GET /api/folders/{id}/recipients
///
/// Requires Admin permission on the folder.
/// Returns list of users who have access to the folder and their permission levels.
pub async fn list_folder_recipients(
    State(mut state): State<AppState>,
    Path(folder_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    let recipients = state
        .user_share_service
        .list_recipients(None, Some(folder_id), auth.user_id)
        .await
        .map_err(share_error_response)?;

    let response: Vec<ShareRecipientResponse> = recipients
        .into_iter()
        .map(|r| ShareRecipientResponse {
            share_id: r.share_id,
            user_id: r.user_id,
            email: r.email,
            permission: r.permission,
            added_at: r.added_at.to_rfc3339(),
            added_by: r.added_by,
        })
        .collect();

    Ok(Json(response).into_response())
}

// ============================================================================
// 6. PUT /api/shares/{id}/permission - Update permission (Admin only)
// ============================================================================

/// Update the permission level for a share recipient.
///
/// PUT /api/shares/{id}/permission
///
/// Requires Admin permission on the shared resource.
/// Updates the permission level for the specified share.
pub async fn update_recipient_permission(
    State(mut state): State<AppState>,
    Path(share_id): Path<Uuid>,
    auth: AuthenticatedUser,
    Json(req): Json<UpdatePermissionRequest>,
) -> Result<Response, Response> {
    let updated_share = state
        .user_share_service
        .update_recipient_permission(share_id, req.permission, auth.user_id)
        .await
        .map_err(share_error_response)?;

    let resource_id = updated_share.file_id.or(updated_share.folder_id).unwrap_or_else(Uuid::nil);
    let resource_type = if updated_share.file_id.is_some() {
        "file"
    } else {
        "folder"
    };

    let response = serde_json::json!({
        "share_id": updated_share.id,
        "resource_id": resource_id,
        "resource_type": resource_type,
        "permission": updated_share.permissions,
        "updated_at": updated_share.created_at.to_rfc3339(),
    });

    Ok(Json(response).into_response())
}

// ============================================================================
// 7. DELETE /api/shares/{id}/recipient - Remove recipient (Admin only)
// ============================================================================

/// Remove a recipient from a share (revoke access).
///
/// DELETE /api/shares/{id}/recipient
///
/// Requires Admin permission on the shared resource.
/// Revokes the share and removes the recipient's access.
pub async fn remove_recipient(
    State(mut state): State<AppState>,
    Path(share_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    state
        .user_share_service
        .remove_recipient(share_id, auth.user_id)
        .await
        .map_err(share_error_response)?;

    Ok((
        StatusCode::NO_CONTENT,
        (),
    ).into_response())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require database setup and axum_test.
    // These tests verify that the handler functions are correctly typed and compile.
    // Integration tests will be added when test infrastructure is set up.

    #[test]
    fn test_create_file_share_request_deserialization() {
        let json = serde_json::json!({
            "recipient_email": "user@example.com",
            "permission": "ReadWrite"
        });

        let req: Result<CreateFileShareRequest, _> = serde_json::from_value(json);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.recipient_email, "user@example.com");
        assert_eq!(req.permission, SharePermissions::ReadWrite);
    }

    #[test]
    fn test_create_folder_share_request_deserialization() {
        let json = serde_json::json!({
            "recipient_email": "admin@example.com",
            "permission": "Admin"
        });

        let req: Result<CreateFolderShareRequest, _> = serde_json::from_value(json);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.recipient_email, "admin@example.com");
        assert_eq!(req.permission, SharePermissions::Admin);
    }

    #[test]
    fn test_update_permission_request_deserialization() {
        let json = serde_json::json!({
            "permission": "View"
        });

        let req: Result<UpdatePermissionRequest, _> = serde_json::from_value(json);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.permission, SharePermissions::View);
    }

    #[test]
    fn test_list_received_shares_query_defaults() {
        let json = serde_json::json!({});
        let query: Result<ListReceivedSharesQuery, _> = serde_json::from_value(json);
        assert!(query.is_ok());
        let query = query.unwrap();
        assert_eq!(query.limit, 50);
        assert_eq!(query.offset, 0);
    }

    #[test]
    fn test_list_received_shares_query_custom() {
        let json = serde_json::json!({
            "limit": 10,
            "offset": 20
        });
        let query: Result<ListReceivedSharesQuery, _> = serde_json::from_value(json);
        assert!(query.is_ok());
        let query = query.unwrap();
        assert_eq!(query.limit, 10);
        assert_eq!(query.offset, 20);
    }
}
