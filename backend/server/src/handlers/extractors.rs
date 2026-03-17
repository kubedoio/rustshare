//! Authentication extractors for JWT token validation.

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    RequestPartsExt,
};
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
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
    type Rejection = (StatusCode, String);

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
                    "Missing or invalid Authorization header".to_string(),
                )
            })?;

        // Validate JWT token
        let claims = state
            .jwt_manager
            .validate(bearer.token())
            .map_err(|e| (StatusCode::UNAUTHORIZED, format!("Invalid token: {}", e)))?;

        // Parse user ID from claims
        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                "Invalid user ID in token".to_string(),
            )
        })?;

        Ok(AuthenticatedUser { user_id })
    }
}
