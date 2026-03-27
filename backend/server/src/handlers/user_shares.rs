//! HTTP handlers for user share operations.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::domain::SharePermissions;
use rustshare_storage::metadata_v2::schemas::{ShareDocument, SharePermission};

use crate::{handlers::AuthenticatedUser, AppState};

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
    pub share_id: String,
    pub resource_id: String,
    pub resource_type: String,
    pub recipient_email: String,
    pub permission: String,
    pub created_at: String,
}

/// Response for a received share.
#[derive(Debug, Serialize)]
pub struct ReceivedShareResponse {
    pub share_id: String,
    pub resource_id: String,
    pub resource_type: String,
    pub resource_name: String,
    pub resource_path: String,
    pub permission: String,
    pub shared_by: String,
    pub shared_by_name: String,
    pub shared_by_email: String,
    pub created_at: String,
}

/// Response for a share recipient.
#[derive(Debug, Serialize)]
pub struct ShareRecipientResponse {
    pub share_id: String,
    pub user_id: String,
    pub email: String,
    pub permission: String,
    pub added_at: String,
    pub added_by: String,
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
    #[allow(dead_code)]
    pub limit: i64,
    #[serde(default)]
    #[allow(dead_code)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

// ============================================================================
// Handlers
// ============================================================================

/// Create a share for a file with a specific user.
///
/// POST /api/files/{id}/share
pub async fn create_file_share(
    State(state): State<AppState>,
    Path(file_id): Path<Uuid>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateFileShareRequest>,
) -> Result<Response, Response> {
    // Find recipient by email
    let recipient = state.user_repo
        .get_user_by_email(&req.recipient_email)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find recipient: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to create share" })),
            )
                .into_response()
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Recipient user not found" })),
            )
                .into_response()
        })?;

    // Create the share using share_repo
    let share_id = Uuid::new_v4();
    let permission = match req.permission {
        SharePermissions::View => SharePermission::View,
        SharePermissions::Edit => SharePermission::Edit,
        _ => SharePermission::View,
    };

    let share = ShareDocument::new_user_share(
        share_id,
        "file".to_string(),
        file_id,
        permission,
        recipient.id,
        auth.user_id,
    );

    // Save the share
    state.share_repo
        .create(&share)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to create share" })),
            )
                .into_response()
        })?;
    
    // TODO: Create notification for recipient using the correct Notification domain type
    
    Ok((
        StatusCode::CREATED,
        Json(UserShareResponse {
            share_id: share_id.to_string(),
            resource_id: file_id.to_string(),
            resource_type: "file".to_string(),
            recipient_email: recipient.email,
            permission: format!("{:?}", permission).to_lowercase(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }),
    )
        .into_response())
}

/// Create a share for a folder with a specific user.
///
/// POST /api/folders/{id}/share
pub async fn create_folder_share(
    State(state): State<AppState>,
    Path(folder_id): Path<Uuid>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateFolderShareRequest>,
) -> Result<Response, Response> {
    // Find recipient by email
    let recipient = state.user_repo
        .get_user_by_email(&req.recipient_email)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find recipient: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to create share" })),
            )
                .into_response()
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Recipient user not found" })),
            )
                .into_response()
        })?;

    // Create the share using share_repo
    let share_id = Uuid::new_v4();
    let permission = match req.permission {
        SharePermissions::View => SharePermission::View,
        SharePermissions::Edit => SharePermission::Edit,
        _ => SharePermission::View,
    };

    let share = ShareDocument::new_user_share(
        share_id,
        "folder".to_string(),
        folder_id,
        permission,
        recipient.id,
        auth.user_id,
    );

    // Save the share
    state.share_repo
        .create(&share)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to create share" })),
            )
                .into_response()
        })?;
    
    // TODO: Create notification for recipient using the correct Notification domain type
    
    Ok((
        StatusCode::CREATED,
        Json(UserShareResponse {
            share_id: share_id.to_string(),
            resource_id: folder_id.to_string(),
            resource_type: "folder".to_string(),
            recipient_email: recipient.email,
            permission: format!("{:?}", permission).to_lowercase(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }),
    )
        .into_response())
}

