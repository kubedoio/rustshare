//! HTTP handlers for resumable upload operations (TODO-004)
//!
//! This module provides endpoints for:
//! - Creating upload sessions
//! - Querying session status (for resume)
//! - Uploading chunks
//! - Completing uploads
//! - Aborting uploads

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use rustshare_core::domain::SharePermissions;
use rustshare_core::services::{CreateSessionRequest, SessionStatusResponse};

use super::AuthenticatedUser;
use crate::handlers::AppError;
use crate::AppState;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to create a new upload session
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateUploadSessionRequest {
    /// Target folder ID (None for root)
    pub folder_id: Option<Uuid>,
    /// File name
    pub file_name: String,
    /// MIME type
    pub mime_type: String,
    /// Total file size in bytes
    pub total_size: u64,
    /// Chunk size in bytes (optional, defaults to 5MB)
    #[serde(default = "default_chunk_size")]
    pub chunk_size: u64,
    /// Expected SHA-256 hash of the complete file (optional)
    pub file_hash: Option<String>,
}

fn default_chunk_size() -> u64 {
    5 * 1024 * 1024 // 5MB default
}

/// Response for session creation
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateUploadSessionResponse {
    /// Session ID
    pub session_id: Uuid,
    /// Total number of chunks expected
    pub total_chunks: u32,
    /// Chunk size in bytes
    pub chunk_size: u64,
    /// Session expiration time
    pub expires_at: String,
}

/// Response for session status query
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UploadSessionStatusResponse {
    /// Session ID
    pub session_id: Uuid,
    /// Current status
    pub status: String,
    /// Total file size
    pub total_size: u64,
    /// Bytes uploaded so far
    pub uploaded_bytes: u64,
    /// Progress percentage (0-100)
    pub progress_percent: u8,
    /// Total number of chunks
    pub total_chunks: u32,
    /// List of received chunk indices
    pub received_chunks: Vec<u32>,
    /// List of missing chunk indices
    pub missing_chunks: Vec<u32>,
    /// Whether the session is expired
    pub is_expired: bool,
    /// Session expiration time
    pub expires_at: String,
    /// File ID (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<Uuid>,
}

impl From<SessionStatusResponse> for UploadSessionStatusResponse {
    fn from(resp: SessionStatusResponse) -> Self {
        Self {
            session_id: resp.session_id,
            status: format!("{:?}", resp.status).to_lowercase(),
            total_size: resp.total_size,
            uploaded_bytes: resp.uploaded_bytes,
            progress_percent: resp.progress_percent,
            total_chunks: resp.total_chunks,
            received_chunks: resp.received_chunks,
            missing_chunks: resp.missing_chunks,
            is_expired: resp.is_expired,
            expires_at: resp.expires_at.to_rfc3339(),
            file_id: resp.file_id,
        }
    }
}

/// Response for chunk upload
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UploadChunkResponse {
    /// Session ID
    pub session_id: Uuid,
    /// Chunk index that was uploaded
    pub chunk_index: u32,
    /// Whether the chunk was verified successfully
    pub verified: bool,
    /// Current progress percentage
    pub progress_percent: u8,
    /// Whether all chunks are now received
    pub is_complete: bool,
}

impl From<rustshare_core::services::upload_session::ChunkUploadResponse> for UploadChunkResponse {
    fn from(resp: rustshare_core::services::upload_session::ChunkUploadResponse) -> Self {
        Self {
            session_id: resp.session_id,
            chunk_index: resp.chunk_index,
            verified: resp.verified,
            progress_percent: resp.progress_percent,
            is_complete: resp.is_complete,
        }
    }
}

/// Response for session completion
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CompleteUploadResponse {
    /// Session ID
    pub session_id: Uuid,
    /// Created file ID
    pub file_id: Uuid,
    /// File name
    pub file_name: String,
    /// File size
    pub file_size: u64,
}

/// Maximum chunk size for resumable uploads (100 MB).
const MAX_CHUNK_SIZE: usize = 100 * 1024 * 1024;

