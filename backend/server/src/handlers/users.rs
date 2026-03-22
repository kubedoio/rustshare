use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use rustshare_auth::PasswordHasher;
use rustshare_core::domain::Theme;
use serde::{Deserialize, Serialize};

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
    AuthenticatedUser { user_id }: AuthenticatedUser,
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
