//! HTTP handlers for note operations.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AuthenticatedUser;
use crate::handlers::AppError;
use crate::services::note_service::{NoteAttachment, NoteSummary, NoteVisibility};
use crate::AppState;

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
) -> Result<(StatusCode, Json<CreateNoteResponse>), AppError> {
    let note = state
        .note_service
        .create_note(
            auth.user_id,
            auth.tenant_id,
            req.title,
            req.parent_folder_id,
            req.content,
        )
        .await?;

    let public_url = note
        .metadata
        .public_share_id
        .as_ref()
        .map(|id| format!("{}/p/note/{}", state.public_base_url, id));

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
) -> Result<Json<GetNoteResponse>, AppError> {
    let note = state
        .note_service
        .get_note(note_id, auth.user_id, auth.tenant_id)
        .await?;

    let public_url = note
        .metadata
        .public_share_id
        .as_ref()
        .map(|id| format!("{}/p/note/{}", state.public_base_url, id));

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
    pub color: Option<String>,
    pub attachments: Option<Vec<NoteAttachment>>,
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
) -> Result<Json<SaveNoteResponse>, AppError> {
    let note = state
        .note_service
        .save_note(
            note_id,
            auth.user_id,
            auth.tenant_id,
            req.content,
            req.color,
            req.attachments,
        )
        .await?;

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
) -> Result<Json<GetNoteResponse>, AppError> {
    let note = state
        .note_service
        .rename_note(note_id, auth.user_id, auth.tenant_id, req.title)
        .await?;

    let public_url = note
        .metadata
        .public_share_id
        .as_ref()
        .map(|id| format!("{}/p/note/{}", state.public_base_url, id));

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
) -> Result<Json<GetNoteResponse>, AppError> {
    let note = state
        .note_service
        .move_note(note_id, auth.user_id, auth.tenant_id, req.target_folder_id)
        .await?;

    let public_url = note
        .metadata
        .public_share_id
        .as_ref()
        .map(|id| format!("{}/p/note/{}", state.public_base_url, id));

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
) -> Result<StatusCode, AppError> {
    state
        .note_service
        .delete_note(note_id, auth.user_id, auth.tenant_id)
        .await?;

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
) -> Result<Json<Vec<NoteSummary>>, AppError> {
    let notes = state
        .note_service
        .list_notes(auth.user_id, auth.tenant_id, query.limit)
        .await?;

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
) -> Result<Json<RecentNotesResponse>, AppError> {
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
    }?;

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
) -> Result<Json<VisibilityResponse>, AppError> {
    let note = state
        .note_service
        .toggle_visibility(note_id, auth.user_id, auth.tenant_id)
        .await?;

    let public_url = note
        .metadata
        .public_share_id
        .as_ref()
        .map(|id| format!("{}/p/note/{}", state.public_base_url, id));

    Ok(Json(VisibilityResponse {
        id: note.id,
        visibility: note.metadata.visibility,
        public_share_id: note.metadata.public_share_id,
        public_url,
    }))
}

// ============================================================================
// Duplicate Note
// ============================================================================

#[derive(Debug, Serialize)]
pub struct DuplicateNoteResponse {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub content: String,
    pub metadata: crate::services::note_service::NoteMetadata,
    pub parent_folder_id: Option<Uuid>,
    pub current_version: i32,
    pub created_at: String,
    pub modified_at: String,
}

pub async fn duplicate_note(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<DuplicateNoteResponse>), AppError> {
    let note = state
        .note_service
        .duplicate_note(id, auth.user_id, auth.tenant_id)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DuplicateNoteResponse {
            id: note.id,
            name: note.name,
            path: note.path,
            content: note.content,
            metadata: note.metadata,
            parent_folder_id: note.parent_folder_id,
            current_version: note.current_version,
            created_at: note.created_at.to_rfc3339(),
            modified_at: note.modified_at.to_rfc3339(),
        }),
    ))
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
) -> Result<Json<PublicNoteResponse>, AppError> {
    let note = state.note_service.get_public_note(&share_id).await?;

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
