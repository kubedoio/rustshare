//! Admin unified audit log handler.

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{admin_bad_request, admin_internal_error};
use crate::{
    handlers::{AdminUser, AppError},
    AppState,
};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
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
#[utoipa::path(
    get,
    path = "/api/v1/admin/audit",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 400, description = "Invalid audit query parameters", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_audit_log(
    State(state): State<AppState>,
    AdminUser { user_id: _ }: AdminUser,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<PaginatedAuditLog>, AppError> {
    let (page, per_page, offset) = audit_pagination_parameters(query.page, query.per_page)
        .ok_or_else(|| admin_bad_request("page produces an offset outside the supported range"))?;

    let event_type_filter = query.event_type.as_deref().unwrap_or("all");

    // Fix 1: Validate the type parameter — reject unknown values immediately.
    if !matches!(
        event_type_filter,
        "all" | "share_access" | "security_event" | "admin_action"
    ) {
        return Err(admin_bad_request(
            "Invalid type filter. Must be one of: share_access, security_event, admin_action, all",
        ));
    }

    // Fix 2: Reject the combination of type=share_access with user_id filter,
    // since share_access_log has no user UUID column.
    if event_type_filter == "share_access" && query.user_id.is_some() {
        return Err(admin_bad_request(
            "user_id filter cannot be combined with type=share_access",
        ));
    }

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
                json_build_object('success', sal.success)::jsonb AS detail,
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
                '{}'::jsonb AS detail,
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
    let _user_id_bind_pos = if user_id_active {
        let pos = bind_index;
        where_parts.push(format!("actor_id = ${}", pos));
        bind_index += 1;
        Some(pos)
    } else {
        None
    };

    // from filter
    let _from_bind_pos = if query.from.is_some() {
        let pos = bind_index;
        where_parts.push(format!("occurred_at >= ${}", pos));
        bind_index += 1;
        Some(pos)
    } else {
        None
    };

    // to filter
    let _to_bind_pos = if query.to.is_some() {
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
    // bind_index ends here; LIMIT and OFFSET are appended after bind_params! macro in select query only

    // Build final queries -----------------------------------------------------
    let count_sql = format!("{cte_sql}\nSELECT COUNT(*) FROM all_events {where_clause}");

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
            if let Some(user_id) = query.user_id {
                q = q.bind(user_id);
            }
            if let Some(from) = query.from {
                q = q.bind(from);
            }
            if let Some(to) = query.to {
                q = q.bind(to);
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

    // A page beyond the current end needs no SELECT; avoid asking PostgreSQL to
    // walk and discard an arbitrarily large offset.
    let rows: Vec<AuditRow> = if offset >= total {
        Vec::new()
    } else {
        let select_query = bind_params!(sqlx::query_as::<_, AuditRow>(&select_sql))
            .bind(per_page)
            .bind(offset);
        select_query
            .fetch_all(&state.db_pool)
            .await
            .map_err(db_error)?
    };

    let entries = rows
        .into_iter()
        .map(|row| {
            let detail = project_audit_detail(&row.event_type, &row.action_type, &row.detail);
            AuditEntry {
                id: row.id.to_string(),
                occurred_at: row.occurred_at,
                event_type: row.event_type,
                actor_label: row.actor_label,
                action_type: row.action_type,
                target_label: row.target_label,
                detail,
            }
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

fn audit_pagination_parameters(
    page: Option<i64>,
    per_page: Option<i64>,
) -> Option<(i64, i64, i64)> {
    let page = page.unwrap_or(1).max(1);
    let per_page = per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1).checked_mul(per_page)?;
    Some((page, per_page, offset))
}

fn db_error(e: sqlx::Error) -> AppError {
    tracing::error!("Database error: {:?}", e);
    admin_internal_error("Database error")
}

fn project_audit_detail(
    event_type: &str,
    action_type: &str,
    detail: &serde_json::Value,
) -> serde_json::Value {
    use serde_json::{json, Map, Value};

    let Some(fields) = detail.as_object() else {
        return json!({});
    };
    let mut projected = Map::new();

    match event_type {
        "share_access" => copy_bool(fields, &mut projected, "success"),
        // Descriptions are free-form and may contain user or session data.
        "security_event" => {}
        "admin_action" => match action_type {
            "group.created" | "group.updated" | "group.deleted" => {
                copy_safe_display_text(fields, &mut projected, "name");
            }
            "user.quota_changed" => {
                copy_nonnegative_integer(fields, &mut projected, "old_quota");
                copy_nonnegative_integer(fields, &mut projected, "new_quota");
            }
            "user.admin_status_changed" => {
                copy_bool(fields, &mut projected, "old_is_admin");
                copy_bool(fields, &mut projected, "new_is_admin");
            }
            "user.deleted" => {
                copy_nonnegative_integer(fields, &mut projected, "storage_keys_count")
            }
            "config.security_updated" => {
                copy_bool(fields, &mut projected, "login_protection_enabled");
                copy_nonnegative_integer(fields, &mut projected, "max_login_attempts");
                copy_nonnegative_integer(fields, &mut projected, "login_block_duration_minutes");
            }
            "application.enabled" | "application.disabled" | "application.updated" => {
                copy_safe_identifier(fields, &mut projected, "application_id");
            }
            "group.member_added" | "group.member_removed" => {
                copy_uuid(fields, &mut projected, "user_id");
            }
            "template.created" | "template.updated" => {
                copy_safe_identifier(fields, &mut projected, "template_key");
                copy_safe_identifier(fields, &mut projected, "application_id");
            }
            "template.deleted" => copy_safe_identifier(fields, &mut projected, "key"),
            "template.duplicated" => {
                copy_safe_identifier(fields, &mut projected, "original_key");
                copy_safe_identifier(fields, &mut projected, "new_key");
                copy_uuid(fields, &mut projected, "new_id");
            }
            "object.created.from_template" => {
                copy_safe_identifier(fields, &mut projected, "template_key");
                copy_uuid(fields, &mut projected, "object_id");
            }
            _ => {}
        },
        _ => {}
    }

    Value::Object(projected)
}

fn copy_bool(
    source: &serde_json::Map<String, serde_json::Value>,
    destination: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) {
    if let Some(value) = source.get(key).and_then(serde_json::Value::as_bool) {
        destination.insert(key.to_string(), serde_json::Value::Bool(value));
    }
}

fn copy_nonnegative_integer(
    source: &serde_json::Map<String, serde_json::Value>,
    destination: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) {
    if let Some(value) = source.get(key).and_then(serde_json::Value::as_u64) {
        destination.insert(key.to_string(), serde_json::json!(value));
    }
}

fn copy_uuid(
    source: &serde_json::Map<String, serde_json::Value>,
    destination: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) {
    if let Some(value) = source
        .get(key)
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
    {
        destination.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
}

fn copy_safe_identifier(
    source: &serde_json::Map<String, serde_json::Value>,
    destination: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) {
    let Some(value) = source.get(key).and_then(serde_json::Value::as_str) else {
        return;
    };
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return;
    }
    destination.insert(
        key.to_string(),
        serde_json::Value::String(value.to_string()),
    );
}

fn copy_safe_display_text(
    source: &serde_json::Map<String, serde_json::Value>,
    destination: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) {
    let Some(value) = source.get(key).and_then(serde_json::Value::as_str) else {
        return;
    };
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return;
    }
    destination.insert(
        key.to_string(),
        serde_json::Value::String(value.to_string()),
    );
}

#[cfg(test)]
mod tests {
    use super::{audit_pagination_parameters, project_audit_detail};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn audit_pagination_parameters_default_and_clamp_inputs() {
        assert_eq!(audit_pagination_parameters(None, None), Some((1, 20, 0)));
        assert_eq!(
            audit_pagination_parameters(Some(0), Some(0)),
            Some((1, 1, 0))
        );
        assert_eq!(
            audit_pagination_parameters(Some(2), Some(1000)),
            Some((2, 100, 100))
        );
    }

    #[test]
    fn audit_pagination_parameters_reject_offset_overflow() {
        assert_eq!(audit_pagination_parameters(Some(i64::MAX), Some(100)), None);
    }

    #[test]
    fn admin_action_detail_keeps_only_typed_allowlisted_fields() {
        let member_id = Uuid::new_v4();
        let detail = json!({
            "user_id": member_id,
            "access_token": "do-not-return",
            "note_body": "private content"
        });

        assert_eq!(
            project_audit_detail("admin_action", "group.member_added", &detail),
            json!({ "user_id": member_id.to_string() })
        );
    }

    #[test]
    fn security_event_detail_omits_free_form_description() {
        let detail = json!({ "description": "secret text", "ip_address": "192.0.2.1" });

        assert_eq!(
            project_audit_detail("security_event", "login.failed", &detail),
            json!({})
        );
    }

    #[test]
    fn share_access_detail_keeps_success_but_omits_network_metadata() {
        let detail = json!({
            "success": true,
            "ip_address": "192.0.2.1",
            "user_agent": "private-agent"
        });

        assert_eq!(
            project_audit_detail("share_access", "download", &detail),
            json!({ "success": true })
        );
    }

    #[test]
    fn object_creation_detail_never_returns_user_path() {
        let object_id = Uuid::new_v4();
        let detail = json!({
            "template_key": "template_default",
            "object_id": object_id,
            "path": "/Workspace/private/notes.md"
        });

        assert_eq!(
            project_audit_detail("admin_action", "object.created.from_template", &detail),
            json!({
                "template_key": "template_default",
                "object_id": object_id.to_string()
            })
        );
    }

    #[test]
    fn unknown_admin_action_detail_fails_closed() {
        let detail = json!({ "arbitrary": "private", "secret": "do-not-return" });

        assert_eq!(
            project_audit_detail("admin_action", "future.action", &detail),
            json!({})
        );
    }

    #[test]
    fn group_lifecycle_detail_keeps_only_bounded_display_name() {
        for action in ["group.created", "group.updated", "group.deleted"] {
            assert_eq!(
                project_audit_detail(
                    "admin_action",
                    action,
                    &json!({
                        "name": "FWS Pilot Team",
                        "extra": "unapproved detail"
                    })
                ),
                json!({ "name": "FWS Pilot Team" })
            );
        }

        assert_eq!(
            project_audit_detail(
                "admin_action",
                "group.deleted",
                &json!({ "name": "unsafe\nname" })
            ),
            json!({})
        );
    }

    #[test]
    fn mismatched_event_type_does_not_expose_admin_action_metadata() {
        assert_eq!(
            project_audit_detail(
                "security_event",
                "group.deleted",
                &json!({ "name": "FWS Pilot Team" })
            ),
            json!({})
        );
    }

    #[test]
    fn unknown_event_type_and_malformed_detail_fail_closed() {
        assert_eq!(
            project_audit_detail(
                "future_event",
                "group.deleted",
                &json!({ "name": "FWS Pilot Team", "secret": "do-not-return" })
            ),
            json!({})
        );
        assert_eq!(
            project_audit_detail("admin_action", "group.deleted", &json!("not an object")),
            json!({})
        );
    }
}
