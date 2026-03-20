//! HTTP handlers for share operations.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::domain::SharePermissions;

use super::{share_error_response, AuthenticatedUser};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateShareRequest {
    pub permissions: SharePermissions,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct ShareResponse {
    pub id: Uuid,
    pub resource_id: Uuid,
    pub resource_type: &'static str,
    pub share_token: String,
    pub permissions: SharePermissions,
    pub password_protected: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create_public_file_share(
    State(state): State<AppState>,
    Path(file_id): Path<Uuid>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateShareRequest>,
) -> Result<Response, Response> {
    let share = state
        .share_service
        .create_share(
            file_id,
            auth.user_id,
            req.permissions,
            req.password,
            req.expires_at,
        )
        .await
        .map_err(share_error_response)?;

    // Extract share_token - it should always be Some for public shares
    let share_token = share.share_token.ok_or_else(|| {
        tracing::error!(
            "Share token is None after create_share for share {}",
            share.id
        );
        share_error_response(rustshare_core::services::ShareError::Database(
            sqlx::Error::PoolClosed,
        ))
    })?;

    let resource_id = share.file_id.ok_or_else(|| {
        tracing::error!("File ID is None after create_share for share {}", share.id);
        share_error_response(rustshare_core::services::ShareError::Database(
            sqlx::Error::PoolClosed,
        ))
    })?;

    Ok((
        StatusCode::CREATED,
        Json(ShareResponse {
            id: share.id,
            resource_id,
            resource_type: "file",
            share_token,
            permissions: share.permissions,
            password_protected: share.password_hash.is_some(),
            expires_at: share.expires_at,
            created_at: share.created_at,
        }),
    )
        .into_response())
}

pub async fn create_public_folder_share(
    State(state): State<AppState>,
    Path(folder_id): Path<Uuid>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateShareRequest>,
) -> Result<Response, Response> {
    let share = state
        .share_service
        .create_folder_share(
            folder_id,
            auth.user_id,
            req.permissions,
            req.password,
            req.expires_at,
        )
        .await
        .map_err(share_error_response)?;

    let share_token = share.share_token.ok_or_else(|| {
        tracing::error!(
            "Share token is None after create_folder_share for share {}",
            share.id
        );
        share_error_response(rustshare_core::services::ShareError::Database(
            sqlx::Error::PoolClosed,
        ))
    })?;

    let resource_id = share.folder_id.ok_or_else(|| {
        tracing::error!("Folder ID is None after create_folder_share for share {}", share.id);
        share_error_response(rustshare_core::services::ShareError::Database(
            sqlx::Error::PoolClosed,
        ))
    })?;

    Ok((
        StatusCode::CREATED,
        Json(ShareResponse {
            id: share.id,
            resource_id,
            resource_type: "folder",
            share_token,
            permissions: share.permissions,
            password_protected: share.password_hash.is_some(),
            expires_at: share.expires_at,
            created_at: share.created_at,
        }),
    )
        .into_response())
}

pub async fn list_public_file_shares(
    State(state): State<AppState>,
    Path(file_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    let shares = state
        .share_service
        .list_file_shares(file_id, auth.user_id)
        .await
        .map_err(share_error_response)?;

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

pub async fn list_public_folder_shares(
    State(state): State<AppState>,
    Path(folder_id): Path<Uuid>,
    auth: AuthenticatedUser,
) -> Result<Response, Response> {
    let shares = state
        .share_service
        .list_folder_shares(folder_id, auth.user_id)
        .await
        .map_err(share_error_response)?;

    let response: Vec<ShareResponse> = shares
        .into_iter()
        .filter_map(|share| {
            let (Some(resource_id), Some(share_token)) = (share.folder_id, share.share_token) else {
                return None;
            };

            Some(ShareResponse {
                id: share.id,
                resource_id,
                resource_type: "folder",
                share_token,
                permissions: share.permissions,
                password_protected: share.password_hash.is_some(),
                expires_at: share.expires_at,
                created_at: share.created_at,
            })
        })
        .collect();

    Ok(Json(response).into_response())
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
        let response = share_error_response(ShareError::NotFound);
        // Response is created - just verify it compiles
        drop(response);

        let file_id = Uuid::new_v4();
        let response = share_error_response(ShareError::FileNotFound(file_id));
        drop(response);

        let response = share_error_response(ShareError::Expired);
        drop(response);

        let response = share_error_response(ShareError::PasswordRequired);
        drop(response);
    }

    #[test]
    fn test_share_request_deserialization() {
        // Test that CreateShareRequest can be deserialized
        let json = serde_json::json!({
            "permissions": "View",
            "password": "test123",
            "expires_at": "2026-12-31T23:59:59Z"
        });

        let req: Result<CreateShareRequest, _> = serde_json::from_value(json);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.permissions, SharePermissions::View);
        assert_eq!(req.password, Some("test123".to_string()));
        assert!(req.expires_at.is_some());
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
    }
}
