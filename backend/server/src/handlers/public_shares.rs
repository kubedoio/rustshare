//! HTTP handlers for public share access operations.
//!
//! This module provides anonymous access to shared files via share tokens.
//! It includes session creation with password validation and file download.

use axum::{
    body::Body,
    extract::{ConnectInfo, Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
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

/// Header used to convey the tenant context for unauthenticated public-share
/// requests. Public share links do not encode tenant in the token. The tenant
/// is normally derived from the globally unique token; callers may supply this
/// header as defense-in-depth, and requests for the wrong tenant are rejected
/// with `ShareNotFoundByToken`.
pub const PUBLIC_SHARE_TENANT_HEADER: &str = "X-Tenant-ID";

/// Extract an optional tenant ID from the public-share tenant header.
///
/// A missing header or a nil UUID is treated as "derive tenant from the
/// token". A syntactically invalid value is rejected with a 400 Bad Request.
fn extract_public_tenant_id(headers: &HeaderMap) -> Result<Option<Uuid>, AppError> {
    let Some(header) = headers.get(PUBLIC_SHARE_TENANT_HEADER) else {
        return Ok(None);
    };
    let value = header
        .to_str()
        .map_err(|_| AppError::bad_request("Invalid X-Tenant-ID header"))?;
    let tenant_id =
        Uuid::parse_str(value).map_err(|_| AppError::bad_request("Invalid X-Tenant-ID header"))?;
    if tenant_id.is_nil() {
        Ok(None)
    } else {
        Ok(Some(tenant_id))
    }
}

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

/// Build a safe `Content-Disposition` header value for a file download.
///
/// Sanitizes the legacy `filename` parameter by escaping quotes and backslashes
/// and stripping control characters (including `\n`, `\r`, and `\x7f`). The
/// RFC 5987 `filename*` parameter is kept and uses percent-encoding so Unicode
/// names round-trip safely for supporting clients.
fn sanitize_legacy_filename(file_name: &str) -> String {
    let mut sanitized = String::with_capacity(file_name.len());
    for ch in file_name.chars() {
        match ch {
            '"' => sanitized.push_str("\\\""),
            '\\' => sanitized.push_str("\\\\"),
            c if c.is_control() => {
                // Drop control characters to prevent header injection.
            }
            c => sanitized.push(c),
        }
    }
    sanitized
}

pub fn build_content_disposition(file_name: &str) -> String {
    format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        sanitize_legacy_filename(file_name),
        percent_encoding::percent_encode(file_name.as_bytes(), &percent_encoding::NON_ALPHANUMERIC)
            .to_string()
    )
}

/// Create anonymous session for share access
#[utoipa::path(
    post,
    path = "/api/v1/public/share/{token}/session",
    tag = "Public Shares",
    params(
        ("X-Tenant-ID" = Option<Uuid>, Header, description = "Optional tenant identifier for the public share link"),
        ("token" = String, Path, description = "Share token"),
    ),
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
    headers: HeaderMap,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Response, AppError> {
    let tenant_id = extract_public_tenant_id(&headers)?;
    let session = state
        .share_service
        .validate_and_create_session(&token, req.password, tenant_id)
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
    params(
        ("X-Tenant-ID" = Option<Uuid>, Header, description = "Optional tenant identifier for the public share link"),
        ("token" = String, Path, description = "Share token"),
    ),
    responses(
        (status = 200, description = "Share information", body = ShareInfoResponse),
        (status = 404, description = "Share not found or revoked", body = crate::handlers::ErrorResponse),
        (status = 410, description = "Share expired or revoked", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_share_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let tenant_id = extract_public_tenant_id(&headers)?;
    let (share, file, folder) = state
        .share_service
        .get_public_share_info(&token, tenant_id)
        .await?;

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
    } else if share.password_hash.is_some() {
        Ok(Json(ShareInfoResponse {
            resource_id: share.id,
            resource_type: "protected".to_string(),
            name: "Protected share".to_string(),
            permissions: share.permissions,
            upload_only: share.upload_only,
            file_size: None,
            mime_type: None,
            password_protected: true,
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

/// Resolve the effective tenant for a public-share session.
///
/// Legacy share session JWTs may have `tenant_id` set to nil. When that
/// happens the tenant is derived from the share itself. If the JWT contains a
/// non-nil tenant, it must match the share's actual tenant (defense in depth).
fn resolve_public_share_tenant(
    token: &str,
    claims_tenant_id: Uuid,
    share_tenant_id: Uuid,
) -> Result<Uuid, AppError> {
    if claims_tenant_id.is_nil() || claims_tenant_id == share_tenant_id {
        Ok(share_tenant_id)
    } else {
        Err(AppError::from(
            rustshare_core::services::ShareError::ShareNotFoundByToken(token.to_string()),
        ))
    }
}

/// Load a public share for a session-authenticated request and resolve the
/// effective tenant.
async fn resolve_share_for_public_session(
    state: &AppState,
    token: &str,
    claims: &rustshare_auth::ShareSessionClaims,
) -> Result<(rustshare_core::domain::Share, Uuid), AppError> {
    let share = state
        .metadata_store
        .get_share_by_token_unscoped(token)
        .await
        .map_err(|e| AppError::internal(format!("Database error: {}", e)))?
        .ok_or_else(|| {
            AppError::from(rustshare_core::services::ShareError::ShareNotFoundByToken(
                token.to_string(),
            ))
        })?;

    ensure_share_session_matches(&share, claims)?;
    ensure_share_is_active(&share)?;

    let effective_tenant = resolve_public_share_tenant(token, claims.tenant_id, share.tenant_id)?;

    Ok((share, effective_tenant))
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
                    super::stream_multipart_field_to_temp_file(&mut field, MAX_PUBLIC_UPLOAD_SIZE)
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
    // Get share to verify token matches. Legacy tokens may have a nil tenant,
    // so derive the effective tenant from the share itself.
    let (share, _effective_tenant) =
        resolve_share_for_public_session(&state, &token, &claims).await?;
    // `_effective_tenant` is not needed for the download itself (the file is
    // looked up by ID and owner), but resolving it validates the JWT against
    // the share and ensures legacy nil-tenant tokens still resolve correctly.

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

    // Stream file content from storage without loading it into memory.
    let storage_key = file.storage_key();
    let (content_type, content_length, stream) = state
        .object_store
        .get_stream(&storage_key)
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

    // Return file with appropriate headers. Only set Content-Length when the
    // object store reports one; falling back to the metadata size could send a
    // stale value if the stored object has changed.
    let content_disposition = build_content_disposition(&file.name);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type.unwrap_or_else(|| file.mime_type.clone()))
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition)
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    if let Some(len) = content_length {
        headers.insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(&len.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("0")),
        );
    }

    Ok((StatusCode::OK, headers, Body::from_stream(stream)).into_response())
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
    // Get share to verify token matches, deriving the tenant from the share for
    // legacy tokens with a nil tenant_id.
    let (share, effective_tenant) =
        resolve_share_for_public_session(&state, &token, &claims).await?;

    if share.folder_id.is_none() {
        return Err(AppError::bad_request("This share is not for a folder"));
    }

    if share.upload_only {
        return Err(AppError::forbidden("This share is upload-only"));
    }

    let (_share, current_folder, folders, files) = state
        .share_service
        .list_public_folder_contents(&token, query.folder_id, Some(effective_tenant))
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
    // Get share to verify token matches, deriving the tenant from the share for
    // legacy tokens with a nil tenant_id.
    let (share, _effective_tenant) =
        resolve_share_for_public_session(&state, &token, &claims).await?;
    // `_effective_tenant` is not needed for the download itself (access is
    // verified by checking the file is inside the shared folder tree), but
    // resolving it validates the JWT and supports legacy nil-tenant tokens.

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

    let storage_key = file.storage_key();
    let (content_type, content_length, stream) = state
        .object_store
        .get_stream(&storage_key)
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

    let content_disposition = build_content_disposition(&file.name);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type.unwrap_or_else(|| file.mime_type.clone()))
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition)
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    if let Some(len) = content_length {
        headers.insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(&len.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("0")),
        );
    }

    Ok((StatusCode::OK, headers, Body::from_stream(stream)).into_response())
}

/// Maximum file size for public share uploads (100 MB).
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
    // Get share to verify token matches, deriving the tenant from the share for
    // legacy tokens with a nil tenant_id.
    let (share, effective_tenant) =
        resolve_share_for_public_session(&state, &token, &claims).await?;

    let root_folder_id = share
        .folder_id
        .ok_or_else(|| AppError::bad_request("This share is not for a folder"))?;

    if !share.upload_only && share.permissions < SharePermissions::Edit {
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
            effective_tenant,
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

    #[test]
    fn public_share_download_header_escaped() {
        let file_name = "report\".txt";
        let header = build_content_disposition(file_name);

        assert!(
            header.contains("filename=\"report\\\".txt\""),
            "legacy filename parameter must escape embedded quotes: {}",
            header
        );
        assert!(
            header.contains("filename*=UTF-8''report%22.txt"),
            "RFC 5987 filename* parameter must URL-encode embedded quotes: {}",
            header
        );
    }

    #[test]
    fn public_share_download_header_newline_sanitized() {
        let file_name = "line\nfeed.txt";
        let header = build_content_disposition(file_name);

        assert!(
            !header.contains("\n"),
            "legacy filename must not contain raw newline: {}",
            header
        );
        assert!(
            header.contains("filename=\"linefeed.txt\""),
            "legacy filename must strip newline: {}",
            header
        );
        assert!(
            header.contains("filename*=UTF-8''line%0Afeed.txt"),
            "RFC 5987 filename* must percent-encode newline: {}",
            header
        );
    }

    #[test]
    fn public_share_download_header_carriage_return_sanitized() {
        let file_name = "car\rriage.txt";
        let header = build_content_disposition(file_name);

        assert!(
            !header.contains("\r"),
            "legacy filename must not contain raw carriage return: {}",
            header
        );
        assert!(
            header.contains("filename=\"carriage.txt\""),
            "legacy filename must strip carriage return: {}",
            header
        );
        assert!(
            header.contains("filename*=UTF-8''car%0Driage.txt"),
            "RFC 5987 filename* must percent-encode carriage return: {}",
            header
        );
    }

    #[test]
    fn public_share_download_header_backslash_escaped() {
        let file_name = "path\\to\\file.txt";
        let header = build_content_disposition(file_name);

        assert!(
            header.contains("filename=\"path\\\\to\\\\file.txt\""),
            "legacy filename parameter must escape backslashes: {}",
            header
        );
        assert!(
            header.contains("filename*=UTF-8''path%5Cto%5Cfile.txt"),
            "RFC 5987 filename* must percent-encode backslashes: {}",
            header
        );
    }

    #[test]
    fn public_share_download_header_control_chars_sanitized() {
        let file_name = "foo\x01bar\x7fbaz.txt";
        let header = build_content_disposition(file_name);

        assert!(
            !header.bytes().any(|b| b < 0x20 || b == 0x7f),
            "legacy filename must not contain control characters: {}",
            header
        );
        assert!(
            header.contains("filename=\"foobarbaz.txt\""),
            "legacy filename must strip all control characters: {}",
            header
        );
        assert!(
            header.contains("filename*=UTF-8''foo%01bar%7Fbaz.txt"),
            "RFC 5987 filename* must percent-encode control characters: {}",
            header
        );
    }

    #[test]
    fn public_share_download_header_unicode_preserved() {
        let file_name = "我的報告 \"v2\".pdf";
        let header = build_content_disposition(file_name);

        assert!(
            header.contains("filename=\"我的報告 \\\"v2\\\".pdf\""),
            "legacy filename must escape quotes while preserving unicode: {}",
            header
        );
        assert!(
            header.contains("filename*=UTF-8''%E6%88%91%E7%9A%84%E5%A0%B1%E5%91%8A%20%22v2%22.pdf"),
            "RFC 5987 filename* must percent-encode unicode: {}",
            header
        );
    }

    #[test]
    fn extract_public_tenant_id_missing_returns_none() {
        let headers = HeaderMap::new();
        let result = extract_public_tenant_id(&headers);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn extract_public_tenant_id_invalid_returns_bad_request() {
        let mut headers = HeaderMap::new();
        headers.insert(PUBLIC_SHARE_TENANT_HEADER, "not-a-uuid".parse().unwrap());
        let result = extract_public_tenant_id(&headers);
        assert!(matches!(result.unwrap_err(), AppError::BadRequest(_)));
    }

    #[test]
    fn extract_public_tenant_id_nil_returns_none() {
        let mut headers = HeaderMap::new();
        headers.insert(
            PUBLIC_SHARE_TENANT_HEADER,
            Uuid::nil().to_string().parse().unwrap(),
        );
        let result = extract_public_tenant_id(&headers);
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn extract_public_tenant_id_matching_tenant_returns_some() {
        let tenant_id = Uuid::new_v4();
        let mut headers = HeaderMap::new();
        headers.insert(
            PUBLIC_SHARE_TENANT_HEADER,
            tenant_id.to_string().parse().unwrap(),
        );
        let result = extract_public_tenant_id(&headers);
        assert_eq!(result.unwrap(), Some(tenant_id));
    }

    #[test]
    fn extract_public_tenant_id_mismatched_tenant_value_returns_some() {
        // The extractor itself does not compare against the share; it just
        // parses the header. The downstream service rejects a mismatch.
        let other_tenant_id = Uuid::new_v4();
        let mut headers = HeaderMap::new();
        headers.insert(
            PUBLIC_SHARE_TENANT_HEADER,
            other_tenant_id.to_string().parse().unwrap(),
        );
        let result = extract_public_tenant_id(&headers);
        assert_eq!(result.unwrap(), Some(other_tenant_id));
    }

    #[test]
    fn resolve_public_share_tenant_nil_claims_uses_share_tenant() {
        let share_tenant_id = Uuid::new_v4();
        let result = resolve_public_share_tenant("token", Uuid::nil(), share_tenant_id);
        assert_eq!(result.unwrap(), share_tenant_id);
    }

    #[test]
    fn resolve_public_share_tenant_matching_claims_succeeds() {
        let share_tenant_id = Uuid::new_v4();
        let result = resolve_public_share_tenant("token", share_tenant_id, share_tenant_id);
        assert_eq!(result.unwrap(), share_tenant_id);
    }

    #[test]
    fn resolve_public_share_tenant_mismatched_claims_fails() {
        let share_tenant_id = Uuid::new_v4();
        let other_tenant_id = Uuid::new_v4();
        let result = resolve_public_share_tenant("token", other_tenant_id, share_tenant_id);
        assert!(matches!(
            result.unwrap_err(),
            AppError::NotFound(_) | AppError::BadRequest(_)
        ));
    }
}
