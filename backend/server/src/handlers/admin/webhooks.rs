//! Admin webhook management handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use hmac::{Hmac, KeyInit, Mac};
use rustshare_crypto::{decrypt_secret, encrypt_secret};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use uuid::Uuid;

use super::{admin_bad_request, admin_internal_error, admin_not_found, log_admin_action};
use crate::{
    handlers::{AdminUser, AppError},
    AppState,
};

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
// Row type
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct WebhookRow {
    id: Uuid,
    name: String,
    url: String,
    secret_enc: Option<String>,
    enabled: bool,
    events: Vec<String>,
    created_by: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Response type
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
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

impl From<WebhookRow> for WebhookResponse {
    fn from(row: WebhookRow) -> Self {
        WebhookResponse {
            id: row.id.to_string(),
            name: row.name,
            url: row.url,
            secret: row.secret_enc.as_deref().map(|_| "***".to_string()),
            enabled: row.enabled,
            events: row.events,
            created_by: row.created_by.map(|u| u.to_string()),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateWebhookRequest {
    pub name: String,
    pub url: String,
    /// Optional HMAC signing secret (plain text; will be encrypted before storage).
    pub secret: Option<String>,
    pub enabled: Option<bool>,
    pub events: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
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

const COLS: &str = "id, name, url, secret_enc, enabled, events, created_by, created_at, updated_at";

/// GET /api/v1/admin/integrations/webhooks
#[utoipa::path(
    get,
    path = "/api/v1/admin/integrations/webhooks",
    tag = "Webhooks",
    responses(
        (status = 200, description = "List of configured webhooks", body = Vec<WebhookResponse>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_webhooks(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<Vec<WebhookResponse>>, AppError> {
    let rows = sqlx::query_as::<_, WebhookRow>(&format!(
        "SELECT {COLS} FROM webhook_configs ORDER BY created_at DESC"
    ))
    .fetch_all(&state.db_pool)
    .await
    .map_err(db_error)?;

    Ok(Json(rows.into_iter().map(WebhookResponse::from).collect()))
}

/// POST /api/v1/admin/integrations/webhooks
#[utoipa::path(
    post,
    path = "/api/v1/admin/integrations/webhooks",
    tag = "Webhooks",
    request_body = CreateWebhookRequest,
    responses(
        (status = 201, description = "Webhook created", body = WebhookResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn create_webhook(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<WebhookResponse>), AppError> {
    // Validate
    if req.name.trim().is_empty() {
        return Err(admin_bad_request("name must not be empty"));
    }
    if req.url.trim().is_empty() {
        return Err(admin_bad_request("url must not be empty"));
    }
    validate_events(&req.events)?;

    let secret_enc = encrypt_optional_secret(req.secret.as_deref(), &state)?;
    let enabled = req.enabled.unwrap_or(true);

    let row = sqlx::query_as::<_, WebhookRow>(&format!(
        "INSERT INTO webhook_configs (name, url, secret_enc, enabled, events, created_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {COLS}"
    ))
    .bind(&req.name)
    .bind(&req.url)
    .bind(&secret_enc)
    .bind(enabled)
    .bind(&req.events)
    .bind(actor_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(db_error)?;

    let webhook_id = row.id;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "webhook.created",
        Some("webhook"),
        Some(webhook_id),
        json!({"name": req.name, "url": req.url}),
    )
    .await;

    Ok((StatusCode::CREATED, Json(WebhookResponse::from(row))))
}

/// PATCH /api/v1/admin/integrations/webhooks/:id
#[utoipa::path(
    patch,
    path = "/api/v1/admin/integrations/webhooks/{id}",
    tag = "Webhooks",
    params(("id" = Uuid, Path, description = "Webhook ID")),
    request_body = UpdateWebhookRequest,
    responses(
        (status = 200, description = "Webhook updated", body = WebhookResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Webhook not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_webhook(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(webhook_id): Path<Uuid>,
    Json(req): Json<UpdateWebhookRequest>,
) -> Result<Json<WebhookResponse>, AppError> {
    // Fetch current
    let current = sqlx::query_as::<_, WebhookRow>(&format!(
        "SELECT {COLS} FROM webhook_configs WHERE id = $1"
    ))
    .bind(webhook_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("Webhook not found"))?;

    let new_name = req.name.as_deref().unwrap_or(&current.name).to_string();
    let new_url = req.url.as_deref().unwrap_or(&current.url).to_string();
    let new_enabled = req.enabled.unwrap_or(current.enabled);

    let new_events = if let Some(ref evts) = req.events {
        validate_events(evts)?;
        evts.clone()
    } else {
        current.events.clone()
    };

    // Determine new secret
    let new_secret_enc = match req.secret.as_deref() {
        None => current.secret_enc.clone(), // absent = keep
        Some("") => None,                   // empty string = clear
        Some(s) => encrypt_optional_secret(Some(s), &state)?,
    };

    let row = sqlx::query_as::<_, WebhookRow>(&format!(
        "UPDATE webhook_configs
         SET name       = $2,
             url        = $3,
             secret_enc = $4,
             enabled    = $5,
             events     = $6,
             updated_at = NOW()
         WHERE id = $1
         RETURNING {COLS}"
    ))
    .bind(webhook_id)
    .bind(&new_name)
    .bind(&new_url)
    .bind(&new_secret_enc)
    .bind(new_enabled)
    .bind(&new_events)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("Webhook not found"))?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "webhook.updated",
        Some("webhook"),
        Some(webhook_id),
        json!({"name": new_name, "url": new_url}),
    )
    .await;

    Ok(Json(WebhookResponse::from(row)))
}

/// DELETE /api/v1/admin/integrations/webhooks/:id
#[utoipa::path(
    delete,
    path = "/api/v1/admin/integrations/webhooks/{id}",
    tag = "Webhooks",
    params(("id" = Uuid, Path, description = "Webhook ID")),
    responses(
        (status = 204, description = "Webhook deleted"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Webhook not found", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn delete_webhook(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(webhook_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query!("DELETE FROM webhook_configs WHERE id = $1", webhook_id)
        .execute(&state.db_pool)
        .await
        .map_err(db_error)?;

    if result.rows_affected() == 0 {
        return Err(admin_not_found("Webhook not found"));
    }

    log_admin_action(
        &state.db_pool,
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
#[utoipa::path(
    post,
    path = "/api/v1/admin/integrations/webhooks/{id}/test",
    tag = "Webhooks",
    params(("id" = Uuid, Path, description = "Webhook ID")),
    responses(
        (status = 200, description = "Test result", body = serde_json::Value),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Webhook not found", body = crate::handlers::ErrorResponse),
        (status = 502, description = "Webhook returned non-success", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn test_webhook(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(webhook_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Fetch webhook
    let webhook = sqlx::query_as::<_, WebhookRow>(&format!(
        "SELECT {COLS} FROM webhook_configs WHERE id = $1"
    ))
    .bind(webhook_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("Webhook not found"))?;

    // Build payload
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let payload = json!({"event": "ping", "timestamp": now_secs});
    let body_str = serde_json::to_string(&payload)
        .map_err(|_| admin_internal_error("Failed to serialize payload"))?;

    // Build HTTP client with 10s timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| admin_internal_error(format!("Failed to build HTTP client: {e}")))?;

    let mut request = client
        .post(&webhook.url)
        .header("Content-Type", "application/json")
        .body(body_str.clone());

    // If there is a secret, compute HMAC-SHA256 and add signature header
    if let Some(ref enc) = webhook.secret_enc {
        let plaintext = decrypt_secret(enc, &state.secret_key)
            .map_err(|_| admin_internal_error("Failed to decrypt webhook secret"))?;

        let mut mac = Hmac::<Sha256>::new_from_slice(plaintext.as_bytes())
            .map_err(|_| admin_internal_error("Failed to create HMAC"))?;
        mac.update(body_str.as_bytes());
        let result = mac.finalize();
        let sig_hex = hex::encode(result.into_bytes());
        request = request.header("X-Webhook-Signature", format!("sha256={sig_hex}"));
    }

    match request.send().await {
        Ok(resp) => {
            let http_status = resp.status().as_u16();
            if resp.status().is_success() {
                Ok(Json(json!({"status": "ok", "http_status": http_status})))
            } else {
                Err(AppError::bad_gateway(format!(
                    "Webhook returned HTTP {http_status}"
                )))
            }
        }
        Err(e) => Err(AppError::bad_gateway(e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)]
fn validate_events(events: &[String]) -> Result<(), AppError> {
    if events.is_empty() {
        return Err(admin_bad_request("events array must not be empty"));
    }
    for event in events {
        if !VALID_EVENTS.contains(&event.as_str()) {
            return Err(admin_bad_request(format!("Unknown event type: {event}")));
        }
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
fn encrypt_optional_secret(
    secret: Option<&str>,
    state: &AppState,
) -> Result<Option<String>, AppError> {
    match secret {
        Some(s) if !s.is_empty() => {
            let enc = encrypt_secret(s, &state.secret_key)
                .map_err(|_| admin_internal_error("Failed to encrypt webhook secret"))?;
            Ok(Some(enc))
        }
        _ => Ok(None),
    }
}

fn db_error(e: sqlx::Error) -> AppError {
    tracing::error!("Database error: {:?}", e);
    admin_internal_error("Database error")
}
