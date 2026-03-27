//! Device pairing authentication handlers.
//!
//! Provides endpoints for the device pairing flow:
//! - POST /api/v1/auth/device/request - Generate user_code and device_code
//! - POST /api/v1/auth/device/poll - Check approval status and issue token
//! - POST /api/v1/auth/device/approve - Approve a pairing request

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
use std::time::{Duration, Instant};
use uuid::Uuid;

use rustshare_storage::metadata_v2::schemas::{
    DeviceTokenDocument, PairingRequestDocument, PairingStatus,
};

use crate::handlers::AuthenticatedUser;
use crate::AppState;

/// User code alphabet - excludes ambiguous characters: 0, O, 1, I, L
const USER_CODE_ALPHABET: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'M', 'N', 'P', 'Q', 'R', 'S', 'T', 'U',
    'V', 'W', 'X', 'Y', 'Z', '2', '3', '4', '5', '6', '7', '8', '9',
];

const USER_CODE_LENGTH: usize = 8;
const DEVICE_CODE_LENGTH: usize = 32;
const TOKEN_LENGTH: usize = 32;
const POLL_RATE_LIMIT_SECONDS: u64 = 5;
const PAIRING_TTL_SECONDS: i64 = 300; // 5 minutes

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
    AuthenticatedUser { user_id }: AuthenticatedUser,
    Json(body): Json<DeviceApproveRequest>,
) -> Result<Json<DeviceApproveResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Find the pairing request by user_code
    let pairing = state.pairing_repo
        .get_by_user_code(&body.user_code.to_uppercase())
        .await
        .map_err(|e| {
            tracing::error!("Failed to get pairing request: {}", e);
            internal_error("Failed to process approval")
        })?;
    
    let mut pairing = pairing.ok_or_else(|| {
        not_found("Invalid or expired user code")
    })?;
    
    // Check if already approved or expired
    if pairing.status == PairingStatus::Approved {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Pairing request already approved"})),
        ));
    }
    
    if pairing.is_expired() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Pairing request has expired"})),
        ));
    }
    
    if pairing.status == PairingStatus::Expired {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Pairing request has expired"})),
        ));
    }
    
    // Generate a device token
    let token = gen_token();
    let token_hash = hash_token(&token);
    
    // Create the device token
    let device_id = Uuid::new_v4();
    let device = DeviceTokenDocument::new(
        device_id,
        user_id,
        token_hash,
        pairing.device_name.clone().unwrap_or_else(|| "Unknown Device".to_string()),
        pairing.device_type.clone().unwrap_or_else(|| "unknown".to_string()),
        None, // No expiration
    );
    
    // Save the device
    state.device_repo
        .create(&device)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create device: {}", e);
            internal_error("Failed to create device")
        })?;
    
    // Mark pairing as approved
    pairing.approve(user_id, device_id, token);
    
    state.pairing_repo
        .update(&pairing)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update pairing: {}", e);
            internal_error("Failed to complete approval")
        })?;
    
    Ok(Json(DeviceApproveResponse {
        device_name: device.device_name,
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

fn not_found(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": msg })))
}

fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
}

/// POST /api/v1/auth/device/request
/// Generates a new device pair request with user_code and device_code
pub async fn device_request(
    State(state): State<AppState>,
) -> Result<Json<DeviceRequestResponse>, (StatusCode, Json<serde_json::Value>)> {
    let user_code = gen_user_code();
    let device_code = gen_device_code();
    
    // Create a pairing request
    let pairing_id = Uuid::new_v4();
    let token = gen_token();
    let token_hash = hash_token(&token);
    
    let pairing = PairingRequestDocument::new(
        pairing_id,
        user_code.clone(),
        device_code.clone(),
        token_hash,
        PAIRING_TTL_SECONDS,
    );
    
    state.pairing_repo
        .create(&pairing)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create pairing request: {}", e);
            internal_error("Failed to create pairing request")
        })?;
    
    Ok(Json(DeviceRequestResponse {
        user_code,
        device_code,
        expires_in: PAIRING_TTL_SECONDS,
    }))
}

/// POST /api/v1/auth/device/poll
/// Polls for approval status and issues token when approved
pub async fn device_poll(
    State(state): State<AppState>,
    _headers: HeaderMap,
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

    // Get the pairing request
    let pairing = state.pairing_repo
        .get_by_device_code(&req.device_code)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get pairing request: {}", e);
            internal_error("Failed to check pairing status")
        })?;
    
    let pairing = match pairing {
        Some(p) => p,
        None => {
            return Ok(Json(DevicePollResponse::Expired {}).into_response());
        }
    };
    
    // Check if expired
    if pairing.is_expired() {
        return Ok(Json(DevicePollResponse::Expired {}).into_response());
    }
    
    // Check status
    match pairing.status {
        PairingStatus::Approved => {
            if let Some(token) = pairing.access_token {
                Ok(Json(DevicePollResponse::Approved { token }).into_response())
            } else {
                // This shouldn't happen, but handle it gracefully
                Ok(Json(DevicePollResponse::Pending {}).into_response())
            }
        }
        PairingStatus::Expired => {
            Ok(Json(DevicePollResponse::Expired {}).into_response())
        }
        _ => {
            Ok(Json(DevicePollResponse::Pending {}).into_response())
        }
    }
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
        headers.insert(axum::http::header::HOST, "secure.example.com".parse().unwrap());

        let result = device_qr_info(headers).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.0.instance_url, "https://secure.example.com");
    }

    #[tokio::test]
    async fn device_qr_info_uses_env_var_fallback_when_no_host_header() {
        let _lock = ENV_VAR_MUTEX.lock().unwrap();

        // Set the environment variable
        std::env::set_var("RUSTSHARE_PUBLIC_URL", "http://env-fallback.example.com:8080");

        let headers = HeaderMap::new();
        let result = device_qr_info(headers).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.0.instance_url, "http://env-fallback.example.com:8080");

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
