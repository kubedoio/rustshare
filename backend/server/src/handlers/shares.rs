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

use crate::AppState;
use super::{AuthenticatedUser, share_error_response};

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
    pub file_id: Uuid,
    pub share_token: String,
    pub permissions: SharePermissions,
    pub password_protected: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create_share(
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

    Ok((
        StatusCode::CREATED,
        Json(ShareResponse {
            id: share.id,
            file_id: share.file_id,
            share_token: share.share_token,
            permissions: share.permissions,
            password_protected: share.password_hash.is_some(),
            expires_at: share.expires_at,
            created_at: share.created_at,
        }),
    )
        .into_response())
}

pub async fn list_file_shares(
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
        .map(|s| ShareResponse {
            id: s.id,
            file_id: s.file_id,
            share_token: s.share_token,
            permissions: s.permissions,
            password_protected: s.password_hash.is_some(),
            expires_at: s.expires_at,
            created_at: s.created_at,
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
            "permissions": "Read",
            "password": "test123",
            "expires_at": "2026-12-31T23:59:59Z"
        });

        let req: Result<CreateShareRequest, _> = serde_json::from_value(json);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.permissions, SharePermissions::Read);
        assert_eq!(req.password, Some("test123".to_string()));
        assert!(req.expires_at.is_some());
    }

    #[test]
    fn test_share_request_deserialization_minimal() {
        // Test that CreateShareRequest with minimal fields works
        let json = serde_json::json!({
            "permissions": "ReadWrite"
        });

        let req: Result<CreateShareRequest, _> = serde_json::from_value(json);
        assert!(req.is_ok());
        let req = req.unwrap();
        assert_eq!(req.permissions, SharePermissions::ReadWrite);
        assert_eq!(req.password, None);
        assert_eq!(req.expires_at, None);
    }
}
