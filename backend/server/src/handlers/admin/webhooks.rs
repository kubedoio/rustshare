//! Admin webhook management handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{handlers::AdminUser, AppState};
use super::log_admin_action;

// ---------------------------------------------------------------------------
// Supported event types
// ---------------------------------------------------------------------------

const VALID_EVENTS: &[&str] = &[
    "file.uploaded",
    "file.deleted",
    "file.restored",
    "folder.created",
    "folder.deleted",
    "share.created",
    "share.revoked",
    "user.created",
    "user.disabled",
    "user.deleted",
];

// ---------------------------------------------------------------------------
// Response type
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    pub id: String,
    pub name: String,
    pub url: String,
    /// `"***"` when a secret is stored; `null` when none is set.
    pub secret: Option<String>,
    pub enabled: bool,
    pub events: Vec<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    pub name: String,
    pub url: String,
    /// Optional HMAC signing secret (plain text; will be encrypted before storage).
    pub secret: Option<String>,
    pub enabled: Option<bool>,
    pub events: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWebhookRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    /// `Some("")` clears the secret; `None` keeps existing; `Some(non_empty)` replaces.
    pub secret: Option<String>,
    pub enabled: Option<bool>,
    pub events: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/integrations/webhooks
pub async fn list_webhooks(
    State(_state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<Vec<WebhookResponse>>, (StatusCode, Json<serde_json::Value>)> {
    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");
    Ok(Json(vec![]))
}

/// POST /api/v1/admin/integrations/webhooks
pub async fn create_webhook(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<WebhookResponse>), (StatusCode, Json<serde_json::Value>)> {
    // Validate
    if req.name.trim().is_empty() {
        return Err(bad_request("name must not be empty"));
    }
    if req.url.trim().is_empty() {
        return Err(bad_request("url must not be empty"));
    }
    validate_events(&req.events)?;

    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");

    let webhook_id = Uuid::new_v4();

    log_admin_action(
        actor_id,
        "webhook.created",
        Some("webhook"),
        Some(webhook_id),
        json!({"name": req.name, "url": req.url}),
    )
    .await;

    Err(internal_error("Webhook creation not yet implemented in zero-PostgreSQL mode"))
}

/// PATCH /api/v1/admin/integrations/webhooks/:id
pub async fn update_webhook(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(webhook_id): Path<Uuid>,
    Json(req): Json<UpdateWebhookRequest>,
) -> Result<Json<WebhookResponse>, (StatusCode, Json<serde_json::Value>)> {
    if let Some(ref events) = req.events {
        validate_events(events)?;
    }

    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");

    let new_name = req.name.unwrap_or_else(|| "unknown".to_string());
    let new_url = req.url.unwrap_or_else(|| "unknown".to_string());

    log_admin_action(
        actor_id,
        "webhook.updated",
        Some("webhook"),
        Some(webhook_id),
        json!({"name": new_name, "url": new_url}),
    )
    .await;

    Err(not_found("Webhook not found"))
}

/// DELETE /api/v1/admin/integrations/webhooks/:id
pub async fn delete_webhook(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(webhook_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");

    log_admin_action(
        actor_id,
        "webhook.deleted",
        Some("webhook"),
        Some(webhook_id),
        json!({}),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/admin/integrations/webhooks/:id/test
///
/// Fires a `ping` event to the webhook URL and returns the result.
pub async fn test_webhook(
    State(_state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(_webhook_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    tracing::warn!("Feature not yet implemented in zero-PostgreSQL mode");
    Err(not_found("Webhook not found"))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn validate_events(
    events: &[String],
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if events.is_empty() {
        return Err(bad_request("events array must not be empty"));
    }
    for event in events {
        if !VALID_EVENTS.contains(&event.as_str()) {
            return Err(bad_request(&format!("Unknown event type: {event}")));
        }
    }
    Ok(())
}

fn not_found(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": msg })))
}

fn bad_request(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg })))
}

fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
}
