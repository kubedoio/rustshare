use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use openidconnect::{
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use rustshare_auth::generate_web_session_token;
use rustshare_core::domain::{OidcLoginState, User};
use serde::{Deserialize, Serialize};

use crate::{
    middleware,
    web_session::{build_session_cookie, create_user_session},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct AuthConfigResponse {
    pub password_login_enabled: bool,
    pub oidc_enabled: bool,
    pub oidc_login_label: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OidcLoginQuery {
    pub redirect_to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OidcCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Clone)]
struct OidcConfig {
    issuer_url: String,
    client_id: String,
    client_secret: String,
    redirect_url: String,
    login_label: Option<String>,
}

impl OidcConfig {
    fn from_env() -> Option<Self> {
        let issuer_url = std::env::var("OIDC_ISSUER_URL").ok()?;
        let client_id = std::env::var("OIDC_CLIENT_ID").ok()?;
        let client_secret = std::env::var("OIDC_CLIENT_SECRET").ok()?;
        let redirect_url = std::env::var("OIDC_REDIRECT_URL").ok()?;
        let login_label = std::env::var("OIDC_LOGIN_LABEL").ok();

        Some(Self {
            issuer_url,
            client_id,
            client_secret,
            redirect_url,
            login_label,
        })
    }

    fn label(&self) -> String {
        self.login_label
            .clone()
            .unwrap_or_else(|| "Single Sign-On".to_string())
    }

    fn scopes(&self) -> Vec<Scope> {
        let raw_scopes =
            std::env::var("OIDC_SCOPES").unwrap_or_else(|_| "openid profile email".to_string());

        raw_scopes
            .split_whitespace()
            .filter(|scope| !scope.is_empty())
            .map(|scope| Scope::new(scope.to_string()))
            .collect()
    }

    async fn discover_provider(
        &self,
    ) -> Result<(CoreProviderMetadata, openidconnect::reqwest::Client), String> {
        let http_client = openidconnect::reqwest::ClientBuilder::new()
            .redirect(openidconnect::reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| format!("Failed to build OIDC HTTP client: {error}"))?;

        let provider_metadata = CoreProviderMetadata::discover_async(
            IssuerUrl::new(self.issuer_url.clone())
                .map_err(|error| format!("Invalid OIDC issuer URL: {error}"))?,
            &http_client,
        )
        .await
        .map_err(|error| format!("OIDC discovery failed: {error}"))?;

        Ok((provider_metadata, http_client))
    }
}

pub async fn auth_config() -> impl IntoResponse {
    let oidc = OidcConfig::from_env();

    Json(AuthConfigResponse {
        password_login_enabled: password_login_enabled(),
        oidc_enabled: oidc.is_some(),
        oidc_login_label: oidc.map(|config| config.label()),
    })
}

pub async fn oidc_login(
    State(state): State<AppState>,
    Query(query): Query<OidcLoginQuery>,
) -> Result<Redirect, (StatusCode, String)> {
    let config = OidcConfig::from_env()
        .ok_or_else(|| (StatusCode::NOT_FOUND, "OIDC is not configured".to_string()))?;
    let redirect_to = sanitize_redirect_target(query.redirect_to.as_deref());
    let (provider_metadata, _http_client) = config
        .discover_provider()
        .await
        .map_err(internal_oidc_error)?;
    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(config.client_id.clone()),
        Some(ClientSecret::new(config.client_secret.clone())),
    )
    .set_redirect_uri(
        RedirectUrl::new(config.redirect_url.clone()).map_err(|error| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Invalid OIDC redirect URL: {error}"),
            )
        })?,
    );

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let mut auth_request = client.authorize_url(
        CoreAuthenticationFlow::AuthorizationCode,
        CsrfToken::new_random,
        Nonce::new_random,
    );

    for scope in config.scopes() {
        auth_request = auth_request.add_scope(scope);
    }

    let (auth_url, csrf_state, nonce) = auth_request.set_pkce_challenge(pkce_challenge).url();

    let login_state = OidcLoginState::new(
        csrf_state.secret().to_string(),
        pkce_verifier.secret().to_string(),
        nonce.secret().to_string(),
        redirect_to,
    );

    state
        .metadata_store
        .create_oidc_login_state(&login_state)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to persist OIDC login state: {error}"),
            )
        })?;

    let auth_url = auth_url.to_string();
    Ok(Redirect::temporary(&auth_url))
}

