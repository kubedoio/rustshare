use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use openidconnect::{core::CoreProviderMetadata, Scope};
use rustshare_crypto::{decrypt_secret, encrypt_secret, SecretEncryptionKey};
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::{uuid, Uuid};

use crate::AppState;

pub const OIDC_CONFIG_ID: Uuid = uuid!("00000000-0000-0000-0000-000000000001");
const OIDC_RUNTIME_CACHE_TTL: Duration = Duration::from_secs(60);
const OIDC_PROVIDER_CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Default)]
pub struct OidcRuntimeCache {
    inner: Arc<RwLock<OidcRuntimeCacheInner>>,
}

#[derive(Clone, Debug, Default)]
struct OidcRuntimeCacheInner {
    settings: Option<CachedOidcSettings>,
    provider_metadata: Option<CachedProviderMetadata>,
}

#[derive(Clone, Debug)]
struct CachedOidcSettings {
    settings: OidcRuntimeSettings,
    fetched_at: Instant,
}

#[derive(Clone, Debug)]
struct CachedProviderMetadata {
    issuer_url: String,
    metadata: CoreProviderMetadata,
    fetched_at: Instant,
}

#[derive(Clone, Debug)]
pub struct OidcRuntimeSettings {
    pub enabled: bool,
    pub provider_name: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub issuer_url: Option<String>,
    pub redirect_url: Option<String>,
    pub login_label: Option<String>,
    pub scopes: Vec<String>,
    pub auto_provision_users: bool,
    pub device_pair_code_ttl_seconds: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct OidcWebRuntimeConfig {
    pub client_id: String,
    pub client_secret: String,
    pub issuer_url: String,
    pub redirect_url: String,
    pub login_label: Option<String>,
    pub scopes: Vec<String>,
    pub auto_provision_users: bool,
}

#[derive(Clone, Debug)]
pub struct MobileOidcRuntimeConfig {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub allowed_redirect_uris: Vec<String>,
    pub scopes: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct OidcConfigRow {
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
}

#[derive(Clone, Debug)]
struct OidcEnvBootstrapConfig {
    client_id: String,
    client_secret: String,
    issuer_url: String,
    redirect_url: String,
    login_label: Option<String>,
    scopes: Vec<String>,
}

impl OidcRuntimeCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_settings(&self) -> Option<OidcRuntimeSettings> {
        let guard = self.inner.read().await;
        let cached = guard.settings.as_ref()?;

        if cached.fetched_at.elapsed() <= OIDC_RUNTIME_CACHE_TTL {
            return Some(cached.settings.clone());
        }

        None
    }

    pub async fn put_settings(&self, settings: OidcRuntimeSettings) {
        let mut guard = self.inner.write().await;
        guard.settings = Some(CachedOidcSettings {
            settings,
            fetched_at: Instant::now(),
        });
    }

    pub async fn get_provider_metadata(&self, issuer_url: &str) -> Option<CoreProviderMetadata> {
        let guard = self.inner.read().await;
        let cached = guard.provider_metadata.as_ref()?;

        if cached.issuer_url == issuer_url && cached.fetched_at.elapsed() <= OIDC_PROVIDER_CACHE_TTL
        {
            return Some(cached.metadata.clone());
        }

        None
    }

    pub async fn put_provider_metadata(&self, issuer_url: String, metadata: CoreProviderMetadata) {
        let mut guard = self.inner.write().await;
        guard.provider_metadata = Some(CachedProviderMetadata {
            issuer_url,
            metadata,
            fetched_at: Instant::now(),
        });
    }

    pub async fn invalidate(&self) {
        let mut guard = self.inner.write().await;
        guard.settings = None;
        guard.provider_metadata = None;
    }
}

impl OidcRuntimeSettings {
    pub fn login_label(&self) -> String {
        self.login_label
            .clone()
            .unwrap_or_else(|| "Continue with SSO".to_string())
    }

    pub fn web_login_config(&self) -> Option<OidcWebRuntimeConfig> {
        if !self.enabled {
            return None;
        }

        Some(OidcWebRuntimeConfig {
            client_id: self.client_id.clone()?,
            client_secret: self.client_secret.clone()?,
            issuer_url: self.issuer_url.clone()?,
            redirect_url: self.redirect_url.clone()?,
            login_label: self.login_label.clone(),
            scopes: self.scopes.clone(),
            auto_provision_users: self.auto_provision_users,
        })
    }

