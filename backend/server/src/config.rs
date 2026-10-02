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
    /// NOTE: `envy` lowercases every environment variable name before matching,
    /// so a `rename` must be the *lowercase* form of the env var
    /// (`RUSTSHARE_PUBLIC_URL` → `rustshare_public_url`). An uppercase rename
    /// silently never matches and the field keeps its default.
    #[serde(default = "default_public_url", rename = "rustshare_public_url")]
    pub public_url: String,
    #[serde(
        default = "default_storage_quota",
        rename = "rustshare_default_storage_quota_bytes"
    )]
    pub default_storage_quota_bytes: i64,
    #[serde(default = "default_ai_enabled", rename = "rustshare_ai_enabled")]
    pub ai_enabled: bool,
    #[serde(default = "default_log_format", rename = "rustshare_log_format")]
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
        rename = "rustshare_bootstrap_password_file"
    )]
    pub bootstrap_password_file: String,
    #[serde(default = "default_broadcast_capacity")]
    pub broadcast_capacity: usize,
    #[serde(
        default = "default_mail_import_worker_enabled",
        rename = "rustshare_mail_import_worker_enabled"
    )]
    pub mail_import_worker_enabled: bool,
    #[serde(
        default = "default_mail_import_worker_poll_secs",
        rename = "rustshare_mail_import_worker_poll_secs"
    )]
    pub mail_import_worker_poll_secs: u64,
    #[serde(
        default = "default_mail_import_worker_max_concurrent",
        rename = "rustshare_mail_import_worker_max_concurrent"
    )]
    pub mail_import_worker_max_concurrent: usize,
    #[serde(
        default = "default_mail_import_worker_stale_secs",
        rename = "rustshare_mail_import_worker_stale_secs"
    )]
    pub mail_import_worker_stale_secs: i64,
    #[serde(
        default = "default_calendar_import_worker_enabled",
        rename = "rustshare_calendar_import_worker_enabled"
    )]
    pub calendar_import_worker_enabled: bool,
    #[serde(
        default = "default_calendar_import_worker_poll_secs",
        rename = "rustshare_calendar_import_worker_poll_secs"
    )]
    pub calendar_import_worker_poll_secs: u64,
    #[serde(
        default = "default_calendar_import_worker_max_concurrent",
        rename = "rustshare_calendar_import_worker_max_concurrent"
    )]
    pub calendar_import_worker_max_concurrent: usize,
    #[serde(
        default = "default_calendar_import_worker_stale_secs",
        rename = "rustshare_calendar_import_worker_stale_secs"
    )]
    pub calendar_import_worker_stale_secs: i64,
    /// Google Calendar OAuth client credentials. Absent = provider
    /// unconfigured (connect returns 503, not a startup error).
    #[serde(default, rename = "rustshare_calendar_google_client_id")]
    pub calendar_google_client_id: Option<String>,
    #[serde(default, rename = "rustshare_calendar_google_client_secret")]
    pub calendar_google_client_secret: Option<String>,
    /// Microsoft/Outlook OAuth client credentials. Absent = provider
    /// unconfigured (connect returns 503, not a startup error).
    #[serde(default, rename = "rustshare_calendar_microsoft_client_id")]
    pub calendar_microsoft_client_id: Option<String>,
    #[serde(default, rename = "rustshare_calendar_microsoft_client_secret")]
    pub calendar_microsoft_client_secret: Option<String>,
    #[serde(
        default = "default_calendar_sync_worker_enabled",
        rename = "rustshare_calendar_sync_worker_enabled"
    )]
    pub calendar_sync_worker_enabled: bool,
    #[serde(
        default = "default_calendar_sync_worker_poll_secs",
        rename = "rustshare_calendar_sync_worker_poll_secs"
    )]
    pub calendar_sync_worker_poll_secs: u64,
    #[serde(
        default = "default_calendar_sync_worker_max_concurrent",
        rename = "rustshare_calendar_sync_worker_max_concurrent"
    )]
    pub calendar_sync_worker_max_concurrent: usize,
    #[serde(
        default = "default_calendar_sync_worker_stale_secs",
        rename = "rustshare_calendar_sync_worker_stale_secs"
    )]
    pub calendar_sync_worker_stale_secs: i64,
    #[serde(
        default = "default_calendar_sync_past_days",
        rename = "rustshare_calendar_sync_past_days"
    )]
    pub calendar_sync_past_days: i64,
    #[serde(
        default = "default_calendar_sync_future_days",
        rename = "rustshare_calendar_sync_future_days"
    )]
    pub calendar_sync_future_days: i64,
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