/// List shares received by the authenticated user.
///
/// GET /api/shares/received
pub async fn list_received_shares(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(_query): Query<ListReceivedSharesQuery>,
) -> Result<Response, Response> {
    // Get shares where user is recipient using share_repo
    let shares = state.share_repo
        .list_by_recipient(auth.user_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list received shares: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to list shares" })),
            )
                .into_response()
        })?;

    let mut responses = Vec::new();
    
    for share in shares {
        // Only include user shares (not public shares)
        if share.recipient_user_id != Some(auth.user_id) {
            continue;
        }

        // Get owner details
        let (owner_name, owner_email) = match state.user_repo.get_user_by_id(share.created_by).await {
            Ok(Some(user)) => (user.display_name, user.email),
            _ => ("Unknown".to_string(), "unknown@localhost".to_string()),
        };

        responses.push(ReceivedShareResponse {
            share_id: share.id.to_string(),
            resource_id: share.resource_id.to_string(),
            resource_type: share.resource_type.clone(),
            resource_name: share.resource_id.to_string(),
            resource_path: "/".to_string(),
            permission: format!("{:?}", share.permissions).to_lowercase(),
            shared_by: share.created_by.to_string(),
            shared_by_name: owner_name.clone(),
            shared_by_email: owner_email,
            created_at: share.created_at.to_rfc3339(),
        });
    }

    Ok(Json(serde_json::json!({
        "shares": responses,
        "total": responses.len(),
    }))
    .into_response())
}

/// List all recipients for a file.
///
/// GET /api/files/{id}/shares
pub async fn list_file_recipients(
    State(state): State<AppState>,
    Path(file_id): Path<Uuid>,
    _auth: AuthenticatedUser,
) -> Result<Response, Response> {
    // Get all shares for this file using share_repo
    let shares = state.share_repo
        .list_by_resource("file", file_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list file shares: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to list shares" })),
            )
                .into_response()
        })?;

    let mut responses = Vec::new();
    
    for share in shares {
        // Only include user shares with a recipient
        let recipient_id = match share.recipient_user_id {
            Some(id) => id,
            None => continue,
        };

        // Get recipient details
        let (email, _name) = match state.user_repo.get_user_by_id(recipient_id).await {
            Ok(Some(user)) => (user.email, user.display_name),
            _ => continue,
        };

        responses.push(ShareRecipientResponse {
            share_id: share.id.to_string(),
            user_id: recipient_id.to_string(),
            email,
            permission: format!("{:?}", share.permissions).to_lowercase(),
            added_at: share.created_at.to_rfc3339(),
            added_by: share.created_by.to_string(),
        });
    }

    Ok(Json(serde_json::json!({
        "recipients": responses,
        "total": responses.len(),
    }))
    .into_response())
}

/// Update a recipient's permission for a file.
///
/// PUT /api/files/{id}/recipients/{recipient_id}
pub async fn update_file_recipient_permission(
    State(state): State<AppState>,
    Path((file_id, recipient_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedUser,
    Json(req): Json<UpdatePermissionRequest>,
) -> Result<Response, Response> {
    // Find the share for this file and recipient
    let shares = state.share_repo
        .list_by_resource("file", file_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update share" })),
            )
                .into_response()
        })?;

    let mut share = shares
        .into_iter()
        .find(|s| s.recipient_user_id == Some(recipient_id))
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Share not found" })),
            )
                .into_response()
        })?;

    // Verify the current user is the owner of the share
    if share.created_by != auth.user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "You can only modify shares you created" })),
        )
            .into_response());
    }

    // Update permission
    share.permissions = match req.permission {
        SharePermissions::View => SharePermission::View,
        SharePermissions::Edit => SharePermission::Edit,
        _ => SharePermission::View,
    };

    // Save the updated share
    state.share_repo
        .update(&share)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update share" })),
            )
                .into_response()
        })?;

    // Get recipient details
    let recipient = state.user_repo
        .get_user_by_id(recipient_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find recipient: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update share" })),
            )
                .into_response()
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Recipient not found" })),
            )
                .into_response()
        })?;

    Ok(Json(UserShareResponse {
        share_id: share.id.to_string(),
        resource_id: file_id.to_string(),
        resource_type: "file".to_string(),
        recipient_email: recipient.email,
        permission: format!("{:?}", share.permissions).to_lowercase(),
        created_at: share.created_at.to_rfc3339(),
    })
    .into_response())
}