    pub fn mobile_config(&self) -> Option<MobileOidcRuntimeConfig> {
        let issuer_url = self
            .issuer_url
            .clone()
            .or_else(|| non_empty_env("OIDC_ISSUER_URL"))?;
        let client_id = non_empty_env("OIDC_MOBILE_CLIENT_ID")?;
        let client_secret = non_empty_env("OIDC_MOBILE_CLIENT_SECRET");
        let allowed_redirect_uris = mobile_redirect_uris_from_env();

        if allowed_redirect_uris.is_empty() {
            return None;
        }

        let scopes = if self.scopes.is_empty() {
            default_scopes()
        } else {
            self.scopes.clone()
        };

        Some(MobileOidcRuntimeConfig {
            issuer_url,
            client_id,
            client_secret,
            allowed_redirect_uris,
            scopes,
        })
    }

    pub fn device_pair_code_ttl_seconds(&self) -> i32 {
        self.device_pair_code_ttl_seconds.unwrap_or(300)
    }
}

impl OidcWebRuntimeConfig {
    pub fn login_label(&self) -> String {
        self.login_label
            .clone()
            .unwrap_or_else(|| "Continue with SSO".to_string())
    }

    pub fn scopes(&self) -> Vec<Scope> {
        self.scopes
            .iter()
            .filter(|scope| !scope.trim().is_empty())
            .map(|scope| Scope::new(scope.clone()))
            .collect()
    }
}

impl MobileOidcRuntimeConfig {
    pub fn allows_redirect_uri(&self, redirect_uri: &str) -> bool {
        self.allowed_redirect_uris
            .iter()
            .any(|allowed| allowed == redirect_uri)
    }

    pub fn scopes(&self) -> Vec<Scope> {
        self.scopes
            .iter()
            .filter(|scope| !scope.trim().is_empty())
            .map(|scope| Scope::new(scope.clone()))
            .collect()
    }
}

pub async fn load_oidc_runtime_settings(state: &AppState) -> Result<OidcRuntimeSettings, String> {
    if let Some(settings) = state.oidc_runtime_cache.get_settings().await {
        return Ok(settings);
    }

    let row = sqlx::query_as::<_, OidcConfigRow>(
        "SELECT enabled, provider_name, client_id, client_secret_enc, issuer_url,
                redirect_url, login_label, scopes, auto_provision_users,
                device_pair_code_ttl_seconds
         FROM oidc_config
         WHERE id = $1",
    )
    .bind(OIDC_CONFIG_ID)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|error| format!("Failed to load OIDC runtime config: {error}"))?
    .ok_or_else(|| "OIDC runtime config row is missing".to_string())?;

    let client_secret = match row.client_secret_enc {
        Some(secret) => Some(
            decrypt_secret(&secret, &state.secret_key)
                .map_err(|error| format!("Failed to decrypt OIDC client secret: {error}"))?,
        ),
        None => None,
    };

    let settings = OidcRuntimeSettings {
        enabled: row.enabled,
        provider_name: row.provider_name,
        client_id: trim_to_option(row.client_id),
        client_secret: trim_to_option(client_secret),
        issuer_url: trim_to_option(row.issuer_url),
        redirect_url: trim_to_option(row.redirect_url),
        login_label: trim_to_option(row.login_label),
        scopes: row
            .scopes
            .unwrap_or_else(default_scopes)
            .into_iter()
            .filter(|scope| !scope.trim().is_empty())
            .collect(),
        auto_provision_users: row.auto_provision_users,
        device_pair_code_ttl_seconds: row.device_pair_code_ttl_seconds,
    };

    state
        .oidc_runtime_cache
        .put_settings(settings.clone())
        .await;

    Ok(settings)
}

pub async fn load_provider_metadata(
    state: &AppState,
    issuer_url: &str,
) -> Result<CoreProviderMetadata, String> {
    if let Some(metadata) = state
        .oidc_runtime_cache
        .get_provider_metadata(issuer_url)
        .await
    {
        return Ok(metadata);
    }

    let http_client = oidc_http_client()?;
    let metadata = CoreProviderMetadata::discover_async(
        openidconnect::IssuerUrl::new(issuer_url.to_string())
            .map_err(|error| format!("Invalid OIDC issuer URL: {error}"))?,
        &http_client,
    )
    .await
    .map_err(|error| format!("OIDC discovery failed: {error}"))?;

    state
        .oidc_runtime_cache
        .put_provider_metadata(issuer_url.to_string(), metadata.clone())
        .await;

    Ok(metadata)
}

