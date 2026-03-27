//! Admin unified audit log handler.
//!
//! TODO: This module needs to be rewritten to use the new AuditStore
//! for audit logging instead of PostgreSQL.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{handlers::AdminUser, AppState};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    /// Filter by event type: `share_access | security_event | admin_action | all`
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    /// Filter by actor user UUID
    pub user_id: Option<Uuid>,
    /// ISO timestamp lower bound (inclusive)
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    /// ISO timestamp upper bound (inclusive)
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AuditEntry {
    pub id: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "type")]
    pub event_type: String,
    pub actor_label: String,
    pub action_type: String,
    pub target_label: Option<String>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct PaginatedAuditLog {
    pub entries: Vec<AuditEntry>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/audit
/// 
/// TODO: Implement using new AuditStore
pub async fn list_audit_log(
    State(_state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<PaginatedAuditLog>, (StatusCode, Json<serde_json::Value>)> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    // TODO: Implement using new AuditStore
    tracing::warn!("Audit log not yet implemented in zero-PostgreSQL mode");

    // Return empty result for now
    Ok(Json(PaginatedAuditLog {
        entries: vec![],
        total: 0,
        page,
        per_page,
    }))
}