/// Stream an HTTP body to a temporary file and return the temp file plus size.
/// Enforces a size limit during streaming to prevent OOM.
pub async fn stream_body_to_temp_file(
    body: Body,
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
    let mut stream = body.into_data_stream();

    while let Some(result) = stream.next().await {
        let chunk = result.map_err(|e| {
            tracing::error!("Failed to read body: {e}");
            AppError::internal(format!("Failed to read body: {e}"))
        })?;
        total_size += chunk.len();
        if total_size > max_size {
            return Err(AppError::payload_too_large(format!(
                "Chunk size exceeds maximum allowed {max_size} bytes"
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

async fn calculate_md5_hex_from_path(path: &std::path::Path) -> Result<String, AppError> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| AppError::internal(format!("Failed to open chunk file: {e}")))?;
    let mut context = md5::Context::new();
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|e| AppError::internal(format!("Failed to read chunk file: {e}")))?;
        if read == 0 {
            break;
        }
        context.consume(&buffer[..read]);
    }

    Ok(format!("{:x}", context.finalize()))
}

// ============================================================================
// Handlers
// ============================================================================

/// Create a new upload session
///
/// POST /api/v1/uploads/sessions
///
/// This initiates a resumable upload session for large files.
/// Returns a session ID that the client uses for subsequent chunk uploads.
#[utoipa::path(
    post,
    path = "/api/v1/uploads/sessions",
    tag = "Uploads",
    request_body = CreateUploadSessionRequest,
    responses(
        (status = 200, description = "Success", body = CreateUploadSessionResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_upload_session(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(request): Json<CreateUploadSessionRequest>,
) -> Result<(StatusCode, Json<CreateUploadSessionResponse>), AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::service_unavailable(
                "Upload service not available",
            ));
        }
    };

    // Verify upload permission for shared folders
    if let Some(folder_id) = request.folder_id {
        let has_permission = state
            .permission_resolver
            .check_folder_permission(
                auth.user_id,
                auth.tenant_id,
                folder_id,
                SharePermissions::Edit,
            )
            .await
            .map_err(|e| AppError::internal(format!("Permission check failed: {}", e)))?;
        if !has_permission {
            return Err(AppError::forbidden(
                "You do not have permission to upload to this folder",
            ));
        }
    }

    let create_request = CreateSessionRequest {
        folder_id: request.folder_id,
        file_name: request.file_name,
        mime_type: request.mime_type,
        total_size: request.total_size,
        chunk_size: request.chunk_size,
        file_hash: request.file_hash,
    };

    let response = service
        .create_session(auth.user_id, auth.tenant_id, create_request)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CreateUploadSessionResponse {
            session_id: response.session_id,
            total_chunks: response.total_chunks,
            chunk_size: response.chunk_size,
            expires_at: response.expires_at.to_rfc3339(),
        }),
    ))
}

/// Get session status (for resume)
///
/// GET /api/v1/uploads/sessions/{id}
///
/// Returns the current status of an upload session, including which
/// chunks have been received and which are missing. Clients use this
/// to resume interrupted uploads.
#[utoipa::path(
    get,
    path = "/api/v1/uploads/sessions/{id}",
    tag = "Uploads",
    params(("session_id" = Uuid, Path, description = "Session Id")),
    responses(
        (status = 200, description = "Success", body = UploadSessionStatusResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_upload_session_status(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<UploadSessionStatusResponse>, AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::service_unavailable(
                "Upload service not available",
            ));
        }
    };

    let status = service.get_session_status(session_id, auth.user_id).await?;

    Ok(Json(status.into()))
}

/// Upload a chunk
///
/// PUT /api/v1/uploads/sessions/{id}/chunks/{index}
///
/// Uploads a single chunk of data. The Content-MD5 header can be
/// provided for integrity verification.
///
/// Headers:
/// - Content-MD5: Base64-encoded MD5 hash of the chunk (optional but recommended)
#[utoipa::path(
    put,
    path = "/api/v1/uploads/sessions/{id}/chunks/{index}",
    tag = "Uploads",
    params(("session_id" = Uuid, Path, description = "Session Id"), ("chunk_index" = u32, Path, description = "Chunk Index")),
    responses(
        (status = 200, description = "Success", body = UploadChunkResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn upload_chunk(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((session_id, chunk_index)): Path<(Uuid, u32)>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<UploadChunkResponse>, AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::service_unavailable(
                "Upload service not available",
            ));
        }
    };

    // Stream chunk body to temp file with size limit to prevent OOM. The temp
    // file is passed to the service using the streaming path so the chunk is
    // never read back into memory.
    let (chunk_temp, _chunk_size) = stream_body_to_temp_file(body, MAX_CHUNK_SIZE).await?;
    let chunk_path = chunk_temp.path();

    let expected_md5 = headers
        .get("Content-MD5")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    if let Some(base64_hash) = expected_md5 {
        let expected_md5 =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &base64_hash)
                .map_err(|_| AppError::bad_request("Invalid Content-MD5 header"))?;
        let expected_md5 = hex::encode(expected_md5);
        let actual_md5 = calculate_md5_hex_from_path(chunk_path).await?;
        if expected_md5 != actual_md5 {
            return Err(AppError::bad_request("Content-MD5 verification failed"));
        }
    }

    let response = service
        .upload_chunk_from_path(session_id, chunk_index, chunk_path, None, auth.user_id)
        .await?;

    Ok(Json(response.into()))
}

