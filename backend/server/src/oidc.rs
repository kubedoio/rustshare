use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use openidconnect::{
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, TokenResponse,
};
use rustshare_auth::generate_web_session_token;
use rustshare_core::domain::{OidcLoginState, User};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    middleware,
    oidc_runtime::{
        load_oidc_runtime_settings, load_provider_metadata, oidc_http_client,
        MobileOidcRuntimeConfig,
    },
    web_session::{build_session_cookie, create_user_session},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct AuthConfigResponse {
    pub password_login_enabled: bool,
    pub oidc_enabled: bool,
    pub oidc_login_label: Option<String>,
    pub oidc_mobile_enabled: bool,
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

#[derive(Debug, Deserialize)]
pub struct MobileOidcAuthorizeRequest {
    pub redirect_uri: String,
    pub code_challenge: String,
    pub state: String,
    pub nonce: String,
}

#[derive(Debug, Serialize)]
pub struct MobileOidcAuthorizeResponse {
    pub authorization_url: String,
}

#[derive(Debug, Deserialize)]
pub struct MobileOidcExchangeRequest {
    pub code: String,
    pub code_verifier: String,
    pub redirect_uri: String,
    pub nonce: String,
}

#[derive(Debug, Serialize)]
pub struct MobileOidcExchangeResponse {
    pub token: String,
    pub expires_in: i64,
    pub user: MobileUserResponse,
}

#[derive(Debug, Serialize)]
pub struct MobileUserResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub is_admin: bool,
}

pub async fn auth_config(
    State(state): State<AppState>,
) -> Result<Json<AuthConfigResponse>, (StatusCode, String)> {
    let oidc_settings = load_oidc_runtime_settings(&state)
        .await
        .map_err(internal_oidc_error)?;
    let web_oidc = oidc_settings.web_login_config();
    let mobile_oidc = oidc_settings.mobile_config();

    Ok(Json(AuthConfigResponse {
        password_login_enabled: password_login_enabled(),
        oidc_enabled: web_oidc.is_some(),
        oidc_login_label: web_oidc.map(|config| config.login_label()),
        oidc_mobile_enabled: mobile_oidc.is_some(),
    }))
}

pub async fn oidc_login(
    State(state): State<AppState>,
    Query(query): Query<OidcLoginQuery>,
) -> Result<Redirect, (StatusCode, String)> {
    let settings = load_oidc_runtime_settings(&state)
        .await
        .map_err(internal_oidc_error)?;
    let config = settings
        .web_login_config()
        .ok_or_else(|| (StatusCode::NOT_FOUND, "OIDC is not configured".to_string()))?;
    let redirect_to = sanitize_redirect_target(query.redirect_to.as_deref());
    let provider_metadata = load_provider_metadata(&state, &config.issuer_url)
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

pub async fn mobile_oidc_authorize(
    State(state): State<AppState>,
    Json(req): Json<MobileOidcAuthorizeRequest>,
) -> Result<Json<MobileOidcAuthorizeResponse>, (StatusCode, String)> {
    let settings = load_oidc_runtime_settings(&state)
        .await
        .map_err(internal_oidc_error)?;
    let config = settings.mobile_config().ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "Mobile OIDC is not configured".to_string(),
        )
    })?;
    validate_mobile_oidc_request(
        &config,
        &req.redirect_uri,
        &req.code_challenge,
        &req.state,
        &req.nonce,
    )?;

    let provider_metadata = load_provider_metadata(&state, &config.issuer_url)
        .await
        .map_err(internal_oidc_error)?;
    let authorization_url = build_mobile_authorization_url(&config, &provider_metadata, &req)?;

    Ok(Json(MobileOidcAuthorizeResponse { authorization_url }))
}

