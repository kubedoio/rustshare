//! Admin unified audit log handler.

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
// Internal row type
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct AuditRow {
    id: Uuid,
    occurred_at: chrono::DateTime<chrono::Utc>,
    event_type: String,
    actor_label: String,
    action_type: String,
    target_label: Option<String>,
    detail: serde_json::Value,
    #[allow(dead_code)]
    actor_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/audit
pub async fn list_audit_log(
    State(state): State<AppState>,
    AdminUser { user_id: _ }: AdminUser,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<PaginatedAuditLog>, (StatusCode, Json<serde_json::Value>)> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).min(100).max(1);
    let offset = (page - 1) * per_page;

    let event_type_filter = query.event_type.as_deref().unwrap_or("all");

    // Determine which branches to include based on the type filter.
    let include_share_access = matches!(event_type_filter, "all" | "share_access");
    let include_security_event = matches!(event_type_filter, "all" | "security_event");
    let include_admin_action = matches!(event_type_filter, "all" | "admin_action");

    // When user_id filter is active, share_access rows have no actor_id, so they
    // are always excluded.
    let user_id_active = query.user_id.is_some();
    let effective_share_access = include_share_access && !user_id_active;

    // Build the UNION branches ------------------------------------------------

    // Each branch selects:
    //   id, occurred_at, event_type, actor_label, action_type, target_label, detail, actor_id
    // We collect them into a Vec<&str> and join with UNION ALL.

    let mut branches: Vec<String> = Vec::new();

    if effective_share_access {
        branches.push(
            "SELECT
                sal.id,
                sal.accessed_at AS occurred_at,
                'share_access'::text AS event_type,
                COALESCE(sal.actor_label, 'anonymous')::text AS actor_label,
                sal.action::text AS action_type,
                sal.share_id::text AS target_label,
                json_build_object('ip_address', sal.ip_address::text, 'success', sal.success) AS detail,
                NULL::uuid AS actor_id
            FROM share_access_log sal"
                .to_string(),
        );
    }

    if include_security_event {
        branches.push(
            "SELECT
                use2.id,
                use2.occurred_at,
                'security_event'::text AS event_type,
                COALESCE(u.username, 'deleted_user')::text AS actor_label,
                use2.event_type::text AS action_type,
                NULL::text AS target_label,
                json_build_object('description', use2.description)::jsonb AS detail,
                use2.user_id AS actor_id
            FROM user_security_events use2
            LEFT JOIN users u ON u.id = use2.user_id"
                .to_string(),
        );
    }

    if include_admin_action {
        branches.push(
            "SELECT
                aa.id,
                aa.performed_at AS occurred_at,
                'admin_action'::text AS event_type,
                COALESCE(u.username, 'deleted_user')::text AS actor_label,
                aa.action_type::text AS action_type,
                aa.target_id::text AS target_label,
                COALESCE(aa.detail, '{}'::jsonb) AS detail,
                aa.actor_id AS actor_id
            FROM admin_actions aa
            LEFT JOIN users u ON u.id = aa.actor_id"
                .to_string(),
        );
    }

    // If no branches are active, return empty result immediately.
    if branches.is_empty() {
        return Ok(Json(PaginatedAuditLog {
            entries: vec![],
            total: 0,
            page,
            per_page,
        }));
    }

    let union_sql = branches.join("\nUNION ALL\n");
    let cte_sql = format!("WITH all_events AS (\n{}\n)", union_sql);

    // Build WHERE clause and bind parameters ----------------------------------
    // We use positional parameters ($1, $2, ...) and track next index.
    let mut where_parts: Vec<String> = Vec::new();
    // bind_index starts at 1; we'll increment as we add each parameter.
    let mut bind_index: u32 = 1;

    // user_id filter — applied to actor_id column
    let user_id_bind_pos = if user_id_active {
        let pos = bind_index;
        where_parts.push(format!("actor_id = ${}", pos));
        bind_index += 1;
        Some(pos)
    } else {
        None
    };

    // from filter
    let from_bind_pos = if query.from.is_some() {
        let pos = bind_index;
        where_parts.push(format!("occurred_at >= ${}", pos));
        bind_index += 1;
        Some(pos)
    } else {
        None
    };

    // to filter
    let to_bind_pos = if query.to.is_some() {
        let pos = bind_index;
        where_parts.push(format!("occurred_at <= ${}", pos));
        bind_index += 1;
        Some(pos)
    } else {
        None
    };

    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_parts.join(" AND "))
    };

    // LIMIT / OFFSET bind positions
    let limit_pos = bind_index;
    bind_index += 1;
    let offset_pos = bind_index;

    // Build final queries -----------------------------------------------------
    let count_sql = format!(
        "{cte_sql}\nSELECT COUNT(*) FROM all_events {where_clause}"
    );

    let select_sql = format!(
        "{cte_sql}
SELECT id, occurred_at, event_type, actor_label, action_type, target_label, detail, actor_id
FROM all_events
{where_clause}
ORDER BY occurred_at DESC
LIMIT ${limit_pos} OFFSET ${offset_pos}"
    );

    // Helper macro: bind all optional parameters in the right order
    macro_rules! bind_params {
        ($q:expr) => {{
            let mut q = $q;
            if let Some(_pos) = user_id_bind_pos {
                q = q.bind(query.user_id.unwrap());
            }
            if let Some(_pos) = from_bind_pos {
                q = q.bind(query.from.unwrap());
            }
            if let Some(_pos) = to_bind_pos {
                q = q.bind(query.to.unwrap());
            }
            q
        }};
    }

    // COUNT query
    let count_query = bind_params!(sqlx::query_scalar::<_, i64>(&count_sql));
    let total: i64 = count_query
        .fetch_one(&state.db_pool)
        .await
        .map_err(db_error)?;

    // SELECT query
    let select_query = bind_params!(sqlx::query_as::<_, AuditRow>(&select_sql))
        .bind(per_page)
        .bind(offset);
    let rows: Vec<AuditRow> = select_query
        .fetch_all(&state.db_pool)
        .await
        .map_err(db_error)?;

    let entries = rows
        .into_iter()
        .map(|row| AuditEntry {
            id: row.id.to_string(),
            occurred_at: row.occurred_at,
            event_type: row.event_type,
            actor_label: row.actor_label,
            action_type: row.action_type,
            target_label: row.target_label,
            detail: row.detail,
        })
        .collect();

    Ok(Json(PaginatedAuditLog {
        entries,
        total,
        page,
        per_page,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn db_error(e: sqlx::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("Database error: {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "Database error" })),
    )
}
