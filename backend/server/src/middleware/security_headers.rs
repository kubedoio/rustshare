use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};

/// Lightweight security headers middleware.
///
/// Adds basic security headers to all responses except health-check endpoints.
/// CSP and HSTS are intentionally omitted — those are reverse-proxy concerns.
pub async fn security_headers_middleware(request: Request, next: Next) -> Response {
    if request.uri().path().starts_with("/health") {
        return next.run(request).await;
    }

    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::StatusCode, routing::get, Router};
    use tower::ServiceExt;

    fn test_app() -> Router {
        Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/api/vault-sync/v1/vaults", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(security_headers_middleware))
    }

    #[tokio::test]
    async fn test_health_does_not_get_security_headers() {
        let app = test_app();
        let request = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers();
        assert!(!headers.contains_key("x-content-type-options"));
        assert!(!headers.contains_key("x-frame-options"));
        assert!(!headers.contains_key("referrer-policy"));
    }

    #[tokio::test]
    async fn test_api_gets_security_headers() {
        let app = test_app();
        let request = Request::builder()
            .uri("/api/vault-sync/v1/vaults")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers();
        assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
        assert_eq!(
            headers.get("referrer-policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
    }
}
