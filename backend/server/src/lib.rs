//! RustShare Server Library
//!
//! This crate provides both the server binary and reusable server components.

pub mod authz;
pub mod bootstrap;
pub mod buzz_bridge;
pub mod buzz_gateway;
pub mod buzz_observation;
pub mod calendar_import_worker;
pub mod calendar_sync_worker;
pub mod config;
#[cfg(test)]
mod dotenv_isolation;
pub mod handlers;
pub mod mail_import_worker;
pub mod memory_files_reconcile_worker;
pub mod memory_orphan_cleanup_worker;
pub mod memory_projection;
pub mod metrics;
pub mod middleware;
pub mod object_gc;
pub mod object_gc_reconcile;
pub mod object_gc_scan;
pub mod oidc;
pub mod oidc_runtime;
pub mod openapi;
pub mod outbox_dispatcher;
pub mod replication;
pub mod replication_handlers;
pub mod retention;
pub mod routes;
pub mod services;
pub mod state;
#[doc(hidden)]
pub mod test_guard;
pub mod trash_cleanup;
pub mod user_disable;
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
