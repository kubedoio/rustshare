//! Device pairing authentication handlers.
//!
//! Provides endpoints for the device pairing flow:
//! - POST /api/v1/auth/device/request - Generate user_code and device_code
//! - POST /api/v1/auth/device/poll - Check approval status and issue token

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{distributions::Uniform, Rng};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::time::{Duration, Instant};


use crate::handlers::AuthenticatedUser;
use crate::oidc_runtime::load_oidc_runtime_settings;
use crate::AppState;

/// User code alphabet - excludes ambiguous characters: 0, O, 1, I, L
const USER_CODE_ALPHABET: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'M', 'N', 'P', 'Q', 'R', 'S', 'T', 'U', 'V',
    'W', 'X', 'Y', 'Z', '2', '3', '4', '5', '6', '7', '8', '9',
];

const USER_CODE_LENGTH: usize = 8;
const DEVICE_CODE_LENGTH: usize = 32;
const TOKEN_LENGTH: usize = 32;
const POLL_RATE_LIMIT_SECONDS: u64 = 5;

/// Response for QR info endpoint
#[derive(Serialize)]
pub struct DeviceQrInfoResponse {
    pub instance_url: String,
    pub device_pairing_path: String,
}

/// GET /api/v1/auth/device/qr-info
/// Returns information needed for QR code generation on the device pairing page
pub async fn device_qr_info(
    headers: HeaderMap,
) -> Result<Json<DeviceQrInfoResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Extract Host header from request
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Extract protocol from X-Forwarded-Proto header, fallback to https
    let protocol = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("https");

    // Construct the instance URL
    let instance_url = match host {
        Some(host) => format!("{}://{}", protocol, host),
        None => {
            // Fall back to RUSTSHARE_PUBLIC_URL env var or default
            std::env::var("RUSTSHARE_PUBLIC_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string())
        }
    };

    Ok(Json(DeviceQrInfoResponse {
        instance_url,
        device_pairing_path: "/device".to_string(),
    }))
}

#[derive(Deserialize)]
pub struct DevicePollRequest {
    pub device_code: String,
}

/// Response for device poll
#[derive(Serialize)]
#[serde(tag = "status")]
pub enum DevicePollResponse {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "approved")]
    Approved { token: String },
    #[serde(rename = "expired")]
    Expired,
}

/// Request body for device approval
#[derive(Deserialize)]
pub struct DeviceApproveRequest {
    pub user_code: String,
}

/// Response for device approval
#[derive(Serialize)]
pub struct DeviceApproveResponse {
    pub device_name: String,
}

/// Response for device request
#[derive(Serialize)]
pub struct DeviceRequestResponse {
    pub user_code: String,
    pub device_code: String,
    pub expires_in: i64,
}

/// POST /api/v1/auth/device/approve
/// Approves a device pair request using the user_code
pub async fn device_approve(
    State(state): State<AppState>,
    AuthenticatedUser { user_id, .. }: AuthenticatedUser,
    Json(body): Json<DeviceApproveRequest>,
) -> Result<Json<DeviceApproveResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Look up pair request by user_code (case-insensitive)
    let row = sqlx::query(
        r#"
        SELECT
            id,
            user_id,
            approved_at IS NOT NULL as is_approved,
            expires_at < NOW() as is_expired
        FROM device_pair_requests
        WHERE UPPER(user_code) = UPPER($1)
        "#,
    )
    .bind(&body.user_code)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| server_error(format!("Database error: {}", e)))?;

    let (id, is_approved, is_expired) = match row {
        Some(row) => {
            let id: uuid::Uuid = row
                .try_get("id")
                .map_err(|e| server_error(format!("Failed to get id: {}", e)))?;
            let is_approved: bool = row.try_get("is_approved").unwrap_or(false);
            let is_expired: bool = row.try_get("is_expired").unwrap_or(true);
            (id, is_approved, is_expired)
        }
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({"error": "code_not_found"})),
            ));
        }
    };

    // Check if expired
    if is_expired {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "code_not_found"})),
        ));
    }

    // Check if already approved
    if is_approved {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({"error": "already_approved"})),
        ));
    }

    // Update the pair request with user_id and approved_at
    sqlx::query(
        r#"
        UPDATE device_pair_requests
        SET user_id = $1, approved_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(user_id)
    .bind(id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| server_error(format!("Failed to approve pair request: {}", e)))?;

    // Return device_name - the actual device name is captured at poll time
    // when the token is created, so we return a placeholder here
    Ok(Json(DeviceApproveResponse {
        device_name: "Device".to_string(),
    }))
}