pub async fn invalidate_oidc_runtime_cache(state: &AppState) {
    state.oidc_runtime_cache.invalidate().await;
}

pub async fn seed_oidc_config_from_env(
    pool: &PgPool,
    secret_key: &SecretEncryptionKey,
) -> anyhow::Result<bool> {
    let Some(seed) = OidcEnvBootstrapConfig::from_env() else {
        return Ok(false);
    };

    let row = sqlx::query_as::<_, OidcConfigRow>(
        "SELECT enabled, provider_name, client_id, client_secret_enc, issuer_url,
                redirect_url, login_label, scopes, auto_provision_users,
                device_pair_code_ttl_seconds
         FROM oidc_config
         WHERE id = $1",
    )
    .bind(OIDC_CONFIG_ID)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(false);
    };

    if !oidc_row_needs_bootstrap(&row) {
        return Ok(false);
    }

    let encrypted_secret = encrypt_secret(&seed.client_secret, secret_key)?;

    sqlx::query(
        "UPDATE oidc_config
         SET enabled = true,
             client_id = $2,
             client_secret_enc = $3,
             issuer_url = $4,
             redirect_url = $5,
             login_label = $6,
             scopes = $7,
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(OIDC_CONFIG_ID)
    .bind(seed.client_id)
    .bind(encrypted_secret)
    .bind(seed.issuer_url)
    .bind(seed.redirect_url)
    .bind(seed.login_label)
    .bind(seed.scopes)
    .execute(pool)
    .await?;

    Ok(true)
}

pub fn oidc_http_client() -> Result<openidconnect::reqwest::Client, String> {
    openidconnect::reqwest::ClientBuilder::new()
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("Failed to build OIDC HTTP client: {error}"))
}

fn oidc_row_needs_bootstrap(row: &OidcConfigRow) -> bool {
    row.client_id.is_none()
        && row.client_secret_enc.is_none()
        && row.issuer_url.is_none()
        && row.redirect_url.is_none()
        && row.provider_name.is_none()
        && row.login_label.is_none()
        && row.scopes.is_none()
}

impl OidcEnvBootstrapConfig {
    fn from_env() -> Option<Self> {
        Some(Self {
            client_id: non_empty_env("OIDC_CLIENT_ID")?,
            client_secret: non_empty_env("OIDC_CLIENT_SECRET")?,
            issuer_url: non_empty_env("OIDC_ISSUER_URL")?,
            redirect_url: non_empty_env("OIDC_REDIRECT_URL")?,
            login_label: non_empty_env("OIDC_LOGIN_LABEL"),
            scopes: env_scopes(),
        })
    }
}

fn env_scopes() -> Vec<String> {
    let raw_scopes =
        std::env::var("OIDC_SCOPES").unwrap_or_else(|_| "openid profile email".to_string());

    raw_scopes
        .split_whitespace()
        .filter(|scope| !scope.is_empty())
        .map(|scope| scope.to_string())
        .collect()
}

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "profile".to_string(),
        "email".to_string(),
    ]
}

pub(crate) fn mobile_redirect_uris_from_env() -> Vec<String> {
    let raw = std::env::var("OIDC_MOBILE_REDIRECT_URIS")
        .or_else(|_| std::env::var("OIDC_MOBILE_REDIRECT_URI"))
        .unwrap_or_default();

    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn trim_to_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{default_scopes, trim_to_option, OidcRuntimeSettings};

    #[test]
    fn runtime_settings_require_complete_web_config() {
        let mut settings = OidcRuntimeSettings {
            enabled: true,
            provider_name: None,
            client_id: Some("client-id".to_string()),
            client_secret: Some("secret".to_string()),
            issuer_url: Some("https://issuer.example.com".to_string()),
            redirect_url: Some("https://app.example.com/callback".to_string()),
            login_label: Some("Organization SSO".to_string()),
            scopes: default_scopes(),
            auto_provision_users: false,
            device_pair_code_ttl_seconds: Some(600),
        };

        assert!(settings.web_login_config().is_some());

        settings.redirect_url = None;
        assert!(settings.web_login_config().is_none());
    }

    #[test]
    fn trim_to_option_drops_blank_values() {
        assert_eq!(
            trim_to_option(Some("  value  ".to_string())),
            Some("value".to_string())
        );
        assert_eq!(trim_to_option(Some("   ".to_string())), None);
        assert_eq!(trim_to_option(None), None);
    }
}
