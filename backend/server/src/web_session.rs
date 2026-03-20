use axum::http::{header, HeaderMap};
use rustshare_auth::{generate_web_session_token, hash_web_session_token, WEB_SESSION_COOKIE_NAME};
use rustshare_core::domain::UserSession;

use crate::AppState;

pub const WEB_CSRF_HEADER_NAME: &str = "X-Rustshare-Csrf";

pub fn extract_cookie_value(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;

    for cookie in cookie_header.split(';') {
        let (name, value) = cookie.trim().split_once('=')?;
        if name == cookie_name {
            return Some(value.to_string());
        }
    }

    None
}

pub async fn resolve_user_session(
    state: &AppState,
    session_token: &str,
) -> Result<Option<UserSession>, String> {
    let token_hash = hash_web_session_token(session_token);
    let Some(session) = state
        .metadata_store
        .find_user_session_by_token_hash(&token_hash)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };

    if session.is_expired() {
        state
            .metadata_store
            .delete_user_session_by_token_hash(&token_hash)
            .await
            .map_err(|error| error.to_string())?;
        return Ok(None);
    }

    state
        .metadata_store
        .touch_user_session(session.id)
        .await
        .map_err(|error| error.to_string())?;

    Ok(Some(session))
}

pub async fn create_user_session(
    state: &AppState,
    user_id: uuid::Uuid,
    user_agent: Option<String>,
    ip_address: Option<String>,
) -> Result<String, String> {
    let session_token = generate_web_session_token();
    let session = UserSession::new(
        user_id,
        hash_web_session_token(&session_token),
        session_ttl_seconds(),
        user_agent,
        ip_address,
    );

    state
        .metadata_store
        .create_user_session(&session)
        .await
        .map_err(|error| error.to_string())?;

    Ok(session_token)
}

pub fn build_session_cookie(session_token: &str) -> String {
    let mut cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        WEB_SESSION_COOKIE_NAME,
        session_token,
        session_ttl_seconds()
    );

    if session_cookie_secure() {
        cookie.push_str("; Secure");
    }

    cookie
}

pub fn build_expired_session_cookie() -> String {
    let mut cookie = format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
        WEB_SESSION_COOKIE_NAME
    );

    if session_cookie_secure() {
        cookie.push_str("; Secure");
    }

    cookie
}

fn session_ttl_seconds() -> i64 {
    std::env::var("WEB_SESSION_TTL_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60 * 60 * 24 * 7)
}

fn session_cookie_secure() -> bool {
    std::env::var("SESSION_COOKIE_SECURE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(false)
}
