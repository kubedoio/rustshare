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
/// leftover env var cannot prevent the server from starting.
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
    /// `0` disables retention cleanup.
    pub retention_hours: i64,
    /// Readiness staleness window: the `outbox` readiness component is only
    /// healthy while the last dispatcher tick is at most this many seconds
    /// old.
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
            readiness_staleness_secs: env_parse("RUSTSHARE_OUTBOX_READINESS_STALENESS_SECS", 60u64),
        };
        // Sanity clamps: a zero/negative value would make the store misbehave
        // (e.g. lease that expires instantly or a batch that claims nothing).
        config.claim_batch_size = config.claim_batch_size.max(1);
        config.lease_secs = config.lease_secs.max(1);
        config.max_attempts = config.max_attempts.max(1);
        config.poll_interval = config.poll_interval.max(Duration::from_millis(1));
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

    const OUTBOX_ENV_VARS: [&str; 9] = [
        "RUSTSHARE_OUTBOX_WORKER_ENABLED",
        "RUSTSHARE_OUTBOX_POLL_INTERVAL_MS",
        "RUSTSHARE_OUTBOX_CLAIM_BATCH_SIZE",
        "RUSTSHARE_OUTBOX_LEASE_SECS",
        "RUSTSHARE_OUTBOX_MAX_ATTEMPTS",
        "RUSTSHARE_OUTBOX_BACKOFF_INITIAL_MS",
        "RUSTSHARE_OUTBOX_BACKOFF_MAX_MS",
        "RUSTSHARE_OUTBOX_RETENTION_HOURS",
        "RUSTSHARE_OUTBOX_READINESS_STALENESS_SECS",
    ];

    fn clear_outbox_env() {
        for name in OUTBOX_ENV_VARS {
            std::env::remove_var(name);
        }
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
        std::env::set_var("RUSTSHARE_OUTBOX_READINESS_STALENESS_SECS", "120");
        std::env::set_var("RUSTSHARE_OUTBOX_CLAIM_BATCH_SIZE", "0");

        let config = OutboxWorkerConfig::from_env();
        assert!(!config.enabled);
        assert_eq!(config.poll_interval, Duration::from_millis(250));
        assert_eq!(config.claim_batch_size, 1, "batch size clamped to >= 1");
        assert_eq!(config.lease_secs, 1, "lease clamped to >= 1 second");
        assert_eq!(config.max_attempts, 1, "max attempts clamped to >= 1");
        assert_eq!(config.backoff_initial_ms, 500);
        assert_eq!(config.backoff_max_ms, 90_000);
        assert_eq!(config.retention_hours, 24);
        assert_eq!(config.readiness_staleness_secs, 120);
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
