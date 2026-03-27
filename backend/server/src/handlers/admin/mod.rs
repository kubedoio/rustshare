//! Admin panel handlers. All routes require `AdminUser` extractor (is_admin = true).

pub mod audit;
pub mod config;
pub mod groups;
pub mod users;
pub mod webhooks;

use uuid::Uuid;

use crate::AppState;

/// Record an admin action in the audit log.
/// Errors are logged as warnings — audit failures must not block the admin operation.
pub async fn log_admin_action(
    state: &AppState,
    actor_id: Uuid,
    action_type: &str,
    target_type: Option<&str>,
    target_id: Option<Uuid>,
    detail: serde_json::Value,
) {
    // Get actor info for the label
    let actor_label = match state.user_metadata_repo.get(actor_id.into()).await {
        Ok(Some(user)) => user.email,
        _ => actor_id.to_string(),
    };

    let target_label = if let Some(tid) = target_id {
        match target_type {
            Some("user") => state.user_metadata_repo.get(tid.into()).await.ok().flatten().map(|u| u.email),
            Some("group") => state.group_repo.get(tid).await.ok().flatten().map(|g| g.name),
            Some("webhook") => state.webhook_repo.get(tid).await.ok().flatten().map(|w| w.name),
            _ => None,
        }
    } else {
        None
    };

    let entry = rustshare_storage::metadata_v2::schemas::AuditLogEntryDocument::new(
        actor_id,
        actor_label,
        action_type.to_string(),
        target_type.map(|t| t.to_string()),
        target_id,
        target_label,
        detail,
    );

    if let Err(e) = state.audit_repo.append(&entry).await {
        tracing::warn!("Failed to write audit log entry: {}", e);
    }
}
