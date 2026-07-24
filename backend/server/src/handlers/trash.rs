//! Trash bin handlers for RustShare API.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use super::{AppError, AuthenticatedUser};
use crate::AppState;

/// Summary of the current user's trash bin contents.
#[derive(Debug, Serialize, utoipa::ToSchema)]
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
#[utoipa::path(
    get,
    path = "/api/v1/trash/summary",
    tag = "Trash",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_trash_summary(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Response, AppError> {
    match state
        .metadata_store
        .get_trash_summary(auth.user_id, auth.tenant_id)
        .await
    {
        Ok((file_count, folder_count, total_size)) => Ok((
            StatusCode::OK,
            Json(TrashSummaryResponse {
                file_count,
                folder_count,
                total_size,
            }),
        )
            .into_response()),
        Err(e) => {
            tracing::error!("Failed to get trash summary: {:?}", e);
            Err(AppError::internal("Failed to get trash summary"))
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
#[utoipa::path(
    delete,
    path = "/api/v1/trash/empty",
    tag = "Trash",
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn empty_trash(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Response, AppError> {
    match state
        .metadata_store
        .empty_trash(auth.user_id, auth.tenant_id)
        .await
    {
        Ok(deleted_file_ids) => {
            // Best-effort removal of any indexed chunks for files that were
            // actually deleted. empty_trash permanently deletes files, so
            // restore is not possible; cleanup is best-effort and must not
            // fail the request.
            if let Some(ref ai_service) = state.ai_service {
                for file_id in deleted_file_ids {
                    if let Err(e) = ai_service.remove_file(file_id, auth.tenant_id).await {
                        tracing::warn!(
                            file_id = %file_id,
                            tenant_id = %auth.tenant_id,
                            error = %e,
                            "Failed to remove file chunk from AI index after trash empty"
                        );
                    }
                }
            }
            Ok(StatusCode::NO_CONTENT.into_response())
        }
        Err(e) => {
            tracing::error!("Failed to empty trash: {:?}", e);
            Err(AppError::internal("Failed to empty trash"))
        }
    }
}
