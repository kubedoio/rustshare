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
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct WsAuthQuery {
    pub token: Option<String>,
}

/// Why client-token validation failed.
///
/// The two classes are treated differently by
/// [`resolve_ws_client_identity`]: an unrecognizable token falls through to
/// the other credential kinds (query token, session cookie), but a valid
/// user JWT whose principal is rejected fails fast — a second credential
/// presented in the same request must not be honoured instead.
#[derive(Debug)]
pub enum WsTokenError {
    /// Not a credential this server can use (bad signature, expired,
    /// malformed). Another credential kind may still succeed.
    InvalidToken,
    /// A valid user JWT whose principal is rejected: the account is
    /// disabled, the user no longer exists, or the token's recorded
    /// organization drifted from the user's row (ADR-0040 D3, invariant A7).
    PrincipalRejected(StatusCode, String),
}

impl From<(StatusCode, String)> for WsTokenError {
    fn from((status, message): (StatusCode, String)) -> Self {
        Self::PrincipalRejected(status, message)
    }
}

/// Validate client token — supports both user and share session JWTs.
///
/// For a user JWT the tenant is *not* taken from the token's `tenant_id`
/// claim: the claim is only a hint, and the organization must come from the
/// principal's `users.tenant_id` row (ADR-0040 D3, invariant A7). The row is
/// loaded anyway for the disabled check; a claim that disagrees with the row
/// is tenant drift and fails closed.
pub async fn validate_client_token(
    token: &str,
    state: &AppState,
) -> Result<ClientIdentity, WsTokenError> {
    // First try to decode as user JWT
    if let Ok(claims) = state.jwt_manager.validate(token) {
        let user_id =
            UserId::from(Uuid::parse_str(&claims.sub).map_err(|_| WsTokenError::InvalidToken)?);
        let row_tenant_id = ensure_ws_user_active(state, user_id).await?;
        if claims.tenant_id != row_tenant_id {
            return Err(WsTokenError::PrincipalRejected(
                StatusCode::UNAUTHORIZED,
                "Token organization does not match the principal".to_string(),
            ));
        }
        return Ok(ClientIdentity::User {
            user_id,
            tenant_id: row_tenant_id,
        });
    }

    // Try to decode as share session JWT
    if let Ok(claims) = state.jwt_manager.decode_custom::<ShareSessionClaims>(token) {
        if claims.is_expired() {
            return Err(WsTokenError::InvalidToken);
        }

        return Ok(ClientIdentity::ShareViewer {
            share_id: claims.share_id,
            file_id: claims.file_id,
            permissions: claims.permissions,
        });
    }

    Err(WsTokenError::InvalidToken)
}

/// Verify the user still exists and is not disabled, mirroring the HTTP
/// extractor's checks. A disabled/deleted user with an unexpired JWT or a live
/// browser session must not keep receiving realtime events or autosaving.
///
/// Returns the principal's organization from the `users` row: the row is the
/// single source of truth for tenant identity (ADR-0040 D3).
async fn ensure_ws_user_active(
    state: &AppState,
    user_id: UserId,
) -> Result<Uuid, (StatusCode, String)> {
    let row: Option<(bool, Uuid)> =
        sqlx::query_as("SELECT disabled_at IS NOT NULL, tenant_id FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            })?;

    match row {
        Some((true, _)) => Err((StatusCode::FORBIDDEN, "Account disabled".to_string())),
        Some((false, tenant_id)) => Ok(tenant_id),
        None => Err((StatusCode::UNAUTHORIZED, "User not found".to_string())),
    }
}

/// Resolve a WebSocket client identity from headers + query parameters.
/// Tries Bearer token, then ?token=, then session cookie.
///
/// A Bearer/query token that is merely unrecognizable falls through to the
/// next credential kind; a *valid user JWT whose principal is rejected*
/// (disabled account, missing user, tenant drift) fails fast — the remaining
/// credential kinds must not be honoured as a side channel (ADR-0040 D3,
/// invariant A7).
pub async fn resolve_ws_client_identity(
    state: &AppState,
    headers: &HeaderMap,
    query: &WsAuthQuery,
) -> Result<ClientIdentity, (StatusCode, String)> {
    let mut last_error: Option<(StatusCode, String)> = None;

    if let Some(token) = bearer_token_from_headers(headers) {
        match validate_client_token(&token, state).await {
            Ok(identity) => return Ok(identity),
            Err(WsTokenError::PrincipalRejected(status, message)) => {
                return Err((status, message));
            }
            Err(WsTokenError::InvalidToken) => {
                last_error = Some((StatusCode::UNAUTHORIZED, "Invalid token".to_string()));
            }
        }
    }

    if let Some(token) = query.token.as_ref() {
        match validate_client_token(token, state).await {
            Ok(identity) => return Ok(identity),
            Err(WsTokenError::PrincipalRejected(status, message)) => {
                return Err((status, message));
            }
            Err(WsTokenError::InvalidToken) => {
                last_error = Some((StatusCode::UNAUTHORIZED, "Invalid token".to_string()));
            }
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

        // resolve_user_session already validated the session's organization
        // against the user's row (ADR-0040 D3); re-check activity here.
        ensure_ws_user_active(state, session.user_id).await?;

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
