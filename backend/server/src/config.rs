use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub database_url: String,
    pub jwt_secret: String,
    #[serde(default = "default_jwt_issuer")]
    pub jwt_issuer: String,
    #[serde(default = "default_jwt_audience")]
    pub jwt_audience: String,
    #[serde(default = "default_jwt_expiry_hours")]
    pub jwt_expiry_hours: i64,
    pub rustfs_endpoint: String,
    pub rustfs_region: String,
    pub rustfs_bucket: String,
    #[serde(
        default = "default_object_store_auto_create_bucket",
        rename = "rustshare_object_store_auto_create_bucket"
    )]
    pub object_store_auto_create_bucket: bool,
    #[serde(default = "default_public_url", rename = "RUSTSHARE_PUBLIC_URL")]
    pub public_url: String,
    #[serde(
        default = "default_storage_quota",
        rename = "RUSTSHARE_DEFAULT_STORAGE_QUOTA_BYTES"
    )]
    pub default_storage_quota_bytes: i64,
    #[serde(default = "default_ai_enabled", rename = "RUSTSHARE_AI_ENABLED")]
    pub ai_enabled: bool,
    #[serde(default = "default_log_format", rename = "RUSTSHARE_LOG_FORMAT")]
    pub log_format: String,
    #[serde(default = "default_pool_max")]
    pub db_pool_max_connections: u32,
    #[serde(default = "default_pool_min")]
    pub db_pool_min_connections: u32,
    #[serde(default = "default_pool_acquire")]
    pub db_pool_acquire_timeout_secs: u64,
    #[serde(default = "default_pool_idle")]
    pub db_pool_idle_timeout_secs: u64,
    #[serde(default = "default_pool_lifetime")]
    pub db_pool_max_lifetime_secs: u64,
    pub rustshare_chat_webhook_secret: String,
    #[serde(default = "default_chat_authority")]
    pub rustshare_chat_authority: String,
    #[serde(default)]
    pub rustshare_chat_bridge_secret_key: Option<String>,
    #[serde(default = "default_chat_provisioning")]
    pub rustshare_chat_provisioning: String,
    #[serde(default)]
    pub rustshare_chat_bootstrap_relay_url: Option<String>,
    #[serde(
        default = "default_bootstrap_password_file",
        rename = "RUSTSHARE_BOOTSTRAP_PASSWORD_FILE"
    )]
    pub bootstrap_password_file: String,
    #[serde(default = "default_broadcast_capacity")]
    pub broadcast_capacity: usize,
    #[serde(
        default = "default_mail_import_worker_enabled",
        rename = "RUSTSHARE_MAIL_IMPORT_WORKER_ENABLED"
    )]
    pub mail_import_worker_enabled: bool,
    #[serde(
        default = "default_mail_import_worker_poll_secs",
        rename = "RUSTSHARE_MAIL_IMPORT_WORKER_POLL_SECS"
    )]
    pub mail_import_worker_poll_secs: u64,
    #[serde(
        default = "default_mail_import_worker_max_concurrent",
        rename = "RUSTSHARE_MAIL_IMPORT_WORKER_MAX_CONCURRENT"
    )]
    pub mail_import_worker_max_concurrent: usize,
    #[serde(
        default = "default_mail_import_worker_stale_secs",
        rename = "RUSTSHARE_MAIL_IMPORT_WORKER_STALE_SECS"
    )]
    pub mail_import_worker_stale_secs: i64,
}

fn default_jwt_issuer() -> String {
    "rustshare".to_string()
}

fn default_jwt_audience() -> String {
    "rustshare-api".to_string()
}

fn default_jwt_expiry_hours() -> i64 {
    24
}

fn default_public_url() -> String {
    "http://localhost:5173".to_string()
}

fn default_storage_quota() -> i64 {
    10_737_418_240
}

fn default_ai_enabled() -> bool {
    true
}

fn default_object_store_auto_create_bucket() -> bool {
    false
}

fn default_log_format() -> String {
    "pretty".to_string()
}

