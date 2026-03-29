//! Profile management handlers for the RustShare API.

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

/// Response for GET /api/v1/users/me/profile
#[derive(Debug, Serialize)]
pub struct ProfileResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub name: Option<String>,
    pub surname: Option<String>,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub email_sharing_enabled: bool,
    pub theme: Theme,
    pub storage_quota: i64,
    pub created_at: String,
}

/// Request for PATCH /api/v1/users/me/profile
#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub name: Option<String>,
    pub surname: Option<String>,
    pub display_name: Option<String>,
    pub email_sharing_enabled: Option<bool>,
    pub theme: Option<Theme>,
}

/// Response for successful profile update
#[derive(Debug, Serialize)]
pub struct UpdateProfileResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub name: Option<String>,
    pub surname: Option<String>,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub email_sharing_enabled: bool,
    pub theme: Theme,
    pub storage_quota: i64,
    pub created_at: String,
}

/// Validate profile update request
fn validate_update_request(req: &UpdateProfileRequest) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if let Some(ref name) = req.name {
        if name.len() > 255 {
            errors.push("name must be at most 255 characters".to_string());
        }
    }

    if let Some(ref surname) = req.surname {
        if surname.len() > 255 {
            errors.push("surname must be at most 255 characters".to_string());
        }
    }

    if let Some(ref display_name) = req.display_name {
        if display_name.is_empty() {
            errors.push("display_name is required".to_string());
        } else if display_name.len() > 255 {
            errors.push("display_name must be at most 255 characters".to_string());
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Get the current user's profile information.
///
/// # Endpoint
/// `GET /api/v1/users/me/profile`
///
/// # Authentication
/// Requires valid JWT token or session cookie.
///
/// # Response
/// - 200 OK: Returns user profile
/// - 401 Unauthorized: Missing or invalid authentication
/// - 404 Not Found: User not found
/// - 500 Internal Server Error: Database error
pub async fn get_profile(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
) -> Response {
    match state.metadata_store.find_user_by_id(user_id).await {
        Ok(Some(user)) => {
            let profile = ProfileResponse {
                id: user.id.to_string(),
                username: user.username,
                email: user.email,
                name: user.name,
                surname: user.surname,
                display_name: user.display_name,
                avatar_path: user.avatar_path,
                email_sharing_enabled: user.email_sharing_enabled,
                theme: user.theme,
                storage_quota: user.storage_quota,
                created_at: user.created_at.to_rfc3339(),
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

/// Update the current user's profile information.
///
/// # Endpoint
/// `PATCH /api/v1/users/me/profile`
///
/// # Authentication
/// Requires valid JWT token or session cookie.
///
/// # Request Body
/// All fields are optional. Only provided fields will be updated.
/// ```json
/// {
///   "name": "string | null",
///   "surname": "string | null",
///   "display_name": "string",
///   "email_sharing_enabled": boolean,
///   "theme": "light" | "dark" | "system"
/// }
/// ```
///
/// # Response
/// - 200 OK: Profile updated successfully, returns updated profile
/// - 400 Bad Request: Invalid request data
/// - 401 Unauthorized: Missing or invalid authentication
/// - 404 Not Found: User not found
/// - 500 Internal Server Error: Database error
pub async fn update_profile(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Json(req): Json<UpdateProfileRequest>,
) -> Response {
    // Validate request
    if let Err(errors) = validate_update_request(&req) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(format!(
                "Validation failed: {}",
                errors.join(", ")
            ))),
        )
            .into_response();
    }

    // Get current user
    let user = match state.metadata_store.find_user_by_id(user_id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("User not found")),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to find user for profile update: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to update profile")),
            )
                .into_response();
        }
    };

    // Build update parameters
    let name = req.name;
    let surname = req.surname;
    let display_name = req.display_name;
    let email_sharing_enabled = req.email_sharing_enabled;
    let theme = req.theme;

    // Update user in database
    if let Err(e) = state
        .metadata_store
        .update_user_profile(
            user_id,
            name.as_deref(),
            surname.as_deref(),
            display_name.as_deref(),
            email_sharing_enabled,
            theme.as_ref().map(|t| t.to_string()),
        )
        .await
    {
        tracing::error!("Failed to update user profile: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to update profile")),
        )
            .into_response();
    }

    // Fetch updated user to return complete profile
    match state.metadata_store.find_user_by_id(user_id).await {
        Ok(Some(updated_user)) => {
            let response = UpdateProfileResponse {
                id: updated_user.id.to_string(),
                username: updated_user.username,
                email: updated_user.email,
                name: updated_user.name,
                surname: updated_user.surname,
                display_name: updated_user.display_name,
                avatar_path: updated_user.avatar_path,
                email_sharing_enabled: updated_user.email_sharing_enabled,
                theme: updated_user.theme,
                storage_quota: updated_user.storage_quota,
                created_at: updated_user.created_at.to_rfc3339(),
            };

            (StatusCode::OK, Json(response)).into_response()
        }
        _ => {
            // If we can't fetch the updated user, return the original with updates applied
            let response = UpdateProfileResponse {
                id: user.id.to_string(),
                username: user.username,
                email: user.email,
                name: name.or(user.name),
                surname: surname.or(user.surname),
                display_name: display_name.unwrap_or(user.display_name),
                avatar_path: user.avatar_path,
                email_sharing_enabled: email_sharing_enabled.unwrap_or(user.email_sharing_enabled),
                theme: theme.unwrap_or(user.theme),
                storage_quota: user.storage_quota,
                created_at: user.created_at.to_rfc3339(),
            };

            (StatusCode::OK, Json(response)).into_response()
        }
    }
}
