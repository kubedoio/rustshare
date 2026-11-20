//! Safe projection of audit detail fields for user-visible admin audit records.

use uuid::Uuid;

pub(crate) fn project_audit_detail(
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
            "group.member_added" | "group.member_removed" => {
                copy_uuid(fields, &mut projected, "user_id");
            }
            "user.created" | "user.invited" => {
                copy_safe_identifier(fields, &mut projected, "username");
            }
            "invite.sibling_revoked_on_acceptance" => {
                copy_safe_identifier(fields, &mut projected, "reason");
            }
            "share.created" | "share.revoked" => {
                copy_uuid(fields, &mut projected, "share_id");
                copy_safe_identifier(fields, &mut projected, "permissions");
                copy_bool(fields, &mut projected, "password_protected");
            }
            "user.quota_changed" => {
                copy_nonnegative_integer(fields, &mut projected, "old_quota");
                copy_nonnegative_integer(fields, &mut projected, "new_quota");
            }
            "user.admin_status_changed" => {
                copy_bool(fields, &mut projected, "old_is_admin");
                copy_bool(fields, &mut projected, "new_is_admin");
            }
            "organization_admin.bootstrap"
            | "organization_admin.granted"
            | "organization_admin.revoked" => {
                copy_uuid(fields, &mut projected, "grant_id");
                if action_type == "organization_admin.revoked"
                    && fields.get("actor_kind").and_then(serde_json::Value::as_str) == Some("scim")
                {
                    projected.insert(
                        "actor_kind".to_string(),
                        serde_json::Value::String("scim".to_string()),
                    );
                }
            }
            "organization_admin.invite_accepted"
            | "organization_admin.invite_revoked_group_unavailable" => {
                copy_uuid_array(fields, &mut projected, "group_ids");
                if action_type == "organization_admin.invite_accepted" {
                    copy_uuid(fields, &mut projected, "accepted_by_user_id");
                }
            }
            "organization_admin.expired_invites_replaced" => {
                copy_nonnegative_integer(fields, &mut projected, "count");
            }
            "organization_admin.invitation_cancelled" => {
                copy_nonnegative_integer(fields, &mut projected, "group_assignments_removed");
            }
            "organization_admin.invitation_resend_recovered" => {
                copy_nonnegative_integer(fields, &mut projected, "count");
            }
            "organization_admin.invitation_resend_requested"
            | "organization_admin.invitation_resend_sent"
            | "organization_admin.invitation_resend_failed"
            | "organization_admin.invitation_resend_superseded" => {}
            "organization_admin.recovered" => {
                copy_uuid(fields, &mut projected, "grant_id");
                if matches!(
                    fields
                        .get("recovery_reason")
                        .and_then(serde_json::Value::as_str),
                    Some(
                        "administrator_unavailable"
                            | "security_incident"
                            | "administrative_continuity"
                    )
                ) {
                    projected.insert(
                        "recovery_reason".to_string(),
                        fields["recovery_reason"].clone(),
                    );
                }
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

fn copy_uuid_array(
    fields: &serde_json::Map<String, serde_json::Value>,
    projected: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
) {
    if let Some(values) = fields.get(key).and_then(serde_json::Value::as_array) {
        let values: Vec<_> = values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter(|value| Uuid::parse_str(value).is_ok())
            .map(str::to_owned)
            .collect();
        projected.insert(key.to_string(), serde_json::json!(values));
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
