//! Admin unified audit log handler.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use rustshare_storage::metadata_v2::schemas::AuditFilter;

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
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/audit
pub async fn list_audit_log(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<PaginatedAuditLog>, (StatusCode, Json<serde_json::Value>)> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    // Build filter
    let mut filter = AuditFilter::new()
        .with_pagination((page - 1) * per_page, per_page);
    
    if let Some(actor_id) = query.user_id {
        filter.actor_id = Some(actor_id);
    }
    
    if let Some(action_type) = query.event_type {
        filter.action_type = Some(action_type);
    }
    
    if let Some(from) = query.from {
        filter.from = Some(from);
    }
    
    if let Some(to) = query.to {
        filter.to = Some(to);
    }

    let entries = state.audit_repo
        .list(filter)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list audit log: {}", e);
            internal_error("Failed to list audit log")
        })?;

    let total = entries.len() as i64; // Note: In production, use count query

    let audit_entries: Vec<AuditEntry> = entries
        .into_iter()
        .map(|e| AuditEntry {
            id: e.id.to_string(),
            occurred_at: e.occurred_at,
            event_type: e.action_type.clone(),
            actor_label: e.actor_label,
            action_type: e.action_type,
            target_label: e.target_label,
            detail: e.detail,
        })
        .collect();

    Ok(Json(PaginatedAuditLog {
        entries: audit_entries,
        total,
        page,
        per_page,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
}
