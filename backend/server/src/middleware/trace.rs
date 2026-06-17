use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use tracing::Instrument;
use uuid::Uuid;

/// Header used to correlate requests and responses across services.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Maximum length allowed for a client-provided request ID.
const MAX_REQUEST_ID_LEN: usize = 64;

/// Request-scoped tracing middleware that assigns a correlation ID to every request.
///
/// Behavior:
/// * If the client provides a valid `X-Request-ID` header, it is preserved.
/// * Otherwise a new UUIDv4 is generated.
/// * The request ID is added to the current tracing span as `request_id`.
/// * The request ID is echoed back to the client in the response headers.
pub async fn trace_middleware(request: Request, next: Next) -> Response {
    let request_id = extract_or_generate_request_id(&request);

    let span = tracing::info_span!(
        "http_request",
        method = %request.method(),
        path = %request.uri().path(),
        request_id = %request_id,
    );

    let mut response = next.run(request).instrument(span).await;

    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }

    response
}

fn extract_or_generate_request_id(request: &Request) -> String {
    request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| is_valid_request_id(value))
        .map(|value| value.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn is_valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REQUEST_ID_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::StatusCode, routing::get, Router};
    use tower::ServiceExt;

    fn test_app() -> Router {
        Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(trace_middleware))
    }

    #[tokio::test]
    async fn test_correlation_id_is_returned_in_response_header() {
        let app = test_app();
        let request = Request::builder().uri("/").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let request_id = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .expect("response should contain a correlation/request ID")
            .to_str()
            .unwrap();
        assert!(!request_id.is_empty());
        assert!(is_valid_request_id(request_id));
    }

    #[tokio::test]
    async fn test_generates_request_id_when_header_missing() {
        let app = test_app();
        let request = Request::builder().uri("/").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let request_id = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .expect("response should contain a request ID")
            .to_str()
            .unwrap();
        assert!(!request_id.is_empty());
        assert!(is_valid_request_id(request_id));
    }

    #[tokio::test]
    async fn test_preserves_valid_client_request_id() {
        let app = test_app();
        let client_id = "abc-123_DEF";
        let request = Request::builder()
            .uri("/")
            .header(REQUEST_ID_HEADER, client_id)
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let returned_id = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(returned_id, client_id);
    }

    #[tokio::test]
    async fn test_replaces_request_id_with_invalid_characters() {
        let app = test_app();
        let request = Request::builder()
            .uri("/")
            .header(REQUEST_ID_HEADER, "foo/bar")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let returned_id = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        assert_ne!(returned_id, "foo/bar");
        assert!(is_valid_request_id(returned_id));
    }

    #[tokio::test]
    async fn test_replaces_too_long_request_id() {
        let app = test_app();
        let long_id = "a".repeat(MAX_REQUEST_ID_LEN + 1);
        let request = Request::builder()
            .uri("/")
            .header(REQUEST_ID_HEADER, long_id.as_str())
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let returned_id = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        assert_ne!(returned_id, long_id);
        assert!(is_valid_request_id(returned_id));
    }

    #[tokio::test]
    async fn test_replaces_non_ascii_request_id() {
        let app = test_app();
        let request = Request::builder()
            .uri("/")
            .header(REQUEST_ID_HEADER, HeaderValue::from_bytes(&[0xff]).unwrap())
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let returned_id = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(is_valid_request_id(returned_id));
    }

    #[tokio::test]
    async fn test_replaces_empty_request_id() {
        let app = test_app();
        let request = Request::builder()
            .uri("/")
            .header(REQUEST_ID_HEADER, "")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let returned_id = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(!returned_id.is_empty());
        assert!(is_valid_request_id(returned_id));
    }
}
