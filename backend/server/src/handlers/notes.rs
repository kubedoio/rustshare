//! HTTP handlers for note operations.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::services::note_service::{NoteError, NoteSummary, NoteVisibility};
use crate::{AppState, handlers::ErrorResponse};
use super::AuthenticatedUser;

// ============================================================================
// Error Response Mapping
// ============================================================================

pub fn note_error_response(err: NoteError) -> Response {
    let (status, message) = match err {
        NoteError::NotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        NoteError::PermissionDenied => (StatusCode::FORBIDDEN, err.to_string()),
        NoteError::InvalidName(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        NoteError::Database(_) | NoteError::Storage(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        }
    };
    (status, Json(ErrorResponse::new(message))).into_response()
}

// ============================================================================
// Create Note
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateNoteRequest {
    pub title: Option<String>,
    pub parent_folder_id: Option<Uuid>,
    pub content: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateNoteResponse {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub content: String,
    pub metadata: crate::services::note_service::NoteMetadata,
    pub parent_folder_id: Option<Uuid>,
    pub current_version: i32,
    pub created_at: String,
    pub modified_at: String,
    pub public_url: Option<String>,
}

pub async fn create_note(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateNoteRequest>,
) -> Result<(StatusCode, Json<CreateNoteResponse>), Response> {
    let note = state
        .note_service
        .create_note(auth.user_id, auth.tenant_id, req.title, req.parent_folder_id, req.content)
        .await
        .map_err(note_error_response)?;

    let public_url = note.metadata.public_share_id.as_ref().map(|id| {
        format!("{}/p/note/{}", state.public_base_url, id)
    });

    Ok((
        StatusCode::CREATED,
        Json(CreateNoteResponse {
            id: note.id,
            name: note.name,
            path: note.path,
            content: note.content,
            metadata: note.metadata,
            parent_folder_id: note.parent_folder_id,
            current_version: note.current_version,
            created_at: note.created_at.to_rfc3339(),
            modified_at: note.modified_at.to_rfc3339(),
            public_url,
        }),
    ))
}

// ============================================================================
// Get Note
// ============================================================================

#[derive(Debug, Serialize)]
pub struct GetNoteResponse {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub content: String,
    pub metadata: crate::services::note_service::NoteMetadata,
    pub parent_folder_id: Option<Uuid>,
    pub current_version: i32,
    pub created_at: String,
    pub modified_at: String,
    pub public_url: Option<String>,
}

pub async fn get_note(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(note_id): Path<Uuid>,
) -> Result<Json<GetNoteResponse>, Response> {
    let note = state
        .note_service
        .get_note(note_id, auth.user_id)
        .await
        .map_err(note_error_response)?;

    let public_url = note.metadata.public_share_id.as_ref().map(|id| {
        format!("{}/p/note/{}", state.public_base_url, id)
    });

    Ok(Json(GetNoteResponse {
        id: note.id,
        name: note.name,
        path: note.path,
        content: note.content,
        metadata: note.metadata,
        parent_folder_id: note.parent_folder_id,
        current_version: note.current_version,
        created_at: note.created_at.to_rfc3339(),
        modified_at: note.modified_at.to_rfc3339(),
        public_url,
    }))
}

// ============================================================================
// Save Note
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SaveNoteRequest {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SaveNoteResponse {
    pub id: Uuid,
    pub current_version: i32,
    pub modified_at: String,
    pub excerpt: String,
}

pub async fn save_note(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(note_id): Path<Uuid>,
    Json(req): Json<SaveNoteRequest>,
) -> Result<Json<SaveNoteResponse>, Response> {
    let note = state
        .note_service
        .save_note(note_id, auth.user_id, req.content)
        .await
        .map_err(note_error_response)?;

    Ok(Json(SaveNoteResponse {
        id: note.id,
        current_version: note.current_version,
        modified_at: note.modified_at.to_rfc3339(),
        excerpt: note.metadata.excerpt,
    }))
}

// ============================================================================
// Rename Note
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct RenameNoteRequest {
    pub title: String,
}

pub async fn rename_note(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(note_id): Path<Uuid>,
    Json(req): Json<RenameNoteRequest>,
) -> Result<Json<GetNoteResponse>, Response> {
    let note = state
        .note_service
        .rename_note(note_id, auth.user_id, req.title)
        .await
        .map_err(note_error_response)?;

    let public_url = note.metadata.public_share_id.as_ref().map(|id| {
        format!("{}/p/note/{}", state.public_base_url, id)
    });

    Ok(Json(GetNoteResponse {
        id: note.id,
        name: note.name,
        path: note.path,
        content: note.content,
        metadata: note.metadata,
        parent_folder_id: note.parent_folder_id,
        current_version: note.current_version,
        created_at: note.created_at.to_rfc3339(),
        modified_at: note.modified_at.to_rfc3339(),
        public_url,
    }))
}

// ============================================================================
// Move Note
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct MoveNoteRequest {
    pub target_folder_id: Option<Uuid>,
}

pub async fn move_note(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(note_id): Path<Uuid>,
    Json(req): Json<MoveNoteRequest>,
) -> Result<Json<GetNoteResponse>, Response> {
    let note = state
        .note_service
        .move_note(note_id, auth.user_id, req.target_folder_id)
        .await
        .map_err(note_error_response)?;

    let public_url = note.metadata.public_share_id.as_ref().map(|id| {
        format!("{}/p/note/{}", state.public_base_url, id)
    });

    Ok(Json(GetNoteResponse {
        id: note.id,
        name: note.name,
        path: note.path,
        content: note.content,
        metadata: note.metadata,
        parent_folder_id: note.parent_folder_id,
        current_version: note.current_version,
        created_at: note.created_at.to_rfc3339(),
        modified_at: note.modified_at.to_rfc3339(),
        public_url,
    }))
}

// ============================================================================
// Delete Note
// ============================================================================

pub async fn delete_note(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(note_id): Path<Uuid>,
) -> Result<StatusCode, Response> {
    state
        .note_service
        .delete_note(note_id, auth.user_id)
        .await
        .map_err(note_error_response)?;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// List Notes
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListNotesQuery {
    pub limit: Option<usize>,
}

pub async fn list_notes(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ListNotesQuery>,
) -> Result<Json<Vec<NoteSummary>>, Response> {
    let notes = state
        .note_service
        .list_notes(auth.user_id, auth.tenant_id, query.limit)
        .await
        .map_err(note_error_response)?;

    Ok(Json(notes))
}

// ============================================================================
// Recent Notes (Dashboard)
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct RecentNotesQuery {
    pub folder_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RecentNotesResponse {
    pub notes: Vec<NoteSummary>,
}

pub async fn list_recent_notes(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<RecentNotesQuery>,
) -> Result<Json<RecentNotesResponse>, Response> {
    let notes = if let Some(folder_name) = query.folder_name {
        let prefix = format!("/{}/", folder_name);
        state
            .note_service
            .list_notes_filtered(auth.user_id, auth.tenant_id, Some(&prefix), Some(8))
            .await
    } else {
        state
            .note_service
            .list_notes(auth.user_id, auth.tenant_id, Some(8))
            .await
    }
    .map_err(note_error_response)?;

    Ok(Json(RecentNotesResponse { notes }))
}

// ============================================================================
// Toggle Visibility
// ============================================================================

#[derive(Debug, Serialize)]
pub struct VisibilityResponse {
    pub id: Uuid,
    pub visibility: NoteVisibility,
    pub public_share_id: Option<String>,
    pub public_url: Option<String>,
}

pub async fn toggle_visibility(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(note_id): Path<Uuid>,
) -> Result<Json<VisibilityResponse>, Response> {
    let note = state
        .note_service
        .toggle_visibility(note_id, auth.user_id)
        .await
        .map_err(note_error_response)?;

    let public_url = note.metadata.public_share_id.as_ref().map(|id| {
        format!("{}/p/note/{}", state.public_base_url, id)
    });

    Ok(Json(VisibilityResponse {
        id: note.id,
        visibility: note.metadata.visibility,
        public_share_id: note.metadata.public_share_id,
        public_url,
    }))
}

// ============================================================================
// Public Note
// ============================================================================

#[derive(Debug, Serialize)]
pub struct PublicNoteResponse {
    pub title: String,
    pub content: String,
    pub excerpt: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get_public_note(
    State(state): State<AppState>,
    Path(share_id): Path<String>,
) -> Result<Json<PublicNoteResponse>, Response> {
    let note = state
        .note_service
        .get_public_note(&share_id)
        .await
        .map_err(note_error_response)?;

    Ok(Json(PublicNoteResponse {
        title: note.title,
        content: note.content,
        excerpt: note.excerpt,
        created_at: note.created_at.to_rfc3339(),
        updated_at: note.updated_at.to_rfc3339(),
    }))
}

// Need Query import for list_notes
use axum::extract::Query;
