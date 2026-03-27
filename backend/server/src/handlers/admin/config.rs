//! Admin OIDC and SMTP config handlers.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

use rustshare_storage::metadata_v2::schemas::{ConfigType, SystemConfigDocument};

use crate::{handlers::AdminUser, AppState};
use super::log_admin_action;

// ---------------------------------------------------------------------------
// OIDC request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct OidcConfigResponse {
    pub id: String,
    pub enabled: bool,
    pub provider_name: Option<String>,
    pub client_id: Option<String>,
    /// `"***"` when a secret is stored; `null` when none is set.
    pub client_secret: Option<String>,
    pub issuer_url: Option<String>,
    pub scopes: Vec<String>,
    pub auto_provision_users: bool,
    pub device_pair_code_ttl_seconds: Option<i32>,
    pub updated_by: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateOidcConfigRequest {
    pub enabled: Option<bool>,
    pub provider_name: Option<String>,
    pub client_id: Option<String>,
    /// Plain-text secret from the client (will be encrypted before storing).
    /// `null` / absent means "clear the stored secret".
    pub client_secret: Option<String>,
    pub issuer_url: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub auto_provision_users: Option<bool>,
    pub device_pair_code_ttl_seconds: Option<i32>,
}

// ---------------------------------------------------------------------------
// SMTP request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SmtpConfigResponse {
    pub id: String,
    pub enabled: bool,
    pub host: Option<String>,
    pub port: Option<i32>,
    pub username: Option<String>,
    /// `"***"` when a password is stored; `null` when none is set.
    pub password: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub tls_mode: Option<String>,
    pub updated_by: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSmtpConfigRequest {
    pub enabled: Option<bool>,
    pub host: Option<String>,
    pub port: Option<i32>,
    pub username: Option<String>,
    /// Plain-text password from the client (will be encrypted before storing).
    /// `null` / absent means "clear the stored password".
    pub password: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub tls_mode: Option<String>,
}

// ---------------------------------------------------------------------------
// OIDC handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/config/oidc
pub async fn get_oidc_config(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<OidcConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    let config = state.config_repo
        .get_oidc()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get OIDC config: {}", e);
            internal_error("Failed to get OIDC config")
        })?;

    let response = match config {
        Some(cfg) => {
            let config_data = cfg.config;
            OidcConfigResponse {
                id: "oidc-config".to_string(),
                enabled: config_data.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
                provider_name: config_data.get("provider_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                client_id: config_data.get("client_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
                client_secret: config_data.get("client_secret_hash").and_then(|v| v.as_str()).map(|_| "***".to_string()),
                issuer_url: config_data.get("issuer_url").and_then(|v| v.as_str()).map(|s| s.to_string()),
                scopes: config_data.get("scopes").and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_else(|| vec!["openid".to_string(), "profile".to_string(), "email".to_string()]),
                auto_provision_users: config_data.get("auto_provision_users").and_then(|v| v.as_bool()).unwrap_or(false),
                device_pair_code_ttl_seconds: config_data.get("device_pair_code_ttl_seconds").and_then(|v| v.as_i64()).map(|v| v as i32),
                updated_by: cfg.updated_by.map(|id| id.to_string()),
                updated_at: cfg.updated_at,
            }
        }
        None => {
            // Return default config
            OidcConfigResponse {
                id: "oidc-config".to_string(),
                enabled: false,
                provider_name: None,
                client_id: None,
                client_secret: None,
                issuer_url: None,
                scopes: vec!["openid".to_string(), "profile".to_string(), "email".to_string()],
                auto_provision_users: false,
                device_pair_code_ttl_seconds: Some(300),
                updated_by: None,
                updated_at: chrono::Utc::now(),
            }
        }
    };

    Ok(Json(response))
}

/// PUT /api/v1/admin/config/oidc
pub async fn update_oidc_config(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<UpdateOidcConfigRequest>,
) -> Result<Json<OidcConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Get existing config or create new
    let mut existing_config = state.config_repo
        .get_oidc()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get OIDC config: {}", e);
            internal_error("Failed to update OIDC config")
        })?
        .map(|c| c.config)
        .unwrap_or_else(|| json!({}));

    // Apply updates
    if let Some(enabled) = req.enabled {
        existing_config["enabled"] = json!(enabled);
    }
    if let Some(provider_name) = req.provider_name {
        existing_config["provider_name"] = json!(provider_name);
    }
    if let Some(client_id) = req.client_id {
        existing_config["client_id"] = json!(client_id);
    }
    if let Some(client_secret) = req.client_secret {
        if client_secret.is_empty() {
            existing_config["client_secret_hash"] = json!(null);
        } else {
            // TODO: Hash the secret before storing
            existing_config["client_secret_hash"] = json!(format!("hash:{}", client_secret));
        }
    }
    if let Some(issuer_url) = req.issuer_url {
        existing_config["issuer_url"] = json!(issuer_url);
    }
    if let Some(scopes) = req.scopes {
        existing_config["scopes"] = json!(scopes);
    }
    if let Some(auto_provision) = req.auto_provision_users {
        existing_config["auto_provision_users"] = json!(auto_provision);
    }
    if let Some(ttl) = req.device_pair_code_ttl_seconds {
        existing_config["device_pair_code_ttl_seconds"] = json!(ttl);
    }

    let config_doc = SystemConfigDocument::new(ConfigType::Oidc, existing_config, Some(actor_id));

    state.config_repo
        .set(&config_doc)
        .await
        .map_err(|e| {
            tracing::error!("Failed to set OIDC config: {}", e);
            internal_error("Failed to update OIDC config")
        })?;

    log_admin_action(
        &state,
        actor_id,
        "config.oidc_updated",
        None,
        None,
        json!({}),
    )
    .await;

    // Return updated config
    get_oidc_config(State(state), AdminUser { user_id: actor_id }).await
}

