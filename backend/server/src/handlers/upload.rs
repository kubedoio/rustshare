//! HTTP handlers for resumable upload operations (TODO-004)
//!
//! This module provides endpoints for:
//! - Creating upload sessions
//! - Querying session status (for resume)
//! - Uploading chunks
//! - Completing uploads
//! - Aborting uploads

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rustshare_core::services::{CreateSessionRequest, SessionStatusResponse};

use super::AuthenticatedUser;
use crate::handlers::AppError;
use crate::AppState;

// ============================================================================
// Request/Response Types
// ============================================================================

/// Request to create a new upload session
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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

// ============================================================================
// Handlers
// ============================================================================

/// Create a new upload session
///
/// POST /api/v1/uploads/sessions
///
/// This initiates a resumable upload session for large files.
/// Returns a session ID that the client uses for subsequent chunk uploads.
pub async fn create_upload_session(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(request): Json<CreateUploadSessionRequest>,
) -> Result<(StatusCode, Json<CreateUploadSessionResponse>), AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::internal("Upload service not available"));
        }
    };

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
pub async fn get_upload_session_status(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(session_id): Path<Uuid>,
) -> Result<Json<UploadSessionStatusResponse>, AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::internal("Upload service not available"));
        }
    };

    let status = service
        .get_session_status(session_id, auth.user_id)
        .await?;

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
pub async fn upload_chunk(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((session_id, chunk_index)): Path<(Uuid, u32)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<UploadChunkResponse>, AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::internal("Upload service not available"));
        }
    };

    // Extract hash from Content-MD5 header if present
    let provided_hash = headers
        .get("Content-MD5")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // Convert MD5 to hex if provided (the header is base64 encoded)
    let provided_hash = if let Some(base64_hash) = provided_hash {
        match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &base64_hash) {
            Ok(bytes) => Some(hex::encode(bytes)),
            Err(_) => None,
        }
    } else {
        None
    };

    let response = service
        .upload_chunk(session_id, chunk_index, body, provided_hash, auth.user_id)
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
pub async fn complete_upload(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(session_id): Path<Uuid>,
) -> Result<(StatusCode, Json<CompleteUploadResponse>), AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::internal("Upload service not available"));
        }
    };

    let response = service
        .complete_upload(session_id, auth.user_id)
        .await?;

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
pub async fn abort_upload_session(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(session_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::internal("Upload service not available"));
        }
    };

    service
        .abort_session(session_id, auth.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// List user's active upload sessions
///
/// GET /api/v1/uploads/sessions
///
/// Returns all upload sessions for the authenticated user,
/// useful for resuming previous uploads.
pub async fn list_upload_sessions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<UploadSessionStatusResponse>>, AppError> {
    let service = match &state.upload_service {
        Some(s) => s,
        None => {
            return Err(AppError::internal("Upload service not available"));
        }
    };

    let sessions = service
        .list_user_sessions(auth.user_id)
        .await?;

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
