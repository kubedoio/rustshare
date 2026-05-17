//! Admin panel handlers. All routes require `AdminUser` extractor (is_admin = true).

pub mod audit;
pub mod config;
pub mod groups;
pub mod modules;
pub mod templates;
pub mod users;
pub mod webhooks;
pub mod workflows;

use sqlx::PgPool;
use uuid::Uuid;

/// Record an admin action in the `admin_actions` table.
/// Errors are logged as warnings — audit failures must not block the admin operation.
pub async fn log_admin_action(
    pool: &PgPool,
    actor_id: Uuid,
    action_type: &str,
    target_type: Option<&str>,
    target_id: Option<Uuid>,
    detail: serde_json::Value,
) {
    let result = sqlx::query!(
        r#"
        INSERT INTO admin_actions (actor_id, action_type, target_type, target_id, detail)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        actor_id,
        action_type,
        target_type,
        target_id,
        detail
    )
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::warn!(
            actor_id = %actor_id,
            action_type = action_type,
            target_id = ?target_id,
            "Failed to log admin action: {:?}", e
        );
    }
}

use crate::handlers::AppError;

pub fn admin_not_found(msg: impl Into<String>) -> AppError {
    AppError::not_found(msg)
}

pub fn admin_bad_request(msg: impl Into<String>) -> AppError {
    AppError::bad_request(msg)
}

pub fn admin_conflict(msg: impl Into<String>) -> AppError {
    AppError::conflict(msg)
}

pub fn admin_internal_error(msg: impl Into<String>) -> AppError {
    AppError::internal(msg)
}
