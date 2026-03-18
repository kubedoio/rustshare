//! HTTP handlers for public share access operations.
//!
//! This module provides anonymous access to shared files via share tokens.
//! It includes session creation with password validation and file download.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub session_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct ShareInfoResponse {
    pub file_id: Uuid,
    pub file_name: String,
    pub file_size: i64,
    pub mime_type: String,
    pub password_protected: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Create anonymous session for share access
pub async fn create_session(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Response, Response> {
    let session = state
        .share_service
        .validate_and_create_session(&token, req.password)
        .await
        .map_err(super::share_error_response)?;

    Ok((
        StatusCode::OK,
        Json(SessionResponse {
            session_token: session.token,
            expires_at: session.expires_at,
        }),
    )
        .into_response())
}

/// Get share info without authentication
pub async fn get_share_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Response, Response> {
    // Get share by token
    let share = state
        .metadata_store
        .get_share_by_token(&token)
        .await
        .map_err(|_| super::share_error_response(rustshare_core::services::ShareError::NotFound))?
        .ok_or_else(|| super::share_error_response(rustshare_core::services::ShareError::NotFound))?;

    // Check if revoked
    if share.revoked_at.is_some() {
        return Err(super::share_error_response(rustshare_core::services::ShareError::Revoked));
    }

    // Check if expired
    if let Some(expires_at) = share.expires_at {
        if expires_at < chrono::Utc::now() {
            return Err(super::share_error_response(rustshare_core::services::ShareError::Expired));
        }
    }

    // Get file info
    let file = state
        .metadata_store
        .find_file_by_id(share.file_id)
        .await
        .map_err(|_| super::share_error_response(rustshare_core::services::ShareError::FileNotFound(share.file_id)))?
        .ok_or_else(|| super::share_error_response(rustshare_core::services::ShareError::FileNotFound(share.file_id)))?;

    Ok(Json(ShareInfoResponse {
        file_id: file.id,
        file_name: file.name,
        file_size: file.size,
        mime_type: file.mime_type,
        password_protected: share.password_hash.is_some(),
        expires_at: share.expires_at,
    })
    .into_response())
}

/// Download shared file (requires session JWT)
pub async fn download_shared_file(
    State(_state): State<AppState>,
    Path(_token): Path<String>,
    // TODO: Add ShareSessionAuth extractor to verify session JWT
) -> Result<Response, Response> {
    // For now, return not implemented
    // This will be completed when we implement the ShareSessionAuth extractor
    Ok((
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({"error": "File download requires ShareSessionAuth extractor"})),
    )
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_session_request_deserialization() {
        let json = r#"{"password": "test123"}"#;
        let req: CreateSessionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.password, Some("test123".to_string()));
    }

    #[test]
    fn test_create_session_request_no_password() {
        let json = r#"{}"#;
        let req: CreateSessionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.password, None);
    }

    #[test]
    fn test_session_response_serialization() {
        let response = SessionResponse {
            session_token: "test_token_123".to_string(),
            expires_at: chrono::DateTime::parse_from_rfc3339("2026-12-31T23:59:59Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["session_token"], "test_token_123");
        assert!(json["expires_at"].is_string());
    }

    #[test]
    fn test_share_info_response_serialization() {
        let file_id = Uuid::new_v4();
        let response = ShareInfoResponse {
            file_id,
            file_name: "test.pdf".to_string(),
            file_size: 1024,
            mime_type: "application/pdf".to_string(),
            password_protected: true,
            expires_at: Some(
                chrono::DateTime::parse_from_rfc3339("2026-12-31T23:59:59Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            ),
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["file_name"], "test.pdf");
        assert_eq!(json["file_size"], 1024);
        assert_eq!(json["mime_type"], "application/pdf");
        assert_eq!(json["password_protected"], true);
        assert!(json["expires_at"].is_string());
    }
}
