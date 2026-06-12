use axum::{extract::MatchedPath, extract::Request, middleware::Next, response::Response};
use metrics::{counter, histogram};
use std::time::Instant;

pub async fn metrics_middleware(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().to_string();
    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched_path| matched_path.as_str().to_string())
        .unwrap_or_else(|| normalize_path(request.uri().path()));

    let response = next.run(request).await;

    let status = response.status().as_u16().to_string();
    let duration = start.elapsed().as_secs_f64();

    counter!(
        "http_requests_total",
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status
    )
    .increment(1);
    histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "path" => path
    )
    .record(duration);

    response
}

fn normalize_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if uuid::Uuid::parse_str(segment).is_ok() {
                "{id}"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}
