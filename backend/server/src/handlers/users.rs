//! User management handlers for the RustShare API.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use rustshare_auth::PasswordHasher;
use rustshare_core::domain::Theme;
use serde::{Deserialize, Serialize};
// tracing::{error, warn} are used as tracing::error! and tracing::warn! in the code


use crate::handlers::{AuthenticatedSession, AuthenticatedUser, ErrorResponse};
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

/// Request to update the authenticated user's password.
#[derive(Debug, Deserialize)]
pub struct UpdatePasswordRequest {
    pub current_password: String,
    pub new_password: String,
    pub confirm_password: String,
}

/// Response for successful password update.
#[derive(Debug, Serialize)]
pub struct UpdatePasswordResponse {
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct UserSessionResponse {
    pub id: String,
    pub created_at: String,
    pub last_seen_at: String,
    pub expires_at: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub is_current: bool,
}

#[derive(Debug, Serialize)]
pub struct UserSecurityEventResponse {
    pub id: String,
    pub event_type: String,
    pub description: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub session_id: Option<String>,
    pub occurred_at: String,
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
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
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

/// Update the authenticated user's password.
pub async fn update_user_password(
    State(state): State<AppState>,
    AuthenticatedSession {
        user_id,
        session_id,
    }: AuthenticatedSession,
    headers: HeaderMap,
    Json(req): Json<UpdatePasswordRequest>,
) -> Response {
    if req.new_password != req.confirm_password {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "New password and confirmation do not match",
            )),
        )
            .into_response();
    }

    if req.new_password.len() < 10 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "New password must be at least 10 characters long",
            )),
        )
            .into_response();
    }

    if req.current_password == req.new_password {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "New password must be different from the current password",
            )),
        )
            .into_response();
    }

    let user = match state.metadata_store.find_user_by_id(user_id).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("User not found")),
            )
                .into_response();
        }
        Err(error) => {
            tracing::error!("Failed to load user for password update: {:?}", error);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to update password")),
            )
                .into_response();
        }
    };

    let is_valid = match PasswordHasher::verify(&req.current_password, &user.password_hash) {
        Ok(is_valid) => is_valid,
        Err(error) => {
            tracing::error!(
                "Failed to verify password during password update: {:?}",
                error
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to update password")),
            )
                .into_response();
        }
    };

    if !is_valid {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Current password is incorrect")),
        )
            .into_response();
    }

    let new_password_hash = match PasswordHasher::hash(&req.new_password) {
        Ok(hash) => hash,
        Err(error) => {
            tracing::error!("Failed to hash new password: {:?}", error);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to update password")),
            )
                .into_response();
        }
    };

    if let Err(error) = state
        .metadata_store
        .update_user_password_hash(user_id, &new_password_hash)
        .await
    {
        tracing::error!("Failed to persist new password hash: {:?}", error);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to update password")),
        )
            .into_response();
    }

    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok());
    let ip_address =
        crate::middleware::extract_client_ip(&headers, None).map(|value| value.to_string());

    if let Err(error) = state
        .metadata_store
        .create_user_security_event(rustshare_storage::UserSecurityEventRecord {
            user_id,
            event_type: "password_changed",
            description: "Account password changed",
            ip_address: ip_address.as_deref(),
            user_agent,
            session_id,
        })
        .await
    {
        tracing::warn!(
            "Failed to record password change security event: {:?}",
            error
        );
    }

    (
        StatusCode::OK,
        Json(UpdatePasswordResponse {
            message: "Password updated successfully".to_string(),
        }),
    )
        .into_response()
}

/// List active browser sessions for the authenticated user.
pub async fn list_user_sessions(
    State(state): State<AppState>,
    AuthenticatedSession {
        user_id,
        session_id: current_session_id,
    }: AuthenticatedSession,
) -> Response {
    match state.metadata_store.list_user_sessions(user_id).await {
        Ok(sessions) => {
            let response: Vec<UserSessionResponse> = sessions
                .into_iter()
                .map(|session| UserSessionResponse {
                    id: session.id.to_string(),
                    created_at: session.created_at.to_rfc3339(),
                    last_seen_at: session.last_seen_at.to_rfc3339(),
                    expires_at: session.expires_at.to_rfc3339(),
                    user_agent: session.user_agent,
                    ip_address: session.ip_address,
                    is_current: current_session_id.is_some_and(|id| id == session.id),
                })
                .collect();

            (StatusCode::OK, Json(response)).into_response()
        }
        Err(error) => {
            tracing::error!("Failed to list user sessions: {:?}", error);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to list user sessions")),
            )
                .into_response()
        }
    }
}

