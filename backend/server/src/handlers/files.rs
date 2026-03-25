//! HTTP handlers for file operations.

use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::{
    domain::{File, FileVersion, ThumbnailSize},
    services::{FileError, ThumbnailError},
};

use super::{file_error_response, AuthenticatedUser, ErrorResponse};
use crate::AppState;

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
pub async fn upload_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<FileUploadResponse>), Response> {
    let mut file_data: Option<Bytes> = None;
    let mut file_name: Option<String> = None;
    let mut parent_folder_id: Option<Uuid> = None;

    // Parse multipart fields
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Failed to read multipart field: {}", e);
        file_error_response(FileError::Storage(format!(
            "Failed to read multipart field: {}",
            e
        )))
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "file" => {
                file_data = Some(field.bytes().await.map_err(|e| {
                    tracing::error!("Failed to read file data: {}", e);
                    file_error_response(FileError::Storage(format!(
                        "Failed to read file data: {}",
                        e
                    )))
                })?);
            }
            "name" => {
                file_name = Some(field.text().await.map_err(|e| {
                    tracing::error!("Failed to read name field: {}", e);
                    file_error_response(FileError::Storage(format!(
                        "Failed to read name field: {}",
                        e
                    )))
                })?);
            }
            "parent_folder_id" => {
                let text = field.text().await.map_err(|e| {
                    tracing::error!("Failed to read parent_folder_id field: {}", e);
                    file_error_response(FileError::Storage(format!(
                        "Failed to read parent_folder_id field: {}",
                        e
                    )))
                })?;
                parent_folder_id = Some(Uuid::parse_str(&text).map_err(|_| {
                    file_error_response(FileError::InvalidName(
                        "Invalid parent_folder_id".to_string(),
                    ))
                })?);
            }
            _ => {}
        }
    }

    // Validate required fields
    let file_data = file_data.ok_or_else(|| {
        file_error_response(FileError::InvalidName("Missing file data".to_string()))
    })?;
    let file_name = file_name.ok_or_else(|| {
        file_error_response(FileError::InvalidName("Missing file name".to_string()))
    })?;

    // Detect MIME type (simple version - can be enhanced)
    let mime_type = "application/octet-stream".to_string(); // TODO: Implement proper MIME detection

    // Upload file
    let file = state
        .file_service
        .upload_file(
            auth.user_id,
            file_name,
            parent_folder_id,
            file_data,
            mime_type,
        )
        .await
        .map_err(file_error_response)?;

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
    ))
}

#[derive(Debug, Serialize)]
pub struct FileUploadResponse {
    pub id: Uuid,
    pub name: String,
    pub size: i64,
    pub mime_type: String,
    pub content_hash: String,
    pub current_version: i32,
    pub created_at: String,
}

// ============================================================================
// Task 16: File Get/Download/Delete
// ============================================================================

/// Get file metadata.
///
/// GET /api/files/{id}
pub async fn get_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Json<File>, Response> {
    let file = state
        .file_service
        .get_file(file_id, auth.user_id)
        .await
        .map_err(file_error_response)?;
    Ok(Json(file))
}

/// Get file download URL.
///
/// GET /api/files/{id}/download
pub async fn download_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Json<DownloadUrlResponse>, Response> {
    let url = state
        .file_service
        .get_download_url(file_id, auth.user_id)
        .await
        .map_err(file_error_response)?;
    Ok(Json(DownloadUrlResponse { url }))
}

#[derive(Debug, Serialize)]
pub struct DownloadUrlResponse {
    pub url: String,
}

/// Download file content directly with proper filename.
///
/// GET /api/v1/files/{id}/content
///
/// Returns the file content with Content-Disposition header set to attachment
/// with the original filename, ensuring downloaded files have correct names.
pub async fn download_file_content(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Response {
    // Get file metadata first (this also checks permissions)
    let file = match state.file_service.get_file(file_id, auth.user_id).await {
        Ok(file) => file,
        Err(e) => return file_error_response(e).into_response(),
    };

    // Get file content from storage
    let storage_key = file.storage_key();
    let content = match state.object_store.get(&storage_key).await {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("Failed to get file content from storage: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to retrieve file content")),
            )
                .into_response();
        }
    };

    // Build Content-Disposition header with original filename
    // Use RFC 5987 encoding for non-ASCII characters
    let filename = &file.name;
    let content_disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        filename.replace('"', "\\\""),
        urlencoding::encode(filename)
    );

    // Build response with proper headers
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_DISPOSITION,
        content_disposition.parse().unwrap(),
    );
    
    // Set Content-Type based on file's MIME type
    headers.insert(
        header::CONTENT_TYPE,
        file.mime_type.parse().unwrap_or_else(|_| "application/octet-stream".parse().unwrap()),
    );

    (StatusCode::OK, headers, content).into_response()
}

