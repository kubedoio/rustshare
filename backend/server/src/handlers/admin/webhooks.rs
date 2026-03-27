//! Admin webhook management handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use rustshare_storage::metadata_v2::schemas::{WebhookDocument, WebhookFilter};

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
    #[allow(dead_code)]
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
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<Vec<WebhookResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let filter = WebhookFilter::new();
    
    let webhooks = state.webhook_repo
        .list(filter)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list webhooks: {}", e);
            internal_error("Failed to list webhooks")
        })?;

    let responses: Vec<WebhookResponse> = webhooks
        .into_iter()
        .map(|w| WebhookResponse {
            id: w.id.to_string(),
            name: w.name,
            url: w.url,
            secret: w.secret_hash.map(|_| "***".to_string()),
            enabled: w.enabled,
            events: w.events,
            created_by: Some(w.created_by.to_string()),
            created_at: w.created_at,
            updated_at: w.updated_at,
        })
        .collect();

    Ok(Json(responses))
}

/// POST /api/v1/admin/integrations/webhooks
pub async fn create_webhook(
    State(state): State<AppState>,
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

    let webhook_id = Uuid::new_v4();
    
    // Hash the secret if provided
    let secret_hash = req.secret.filter(|s| !s.is_empty()).map(|s| {
        // TODO: Use proper hashing
        format!("hash:{}", s)
    });

    let webhook = WebhookDocument::new(
        webhook_id,
        req.name.trim().to_string(),
        req.url.trim().to_string(),
        secret_hash,
        req.events,
        actor_id,
    );

    state.webhook_repo
        .create(&webhook)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create webhook: {}", e);
            internal_error("Failed to create webhook")
        })?;

    log_admin_action(
        &State(state),
        actor_id,
        "webhook.created",
        Some("webhook"),
        Some(webhook_id),
        json!({"name": webhook.name, "url": webhook.url}),
    )
    .await;

    Ok((StatusCode::CREATED, Json(WebhookResponse {
        id: webhook.id.to_string(),
        name: webhook.name,
        url: webhook.url,
        secret: webhook.secret_hash.map(|_| "***".to_string()),
        enabled: webhook.enabled,
        events: webhook.events,
        created_by: Some(webhook.created_by.to_string()),
        created_at: webhook.created_at,
        updated_at: webhook.updated_at,
    })))
}

/// PATCH /api/v1/admin/integrations/webhooks/:id
pub async fn update_webhook(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(webhook_id): Path<Uuid>,
    Json(req): Json<UpdateWebhookRequest>,
) -> Result<Json<WebhookResponse>, (StatusCode, Json<serde_json::Value>)> {
    if let Some(ref events) = req.events {
        validate_events(events)?;
    }

    let mut webhook = state.webhook_repo
        .get(webhook_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get webhook: {}", e);
            internal_error("Failed to update webhook")
        })?
        .ok_or_else(|| not_found("Webhook not found"))?;

    // Apply updates
    let new_name = req.name.unwrap_or_else(|| webhook.name.clone());
    let new_url = req.url.unwrap_or_else(|| webhook.url.clone());
    let new_events = req.events.unwrap_or_else(|| webhook.events.clone());
    let new_enabled = req.enabled.unwrap_or(webhook.enabled);

    // Handle secret update
    let new_secret = match req.secret {
        Some(ref s) if s.is_empty() => None, // Clear secret
        Some(s) => Some(format!("hash:{}", s)), // New secret
        None => webhook.secret_hash.clone(), // Keep existing
    };

    webhook.update(
        Some(new_name),
        Some(new_url),
        Some(new_events),
        Some(new_enabled),
    );
    webhook.secret_hash = new_secret;

    state.webhook_repo
        .update(&webhook)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update webhook: {}", e);
            internal_error("Failed to update webhook")
        })?;

    log_admin_action(
        &State(state),
        actor_id,
        "webhook.updated",
        Some("webhook"),
        Some(webhook_id),
        json!({"name": webhook.name, "url": webhook.url}),
    )
    .await;

    Ok(Json(WebhookResponse {
        id: webhook.id.to_string(),
        name: webhook.name,
        url: webhook.url,
        secret: webhook.secret_hash.map(|_| "***".to_string()),
        enabled: webhook.enabled,
        events: webhook.events,
        created_by: Some(webhook.created_by.to_string()),
        created_at: webhook.created_at,
        updated_at: webhook.updated_at,
    }))
}

/// DELETE /api/v1/admin/integrations/webhooks/:id
pub async fn delete_webhook(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(webhook_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    // Verify webhook exists
    let _webhook = state.webhook_repo
        .get(webhook_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get webhook: {}", e);
            internal_error("Failed to delete webhook")
        })?
        .ok_or_else(|| not_found("Webhook not found"))?;

    state.webhook_repo
        .delete(webhook_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete webhook: {}", e);
            internal_error("Failed to delete webhook")
        })?;

    log_admin_action(
        &State(state),
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
pub async fn test_webhook(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(webhook_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Verify webhook exists
    let _webhook = state.webhook_repo
        .get(webhook_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get webhook: {}", e);
            internal_error("Failed to test webhook")
        })?
        .ok_or_else(|| not_found("Webhook not found"))?;

    // TODO: Implement webhook test (send ping event)
    tracing::warn!("Webhook test not yet implemented");
    
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "status": "not_implemented",
            "message": "Webhook test not yet available"
        })),
    ))
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