pub async fn mobile_oidc_exchange(
    State(state): State<AppState>,
    Json(req): Json<MobileOidcExchangeRequest>,
) -> Result<Json<MobileOidcExchangeResponse>, (StatusCode, String)> {
    let settings = load_oidc_runtime_settings(&state)
        .await
        .map_err(internal_oidc_error)?;
    let config = settings.mobile_config().ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "Mobile OIDC is not configured".to_string(),
        )
    })?;
    validate_mobile_oidc_exchange_request(&config, &req)?;

    let provider_metadata = load_provider_metadata(&state, &config.issuer_url)
        .await
        .map_err(internal_oidc_error)?;
    let http_client = oidc_http_client().map_err(internal_oidc_error)?;
    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(config.client_id.clone()),
        config.client_secret.clone().map(ClientSecret::new),
    )
    .set_redirect_uri(RedirectUrl::new(req.redirect_uri.clone()).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid mobile OIDC redirect URI: {error}"),
        )
    })?);

    let token_response = client
        .exchange_code(AuthorizationCode::new(req.code.clone()))
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid OIDC code exchange: {error}"),
            )
        })?
        .set_pkce_verifier(PkceCodeVerifier::new(req.code_verifier.clone()))
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
    let nonce = Nonce::new(req.nonce.clone());
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

    let user = find_or_create_oidc_user(&state, &email, settings.auto_provision_users).await?;
    let token = state
        .jwt_manager
        .generate(user.id, user.email.clone(), user.tenant_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;

    Ok(Json(MobileOidcExchangeResponse {
        token,
        expires_in: 24 * 60 * 60,
        user: MobileUserResponse {
            id: user.id.to_string(),
            email: user.email,
            display_name: user.display_name,
            is_admin: user.is_admin,
        },
    }))
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

    let settings = load_oidc_runtime_settings(&state)
        .await
        .map_err(internal_oidc_error)?;
    let config = settings
        .web_login_config()
        .ok_or_else(|| (StatusCode::NOT_FOUND, "OIDC is not configured".to_string()))?;
    let provider_metadata = load_provider_metadata(&state, &config.issuer_url)
        .await
        .map_err(internal_oidc_error)?;
    let http_client = oidc_http_client().map_err(internal_oidc_error)?;
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
    let user = find_or_create_oidc_user(&state, &email, config.auto_provision_users).await?;

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let ip_address = middleware::extract_client_ip(&headers, None).map(|value| value.to_string());
    let session_token = create_user_session(
        &state,
        user.id,
        user.tenant_id,
        user_agent.clone(),
        ip_address.clone(),
    )
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;

    if let Err(error) = state
        .metadata_store
        .create_user_security_event(rustshare_storage::UserSecurityEventRecord {
            user_id: user.id,
            event_type: "browser_oidc_login",
            description: "Signed in with single sign-on",
            ip_address: ip_address.as_deref(),
            user_agent: user_agent.as_deref(),
            session_id: None,
        })
        .await
    {
        tracing::warn!("Failed to record OIDC login security event: {:?}", error);
    }

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

fn validate_mobile_oidc_request(
    config: &MobileOidcRuntimeConfig,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    nonce: &str,
) -> Result<(), (StatusCode, String)> {
    if !config.allows_redirect_uri(redirect_uri) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Unsupported mobile OIDC redirect URI".to_string(),
        ));
    }
    Url::parse(redirect_uri).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid mobile OIDC redirect URI: {error}"),
        )
    })?;
    if code_challenge.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Missing mobile OIDC code challenge".to_string(),
        ));
    }
    if state.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Missing mobile OIDC state".to_string(),
        ));
    }
    if nonce.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Missing mobile OIDC nonce".to_string(),
        ));
    }

    Ok(())
}

fn validate_mobile_oidc_exchange_request(
    config: &MobileOidcRuntimeConfig,
    req: &MobileOidcExchangeRequest,
) -> Result<(), (StatusCode, String)> {
    if !config.allows_redirect_uri(&req.redirect_uri) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Unsupported mobile OIDC redirect URI".to_string(),
        ));
    }
    Url::parse(&req.redirect_uri).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid mobile OIDC redirect URI: {error}"),
        )
    })?;
    if req.code.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Missing mobile OIDC authorization code".to_string(),
        ));
    }
    if req.code_verifier.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Missing mobile OIDC code verifier".to_string(),
        ));
    }
    if req.nonce.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Missing mobile OIDC nonce".to_string(),
        ));
    }

    Ok(())
}

fn build_mobile_authorization_url(
    config: &MobileOidcRuntimeConfig,
    provider_metadata: &CoreProviderMetadata,
    req: &MobileOidcAuthorizeRequest,
) -> Result<String, (StatusCode, String)> {
    let mut authorization_url = Url::parse(provider_metadata.authorization_endpoint().as_str())
        .map_err(|error| {
            internal_oidc_error(format!(
                "Invalid provider authorization endpoint URL: {error}"
            ))
        })?;
    let scope_value = config
        .scopes()
        .into_iter()
        .map(|scope| scope.to_string())
        .collect::<Vec<_>>()
        .join(" ");

    {
        let mut pairs = authorization_url.query_pairs_mut();
        pairs.append_pair("response_type", "code");
        pairs.append_pair("client_id", &config.client_id);
        pairs.append_pair("redirect_uri", &req.redirect_uri);
        pairs.append_pair("scope", &scope_value);
        pairs.append_pair("code_challenge", &req.code_challenge);
        pairs.append_pair("code_challenge_method", "S256");
        pairs.append_pair("state", &req.state);
        pairs.append_pair("nonce", &req.nonce);
    }

    Ok(authorization_url.into())
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
    auto_provision_users: bool,
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
        if user.disabled_at.is_some() {
            return Err((StatusCode::FORBIDDEN, "account_disabled".to_string()));
        }
        return Ok(user);
    }

    if !auto_provision_users {
        return Err((
            StatusCode::FORBIDDEN,
            "OIDC user provisioning is disabled for this deployment".to_string(),
        ));
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
        crate::default_storage_quota_bytes(),
        state.default_tenant_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oidc_runtime::mobile_redirect_uris_from_env;

    #[test]
    fn sanitize_redirect_target_only_accepts_internal_paths() {
        assert_eq!(sanitize_redirect_target(Some("/files")), "/files");
        assert_eq!(sanitize_redirect_target(Some("//evil.example")), "/files");
        assert_eq!(
            sanitize_redirect_target(Some("https://evil.example")),
            "/files"
        );
        assert_eq!(sanitize_redirect_target(None), "/files");
    }

    #[test]
    fn mobile_redirect_uris_parse_comma_separated_env() {
        let key = "OIDC_MOBILE_REDIRECT_URIS";
        let previous = std::env::var(key).ok();
        std::env::set_var(
            key,
            "rustshare://auth/callback, https://app.example/callback ",
        );

        let parsed = mobile_redirect_uris_from_env();

        match previous {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }

        assert_eq!(
            parsed,
            vec![
                "rustshare://auth/callback".to_string(),
                "https://app.example/callback".to_string()
            ]
        );
    }
}
