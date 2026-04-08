//! Authentication extractors for JWT token validation.

use axum::{
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
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::web_session::{extract_cookie_value, resolve_user_session};
use crate::AppState;

/// Authentication error types for token resolution.
#[derive(Debug)]
pub enum AuthError {
    InvalidToken,
    UserNotFound,
    AccountDisabled,
    DatabaseError,
}

impl AuthError {
    /// Convert AuthError to an HTTP response.
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token"),
            AuthError::UserNotFound => (StatusCode::UNAUTHORIZED, "User not found"),
            AuthError::AccountDisabled => (StatusCode::UNAUTHORIZED, "Account is disabled"),
            AuthError::DatabaseError => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Authentication failed")
            }
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// Hash a token using SHA-256, return hex string.
fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

/// Resolve a bearer token to a user ID.
///
/// First tries JWT validation, then falls back to device token lookup.
/// For device tokens, updates `last_used_at` timestamp on successful lookup.
pub async fn resolve_bearer_token(token: &str, state: &AppState) -> Result<Uuid, AuthError> {
    // First, try JWT validation
    match state.jwt_manager.validate(token) {
        Ok(claims) => {
            let user_id = Uuid::parse_str(&claims.sub).map_err(|_| AuthError::InvalidToken)?;

            // Check disabled status
            let disabled: bool =
                sqlx::query_scalar("SELECT disabled_at IS NOT NULL FROM users WHERE id = $1")
                    .bind(user_id)
                    .fetch_optional(&state.db_pool)
                    .await
                    .map_err(|_| AuthError::DatabaseError)?
                    .ok_or(AuthError::UserNotFound)?;

            if disabled {
                return Err(AuthError::AccountDisabled);
            }

            return Ok(user_id);
        }
        Err(_) => {
            // JWT validation failed, try device token lookup
        }
    }

    // Device token lookup
    let token_hash = hash_token(token);

    // Look up device token by hash
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT user_id FROM device_tokens WHERE token_hash = $1 AND revoked_at IS NULL",
    )
    .bind(&token_hash)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|_| AuthError::DatabaseError)?;

    let user_id = match row {
        Some((uid,)) => uid,
        None => return Err(AuthError::InvalidToken),
    };

    // Update last_used_at
    sqlx::query("UPDATE device_tokens SET last_used_at = NOW() WHERE token_hash = $1")
        .bind(&token_hash)
        .execute(&state.db_pool)
        .await
        .map_err(|_| AuthError::DatabaseError)?;

    // Check disabled status for device token user
    let disabled: bool =
        sqlx::query_scalar("SELECT disabled_at IS NOT NULL FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|_| AuthError::DatabaseError)?
            .ok_or(AuthError::UserNotFound)?;

    if disabled {
        return Err(AuthError::AccountDisabled);
    }

    Ok(user_id)
}

/// Authenticated user extracted from JWT token.
///
/// This extractor validates the JWT token from the Authorization header
/// and extracts the user ID. Use this in handler functions to ensure
/// the request is authenticated.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    /// Tenant ID for multi-tenant support (defaults to nil UUID for single-tenant mode).
    pub tenant_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedSession {
    pub user_id: Uuid,
    pub session_id: Option<Uuid>,
}

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
                    tenant_id: Uuid::nil(), // TODO: Get tenant_id from session when available
                });
            }
        }

        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| auth_error("Missing or invalid authentication"))?;

        let user_id = resolve_bearer_token(bearer.token(), state)
            .await
            .map_err(|e| e.into_response())?;

        Ok(AuthenticatedUser {
            user_id,
            tenant_id: Uuid::nil(), // TODO: Get tenant_id from JWT claims when available
        })
    }
}

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

        let user_id = resolve_bearer_token(bearer.token(), state)
            .await
            .map_err(|e| e.into_response())?;

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

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth = AuthenticatedUser::from_request_parts(parts, state).await?;

        let row = sqlx::query("SELECT is_admin, disabled_at FROM users WHERE id = $1")
            .bind(auth.user_id)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|_| admin_internal_error("Failed to verify admin status"))?
            .ok_or_else(|| admin_unauthorized_error("User not found"))?;

        let is_admin: bool = row
            .try_get("is_admin")
            .map_err(|_| admin_internal_error("Failed to read admin status"))?;
        let disabled_at: Option<chrono::DateTime<chrono::Utc>> = row
            .try_get("disabled_at")
            .map_err(|_| admin_internal_error("Failed to read disabled status"))?;

        if !is_admin {
            return Err(admin_forbidden_error("Admin access required"));
        }
        if disabled_at.is_some() {
            return Err(admin_forbidden_error("Account is disabled"));
        }
        Ok(AdminUser {
            user_id: auth.user_id,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_token_produces_sha256_hex() {
        let token = "test_token_123";
        let hash = hash_token(token);

        // SHA-256 produces 32 bytes = 64 hex chars
        assert_eq!(hash.len(), 64);

        // Should be valid hex
        for ch in hash.chars() {
            assert!(ch.is_ascii_hexdigit(), "Hash contains non-hex character");
        }
    }

    #[test]
    fn hash_token_is_deterministic() {
        let token = "my_test_token";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn hash_token_differs_for_different_inputs() {
        let hash1 = hash_token("token_one");
        let hash2 = hash_token("token_two");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn auth_error_into_response() {
        let response = AuthError::InvalidToken.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = AuthError::UserNotFound.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = AuthError::AccountDisabled.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = AuthError::DatabaseError.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
