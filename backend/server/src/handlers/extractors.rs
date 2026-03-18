//! Authentication extractors for JWT token validation.

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json, RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use rustshare_auth::ShareSessionClaims;
use uuid::Uuid;

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

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedUser {
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

        // Validate JWT token
        let claims = state
            .jwt_manager
            .validate(bearer.token())
            .map_err(|e| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({"error": format!("Invalid token: {}", e)})),
                )
                    .into_response()
            })?;

        // Parse user ID from claims
        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Invalid user ID in token"})),
            )
                .into_response()
        })?;

        Ok(AuthenticatedUser { user_id })
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
