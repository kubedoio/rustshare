//! HTTP handlers for public share access operations.
//!
//! This module provides anonymous access to shared files via share tokens.
//! It includes session creation with password validation and file download.

use axum::{
    extract::{ConnectInfo, Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

use rustshare_core::{
    domain::SharePermissions,
    services::{FileError, FileUploadActor},
};
use rustshare_storage::ShareAccessLogEntry;

use crate::{handlers::ShareSessionAuth, AppState};

use super::files::FileUploadResponse;

type HandlerResponseError = Box<Response>;

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub session_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub permissions: SharePermissions,
    pub upload_only: bool,
}

#[derive(Debug, Serialize)]
pub struct ShareInfoResponse {
    pub resource_id: Uuid,
    pub resource_type: String,
    pub name: String,
    pub permissions: SharePermissions,
    pub upload_only: bool,
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
            permissions: session.permissions,
            upload_only: session.upload_only,
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
            permissions: share.permissions,
            upload_only: share.upload_only,
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
            permissions: share.permissions,
            upload_only: share.upload_only,
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

fn ensure_share_session_matches(
    share: &rustshare_core::domain::Share,
    claims: &rustshare_auth::ShareSessionClaims,
) -> Result<(), HandlerResponseError> {
    if share.id != claims.share_id {
        return Err(Box::new(
            (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": "Token is not valid for this share"})),
            )
                .into_response(),
        ));
    }

    Ok(())
}

async fn parse_upload_multipart(
    mut multipart: Multipart,
) -> Result<(Bytes, String, Option<Uuid>, String, Option<String>), Response> {
    let mut file_data: Option<Bytes> = None;
    let mut file_name: Option<String> = None;
    let mut parent_folder_id: Option<Uuid> = None;
    let mut uploader_name: Option<String> = None;
    let mut mime_type = "application/octet-stream".to_string();

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        super::file_error_response(FileError::Storage(format!(
            "Failed to read multipart field: {}",
            e
        )))
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "file" => {
                if let Some(content_type) = field.content_type() {
                    mime_type = content_type.to_string();
                }
                if file_name.is_none() {
                    if let Some(name) = field.file_name() {
                        file_name = Some(name.to_string());
                    }
                }
                file_data = Some(field.bytes().await.map_err(|e| {
                    super::file_error_response(FileError::Storage(format!(
                        "Failed to read file data: {}",
                        e
                    )))
                })?);
            }
            "name" => {
                file_name = Some(field.text().await.map_err(|e| {
                    super::file_error_response(FileError::Storage(format!(
                        "Failed to read name field: {}",
                        e
                    )))
                })?);
            }
            "parent_folder_id" => {
                let text = field.text().await.map_err(|e| {
                    super::file_error_response(FileError::Storage(format!(
                        "Failed to read parent_folder_id field: {}",
                        e
                    )))
                })?;

                if !text.is_empty() {
                    parent_folder_id = Some(Uuid::parse_str(&text).map_err(|_| {
                        super::file_error_response(FileError::InvalidName(
                            "Invalid parent_folder_id".to_string(),
                        ))
                    })?);
                }
            }
            "uploader_name" => {
                let text = field.text().await.map_err(|e| {
                    super::file_error_response(FileError::Storage(format!(
                        "Failed to read uploader_name field: {}",
                        e
                    )))
                })?;

                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    if trimmed.len() > 120 {
                        return Err(super::file_error_response(FileError::InvalidName(
                            "Uploader name must be 120 characters or fewer".to_string(),
                        )));
                    }
                    uploader_name = Some(trimmed.to_string());
                }
            }
            _ => {}
        }
    }

    let file_data = file_data.ok_or_else(|| {
        super::file_error_response(FileError::InvalidName("Missing file data".to_string()))
    })?;
    let file_name = file_name.ok_or_else(|| {
        super::file_error_response(FileError::InvalidName("Missing file name".to_string()))
    })?;

    // If mime_type is generic or not provided, guess from file extension
    if mime_type == "application/octet-stream" {
        mime_type = mime_guess::from_path(&file_name)
            .first_or_octet_stream()
            .to_string();
    }

    Ok((
        file_data,
        file_name,
        parent_folder_id,
        mime_type,
        uploader_name,
    ))
}