/// Default Buzz authority mode: `local` keeps the coarse community-level gate
/// (see `rustshare_resource_auth::buzz_authority::LocalFallbackAuthority`)
/// until an upstream Buzz authority is configured and provisioned.
fn default_chat_authority() -> String {
    "local".into()
}

/// Default provisioning mode: `manual` keeps the existing explicit admin
/// mapping API (zero-config bootstrap is opt-in via `auto`).
fn default_chat_provisioning() -> String {
    "manual".into()
}

fn default_pool_max() -> u32 {
    50
}

fn default_pool_min() -> u32 {
    5
}

fn default_pool_acquire() -> u64 {
    10
}

fn default_pool_idle() -> u64 {
    300
}

fn default_pool_lifetime() -> u64 {
    1800
}

fn default_bootstrap_password_file() -> String {
    "/tmp/rustshare-bootstrap-password.txt".to_string()
}

fn default_broadcast_capacity() -> usize {
    1000
}

fn default_mail_import_worker_enabled() -> bool {
    true
}

fn default_mail_import_worker_poll_secs() -> u64 {
    10
}

fn default_mail_import_worker_max_concurrent() -> usize {
    2
}

fn default_mail_import_worker_stale_secs() -> i64 {
    300
}

/// Configuration for the durable integration-event outbox dispatcher
/// (ADR-0031 / issue #212).
///
/// Maps onto `rustshare_storage::OutboxConfig` (claim/lease/backoff/retention)
/// plus the dispatcher's own poll interval, enabled flag and readiness
/// staleness window. Values are sanity-clamped (never rejected) so a bogus
/// leftover env var cannot prevent the server from starting. Two special
/// values are intentional: `retention_hours <= 0` disables retention cleanup
/// entirely, and `readiness_staleness_secs = 0` makes the `outbox` readiness
/// component permanently stale (it is informational only and never fails
/// overall readiness).
#[derive(Debug, Clone)]
pub struct OutboxWorkerConfig {
    /// Whether the dispatcher loop is spawned. Publishing into the outbox
    /// stays active regardless; a disabled worker just means events
    /// accumulate until it is enabled again.
    pub enabled: bool,
    /// Poll interval between dispatcher ticks.
    pub poll_interval: Duration,
    /// Maximum rows claimed per consumer per tick.
    pub claim_batch_size: i64,
    /// Lease duration in seconds for a claimed delivery.
    pub lease_secs: i64,
    /// Maximum attempts before a delivery is dead-lettered.
    pub max_attempts: i32,
    /// Initial retry backoff in milliseconds.
    pub backoff_initial_ms: u64,
    /// Maximum retry backoff in milliseconds.
    pub backoff_max_ms: u64,
    /// Outbox retention in hours before fully-delivered rows are compacted;
    /// `<= 0` disables retention cleanup.
    pub retention_hours: i64,
    /// Per-event processing deadline: a consumer that does not return within
    /// this window has its delivery failed retryable (bounded backoff, then
    /// DLQ) so a wedged consumer cannot stall the dispatch loop.
    pub process_timeout: Duration,
    /// Readiness staleness window: the `outbox` readiness component is only
    /// healthy while the last dispatcher tick is at most this many seconds
    /// old. `0` makes the component permanently stale; the component is
    /// informational and never fails overall readiness.
    pub readiness_staleness_secs: u64,
}

impl Default for OutboxWorkerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_interval: Duration::from_millis(1000),
            claim_batch_size: 50,
            lease_secs: 60,
            max_attempts: 5,
            backoff_initial_ms: 1000,
            backoff_max_ms: 300_000,
            retention_hours: 168,
            process_timeout: Duration::from_secs(60),
            readiness_staleness_secs: 60,
        }
    }
}

