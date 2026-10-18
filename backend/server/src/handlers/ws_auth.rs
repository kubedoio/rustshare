//! Shared WebSocket authentication utilities.
//! Extracted from sync.rs so both sync_handler and collab_handler can reuse auth logic.

use axum::http::{HeaderMap, StatusCode};
use rustshare_auth::ShareSessionClaims;
use rustshare_core::domain::{FileId, ShareId, SharePermissions, UserId};
use serde::Deserialize;
use uuid::Uuid;

use super::extractors::bearer_token_from_headers;
use crate::web_session::extract_cookie_value;
use crate::AppState;

/// Identifies the client connected to a WebSocket.
#[derive(Debug, Clone)]
pub enum ClientIdentity {
    /// Authenticated user
    User {
        user_id: UserId,
        tenant_id: Uuid,
        credential_version: i64,
    },
    /// Anonymous share viewer with session token
    ShareViewer {
        share_id: ShareId,
        file_id: Option<FileId>,
        permissions: SharePermissions,
        session_expires_at: i64,
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
        let (row_tenant_id, credential_version) = ensure_ws_user_active(
            state,
            user_id,
            Some(claims.iat),
            Some(claims.credential_version),
        )
        .await?;
        if claims.tenant_id != row_tenant_id {
            return Err(WsTokenError::PrincipalRejected(
                StatusCode::UNAUTHORIZED,
                "Token organization does not match the principal".to_string(),
            ));
        }
        return Ok(ClientIdentity::User {
            user_id,
            tenant_id: row_tenant_id,
            credential_version,
        });
    }

    // Try to decode as share session JWT
    if let Ok(claims) = state.jwt_manager.decode_custom::<ShareSessionClaims>(token) {
        if claims.is_expired() {
            return Err(WsTokenError::InvalidToken);
        }
        if !share_is_active(state, claims.share_id, claims.exp)
            .await
            .map_err(|_| {
                WsTokenError::PrincipalRejected(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Share access could not be verified".to_string(),
                )
            })?
        {
            return Err(WsTokenError::PrincipalRejected(
                StatusCode::UNAUTHORIZED,
                "Share is no longer active".to_string(),
            ));
        }

        return Ok(ClientIdentity::ShareViewer {
            share_id: claims.share_id,
            file_id: claims.file_id,
            permissions: claims.permissions,
            session_expires_at: claims.exp,
        });
    }

    Err(WsTokenError::InvalidToken)
}

pub async fn share_is_active(
    state: &AppState,
    share_id: ShareId,
    session_expires_at: i64,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM shares WHERE id=$1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > NOW()) AND to_timestamp($2) > NOW())")
        .bind(share_id)
        .bind(session_expires_at)
        .fetch_one(&state.db_pool)
        .await
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
    token_iat: Option<i64>,
    token_credential_version: Option<i64>,
) -> Result<(Uuid, i64), (StatusCode, String)> {
    let row: Option<(bool, Uuid, bool, i64)> = sqlx::query_as(
        "SELECT disabled_at IS NOT NULL, tenant_id, $2::bigint IS NOT NULL AND (credential_version <> $3 OR (credentials_revoked_at IS NOT NULL AND to_timestamp($2) <= credentials_revoked_at)), credential_version FROM users WHERE id = $1",
    )
            .bind(user_id)
            .bind(token_iat)
            .bind(token_credential_version)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            })?;

    match row {
        Some((true, _, _, _)) | Some((false, _, true, _)) => {
            Err((StatusCode::FORBIDDEN, "Account disabled".to_string()))
        }
        Some((false, tenant_id, false, version)) => Ok((tenant_id, version)),
        None => Err((StatusCode::UNAUTHORIZED, "User not found".to_string())),
    }
}

/// Recheck a live user socket against the credential generation captured at
/// authentication time. Account reactivation cannot revive the old socket.
pub async fn user_credential_is_current(
    state: &AppState,
    user_id: UserId,
    tenant_id: Uuid,
    credential_version: i64,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id=$1 AND tenant_id=$2 AND disabled_at IS NULL AND credential_version=$3)",
    )
    .bind(user_id)
    .bind(tenant_id)
    .bind(credential_version)
    .fetch_one(&state.db_pool)
    .await
}

async fn resolve_ws_user_session(
    state: &AppState,
    session_token: &str,
) -> Result<Option<(UserId, Uuid, i64)>, (StatusCode, String)> {
    let session_hash = rustshare_auth::hash_web_session_token(session_token);
    let mut tx = state.db_pool.begin().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;
    let session_owner: Option<(Uuid, Uuid)> =
        sqlx::query_as("SELECT user_id, tenant_id FROM user_sessions WHERE session_token_hash=$1")
            .bind(&session_hash)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            })?;
    let Some((user_id, session_tenant_id)) = session_owner else {
        return Ok(None);
    };
    let user: Option<(bool, Uuid, i64)> = sqlx::query_as(
        "SELECT disabled_at IS NULL, tenant_id, credential_version FROM users WHERE id=$1 FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;
    let Some((true, tenant_id, credential_version)) = user else {
        return Ok(None);
    };
    if tenant_id != session_tenant_id {
        return Ok(None);
    }
    let session_is_current: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_sessions WHERE session_token_hash=$1 AND user_id=$2 AND tenant_id=$3 AND expires_at > clock_timestamp())",
    )
    .bind(&session_hash)
    .bind(user_id)
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;
    if !session_is_current {
        return Ok(None);
    }
    sqlx::query(
        "UPDATE user_sessions SET last_seen_at=clock_timestamp() WHERE session_token_hash=$1",
    )
    .bind(session_hash)
    .execute(&mut *tx)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;
    tx.commit().await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;
    Ok(Some((UserId::from(user_id), tenant_id, credential_version)))
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
        let Some((user_id, tenant_id, credential_version)) =
            resolve_ws_user_session(state, &session_token).await?
        else {
            return Err((StatusCode::UNAUTHORIZED, "Invalid session".to_string()));
        };

        return Ok(ClientIdentity::User {
            user_id,
            tenant_id,
            credential_version,
        });
    }

    Err(last_error.unwrap_or((
        StatusCode::UNAUTHORIZED,
        "Missing authentication (cookie, Authorization header, or ?token= query parameter)"
            .to_string(),
    )))
}