/// Download shared file (requires session JWT)
pub async fn download_shared_file(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ShareSessionAuth(claims): ShareSessionAuth,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, Response> {
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
        .ok_or_else(|| {
            super::share_error_response(rustshare_core::services::ShareError::ShareNotFoundByToken(
                token.clone(),
            ))
        })?;

    // Verify JWT share_id matches the share we're accessing
    ensure_share_session_matches(&share, &claims).map_err(|error| *error)?;

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
        .ok_or_else(|| {
            super::share_error_response(rustshare_core::services::ShareError::FileNotFound(file_id))
        })?;

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
    if let Err(e) = state
        .metadata_store
        .log_share_access(ShareAccessLogEntry {
            share_id: share.id,
            ip_address,
            user_agent,
            action: "download".to_string(),
            success: true,
            actor_type: Some("public_share_session".to_string()),
            actor_label: None,
            share_session_id: Some(claims.session_id),
            share_session_subject: Some(claims.sub.clone()),
        })
        .await
    {
        tracing::warn!("Failed to log share access: {}", e);
    }

    // Return file with appropriate headers
    Ok((
        StatusCode::OK,
        [
            ("Content-Type", file.mime_type.as_str()),
            (
                "Content-Disposition",
                &format!("attachment; filename=\"{}\"", file.name),
            ),
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
        .ok_or_else(|| {
            super::share_error_response(rustshare_core::services::ShareError::ShareNotFoundByToken(
                token.clone(),
            ))
        })?;

    ensure_share_session_matches(&share, &claims).map_err(|error| *error)?;

    if share.folder_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "This share is not for a folder"})),
        )
            .into_response());
    }

    if share.upload_only {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "This share is upload-only"})),
        )
            .into_response());
    }

    let (_share, current_folder, folders, files) = state
        .share_service
        .list_public_folder_contents(&token, query.folder_id)
        .await
        .map_err(super::share_error_response)?;

    let root_folder_id = share.folder_id.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Invalid share: missing folder_id"})),
        )
            .into_response()
    })?;

    Ok(Json(SharedFolderContentsResponse {
        root_folder_id,
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
        .ok_or_else(|| {
            super::share_error_response(rustshare_core::services::ShareError::ShareNotFoundByToken(
                token.clone(),
            ))
        })?;

    ensure_share_session_matches(&share, &claims).map_err(|error| *error)?;

    let root_folder_id = share.folder_id.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "This share is not for a folder"})),
        )
            .into_response()
    })?;

    if share.upload_only {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "This share is upload-only"})),
        )
            .into_response());
    }

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
        .ok_or_else(|| {
            super::share_error_response(rustshare_core::services::ShareError::FileNotFound(file_id))
        })?;

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
        .log_share_access(ShareAccessLogEntry {
            share_id: share.id,
            ip_address,
            user_agent,
            action: "download".to_string(),
            success: true,
            actor_type: Some("public_share_session".to_string()),
            actor_label: None,
            share_session_id: Some(claims.session_id),
            share_session_subject: Some(claims.sub.clone()),
        })
        .await
    {
        tracing::warn!("Failed to log share access: {}", e);
    }

    Ok((
        StatusCode::OK,
        [
            ("Content-Type", file.mime_type.as_str()),
            (
                "Content-Disposition",
                &format!("attachment; filename=\"{}\"", file.name),
            ),
            ("Content-Length", &content.len().to_string()),
        ],
        content,
    )
        .into_response())
}

/// Upload a file into a shared folder using an authenticated share session.
pub async fn upload_shared_folder_file(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ShareSessionAuth(claims): ShareSessionAuth,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    multipart: Multipart,
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
        .ok_or_else(|| {
            super::share_error_response(rustshare_core::services::ShareError::ShareNotFoundByToken(
                token.clone(),
            ))
        })?;

    ensure_share_session_matches(&share, &claims).map_err(|error| *error)?;

    let root_folder_id = share.folder_id.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "This share is not for a folder"})),
        )
            .into_response()
    })?;

    if !share.upload_only && claims.permissions < SharePermissions::Edit {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "This share does not allow uploads"})),
        )
            .into_response());
    }

    let (file_data, file_name, requested_folder_id, mime_type, uploader_name) =
        parse_upload_multipart(multipart).await?;

    let root_folder = state
        .metadata_store
        .find_folder_by_id(root_folder_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Database error: {}", e)})),
            )
                .into_response()
        })?
        .ok_or_else(|| {
            super::share_error_response(rustshare_core::services::ShareError::FolderNotFound(
                root_folder_id,
            ))
        })?;

    if share.upload_only
        && requested_folder_id.is_some()
        && requested_folder_id != Some(root_folder_id)
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Upload-only shares can only upload to the shared root folder"
            })),
        )
            .into_response());
    }

    let target_folder_id = requested_folder_id.unwrap_or(root_folder_id);
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

    if !descendants
        .iter()
        .any(|folder| folder.id == target_folder_id)
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Target folder is outside the shared folder"})),
        )
            .into_response());
    }

    let file = state
        .file_service
        .upload_file_with_actor(
            root_folder.owner_id,
            FileUploadActor {
                actor_type: "public_share_session".to_string(),
                actor_user_id: None,
                actor_share_id: Some(share.id),
                actor_share_session_id: Some(claims.session_id),
                actor_display_name: uploader_name.clone(),
            },
            file_name,
            Some(target_folder_id),
            file_data,
            mime_type,
            share.tenant_id,
        )
        .await
        .map_err(super::file_error_response)?;

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
        .log_share_access(ShareAccessLogEntry {
            share_id: share.id,
            ip_address,
            user_agent,
            action: "upload".to_string(),
            success: true,
            actor_type: Some("public_share_session".to_string()),
            actor_label: uploader_name,
            share_session_id: Some(claims.session_id),
            share_session_subject: Some(claims.sub.clone()),
        })
        .await
    {
        tracing::warn!("Failed to log share upload: {}", e);
    }

    Ok((
        StatusCode::OK,
        Json(FileUploadResponse {
            id: file.id,
            name: file.name,
            size: file.size,
            mime_type: file.mime_type,
            content_hash: file.content_hash,
            current_version: file.current_version,
            created_at: file.created_at.to_rfc3339(),
        }),
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
            permissions: SharePermissions::Edit,
            upload_only: true,
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["session_token"], "test_token_123");
        assert_eq!(json["permissions"], "Edit");
        assert_eq!(json["upload_only"], true);
        assert!(json["expires_at"].is_string());
    }

    #[test]
    fn test_share_info_response_serialization() {
        let file_id = Uuid::new_v4();
        let response = ShareInfoResponse {
            resource_id: file_id,
            resource_type: "file".to_string(),
            name: "test.pdf".to_string(),
            permissions: SharePermissions::View,
            upload_only: false,
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
        assert_eq!(json["permissions"], "View");
        assert_eq!(json["upload_only"], false);
        assert_eq!(json["file_size"], 1024);
        assert_eq!(json["mime_type"], "application/pdf");
        assert_eq!(json["password_protected"], true);
        assert!(json["expires_at"].is_string());
    }
}
