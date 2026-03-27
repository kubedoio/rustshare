//! Admin panel handlers. All routes require `AdminUser` extractor (is_admin = true).
//!
//! TODO: This module needs to be rewritten to use the new AuditStore
//! for audit logging instead of PostgreSQL.

pub mod audit;
pub mod config;
pub mod groups;
pub mod users;
pub mod webhooks;

use serde_json::json;
use uuid::Uuid;

/// Record an admin action in the audit log.
/// Errors are logged as warnings — audit failures must not block the admin operation.
///
/// TODO: Replace PostgreSQL audit logging with AuditStore (RustFS-based)
pub async fn log_admin_action(
    _actor_id: Uuid,
    _action_type: &str,
    _target_type: Option<&str>,
    _target_id: Option<Uuid>,
    _detail: serde_json::Value,
) {
    // TODO: Implement using AuditStore for audit logging
    // This requires rewriting to use RustFS instead of PostgreSQL
    
    tracing::warn!("Admin audit logging not yet implemented in zero-PostgreSQL mode");
}
