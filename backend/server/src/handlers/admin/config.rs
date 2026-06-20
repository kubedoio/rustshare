//! Admin OIDC and SMTP config handlers.

use axum::{extract::State, http::StatusCode, Json};
use rustshare_core::services::{EmailError, EmailService};
use rustshare_crypto::encrypt_secret;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::{uuid, Uuid};

use super::{admin_bad_request, admin_internal_error, admin_not_found, log_admin_action};
use crate::{
    handlers::{AdminUser, AppError},
    oidc_runtime::{invalidate_oidc_runtime_cache, OIDC_CONFIG_ID},
    AppState,
};

// ---------------------------------------------------------------------------
// Fixed singleton row IDs (pre-seeded by migrations)
// ---------------------------------------------------------------------------

const SMTP_CONFIG_ID: uuid::Uuid = uuid!("00000000-0000-0000-0000-000000000002");

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
    redirect_url: Option<String>,
    login_label: Option<String>,
    scopes: Option<Vec<String>>,
    auto_provision_users: bool,
    device_pair_code_ttl_seconds: Option<i32>,
    updated_by: Option<Uuid>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// OIDC — request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OidcConfigResponse {
    pub id: String,
    pub enabled: bool,
    pub provider_name: Option<String>,
    pub client_id: Option<String>,
    /// `"***"` when a secret is stored; `null` when none is set.
    pub client_secret: Option<String>,
    pub issuer_url: Option<String>,
    pub redirect_url: Option<String>,
    pub login_label: Option<String>,
    pub scopes: Vec<String>,
    pub auto_provision_users: bool,
    pub device_pair_code_ttl_seconds: Option<i32>,
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
            redirect_url: row.redirect_url,
            login_label: row.login_label,
            scopes: row.scopes.unwrap_or_default(),
            auto_provision_users: row.auto_provision_users,
            device_pair_code_ttl_seconds: row.device_pair_code_ttl_seconds,
            updated_by: row.updated_by.map(|u| u.to_string()),
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateOidcConfigRequest {
    pub enabled: Option<bool>,
    pub provider_name: Option<String>,
    pub client_id: Option<String>,
    /// Plain-text secret from the client (will be encrypted before storing).
    /// `null` / absent means "clear the stored secret".
    pub client_secret: Option<String>,
    pub issuer_url: Option<String>,
    pub redirect_url: Option<String>,
    pub login_label: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub auto_provision_users: Option<bool>,
    pub device_pair_code_ttl_seconds: Option<i32>,
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
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

#[derive(Debug, Deserialize, utoipa::ToSchema)]
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SmtpTestResponse {
    pub success: bool,
    pub message: String,
}

// ---------------------------------------------------------------------------
// OIDC handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/config/oidc
#[utoipa::path(
    get,
    path = "/api/v1/admin/config/oidc",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_oidc_config(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<OidcConfigResponse>, AppError> {
    let row = sqlx::query_as::<_, OidcConfigRow>(
        "SELECT id, enabled, provider_name, client_id, client_secret_enc,
                issuer_url, redirect_url, login_label, scopes, auto_provision_users, device_pair_code_ttl_seconds,
                updated_by, updated_at
         FROM oidc_config
         WHERE id = $1",
    )
    .bind(OIDC_CONFIG_ID)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("OIDC config not found"))?;

    Ok(Json(OidcConfigResponse::from(row)))
}

/// PUT /api/v1/admin/config/oidc
#[utoipa::path(
    put,
    path = "/api/v1/admin/config/oidc",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_oidc_config(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<UpdateOidcConfigRequest>,
) -> Result<Json<OidcConfigResponse>, AppError> {
    // Validate TTL value
    if let Some(ttl) = req.device_pair_code_ttl_seconds {
        if ![300, 600, 1800].contains(&ttl) {
            return Err(admin_bad_request(
                "device_pair_code_ttl_seconds must be 300, 600, or 1800",
            ));
        }
    }

    // Fetch existing row to use as fallback for fields not provided.
    let current = sqlx::query_as::<_, OidcConfigRow>(
        "SELECT id, enabled, provider_name, client_id, client_secret_enc,
                issuer_url, redirect_url, login_label, scopes, auto_provision_users, device_pair_code_ttl_seconds,
                updated_by, updated_at
         FROM oidc_config
         WHERE id = $1",
    )
    .bind(OIDC_CONFIG_ID)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("OIDC config not found"))?;

    let new_enabled = req.enabled.unwrap_or(current.enabled);
    let new_provider_name = req.provider_name.or(current.provider_name);
    let new_client_id = req.client_id.or(current.client_id);
    let new_issuer_url = req.issuer_url.or(current.issuer_url);
    let new_redirect_url = req.redirect_url.or(current.redirect_url);
    let new_login_label = req.login_label.or(current.login_label);
    let new_scopes = req.scopes.or(current.scopes);
    let new_auto_provision = req
        .auto_provision_users
        .unwrap_or(current.auto_provision_users);
    let new_device_pair_ttl = req
        .device_pair_code_ttl_seconds
        .or(current.device_pair_code_ttl_seconds);

    // Determine encrypted secret:
    //   - If req.client_secret is Some(s) and s is non-empty → encrypt it.
    //   - If req.client_secret is Some("") → clear (set to NULL).
    //   - If req.client_secret is None → keep existing value.
    let new_secret_enc: Option<String> = match req.client_secret {
        Some(ref s) if !s.is_empty() => {
            let enc = encrypt_secret(s, &state.secret_key)
                .map_err(|_| admin_internal_error("Failed to encrypt client secret"))?;
            Some(enc)
        }
        Some(_) => None, // empty string → clear
        None => current.client_secret_enc,
    };

    let row = sqlx::query_as::<_, OidcConfigRow>(
        "UPDATE oidc_config
         SET enabled                       = $2,
             provider_name                 = $3,
             client_id                     = $4,
             client_secret_enc             = $5,
             issuer_url                    = $6,
             redirect_url                  = $7,
             login_label                   = $8,
             scopes                        = $9,
             auto_provision_users          = $10,
             device_pair_code_ttl_seconds  = $11,
             updated_by                    = $12,
             updated_at                    = NOW()
         WHERE id = $1
         RETURNING id, enabled, provider_name, client_id, client_secret_enc,
                   issuer_url, redirect_url, login_label, scopes, auto_provision_users, device_pair_code_ttl_seconds,
                   updated_by, updated_at",
    )
    .bind(OIDC_CONFIG_ID)
    .bind(new_enabled)
    .bind(new_provider_name)
    .bind(new_client_id)
    .bind(new_secret_enc)
    .bind(new_issuer_url)
    .bind(new_redirect_url)
    .bind(new_login_label)
    .bind(new_scopes)
    .bind(new_auto_provision)
    .bind(new_device_pair_ttl)
    .bind(actor_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("OIDC config not found"))?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "config.oidc_updated",
        None,
        None,
        json!({}),
    )
    .await;

    invalidate_oidc_runtime_cache(&state).await;

    Ok(Json(OidcConfigResponse::from(row)))
}