impl OutboxWorkerConfig {
    pub fn from_env() -> Self {
        let mut config = Self {
            enabled: env_parse("RUSTSHARE_OUTBOX_WORKER_ENABLED", true),
            poll_interval: Duration::from_millis(env_parse(
                "RUSTSHARE_OUTBOX_POLL_INTERVAL_MS",
                1000u64,
            )),
            claim_batch_size: env_parse("RUSTSHARE_OUTBOX_CLAIM_BATCH_SIZE", 50i64),
            lease_secs: env_parse("RUSTSHARE_OUTBOX_LEASE_SECS", 60i64),
            max_attempts: env_parse("RUSTSHARE_OUTBOX_MAX_ATTEMPTS", 5i32),
            backoff_initial_ms: env_parse("RUSTSHARE_OUTBOX_BACKOFF_INITIAL_MS", 1000u64),
            backoff_max_ms: env_parse("RUSTSHARE_OUTBOX_BACKOFF_MAX_MS", 300_000u64),
            retention_hours: env_parse("RUSTSHARE_OUTBOX_RETENTION_HOURS", 168i64),
            process_timeout: Duration::from_secs(env_parse(
                "RUSTSHARE_OUTBOX_PROCESS_TIMEOUT_SECS",
                60u64,
            )),
            readiness_staleness_secs: env_parse("RUSTSHARE_OUTBOX_READINESS_STALENESS_SECS", 60u64),
        };
        // Sanity clamps: a zero/negative value would make the store misbehave
        // (e.g. lease that expires instantly or a batch that claims nothing),
        // and an unbounded backoff would overflow the database's
        // `timestamptz` on `now() + interval` and wedge deliveries claimed
        // forever.
        config.claim_batch_size = config.claim_batch_size.max(1);
        config.lease_secs = config.lease_secs.max(1);
        config.max_attempts = config.max_attempts.max(1);
        config.poll_interval = config.poll_interval.max(Duration::from_millis(1));
        config.process_timeout = config.process_timeout.max(Duration::from_secs(1));
        config.backoff_initial_ms = config.backoff_initial_ms.min(86_400_000); // 1 day
        config.backoff_max_ms = config.backoff_max_ms.min(86_400_000); // 1 day
        config
    }
}

fn env_parse<T>(name: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// Valid `RUSTSHARE_CHAT_AUTHORITY` values. `buzz` activates the upstream
/// source-authorization client (built by a later task); `local` keeps the
/// coarse `LocalFallbackAuthority` community-level gate.
const CHAT_AUTHORITY_VALUES: &str = "local|buzz";

/// Chat community provisioning mode (zero-config bootstrap, ADR-0036).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatProvisioningMode {
    /// Enable-Chat auto-provisions the deployment Buzz community (single
    /// workspace model). Requires a bootstrap relay URL.
    Auto,
    /// Mapping is configured explicitly by an administrator (existing API).
    Manual,
}

impl ChatProvisioningMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatProvisioningMode::Auto => "auto",
            ChatProvisioningMode::Manual => "manual",
        }
    }

    /// Parse a `RUSTSHARE_CHAT_PROVISIONING` value (round-trip with
    /// [`Self::as_str`]); anything else is a configuration error.
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(ChatProvisioningMode::Auto),
            "manual" => Ok(ChatProvisioningMode::Manual),
            other => Err(format!(
                "invalid RUSTSHARE_CHAT_PROVISIONING {other:?} (expected auto|manual)"
            )),
        }
    }
}

