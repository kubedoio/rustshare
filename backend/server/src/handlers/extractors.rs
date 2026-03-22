//! Authentication extractors for JWT token validation.

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json, RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use rustshare_auth::ShareSessionClaims;
use sqlx::Row;
use uuid::Uuid;

use crate::web_session::{extract_cookie_value, resolve_user_session};
use crate::AppState;

/// Authenticated user extracted from JWT token.
///
/// This extractor validates the JWT token from the Authorization header
/// and extracts the user ID. Use this in handler functions to ensure
/// the request is authenticated.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedSession {
    pub user_id: Uuid,
    pub session_id: Option<Uuid>,
}

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(session_token) =
            extract_cookie_value(&parts.headers, rustshare_auth::WEB_SESSION_COOKIE_NAME)
        {
            if let Some(session) = resolve_user_session(state, &session_token)
                .await
                .map_err(session_auth_error)?
            {
                return Ok(AuthenticatedUser {
                    user_id: session.user_id,
                });
            }
        }

        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| auth_error("Missing or invalid authentication"))?;

        let claims = state
            .jwt_manager
            .validate(bearer.token())
            .map_err(|e| auth_error(&format!("Invalid token: {}", e)))?;

        let user_id =
            Uuid::parse_str(&claims.sub).map_err(|_| auth_error("Invalid user ID in token"))?;

        let disabled: bool = sqlx::query_scalar("SELECT disabled_at IS NOT NULL FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|_| auth_error("Authentication failed"))?
            .ok_or_else(|| auth_error("User not found"))?;

        if disabled {
            return Err(auth_error("Account is disabled"));
        }

        Ok(AuthenticatedUser { user_id })
    }
}

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedSession {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(session_token) =
            extract_cookie_value(&parts.headers, rustshare_auth::WEB_SESSION_COOKIE_NAME)
        {
            if let Some(session) = resolve_user_session(state, &session_token)
                .await
                .map_err(session_auth_error)?
            {
                return Ok(AuthenticatedSession {
                    user_id: session.user_id,
                    session_id: Some(session.id),
                });
            }
        }

        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| auth_error("Missing or invalid authentication"))?;

        let claims = state
            .jwt_manager
            .validate(bearer.token())
            .map_err(|e| auth_error(&format!("Invalid token: {}", e)))?;

        let user_id =
            Uuid::parse_str(&claims.sub).map_err(|_| auth_error("Invalid user ID in token"))?;

        let disabled: bool = sqlx::query_scalar("SELECT disabled_at IS NOT NULL FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|_| auth_error("Authentication failed"))?
            .ok_or_else(|| auth_error("User not found"))?;

        if disabled {
            return Err(auth_error("Account is disabled"));
        }

        Ok(AuthenticatedSession {
            user_id,
            session_id: None,
        })
    }
}

/// Extractor for share session authentication.
///
/// Validates JWT tokens issued by create_session endpoint.
/// Token must contain valid ShareSessionClaims.
#[derive(Debug, Clone)]
pub struct ShareSessionAuth(pub ShareSessionClaims);

#[async_trait]
impl FromRequestParts<AppState> for ShareSessionAuth {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Extract Authorization header
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({"error": "Missing or invalid Authorization header"})),
                )
                    .into_response()
            })?;

        // Decode and validate JWT
        let claims = state
            .jwt_manager
            .decode_custom::<ShareSessionClaims>(bearer.token())
            .map_err(|e| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({"error": format!("Invalid token: {}", e)})),
                )
                    .into_response()
            })?;

        // Check if token is expired
        if claims.is_expired() {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Token has expired"})),
            )
                .into_response());
        }

        Ok(ShareSessionAuth(claims))
    }
}

/// Admin-only extractor. Validates the session/JWT (like AuthenticatedUser),
/// then asserts the user has is_admin = true and is not disabled.
/// Returns 403 Forbidden if not admin or account is disabled.
#[derive(Debug, Clone)]
pub struct AdminUser {
    pub user_id: Uuid,
}

#[async_trait]
impl FromRequestParts<AppState> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthenticatedUser::from_request_parts(parts, state).await?;

        let row = sqlx::query(
            "SELECT is_admin, disabled_at FROM users WHERE id = $1",
        )
        .bind(auth.user_id)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|_| admin_internal_error("Failed to verify admin status"))?
        .ok_or_else(|| admin_unauthorized_error("User not found"))?;

        let is_admin: bool = row.try_get("is_admin")
            .map_err(|_| admin_internal_error("Failed to read admin status"))?;
        let disabled_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("disabled_at")
            .map_err(|_| admin_internal_error("Failed to read disabled status"))?;

        if !is_admin {
            return Err(admin_forbidden_error("Admin access required"));
        }
        if disabled_at.is_some() {
            return Err(admin_forbidden_error("Account is disabled"));
        }
        Ok(AdminUser { user_id: auth.user_id })
    }
}

fn admin_forbidden_error(msg: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({ "error": msg })),
    )
        .into_response()
}

fn admin_internal_error(msg: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": msg })),
    )
        .into_response()
}

fn admin_unauthorized_error(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": msg })),
    )
        .into_response()
}

pub(crate) fn bearer_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let bearer = value.strip_prefix("Bearer ")?;
    Some(bearer.to_string())
}

fn auth_error(message: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}

fn session_auth_error(error: String) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": format!("Session validation failed: {}", error) })),
    )
        .into_response()
}