/// Delete a file.
///
/// DELETE /api/files/{id}
pub async fn delete_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<StatusCode, Response> {
    state
        .file_service
        .delete_file(file_id, auth.user_id)
        .await
        .map_err(file_error_response)?;

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
pub async fn update_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<FileUpdateResponse>, Response> {
    // Parse If-Match header
    let if_match = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            file_error_response(FileError::InvalidName(
                "Missing If-Match header".to_string(),
            ))
        })?;

    let expected_version: i32 = if_match.parse().map_err(|_| {
        file_error_response(FileError::InvalidName(
            "Invalid If-Match header: must be an integer".to_string(),
        ))
    })?;

    // Extract file data from multipart
    let mut file_data: Option<Bytes> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        file_error_response(FileError::Storage(format!(
            "Failed to read multipart field: {}",
            e
        )))
    })? {
        if field.name() == Some("file") {
            file_data = Some(field.bytes().await.map_err(|e| {
                file_error_response(FileError::Storage(format!(
                    "Failed to read file data: {}",
                    e
                )))
            })?);
            break;
        }
    }

    let file_data = file_data.ok_or_else(|| {
        file_error_response(FileError::InvalidName("Missing file data".to_string()))
    })?;

    // Update file
    let file = state
        .file_service
        .update_file(file_id, auth.user_id, expected_version, file_data)
        .await
        .map_err(file_error_response)?;

    Ok(Json(FileUpdateResponse {
        id: file.id,
        current_version: file.current_version,
        content_hash: file.content_hash,
        size: file.size,
        modified_at: file.modified_at.to_rfc3339(),
    }))
}

#[derive(Debug, Serialize)]
pub struct FileUpdateResponse {
    pub id: Uuid,
    pub current_version: i32,
    pub content_hash: String,
    pub size: i64,
    pub modified_at: String,
}

// ============================================================================
// Task 18: File Version Endpoints
// ============================================================================

/// Get file version history.
///
/// GET /api/files/{id}/versions
pub async fn get_file_versions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Json<Vec<FileVersion>>, Response> {
    let versions = state
        .file_service
        .list_versions(file_id, auth.user_id)
        .await
        .map_err(file_error_response)?;
    Ok(Json(versions))
}