/// Whether `value` is exactly 64 lowercase hex characters — the shape of
/// Nostr x-only public keys and secret keys, and of the DB CHECK on
/// `relay_pubkey`.
pub(crate) fn is_lowercase_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Fail closed at startup on an invalid chat authority configuration. A
/// silent fallback to `local` would be a wrong-authorization bug, so this
/// deliberately rejects `buzz` without a valid bridge key (unlike
/// `BuzzAdmissionBridge::from_env`, which warns and disables).
fn validate_chat_authority(config: &AppConfig, errors: &mut Vec<String>) {
    match config.rustshare_chat_authority.as_str() {
        "local" => {}
        "buzz" => {
            let Some(key) = config.rustshare_chat_bridge_secret_key.as_deref() else {
                errors.push(
                    "RUSTSHARE_CHAT_AUTHORITY is 'buzz' but RUSTSHARE_CHAT_BRIDGE_SECRET_KEY is not set; failing closed (a silent fallback to local would be a wrong-authorization bug)".to_string(),
                );
                return;
            };
            // The shape gate keeps the documented format (64 lowercase hex);
            // `nostr::Keys::parse` — the exact parser
            // `BuzzAdmissionBridge::from_env` uses at runtime — then rejects
            // strings that pass the shape check but are not a valid 32-byte
            // secret key scalar (e.g. all-zeros), so such a key fails startup
            // instead of silently disabling the bridge at runtime.
            if !is_lowercase_hex_64(key) || nostr::Keys::parse(key).is_err() {
                errors.push(
                    "RUSTSHARE_CHAT_BRIDGE_SECRET_KEY must be a valid Nostr secret key (64 lowercase hex characters) when RUSTSHARE_CHAT_AUTHORITY is 'buzz'".to_string(),
                );
            }
        }
        other => errors.push(format!(
            "RUSTSHARE_CHAT_AUTHORITY must be one of {CHAT_AUTHORITY_VALUES}, got {other:?}"
        )),
    }
}

/// Fail closed at startup on an invalid chat provisioning configuration. In
/// `auto` mode, zero-config bootstrap requires the Buzz authority and a
/// ws/wss bootstrap relay URL; any violation rejects startup. The URL is only
/// shape-checked here — the gateway's `validated_http` enforces the SSRF pin
/// per request, so no DNS resolution happens at startup.
fn validate_chat_provisioning(config: &AppConfig, errors: &mut Vec<String>) {
    let mode = match ChatProvisioningMode::parse(&config.rustshare_chat_provisioning) {
        Ok(mode) => mode,
        Err(message) => {
            errors.push(message);
            return;
        }
    };
    match mode {
        ChatProvisioningMode::Manual => {}
        ChatProvisioningMode::Auto => {
            if config.rustshare_chat_authority != "buzz" {
                errors.push(
                    "RUSTSHARE_CHAT_PROVISIONING=auto requires RUSTSHARE_CHAT_AUTHORITY=buzz"
                        .to_string(),
                );
            }
            let Some(relay_url) = config.rustshare_chat_bootstrap_relay_url.as_deref() else {
                errors.push(
                    "RUSTSHARE_CHAT_PROVISIONING=auto requires RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL"
                        .to_string(),
                );
                return;
            };
            match url::Url::parse(relay_url) {
                Ok(url) => {
                    if !matches!(url.scheme(), "wss" | "ws") || url.host_str().is_none() {
                        errors.push(
                            "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL must use ws:// or wss:// and include a host"
                                .to_string(),
                        );
                    }
                }
                Err(error) => errors.push(format!(
                    "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL is not a valid URL: {error}"
                )),
            }
        }
    }
}

