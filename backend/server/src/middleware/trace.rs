use axum::{extract::Request, middleware::Next, response::Response};
use tracing::Instrument;
use uuid::Uuid;

pub async fn trace_middleware(request: Request, next: Next) -> Response {
    let trace_id = request
        .headers()
        .get("X-Request-ID")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let span = tracing::info_span!(
        "http_request",
        method = %request.method(),
        path = %request.uri().path(),
        trace_id = %trace_id,
    );

    let mut response = next.run(request).instrument(span).await;
    response
        .headers_mut()
        .insert("X-Request-ID", trace_id.parse().unwrap());
    response
}
