use axum::http::HeaderMap;
use axum_extra::extract::cookie::{
    Cookie,
    CookieJar,
    SameSite,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const SESSION_COOKIE_NAME: &str = "rustshare_session";

pub fn generate_session_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub fn hash_session_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("{digest:x}")
}

pub fn build_session_cookie(token: &str, secure: bool) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, token.to_string()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .build()
}

pub fn build_expired_session_cookie(secure: bool) -> Cookie<'static> {
    let mut cookie = Cookie::build((SESSION_COOKIE_NAME, String::new()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .build();
    cookie.make_removal();
    cookie
}

pub fn get_session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    CookieJar::from_headers(headers)
        .get(SESSION_COOKIE_NAME)
        .map(|cookie| cookie.value().to_string())
}