/// Revoke a browser session owned by the authenticated user.
pub async fn delete_user_session(
    State(state): State<AppState>,
    AuthenticatedSession {
        user_id,
        session_id: current_session_id,
    }: AuthenticatedSession,
    headers: HeaderMap,
    axum::extract::Path(session_id): axum::extract::Path<uuid::Uuid>,
) -> Response {
    if current_session_id.is_some_and(|current| current == session_id) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "Use Sign Out to end your current browser session",
            )),
        )
            .into_response();
    }

    let target_session = match state.metadata_store.list_user_sessions(user_id).await {
        Ok(sessions) => sessions
            .into_iter()
            .find(|session| session.id == session_id),
        Err(error) => {
            tracing::error!("Failed to load user sessions before delete: {:?}", error);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to revoke session")),
            )
                .into_response();
        }
    };

    let Some(target_session) = target_session else {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Session not found")),
        )
            .into_response();
    };

    match state
        .metadata_store
        .delete_user_session_by_id(user_id, session_id)
        .await
    {
        Ok(()) => {
            let user_agent = headers
                .get(axum::http::header::USER_AGENT)
                .and_then(|value| value.to_str().ok());
            let ip_address =
                crate::middleware::extract_client_ip(&headers, None).map(|value| value.to_string());
            let description = format!(
                "Revoked browser session{}",
                target_session
                    .user_agent
                    .as_deref()
                    .map(|agent| format!(" ({agent})"))
                    .unwrap_or_default()
            );

            if let Err(error) = state
                .metadata_store
                .create_user_security_event(rustshare_storage::UserSecurityEventRecord {
                    user_id,
                    event_type: "session_revoked",
                    description: &description,
                    ip_address: ip_address.as_deref(),
                    user_agent,
                    session_id: current_session_id,
                })
                .await
            {
                tracing::warn!(
                    "Failed to record session revoke security event: {:?}",
                    error
                );
            }

            StatusCode::NO_CONTENT.into_response()
        }
        Err(error) => {
            tracing::error!("Failed to delete user session: {:?}", error);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to revoke session")),
            )
                .into_response()
        }
    }
}