/// POST /api/v1/admin/config/oidc/test
#[utoipa::path(
    post,
    path = "/api/v1/admin/config/oidc/test",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn test_oidc_config(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<serde_json::Value>, AppError> {
    // Read current issuer_url
    let row = sqlx::query_as::<_, OidcConfigRow>(
        "SELECT id, enabled, provider_name, client_id, client_secret_enc,
                issuer_url, redirect_url, login_label, scopes, auto_provision_users, device_pair_code_ttl_seconds,
                updated_by, updated_at
         FROM oidc_config
         WHERE id = $1",
    )
    .bind(OIDC_CONFIG_ID)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("OIDC config not found"))?;

    let issuer_url = row
        .issuer_url
        .ok_or_else(|| admin_bad_request("No issuer URL configured"))?;

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
        .map_err(|e| admin_internal_error(format!("Failed to build HTTP client: {}", e)))?;

    match client.get(&discovery_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                Ok(Json(json!({
                    "success": true,
                    "message": "Discovery document fetched successfully"
                })))
            } else {
                let status_code = resp.status();
                Err(AppError::bad_gateway(format!(
                    "Discovery URL returned HTTP {}",
                    status_code
                )))
            }
        }
        Err(e) => Err(AppError::bad_gateway(format!("{}", e))),
    }
}

