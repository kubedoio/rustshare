//! HTTP handlers for share operations.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::domain::SharePermissions;

use super::{AppError, AuthenticatedUser};
use crate::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateShareRequest {
    pub permissions: SharePermissions,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub upload_only: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ShareResponse {
    pub id: Uuid,
    pub resource_id: Uuid,
    pub resource_type: &'static str,
    pub share_token: String,
    pub permissions: SharePermissions,
    pub upload_only: bool,
    pub password_protected: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    post,
    path = "/api/v1/files/{file_id}/shares",
    tag = "Shares",
    params(("file_id" = Uuid, Path, description = "File Id")),
    request_body = CreateShareRequest,
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_public_file_share(
    State(state): State<AppState>,
    Path(file_id): Path<Uuid>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateShareRequest>,
) -> Result<Response, AppError> {
    if req.upload_only {
        return Err(AppError::bad_request(
            "Upload-only links are only supported for folders",
        ));
    }

    let share = state
        .share_service
        .create_share(
            file_id,
            auth.user_id,
            req.permissions,
            req.password,
            req.expires_at,
            auth.tenant_id,
        )
        .await?;

    // Extract share_token - it should always be Some for public shares
    let share_token = share.share_token.ok_or_else(|| {
        tracing::error!(
            "Share token is None after create_share for share {}",
            share.id
        );
        AppError::internal("Share token is missing after create_share")
    })?;

    let resource_id = share.file_id.ok_or_else(|| {
        tracing::error!("File ID is None after create_share for share {}", share.id);
        AppError::internal("File ID is missing after create_share")
    })?;

    Ok((
        StatusCode::CREATED,
        Json(ShareResponse {
            id: share.id,
            resource_id,
            resource_type: "file",
            share_token,
            permissions: share.permissions,
            upload_only: share.upload_only,
            password_protected: share.password_hash.is_some(),
            expires_at: share.expires_at,
            created_at: share.created_at,
        }),
    )
        .into_response())
}

#[utoipa::path(
    post,
    path = "/api/v1/folders/{folder_id}/shares",
    tag = "Shares",
    params(("folder_id" = Uuid, Path, description = "Folder Id")),
    request_body = CreateShareRequest,
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_public_folder_share(
    State(state): State<AppState>,
    Path(folder_id): Path<Uuid>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateShareRequest>,
) -> Result<Response, AppError> {
    let share = state
        .share_service
        .create_folder_share(
            folder_id,
            auth.user_id,
            req.permissions,
            req.password,
            req.expires_at,
            req.upload_only,
            auth.tenant_id,
        )
        .await?;

    let share_token = share.share_token.ok_or_else(|| {
        tracing::error!(
            "Share token is None after create_folder_share for share {}",
            share.id
        );
        AppError::internal("Share token is missing after create_folder_share")
    })?;

    let resource_id = share.folder_id.ok_or_else(|| {
        tracing::error!(
            "Folder ID is None after create_folder_share for share {}",
            share.id
        );
        AppError::internal("Folder ID is missing after create_folder_share")
    })?;

    Ok((
        StatusCode::CREATED,
        Json(ShareResponse {
            id: share.id,
            resource_id,
            resource_type: "folder",
            share_token,
            permissions: share.permissions,
            upload_only: share.upload_only,
            password_protected: share.password_hash.is_some(),
            expires_at: share.expires_at,
            created_at: share.created_at,
        }),
    )
        .into_response())
}

#[utoipa::path(
    get,
    path = "/api/v1/files/{file_id}/shares",
    tag = "Shares",
    params(("file_id" = Uuid, Path, description = "File Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_public_file_shares(
    State(state): State<AppState>,
    Path(file_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, AppError> {
    let shares = state
        .share_service
        .list_file_shares(file_id, auth.user_id)
        .await?;

    let response: Vec<ShareResponse> = shares
        .into_iter()
        .filter_map(|s| {
            // Only include shares with file_id and share_token (public shares)
            if let (Some(file_id), Some(share_token)) = (s.file_id, s.share_token.clone()) {
                Some(ShareResponse {
                    id: s.id,
                    resource_id: file_id,
                    resource_type: "file",
                    share_token,
                    permissions: s.permissions,
                    upload_only: s.upload_only,
                    password_protected: s.password_hash.is_some(),
                    expires_at: s.expires_at,
                    created_at: s.created_at,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(Json(response).into_response())
}

#[utoipa::path(
    get,
    path = "/api/v1/folders/{folder_id}/shares",
    tag = "Shares",
    params(("folder_id" = Uuid, Path, description = "Folder Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_public_folder_shares(
    State(state): State<AppState>,
    Path(folder_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, AppError> {
    let shares = state
        .share_service
        .list_folder_shares(folder_id, auth.user_id)
        .await?;

    let response: Vec<ShareResponse> = shares
        .into_iter()
        .filter_map(|share| {
            let (Some(resource_id), Some(share_token)) = (share.folder_id, share.share_token)
            else {
                return None;
            };

            Some(ShareResponse {
                id: share.id,
                resource_id,
                resource_type: "folder",
                share_token,
                permissions: share.permissions,
                upload_only: share.upload_only,
                password_protected: share.password_hash.is_some(),
                expires_at: share.expires_at,
                created_at: share.created_at,
            })
        })
        .collect();

    Ok(Json(response).into_response())
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct OwnedShareResponse {
    pub id: uuid::Uuid,
    pub resource_id: uuid::Uuid,
    pub resource_type: String,
    pub resource_name: String,
    pub share_token: Option<String>,
    pub permissions: SharePermissions,
    pub password_protected: bool,
    pub access_count: i32,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub recipient_user_id: Option<uuid::Uuid>,
    pub recipient_group_id: Option<uuid::Uuid>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ShareAccessLogQuery {
    pub limit: Option<i64>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ShareAccessLogResponse {
    pub accessed_at: chrono::DateTime<chrono::Utc>,
    pub action: String,
    pub success: bool,
    pub actor_type: Option<String>,
    pub actor_label: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub share_session_id: Option<uuid::Uuid>,
    pub share_session_subject: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/shares",
    tag = "Shares",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_user_shares(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Query(query): Query<crate::handlers::PaginationQuery>,
) -> Result<Json<Vec<OwnedShareResponse>>, AppError> {
    let shares = state
        .metadata_store
        .get_user_all_shares(user_id, query.limit(), query.offset())
        .await
        .map_err(|error| {
            tracing::error!("Failed to list shares: {error}");
            AppError::internal("Failed to list shares")
        })?;

    let response = shares
        .into_iter()
        .map(|entry| {
            let share = entry.share;

            OwnedShareResponse {
                id: share.id,
                resource_id: entry.resource_id,
                resource_type: entry.resource_type,
                resource_name: entry.resource_name,
                share_token: share.share_token,
                permissions: share.permissions,
                password_protected: share.password_hash.is_some(),
                access_count: share.access_count,
                expires_at: share.expires_at,
                created_at: share.created_at,
                recipient_user_id: share.recipient_user_id,
                recipient_group_id: share.recipient_group_id,
            }
        })
        .collect();

    Ok(Json(response))
}

#[utoipa::path(
    delete,
    path = "/api/v1/shares/{id}",
    tag = "Shares",
    params(("share_id" = uuid::Uuid, Path, description = "Share Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn revoke_share(
    State(state): State<AppState>,
    Path(share_id): Path<uuid::Uuid>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
) -> Result<StatusCode, AppError> {
    // Preserve the file id before the share is revoked so we can refresh the
    // AI index ACL afterwards.
    let share = state
        .share_service
        .get_share_by_id(share_id, user_id)
        .await?
        .ok_or(AppError::not_found("Share not found"))?;

    state.share_service.revoke_share(share_id, user_id).await?;

    // Best-effort refresh if the share targeted a file (note).
    if let Some(file_id) = share.file_id {
        if let Err(e) = state
            .note_service
            .refresh_note_index_acl(file_id, user_id, share.tenant_id)
            .await
        {
            tracing::warn!(
                file_id = %file_id,
                share_id = %share_id,
                error = %e,
                "Failed to refresh AI index ACL after share revocation"
            );
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/shares/{id}/access-log",
    tag = "Shares",
    params(("share_id" = uuid::Uuid, Path, description = "Share Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_share_access_log(
    State(state): State<AppState>,
    Path(share_id): Path<uuid::Uuid>,
    Query(query): Query<ShareAccessLogQuery>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
) -> Result<Json<Vec<ShareAccessLogResponse>>, AppError> {
    let requested_limit = query.limit.unwrap_or(50);
    let limit = requested_limit.clamp(1, 200);

    let entries = state
        .metadata_store
        .get_public_share_access_log(share_id, user_id, limit)
        .await
        .map_err(|error| {
            tracing::error!("Failed to fetch share access log: {error}");
            AppError::internal("Failed to fetch share access log")
        })?;

    let response = entries
        .into_iter()
        .map(|entry| ShareAccessLogResponse {
            accessed_at: entry.accessed_at,
            action: entry.action,
            success: entry.success,
            actor_type: entry.actor_type,
            actor_label: entry.actor_label,
            ip_address: entry.ip_address,
            user_agent: entry.user_agent,
            share_session_id: entry.share_session_id,
            share_session_subject: entry.share_session_subject,
        })
        .collect();

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::services::ShareError;

    // Note: Full integration tests require axum_test which is not yet configured.
    // These tests verify that the handler functions are correctly typed and compile.
    // Integration tests will be added when test infrastructure is set up.

    #[test]
    fn test_share_error_response_mappings() {
        // Test that error mappings are correct
        let share_id = Uuid::new_v4();
        let _response: AppError = ShareError::ShareNotFound(share_id).into();
        // Response is created - just verify it compiles
        drop(_response);

        let file_id = Uuid::new_v4();
        let _response: AppError = ShareError::FileNotFound(file_id).into();
        drop(_response);

        let _response: AppError = ShareError::Expired.into();
        drop(_response);

        let _response: AppError = ShareError::PasswordRequired.into();
        drop(_response);
    }

    #[test]
    fn test_share_request_deserialization() {
        // Test that CreateShareRequest can be deserialized
        let json = serde_json::json!({
            "permissions": "View",
            "password": "test123",
            "expires_at": "2026-12-31T23:59:59Z",
            "upload_only": true
        });

        let req: Result<CreateShareRequest, _> = serde_json::from_value(json);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.permissions, SharePermissions::View);
        assert_eq!(req.password, Some("test123".to_string()));
        assert!(req.expires_at.is_some());
        assert!(req.upload_only);
    }

    #[test]
    fn test_share_request_deserialization_minimal() {
        // Test that CreateShareRequest with minimal fields works
        let json = serde_json::json!({
            "permissions": "Edit"
        });

        let req: Result<CreateShareRequest, _> = serde_json::from_value(json);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.permissions, SharePermissions::Edit);
        assert_eq!(req.password, None);
        assert_eq!(req.expires_at, None);
        assert!(!req.upload_only);
    }
}
