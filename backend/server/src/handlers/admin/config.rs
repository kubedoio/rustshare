//! Admin OIDC and SMTP config handlers.

use axum::{extract::State, http::StatusCode, Json};
use rustshare_crypto::encrypt_secret;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{handlers::AdminUser, AppState};
use super::log_admin_action;

// ---------------------------------------------------------------------------
// Fixed singleton row IDs (pre-seeded by migrations)
// ---------------------------------------------------------------------------

const OIDC_CONFIG_ID: &str = "00000000-0000-0000-0000-000000000001";
const SMTP_CONFIG_ID: &str = "00000000-0000-0000-0000-000000000002";

// ---------------------------------------------------------------------------
// OIDC — row type
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct OidcConfigRow {
    id: Uuid,
    enabled: bool,
    provider_name: Option<String>,
    client_id: Option<String>,
    client_secret_enc: Option<String>,
    issuer_url: Option<String>,
    scopes: Option<Vec<String>>,
    auto_provision_users: bool,
    updated_by: Option<Uuid>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// OIDC — request / response types
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
    pub updated_by: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<OidcConfigRow> for OidcConfigResponse {
    fn from(row: OidcConfigRow) -> Self {
        OidcConfigResponse {
            id: row.id.to_string(),
            enabled: row.enabled,
            provider_name: row.provider_name,
            client_id: row.client_id,
            client_secret: row.client_secret_enc.as_deref().map(|_| "***".to_string()),
            issuer_url: row.issuer_url,
            scopes: row.scopes.unwrap_or_default(),
            auto_provision_users: row.auto_provision_users,
            updated_by: row.updated_by.map(|u| u.to_string()),
            updated_at: row.updated_at,
        }
    }
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
}

// ---------------------------------------------------------------------------
// SMTP — row type
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SmtpConfigRow {
    id: Uuid,
    enabled: bool,
    host: Option<String>,
    port: Option<i32>,
    username: Option<String>,
    password_enc: Option<String>,
    from_address: Option<String>,
    from_name: Option<String>,
    tls_mode: Option<String>,
    updated_by: Option<Uuid>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// SMTP — request / response types
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

impl From<SmtpConfigRow> for SmtpConfigResponse {
    fn from(row: SmtpConfigRow) -> Self {
        SmtpConfigResponse {
            id: row.id.to_string(),
            enabled: row.enabled,
            host: row.host,
            port: row.port,
            username: row.username,
            password: row.password_enc.as_deref().map(|_| "***".to_string()),
            from_address: row.from_address,
            from_name: row.from_name,
            tls_mode: row.tls_mode,
            updated_by: row.updated_by.map(|u| u.to_string()),
            updated_at: row.updated_at,
        }
    }
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
    let row = sqlx::query_as::<_, OidcConfigRow>(
        "SELECT id, enabled, provider_name, client_id, client_secret_enc,
                issuer_url, scopes, auto_provision_users, updated_by, updated_at
         FROM oidc_config
         WHERE id = $1",
    )
    .bind(OIDC_CONFIG_ID.parse::<Uuid>().unwrap())
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("OIDC config not found"))?;

    Ok(Json(OidcConfigResponse::from(row)))
}

/// PUT /api/v1/admin/config/oidc
pub async fn update_oidc_config(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<UpdateOidcConfigRequest>,
) -> Result<Json<OidcConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Fetch existing row to use as fallback for fields not provided.
    let current = sqlx::query_as::<_, OidcConfigRow>(
        "SELECT id, enabled, provider_name, client_id, client_secret_enc,
                issuer_url, scopes, auto_provision_users, updated_by, updated_at
         FROM oidc_config
         WHERE id = $1",
    )
    .bind(OIDC_CONFIG_ID.parse::<Uuid>().unwrap())
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("OIDC config not found"))?;

    let new_enabled = req.enabled.unwrap_or(current.enabled);
    let new_provider_name = req.provider_name.or(current.provider_name);
    let new_client_id = req.client_id.or(current.client_id);
    let new_issuer_url = req.issuer_url.or(current.issuer_url);
    let new_scopes = req.scopes.or(current.scopes);
    let new_auto_provision = req.auto_provision_users.unwrap_or(current.auto_provision_users);

    // Determine encrypted secret:
    //   - If req.client_secret is Some(s) and s is non-empty → encrypt it.
    //   - If req.client_secret is Some("") → clear (set to NULL).
    //   - If req.client_secret is None → keep existing value.
    let new_secret_enc: Option<String> = match req.client_secret {
        Some(ref s) if !s.is_empty() => {
            let enc = encrypt_secret(s, &state.secret_key)
                .map_err(|_| internal_error("Failed to encrypt client secret"))?;
            Some(enc)
        }
        Some(_) => None, // empty string → clear
        None => current.client_secret_enc,
    };

    let row = sqlx::query_as::<_, OidcConfigRow>(
        "UPDATE oidc_config
         SET enabled              = $2,
             provider_name        = $3,
             client_id            = $4,
             client_secret_enc    = $5,
             issuer_url           = $6,
             scopes               = $7,
             auto_provision_users = $8,
             updated_by           = $9,
             updated_at           = NOW()
         WHERE id = $1
         RETURNING id, enabled, provider_name, client_id, client_secret_enc,
                   issuer_url, scopes, auto_provision_users, updated_by, updated_at",
    )
    .bind(OIDC_CONFIG_ID.parse::<Uuid>().unwrap())
    .bind(new_enabled)
    .bind(new_provider_name)
    .bind(new_client_id)
    .bind(new_secret_enc)
    .bind(new_issuer_url)
    .bind(new_scopes)
    .bind(new_auto_provision)
    .bind(actor_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("OIDC config not found"))?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "config.oidc_updated",
        None,
        None,
        json!({}),
    )
    .await;

    Ok(Json(OidcConfigResponse::from(row)))
}