impl AppConfig {
    pub fn from_env() -> Result<Self, Vec<String>> {
        match envy::from_env::<Self>() {
            Ok(config) => {
                let mut errors = Vec::new();
                if config.database_url.is_empty() {
                    errors.push("DATABASE_URL is required".to_string());
                }
                if config.jwt_secret.len() < 32 {
                    errors.push(
                        "JWT_SECRET must be at least 32 characters. Generate one with: openssl rand -base64 32".to_string(),
                    );
                }
                if config.jwt_secret == "dev-secret-change-in-production"
                    || config.jwt_secret == "dev-secret-key-change-in-production-12345"
                    || config.jwt_secret == "ci-pilot-secret"
                {
                    errors.push(
                        "JWT_SECRET is using a known weak default value. Generate a strong secret with: openssl rand -base64 32".to_string(),
                    );
                }
                if config.rustfs_endpoint.is_empty() {
                    errors.push("RUSTFS_ENDPOINT is required".to_string());
                }
                if config.rustfs_region.is_empty() {
                    errors.push("RUSTFS_REGION is required".to_string());
                }
                if config.rustfs_bucket.is_empty() {
                    errors.push("RUSTFS_BUCKET is required".to_string());
                }
                if config.rustshare_chat_webhook_secret.is_empty() {
                    errors.push("RUSTSHARE_CHAT_WEBHOOK_SECRET is required".to_string());
                }
                validate_chat_authority(&config, &mut errors);
                validate_chat_provisioning(&config, &mut errors);
                if errors.is_empty() {
                    Ok(config)
                } else {
                    Err(errors)
                }
            }
            Err(e) => Err(vec![format!("Configuration error: {}", e)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env mutation is process-global; serialize the config tests so they
    /// cannot clobber each other's variables.
    static ENV_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

    const OUTBOX_ENV_VARS: [&str; 10] = [
        "RUSTSHARE_OUTBOX_WORKER_ENABLED",
        "RUSTSHARE_OUTBOX_POLL_INTERVAL_MS",
        "RUSTSHARE_OUTBOX_CLAIM_BATCH_SIZE",
        "RUSTSHARE_OUTBOX_LEASE_SECS",
        "RUSTSHARE_OUTBOX_MAX_ATTEMPTS",
        "RUSTSHARE_OUTBOX_BACKOFF_INITIAL_MS",
        "RUSTSHARE_OUTBOX_BACKOFF_MAX_MS",
        "RUSTSHARE_OUTBOX_RETENTION_HOURS",
        "RUSTSHARE_OUTBOX_PROCESS_TIMEOUT_SECS",
        "RUSTSHARE_OUTBOX_READINESS_STALENESS_SECS",
    ];

    fn clear_outbox_env() {
        for name in OUTBOX_ENV_VARS {
            std::env::remove_var(name);
        }
    }

    const CHAT_AUTHORITY_ENV_VARS: [&str; 2] = [
        "RUSTSHARE_CHAT_AUTHORITY",
        "RUSTSHARE_CHAT_BRIDGE_SECRET_KEY",
    ];

    const CHAT_PROVISIONING_ENV_VARS: [&str; 2] = [
        "RUSTSHARE_CHAT_PROVISIONING",
        "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL",
    ];

    /// A minimal `AppConfig::from_env` environment that passes all existing
    /// required-field checks, with the chat authority vars cleared.
    fn set_valid_base_env() {
        // Preserve an already-configured DATABASE_URL: the config tests only
        // need `from_env()` to parse, and clobbering the real URL races with
        // concurrent tests in the same binary (e.g. handlers::auth::tests::
        // login_*, which connect to the configured database). Only fall back
        // to a dummy URL when none is set (bare `cargo test --lib` runs).
        if std::env::var_os("DATABASE_URL").is_none() {
            std::env::set_var("DATABASE_URL", "postgres://test:test@localhost:5432/test");
        }
        std::env::set_var("JWT_SECRET", "test-jwt-secret-0123456789abcdef0123456789");
        std::env::set_var("RUSTFS_ENDPOINT", "http://localhost:9000");
        std::env::set_var("RUSTFS_REGION", "us-east-1");
        std::env::set_var("RUSTFS_BUCKET", "test-bucket");
        std::env::set_var("RUSTSHARE_CHAT_WEBHOOK_SECRET", "test-webhook-secret");
        for name in CHAT_AUTHORITY_ENV_VARS
            .into_iter()
            .chain(CHAT_PROVISIONING_ENV_VARS)
        {
            std::env::remove_var(name);
        }
    }

    #[test]
    fn chat_authority_defaults_to_local() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        let config = AppConfig::from_env().expect("base env must validate");
        assert_eq!(config.rustshare_chat_authority, "local");
        assert_eq!(config.rustshare_chat_bridge_secret_key, None);
    }

    #[test]
    fn chat_authority_buzz_requires_bridge_secret_key() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        let errors = AppConfig::from_env().expect_err("buzz without a key must fail closed");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_authority_rejects_unknown_value() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "mystery");
        let errors = AppConfig::from_env().expect_err("unknown authority must fail");
        assert!(
            errors.iter().any(|error| error.contains("local|buzz")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_authority_buzz_with_valid_key_passes() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "a".repeat(64));
        let config = AppConfig::from_env().expect("buzz with a valid key must pass");
        assert_eq!(config.rustshare_chat_authority, "buzz");
        let expected = "a".repeat(64);
        assert_eq!(
            config.rustshare_chat_bridge_secret_key.as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn chat_authority_buzz_rejects_malformed_key() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "not-a-64-hex-key");
        let errors = AppConfig::from_env().expect_err("malformed key must fail closed");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_authority_rejects_empty_value() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "");
        let errors = AppConfig::from_env().expect_err("empty authority must fail");
        assert!(
            errors.iter().any(|error| error.contains("local|buzz")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_authority_rejects_whitespace_value() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "   ");
        let errors = AppConfig::from_env().expect_err("whitespace authority must fail");
        assert!(
            errors.iter().any(|error| error.contains("local|buzz")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_authority_buzz_rejects_empty_bridge_secret_key() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "");
        let errors = AppConfig::from_env().expect_err("empty bridge key must fail closed");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_authority_buzz_rejects_uppercase_hex_key() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "A".repeat(64));
        let errors = AppConfig::from_env().expect_err("uppercase hex key must fail closed");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_authority_buzz_rejects_zero_scalar_key() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        // 64 lowercase hex that passes the shape check but is not a valid
        // 32-byte secret key scalar — `nostr::Keys::parse` rejects it, and so
        // must startup (the runtime bridge would otherwise silently disable).
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "0".repeat(64));
        let errors = AppConfig::from_env().expect_err("zero scalar key must fail closed");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_authority_buzz_with_valid_parsable_key_passes() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        // Scalar 1 — a valid 32-byte secret key that `nostr::Keys::parse`
        // accepts.
        std::env::set_var(
            "RUSTSHARE_CHAT_BRIDGE_SECRET_KEY",
            "0000000000000000000000000000000000000000000000000000000000000001",
        );
        let config = AppConfig::from_env().expect("buzz with a valid key must pass");
        assert_eq!(config.rustshare_chat_authority, "buzz");
    }

    #[test]
    fn chat_provisioning_auto_with_buzz_and_ws_url_passes() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "a".repeat(64));
        std::env::set_var("RUSTSHARE_CHAT_PROVISIONING", "auto");
        std::env::set_var(
            "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL",
            "wss://chat.example.test",
        );
        let config = AppConfig::from_env().expect("auto+buzz+ws must validate");
        assert_eq!(config.rustshare_chat_provisioning, "auto");
        assert_eq!(
            config.rustshare_chat_bootstrap_relay_url.as_deref(),
            Some("wss://chat.example.test")
        );
    }

    #[test]
    fn chat_provisioning_auto_with_local_authority_fails() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_PROVISIONING", "auto");
        std::env::set_var(
            "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL",
            "wss://chat.example.test",
        );
        let errors = AppConfig::from_env().expect_err("auto without buzz must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("requires RUSTSHARE_CHAT_AUTHORITY=buzz")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_provisioning_auto_without_relay_url_fails() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "a".repeat(64));
        std::env::set_var("RUSTSHARE_CHAT_PROVISIONING", "auto");
        let errors = AppConfig::from_env().expect_err("auto without a relay URL must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("requires RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_provisioning_rejects_non_ws_scheme() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "a".repeat(64));
        std::env::set_var("RUSTSHARE_CHAT_PROVISIONING", "auto");
        std::env::set_var(
            "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL",
            "https://chat.example.test",
        );
        let errors = AppConfig::from_env().expect_err("auto with a non-ws scheme must fail");
        assert!(
            errors.iter().any(|error| error.contains("ws:// or wss://")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_provisioning_rejects_invalid_mode() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_PROVISIONING", "mystery");
        let errors = AppConfig::from_env().expect_err("unknown provisioning mode must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("invalid RUSTSHARE_CHAT_PROVISIONING")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_provisioning_defaults_to_manual() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        let config = AppConfig::from_env().expect("base env must validate");
        assert_eq!(config.rustshare_chat_provisioning, "manual");
        assert_eq!(config.rustshare_chat_bootstrap_relay_url, None);
    }

    #[test]
    fn from_env_uses_defaults_when_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_outbox_env();
        let config = OutboxWorkerConfig::from_env();
        assert!(config.enabled);
        assert_eq!(config.poll_interval, Duration::from_millis(1000));
        assert_eq!(config.claim_batch_size, 50);
        assert_eq!(config.lease_secs, 60);
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.backoff_initial_ms, 1000);
        assert_eq!(config.backoff_max_ms, 300_000);
        assert_eq!(config.retention_hours, 168);
        assert_eq!(config.process_timeout, Duration::from_secs(60));
        assert_eq!(config.readiness_staleness_secs, 60);
    }

    #[test]
    fn from_env_parses_and_sanity_clamps() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_outbox_env();
        std::env::set_var("RUSTSHARE_OUTBOX_WORKER_ENABLED", "false");
        std::env::set_var("RUSTSHARE_OUTBOX_POLL_INTERVAL_MS", "250");
        std::env::set_var("RUSTSHARE_OUTBOX_CLAIM_BATCH_SIZE", "0");
        std::env::set_var("RUSTSHARE_OUTBOX_LEASE_SECS", "-5");
        std::env::set_var("RUSTSHARE_OUTBOX_MAX_ATTEMPTS", "0");
        std::env::set_var("RUSTSHARE_OUTBOX_BACKOFF_INITIAL_MS", "500");
        std::env::set_var("RUSTSHARE_OUTBOX_BACKOFF_MAX_MS", "90000");
        std::env::set_var("RUSTSHARE_OUTBOX_RETENTION_HOURS", "24");
        std::env::set_var("RUSTSHARE_OUTBOX_PROCESS_TIMEOUT_SECS", "0");
        std::env::set_var("RUSTSHARE_OUTBOX_READINESS_STALENESS_SECS", "120");

        let config = OutboxWorkerConfig::from_env();
        assert!(!config.enabled);
        assert_eq!(config.poll_interval, Duration::from_millis(250));
        assert_eq!(config.claim_batch_size, 1, "batch size clamped to >= 1");
        assert_eq!(config.lease_secs, 1, "lease clamped to >= 1 second");
        assert_eq!(config.max_attempts, 1, "max attempts clamped to >= 1");
        assert_eq!(config.backoff_initial_ms, 500);
        assert_eq!(config.backoff_max_ms, 90_000);
        assert_eq!(config.retention_hours, 24);
        assert_eq!(
            config.process_timeout,
            Duration::from_secs(1),
            "process timeout clamped to >= 1 second"
        );
        assert_eq!(config.readiness_staleness_secs, 120);
    }

    #[test]
    fn from_env_clamps_backoff_to_one_day() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_outbox_env();
        // u64::MAX would overflow `timestamptz` on `now() + interval` and
        // wedge deliveries claimed forever; both backoffs must be clamped.
        std::env::set_var(
            "RUSTSHARE_OUTBOX_BACKOFF_INITIAL_MS",
            "18446744073709551615",
        );
        std::env::set_var("RUSTSHARE_OUTBOX_BACKOFF_MAX_MS", "18446744073709551615");

        let config = OutboxWorkerConfig::from_env();
        assert_eq!(
            config.backoff_initial_ms, 86_400_000,
            "initial backoff clamped to 1 day"
        );
        assert_eq!(
            config.backoff_max_ms, 86_400_000,
            "max backoff clamped to 1 day"
        );
    }

    #[test]
    fn from_env_ignores_unparseable_values() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_outbox_env();
        std::env::set_var("RUSTSHARE_OUTBOX_MAX_ATTEMPTS", "not-a-number");
        std::env::set_var("RUSTSHARE_OUTBOX_RETENTION_HOURS", "0"); // 0 disables retention
        let config = OutboxWorkerConfig::from_env();
        assert_eq!(config.max_attempts, 5, "unparseable falls back to default");
        assert_eq!(config.retention_hours, 0);
    }
}