/// Remove a recipient from a file share.
///
/// DELETE /api/files/{id}/recipients/{recipient_id}
pub async fn remove_file_recipient(
    State(state): State<AppState>,
    Path((file_id, recipient_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    // Find the share for this file and recipient
    let shares = state.share_repo
        .list_by_resource("file", file_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to remove share" })),
            )
                .into_response()
        })?;

    let share = shares
        .into_iter()
        .find(|s| s.recipient_user_id == Some(recipient_id))
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Share not found" })),
            )
                .into_response()
        })?;

    // Verify the current user is the owner of the share
    if share.created_by != auth.user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "You can only remove shares you created" })),
        )
            .into_response());
    }

    // Delete the share
    state.share_repo
        .delete(share.id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to remove share" })),
            )
                .into_response()
        })?;

    Ok((StatusCode::NO_CONTENT, ()).into_response())
}

/// List all recipients for a folder.
///
/// GET /api/folders/{id}/shares
pub async fn list_folder_recipients(
    State(state): State<AppState>,
    Path(folder_id): Path<Uuid>,
    _auth: AuthenticatedUser,
) -> Result<Response, Response> {
    // Get all shares for this folder using share_repo
    let shares = state.share_repo
        .list_by_resource("folder", folder_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list folder shares: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to list shares" })),
            )
                .into_response()
        })?;

    let mut responses = Vec::new();
    
    for share in shares {
        // Only include user shares with a recipient
        let recipient_id = match share.recipient_user_id {
            Some(id) => id,
            None => continue,
        };

        // Get recipient details
        let (email, _name) = match state.user_repo.get_user_by_id(recipient_id).await {
            Ok(Some(user)) => (user.email, user.display_name),
            _ => continue,
        };

        responses.push(ShareRecipientResponse {
            share_id: share.id.to_string(),
            user_id: recipient_id.to_string(),
            email,
            permission: format!("{:?}", share.permissions).to_lowercase(),
            added_at: share.created_at.to_rfc3339(),
            added_by: share.created_by.to_string(),
        });
    }

    Ok(Json(serde_json::json!({
        "recipients": responses,
        "total": responses.len(),
    }))
    .into_response())
}

/// Update a recipient's permission for a folder.
///
/// PUT /api/folders/{id}/recipients/{recipient_id}
pub async fn update_folder_recipient_permission(
    State(state): State<AppState>,
    Path((folder_id, recipient_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedUser,
    Json(req): Json<UpdatePermissionRequest>,
) -> Result<Response, Response> {
    // Find the share for this folder and recipient
    let shares = state.share_repo
        .list_by_resource("folder", folder_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update share" })),
            )
                .into_response()
        })?;

    let mut share = shares
        .into_iter()
        .find(|s| s.recipient_user_id == Some(recipient_id))
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Share not found" })),
            )
                .into_response()
        })?;

    // Verify the current user is the owner of the share
    if share.created_by != auth.user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "You can only modify shares you created" })),
        )
            .into_response());
    }

    // Update permission
    share.permissions = match req.permission {
        SharePermissions::View => SharePermission::View,
        SharePermissions::Edit => SharePermission::Edit,
        _ => SharePermission::View,
    };

    // Save the updated share
    state.share_repo
        .update(&share)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update share" })),
            )
                .into_response()
        })?;

    // Get recipient details
    let recipient = state.user_repo
        .get_user_by_id(recipient_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find recipient: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update share" })),
            )
                .into_response()
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Recipient not found" })),
            )
                .into_response()
        })?;

    Ok(Json(UserShareResponse {
        share_id: share.id.to_string(),
        resource_id: folder_id.to_string(),
        resource_type: "folder".to_string(),
        recipient_email: recipient.email,
        permission: format!("{:?}", share.permissions).to_lowercase(),
        created_at: share.created_at.to_rfc3339(),
    })
    .into_response())
}

/// Remove a recipient from a folder share.
///
/// DELETE /api/folders/{id}/recipients/{recipient_id}
pub async fn remove_folder_recipient(
    State(state): State<AppState>,
    Path((folder_id, recipient_id)): Path<(Uuid, Uuid)>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    // Find the share for this folder and recipient
    let shares = state.share_repo
        .list_by_resource("folder", folder_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to remove share" })),
            )
                .into_response()
        })?;

    let share = shares
        .into_iter()
        .find(|s| s.recipient_user_id == Some(recipient_id))
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Share not found" })),
            )
                .into_response()
        })?;

    // Verify the current user is the owner of the share
    if share.created_by != auth.user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "You can only remove shares you created" })),
        )
            .into_response());
    }

    // Delete the share
    state.share_repo
        .delete(share.id.into())
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete share: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to remove share" })),
            )
                .into_response()
        })?;

    Ok((StatusCode::NO_CONTENT, ()).into_response())
}
