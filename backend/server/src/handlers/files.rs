//! HTTP handlers for file operations.

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::{
    domain::{File, ThumbnailSize},
    services::ThumbnailError,
};

use super::{AppError, AuthenticatedUser};
use crate::AppState;

/// Hidden kanban metadata files that should never be exposed through file APIs.
fn is_hidden_kanban_file(name: &str) -> bool {
    matches!(
        name,
        ".rustshare-board.json"
            | ".rustshare-column.json"
            | ".rustshare-card.json"
            | "events.jsonl"
            | "index.md"
            | "__primary__.md"
    ) || name.ends_with(".editor.json")
}

// ============================================================================
// Task 15: File Upload
// ============================================================================

/// Upload a new file.
///
/// POST /api/files/upload
///
/// Accepts multipart/form-data with fields:
/// - file: the file content
/// - name: the file name
/// - parent_folder_id: optional parent folder UUID
#[utoipa::path(
    post,
    path = "/api/v1/files/upload",
    tag = "Files",
    responses(
        (status = 200, description = "File uploaded", body = FileUploadResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn upload_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<FileUploadResponse>), AppError> {
    let mut file_temp: Option<tempfile::NamedTempFile> = None;
    let mut file_name: Option<String> = None;
    let mut parent_folder_id: Option<Uuid> = None;

    // Parse multipart fields
    while let Some(mut field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Failed to read multipart field: {}", e);
        AppError::internal(format!("Failed to read multipart field: {}", e))
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "file" => {
                file_temp = Some(
                    super::stream_multipart_field_to_temp_file(
                        &mut field,
                        super::max_upload_size_bytes(),
                    )
                    .await?
                    .0,
                );
            }
            "name" => {
                file_name = Some(field.text().await.map_err(|e| {
                    tracing::error!("Failed to read name field: {}", e);
                    AppError::internal(format!("Failed to read name field: {}", e))
                })?);
            }
            "parent_folder_id" => {
                let text = field.text().await.map_err(|e| {
                    tracing::error!("Failed to read parent_folder_id field: {}", e);
                    AppError::internal(format!("Failed to read parent_folder_id field: {}", e))
                })?;
                parent_folder_id = Some(
                    Uuid::parse_str(&text)
                        .map_err(|_| AppError::bad_request("Invalid parent_folder_id"))?,
                );
            }
            _ => {}
        }
    }

    // Validate required fields
    let file_temp = file_temp.ok_or_else(|| AppError::bad_request("Missing file data"))?;
    let file_name = file_name.ok_or_else(|| AppError::bad_request("Missing file name"))?;

    // Validate file name length and content
    if file_name.trim().is_empty() {
        return Err(AppError::bad_request("File name must not be empty"));
    }
    if file_name.len() > 255 {
        return Err(AppError::bad_request(
            "File name must not exceed 255 characters",
        ));
    }
    if file_name.contains('\0')
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name.contains("..")
    {
        return Err(AppError::bad_request(
            "File name contains invalid characters",
        ));
    }
    if file_name.starts_with(".rustshare") || file_name == "index.editor.json" {
        return Err(AppError::bad_request(
            "File name is reserved for internal use",
        ));
    }

    // Detect MIME type from file extension
    let mime_type = mime_guess::from_path(&file_name)
        .first_or_octet_stream()
        .to_string();

    // Upload file using the streaming path so the temp file is never read
    // back into memory.
    let file_path = file_temp.path();
    let file = state
        .file_service
        .upload_file_from_path(
            auth.user_id,
            file_name,
            parent_folder_id,
            file_path,
            mime_type,
            auth.tenant_id,
        )
        .await?;

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
    ))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FileUploadResponse {
    pub id: Uuid,
    pub name: String,
    pub size: i64,
    pub mime_type: String,
    pub current_version: i32,
    pub created_at: String,
}

// ============================================================================
// Task 16: File Get/Download/Delete
// ============================================================================

/// Get file metadata.
///
/// GET /api/files/{id}
#[utoipa::path(
    get,
    path = "/api/v1/files/{id}",
    tag = "Files",
    params(("id" = Uuid, Path, description = "File ID")),
    responses(
        (status = 200, description = "File metadata", body = File),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "File not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Json<File>, AppError> {
    let file = state.file_service.get_file(file_id, auth.user_id).await?;
    if is_hidden_kanban_file(&file.name) {
        return Err(AppError::not_found(format!("File not found: {}", file_id)));
    }
    Ok(Json(file))
}

