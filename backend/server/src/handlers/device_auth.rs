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
use rand::{distr::Uniform, RngExt};
use rustshare_infrastructure::repositories::{
    DeviceApprovalBearer, DeviceApprovalLookup, DeviceApprovalOutcome, DeviceRepository,
    DeviceTokenIssueOutcome,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Percent-encoding set that preserves RFC 3986 unreserved characters.
const URL_SAFE: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

use super::ErrorResponse;
use crate::handlers::AppError;
use crate::handlers::AuthenticatedUser;
use crate::oidc_runtime::load_oidc_runtime_settings;
use crate::state::{AppConfigState, AppState, DatabaseState};
use crate::web_session::extract_cookie_value;

/// User code alphabet - excludes ambiguous characters: 0, O, 1, I, L
const USER_CODE_ALPHABET: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'M', 'N', 'P', 'Q', 'R', 'S', 'T', 'U', 'V',
    'W', 'X', 'Y', 'Z', '2', '3', '4', '5', '6', '7', '8', '9',
];

const USER_CODE_LENGTH: usize = 8;
const DEVICE_CODE_LENGTH: usize = 32;
const TOKEN_LENGTH: usize = 32;
const POLL_RATE_LIMIT_SECONDS: u64 = 5;

/// Pairing link path used by the frontend approval flow.
const DEVICE_APPROVAL_PATH: &str = "/device/approve";

/// Response for QR info endpoint
#[derive(Serialize, utoipa::ToSchema)]
pub struct DeviceQrInfoResponse {
    pub instance_url: String,
    pub device_pairing_path: String,
}