/// Generate a random user code (8 chars from safe alphabet)
fn gen_user_code() -> String {
    let mut rng = rand::thread_rng();
    let alphabet = Uniform::from(0..USER_CODE_ALPHABET.len());

    (0..USER_CODE_LENGTH)
        .map(|_| USER_CODE_ALPHABET[rng.sample(alphabet)])
        .collect()
}

/// Generate a random device code (32 bytes, base64url-encoded)
fn gen_device_code() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..DEVICE_CODE_LENGTH).map(|_| rng.gen()).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// Generate a random token (32 bytes, base64url-encoded)
fn gen_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..TOKEN_LENGTH).map(|_| rng.gen()).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// Hash a token using SHA-256, return hex string
fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

/// Standard server error response
fn server_error(msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": msg.into() })),
    )
}

/// POST /api/v1/auth/device/request
/// Generates a new device pair request with user_code and device_code
pub async fn device_request(
    State(state): State<AppState>,
) -> Result<Json<DeviceRequestResponse>, (StatusCode, Json<serde_json::Value>)> {
    let ttl_seconds = load_oidc_runtime_settings(&state)
        .await
        .map(|settings| settings.device_pair_code_ttl_seconds())
        .unwrap_or(300);
    let ttl_seconds_i64 = i64::from(ttl_seconds);

    let user_code = gen_user_code();
    let device_code = gen_device_code();

    // Insert into device_pair_requests
    sqlx::query(
        r#"
        INSERT INTO device_pair_requests (id, device_code, user_code, expires_at)
        VALUES (gen_random_uuid(), $1, $2, NOW() + INTERVAL '1 second' * $3)
        "#,
    )
    .bind(&device_code)
    .bind(&user_code)
    .bind(f64::from(ttl_seconds))
    .execute(&state.db_pool)
    .await
    .map_err(|e| server_error(format!("Failed to create pair request: {}", e)))?;

    Ok(Json(DeviceRequestResponse {
        user_code,
        device_code,
        expires_in: ttl_seconds_i64,
    }))
}