/// Download file content directly with proper filename.
///
/// GET /api/files/{id}/download
#[utoipa::path(
    get,
    path = "/api/v1/files/{id}/download",
    tag = "Files",
    params(("id" = Uuid, Path, description = "File ID")),
    responses(
        (status = 200, description = "File content"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "File not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn download_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Response, AppError> {
    stream_file_response(&state, auth.user_id, file_id, FileDisposition::Attachment).await
}

/// Download file content directly with proper filename.
///
/// GET /api/v1/files/{id}/content
///
/// Returns the file content with Content-Disposition header set to attachment
/// with the original filename, ensuring downloaded files have correct names.
#[utoipa::path(
    get,
    path = "/api/v1/files/{id}/content",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn download_file_content(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Response, AppError> {
    stream_file_response(&state, auth.user_id, file_id, FileDisposition::Attachment).await
}

enum FileDisposition {
    Attachment,
    Inline,
}

async fn stream_file_response(
    state: &AppState,
    user_id: Uuid,
    file_id: Uuid,
    disposition: FileDisposition,
) -> Result<Response, AppError> {
    // Get file metadata first (this also checks permissions)
    let file = state.file_service.get_file(file_id, user_id).await?;

    if is_hidden_kanban_file(&file.name) {
        return Err(AppError::not_found(format!("File not found: {}", file_id)));
    }

    // Stream the file content directly (avoids redirecting to internal storage URLs)
    let storage_key = file.storage_key();
    let (content_type, content_length, stream) = state
        .object_store
        .get_stream(&storage_key)
        .await
        .map_err(|e| {
            tracing::error!("Failed to read file content: {}", e);
            AppError::internal("Failed to read file content")
        })?;

    let content_disposition = match disposition {
        FileDisposition::Attachment => super::public_shares::build_content_disposition(&file.name),
        FileDisposition::Inline => "inline".to_string(),
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type.unwrap_or(file.mime_type))
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

/// Preview file content (inline disposition for browser viewing).
///
/// GET /api/v1/files/{id}/preview
///
/// Returns the file content with Content-Disposition set to inline
/// for browser preview (images, PDFs, videos, etc).
#[utoipa::path(
    get,
    path = "/api/v1/files/{id}/preview",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn preview_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Response, AppError> {
    stream_file_response(&state, auth.user_id, file_id, FileDisposition::Inline).await
}

/// Delete a file.
///
/// DELETE /api/files/{id}
#[utoipa::path(
    delete,
    path = "/api/v1/files/{id}",
    tag = "Files",
    params(("id" = Uuid, Path, description = "File ID")),
    responses(
        (status = 204, description = "File deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "File not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn delete_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .file_service
        .delete_file(file_id, auth.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Task 17: File Update with If-Match
// ============================================================================

/// Update file content with optimistic locking.
///
/// PUT /api/files/{id}
///
/// Requires If-Match header with expected version number.
/// Accepts multipart/form-data with field:
/// - file: the new file content
#[utoipa::path(
    put,
    path = "/api/v1/files/{id}",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    responses(
        (status = 200, description = "Success", body = FileUpdateResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<FileUpdateResponse>, AppError> {
    // Parse If-Match header
    let if_match = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("Missing If-Match header"))?;

    let expected_version: i32 = if_match
        .parse()
        .map_err(|_| AppError::bad_request("Invalid If-Match header: must be an integer"))?;

    // Extract file data from multipart, streaming to a temp file on disk.
    let mut file_temp: Option<tempfile::NamedTempFile> = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::internal(format!("Failed to read multipart field: {}", e)))?
    {
        if field.name() == Some("file") {
            file_temp = Some(
                super::stream_multipart_field_to_temp_file(
                    &mut field,
                    super::max_upload_size_bytes(),
                )
                .await?
                .0,
            );
            break;
        }
    }

    let file_temp = file_temp.ok_or_else(|| AppError::bad_request("Missing file data"))?;

    // Update file using the streaming path.
    let file = state
        .file_service
        .update_file_from_path(file_id, auth.user_id, expected_version, file_temp.path())
        .await?;

    Ok(Json(FileUpdateResponse {
        id: file.id,
        current_version: file.current_version,
        size: file.size,
        modified_at: file.modified_at.to_rfc3339(),
    }))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FileUpdateResponse {
    pub id: Uuid,
    pub current_version: i32,
    pub size: i64,
    pub modified_at: String,
}

// ============================================================================
// Task 18: File Version Endpoints
// ============================================================================

/// Get file version history.
///
/// GET /api/files/{id}/versions
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FileVersionResponse {
    pub id: Uuid,
    pub version_number: i32,
    pub size: i64,
    pub created_at: String,
    pub created_by_user_id: Uuid,
    pub change_description: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/files/{id}/versions",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    responses(
        (status = 200, description = "Success", body = Vec<FileVersionResponse>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_file_versions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Json<Vec<FileVersionResponse>>, AppError> {
    let versions = state
        .file_service
        .list_versions(file_id, auth.user_id)
        .await?;
    let response: Vec<FileVersionResponse> = versions
        .into_iter()
        .map(|v| FileVersionResponse {
            id: v.id,
            version_number: v.version_number,
            size: v.size,
            created_at: v.created_at.to_rfc3339(),
            created_by_user_id: v.created_by,
            change_description: v.change_description,
        })
        .collect();
    Ok(Json(response))
}

/// Restore a file to a previous version.
///
/// POST /api/files/{id}/restore
///
/// Request body: { "version": 3 }
#[utoipa::path(
    post,
    path = "/api/v1/files/{id}/restore",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    request_body = RestoreVersionRequest,
    responses(
        (status = 200, description = "Success", body = FileRestoreResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn restore_file_version(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<RestoreVersionRequest>,
) -> Result<Json<FileRestoreResponse>, AppError> {
    let file = state
        .file_service
        .restore_version(file_id, req.version, auth.user_id)
        .await?;

    Ok(Json(FileRestoreResponse {
        id: file.id,
        current_version: file.current_version,
        restored_from_version: req.version,
        size: file.size,
        modified_at: file.modified_at.to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RestoreVersionRequest {
    pub version: i32,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetFileColorRequest {
    pub color: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FileColorResponse {
    pub id: Uuid,
    pub color: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/api/v1/files/{id}/color",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    request_body = SetFileColorRequest,
    responses(
        (status = 200, description = "Color updated", body = FileColorResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn set_file_color(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<SetFileColorRequest>,
) -> Result<Json<FileColorResponse>, AppError> {
    // Verify the file exists and is accessible by the user
    let _file = state
        .file_service
        .get_file(file_id, auth.user_id)
        .await
        .map_err(|e| match e {
            rustshare_core::services::FileError::NotFound(_) => {
                AppError::not_found(format!("File not found: {}", file_id))
            }
            _ => AppError::internal(format!("Failed to get file: {}", e)),
        })?;

    sqlx::query("UPDATE files SET color = $1, modified_at = NOW() WHERE id = $2 AND owner_id = $3 AND tenant_id = $4")
        .bind(&req.color)
        .bind(file_id)
        .bind(auth.user_id)
        .bind(auth.tenant_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| AppError::internal(format!("Failed to set file color: {}", e)))?;

    Ok(Json(FileColorResponse {
        id: file_id,
        color: req.color,
    }))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FileRestoreResponse {
    pub id: Uuid,
    pub current_version: i32,
    pub restored_from_version: i32,
    pub size: i64,
    pub modified_at: String,
}

// ============================================================================
// Task 19: File Move/Rename
// ============================================================================

/// Move a file to a different folder.
///
/// POST /api/files/{id}/move
///
/// Request body: { "target_folder_id": "uuid" }
#[utoipa::path(
    post,
    path = "/api/v1/files/{id}/move",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    request_body = MoveFileRequest,
    responses(
        (status = 200, description = "Success", body = File),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn move_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<MoveFileRequest>,
) -> Result<Json<File>, AppError> {
    let file = state
        .file_service
        .move_file(file_id, req.target_folder_id, auth.user_id)
        .await?;

    Ok(Json(file))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MoveFileRequest {
    pub target_folder_id: Option<Uuid>,
}

/// Rename a file.
///
/// POST /api/files/{id}/rename
///
/// Request body: { "new_name": "document.pdf" }
#[utoipa::path(
    post,
    path = "/api/vault-sync/v1/vaults/{vault_id}/rename",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    request_body = RenameFileRequest,
    responses(
        (status = 200, description = "Success", body = File),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn rename_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<RenameFileRequest>,
) -> Result<Json<File>, AppError> {
    let file = state
        .file_service
        .rename_file(file_id, req.new_name, auth.user_id)
        .await?;

    Ok(Json(file))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RenameFileRequest {
    pub new_name: String,
}

// ============================================================================
// Thumbnail Endpoint
// ============================================================================

/// Maximum file size for thumbnail generation (100MB)
const MAX_THUMBNAIL_FILE_SIZE: i64 = 100 * 1024 * 1024;

/// Query parameters for thumbnail requests.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ThumbnailParams {
    /// Thumbnail size: sm (40x40), md (128x128), lg (256x256)
    /// Defaults to "md" if not specified
    pub size: Option<String>,
}

/// Get file thumbnail.
///
/// GET /api/v1/files/:id/thumbnail
///
/// Returns the thumbnail image for a file. If the thumbnail doesn't exist,
/// it will be generated on-demand for supported file types (images, PDFs, videos).
///
/// Query parameters:
/// - size: "sm" | "md" | "lg" (default: "md")
#[utoipa::path(
    get,
    path = "/api/v1/files/{id}/thumbnail",
    tag = "Files",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_file_thumbnail(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Query(params): Query<ThumbnailParams>,
) -> Result<Response, AppError> {
    // First, verify the user has access to the file
    let file = state.file_service.get_file(file_id, user_id).await?;

    // Check file size - don't generate thumbnails for files larger than 100MB
    if file.size > MAX_THUMBNAIL_FILE_SIZE {
        return Err(AppError::payload_too_large(format!(
            "File size {} exceeds maximum allowed {} bytes",
            file.size, MAX_THUMBNAIL_FILE_SIZE
        )));
    }

    // Parse size parameter (default to "md")
    let size_str = params.size.as_deref().unwrap_or("md");
    let size = ThumbnailSize::try_from(size_str).map_err(|_| {
        AppError::bad_request(format!(
            "Invalid size parameter: {}. Use 'sm', 'md', or 'lg'",
            size_str
        ))
    })?;

    // Check if thumbnail exists
    let thumbnail = match state.thumbnail_service.get_thumbnail(file_id, size).await {
        Ok(Some(thumbnail)) => thumbnail,
        Ok(None) => {
            // Thumbnail doesn't exist, try to generate it
            state
                .thumbnail_service
                .generate_thumbnail(file_id, &file.mime_type, &file.name, size)
                .await
                .map_err(|e| match e {
                    ThumbnailError::NotFound => AppError::not_found("File not found"),
                    ThumbnailError::UnsupportedType => AppError::unsupported_media_type(
                        "Thumbnail generation not supported for this file type",
                    ),
                    _ => {
                        tracing::error!("Thumbnail service error: {}", e);
                        AppError::internal("Failed to generate thumbnail")
                    }
                })?
        }
        Err(e) => {
            tracing::error!("Failed to get thumbnail: {}", e);
            return Err(match e {
                ThumbnailError::NotFound => AppError::not_found("File not found"),
                ThumbnailError::UnsupportedType => AppError::unsupported_media_type(
                    "Thumbnail generation not supported for this file type",
                ),
                _ => {
                    tracing::error!("Thumbnail service error: {}", e);
                    AppError::internal("Failed to generate thumbnail")
                }
            });
        }
    };

    // Get thumbnail data from storage
    let thumbnail_data = state
        .thumbnail_service
        .get_thumbnail_data(&thumbnail.storage_path)
        .await
        .map_err(|e| match e {
            ThumbnailError::NotFound => AppError::not_found("File not found"),
            ThumbnailError::UnsupportedType => AppError::unsupported_media_type(
                "Thumbnail generation not supported for this file type",
            ),
            _ => {
                tracing::error!("Thumbnail service error: {}", e);
                AppError::internal("Failed to generate thumbnail")
            }
        })?;

    // Build response with cache headers
    // Thumbnails are immutable once generated
    let etag = format!("{}-{}", file_id, size_str);
    let headers = [
        (header::CONTENT_TYPE, thumbnail.content_type.as_str()),
        (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        (header::ETAG, etag.as_str()),
    ];

    let response = (StatusCode::OK, headers, thumbnail_data).into_response();

    Ok(response)
}

// ============================================================================
// Task: File Edit with Content
// ============================================================================

/// Edit file content with save mode option.
///
/// POST /api/v1/files/{id}/edit
///
/// Request body: JSON with base64-encoded content and save mode
/// {
///   "content": "base64-encoded-content",
///   "save_mode": "overwrite" | "new_version",
///   "change_description": "optional description"
/// }
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct EditFileRequest {
    /// Base64-encoded file content
    pub content: String,
    /// Save mode: "overwrite" or "new_version"
    pub save_mode: String,
    /// Optional change description for new versions
    pub change_description: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EditFileResponse {
    pub id: Uuid,
    pub current_version: i32,
    pub size: i64,
    pub modified_at: String,
    pub saved_as_new_version: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/files/{id}/edit",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    request_body = EditFileRequest,
    responses(
        (status = 200, description = "Success", body = EditFileResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn edit_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<EditFileRequest>,
) -> Result<Json<EditFileResponse>, AppError> {
    // Validate save mode
    if req.save_mode != "overwrite" && req.save_mode != "new_version" {
        return Err(AppError::bad_request(
            "Invalid save_mode. Must be 'overwrite' or 'new_version'",
        ));
    }

    // Decode base64 content
    let content = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &req.content)
        .map_err(|e| {
            tracing::error!("Failed to decode base64 content: {}", e);
            AppError::bad_request("Invalid base64 content")
        })?;
    let content = Bytes::from(content);

    // Edit file
    let file = state
        .file_service
        .edit_file(
            file_id,
            auth.user_id,
            content,
            &req.save_mode,
            req.change_description,
        )
        .await?;

    Ok(Json(EditFileResponse {
        id: file.id,
        current_version: file.current_version,
        size: file.size,
        modified_at: file.modified_at.to_rfc3339(),
        saved_as_new_version: req.save_mode == "new_version",
    }))
}

// ============================================================================
// List All User Files (Simple View)
// ============================================================================

// ============================================================================
// Share Indicator Types
// ============================================================================

/// File with share information for list responses
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct FileWithShares {
    // File fields
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub mime_type: String,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub current_version: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub modified_at: chrono::DateTime<chrono::Utc>,
    pub starred_at: Option<chrono::DateTime<chrono::Utc>>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    // Share info
    pub is_shared: bool,
    pub share_count: i64,
    /// Earliest share expiration date (None if no shares have expiration)
    pub share_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_permission: Option<String>,
}

/// List all files for the current user.
///
/// GET /api/files
///
/// Returns a simple flat list of all files owned by the user with share indicators.
#[utoipa::path(
    get,
    path = "/api/v1/files",
    tag = "Files",
    responses(
        (status = 200, description = "Success", body = Vec<FileWithShares>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_files(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<super::PaginationQuery>,
) -> Result<Json<Vec<FileWithShares>>, AppError> {
    // Query all files with share information
    let files = sqlx::query_as::<_, FileWithShares>(
        r#"
        SELECT
            f.id, f.name, f.path, f.size, f.mime_type,
            f.parent_folder_id, f.owner_id, f.current_version,
            f.created_at, f.modified_at, f.starred_at, f.deleted_at,
            f.color,
            EXISTS(
                SELECT 1 FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
            ) as is_shared,
            (
                SELECT COUNT(*) FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
            ) as share_count,
            (
                SELECT MIN(expires_at) FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
                AND expires_at IS NOT NULL
            ) as share_expires_at,
            'Admin'::TEXT as effective_permission
        FROM files f
        WHERE f.owner_id = $1
          AND f.tenant_id = $2
          AND f.deleted_at IS NULL
          AND f.name NOT LIKE '.rustshare-%'
          AND f.name NOT IN ('index.md', '__primary__.md')
          AND f.name NOT LIKE '%.editor.json'
        ORDER BY f.created_at DESC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .bind(query.limit())
    .bind(query.offset())
    .fetch_all(&state.db_pool)
    .await?;

    Ok(Json(files))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct WorkspaceStarRequest {
    pub starred: bool,
}

#[utoipa::path(
    patch,
    path = "/api/v1/files/{id}/star",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    request_body = WorkspaceStarRequest,
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn toggle_file_star(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<WorkspaceStarRequest>,
) -> Result<StatusCode, AppError> {
    let updated = state
        .metadata_store
        .set_file_starred(file_id, auth.user_id, req.starred)
        .await
        .map_err(|e| AppError::internal(format!("Failed to update star state: {}", e)))?;

    if !updated {
        return Err(AppError::not_found(format!("File not found: {}", file_id)));
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/files/{id}/restore-from-trash",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn restore_file_from_trash(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let restored = state
        .metadata_store
        .restore_file(file_id, auth.user_id, auth.tenant_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to restore file: {}", e)))?;

    if !restored {
        return Err(AppError::not_found(format!("File not found: {}", file_id)));
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/files/{id}/permanent",
    tag = "Files",
    params(("file_id" = Uuid, Path, description = "File Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn permanently_delete_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let deleted = state
        .metadata_store
        .permanently_delete_file(file_id, auth.user_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to permanently delete file: {}", e)))?;

    if !deleted {
        return Err(AppError::not_found(format!("File not found: {}", file_id)));
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/files/starred",
    tag = "Files",
    responses(
        (status = 200, description = "Success", body = crate::handlers::folders::FolderContentsWithShares),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_starred_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<super::PaginationQuery>,
) -> Result<Json<crate::handlers::folders::FolderContentsWithShares>, AppError> {
    let folders = sqlx::query_as::<_, crate::handlers::folders::FolderWithShares>(
        r#"
        WITH RECURSIVE folder_tree AS (
            SELECT id, parent_folder_id, id as root_id
            FROM folders
            WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NULL AND starred_at IS NOT NULL
            UNION ALL
            SELECT child.id, child.parent_folder_id, parent.root_id
            FROM folders child
            INNER JOIN folder_tree parent ON child.parent_folder_id = parent.id
            WHERE child.deleted_at IS NULL
        ),
        folder_sizes AS (
            SELECT ft.root_id, COALESCE(SUM(files.size), 0)::bigint as total_size
            FROM folder_tree ft
            LEFT JOIN files ON files.parent_folder_id = ft.id AND files.deleted_at IS NULL
            GROUP BY ft.root_id
        )
        SELECT
            f.id, f.name, f.path, f.parent_folder_id, f.owner_id,
            f.created_at, f.updated_at, f.starred_at, f.deleted_at,
            COALESCE(fs.total_size, 0) as size,
            EXISTS(
                SELECT 1 FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
            ) as is_shared,
            (
                SELECT COUNT(*) FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
            ) as share_count,
            (
                SELECT MIN(expires_at) FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
                AND expires_at IS NOT NULL
            ) as share_expires_at,
            'Admin'::TEXT as effective_permission,
            (
                SELECT fi.id
                FROM files fi
                WHERE fi.parent_folder_id = f.id AND fi.name = 'note.md' AND fi.deleted_at IS NULL
                LIMIT 1
            ) as note_bundle_file_id
        FROM folders f
        LEFT JOIN folder_sizes fs ON fs.root_id = f.id
        WHERE f.owner_id = $1
          AND f.tenant_id = $2
          AND f.deleted_at IS NULL
          AND f.starred_at IS NOT NULL
        ORDER BY f.starred_at DESC NULLS LAST, f.name ASC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .bind(query.limit())
    .bind(query.offset())
    .fetch_all(&state.db_pool)
    .await?;

    let files = sqlx::query_as::<_, FileWithShares>(
        r#"
        SELECT
            f.id, f.name, f.path, f.size, f.mime_type,
            f.parent_folder_id, f.owner_id, f.current_version,
            f.created_at, f.modified_at, f.starred_at, f.deleted_at,
            f.color,
            EXISTS(
                SELECT 1 FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
            ) as is_shared,
            (
                SELECT COUNT(*) FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
            ) as share_count,
            (
                SELECT MIN(expires_at) FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
                AND expires_at IS NOT NULL
            ) as share_expires_at,
            'Admin'::TEXT as effective_permission
        FROM files f
        WHERE f.owner_id = $1
          AND f.tenant_id = $2
          AND f.deleted_at IS NULL
          AND f.starred_at IS NOT NULL
        ORDER BY f.starred_at DESC NULLS LAST, f.name ASC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .bind(query.limit())
    .bind(query.offset())
    .fetch_all(&state.db_pool)
    .await?;

    Ok(Json(crate::handlers::folders::FolderContentsWithShares {
        folders,
        files,
        current_folder_permission: None,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/files/deleted",
    tag = "Files",
    responses(
        (status = 200, description = "Success", body = crate::handlers::folders::FolderContentsWithShares),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_deleted_items(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<super::PaginationQuery>,
) -> Result<Json<crate::handlers::folders::FolderContentsWithShares>, AppError> {
    let folders = sqlx::query_as::<_, crate::handlers::folders::FolderWithShares>(
        r#"
        WITH RECURSIVE folder_tree AS (
            SELECT id, parent_folder_id, id as root_id
            FROM folders
            WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL
            UNION ALL
            SELECT child.id, child.parent_folder_id, parent.root_id
            FROM folders child
            INNER JOIN folder_tree parent ON child.parent_folder_id = parent.id
        ),
        folder_sizes AS (
            SELECT ft.root_id, COALESCE(SUM(files.size), 0)::bigint as total_size
            FROM folder_tree ft
            LEFT JOIN files ON files.parent_folder_id = ft.id
            GROUP BY ft.root_id
        )
        SELECT
            f.id, f.name, f.path, f.parent_folder_id, f.owner_id,
            f.created_at, f.updated_at, f.starred_at, f.deleted_at,
            COALESCE(fs.total_size, 0) as size,
            EXISTS(
                SELECT 1 FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
            ) as is_shared,
            (
                SELECT COUNT(*) FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
            ) as share_count,
            (
                SELECT MIN(expires_at) FROM shares
                WHERE folder_id = f.id
                AND revoked_at IS NULL
                AND expires_at IS NOT NULL
            ) as share_expires_at,
            'Admin'::TEXT as effective_permission,
            (
                SELECT fi.id
                FROM files fi
                WHERE fi.parent_folder_id = f.id AND fi.name = 'note.md' AND fi.deleted_at IS NULL
                LIMIT 1
            ) as note_bundle_file_id
        FROM folders f
        LEFT JOIN folder_sizes fs ON fs.root_id = f.id
        WHERE f.owner_id = $1
          AND f.tenant_id = $2
          AND f.deleted_at IS NOT NULL
        ORDER BY f.deleted_at DESC NULLS LAST, f.name ASC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .bind(query.limit())
    .bind(query.offset())
    .fetch_all(&state.db_pool)
    .await?;

    let files = sqlx::query_as::<_, FileWithShares>(
        r#"
        SELECT
            f.id, f.name, f.path, f.size, f.mime_type,
            f.parent_folder_id, f.owner_id, f.current_version,
            f.created_at, f.modified_at, f.starred_at, f.deleted_at,
            f.color,
            EXISTS(
                SELECT 1 FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
            ) as is_shared,
            (
                SELECT COUNT(*) FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
            ) as share_count,
            (
                SELECT MIN(expires_at) FROM shares
                WHERE file_id = f.id
                AND revoked_at IS NULL
                AND expires_at IS NOT NULL
            ) as share_expires_at,
            'Admin'::TEXT as effective_permission
        FROM files f
        WHERE f.owner_id = $1
          AND f.tenant_id = $2
          AND f.deleted_at IS NOT NULL
        ORDER BY f.deleted_at DESC NULLS LAST, f.name ASC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(auth.user_id)
    .bind(auth.tenant_id)
    .bind(query.limit())
    .bind(query.offset())
    .fetch_all(&state.db_pool)
    .await?;

    Ok(Json(crate::handlers::folders::FolderContentsWithShares {
        folders,
        files,
        current_folder_permission: None,
    }))
}
