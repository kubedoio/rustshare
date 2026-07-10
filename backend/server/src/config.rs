use serde::Deserialize;

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
        rename = "RUSTSHARE_OBJECT_STORE_AUTO_CREATE_BUCKET"
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
