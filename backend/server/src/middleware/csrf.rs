use axum::{
    extract::Request,
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};

use crate::web_session::{extract_cookie_value, WEB_CSRF_HEADER_NAME};

const PUBLIC_SHARE_PREFIX: &str = "/api/public/share/";
const PUBLIC_SHARE_V1_PREFIX: &str = "/api/v1/public/share/";

pub async fn csrf_middleware(request: Request, next: Next) -> Response {
    if !requires_csrf_check(&request) {
        return next.run(request).await;
    }

    let headers = request.headers();
    let has_session_cookie =
        extract_cookie_value(headers, rustshare_auth::WEB_SESSION_COOKIE_NAME).is_some();

    if !has_session_cookie {
        return next.run(request).await;
    }

    let csrf_header = headers
        .get(WEB_CSRF_HEADER_NAME)
        .and_then(|value| value.to_str().ok());

    if csrf_header != Some("1") {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Missing CSRF protection header"
            })),
        )
            .into_response();
    }

    next.run(request).await
}

fn requires_csrf_check(request: &Request) -> bool {
    let path = request.uri().path();

    if !path.starts_with("/api/") {
        return false;
    }

    if path.starts_with(PUBLIC_SHARE_PREFIX) || path.starts_with(PUBLIC_SHARE_V1_PREFIX) {
        return false;
    }

    !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    )
}