/// Complete upload and assemble file
///
/// POST /api/v1/uploads/sessions/{id}/complete
///
/// Finalizes the upload by assembling all chunks into the final file,
/// verifying the content hash, creating the file metadata, and cleaning
/// up temporary chunk storage.
#[utoipa::path(
    post,
    path = "/api/v1/uploads/sessions/{id}/complete",
    tag = "Uploads",
    params(("session_id" = Uuid, Path, description = "Session Id")),
    responses(
        (status = 200, description = "Success", body = CompleteUploadResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn complete_upload(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(session_id): Path<Uuid>,
) -> Result<(StatusCode, Json<CompleteUploadResponse>), AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::service_unavailable(
                "Upload service not available",
            ));
        }
    };

    if let Some(folder_id) = service
        .get_session_target_folder(session_id, auth.user_id)
        .await?
    {
        let has_permission = state
            .permission_resolver
            .check_folder_permission(
                auth.user_id,
                auth.tenant_id,
                folder_id,
                SharePermissions::Edit,
            )
            .await
            .map_err(|e| AppError::internal(format!("Permission check failed: {}", e)))?;
        if !has_permission {
            return Err(AppError::forbidden(
                "You do not have permission to complete an upload to this folder",
            ));
        }
    }

    let response = service.complete_upload(session_id, auth.user_id).await?;

    Ok((
        StatusCode::OK,
        Json(CompleteUploadResponse {
            session_id: response.session_id,
            file_id: response.file_id,
            file_name: response.file_name,
            file_size: response.file_size,
        }),
    ))
}

/// Abort upload session
///
/// DELETE /api/v1/uploads/sessions/{id}
///
/// Cancels an in-progress upload and cleans up temporary storage.
#[utoipa::path(
    delete,
    path = "/api/v1/uploads/sessions/{id}",
    tag = "Uploads",
    params(("session_id" = Uuid, Path, description = "Session Id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn abort_upload_session(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(session_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::service_unavailable(
                "Upload service not available",
            ));
        }
    };

    service.abort_session(session_id, auth.user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// List user's active upload sessions
///
/// GET /api/v1/uploads/sessions
///
/// Returns all upload sessions for the authenticated user,
/// useful for resuming previous uploads.
#[utoipa::path(
    get,
    path = "/api/v1/uploads/sessions",
    tag = "Uploads",
    responses(
        (status = 200, description = "Success", body = Vec<UploadSessionStatusResponse>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_upload_sessions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<UploadSessionStatusResponse>>, AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::service_unavailable(
                "Upload service not available",
            ));
        }
    };

    let sessions = service.list_user_sessions(auth.user_id).await?;

    Ok(Json(sessions.into_iter().map(|s| s.into()).collect()))
}

