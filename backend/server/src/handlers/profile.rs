//! Profile management handlers for the RustShare API.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use rustshare_core::domain::Theme;
use serde::{Deserialize, Serialize};

use crate::handlers::{AppError, AuthenticatedUser};
use crate::AppState;

/// Response for GET /api/v1/users/me/profile
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProfileResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub name: Option<String>,
    pub surname: Option<String>,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub email_sharing_enabled: bool,
    pub trash_retention_days: Option<i32>,
    pub theme: Theme,
    pub storage_quota: i64,
    pub created_at: String,
}

/// Request for PATCH /api/v1/users/me/profile
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateProfileRequest {
    pub name: Option<String>,
    pub surname: Option<String>,
    pub display_name: Option<String>,
    pub email_sharing_enabled: Option<bool>,
    pub theme: Option<Theme>,
}

/// Response for successful profile update
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UpdateProfileResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub name: Option<String>,
    pub surname: Option<String>,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub email_sharing_enabled: bool,
    pub trash_retention_days: Option<i32>,
    pub theme: Theme,
    pub storage_quota: i64,
    pub created_at: String,
}

/// Request for PATCH /api/v1/users/me/trash-retention
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateTrashRetentionRequest {
    pub days: Option<i32>,
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
#[utoipa::path(
    get,
    path = "/api/v1/users/me/profile",
    tag = "Users",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_profile(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
) -> Result<Response, AppError> {
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
                trash_retention_days: user.trash_retention_days,
                theme: user.theme,
                storage_quota: user.storage_quota,
                created_at: user.created_at.to_rfc3339(),
            };

            Ok((StatusCode::OK, Json(profile)).into_response())
        }
        Ok(None) => Err(AppError::not_found("User not found")),
        Err(e) => {
            tracing::error!("Failed to get user profile: {:?}", e);
            Err(AppError::internal("Failed to get user profile"))
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
#[utoipa::path(
    patch,
    path = "/api/v1/users/me/profile",
    tag = "Users",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_profile(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Json(req): Json<UpdateProfileRequest>,
) -> Result<Response, AppError> {
    // Validate request
    if let Err(errors) = validate_update_request(&req) {
        return Err(AppError::bad_request(format!(
            "Validation failed: {}",
            errors.join(", ")
        )));
    }

    // Get current user
    let user = match state.metadata_store.find_user_by_id(user_id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Err(AppError::not_found("User not found"));
        }
        Err(e) => {
            tracing::error!("Failed to find user for profile update: {:?}", e);
            return Err(AppError::internal("Failed to update profile"));
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
        return Err(AppError::internal("Failed to update profile"));
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
                trash_retention_days: updated_user.trash_retention_days,
                theme: updated_user.theme,
                storage_quota: updated_user.storage_quota,
                created_at: updated_user.created_at.to_rfc3339(),
            };

            Ok((StatusCode::OK, Json(response)).into_response())
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
                trash_retention_days: user.trash_retention_days,
                theme: theme.unwrap_or(user.theme),
                storage_quota: user.storage_quota,
                created_at: user.created_at.to_rfc3339(),
            };

            Ok((StatusCode::OK, Json(response)).into_response())
        }
    }
}

/// Update the current user's trash retention setting.
///
/// # Endpoint
/// `PATCH /api/v1/users/me/trash-retention`
///
/// # Authentication
/// Requires valid JWT token or session cookie.
///
/// # Request Body
/// ```json
/// {
///   "days": 30 | null
/// }
/// ```
///
/// # Response
/// - 200 OK: Returns updated profile
/// - 400 Bad Request: Invalid days value
/// - 401 Unauthorized: Missing or invalid authentication
/// - 500 Internal Server Error: Database error
#[utoipa::path(
    patch,
    path = "/api/v1/users/me/trash-retention",
    tag = "Users",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_trash_retention(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Json(req): Json<UpdateTrashRetentionRequest>,
) -> Result<Response, AppError> {
    // Validate
    if let Some(days) = req.days {
        if !(1..=365).contains(&days) {
            return Err(AppError::bad_request(
                "trash_retention_days must be between 1 and 365",
            ));
        }
    }

    // Update in database
    if let Err(e) = state
        .metadata_store
        .update_user_trash_retention(user_id, req.days)
        .await
    {
        tracing::error!("Failed to update trash retention: {:?}", e);
        return Err(AppError::internal("Failed to update trash retention"));
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
                trash_retention_days: updated_user.trash_retention_days,
                theme: updated_user.theme,
                storage_quota: updated_user.storage_quota,
                created_at: updated_user.created_at.to_rfc3339(),
            };

            Ok((StatusCode::OK, Json(response)).into_response())
        }
        _ => Err(AppError::internal("Failed to fetch updated profile")),
    }
}
