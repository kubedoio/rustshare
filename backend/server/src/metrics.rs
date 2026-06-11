use axum::{extract::State, http::header::CONTENT_TYPE, response::IntoResponse};
use metrics_exporter_prometheus::PrometheusBuilder;

use crate::state::AppState;

pub fn init_metrics() -> metrics_exporter_prometheus::PrometheusHandle {
    PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install Prometheus recorder")
}

pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.prometheus_handle.render(),
    )
}