/// GET /api/v1/auth/device/qr-info
/// Returns information needed for QR code generation on the device pairing page
#[utoipa::path(
    get,
    path = "/api/v1/auth/device/qr-info",
    tag = "Auth",
    responses(
        (status = 200, description = "Success", body = DeviceQrInfoResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn device_qr_info(headers: HeaderMap) -> Result<Json<DeviceQrInfoResponse>, AppError> {
    let instance_url = build_instance_url(&headers);

    Ok(Json(DeviceQrInfoResponse {
        instance_url,
        device_pairing_path: "/device".to_string(),
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
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
#[derive(Deserialize, utoipa::ToSchema)]
pub struct DeviceApproveRequest {
    pub user_code: Option<String>,
    pub device_code: Option<String>,
}

/// Response for device approval
#[derive(Serialize, utoipa::ToSchema)]
pub struct DeviceApproveResponse {
    pub device_name: String,
}

/// Response for device request
#[derive(Serialize, utoipa::ToSchema)]
pub struct DeviceRequestResponse {
    pub user_code: String,
    pub device_code: String,
    pub expires_in: i64,
    pub verification_uri: String,
    pub verification_uri_complete: String,
}

/// POST /api/v1/auth/device/approve
/// Approves a device pair request using either a user_code or device_code
#[utoipa::path(
    post,
    path = "/api/v1/auth/device/approve",
    tag = "Auth",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn device_approve(
    State(state): State<AppState>,
    headers: HeaderMap,
    AuthenticatedUser { user_id, tenant_id }: AuthenticatedUser,
    Json(body): Json<DeviceApproveRequest>,
) -> Result<Json<DeviceApproveResponse>, AppError> {
    let lookup = validate_device_approval_request(&body)?;
    approve_device_pair_request(
        &DeviceRepository::new(state.db_pool.clone()),
        &state.jwt_manager,
        user_id,
        tenant_id,
        &headers,
        lookup,
    )
    .await?;

    // Return device_name - the actual device name is captured at poll time
    // when the token is created, so we return a placeholder here
    Ok(Json(DeviceApproveResponse {
        device_name: "Device".to_string(),
    }))
}

async fn approve_device_pair_request(
    repository: &DeviceRepository,
    jwt_manager: &rustshare_auth::JwtManager,
    user_id: Uuid,
    tenant_id: Uuid,
    headers: &HeaderMap,
    lookup: DeviceApprovalLookup,
) -> Result<(), AppError> {
    let session_token_hash = extract_cookie_value(headers, rustshare_auth::WEB_SESSION_COOKIE_NAME)
        .map(|token| rustshare_auth::hash_web_session_token(&token));
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, token)| scheme.eq_ignore_ascii_case("Bearer") && !token.is_empty())
        .map(|(_, token)| -> Result<DeviceApprovalBearer, AppError> {
            match jwt_manager.validate(token) {
                Ok(claims) => Ok(match Uuid::parse_str(&claims.sub) {
                    Ok(user_id) => DeviceApprovalBearer::Jwt {
                        user_id,
                        issued_at: claims.iat,
                        credential_version: claims.credential_version,
                    },
                    Err(_) => DeviceApprovalBearer::InvalidJwt,
                }),
                Err(_) => Ok(DeviceApprovalBearer::DeviceTokenHash(hex::encode(
                    Sha256::digest(token.as_bytes()),
                ))),
            }
        })
        .transpose()?;
    match repository
        .approve_pair_request(
            user_id,
            tenant_id,
            session_token_hash.as_deref(),
            bearer.as_ref(),
            &lookup,
        )
        .await?
    {
        DeviceApprovalOutcome::Approved => Ok(()),
        DeviceApprovalOutcome::NotFound => Err(AppError::not_found("code_not_found")),
        DeviceApprovalOutcome::AlreadyApproved => Err(AppError::conflict("already_approved")),
        DeviceApprovalOutcome::AccountDisabled => Err(AppError::forbidden("account_disabled")),
        DeviceApprovalOutcome::Unauthorized => Err(AppError::Unauthorized),
    }
}

/// Determine how a device approval request should be resolved.
fn validate_device_approval_request(
    body: &DeviceApproveRequest,
) -> Result<DeviceApprovalLookup, AppError> {
    match (&body.user_code, &body.device_code) {
        (Some(user_code), None) if !user_code.trim().is_empty() => {
            Ok(DeviceApprovalLookup::UserCode(user_code.clone()))
        }
        (None, Some(device_code)) if !device_code.trim().is_empty() => {
            Ok(DeviceApprovalLookup::DeviceCode(device_code.clone()))
        }
        (Some(_), Some(_)) => Err(AppError::bad_request(
            "approve_request_accepts_only_one_identifier",
        )),
        (None, None) => Err(AppError::bad_request(
            "approve_request_requires_user_code_or_device_code",
        )),
        _ => Err(AppError::bad_request(
            "approve_request_identifier_must_not_be_empty",
        )),
    }
}

/// Build the public instance URL from request headers.
fn build_instance_url(headers: &HeaderMap) -> String {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let protocol = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("https");

    match host {
        Some(host) => format!("{}://{}", protocol, host),
        None => std::env::var("RUSTSHARE_PUBLIC_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string()),
    }
}

/// Build the verification and deep-link URLs for device pairing.
fn build_device_pairing_uris(instance_url: &str, device_code: &str) -> (String, String) {
    let verification_uri = format!(
        "{}{}",
        instance_url.trim_end_matches('/'),
        DEVICE_APPROVAL_PATH
    );
    let verification_uri_complete = format!(
        "{}?device_code={}",
        verification_uri,
        percent_encoding::percent_encode(device_code.as_bytes(), URL_SAFE)
    );

    (verification_uri, verification_uri_complete)
}

fn build_device_request_response(
    instance_url: &str,
    device_code: String,
    user_code: String,
    expires_in: i64,
) -> DeviceRequestResponse {
    let (verification_uri, verification_uri_complete) =
        build_device_pairing_uris(instance_url, &device_code);

    DeviceRequestResponse {
        user_code,
        device_code,
        expires_in,
        verification_uri,
        verification_uri_complete,
    }
}

/// Generate a random user code (8 chars from safe alphabet)
fn gen_user_code() -> String {
    let mut rng = rand::rng();
    let alphabet = Uniform::new(0, USER_CODE_ALPHABET.len()).unwrap();

    (0..USER_CODE_LENGTH)
        .map(|_| USER_CODE_ALPHABET[rng.sample(alphabet)])
        .collect()
}

/// Generate a random device code (32 bytes, base64url-encoded)
fn gen_device_code() -> String {
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..DEVICE_CODE_LENGTH).map(|_| rng.random()).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// Generate a random token (32 bytes, base64url-encoded)
fn gen_token() -> String {
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..TOKEN_LENGTH).map(|_| rng.random()).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// Hash a token using SHA-256, return hex string
fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

/// POST /api/v1/auth/device/request
/// Generates a new device pair request with user_code and device_code
#[utoipa::path(
    post,
    path = "/api/v1/auth/device/request",
    tag = "Auth",
    responses(
        (status = 200, description = "Success", body = DeviceRequestResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn device_request(
    State(state): State<crate::state::AppState>,
    headers: HeaderMap,
) -> Result<Json<DeviceRequestResponse>, AppError> {
    let ttl_seconds = load_oidc_runtime_settings(&state)
        .await
        .map(|settings| settings.device_pair_code_ttl_seconds())
        .unwrap_or(300);
    let ttl_seconds_i64 = i64::from(ttl_seconds);

    let user_code = gen_user_code();
    let device_code = gen_device_code();

    DeviceRepository::new(state.db_pool.clone())
        .create_pair_request(&device_code, &user_code, f64::from(ttl_seconds))
        .await?;

    let instance_url = build_instance_url(&headers);
    Ok(Json(build_device_request_response(
        &instance_url,
        device_code,
        user_code,
        ttl_seconds_i64,
    )))
}

/// POST /api/v1/auth/device/poll
/// Polls for approval status and issues token when approved
#[utoipa::path(
    post,
    path = "/api/v1/auth/device/poll",
    tag = "Auth",
    request_body = DevicePollRequest,
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn device_poll(
    State(db): State<DatabaseState>,
    State(config): State<AppConfigState>,
    headers: HeaderMap,
    Json(req): Json<DevicePollRequest>,
) -> Result<Response, AppError> {
    device_poll_inner(
        &db.device_repository,
        &config.poll_rate_limiter,
        headers,
        req,
    )
    .await
}

async fn device_poll_inner(
    repository: &DeviceRepository,
    poll_rate_limiter: &std::sync::Arc<
        tokio::sync::Mutex<std::collections::HashMap<String, Instant>>,
    >,
    headers: HeaderMap,
    req: DevicePollRequest,
) -> Result<Response, AppError> {
    // Check rate limit
    let now = Instant::now();
    let rate_limit_key = req.device_code.clone();

    {
        let mut rate_limiter = poll_rate_limiter.lock().await;

        if let Some(last_request) = rate_limiter.get(&rate_limit_key) {
            let elapsed = now.duration_since(*last_request);
            if elapsed < Duration::from_secs(POLL_RATE_LIMIT_SECONDS) {
                let retry_after = POLL_RATE_LIMIT_SECONDS - elapsed.as_secs();
                return Ok((
                    StatusCode::TOO_MANY_REQUESTS,
                    [(axum::http::header::RETRY_AFTER, retry_after.to_string())],
                    Json(ErrorResponse::new("Rate limit exceeded")),
                )
                    .into_response());
            }
        }

        // Prune stale entries so the map cannot grow without bound: every poll
        // creates a fresh key, and device codes expire within minutes, so any
        // entry older than the retention window is dead weight.
        const POLL_LIMITER_RETENTION: Duration = Duration::from_secs(15 * 60);
        rate_limiter
            .retain(|_, last_request| now.duration_since(*last_request) < POLL_LIMITER_RETENTION);

        rate_limiter.insert(rate_limit_key, now);
    }

    // Look up the pair request
    let row = repository.pair_request_for_poll(&req.device_code).await?;

    let (user_id_opt, is_approved, is_expired) = match row {
        Some(row) => {
            let user_id = row.user_id;
            let is_approved = row.is_approved.unwrap_or(false);
            let is_expired = row.is_expired.unwrap_or(true);
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
        if let Err(e) = repository.delete_pair_request(&req.device_code).await {
            tracing::debug!(device_code = %req.device_code, error = %e, "failed to clean up expired device pair request");
        }

        return Ok(Json(DevicePollResponse::Expired {}).into_response());
    }

    // Check if still pending
    if !is_approved {
        return Ok(Json(DevicePollResponse::Pending {}).into_response());
    }

    // Approved - generate token and clean up
    let user_id =
        user_id_opt.ok_or_else(|| AppError::internal("Approved request missing user_id"))?;

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
    match repository
        .issue_token_for_approved_pair(&req.device_code, user_id, &token_hash, &device_name)
        .await?
    {
        DeviceTokenIssueOutcome::Issued => {
            Ok(Json(DevicePollResponse::Approved { token: raw_token }).into_response())
        }
        DeviceTokenIssueOutcome::Expired => {
            Ok(Json(DevicePollResponse::Expired {}).into_response())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::Row;
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::sync::Mutex as AsyncMutex;

    const TEST_DATABASE_URL: &str = "postgres://rustshare:changeme@localhost:5432/rustshare";
    const TEST_TOKEN: &str = "test-token-123";
    const TEST_TOKEN_2: &str = "my-test-token";

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
        let token = TEST_TOKEN;
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
        let token = TEST_TOKEN_2;
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
    #[allow(clippy::await_holding_lock)]
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
    #[allow(clippy::await_holding_lock)]
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

    #[test]
    fn build_device_pairing_uris_uses_approval_path_and_device_code() {
        let (verification_uri, verification_uri_complete) =
            build_device_pairing_uris("https://example.com", "device-code-123");

        assert_eq!(verification_uri, "https://example.com/device/approve");
        assert_eq!(
            verification_uri_complete,
            "https://example.com/device/approve?device_code=device-code-123"
        );
    }

    #[test]
    fn build_device_request_response_includes_verification_links() {
        let response = build_device_request_response(
            "https://rustshare.example.com",
            "device-code-123".to_string(),
            "ABCD1234".to_string(),
            300,
        );

        assert_eq!(response.user_code, "ABCD1234");
        assert_eq!(response.device_code, "device-code-123");
        assert_eq!(response.expires_in, 300);
        assert_eq!(
            response.verification_uri,
            "https://rustshare.example.com/device/approve"
        );
        assert_eq!(
            response.verification_uri_complete,
            "https://rustshare.example.com/device/approve?device_code=device-code-123"
        );
    }

    #[test]
    fn validate_device_approval_request_requires_exactly_one_identifier() {
        let user_code_only = DeviceApproveRequest {
            user_code: Some("ABCD1234".to_string()),
            device_code: None,
        };
        let user_code_empty = DeviceApproveRequest {
            user_code: Some("".to_string()),
            device_code: None,
        };
        let device_code_only = DeviceApproveRequest {
            user_code: None,
            device_code: Some("device-code-123".to_string()),
        };
        let device_code_empty = DeviceApproveRequest {
            user_code: None,
            device_code: Some("".to_string()),
        };
        let both = DeviceApproveRequest {
            user_code: Some("ABCD1234".to_string()),
            device_code: Some("device-code-123".to_string()),
        };
        let neither = DeviceApproveRequest {
            user_code: None,
            device_code: None,
        };

        assert!(matches!(
            validate_device_approval_request(&user_code_only),
            Ok(DeviceApprovalLookup::UserCode(code)) if code == "ABCD1234"
        ));
        assert!(matches!(
            validate_device_approval_request(&user_code_empty),
            Err(AppError::BadRequest(_))
        ));
        assert!(matches!(
            validate_device_approval_request(&device_code_only),
            Ok(DeviceApprovalLookup::DeviceCode(code)) if code == "device-code-123"
        ));
        assert!(matches!(
            validate_device_approval_request(&device_code_empty),
            Err(AppError::BadRequest(_))
        ));
        assert!(matches!(
            validate_device_approval_request(&both),
            Err(AppError::BadRequest(_))
        ));
        assert!(matches!(
            validate_device_approval_request(&neither),
            Err(AppError::BadRequest(_))
        ));
    }

    async fn test_db_pool() -> sqlx::PgPool {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());

        PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(60))
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database")
    }

    async fn insert_pair_request(
        pool: &sqlx::PgPool,
        device_code: &str,
        user_code: &str,
        user_id: Option<Uuid>,
        expires_at: chrono::DateTime<chrono::Utc>,
        approved_at: Option<chrono::DateTime<chrono::Utc>>,
    ) {
        sqlx::query(
            r#"
            INSERT INTO device_pair_requests (id, device_code, user_code, user_id, expires_at, approved_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(device_code)
        .bind(user_code)
        .bind(user_id)
        .bind(expires_at)
        .bind(approved_at)
        .execute(pool)
        .await
        .expect("Failed to insert test pair request");
    }

    async fn insert_test_user(pool: &sqlx::PgPool, user_id: Uuid) {
        let suffix = user_id.as_simple().to_string();
        sqlx::query(
            r#"
            INSERT INTO users (
                id,
                username,
                email,
                password_hash,
                display_name,
                is_admin,
                storage_quota,
                tenant_id
            )
            VALUES ($1, $2, $3, $4, $5, FALSE, $6, $7)
            "#,
        )
        .bind(user_id)
        .bind(format!("device_pairing_test_{}", suffix))
        .bind(format!("device_pairing_test_{}@example.com", suffix))
        .bind("test-password-hash")
        .bind("Device Pairing Test")
        .bind(10_737_418_240_i64)
        .bind(Uuid::nil())
        .execute(pool)
        .await
        .expect("Failed to insert test user");
    }

    async fn cleanup_test_user(pool: &sqlx::PgPool, user_id: Uuid) {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .ok();
    }

    fn test_approval_auth(user_id: Uuid) -> (rustshare_auth::JwtManager, HeaderMap) {
        let manager = rustshare_auth::JwtManager::new(
            "device_pair_test_secret_at_least_32_chars",
            "rustshare",
            "rustshare-api",
            24,
        );
        let token = manager
            .generate(user_id, "device-pair@test.local", Uuid::nil())
            .expect("test JWT");
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}")
                .parse()
                .expect("authorization header"),
        );
        (manager, headers)
    }

    async fn device_token_count(pool: &sqlx::PgPool, user_id: Uuid) -> i64 {
        sqlx::query(
            r#"
            SELECT COUNT(*)::bigint AS count
            FROM device_tokens
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(pool)
        .await
        .expect("Failed to count device tokens")
        .get::<Option<i64>, _>("count")
        .unwrap_or(0)
    }

    async fn pair_request_count(pool: &sqlx::PgPool, device_code: &str) -> i64 {
        sqlx::query(
            r#"
            SELECT COUNT(*)::bigint AS count
            FROM device_pair_requests
            WHERE device_code = $1
            "#,
        )
        .bind(device_code)
        .fetch_one(pool)
        .await
        .expect("Failed to count device pair requests")
        .get::<Option<i64>, _>("count")
        .unwrap_or(0)
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn expired_device_code_cannot_be_approved() {
        let pool = test_db_pool().await;
        let device_code = "expired-device-code-123";
        let user_code = "EXPIRED12";
        let user_id = Uuid::new_v4();

        insert_test_user(&pool, user_id).await;

        insert_pair_request(
            &pool,
            device_code,
            user_code,
            Some(user_id),
            chrono::Utc::now() - chrono::Duration::minutes(1),
            None,
        )
        .await;

        let (jwt_manager, headers) = test_approval_auth(user_id);
        let result = approve_device_pair_request(
            &DeviceRepository::new(pool.clone()),
            &jwt_manager,
            user_id,
            Uuid::nil(),
            &headers,
            DeviceApprovalLookup::DeviceCode(device_code.to_string()),
        )
        .await;

        assert!(matches!(result, Err(AppError::NotFound(_))));
        assert_eq!(device_token_count(&pool, user_id).await, 0);

        sqlx::query("DELETE FROM device_pair_requests WHERE device_code = $1")
            .bind(device_code)
            .execute(&pool)
            .await
            .ok();
        cleanup_test_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn approval_rejects_a_different_tenant_without_claiming_the_request() {
        let pool = test_db_pool().await;
        let device_code = format!("tenant-device-{}", Uuid::new_v4());
        let user_code = "TENANT12";
        let user_id = Uuid::new_v4();
        insert_test_user(&pool, user_id).await;
        insert_pair_request(
            &pool,
            &device_code,
            user_code,
            None,
            chrono::Utc::now() + chrono::Duration::minutes(5),
            None,
        )
        .await;

        let (jwt_manager, headers) = test_approval_auth(user_id);
        let result = approve_device_pair_request(
            &DeviceRepository::new(pool.clone()),
            &jwt_manager,
            user_id,
            Uuid::new_v4(),
            &headers,
            DeviceApprovalLookup::DeviceCode(device_code.clone()),
        )
        .await;

        assert!(matches!(result, Err(AppError::Forbidden(_))));
        let approved_user: Option<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM device_pair_requests WHERE device_code = $1")
                .bind(&device_code)
                .fetch_one(&pool)
                .await
                .expect("pair request should remain");
        assert_eq!(approved_user, None);

        sqlx::query("DELETE FROM device_pair_requests WHERE device_code = $1")
            .bind(device_code)
            .execute(&pool)
            .await
            .ok();
        cleanup_test_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn approval_rejects_a_revoked_jwt_credential_version() {
        let pool = test_db_pool().await;
        let device_code = format!("revoked-device-{}", Uuid::new_v4());
        let user_id = Uuid::new_v4();
        insert_test_user(&pool, user_id).await;
        insert_pair_request(
            &pool,
            &device_code,
            "REVOKED1",
            None,
            chrono::Utc::now() + chrono::Duration::minutes(5),
            None,
        )
        .await;
        let (jwt_manager, headers) = test_approval_auth(user_id);
        sqlx::query("UPDATE users SET credential_version = credential_version + 1 WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("revoke test JWT");

        let result = approve_device_pair_request(
            &DeviceRepository::new(pool.clone()),
            &jwt_manager,
            user_id,
            Uuid::nil(),
            &headers,
            DeviceApprovalLookup::DeviceCode(device_code.clone()),
        )
        .await;

        assert!(matches!(result, Err(AppError::Unauthorized)));
        let approved_user: Option<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM device_pair_requests WHERE device_code = $1")
                .bind(&device_code)
                .fetch_one(&pool)
                .await
                .expect("pair request should remain");
        assert_eq!(approved_user, None);

        sqlx::query("DELETE FROM device_pair_requests WHERE device_code = $1")
            .bind(device_code)
            .execute(&pool)
            .await
            .ok();
        cleanup_test_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn current_session_takes_precedence_over_jwt_with_non_uuid_subject() {
        #[derive(serde::Serialize)]
        struct Claims {
            sub: &'static str,
            email: &'static str,
            tenant_id: Uuid,
            exp: i64,
            iat: i64,
            credential_version: i64,
            iss: &'static str,
            aud: &'static str,
        }

        let pool = test_db_pool().await;
        let user_id = Uuid::new_v4();
        let suffix = user_id.as_simple().to_string();
        let session_token = rustshare_auth::generate_web_session_token();
        let session_id = Uuid::new_v4();
        let session_hash = rustshare_auth::hash_web_session_token(&session_token);
        let good_device_code = format!("session-device-{suffix}");
        let bad_device_code = format!("jwt-device-{suffix}");
        insert_test_user(&pool, user_id).await;
        sqlx::query(
            "INSERT INTO user_sessions (id, user_id, session_token_hash, expires_at, tenant_id)
             VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4)",
        )
        .bind(session_id)
        .bind(user_id)
        .bind(&session_hash)
        .bind(Uuid::nil())
        .execute(&pool)
        .await
        .expect("insert current web session");
        for (device_code, user_code) in [
            (&good_device_code, "GOODSESS"),
            (&bad_device_code, "BADSESS1"),
        ] {
            insert_pair_request(
                &pool,
                device_code,
                user_code,
                None,
                chrono::Utc::now() + chrono::Duration::minutes(5),
                None,
            )
            .await;
        }

        let (jwt_manager, mut headers) = test_approval_auth(user_id);
        let now = chrono::Utc::now().timestamp();
        let invalid_subject_jwt = jwt_manager
            .encode_custom_claims(&Claims {
                sub: "not-a-uuid",
                email: "device-pair@test.local",
                tenant_id: Uuid::nil(),
                exp: now + 600,
                iat: now,
                credential_version: 0,
                iss: "rustshare",
                aud: "rustshare-api",
            })
            .expect("signed JWT with non-UUID subject");
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {invalid_subject_jwt}")
                .parse()
                .expect("authorization header"),
        );
        headers.insert(
            axum::http::header::COOKIE,
            format!(
                "{}={session_token}",
                rustshare_auth::WEB_SESSION_COOKIE_NAME
            )
            .parse()
            .expect("session cookie"),
        );

        approve_device_pair_request(
            &DeviceRepository::new(pool.clone()),
            &jwt_manager,
            user_id,
            Uuid::nil(),
            &headers,
            DeviceApprovalLookup::DeviceCode(good_device_code.clone()),
        )
        .await
        .expect("current session must take precedence over bearer JWT subject");

        headers.remove(axum::http::header::COOKIE);
        let result = approve_device_pair_request(
            &DeviceRepository::new(pool.clone()),
            &jwt_manager,
            user_id,
            Uuid::nil(),
            &headers,
            DeviceApprovalLookup::DeviceCode(bad_device_code.clone()),
        )
        .await;
        assert!(matches!(result, Err(AppError::Unauthorized)));

        sqlx::query("DELETE FROM user_sessions WHERE id = $1")
            .bind(session_id)
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DELETE FROM device_pair_requests WHERE device_code = ANY($1)")
            .bind(vec![good_device_code, bad_device_code])
            .execute(&pool)
            .await
            .ok();
        cleanup_test_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn approval_does_not_mint_token_until_poll() {
        let pool = test_db_pool().await;
        let device_code = "poll-device-code-123";
        let user_code = "POLL1234";
        let user_id = Uuid::new_v4();
        let rate_limiter = std::sync::Arc::new(AsyncMutex::new(std::collections::HashMap::new()));

        insert_test_user(&pool, user_id).await;

        insert_pair_request(
            &pool,
            device_code,
            user_code,
            None,
            chrono::Utc::now() + chrono::Duration::minutes(5),
            None,
        )
        .await;

        let (jwt_manager, headers) = test_approval_auth(user_id);
        approve_device_pair_request(
            &DeviceRepository::new(pool.clone()),
            &jwt_manager,
            user_id,
            Uuid::nil(),
            &headers,
            DeviceApprovalLookup::DeviceCode(device_code.to_string()),
        )
        .await
        .expect("approval should succeed");

        assert_eq!(device_token_count(&pool, user_id).await, 0);

        let response = device_poll_inner(
            &DeviceRepository::new(pool.clone()),
            &rate_limiter,
            HeaderMap::new(),
            DevicePollRequest {
                device_code: device_code.to_string(),
            },
        )
        .await
        .expect("poll should succeed");

        assert_eq!(response.status(), StatusCode::OK);

        assert_eq!(device_token_count(&pool, user_id).await, 1);
        assert_eq!(pair_request_count(&pool, device_code).await, 0);

        sqlx::query("DELETE FROM device_tokens WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .ok();
        cleanup_test_user(&pool, user_id).await;
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn approved_pair_request_is_removed_after_poll_completion() {
        let pool = test_db_pool().await;
        let device_code = "cleanup-device-code-123";
        let user_code = "CLEANUP1";
        let user_id = Uuid::new_v4();
        let rate_limiter = std::sync::Arc::new(AsyncMutex::new(std::collections::HashMap::new()));

        insert_test_user(&pool, user_id).await;

        insert_pair_request(
            &pool,
            device_code,
            user_code,
            Some(user_id),
            chrono::Utc::now() + chrono::Duration::minutes(5),
            Some(chrono::Utc::now()),
        )
        .await;

        let _ = device_poll_inner(
            &DeviceRepository::new(pool.clone()),
            &rate_limiter,
            HeaderMap::new(),
            DevicePollRequest {
                device_code: device_code.to_string(),
            },
        )
        .await
        .expect("poll should succeed for approved request");

        assert_eq!(pair_request_count(&pool, device_code).await, 0);

        sqlx::query("DELETE FROM device_tokens WHERE user_id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .ok();
        cleanup_test_user(&pool, user_id).await;
    }
}
