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

use axum::{http::StatusCode, response::IntoResponse, Json};
use crate::handlers::ErrorResponse;

pub fn admin_ok<T>(data: T) -> axum::response::Response
where
    T: serde::Serialize,
{
    (StatusCode::OK, Json(data)).into_response()
}

pub fn admin_not_found(msg: impl Into<String>) -> axum::response::Response {
    (StatusCode::NOT_FOUND, Json(ErrorResponse::new(msg))).into_response()
}

pub fn admin_bad_request(msg: impl Into<String>) -> axum::response::Response {
    (StatusCode::BAD_REQUEST, Json(ErrorResponse::new(msg))).into_response()
}

pub fn admin_conflict(msg: impl Into<String>) -> axum::response::Response {
    (StatusCode::CONFLICT, Json(ErrorResponse::new(msg))).into_response()
}

pub fn admin_internal_error(msg: impl Into<String>) -> axum::response::Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(msg))).into_response()
}