// Helper for hex encoding
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;
    use axum::body::Body;
    use bytes::Bytes;
    use futures_util::stream;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn stream_body_to_temp_file_writes_all_content() {
        let content = b"hello streaming upload";
        let body = Body::from(content.as_slice());

        let (temp_file, size) = stream_body_to_temp_file(body, 1024 * 1024)
            .await
            .expect("should stream small body to temp file");

        assert_eq!(size, content.len());

        let mut file = tokio::fs::File::open(temp_file.path())
            .await
            .expect("temp file should exist");
        let mut read = Vec::new();
        file.read_to_end(&mut read)
            .await
            .expect("should read temp file");
        assert_eq!(read, content);
    }

    #[tokio::test]
    async fn stream_body_to_temp_file_handles_many_small_chunks() {
        // Total content larger than the internal buffer so the streaming path
        // is exercised, but still small enough for a unit test.
        let chunk_size = 1024usize;
        let chunk_count = 1024usize;
        let total_size = chunk_size * chunk_count;
        let chunks: Vec<Bytes> = (0..chunk_count)
            .map(|i| Bytes::from(vec![(i % 256) as u8; chunk_size]))
            .collect();

        let body =
            Body::from_stream(stream::iter(chunks.clone()).map(Ok::<_, std::convert::Infallible>));

        let (temp_file, size) = stream_body_to_temp_file(body, total_size * 2)
            .await
            .expect("should stream chunked body to temp file");

        assert_eq!(size, total_size);

        let mut file = tokio::fs::File::open(temp_file.path())
            .await
            .expect("temp file should exist");
        let mut read = Vec::with_capacity(total_size);
        file.read_to_end(&mut read)
            .await
            .expect("should read temp file");
        assert_eq!(read.len(), total_size);

        // Verify each chunk round-tripped correctly.
        for (i, chunk) in chunks.iter().enumerate() {
            let offset = i * chunk_size;
            assert_eq!(&read[offset..offset + chunk_size], chunk.as_ref());
        }
    }

    #[tokio::test]
    async fn stream_body_to_temp_file_rejects_oversized_content() {
        let content = b"this content exceeds the tiny limit";
        let body = Body::from(content.as_slice());

        let err = stream_body_to_temp_file(body, 5)
            .await
            .expect_err("should reject body over max size");

        match err {
            AppError::PayloadTooLarge(_) => {}
            other => panic!("expected PayloadTooLarge, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn calculate_md5_hex_from_path_matches_content_md5_header_value() {
        let temp_file = tempfile::NamedTempFile::new().expect("temp file should be created");
        let content = b"chunk with content-md5";
        std::fs::write(temp_file.path(), content).expect("temp file should be writable");

        let actual = calculate_md5_hex_from_path(temp_file.path())
            .await
            .expect("md5 should be calculated");
        let expected_header = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            md5::compute(content).as_ref(),
        );
        let expected_bytes =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, expected_header)
                .expect("header should decode");

        assert_eq!(actual, hex::encode(expected_bytes));
    }

    #[tokio::test]
    async fn calculate_md5_hex_from_path_differs_from_sha256() {
        let temp_file = tempfile::NamedTempFile::new().expect("temp file should be created");
        let content = b"md5 is not sha256";
        std::fs::write(temp_file.path(), content).expect("temp file should be writable");

        let actual_md5 = calculate_md5_hex_from_path(temp_file.path())
            .await
            .expect("md5 should be calculated");
        let sha256 = rustshare_core::validation::calculate_sha256(&Bytes::from_static(content));

        assert_ne!(actual_md5, sha256);
    }

    #[tokio::test]
    async fn stream_body_to_temp_file_cleans_up_on_drop() {
        let content = b"temporary content";
        let body = Body::from(content.as_slice());

        let (temp_file, _size) = stream_body_to_temp_file(body, 1024 * 1024)
            .await
            .expect("should stream body to temp file");

        let path = temp_file.path().to_path_buf();
        assert!(path.exists(), "temp file should exist before drop");

        drop(temp_file);

        // NamedTempFile deletes the underlying file on drop.
        assert!(!path.exists(), "temp file should be cleaned up after drop");
    }

    #[tokio::test]
    async fn stream_body_to_temp_file_cleans_up_on_size_error() {
        let chunks: Vec<Bytes> = (0..4).map(|_| Bytes::from_static(b"chunk")).collect();
        let body = Body::from_stream(stream::iter(chunks).map(Ok::<_, std::convert::Infallible>));

        let err = stream_body_to_temp_file(body, 10)
            .await
            .expect_err("should reject body over max size");

        match err {
            AppError::PayloadTooLarge(_) => {}
            other => panic!("expected PayloadTooLarge, got {other:?}"),
        }

        // We cannot inspect the temp file path because it is not returned on
        // error, but NamedTempFile's Drop impl deletes it automatically. This
        // test documents that cleanup is expected even on failure.
    }
}
