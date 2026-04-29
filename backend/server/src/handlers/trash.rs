//! Trash bin handlers for RustShare API.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use super::{AuthenticatedUser, ErrorResponse};
use crate::AppState;

/// Summary of the current user's trash bin contents.
#[derive(Debug, Serialize)]
pub struct TrashSummaryResponse {
    pub file_count: i64,
    pub folder_count: i64,
    pub total_size: i64,
}

/// Get a summary of the current user's trash bin.
///
/// # Endpoint
/// `GET /api/v1/trash/summary`
///
/// # Authentication
/// Requires valid JWT token or session cookie.
///
/// # Response
/// - 200 OK: Returns trash summary
/// - 401 Unauthorized: Missing or invalid authentication
/// - 500 Internal Server Error: Database error
pub async fn get_trash_summary(State(state): State<AppState>, auth: AuthenticatedUser) -> Response {
    match state
        .metadata_store
        .get_trash_summary(auth.user_id, auth.tenant_id)
        .await
    {
        Ok((file_count, folder_count, total_size)) => (
            StatusCode::OK,
            Json(TrashSummaryResponse {
                file_count,
                folder_count,
                total_size,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to get trash summary: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to get trash summary")),
            )
                .into_response()
        }
    }
}

/// Empty the current user's trash bin permanently.
///
/// # Endpoint
/// `DELETE /api/v1/trash/empty`
///
/// # Authentication
/// Requires valid JWT token or session cookie.
///
/// # Response
/// - 204 No Content: Trash emptied successfully
/// - 401 Unauthorized: Missing or invalid authentication
/// - 500 Internal Server Error: Database error
pub async fn empty_trash(State(state): State<AppState>, auth: AuthenticatedUser) -> Response {
    match state
        .metadata_store
        .empty_trash(auth.user_id, auth.tenant_id)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("Failed to empty trash: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to empty trash")),
            )
                .into_response()
        }
    }
}
