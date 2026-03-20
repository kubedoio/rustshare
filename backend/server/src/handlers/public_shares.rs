//! HTTP handlers for public share access operations.
//!
//! This module provides anonymous access to shared files via share tokens.
//! It includes session creation with password validation and file download.

use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

use crate::{handlers::ShareSessionAuth, AppState};

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
    pub resource_id: Uuid,
    pub resource_type: String,
    pub name: String,
    pub file_size: Option<i64>,
    pub mime_type: Option<String>,
    pub password_protected: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct SharedFolderContentsResponse {
    pub root_folder_id: Uuid,
    pub current_folder_id: Uuid,
    pub current_folder_name: String,
    pub path: String,
    pub folders: Vec<rustshare_core::domain::Folder>,
    pub files: Vec<rustshare_core::domain::File>,
}

#[derive(Debug, Deserialize)]
pub struct SharedFolderContentsQuery {
    #[serde(default)]
    pub folder_id: Option<Uuid>,
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
    let (share, file, folder) = state
        .share_service
        .get_public_share_info(&token)
        .await
        .map_err(super::share_error_response)?;

    if let Some(file) = file {
        Ok(Json(ShareInfoResponse {
            resource_id: file.id,
            resource_type: "file".to_string(),
            name: file.name,
            file_size: Some(file.size),
            mime_type: Some(file.mime_type),
            password_protected: share.password_hash.is_some(),
            expires_at: share.expires_at,
        })
        .into_response())
    } else if let Some(folder) = folder {
        Ok(Json(ShareInfoResponse {
            resource_id: folder.id,
            resource_type: "folder".to_string(),
            name: folder.name,
            file_size: None,
            mime_type: None,
            password_protected: share.password_hash.is_some(),
            expires_at: share.expires_at,
        })
        .into_response())
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Share resource is missing"})),
        )
            .into_response())
    }
}

/// Download shared file (requires session JWT)
pub async fn download_shared_file(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ShareSessionAuth(claims): ShareSessionAuth,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    // Verify the JWT was issued for this specific share token
    let share_id_from_token = claims.share_id;

    // Get share to verify token matches
    let share = state
        .metadata_store
        .get_share_by_token(&token)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Database error: {}", e)})),
            )
                .into_response()
        })?
        .ok_or_else(|| super::share_error_response(rustshare_core::services::ShareError::NotFound))?;

    // Verify JWT share_id matches the share we're accessing
    if share.id != share_id_from_token {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Token is not valid for this share"})),
        )
            .into_response());
    }

    // Get file metadata
    let file_id = share.file_id.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "This share is not for a file"})),
        )
            .into_response()
    })?;

    let file = state
        .metadata_store
        .find_file_by_id(file_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Database error: {}", e)})),
            )
                .into_response()
        })?
        .ok_or_else(|| super::share_error_response(rustshare_core::services::ShareError::FileNotFound(file_id)))?;

    // Get file content from storage
    let content = state
        .object_store
        .get(&file.storage_key())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to retrieve file: {}", e)})),
            )
                .into_response()
        })?;

    // Extract request metadata
    let ip_address = Some(addr.ip().to_string());
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    // Increment access count - log error but continue
    if let Err(e) = state.metadata_store.increment_share_access(share.id).await {
        tracing::warn!("Failed to increment share access count: {}", e);
    }

    // Log access with request metadata
    if let Err(e) = state.metadata_store.log_share_access(
        share.id,
        ip_address,
        user_agent,
        "download".to_string(),
        true,
    ).await {
        tracing::warn!("Failed to log share access: {}", e);
    }

    // Return file with appropriate headers
    Ok((
        StatusCode::OK,
        [
            ("Content-Type", file.mime_type.as_str()),
            ("Content-Disposition", &format!("attachment; filename=\"{}\"", file.name)),
            ("Content-Length", &content.len().to_string()),
        ],
        content,
    )
        .into_response())
}

