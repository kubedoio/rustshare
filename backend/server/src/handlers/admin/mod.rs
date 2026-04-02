//! Admin panel handlers. All routes require `AdminUser` extractor (is_admin = true).

pub mod audit;
pub mod config;
pub mod groups;
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
    let result = sqlx::query(
        r#"
        INSERT INTO admin_actions (actor_id, action_type, target_type, target_id, detail)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(actor_id)
    .bind(action_type)
    .bind(target_type)
    .bind(target_id)
    .bind(detail)
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
