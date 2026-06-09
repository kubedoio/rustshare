//! RustShare Server Library
//!
//! This crate provides both the server binary and reusable server components.

pub mod adapters;
pub mod bootstrap;
pub mod handlers;
pub mod middleware;
pub mod oidc;
pub mod oidc_runtime;
pub mod openapi;
pub mod replication;
pub mod replication_handlers;
pub mod routes;
pub mod services;
pub mod state;
pub mod trash_cleanup;
pub mod web_session;

pub use bootstrap::default_storage_quota_bytes;
pub use state::{
    AppAiService, AppChatIntegrationService, AppState, AppUploadService, AppUserShareService,
};

use axum::Json;
use serde::Serialize;

/// Health check endpoint
pub(crate) async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

#[derive(Serialize)]
pub(crate) struct HealthResponse {
    status: String,
}
