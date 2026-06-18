use axum::http::{header, HeaderMap};
use rand::Rng;
use rustshare_auth::{generate_web_session_token, hash_web_session_token, WEB_SESSION_COOKIE_NAME};
use rustshare_core::domain::UserSession;

use crate::AppState;

pub const WEB_CSRF_HEADER_NAME: &str = "X-Rustshare-Csrf";
pub const WEB_CSRF_COOKIE_NAME: &str = "rustshare_csrf_token";

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
    tenant_id: uuid::Uuid,
    user_agent: Option<String>,
    ip_address: Option<String>,
) -> Result<(String, String), String> {
    let session_token = generate_web_session_token();
    let session = UserSession::new(
        user_id,
        hash_web_session_token(&session_token),
        session_ttl_seconds(),
        user_agent,
        ip_address,
        tenant_id,
    );

    state
        .metadata_store
        .create_user_session(&session)
        .await
        .map_err(|error| error.to_string())?;

    let csrf_token = generate_csrf_token();

    Ok((session_token, csrf_token))
}

/// Build the session cookie.
///
/// `SameSite=Lax` is used instead of `Strict` so that users following a link
/// from an external site (e.g. a shared link in an email or chat) still arrive
/// logged in. The cookie is still sent on safe top-level navigations, but is
/// withheld from cross-site POST/iframe requests, which blocks the common CSRF
/// vector. Mutating requests are further protected by the double-submit CSRF
/// cookie/header pair.
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

pub fn generate_csrf_token() -> String {
    let mut csrf_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut csrf_bytes);
    hex::encode(csrf_bytes)
}

/// Build the double-submit CSRF cookie.
///
/// `SameSite=Lax` is sufficient here because the attacker scenario we care
/// about is a cross-site POST/iframe submission, and Lax cookies are not sent
/// with those requests. Using `Strict` would break legitimate top-level
/// navigations from external links (e.g. opening a shared RustShare link)
/// because the browser would not send the CSRF cookie on the initial GET,
/// causing the first mutating request after navigation to fail.
pub fn build_csrf_cookie(csrf_token: &str) -> String {
    let mut cookie = format!(
        "{}={}; Path=/; SameSite=Lax; Max-Age={}",
        WEB_CSRF_COOKIE_NAME,
        csrf_token,
        session_ttl_seconds()
    );

    if session_cookie_secure() {
        cookie.push_str("; Secure");
    }

    cookie
}

pub fn build_expired_csrf_cookie() -> String {
    let mut cookie = format!("{}=; Path=/; SameSite=Lax; Max-Age=0", WEB_CSRF_COOKIE_NAME);

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
    // Production default is `true` (cookies only sent over HTTPS).
    // Developers running on `http://localhost` must explicitly set
    // `RUSTSHARE_SESSION_COOKIE_SECURE=false` (or the legacy
    // `SESSION_COOKIE_SECURE=false`).
    std::env::var("RUSTSHARE_SESSION_COOKIE_SECURE")
        .ok()
        .or_else(|| std::env::var("SESSION_COOKIE_SECURE").ok())
        .map(|value| parse_env_bool(&value))
        .unwrap_or(true)
}

/// Parse a truthy environment variable value.
///
/// Accepts `"true"` or `"1"` (case-insensitive) as true; everything else is false.
fn parse_env_bool(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value == "1"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that mutate process-global environment variables.
    ///
    /// Env vars are shared across the process, so parallel tests that read and
    /// write the same variables race with each other. Acquiring this mutex for
    /// the duration of each such test eliminates the race.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn session_cookie_secure_env_var_precedence_and_fallback() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Prefixed name takes precedence when both are set.
        std::env::set_var("RUSTSHARE_SESSION_COOKIE_SECURE", "false");
        std::env::set_var("SESSION_COOKIE_SECURE", "true");
        assert!(
            !session_cookie_secure(),
            "RUSTSHARE_SESSION_COOKIE_SECURE must take precedence over SESSION_COOKIE_SECURE"
        );

        // Fall back to legacy name when prefixed name is absent.
        std::env::remove_var("RUSTSHARE_SESSION_COOKIE_SECURE");
        std::env::set_var("SESSION_COOKIE_SECURE", "false");
        assert!(
            !session_cookie_secure(),
            "SESSION_COOKIE_SECURE must be used when RUSTSHARE_SESSION_COOKIE_SECURE is unset"
        );

        // Default remains true when neither is set.
        std::env::remove_var("RUSTSHARE_SESSION_COOKIE_SECURE");
        std::env::remove_var("SESSION_COOKIE_SECURE");
        assert!(
            session_cookie_secure(),
            "cookie_secure must default to true"
        );
    }

    #[test]
    fn session_cookie_secure_truthy_values() {
        assert!(parse_env_bool("true"));
        assert!(parse_env_bool("True"));
        assert!(parse_env_bool("TRUE"));
        assert!(parse_env_bool("1"));

        assert!(!parse_env_bool("false"));
        assert!(!parse_env_bool("False"));
        assert!(!parse_env_bool("FALSE"));
        assert!(!parse_env_bool("0"));
        assert!(!parse_env_bool("yes"));
        assert!(!parse_env_bool(""));
    }

    #[test]
    fn secure_session_cookie_flags() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("RUSTSHARE_SESSION_COOKIE_SECURE", "true");

        let session_cookie = build_session_cookie("test-token");
        assert!(
            session_cookie.contains("HttpOnly"),
            "session cookie must be HttpOnly"
        );
        assert!(
            session_cookie.contains("Secure"),
            "session cookie must be Secure"
        );
        assert!(
            session_cookie.contains("SameSite=Lax"),
            "session cookie must use SameSite=Lax"
        );
        assert!(
            session_cookie.contains("Path=/"),
            "session cookie must have Path=/"
        );

        let expired_session_cookie = build_expired_session_cookie();
        assert!(expired_session_cookie.contains("HttpOnly"));
        assert!(expired_session_cookie.contains("Secure"));
        assert!(expired_session_cookie.contains("Max-Age=0"));

        let csrf_cookie = build_csrf_cookie("test-csrf");
        assert!(
            !csrf_cookie.contains("HttpOnly"),
            "CSRF cookie must not be HttpOnly so JavaScript can read it"
        );
        assert!(csrf_cookie.contains("Secure"), "CSRF cookie must be Secure");
        assert!(
            csrf_cookie.contains("SameSite=Lax"),
            "CSRF cookie must use SameSite=Lax"
        );

        let expired_csrf_cookie = build_expired_csrf_cookie();
        assert!(expired_csrf_cookie.contains("Secure"));
        assert!(expired_csrf_cookie.contains("Max-Age=0"));

        std::env::remove_var("RUSTSHARE_SESSION_COOKIE_SECURE");
    }

    #[test]
    fn insecure_session_cookie_flags() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("RUSTSHARE_SESSION_COOKIE_SECURE", "false");

        let session_cookie = build_session_cookie("test-token");
        assert!(session_cookie.contains("HttpOnly"));
        assert!(
            !session_cookie.contains("Secure"),
            "session cookie must not be Secure when secure=false"
        );
        assert!(session_cookie.contains("SameSite=Lax"));

        let csrf_cookie = build_csrf_cookie("test-csrf");
        assert!(
            !csrf_cookie.contains("Secure"),
            "CSRF cookie must not be Secure when secure=false"
        );
        assert!(csrf_cookie.contains("SameSite=Lax"));

        std::env::remove_var("RUSTSHARE_SESSION_COOKIE_SECURE");
    }
}