/// List recent security events for the authenticated user.
pub async fn list_user_security_events(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
) -> Response {
    match state
        .metadata_store
        .list_user_security_events(user_id, 20)
        .await
    {
        Ok(events) => {
            let response: Vec<UserSecurityEventResponse> = events
                .into_iter()
                .map(|event| UserSecurityEventResponse {
                    id: event.id.to_string(),
                    event_type: event.event_type,
                    description: event.description,
                    ip_address: event.ip_address,
                    user_agent: event.user_agent,
                    session_id: event.session_id.map(|value| value.to_string()),
                    occurred_at: event.occurred_at.to_rfc3339(),
                })
                .collect();

            (StatusCode::OK, Json(response)).into_response()
        }
        Err(error) => {
            tracing::error!("Failed to list user security events: {:?}", error);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to list security events")),
            )
                .into_response()
        }
    }
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
    pub avatar_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get_user_profile(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
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
                avatar_path: user.avatar_path,
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

/// Response for successful avatar upload.
#[derive(Debug, Serialize)]
pub struct UploadAvatarResponse {
    pub avatar_path: String,
}

/// Maximum avatar file size (5MB).
const MAX_AVATAR_SIZE: usize = 5 * 1024 * 1024;

/// Target avatar size (256x256).
const AVATAR_SIZE: u32 = 256;

/// Upload avatar for the authenticated user.
///
/// # Endpoint
/// `POST /api/v1/users/me/avatar`
///
/// # Authentication
/// Requires valid JWT token.
///
/// # Request Body
/// Raw image data (image/* content types accepted)
///
/// # Response
/// - 200 OK: Avatar uploaded successfully
/// - 400 Bad Request: Invalid image or too large
/// - 401 Unauthorized: Missing or invalid token
/// - 500 Internal Server Error: Processing or storage error
pub async fn upload_avatar(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    // Check content type
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if !content_type.starts_with("image/") {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Content-Type must be image/*")),
        )
            .into_response();
    }

    // Check file size
    if body.len() > MAX_AVATAR_SIZE {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Avatar must be less than 5MB")),
        )
            .into_response();
    }

    // Process image: resize to 256x256 and convert to WebP
    let processed = tokio::task::spawn_blocking(move || {
        use image::imageops::FilterType;

        let img = image::load_from_memory(&body)
            .map_err(|e| format!("Failed to load image: {}", e))?;

        let resized = img.resize(AVATAR_SIZE, AVATAR_SIZE, FilterType::Lanczos3);

        let mut output = Vec::new();
        let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut output);
        let rgba = resized.to_rgba8();
        encoder
            .encode(&rgba, resized.width(), resized.height(), image::ColorType::Rgba8)
            .map_err(|e| format!("WebP encode failed: {}", e))?;

        Ok::<Vec<u8>, String>(output)
    })
    .await;

    let processed = match processed {
        Ok(Ok(data)) => data,
        Ok(Err(e)) => {
            tracing::error!("Image processing failed: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!("Image processing failed: {}", e))),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Image processing task failed: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to process image")),
            )
                .into_response();
        }
    };

    // Store in object storage
    let avatar_path = format!("avatars/{}.webp", user_id);
    if let Err(e) = state
        .object_store
        .put(&avatar_path, processed.into())
        .await
    {
        tracing::error!("Failed to store avatar: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to store avatar")),
        )
            .into_response();
    }

    // Update database
    if let Err(e) = state
        .metadata_store
        .update_user_avatar(user_id, Some(&avatar_path))
        .await
    {
        tracing::error!("Failed to update user avatar_path: {:?}", e);
        // Try to clean up stored avatar
        if let Err(e) = state.object_store.delete(&avatar_path).await {
                    tracing::warn!(avatar_path = %avatar_path, error = %e, "failed to delete old avatar");
                }
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to update avatar")),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(UploadAvatarResponse { avatar_path }),
    )
        .into_response()
}

/// Delete avatar for the authenticated user.
///
/// # Endpoint
/// `DELETE /api/v1/users/me/avatar`
///
/// # Authentication
/// Requires valid JWT token.
///
/// # Response
/// - 204 No Content: Avatar deleted successfully
/// - 401 Unauthorized: Missing or invalid token
/// - 500 Internal Server Error: Database error
pub async fn delete_avatar(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
) -> Response {
    // Get current avatar_path
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
            tracing::error!("Failed to find user for avatar deletion: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to delete avatar")),
            )
                .into_response();
        }
    };

    // Delete from object storage if exists
    if let Some(avatar_path) = &user.avatar_path {
        if let Err(e) = state.object_store.delete(avatar_path).await {
                    tracing::warn!(avatar_path = %avatar_path, error = %e, "failed to delete avatar");
                }
    }

    // Update database to clear avatar_path
    if let Err(e) = state.metadata_store.update_user_avatar(user_id, None).await {
        tracing::error!("Failed to clear user avatar_path: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to delete avatar")),
        )
            .into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

/// Get avatar image for a user.
///
/// # Endpoint
/// `GET /api/v1/users/:id/avatar`
///
/// # Authentication
/// None required - public endpoint.
///
/// # Response
/// - 200 OK: Returns avatar image (image/webp)
/// - 404 Not Found: User has no avatar
/// - 500 Internal Server Error: Storage error
pub async fn get_avatar(
    State(state): State<AppState>,
    Path(user_id): Path<uuid::Uuid>,
) -> Response {
    // Get user to find avatar_path
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
            tracing::error!("Failed to find user for avatar: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to get avatar")),
            )
                .into_response();
        }
    };

    // Check if user has an avatar
    let avatar_path = match user.avatar_path {
        Some(path) => path,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Avatar not found")),
            )
                .into_response();
        }
    };

    // Fetch image data from storage
    let data = match state.object_store.get(&avatar_path).await {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("Failed to fetch avatar from storage: {:?}", e);
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Avatar not found")),
            )
                .into_response();
        }
    };

    // Return image with proper content type
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "image/webp")],
        data,
    )
        .into_response()
}