// ---------------------------------------------------------------------------
// SMTP handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/config/smtp
#[utoipa::path(
    get,
    path = "/api/v1/admin/config/smtp",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_smtp_config(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<SmtpConfigResponse>, AppError> {
    let row = sqlx::query_as::<_, SmtpConfigRow>(
        "SELECT id, enabled, host, port, username, password_enc,
                from_address, from_name, tls_mode, updated_by, updated_at
         FROM smtp_config
         WHERE id = $1",
    )
    .bind(SMTP_CONFIG_ID)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("SMTP config not found"))?;

    Ok(Json(SmtpConfigResponse::from(row)))
}

/// PUT /api/v1/admin/config/smtp
///
/// Validates the supplied port (1-65535) and TLS mode (`tls` or `starttls`)
/// before persisting the configuration.
#[utoipa::path(
    put,
    path = "/api/v1/admin/config/smtp",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 400, description = "Bad Request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_smtp_config(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<UpdateSmtpConfigRequest>,
) -> Result<Json<SmtpConfigResponse>, AppError> {
    // Fetch current row to preserve unset fields
    let current = sqlx::query_as::<_, SmtpConfigRow>(
        "SELECT id, enabled, host, port, username, password_enc,
                from_address, from_name, tls_mode, updated_by, updated_at
         FROM smtp_config
         WHERE id = $1",
    )
    .bind(SMTP_CONFIG_ID)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| admin_not_found("SMTP config not found"))?;

    let new_enabled = req.enabled.unwrap_or(current.enabled);
    let new_host = req.host.or(current.host);
    let new_port = req.port.or(current.port);
    let new_username = req.username.or(current.username);
    let new_from_address = req.from_address.or(current.from_address);
    let new_from_name = req.from_name.or(current.from_name);
    let new_tls_mode = req.tls_mode.or(current.tls_mode);

    if let Some(port) = new_port {
        if !(1..=65535).contains(&port) {
            return Err(admin_bad_request("SMTP port must be between 1 and 65535"));
        }
    }

    if let Some(ref tls_mode) = new_tls_mode {
        if tls_mode != "tls" && tls_mode != "starttls" {
            return Err(admin_bad_request(
                "SMTP TLS mode must be 'tls' or 'starttls'",
            ));
        }
    }

    // Determine encrypted password:
    //   - Some(s) where s is non-empty → encrypt it.
    //   - Some("") → clear (set to NULL).
    //   - None → keep existing value.
    let new_password_enc: Option<String> = match req.password {
        Some(ref s) if !s.is_empty() => {
            let enc = encrypt_secret(s, &state.secret_key)
                .map_err(|_| admin_internal_error("Failed to encrypt SMTP password"))?;
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
    .bind(SMTP_CONFIG_ID)
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
    .ok_or_else(|| admin_not_found("SMTP config not found"))?;

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
/// Sends a test email to the acting admin's email address using the stored SMTP
/// configuration. Returns 400 Bad Request for an invalid TLS mode, 503 Service
/// Unavailable when SMTP is disabled or incomplete, and 502 Bad Gateway when the
/// remote server rejects the message.
#[utoipa::path(
    post,
    path = "/api/v1/admin/config/smtp/test",
    tag = "Admin",
    responses(
        (status = 200, description = "Success", body = SmtpTestResponse),
        (status = 400, description = "Bad Request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 502, description = "Bad Gateway", body = crate::handlers::ErrorResponse),
        (status = 503, description = "Service Unavailable", body = SmtpTestResponse),
    ),
)]
pub async fn test_smtp_config(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
) -> Result<(StatusCode, Json<SmtpTestResponse>), AppError> {
    let admin_email = sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
        .bind(actor_id)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(db_error)?
        .ok_or_else(|| admin_not_found("Admin user not found"))?;

    let email_service = EmailService::new(state.db_pool, state.secret_key);

    match email_service.send_test_email(&admin_email).await {
        Ok(()) => Ok((
            StatusCode::OK,
            Json(SmtpTestResponse {
                success: true,
                message: "Test email sent successfully".to_string(),
            }),
        )),
        Err(EmailError::SmtpNotConfigured) => Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SmtpTestResponse {
                success: false,
                message: "SMTP is not configured or not enabled".to_string(),
            }),
        )),
        Err(EmailError::InvalidTlsMode(mode)) => Err(AppError::bad_request(format!(
            "Invalid SMTP TLS mode: {}. Must be 'tls' or 'starttls'",
            mode
        ))),
        Err(e) => {
            tracing::warn!(error = %e, "SMTP test email failed");
            Err(AppError::bad_gateway(
                "Failed to send test email. Check the server logs for details.",
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Security config — request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SecurityConfigResponse {
    pub login_protection_enabled: bool,
    pub max_login_attempts: i32,
    pub login_block_duration_minutes: i32,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateSecurityConfigRequest {
    pub login_protection_enabled: Option<bool>,
    pub max_login_attempts: Option<i32>,
    pub login_block_duration_minutes: Option<i32>,
}

// ---------------------------------------------------------------------------
// Security config handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/admin/config/security
#[utoipa::path(
    get,
    path = "/api/v1/admin/config/security",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_security_config(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<SecurityConfigResponse>, AppError> {
    let config = state
        .metadata_store
        .get_security_config()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get security config: {}", e);
            admin_internal_error("Failed to get security config")
        })?
        .ok_or_else(|| AppError::not_found("Security config not found"))?;

    Ok(Json(SecurityConfigResponse {
        login_protection_enabled: config.login_protection_enabled,
        max_login_attempts: config.max_login_attempts,
        login_block_duration_minutes: config.login_block_duration_minutes,
        updated_at: config.updated_at,
    }))
}

/// PUT /api/v1/admin/config/security
#[utoipa::path(
    put,
    path = "/api/v1/admin/config/security",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_security_config(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Json(req): Json<UpdateSecurityConfigRequest>,
) -> Result<Json<SecurityConfigResponse>, AppError> {
    if let Some(max) = req.max_login_attempts {
        if !(1..=100).contains(&max) {
            return Err(admin_bad_request(
                "max_login_attempts must be between 1 and 100",
            ));
        }
    }

    if let Some(duration) = req.login_block_duration_minutes {
        if !(1..=10080).contains(&duration) {
            return Err(admin_bad_request(
                "login_block_duration_minutes must be between 1 and 10080 (7 days)",
            ));
        }
    }

    let config = state
        .metadata_store
        .update_security_config(
            req.login_protection_enabled,
            req.max_login_attempts,
            req.login_block_duration_minutes,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to update security config: {}", e);
            admin_internal_error("Failed to update security config")
        })?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "config.security_updated",
        None,
        None,
        json!({
            "login_protection_enabled": config.login_protection_enabled,
            "max_login_attempts": config.max_login_attempts,
            "login_block_duration_minutes": config.login_block_duration_minutes,
        }),
    )
    .await;

    Ok(Json(SecurityConfigResponse {
        login_protection_enabled: config.login_protection_enabled,
        max_login_attempts: config.max_login_attempts,
        login_block_duration_minutes: config.login_block_duration_minutes,
        updated_at: config.updated_at,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn db_error(e: sqlx::Error) -> AppError {
    tracing::error!("Database error: {:?}", e);
    admin_internal_error("Database error")
}