/// Restore a file to a previous version.
///
/// POST /api/files/{id}/restore
///
/// Request body: { "version": 3 }
pub async fn restore_file_version(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<RestoreVersionRequest>,
) -> Result<Json<FileRestoreResponse>, Response> {
    let file = state
        .file_service
        .restore_version(file_id, req.version, auth.user_id)
        .await
        .map_err(file_error_response)?;

    Ok(Json(FileRestoreResponse {
        id: file.id,
        current_version: file.current_version,
        restored_from_version: req.version,
        content_hash: file.content_hash,
        size: file.size,
        modified_at: file.modified_at.to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct RestoreVersionRequest {
    pub version: i32,
}

#[derive(Debug, Serialize)]
pub struct FileRestoreResponse {
    pub id: Uuid,
    pub current_version: i32,
    pub restored_from_version: i32,
    pub content_hash: String,
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
pub async fn move_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<MoveFileRequest>,
) -> Result<Json<File>, Response> {
    let file = state
        .file_service
        .move_file(file_id, req.target_folder_id, auth.user_id)
        .await
        .map_err(file_error_response)?;

    Ok(Json(file))
}

#[derive(Debug, Deserialize)]
pub struct MoveFileRequest {
    pub target_folder_id: Option<Uuid>,
}

/// Rename a file.
///
/// POST /api/files/{id}/rename
///
/// Request body: { "new_name": "document.pdf" }
pub async fn rename_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Json(req): Json<RenameFileRequest>,
) -> Result<Json<File>, Response> {
    let file = state
        .file_service
        .rename_file(file_id, req.new_name, auth.user_id)
        .await
        .map_err(file_error_response)?;

    Ok(Json(file))
}

#[derive(Debug, Deserialize)]
pub struct RenameFileRequest {
    pub new_name: String,
}

// ============================================================================
// Thumbnail Endpoint
// ============================================================================

/// Maximum file size for thumbnail generation (100MB)
const MAX_THUMBNAIL_FILE_SIZE: i64 = 100 * 1024 * 1024;

/// Map ThumbnailError to HTTP response.
fn thumbnail_error_response(err: ThumbnailError) -> Response {
    let (status, message) = match err {
        ThumbnailError::NotFound => (StatusCode::NOT_FOUND, "File not found".to_string()),
        ThumbnailError::UnsupportedType => (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Thumbnail generation not supported for this file type".to_string(),
        ),
        ThumbnailError::Storage(_) | ThumbnailError::Generation(_) | ThumbnailError::Database(_) => {
            tracing::error!("Thumbnail service error: {}", err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to generate thumbnail".to_string(),
            )
        }
    };

    (status, Json(ErrorResponse::new(message))).into_response()
}

/// Query parameters for thumbnail requests.
#[derive(Debug, Deserialize)]
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
pub async fn get_file_thumbnail(
    State(state): State<AppState>,
    AuthenticatedUser { user_id }: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
    Query(params): Query<ThumbnailParams>,
) -> Result<Response, Response> {
    // First, verify the user has access to the file
    let file = state
        .file_service
        .get_file(file_id, user_id)
        .await
        .map_err(file_error_response)?;

    // Check file size - don't generate thumbnails for files larger than 100MB
    if file.size > MAX_THUMBNAIL_FILE_SIZE {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ErrorResponse {
                error: "File too large for thumbnail generation".to_string(),
                details: Some(format!(
                    "File size {} exceeds maximum allowed {} bytes",
                    file.size, MAX_THUMBNAIL_FILE_SIZE
                )),
            }),
        )
            .into_response());
    }

    // Parse size parameter (default to "md")
    let size_str = params.size.as_deref().unwrap_or("md");
    let size = ThumbnailSize::try_from(size_str).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(format!(
                "Invalid size parameter: {}. Use 'sm', 'md', or 'lg'",
                size_str
            ))),
        )
            .into_response()
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
                .map_err(thumbnail_error_response)?
        }
        Err(e) => {
            tracing::error!("Failed to get thumbnail: {}", e);
            return Err(thumbnail_error_response(e));
        }
    };

    // Get thumbnail data from storage
    let thumbnail_data = state
        .thumbnail_service
        .get_thumbnail_data(&thumbnail.storage_path)
        .await
        .map_err(thumbnail_error_response)?;

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
// List All User Files (Simple View)
// ============================================================================

// ============================================================================
// Share Indicator Types
// ============================================================================

/// File with share information for list responses
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FileWithShares {
    // File fields
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub content_hash: String,
    pub size: i64,
    pub mime_type: String,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub current_version: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub modified_at: chrono::DateTime<chrono::Utc>,
    // Share info
    pub is_shared: bool,
    pub share_count: i64,
    /// Earliest share expiration date (None if no shares have expiration)
    pub share_expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// List all files for the current user.
///
/// GET /api/files
///
/// Returns a simple flat list of all files owned by the user with share indicators.
pub async fn list_files(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<FileWithShares>>, Response> {
    // Query all files with share information
    let files = sqlx::query_as::<_, FileWithShares>(
        r#"
        SELECT
            f.id, f.name, f.path, f.content_hash, f.size, f.mime_type,
            f.parent_folder_id, f.owner_id, f.current_version,
            f.created_at, f.modified_at,
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
            ) as share_expires_at
        FROM files f
        WHERE f.owner_id = $1
        ORDER BY f.created_at DESC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| file_error_response(FileError::Storage(format!("Failed to list files: {}", e))))?;

    Ok(Json(files))
}
