//! Admin OIDC and SMTP config handlers.
//!
//! TODO: This module needs to be rewritten to use the new RustFS-based
//! configuration storage instead of PostgreSQL.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

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
/// 
/// TODO: Implement using new ConfigStore
pub async fn get_oidc_config(
    State(_state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<OidcConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new ConfigStore
    tracing::warn!("OIDC config not yet implemented in zero-PostgreSQL mode");
    
    // Return placeholder response
    Ok(Json(OidcConfigResponse {
        id: "00000000-0000-0000-0000-000000000001".to_string(),
        enabled: false,
        provider_name: None,
        client_id: None,
        client_secret: None,
        issuer_url: None,
        scopes: vec![],
        auto_provision_users: false,
        device_pair_code_ttl_seconds: Some(300),
        updated_by: None,
        updated_at: chrono::Utc::now(),
    }))
}

/// PUT /api/v1/admin/config/oidc
/// 
/// TODO: Implement using new ConfigStore
pub async fn update_oidc_config(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(_req): Json<UpdateOidcConfigRequest>,
) -> Result<Json<OidcConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new ConfigStore
    tracing::warn!("OIDC config update not yet implemented in zero-PostgreSQL mode");
    
    // Log admin action (noop for now)
    log_admin_action(
        actor_id,
        "config.oidc_updated",
        None,
        None,
        json!({}),
    )
    .await;

    // Return placeholder response
    Ok(Json(OidcConfigResponse {
        id: "00000000-0000-0000-0000-000000000001".to_string(),
        enabled: false,
        provider_name: None,
        client_id: None,
        client_secret: None,
        issuer_url: None,
        scopes: vec![],
        auto_provision_users: false,
        device_pair_code_ttl_seconds: Some(300),
        updated_by: Some(actor_id.to_string()),
        updated_at: chrono::Utc::now(),
    }))
}

/// POST /api/v1/admin/config/oidc/test
/// 
/// TODO: Implement using new ConfigStore
pub async fn test_oidc_config(
    State(_state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new ConfigStore
    tracing::warn!("OIDC config test not yet implemented in zero-PostgreSQL mode");
    
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
/// 
/// TODO: Implement using new ConfigStore
pub async fn get_smtp_config(
    State(_state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<SmtpConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new ConfigStore
    tracing::warn!("SMTP config not yet implemented in zero-PostgreSQL mode");
    
    // Return placeholder response
    Ok(Json(SmtpConfigResponse {
        id: "00000000-0000-0000-0000-000000000002".to_string(),
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
    }))
}

/// PUT /api/v1/admin/config/smtp
/// 
/// TODO: Implement using new ConfigStore
pub async fn update_smtp_config(
    State(_state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(_req): Json<UpdateSmtpConfigRequest>,
) -> Result<Json<SmtpConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    // TODO: Implement using new ConfigStore
    tracing::warn!("SMTP config update not yet implemented in zero-PostgreSQL mode");
    
    // Log admin action (noop for now)
    log_admin_action(
        actor_id,
        "config.smtp_updated",
        None,
        None,
        json!({}),
    )
    .await;

    // Return placeholder response
    Ok(Json(SmtpConfigResponse {
        id: "00000000-0000-0000-0000-000000000002".to_string(),
        enabled: false,
        host: None,
        port: None,
        username: None,
        password: None,
        from_address: None,
        from_name: None,
        tls_mode: None,
        updated_by: Some(actor_id.to_string()),
        updated_at: chrono::Utc::now(),
    }))
}

/// POST /api/v1/admin/config/smtp/test
///
/// No SMTP library is available; always returns a "not_implemented" stub.
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