/// POST /api/v1/auth/device/poll
/// Polls for approval status and issues token when approved
pub async fn device_poll(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DevicePollRequest>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // Check rate limit
    let now = Instant::now();
    let rate_limit_key = req.device_code.clone();

    {
        let mut rate_limiter = state.poll_rate_limiter.lock().await;

        if let Some(last_request) = rate_limiter.get(&rate_limit_key) {
            let elapsed = now.duration_since(*last_request);
            if elapsed < Duration::from_secs(POLL_RATE_LIMIT_SECONDS) {
                let retry_after = POLL_RATE_LIMIT_SECONDS - elapsed.as_secs();
                return Ok((
                    StatusCode::TOO_MANY_REQUESTS,
                    [(axum::http::header::RETRY_AFTER, retry_after.to_string())],
                    Json(serde_json::json!({ "error": "Rate limit exceeded" })),
                )
                    .into_response());
            }
        }

        rate_limiter.insert(rate_limit_key, now);
    }

    // Look up the pair request
    let row = sqlx::query(
        r#"
        SELECT
            user_id,
            approved_at IS NOT NULL as is_approved,
            expires_at < NOW() as is_expired
        FROM device_pair_requests
        WHERE device_code = $1
        "#,
    )
    .bind(&req.device_code)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| server_error(format!("Database error: {}", e)))?;

    let (user_id_opt, is_approved, is_expired) = match row {
        Some(row) => {
            let user_id: Option<uuid::Uuid> = row.try_get("user_id").ok();
            let is_approved: bool = row.try_get("is_approved").unwrap_or(false);
            let is_expired: bool = row.try_get("is_expired").unwrap_or(true);
            (user_id, is_approved, is_expired)
        }
        None => {
            // Device code not found - treat as expired
            return Ok(Json(DevicePollResponse::Expired {}).into_response());
        }
    };

    // Check if expired
    if is_expired {
        // Clean up expired request - best effort
        if let Err(e) = sqlx::query("DELETE FROM device_pair_requests WHERE device_code = $1")
            .bind(&req.device_code)
            .execute(&state.db_pool)
            .await
        {
            tracing::debug!(device_code = %req.device_code, error = %e, "failed to clean up expired device pair request");
        }

        return Ok(Json(DevicePollResponse::Expired {}).into_response());
    }

    // Check if still pending
    if !is_approved {
        return Ok(Json(DevicePollResponse::Pending {}).into_response());
    }

    // Approved - generate token and clean up
    let user_id = user_id_opt.ok_or_else(|| server_error("Approved request missing user_id"))?;

    let raw_token = gen_token();
    let token_hash = hash_token(&raw_token);

    // Get device name from User-Agent header
    let device_name = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| if s.len() > 255 { &s[..255] } else { s })
        .unwrap_or("Unknown device")
        .to_string();

    // Insert token and delete pair request in a transaction
    let mut tx = state
        .db_pool
        .begin()
        .await
        .map_err(|e| server_error(format!("Transaction error: {}", e)))?;

    // Insert the device token
    sqlx::query(
        r#"
        INSERT INTO device_tokens (id, user_id, token_hash, device_name)
        VALUES (gen_random_uuid(), $1, $2, $3)
        "#,
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(&device_name)
    .execute(&mut *tx)
    .await
    .map_err(|e| server_error(format!("Failed to create token: {}", e)))?;

    // Delete the pair request
    sqlx::query("DELETE FROM device_pair_requests WHERE device_code = $1")
        .bind(&req.device_code)
        .execute(&mut *tx)
        .await
        .map_err(|e| server_error(format!("Failed to clean up pair request: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| server_error(format!("Transaction commit error: {}", e)))?;

    Ok(Json(DevicePollResponse::Approved { token: raw_token }).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Static mutex to ensure env var tests run serially
    static ENV_VAR_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn generate_user_code_uses_safe_alphabet() {
        // Generate 1000 codes and verify all chars are in the safe alphabet
        for _ in 0..1000 {
            let code = gen_user_code();
            assert_eq!(code.len(), USER_CODE_LENGTH);
            for ch in code.chars() {
                assert!(
                    USER_CODE_ALPHABET.contains(&ch),
                    "Character '{}' not in safe alphabet",
                    ch
                );
            }
        }
    }

    #[test]
    fn generate_device_code_is_url_safe_and_long_enough() {
        // Generate 100 codes and verify they only contain base64url chars
        let base64url_chars: std::collections::HashSet<char> =
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
                .chars()
                .collect();

        for _ in 0..100 {
            let code = gen_device_code();
            // Should be at least 40 chars (32 bytes base64url encoded is ~43 chars)
            assert!(
                code.len() >= 40,
                "Device code too short: {} chars",
                code.len()
            );

            for ch in code.chars() {
                assert!(
                    base64url_chars.contains(&ch),
                    "Character '{}' not in base64url alphabet",
                    ch
                );
            }
        }
    }

    #[test]
    fn hash_token_is_hex_sha256() {
        let token = "test_token_123";
        let hash = hash_token(token);

        // SHA-256 produces 32 bytes = 64 hex chars
        assert_eq!(hash.len(), 64);

        // Should be valid hex
        for ch in hash.chars() {
            assert!(ch.is_ascii_hexdigit(), "Hash contains non-hex character");
        }
    }

    #[test]
    fn hash_token_is_deterministic() {
        let token = "my_test_token";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn hash_token_differs_for_different_inputs() {
        let hash1 = hash_token("token_one");
        let hash2 = hash_token("token_two");
        assert_ne!(hash1, hash2);
    }

    #[tokio::test]
    async fn device_qr_info_uses_host_header_when_present() {
        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::HOST, "example.com".parse().unwrap());

        let result = device_qr_info(headers).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.0.instance_url, "https://example.com");
        assert_eq!(response.0.device_pairing_path, "/device");
    }

    #[tokio::test]
    async fn device_qr_info_uses_x_forwarded_proto_header() {
        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::HOST, "example.com".parse().unwrap());
        headers.insert("x-forwarded-proto", "http".parse().unwrap());

        let result = device_qr_info(headers).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.0.instance_url, "http://example.com");
    }

    #[tokio::test]
    async fn device_qr_info_defaults_to_https_when_no_x_forwarded_proto() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::HOST,
            "secure.example.com".parse().unwrap(),
        );

        let result = device_qr_info(headers).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.0.instance_url, "https://secure.example.com");
    }

    #[tokio::test]
    async fn device_qr_info_uses_env_var_fallback_when_no_host_header() {
        let _lock = ENV_VAR_MUTEX.lock().unwrap();

        // Set the environment variable
        std::env::set_var(
            "RUSTSHARE_PUBLIC_URL",
            "http://env-fallback.example.com:8080",
        );

        let headers = HeaderMap::new();
        let result = device_qr_info(headers).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(
            response.0.instance_url,
            "http://env-fallback.example.com:8080"
        );

        // Clean up
        std::env::remove_var("RUSTSHARE_PUBLIC_URL");
    }

    #[tokio::test]
    async fn device_qr_info_uses_localhost_fallback_when_no_host_or_env_var() {
        let _lock = ENV_VAR_MUTEX.lock().unwrap();

        // Ensure env var is not set
        std::env::remove_var("RUSTSHARE_PUBLIC_URL");

        let headers = HeaderMap::new();
        let result = device_qr_info(headers).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.0.instance_url, "http://localhost:8080");
    }
}