/// Path suffixes of the two Calendar OAuth callback endpoints. The redirect
/// URI an operator must register verbatim in each provider console is
/// `{RUSTSHARE_PUBLIC_URL}{path}`.
pub const CALENDAR_GOOGLE_CALLBACK_PATH: &str = "/api/v1/calendar/oauth/google/callback";
pub const CALENDAR_OUTLOOK_CALLBACK_PATH: &str = "/api/v1/calendar/oauth/outlook/callback";

/// The debug/dev frontend-origin default. A release build must not derive
/// OAuth redirect URIs (or share/device links) from this unless the operator
/// explicitly opts in with `RUSTSHARE_ALLOW_DEV_PUBLIC_URL=1`.
const DEV_PUBLIC_URL: &str = "http://localhost:5173";

/// Whether `RUSTSHARE_ALLOW_DEV_PUBLIC_URL` permits the dev default. Unlike a
/// plain `envy` bool this also accepts `1`/`yes`, matching how operators
/// commonly write env toggles.
fn dev_public_url_allowed() -> bool {
    matches!(
        std::env::var("RUSTSHARE_ALLOW_DEV_PUBLIC_URL")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Validate `RUSTSHARE_PUBLIC_URL` (one error string appended to `errors`).
///
/// Rules, in order:
///   * absolute `http`/`https` URL with a host (no relative/other schemes);
///   * in release builds the `http://localhost:5173` dev default is rejected
///     unless `allow_dev` is set — silently advertising a dead dev port is
///     how the Calendar connect flow broke (issue #315);
///   * non-loopback hosts must use `https` (redirect URIs carrying OAuth
///     authorization codes must not travel in cleartext).
fn validate_public_url(url: &str, allow_dev: bool, is_release: bool, errors: &mut Vec<String>) {
    let parsed = match url::Url::parse(url) {
        Ok(parsed) => parsed,
        Err(error) => {
            errors.push(format!(
                "RUSTSHARE_PUBLIC_URL must be an absolute http:// or https:// URL, got {url:?}: {error}"
            ));
            return;
        }
    };
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        errors.push(format!(
            "RUSTSHARE_PUBLIC_URL must be an absolute http:// or https:// URL with a host, got {url:?}"
        ));
        return;
    }
    let is_loopback = is_loopback_host(&parsed);
    let is_dev_default = is_localhost_name(&parsed)
        && parsed.port() == Some(5173)
        && matches!(parsed.path(), "" | "/");
    if is_release && !allow_dev && is_dev_default {
        errors.push(format!(
            "RUSTSHARE_PUBLIC_URL is still the development default {DEV_PUBLIC_URL}; set it to this \
             deployment's public origin (for example https://app.example.com) or set \
             RUSTSHARE_ALLOW_DEV_PUBLIC_URL=1 for a throwaway local environment"
        ));
        return;
    }
    if !is_loopback && parsed.scheme() != "https" {
        errors.push(format!(
            "RUSTSHARE_PUBLIC_URL must use https for non-local hosts (got {url:?}); Calendar OAuth \
             redirect URIs carry authorization codes and may not travel in cleartext"
        ));
    }
}

/// Whether the parsed URL's host is a loopback address: the name `localhost`
/// (with or without a trailing dot), the IPv6 loopback `::1`, or any IPv4
/// address in the whole `127.0.0.0/8` range — not only the literal
/// `127.0.0.1`. Used to decide whether cleartext `http` is acceptable.
fn is_loopback_host(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        Some(url::Host::Domain(domain)) => domain
            .trim_end_matches('.')
            .eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

/// Whether the parsed URL's host is the name `localhost` (a trailing dot
/// denotes the same name).
fn is_localhost_name(url: &url::Url) -> bool {
    matches!(
        url.host(),
        Some(url::Host::Domain(domain))
            if domain.trim_end_matches('.').eq_ignore_ascii_case("localhost")
    )
}

/// Normalize `RUSTSHARE_PUBLIC_URL` at startup: trailing `/` characters are
/// stripped so the derived OAuth redirect URIs (`{public_url}{callback_path}`)
/// never contain a doubled slash (`https://app.example.com//api/v1/...`). The
/// stored value therefore never carries a trailing slash.
fn normalize_public_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
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

fn default_calendar_import_worker_enabled() -> bool {
    true
}

fn default_calendar_import_worker_poll_secs() -> u64 {
    10
}

fn default_calendar_import_worker_max_concurrent() -> usize {
    2
}

fn default_calendar_import_worker_stale_secs() -> i64 {
    300
}

fn default_calendar_sync_worker_enabled() -> bool {
    true
}

fn default_calendar_sync_worker_poll_secs() -> u64 {
    10
}

fn default_calendar_sync_worker_max_concurrent() -> usize {
    2
}

fn default_calendar_sync_worker_stale_secs() -> i64 {
    300
}

fn default_calendar_sync_past_days() -> i64 {
    90
}

fn default_calendar_sync_future_days() -> i64 {
    365
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
                    if !matches!(url.scheme(), "wss" | "ws")
                        || url.host_str().is_none()
                        || !url.username().is_empty()
                        || url.password().is_some()
                        || (url.path() != "" && url.path() != "/")
                        || url.query().is_some()
                        || url.fragment().is_some()
                        || url.port() == Some(0)
                    {
                        errors.push(
                            "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL must use ws:// or wss:// with a host, no credentials, no path/query/fragment, and a non-zero port"
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
            Ok(mut config) => {
                // An empty or whitespace-only `RUSTSHARE_PUBLIC_URL` behaves as
                // unset. The compose passthrough injects an empty string when
                // the operator's root `.env` omits the variable, and validating
                // that literal would fail with a misleading `got ""` instead of
                // the actionable "still the development default" guidance.
                // Fall back to the compiled default, then normalize.
                if config.public_url.trim().is_empty() {
                    config.public_url = default_public_url();
                }
                // Normalize once here so every consumer (startup logging, both
                // provider clients, and the provider-status endpoint) derives
                // redirect URIs without a doubled slash.
                config.public_url = normalize_public_url(&config.public_url);
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
                validate_public_url(
                    &config.public_url,
                    dev_public_url_allowed(),
                    !cfg!(debug_assertions),
                    &mut errors,
                );
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
        // A valid, non-dev public URL: localhost keeps the http-vs-https rule
        // permissive and is not the rejected dev default.
        std::env::set_var("RUSTSHARE_PUBLIC_URL", "http://localhost:8080");
        std::env::remove_var("RUSTSHARE_ALLOW_DEV_PUBLIC_URL");
        for name in CHAT_AUTHORITY_ENV_VARS
            .into_iter()
            .chain(CHAT_PROVISIONING_ENV_VARS)
        {
            std::env::remove_var(name);
        }
        for name in [
            "RUSTSHARE_CALENDAR_GOOGLE_CLIENT_ID",
            "RUSTSHARE_CALENDAR_GOOGLE_CLIENT_SECRET",
            "RUSTSHARE_CALENDAR_MICROSOFT_CLIENT_ID",
            "RUSTSHARE_CALENDAR_MICROSOFT_CLIENT_SECRET",
        ] {
            std::env::remove_var(name);
        }
    }

    /// The 18 switches that used to carry uppercase `serde(rename = ...)`
    /// values: `envy` lowercases env names, so those renames never matched and
    /// the fields kept their defaults. A representative sample (log format, the
    /// AI toggle, quota, a mail worker knob, a calendar import worker knob, and
    /// the sync window bounds) must now be read from the environment.
    #[test]
    fn from_env_reads_previously_ignored_switches() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_LOG_FORMAT", "json");
        std::env::set_var("RUSTSHARE_AI_ENABLED", "false");
        std::env::set_var("RUSTSHARE_DEFAULT_STORAGE_QUOTA_BYTES", "123456789");
        std::env::set_var("RUSTSHARE_MAIL_IMPORT_WORKER_POLL_SECS", "42");
        std::env::set_var("RUSTSHARE_CALENDAR_IMPORT_WORKER_ENABLED", "false");
        std::env::set_var("RUSTSHARE_CALENDAR_SYNC_PAST_DAYS", "7");
        std::env::set_var("RUSTSHARE_CALENDAR_SYNC_FUTURE_DAYS", "30");

        let config = AppConfig::from_env().expect("valid env must pass");
        assert_eq!(config.log_format, "json");
        assert!(!config.ai_enabled);
        assert_eq!(config.default_storage_quota_bytes, 123_456_789);
        assert_eq!(config.mail_import_worker_poll_secs, 42);
        assert!(!config.calendar_import_worker_enabled);
        assert_eq!(config.calendar_sync_past_days, 7);
        assert_eq!(config.calendar_sync_future_days, 30);
        // Untouched switches keep their documented defaults.
        assert_eq!(config.calendar_sync_worker_stale_secs, 300);
        assert_eq!(config.mail_import_worker_max_concurrent, 2);

        for name in [
            "RUSTSHARE_LOG_FORMAT",
            "RUSTSHARE_AI_ENABLED",
            "RUSTSHARE_DEFAULT_STORAGE_QUOTA_BYTES",
            "RUSTSHARE_MAIL_IMPORT_WORKER_POLL_SECS",
            "RUSTSHARE_CALENDAR_IMPORT_WORKER_ENABLED",
            "RUSTSHARE_CALENDAR_SYNC_PAST_DAYS",
            "RUSTSHARE_CALENDAR_SYNC_FUTURE_DAYS",
        ] {
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
    fn chat_provisioning_rejects_port_zero() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "a".repeat(64));
        std::env::set_var("RUSTSHARE_CHAT_PROVISIONING", "auto");
        std::env::set_var(
            "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL",
            "ws://chat.example.test:0",
        );
        let errors = AppConfig::from_env().expect_err("auto with port 0 must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_provisioning_rejects_non_root_path() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "a".repeat(64));
        std::env::set_var("RUSTSHARE_CHAT_PROVISIONING", "auto");
        std::env::set_var(
            "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL",
            "ws://chat.example.test/path",
        );
        let errors = AppConfig::from_env().expect_err("auto with a path must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_provisioning_rejects_query() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "a".repeat(64));
        std::env::set_var("RUSTSHARE_CHAT_PROVISIONING", "auto");
        std::env::set_var(
            "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL",
            "wss://chat.example.test?x=1",
        );
        let errors = AppConfig::from_env().expect_err("auto with a query must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn chat_provisioning_rejects_userinfo() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CHAT_AUTHORITY", "buzz");
        std::env::set_var("RUSTSHARE_CHAT_BRIDGE_SECRET_KEY", "a".repeat(64));
        std::env::set_var("RUSTSHARE_CHAT_PROVISIONING", "auto");
        std::env::set_var(
            "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL",
            "ws://user:pass@chat.example.test",
        );
        let errors = AppConfig::from_env().expect_err("auto with userinfo must fail");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL")),
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

    fn public_url_errors(url: &str, allow_dev: bool, is_release: bool) -> Vec<String> {
        let mut errors = Vec::new();
        validate_public_url(url, allow_dev, is_release, &mut errors);
        errors
    }

    #[test]
    fn public_url_accepts_absolute_http_and_https() {
        assert!(public_url_errors("https://app.example.com", false, true).is_empty());
        assert!(public_url_errors("http://localhost:8080", false, true).is_empty());
        assert!(public_url_errors("http://127.0.0.1:5173", false, true).is_empty());
        assert!(public_url_errors("https://app.example.com:8443", false, true).is_empty());
    }

    #[test]
    fn public_url_rejects_relative_and_non_http_schemes() {
        for url in [
            "",
            "/relative",
            "ftp://host",
            "javascript:alert(1)",
            "host:8080",
        ] {
            let errors = public_url_errors(url, false, true);
            assert!(
                errors.iter().any(|e| e.contains("RUSTSHARE_PUBLIC_URL")),
                "{url:?} must be rejected: {errors:?}"
            );
        }
    }

    #[test]
    fn public_url_rejects_plain_http_for_non_local_hosts() {
        let errors = public_url_errors("http://app.example.com", false, true);
        assert!(
            errors.iter().any(|e| e.contains("must use https")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn public_url_rejects_dev_default_in_release_unless_allowed() {
        let errors = public_url_errors("http://localhost:5173", false, true);
        assert!(
            errors.iter().any(|e| e.contains("development default")),
            "errors: {errors:?}"
        );
        // Trailing slash and an explicit port variant are the same origin.
        assert!(!public_url_errors("http://localhost:5173/", false, true).is_empty());
        // Debug builds keep the convenience default.
        assert!(public_url_errors("http://localhost:5173", false, false).is_empty());
        // Explicit opt-in is honoured.
        assert!(public_url_errors("http://localhost:5173", true, true).is_empty());
    }

    #[test]
    fn from_env_accepts_non_dev_public_url() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_PUBLIC_URL", "https://app.example.com");
        let config = AppConfig::from_env().expect("valid public URL must pass");
        assert_eq!(config.public_url, "https://app.example.com");
    }

    #[test]
    fn from_env_treats_blank_public_url_as_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        // The compose passthrough injects an empty string when the operator's
        // root `.env` omits RUSTSHARE_PUBLIC_URL. It must fall back to the
        // compiled default rather than failing validation with `got ""`.
        for blank in ["", "   "] {
            std::env::set_var("RUSTSHARE_PUBLIC_URL", blank);
            // Tests run in debug builds, where the compiled dev default is
            // accepted; the resolved value is what matters here.
            let config =
                AppConfig::from_env().expect("blank public URL must fall back to the default");
            assert_eq!(config.public_url, DEV_PUBLIC_URL);
        }
        // In a release build that same fallback is rejected with the
        // actionable "still the development default" message, not `got ""`.
        let errors = public_url_errors(&default_public_url(), false, true);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("development default")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn from_env_reads_calendar_provider_credentials() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_CALENDAR_GOOGLE_CLIENT_ID", "google-id");
        std::env::set_var("RUSTSHARE_CALENDAR_GOOGLE_CLIENT_SECRET", "google-secret");
        std::env::set_var("RUSTSHARE_CALENDAR_MICROSOFT_CLIENT_ID", "ms-id");
        std::env::set_var("RUSTSHARE_CALENDAR_MICROSOFT_CLIENT_SECRET", "ms-secret");
        let config = AppConfig::from_env().expect("valid env must pass");
        assert_eq!(
            config.calendar_google_client_id.as_deref(),
            Some("google-id")
        );
        assert_eq!(
            config.calendar_google_client_secret.as_deref(),
            Some("google-secret")
        );
        assert_eq!(
            config.calendar_microsoft_client_id.as_deref(),
            Some("ms-id")
        );
        assert_eq!(
            config.calendar_microsoft_client_secret.as_deref(),
            Some("ms-secret")
        );
        for name in [
            "RUSTSHARE_CALENDAR_GOOGLE_CLIENT_ID",
            "RUSTSHARE_CALENDAR_GOOGLE_CLIENT_SECRET",
            "RUSTSHARE_CALENDAR_MICROSOFT_CLIENT_ID",
            "RUSTSHARE_CALENDAR_MICROSOFT_CLIENT_SECRET",
        ] {
            std::env::remove_var(name);
        }
    }

    #[test]
    fn from_env_rejects_plain_http_public_url_for_non_local_host() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        std::env::set_var("RUSTSHARE_PUBLIC_URL", "http://app.example.com");
        let errors = AppConfig::from_env().expect_err("http on a public host must fail");
        assert!(
            errors.iter().any(|e| e.contains("must use https")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn from_env_normalizes_trailing_slash_in_public_url() {
        let _guard = ENV_LOCK.lock().unwrap();
        set_valid_base_env();
        // A trailing slash must not survive into the derived redirect URIs
        // (`{public_url}{path}` would otherwise double the slash).
        std::env::set_var("RUSTSHARE_PUBLIC_URL", "https://app.example.com/");
        let config = AppConfig::from_env().expect("valid public URL must pass");
        assert_eq!(config.public_url, "https://app.example.com");
        assert_eq!(
            format!("{}{}", config.public_url, CALENDAR_GOOGLE_CALLBACK_PATH),
            "https://app.example.com/api/v1/calendar/oauth/google/callback"
        );
        // Multiple trailing slashes and a path prefix are normalized too.
        std::env::set_var("RUSTSHARE_PUBLIC_URL", "https://app.example.com/app///");
        let config = AppConfig::from_env().expect("valid public URL must pass");
        assert_eq!(config.public_url, "https://app.example.com/app");
    }

    #[test]
    fn public_url_treats_loopback_variants_as_local() {
        // The whole 127.0.0.0/8 range is loopback, not just the literal
        // 127.0.0.1, so cleartext http stays acceptable for a local host.
        assert!(public_url_errors("http://127.0.0.2:8080", false, true).is_empty());
        assert!(public_url_errors("http://127.255.255.254:8080", false, true).is_empty());
        // `localhost` with a trailing dot is the same name.
        assert!(public_url_errors("http://localhost.:8080", false, true).is_empty());
        // ...including when it would otherwise be the rejected dev default.
        assert!(!public_url_errors("http://localhost.:5173", false, true).is_empty());
        // A non-loopback host still requires https.
        assert!(public_url_errors("http://192.168.1.10:8080", false, true)
            .iter()
            .any(|e| e.contains("must use https")));
    }
}