/// POST /api/v1/admin/config/oidc/test
pub async fn test_oidc_config(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Read current issuer_url
    let row = sqlx::query_as::<_, OidcConfigRow>(
        "SELECT id, enabled, provider_name, client_id, client_secret_enc,
                issuer_url, scopes, auto_provision_users, updated_by, updated_at
         FROM oidc_config
         WHERE id = $1",
    )
    .bind(OIDC_CONFIG_ID.parse::<Uuid>().unwrap())
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("OIDC config not found"))?;

    let issuer_url = row
        .issuer_url
        .ok_or_else(|| bad_request("No issuer URL configured"))?;

    // Build discovery URL
    let discovery_url = if issuer_url.ends_with("/.well-known/openid-configuration") {
        issuer_url
    } else {
        let base = issuer_url.trim_end_matches('/');
        format!("{}/.well-known/openid-configuration", base)
    };

    // Attempt to fetch the discovery document (10s timeout).
    // We use `openidconnect::reqwest` which is already a project dependency.
    let client = openidconnect::reqwest::ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| internal_error(&format!("Failed to build HTTP client: {}", e)))?;

    match client.get(&discovery_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                Ok(Json(json!({
                    "status": "ok",
                    "detail": "Discovery document fetched successfully"
                })))
            } else {
                let status_code = resp.status();
                Err((
                    StatusCode::BAD_GATEWAY,
                    Json(json!({
                        "status": "error",
                        "detail": format!("Discovery URL returned HTTP {}", status_code)
                    })),
                ))
            }
        }
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "status": "error",
                "detail": format!("{}", e)
            })),
        )),
    }
}

// ---------------------------------------------------------------------------
// SMTP handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/config/smtp
pub async fn get_smtp_config(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<SmtpConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, SmtpConfigRow>(
        "SELECT id, enabled, host, port, username, password_enc,
                from_address, from_name, tls_mode, updated_by, updated_at
         FROM smtp_config
         WHERE id = $1",
    )
    .bind(SMTP_CONFIG_ID.parse::<Uuid>().unwrap())
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("SMTP config not found"))?;

    Ok(Json(SmtpConfigResponse::from(row)))
}

/// PUT /api/v1/admin/config/smtp
pub async fn update_smtp_config(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<UpdateSmtpConfigRequest>,
) -> Result<Json<SmtpConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Fetch current row to preserve unset fields
    let current = sqlx::query_as::<_, SmtpConfigRow>(
        "SELECT id, enabled, host, port, username, password_enc,
                from_address, from_name, tls_mode, updated_by, updated_at
         FROM smtp_config
         WHERE id = $1",
    )
    .bind(SMTP_CONFIG_ID.parse::<Uuid>().unwrap())
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("SMTP config not found"))?;

    let new_enabled = req.enabled.unwrap_or(current.enabled);
    let new_host = req.host.or(current.host);
    let new_port = req.port.or(current.port);
    let new_username = req.username.or(current.username);
    let new_from_address = req.from_address.or(current.from_address);
    let new_from_name = req.from_name.or(current.from_name);
    let new_tls_mode = req.tls_mode.or(current.tls_mode);

    // Determine encrypted password:
    //   - Some(s) where s is non-empty → encrypt it.
    //   - Some("") → clear (set to NULL).
    //   - None → keep existing value.
    let new_password_enc: Option<String> = match req.password {
        Some(ref s) if !s.is_empty() => {
            let enc = encrypt_secret(s, &state.secret_key)
                .map_err(|_| internal_error("Failed to encrypt SMTP password"))?;
            Some(enc)
        }
        Some(_) => None, // empty string → clear
        None => current.password_enc,
    };

    let row = sqlx::query_as::<_, SmtpConfigRow>(
        "UPDATE smtp_config
         SET enabled      = $2,
             host         = $3,
             port         = $4,
             username     = $5,
             password_enc = $6,
             from_address = $7,
             from_name    = $8,
             tls_mode     = $9,
             updated_by   = $10,
             updated_at   = NOW()
         WHERE id = $1
         RETURNING id, enabled, host, port, username, password_enc,
                   from_address, from_name, tls_mode, updated_by, updated_at",
    )
    .bind(SMTP_CONFIG_ID.parse::<Uuid>().unwrap())
    .bind(new_enabled)
    .bind(new_host)
    .bind(new_port)
    .bind(new_username)
    .bind(new_password_enc)
    .bind(new_from_address)
    .bind(new_from_name)
    .bind(new_tls_mode)
    .bind(actor_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| not_found("SMTP config not found"))?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "config.smtp_updated",
        None,
        None,
        json!({}),
    )
    .await;

    Ok(Json(SmtpConfigResponse::from(row)))
}

/// POST /api/v1/admin/config/smtp/test
///
/// No SMTP library is available; always returns a "not_implemented" stub.
pub async fn test_smtp_config(
    AdminUser { .. }: AdminUser,
) -> Json<serde_json::Value> {
    Json(json!({
        "status": "not_implemented",
        "message": "SMTP test not yet available"
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn db_error(e: sqlx::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("Database error: {:?}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "Database error" })),
    )
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

