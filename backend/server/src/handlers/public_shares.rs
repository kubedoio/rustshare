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
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

use rustshare_core::{domain::SharePermissions, services::FileUploadActor};
use rustshare_storage::ShareAccessLogEntry;

use crate::{handlers::ShareSessionAuth, AppState};

use super::files::FileUploadResponse;
use crate::handlers::AppError;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SessionResponse {
    pub session_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub permissions: SharePermissions,
    pub upload_only: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SharedFolderContentsResponse {
    pub root_folder_id: Uuid,
    pub current_folder_id: Uuid,
    pub current_folder_name: String,
    pub path: String,
    pub folders: Vec<rustshare_core::domain::Folder>,
    pub files: Vec<rustshare_core::domain::File>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SharedFolderContentsQuery {
    #[serde(default)]
    pub folder_id: Option<Uuid>,
}

/// Create anonymous session for share access
#[utoipa::path(
    post,
    path = "/api/v1/public/share/{token}/session",
    tag = "Public Shares",
    params(("token" = String, Path, description = "Share token")),
    request_body = CreateSessionRequest,
    responses(
        (status = 200, description = "Session created", body = SessionResponse),
        (status = 401, description = "Password required or invalid", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Share not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_session(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Response, AppError> {
    let session = state
        .share_service
        .validate_and_create_session(&token, req.password)
        .await?;

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
#[utoipa::path(
    get,
    path = "/api/v1/public/share/{token}/info",
    tag = "Public Shares",
    params(("token" = String, Path, description = "Share token")),
    responses(
        (status = 200, description = "Share information", body = ShareInfoResponse),
        (status = 404, description = "Share not found or revoked", body = crate::handlers::ErrorResponse),
        (status = 410, description = "Share expired or revoked", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_share_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Response, AppError> {
    let (share, file, folder) = state.share_service.get_public_share_info(&token).await?;

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
        Err(AppError::internal("Share resource is missing"))
    }
}

fn ensure_share_session_matches(
    share: &rustshare_core::domain::Share,
    claims: &rustshare_auth::ShareSessionClaims,
) -> Result<(), AppError> {
    if share.id != claims.share_id {
        return Err(AppError::forbidden("Token is not valid for this share"));
    }

    Ok(())
}

fn ensure_share_is_active(share: &rustshare_core::domain::Share) -> Result<(), AppError> {
    if share.revoked_at.is_some() {
        return Err(AppError::from(
            rustshare_core::services::ShareError::Revoked,
        ));
    }
    if share.is_expired() {
        return Err(AppError::from(
            rustshare_core::services::ShareError::Expired,
        ));
    }
    Ok(())
}

/// Stream a multipart field to a temporary file and return the temp file plus size.
/// Enforces a per-field size limit during streaming to prevent OOM.
async fn stream_multipart_field_to_temp_file(
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
        tokio::io::AsyncWriteExt::write_all(&mut async_file, &chunk)
            .await
            .map_err(|e| {
                tracing::error!("Failed to write to temp file: {e}");
                AppError::internal(format!("Failed to write to temp file: {e}"))
            })?;
    }

    tokio::io::AsyncWriteExt::flush(&mut async_file)
        .await
        .map_err(|e| AppError::internal(format!("Failed to flush temp file: {e}")))?;

    Ok((temp_file, total_size))
}

async fn parse_upload_multipart(
    mut multipart: Multipart,
) -> Result<
    (
        tempfile::NamedTempFile,
        String,
        Option<Uuid>,
        String,
        Option<String>,
    ),
    AppError,
> {
    let mut file_temp: Option<tempfile::NamedTempFile> = None;
    let mut file_name: Option<String> = None;
    let mut parent_folder_id: Option<Uuid> = None;
    let mut uploader_name: Option<String> = None;
    let mut mime_type = "application/octet-stream".to_string();

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::internal(format!("Failed to read multipart field: {}", e)))?
    {
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
                file_temp = Some(
                    stream_multipart_field_to_temp_file(&mut field, MAX_PUBLIC_UPLOAD_SIZE)
                        .await?
                        .0,
                );
            }
            "name" => {
                file_name = Some(field.text().await.map_err(|e| {
                    AppError::internal(format!("Failed to read name field: {}", e))
                })?);
            }
            "parent_folder_id" => {
                let text = field.text().await.map_err(|e| {
                    AppError::internal(format!("Failed to read parent_folder_id field: {}", e))
                })?;

                if !text.is_empty() {
                    parent_folder_id = Some(
                        Uuid::parse_str(&text)
                            .map_err(|_| AppError::bad_request("Invalid parent_folder_id"))?,
                    );
                }
            }
            "uploader_name" => {
                let text = field.text().await.map_err(|e| {
                    AppError::internal(format!("Failed to read uploader_name field: {}", e))
                })?;

                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    if trimmed.len() > 120 {
                        return Err(AppError::bad_request(
                            "Uploader name must be 120 characters or fewer",
                        ));
                    }
                    uploader_name = Some(trimmed.to_string());
                }
            }
            _ => {}
        }
    }

    let file_temp = file_temp.ok_or_else(|| AppError::bad_request("Missing file data"))?;
    let file_name = file_name.ok_or_else(|| AppError::bad_request("Missing file name"))?;

    // If mime_type is generic or not provided, guess from file extension
    if mime_type == "application/octet-stream" {
        mime_type = mime_guess::from_path(&file_name)
            .first_or_octet_stream()
            .to_string();
    }

    Ok((
        file_temp,
        file_name,
        parent_folder_id,
        mime_type,
        uploader_name,
    ))
}

/// Download shared file (requires session JWT)
#[utoipa::path(
    get,
    path = "/api/v1/public/share/{token}/file",
    tag = "Public Shares",
    params(("token" = String, Path, description = "Token")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn download_shared_file(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ShareSessionAuth(claims): ShareSessionAuth,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    // Get share to verify token matches
    let share = state
        .metadata_store
        .get_share_by_token(&token)
        .await
        .map_err(|e| AppError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| {
            AppError::from(rustshare_core::services::ShareError::ShareNotFoundByToken(
                token.clone(),
            ))
        })?;

    // Verify JWT share_id matches the share we're accessing
    ensure_share_session_matches(&share, &claims)?;

    // Re-check revocation and expiration to block already-issued tokens
    ensure_share_is_active(&share)?;

    // Get file metadata
    let file_id = share
        .file_id
        .ok_or_else(|| AppError::bad_request("This share is not for a file"))?;

    let file = state
        .metadata_store
        .find_file_by_id(file_id, share.created_by)
        .await
        .map_err(|e| AppError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| {
            AppError::from(rustshare_core::services::ShareError::FileNotFound(file_id))
        })?;

    if file.name.starts_with(".rustshare")
        || file.name == "events.jsonl"
        || file.name == "index.md"
        || file.name == "__primary__.md"
        || file.name.ends_with(".editor.json")
    {
        return Err(AppError::from(
            rustshare_core::services::ShareError::FileNotFound(file_id),
        ));
    }

    // Get file content from storage
    let content = state
        .object_store
        .get(&file.storage_key())
        .await
        .map_err(|e| AppError::internal(format!("Failed to retrieve file: {}", e)))?;

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
#[utoipa::path(
    get,
    path = "/api/v1/public/share/{token}/folder/contents",
    tag = "Public Shares",
    params(("token" = String, Path, description = "Token")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_shared_folder_contents(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ShareSessionAuth(claims): ShareSessionAuth,
    Query(query): Query<SharedFolderContentsQuery>,
) -> Result<Response, AppError> {
    let share = state
        .metadata_store
        .get_share_by_token(&token)
        .await
        .map_err(|e| AppError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| {
            AppError::from(rustshare_core::services::ShareError::ShareNotFoundByToken(
                token.clone(),
            ))
        })?;

    ensure_share_session_matches(&share, &claims)?;

    // Re-check revocation and expiration to block already-issued tokens
    ensure_share_is_active(&share)?;

    if share.folder_id.is_none() {
        return Err(AppError::bad_request("This share is not for a folder"));
    }

    if share.upload_only {
        return Err(AppError::forbidden("This share is upload-only"));
    }

    let (_share, current_folder, folders, files) = state
        .share_service
        .list_public_folder_contents(&token, query.folder_id)
        .await?;

    let root_folder_id = share
        .folder_id
        .ok_or_else(|| AppError::internal("Invalid share: missing folder_id"))?;

    // Hide module metadata sidecar files from public share listings
    let visible_files: Vec<_> = files
        .into_iter()
        .filter(|f| {
            !f.name.starts_with(".rustshare")
                && f.name != "events.jsonl"
                && f.name != "index.md"
                && f.name != "__primary__.md"
                && !f.name.ends_with(".editor.json")
        })
        .collect();

    Ok(Json(SharedFolderContentsResponse {
        root_folder_id,
        current_folder_id: current_folder.id,
        current_folder_name: current_folder.name,
        path: current_folder.path,
        folders,
        files: visible_files,
    })
    .into_response())
}

/// Download a file from a shared folder.
#[utoipa::path(
    get,
    path = "/api/v1/public/share/{token}/folder/files/{file_id}",
    tag = "Public Shares",
    params(("token" = String, Path, description = "Token"), ("file_id" = Uuid, Path, description = "File Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn download_shared_folder_file(
    State(state): State<AppState>,
    Path((token, file_id)): Path<(String, Uuid)>,
    ShareSessionAuth(claims): ShareSessionAuth,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let share = state
        .metadata_store
        .get_share_by_token(&token)
        .await
        .map_err(|e| AppError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| {
            AppError::from(rustshare_core::services::ShareError::ShareNotFoundByToken(
                token.clone(),
            ))
        })?;

    ensure_share_session_matches(&share, &claims)?;

    // Re-check revocation and expiration to block already-issued tokens
    ensure_share_is_active(&share)?;

    let root_folder_id = share
        .folder_id
        .ok_or_else(|| AppError::bad_request("This share is not for a folder"))?;

    if share.upload_only {
        return Err(AppError::forbidden("This share is upload-only"));
    }

    let descendants = state
        .metadata_store
        .find_descendant_folders_unchecked(root_folder_id)
        .await
        .map_err(|e| AppError::internal(format!("Database error: {}", e)))?;

    let file = state
        .metadata_store
        .find_file_by_id(file_id, share.created_by)
        .await
        .map_err(|e| AppError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| {
            AppError::from(rustshare_core::services::ShareError::FileNotFound(file_id))
        })?;

    if file.name.starts_with(".rustshare")
        || file.name == "events.jsonl"
        || file.name == "index.md"
        || file.name == "__primary__.md"
        || file.name.ends_with(".editor.json")
    {
        return Err(AppError::from(
            rustshare_core::services::ShareError::FileNotFound(file_id),
        ));
    }

    let allowed_folder_ids: Vec<Uuid> = descendants.into_iter().map(|folder| folder.id).collect();
    if !allowed_folder_ids.contains(&file.parent_folder_id.unwrap_or(Uuid::nil())) {
        return Err(AppError::forbidden("File is not inside the shared folder"));
    }

    let content = state
        .object_store
        .get(&file.storage_key())
        .await
        .map_err(|e| AppError::internal(format!("Failed to retrieve file: {}", e)))?;

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

/// Maximum file size for public share uploads (100MB).
const MAX_PUBLIC_UPLOAD_SIZE: usize = 100 * 1024 * 1024;

/// Upload a file into a shared folder using an authenticated share session.
#[utoipa::path(
    post,
    path = "/api/v1/public/share/{token}/folder/upload",
    tag = "Public Shares",
    params(("token" = String, Path, description = "Token")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn upload_shared_folder_file(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ShareSessionAuth(claims): ShareSessionAuth,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Response, AppError> {
    let share = state
        .metadata_store
        .get_share_by_token(&token)
        .await
        .map_err(|e| AppError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| {
            AppError::from(rustshare_core::services::ShareError::ShareNotFoundByToken(
                token.clone(),
            ))
        })?;

    ensure_share_session_matches(&share, &claims)?;

    // Re-check revocation and expiration to block already-issued tokens
    ensure_share_is_active(&share)?;

    let root_folder_id = share
        .folder_id
        .ok_or_else(|| AppError::bad_request("This share is not for a folder"))?;

    if !share.upload_only && claims.permissions < SharePermissions::Edit {
        return Err(AppError::forbidden("This share does not allow uploads"));
    }

    let (file_temp, file_name, requested_folder_id, mime_type, uploader_name) =
        parse_upload_multipart(multipart).await?;
    let file_path = file_temp.path();

    let root_folder = state
        .metadata_store
        .find_folder_by_id(root_folder_id, share.created_by)
        .await
        .map_err(|e| AppError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| {
            AppError::from(rustshare_core::services::ShareError::FolderNotFound(
                root_folder_id,
            ))
        })?;

    if share.upload_only
        && requested_folder_id.is_some()
        && requested_folder_id != Some(root_folder_id)
    {
        return Err(AppError::forbidden(
            "Upload-only shares can only upload to the shared root folder",
        ));
    }

    let target_folder_id = requested_folder_id.unwrap_or(root_folder_id);
    let descendants = state
        .metadata_store
        .find_descendant_folders_unchecked(root_folder_id)
        .await
        .map_err(|e| AppError::internal(format!("Database error: {}", e)))?;

    if !descendants
        .iter()
        .any(|folder| folder.id == target_folder_id)
    {
        return Err(AppError::forbidden(
            "Target folder is outside the shared folder",
        ));
    }

    let file = state
        .file_service
        .upload_file_with_actor_from_path(
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
            file_path,
            mime_type,
            share.tenant_id,
        )
        .await?;

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
            current_version: file.current_version,
            created_at: file.created_at.to_rfc3339(),
        }),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SESSION_TOKEN: &str = "test-session-token";

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
            session_token: TEST_SESSION_TOKEN.to_string(),
            expires_at: chrono::DateTime::parse_from_rfc3339("2026-12-31T23:59:59Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            permissions: SharePermissions::Edit,
            upload_only: true,
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["session_token"], TEST_SESSION_TOKEN);
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
