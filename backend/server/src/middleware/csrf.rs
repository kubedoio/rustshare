use axum::{
    extract::Request,
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use constant_time_eq::constant_time_eq;

use crate::web_session::{extract_cookie_value, WEB_CSRF_COOKIE_NAME, WEB_CSRF_HEADER_NAME};

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

    let csrf_cookie = extract_cookie_value(headers, WEB_CSRF_COOKIE_NAME);
    let csrf_header = headers
        .get(WEB_CSRF_HEADER_NAME)
        .and_then(|value| value.to_str().ok());

    match (csrf_cookie, csrf_header) {
        (Some(cookie), Some(header)) => {
            if !constant_time_eq(cookie.as_bytes(), header.as_bytes()) {
                return forbidden_response();
            }
        }
        _ => return forbidden_response(),
    }

    next.run(request).await
}

fn forbidden_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": "Missing or invalid CSRF protection token"
        })),
    )
        .into_response()
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware::from_fn,
        routing::post,
        Router,
    };
    use tower::ServiceExt;

    fn test_app() -> Router {
        Router::new()
            .route("/api/v1/test", post(|| async { StatusCode::OK }))
            .layer(from_fn(csrf_middleware))
    }

    #[tokio::test]
    async fn missing_csrf_cookie_returns_403() {
        let app = test_app();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/test")
            .header(
                axum::http::header::COOKIE,
                "rustshare_session=session_value",
            )
            .header(WEB_CSRF_HEADER_NAME, "token")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn missing_csrf_header_returns_403() {
        let app = test_app();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/test")
            .header(
                axum::http::header::COOKIE,
                "rustshare_session=session_value; rustshare_csrf_token=token",
            )
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn mismatched_csrf_cookie_and_header_returns_403() {
        let app = test_app();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/test")
            .header(
                axum::http::header::COOKIE,
                "rustshare_session=session_value; rustshare_csrf_token=token_a",
            )
            .header(WEB_CSRF_HEADER_NAME, "token_b")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn matching_csrf_cookie_and_header_returns_200() {
        let app = test_app();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/test")
            .header(
                axum::http::header::COOKIE,
                "rustshare_session=session_value; rustshare_csrf_token=valid_token",
            )
            .header(WEB_CSRF_HEADER_NAME, "valid_token")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn no_session_cookie_skips_csrf_check() {
        let app = test_app();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/test")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_requires_csrf_check_skips_get() {
        let request = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/test")
            .body(Body::empty())
            .unwrap();
        assert!(!requires_csrf_check(&request));
    }

    #[test]
    fn test_requires_csrf_check_includes_post() {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/test")
            .body(Body::empty())
            .unwrap();
        assert!(requires_csrf_check(&request));
    }

    #[test]
    fn test_requires_csrf_check_skips_public_share() {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/public/share/abc")
            .body(Body::empty())
            .unwrap();
        assert!(!requires_csrf_check(&request));
    }

    #[test]
    fn test_requires_csrf_check_skips_non_api() {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/static/file.css")
            .body(Body::empty())
            .unwrap();
        assert!(!requires_csrf_check(&request));
    }
}