/// List contents of a shared folder.
pub async fn get_shared_folder_contents(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ShareSessionAuth(claims): ShareSessionAuth,
    Query(query): Query<SharedFolderContentsQuery>,
) -> Result<Response, Response> {
    let share = state
        .metadata_store
        .get_share_by_token(&token)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Database error: {}", e)})),
            )
                .into_response()
        })?
        .ok_or_else(|| super::share_error_response(rustshare_core::services::ShareError::NotFound))?;

    if share.id != claims.share_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Token is not valid for this share"})),
        )
            .into_response());
    }

    if share.folder_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "This share is not for a folder"})),
        )
            .into_response());
    }

    let (_share, current_folder, folders, files) = state
        .share_service
        .list_public_folder_contents(&token, query.folder_id)
        .await
        .map_err(super::share_error_response)?;

    Ok(Json(SharedFolderContentsResponse {
        root_folder_id: share.folder_id.expect("checked above"),
        current_folder_id: current_folder.id,
        current_folder_name: current_folder.name,
        path: current_folder.path,
        folders,
        files,
    })
    .into_response())
}

/// Download a file from a shared folder.
pub async fn download_shared_folder_file(
    State(state): State<AppState>,
    Path((token, file_id)): Path<(String, Uuid)>,
    ShareSessionAuth(claims): ShareSessionAuth,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    let share = state
        .metadata_store
        .get_share_by_token(&token)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Database error: {}", e)})),
            )
                .into_response()
        })?
        .ok_or_else(|| super::share_error_response(rustshare_core::services::ShareError::NotFound))?;

    if share.id != claims.share_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Token is not valid for this share"})),
        )
            .into_response());
    }

    let root_folder_id = share.folder_id.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "This share is not for a folder"})),
        )
            .into_response()
    })?;

    let descendants = state
        .metadata_store
        .find_descendant_folders(root_folder_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Database error: {}", e)})),
            )
                .into_response()
        })?;

    let file = state
        .metadata_store
        .find_file_by_id(file_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Database error: {}", e)})),
            )
                .into_response()
        })?
        .ok_or_else(|| super::share_error_response(rustshare_core::services::ShareError::FileNotFound(file_id)))?;

    let allowed_folder_ids: Vec<Uuid> = descendants.into_iter().map(|folder| folder.id).collect();
    if !allowed_folder_ids.contains(&file.parent_folder_id.unwrap_or(Uuid::nil())) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "File is not inside the shared folder"})),
        )
            .into_response());
    }

    let content = state
        .object_store
        .get(&file.storage_key())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to retrieve file: {}", e)})),
            )
                .into_response()
        })?;

    let ip_address = Some(addr.ip().to_string());
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    if let Err(e) = state.metadata_store.increment_share_access(share.id).await {
        tracing::warn!("Failed to increment share access count: {}", e);
    }

    if let Err(e) = state
        .metadata_store
        .log_share_access(share.id, ip_address, user_agent, "download".to_string(), true)
        .await
    {
        tracing::warn!("Failed to log share access: {}", e);
    }

    Ok((
        StatusCode::OK,
        [
            ("Content-Type", file.mime_type.as_str()),
            ("Content-Disposition", &format!("attachment; filename=\"{}\"", file.name)),
            ("Content-Length", &content.len().to_string()),
        ],
        content,
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
            resource_id: file_id,
            resource_type: "file".to_string(),
            name: "test.pdf".to_string(),
            file_size: Some(1024),
            mime_type: Some("application/pdf".to_string()),
            password_protected: true,
            expires_at: Some(
                chrono::DateTime::parse_from_rfc3339("2026-12-31T23:59:59Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            ),
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["name"], "test.pdf");
        assert_eq!(json["file_size"], 1024);
        assert_eq!(json["mime_type"], "application/pdf");
        assert_eq!(json["password_protected"], true);
        assert!(json["expires_at"].is_string());
    }
}
