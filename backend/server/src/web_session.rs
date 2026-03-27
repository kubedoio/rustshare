use axum::http::{header, HeaderMap};
use rustshare_auth::{generate_web_session_token, WEB_SESSION_COOKIE_NAME};
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
    _state: &AppState,
    _session_token: &str,
) -> Result<Option<UserSession>, String> {
    // JWT-based sessions don't track session lists server-side
    // Return None to fall back to JWT token validation
    Ok(None)
}

pub async fn create_user_session(
    _state: &AppState,
    _user_id: uuid::Uuid,
    _user_agent: Option<String>,
    _ip_address: Option<String>,
) -> Result<String, String> {
    // JWT-based sessions don't track session lists server-side
    // Just generate and return a session token
    let session_token = generate_web_session_token();
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
