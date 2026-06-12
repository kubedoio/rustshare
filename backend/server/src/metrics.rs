use axum::{
    extract::State,
    http::{header, header::HeaderMap, StatusCode},
    response::IntoResponse,
};
use constant_time_eq::constant_time_eq;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;

use crate::state::AppState;

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

pub fn init_metrics() -> PrometheusHandle {
    PROMETHEUS_HANDLE
        .get_or_init(|| {
            PrometheusBuilder::new()
                .install_recorder()
                .expect("Failed to install Prometheus recorder")
        })
        .clone()
}

pub async fn metrics_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    // If a metrics API token is configured, require it in the Authorization
    // header. This lets Prometheus scrapers authenticate while preventing
    // accidental public exposure of internal metrics.
    if let Ok(expected_token) = std::env::var("METRICS_API_TOKEN") {
        if !expected_token.is_empty() {
            let provided = headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");

            const BEARER_PREFIX: &str = "Bearer ";
            let provided_token = if let Some(token) = provided.strip_prefix(BEARER_PREFIX) {
                token
            } else {
                provided
            };

            if !constant_time_eq(provided_token.as_bytes(), expected_token.as_bytes()) {
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    Ok((
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.prometheus_handle.render(),
    ))
}
