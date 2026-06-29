//! HTTP handlers for note operations.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AuthenticatedUser;
use crate::handlers::{files::validate_color, AppError, ValidatedJson};
use crate::services::note_service::{NoteAttachment, NoteSummary, NoteVisibility};
use crate::AppState;

// ============================================================================
// Create Note
// ============================================================================

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateNoteRequest {
    pub title: Option<String>,
    pub parent_folder_id: Option<Uuid>,
    pub content: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateNoteResponse {
    pub id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub okf_id: Option<Uuid>,
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

#[utoipa::path(
    post,
    path = "/api/v1/notes",
    tag = "Notes",
    request_body = CreateNoteRequest,
    responses(
        (status = 201, description = "Note created", body = CreateNoteResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
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
            okf_id: note.okf_id,
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct GetNoteResponse {
    pub id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub okf_id: Option<Uuid>,
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

#[utoipa::path(
    get,
    path = "/api/v1/notes/{id}",
    tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses(
        (status = 200, description = "Note content", body = GetNoteResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Note not found", body = crate::handlers::ErrorResponse),
    ),
)]
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
        okf_id: note.okf_id,
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

/// Deserializer that distinguishes a missing field (`None`) from an
/// explicit JSON `null` (`Some(None)`) from a string value (`Some(Some(s))`).
fn deserialize_optional_color<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<String> = Option::deserialize(deserializer)?;
    Ok(Some(value))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SaveNoteRequest {
    pub content: String,
    /// `None` = omitted/no change, `Some(None)` = explicit `null`/clear,
    /// `Some(Some(color))` = set to a specific color key.
    #[serde(default, deserialize_with = "deserialize_optional_color")]
    pub color: Option<Option<String>>,
    pub attachments: Option<Vec<NoteAttachment>>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SaveNoteResponse {
    pub id: Uuid,
    pub current_version: i32,
    pub modified_at: String,
    pub excerpt: String,
}

#[utoipa::path(
    put,
    path = "/api/v1/notes/{id}",
    tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    request_body = SaveNoteRequest,
    responses(
        (status = 200, description = "Note saved", body = SaveNoteResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Note not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn save_note(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(note_id): Path<Uuid>,
    Json(req): Json<SaveNoteRequest>,
) -> Result<Json<SaveNoteResponse>, AppError> {
    if let Some(Some(ref c)) = req.color {
        validate_color(&Some(c.clone()))?;
    }

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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RenameNoteRequest {
    pub title: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/notes/{id}/rename",
    tag = "Notes",
    params(("note_id" = Uuid, Path, description = "Note Id")),
    request_body = RenameNoteRequest,
    responses(
        (status = 200, description = "Success", body = GetNoteResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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
        okf_id: note.okf_id,
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct MoveNoteRequest {
    pub target_folder_id: Option<Uuid>,
}

#[utoipa::path(
    post,
    path = "/api/v1/notes/{id}/move",
    tag = "Notes",
    params(("note_id" = Uuid, Path, description = "Note Id")),
    request_body = MoveNoteRequest,
    responses(
        (status = 200, description = "Success", body = GetNoteResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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
        okf_id: note.okf_id,
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

#[utoipa::path(
    delete,
    path = "/api/v1/notes/{id}",
    tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    responses(
        (status = 204, description = "Note deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Note not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[utoipa::path(
    get,
    path = "/api/v1/notes",
    tag = "Notes",
    responses(
        (status = 200, description = "List of notes", body = Vec<NoteSummary>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_notes(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<crate::handlers::PaginationQuery>,
) -> Result<Json<Vec<NoteSummary>>, AppError> {
    let notes = state
        .note_service
        .list_notes(auth.user_id, auth.tenant_id, query.limit(), query.offset())
        .await?;

    Ok(Json(notes))
}

// ============================================================================
// Recent Notes (Dashboard)
// ============================================================================

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RecentNotesQuery {
    pub folder_name: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RecentNotesResponse {
    pub notes: Vec<NoteSummary>,
}

#[utoipa::path(
    get,
    path = "/api/v1/notes/recent",
    tag = "Notes",
    responses(
        (status = 200, description = "Success", body = RecentNotesResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_recent_notes(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<RecentNotesQuery>,
) -> Result<Json<RecentNotesResponse>, AppError> {
    let notes = if let Some(folder_name) = query.folder_name {
        let prefix = format!("/{}/", folder_name);
        state
            .note_service
            .list_notes_filtered(auth.user_id, auth.tenant_id, Some(&prefix), 8, 0)
            .await
    } else {
        state
            .note_service
            .list_notes(auth.user_id, auth.tenant_id, 8, 0)
            .await
    }?;

    Ok(Json(RecentNotesResponse { notes }))
}

// ============================================================================
// Toggle Visibility
// ============================================================================

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct VisibilityResponse {
    pub id: Uuid,
    pub visibility: NoteVisibility,
    pub public_share_id: Option<String>,
    pub public_url: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/notes/{id}/visibility",
    tag = "Notes",
    params(("note_id" = Uuid, Path, description = "Note Id")),
    responses(
        (status = 200, description = "Success", body = VisibilityResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DuplicateNoteResponse {
    pub id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub okf_id: Option<Uuid>,
    pub name: String,
    pub path: String,
    pub content: String,
    pub metadata: crate::services::note_service::NoteMetadata,
    pub parent_folder_id: Option<Uuid>,
    pub current_version: i32,
    pub created_at: String,
    pub modified_at: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/notes/{id}/duplicate",
    tag = "Notes",
    params(("id" = Uuid, Path, description = "Id")),
    responses(
        (status = 200, description = "Success", body = DuplicateNoteResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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
            okf_id: note.okf_id,
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
// Resolve Conflict
// ============================================================================

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case", tag = "strategy")]
pub enum ResolveConflictRequest {
    PreferYaml,
    PreferFolder,
    Custom { title: String },
}

impl validator::Validate for ResolveConflictRequest {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Ok(())
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/notes/{id}/resolve-conflict",
    tag = "Notes",
    params(("id" = Uuid, Path, description = "Note ID")),
    request_body = ResolveConflictRequest,
    responses(
        (status = 200, description = "Conflict resolved", body = GetNoteResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn resolve_conflict(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(note_id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<ResolveConflictRequest>,
) -> Result<Json<GetNoteResponse>, AppError> {
    use crate::services::note_service::NoteConflictResolution;

    let resolution = match req {
        ResolveConflictRequest::PreferYaml => NoteConflictResolution::PreferYaml,
        ResolveConflictRequest::PreferFolder => NoteConflictResolution::PreferFolder,
        ResolveConflictRequest::Custom { title } => NoteConflictResolution::Custom(title),
    };

    let note = state
        .note_service
        .resolve_note_conflict(note_id, auth.user_id, auth.tenant_id, resolution)
        .await?;

    let public_url = note
        .metadata
        .public_share_id
        .as_ref()
        .map(|id| format!("{}/p/note/{}", state.public_base_url, id));

    Ok(Json(GetNoteResponse {
        id: note.id,
        okf_id: note.okf_id,
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
// Public Note
// ============================================================================

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PublicNoteResponse {
    pub title: String,
    pub content: String,
    pub excerpt: String,
    pub created_at: String,
    pub updated_at: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/public/notes/{share_id}",
    tag = "Notes",
    params(("share_id" = String, Path, description = "Share Id")),
    responses(
        (status = 200, description = "Success", body = PublicNoteResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::post, Router};
    use tower::ServiceExt;

    #[test]
    fn resolve_conflict_request_deserializes() {
        let json = r#"{"strategy":"prefer_yaml"}"#;
        let req: ResolveConflictRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req, ResolveConflictRequest::PreferYaml));

        let json = r#"{"strategy":"custom","title":"My Title"}"#;
        let req: ResolveConflictRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req, ResolveConflictRequest::Custom { title } if title == "My Title"));
    }

    #[tokio::test]
    async fn resolve_conflict_valid_body_returns_200() {
        async fn handler(ValidatedJson(_req): ValidatedJson<ResolveConflictRequest>) -> StatusCode {
            StatusCode::OK
        }

        let app = Router::new().route("/resolve-conflict", post(handler));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/resolve-conflict")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"strategy":"prefer_yaml"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn resolve_conflict_invalid_body_returns_400() {
        async fn handler(ValidatedJson(_req): ValidatedJson<ResolveConflictRequest>) -> StatusCode {
            StatusCode::OK
        }

        let app = Router::new().route("/resolve-conflict", post(handler));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/resolve-conflict")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"strategy":"custom"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