pub async fn oidc_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OidcCallbackQuery>,
) -> Result<Response, (StatusCode, String)> {
    if let Some(error) = query.error {
        let description = query
            .error_description
            .unwrap_or_else(|| "Provider rejected the login request".to_string());
        return Err((
            StatusCode::BAD_REQUEST,
            format!("OIDC login failed: {error}: {description}"),
        ));
    }

    let code = query.code.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Missing OIDC authorization code".to_string(),
        )
    })?;
    let state_token = query
        .state
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Missing OIDC state".to_string()))?;

    let Some(login_state) = state
        .metadata_store
        .find_oidc_login_state(&state_token)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load OIDC login state: {error}"),
            )
        })?
    else {
        return Err((StatusCode::BAD_REQUEST, "Unknown OIDC state".to_string()));
    };

    state
        .metadata_store
        .delete_oidc_login_state(&state_token)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to consume OIDC login state: {error}"),
            )
        })?;

    if login_state.is_expired() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Expired OIDC login state".to_string(),
        ));
    }

    let config = OidcConfig::from_env()
        .ok_or_else(|| (StatusCode::NOT_FOUND, "OIDC is not configured".to_string()))?;
    let (provider_metadata, http_client) = config
        .discover_provider()
        .await
        .map_err(internal_oidc_error)?;
    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(config.client_id.clone()),
        Some(ClientSecret::new(config.client_secret.clone())),
    )
    .set_redirect_uri(
        RedirectUrl::new(config.redirect_url.clone()).map_err(|error| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Invalid OIDC redirect URL: {error}"),
            )
        })?,
    );
    let token_response = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid OIDC code exchange: {error}"),
            )
        })?
        .set_pkce_verifier(PkceCodeVerifier::new(login_state.pkce_verifier.clone()))
        .request_async(&http_client)
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_GATEWAY,
                format!("OIDC token exchange failed: {error}"),
            )
        })?;

    let id_token = token_response.id_token().ok_or_else(|| {
        (
            StatusCode::BAD_GATEWAY,
            "OIDC provider returned no ID token".to_string(),
        )
    })?;
    let id_token_verifier = client.id_token_verifier();
    let nonce = Nonce::new(login_state.nonce.clone());
    let claims = id_token
        .claims(&id_token_verifier, &nonce)
        .map_err(|error| {
            (
                StatusCode::UNAUTHORIZED,
                format!("Invalid OIDC ID token: {error}"),
            )
        })?;

    if let Some(email_verified) = claims.email_verified() {
        if !email_verified {
            return Err((
                StatusCode::UNAUTHORIZED,
                "OIDC provider returned an unverified e-mail address".to_string(),
            ));
        }
    }

    let email = claims
        .email()
        .map(|value| value.as_str().to_string())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "OIDC provider did not return an e-mail address".to_string(),
            )
        })?;
    let user = find_or_create_oidc_user(&state, &email).await?;

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let ip_address = middleware::extract_client_ip(&headers, None).map(|value| value.to_string());
    let session_token = create_user_session(&state, user.id, user_agent, ip_address)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_session_cookie(&session_token)).map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to serialize session cookie: {error}"),
            )
        })?,
    );

    Ok((response_headers, Redirect::to(&login_state.redirect_to)).into_response())
}

pub fn password_login_enabled() -> bool {
    std::env::var("PASSWORD_LOGIN_ENABLED")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(true)
}

fn internal_oidc_error(message: String) -> (StatusCode, String) {
    (StatusCode::BAD_GATEWAY, message)
}

fn sanitize_redirect_target(value: Option<&str>) -> String {
    match value {
        Some(path) if path.starts_with('/') && !path.starts_with("//") => path.to_string(),
        _ => "/files".to_string(),
    }
}

async fn find_or_create_oidc_user(
    state: &AppState,
    email: &str,
) -> Result<User, (StatusCode, String)> {
    if let Some(user) = state
        .metadata_store
        .find_user_by_email(email)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load user by e-mail: {error}"),
            )
        })?
    {
        return Ok(user);
    }

    let username = allocate_username(state, email).await?;
    let password_hash = rustshare_auth::PasswordHasher::hash(&generate_web_session_token())
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to provision OIDC user password placeholder: {error}"),
            )
        })?;
    let user = User::new(
        username,
        display_name_from_email(email),
        password_hash,
        email.to_string(),
        false,
        default_storage_quota_bytes(),
    );

    state
        .metadata_store
        .create_user(&user)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create OIDC user: {error}"),
            )
        })?;

    Ok(user)
}

async fn allocate_username(state: &AppState, email: &str) -> Result<String, (StatusCode, String)> {
    let base = normalize_username(email);

    for suffix in 0..1000 {
        let candidate = if suffix == 0 {
            base.clone()
        } else {
            format!("{base}-{suffix}")
        };

        let existing = state
            .metadata_store
            .find_user_by_username(&candidate)
            .await
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to check username availability: {error}"),
                )
            })?;

        if existing.is_none() {
            return Ok(candidate);
        }
    }

    Err((
        StatusCode::CONFLICT,
        "Failed to allocate a unique username for the OIDC user".to_string(),
    ))
}

fn normalize_username(email: &str) -> String {
    let local_part = email.split('@').next().unwrap_or("user");
    let mut normalized = String::with_capacity(local_part.len());
    let mut previous_was_dash = false;

    for ch in local_part.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            previous_was_dash = false;
        } else if !previous_was_dash {
            normalized.push('-');
            previous_was_dash = true;
        }
    }

    let trimmed = normalized.trim_matches('-');
    if trimmed.is_empty() {
        "user".to_string()
    } else {
        trimmed.to_string()
    }
}

fn display_name_from_email(email: &str) -> String {
    email
        .split('@')
        .next()
        .unwrap_or("User")
        .replace(['.', '_', '-'], " ")
}

fn default_storage_quota_bytes() -> i64 {
    std::env::var("RUSTSHARE_DEFAULT_STORAGE_QUOTA_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_737_418_240)
}
