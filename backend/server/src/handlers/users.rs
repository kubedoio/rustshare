use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use rustshare_core::domain::Theme;
use serde::{Deserialize, Serialize};

use crate::handlers::{AuthenticatedUser, ErrorResponse};
use crate::AppState;

/// Request to update user theme preference.
#[derive(Debug, Deserialize)]
pub struct UpdateThemeRequest {
    pub theme: Theme,
}

/// Response for successful theme update.
#[derive(Debug, Serialize)]
pub struct UpdateThemeResponse {
    pub theme: Theme,
}

/// Update the authenticated user's theme preference.
///
/// # Endpoint
/// `PATCH /api/users/me/theme`
///
/// # Authentication
/// Requires valid JWT token.
///
/// # Request Body
/// ```json
/// {
///   "theme": "light" | "dark" | "system"
/// }
/// ```
///
/// # Response
/// - 200 OK: Theme updated successfully
/// - 400 Bad Request: Invalid theme value
/// - 401 Unauthorized: Missing or invalid token
/// - 500 Internal Server Error: Database error
pub async fn update_user_theme(
    State(state): State<AppState>,
    AuthenticatedUser { user_id }: AuthenticatedUser,
    Json(req): Json<UpdateThemeRequest>,
) -> Response {
    // Update theme in database
    if let Err(e) = state
        .metadata_store
        .update_user_theme(user_id, &req.theme.to_string())
        .await
    {
        tracing::error!("Failed to update user theme: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to update theme")),
        )
            .into_response();
    }

    // Return success response
    (
        StatusCode::OK,
        Json(UpdateThemeResponse { theme: req.theme }),
    )
        .into_response()
}

/// Get the authenticated user's profile information.
///
/// # Endpoint
/// `GET /api/users/me`
///
/// # Authentication
/// Requires valid JWT token.
///
/// # Response
/// - 200 OK: Returns user profile
/// - 401 Unauthorized: Missing or invalid token
/// - 404 Not Found: User not found
/// - 500 Internal Server Error: Database error
#[derive(Debug, Serialize)]
pub struct UserProfile {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub is_admin: bool,
    pub storage_quota: i64,
    pub theme: Theme,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get_user_profile(
    State(state): State<AppState>,
    AuthenticatedUser { user_id }: AuthenticatedUser,
) -> Response {
    // Get user from database
    match state.metadata_store.find_user_by_id(user_id).await {
        Ok(Some(user)) => {
            let profile = UserProfile {
                id: user.id.to_string(),
                username: user.username,
                display_name: user.display_name,
                email: user.email,
                is_admin: user.is_admin,
                storage_quota: user.storage_quota,
                theme: user.theme,
                created_at: user.created_at.to_rfc3339(),
                updated_at: user.updated_at.to_rfc3339(),
            };

            (StatusCode::OK, Json(profile)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("User not found")),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to get user profile: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to get user profile")),
            )
                .into_response()
        }
    }
}
