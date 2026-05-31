//! Shared WebSocket authentication utilities.
//! Extracted from sync.rs so both sync_handler and collab_handler can reuse auth logic.

use axum::http::{HeaderMap, StatusCode};
use rustshare_auth::ShareSessionClaims;
use rustshare_core::domain::{FileId, ShareId, SharePermissions, UserId};
use serde::Deserialize;
use uuid::Uuid;

use super::extractors::bearer_token_from_headers;
use crate::web_session::{extract_cookie_value, resolve_user_session};
use crate::AppState;

/// Identifies the client connected to a WebSocket.
#[derive(Debug, Clone)]
pub enum ClientIdentity {
    /// Authenticated user
    User { user_id: UserId, tenant_id: Uuid },
    /// Anonymous share viewer with session token
    ShareViewer {
        share_id: ShareId,
        file_id: Option<FileId>,
        permissions: SharePermissions,
    },
}

/// Query parameters for WebSocket authentication.
#[derive(Debug, Deserialize)]
pub struct WsAuthQuery {
    pub token: Option<String>,
}

/// Validate client token — supports both user and share session JWTs.
pub async fn validate_client_token(
    token: &str,
    jwt_manager: &rustshare_auth::JwtManager,
) -> Result<ClientIdentity, (StatusCode, String)> {
    // First try to decode as user JWT
    if let Ok(claims) = jwt_manager.validate(token) {
        let user_id = UserId::from(
            Uuid::parse_str(&claims.sub)
                .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid user ID".to_string()))?,
        );
        return Ok(ClientIdentity::User {
            user_id,
            tenant_id: claims.tenant_id,
        });
    }

    // Try to decode as share session JWT
    if let Ok(claims) = jwt_manager.decode_custom::<ShareSessionClaims>(token) {
        if claims.is_expired() {
            return Err((StatusCode::UNAUTHORIZED, "Token expired".to_string()));
        }

        return Ok(ClientIdentity::ShareViewer {
            share_id: claims.share_id,
            file_id: claims.file_id,
            permissions: claims.permissions,
        });
    }

    Err((StatusCode::UNAUTHORIZED, "Invalid token".to_string()))
}

/// Resolve a WebSocket client identity from headers + query parameters.
/// Tries Bearer token, then ?token=, then session cookie.
pub async fn resolve_ws_client_identity(
    state: &AppState,
    headers: &HeaderMap,
    query: &WsAuthQuery,
) -> Result<ClientIdentity, (StatusCode, String)> {
    let mut last_error: Option<(StatusCode, String)> = None;

    if let Some(token) = bearer_token_from_headers(headers) {
        match validate_client_token(&token, &state.jwt_manager).await {
            Ok(identity) => return Ok(identity),
            Err(error) => last_error = Some(error),
        }
    }

    if let Some(token) = query.token.as_ref() {
        match validate_client_token(token, &state.jwt_manager).await {
            Ok(identity) => return Ok(identity),
            Err(error) => last_error = Some(error),
        }
    }

    if let Some(session_token) =
        extract_cookie_value(headers, rustshare_auth::WEB_SESSION_COOKIE_NAME)
    {
        let Some(session) = resolve_user_session(state, &session_token)
            .await
            .map_err(|error| (StatusCode::UNAUTHORIZED, error))?
        else {
            return Err((StatusCode::UNAUTHORIZED, "Invalid session".to_string()));
        };

        return Ok(ClientIdentity::User {
            user_id: session.user_id,
            tenant_id: session.tenant_id,
        });
    }

    Err(last_error.unwrap_or((
        StatusCode::UNAUTHORIZED,
        "Missing authentication (cookie, Authorization header, or ?token= query parameter)"
            .to_string(),
    )))
}