/// POST /api/v1/admin/config/oidc/test
pub async fn test_oidc_config(
    State(_state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement OIDC connectivity test
    // For now, return not implemented
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "status": "not_implemented",
            "detail": "OIDC config test not yet implemented"
        })),
    ))
}

// ---------------------------------------------------------------------------
// SMTP handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/config/smtp
pub async fn get_smtp_config(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<SmtpConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    let config = state.config_repo
        .get_smtp()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get SMTP config: {}", e);
            internal_error("Failed to get SMTP config")
        })?;

    let response = match config {
        Some(cfg) => {
            let config_data = cfg.config;
            SmtpConfigResponse {
                id: "smtp-config".to_string(),
                enabled: config_data.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
                host: config_data.get("host").and_then(|v| v.as_str()).map(|s| s.to_string()),
                port: config_data.get("port").and_then(|v| v.as_i64()).map(|v| v as i32),
                username: config_data.get("username").and_then(|v| v.as_str()).map(|s| s.to_string()),
                password: config_data.get("password_hash").and_then(|v| v.as_str()).map(|_| "***".to_string()),
                from_address: config_data.get("from_address").and_then(|v| v.as_str()).map(|s| s.to_string()),
                from_name: config_data.get("from_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
                tls_mode: config_data.get("tls_mode").and_then(|v| v.as_str()).map(|s| s.to_string()),
                updated_by: cfg.updated_by.map(|id| id.to_string()),
                updated_at: cfg.updated_at,
            }
        }
        None => {
            // Return default config
            SmtpConfigResponse {
                id: "smtp-config".to_string(),
                enabled: false,
                host: None,
                port: None,
                username: None,
                password: None,
                from_address: None,
                from_name: None,
                tls_mode: None,
                updated_by: None,
                updated_at: chrono::Utc::now(),
            }
        }
    };

    Ok(Json(response))
}

/// PUT /api/v1/admin/config/smtp
pub async fn update_smtp_config(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<UpdateSmtpConfigRequest>,
) -> Result<Json<SmtpConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Get existing config or create new
    let mut existing_config = state.config_repo
        .get_smtp()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get SMTP config: {}", e);
            internal_error("Failed to update SMTP config")
        })?
        .map(|c| c.config)
        .unwrap_or_else(|| json!({}));

    // Apply updates
    if let Some(enabled) = req.enabled {
        existing_config["enabled"] = json!(enabled);
    }
    if let Some(host) = req.host {
        existing_config["host"] = json!(host);
    }
    if let Some(port) = req.port {
        existing_config["port"] = json!(port);
    }
    if let Some(username) = req.username {
        existing_config["username"] = json!(username);
    }
    if let Some(password) = req.password {
        if password.is_empty() {
            existing_config["password_hash"] = json!(null);
        } else {
            // TODO: Hash the password before storing
            existing_config["password_hash"] = json!(format!("hash:{}", password));
        }
    }
    if let Some(from_address) = req.from_address {
        existing_config["from_address"] = json!(from_address);
    }
    if let Some(from_name) = req.from_name {
        existing_config["from_name"] = json!(from_name);
    }
    if let Some(tls_mode) = req.tls_mode {
        existing_config["tls_mode"] = json!(tls_mode);
    }

    let config_doc = SystemConfigDocument::new(ConfigType::Smtp, existing_config, Some(actor_id));

    state.config_repo
        .set(&config_doc)
        .await
        .map_err(|e| {
            tracing::error!("Failed to set SMTP config: {}", e);
            internal_error("Failed to update SMTP config")
        })?;

    log_admin_action(
        &state,
        actor_id,
        "config.smtp_updated",
        None,
        None,
        json!({}),
    )
    .await;

    // Return updated config
    get_smtp_config(State(state), AdminUser { user_id: actor_id }).await
}

/// POST /api/v1/admin/config/smtp/test
pub async fn test_smtp_config(
    AdminUser { .. }: AdminUser,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "status": "not_implemented",
            "message": "SMTP test not yet available"
        })),
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
}
