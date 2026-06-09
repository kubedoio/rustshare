//! HTTP handlers for authentication operations.

use anyhow::Context;
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use rustshare_auth::PasswordHasher;
use rustshare_core::domain::User;
use rustshare_storage::MetadataStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::AppError;
use crate::{
    middleware, oidc,
    web_session::{
        build_expired_session_cookie, build_session_cookie, create_user_session,
        extract_cookie_value,
    },
    AppState,
};

/// Login request
#[derive(Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct LoginRequest {
    #[validate(email(message = "Invalid email address"))]
    pub email: String,
    #[validate(length(min = 1, message = "Password must not be empty"))]
    pub password: String,
}

/// Login response
#[derive(Serialize, utoipa::ToSchema)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserResponse,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub is_admin: bool,
    pub avatar_path: Option<String>,
    pub theme: String,
}

async fn check_ip_block(state: &AppState, ip: &str) -> Result<(), AppError> {
    match state.metadata_store.is_ip_blocked(ip).await {
        Ok(true) => Err(AppError::TooManyRequests),
        Ok(false) => Ok(()),
        Err(e) => {
            tracing::warn!("Failed to check IP block status: {}", e);
            Ok(())
        }
    }
}

async fn validate_credentials(
    state: &AppState,
    req: &LoginRequest,
    ip: Option<&str>,
) -> Result<User, AppError> {
    let user = state
        .metadata_store
        .find_user_by_email(&req.email)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    let user = match user {
        Some(u) => u,
        None => {
            if let Some(ip) = ip {
                if let Err(e) = state.metadata_store.record_login_failure(ip).await {
                    tracing::warn!("Failed to record login failure: {}", e);
                }
            }
            return Err(AppError::Unauthorized);
        }
    };

    let is_valid = PasswordHasher::verify(&req.password, &user.password_hash)
        .map_err(|e| AppError::internal(e.to_string()))?;

    if !is_valid {
        if let Some(ip) = ip {
            if let Err(e) = state.metadata_store.record_login_failure(ip).await {
                tracing::warn!("Failed to record login failure: {}", e);
            }
        }
        return Err(AppError::Unauthorized);
    }

    if let Some(ip) = ip {
        if let Err(e) = state.metadata_store.clear_login_attempts(ip).await {
            tracing::warn!("Failed to clear login attempts: {}", e);
        }
    }

    if user.disabled_at.is_some() {
        return Err(AppError::forbidden("account_disabled"));
    }

    Ok(user)
}

fn build_login_response(token: String, user: User) -> LoginResponse {
    LoginResponse {
        token,
        user: UserResponse {
            id: user.id.to_string(),
            email: user.email,
            display_name: user.display_name,
            is_admin: user.is_admin,
            avatar_path: user.avatar_path,
            theme: user.theme.to_string(),
        },
    }
}

/// Login handler
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "Auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 400, description = "Invalid request", body = crate::handlers::ErrorResponse),
        (status = 401, description = "Invalid credentials", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Password login disabled", body = crate::handlers::ErrorResponse),
        (status = 429, description = "Too many requests", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    super::ValidatedJson(req): super::ValidatedJson<LoginRequest>,
) -> Result<Response, AppError> {
    if !oidc::password_login_enabled() {
        return Err(AppError::forbidden(
            "Password login is disabled for this deployment",
        ));
    }

    let client_ip = middleware::extract_client_ip(&headers, None).map(|ip| ip.to_string());
    if let Some(ref ip) = client_ip {
        check_ip_block(&state, ip).await?;
    }

    let user = validate_credentials(&state, &req, client_ip.as_deref()).await?;

    let token = state
        .jwt_manager
        .generate(user.id, user.email.clone(), user.tenant_id)
        .map_err(|e| AppError::internal(e.to_string()))?;

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let ip_address = client_ip.clone();
    let session_token = create_user_session(
        &state,
        user.id,
        user.tenant_id,
        user_agent.clone(),
        ip_address.clone(),
    )
    .await
    .map_err(AppError::internal)?;

    if let Err(error) = log_user_security_event(
        &state,
        rustshare_storage::UserSecurityEventRecord {
            user_id: user.id,
            event_type: "password_login",
            description: "Signed in with email and password",
            ip_address: ip_address.as_deref(),
            user_agent: user_agent.as_deref(),
            session_id: None,
        },
    )
    .await
    {
        tracing::warn!(
            "Failed to record password login security event: {:?}",
            error
        );
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_session_cookie(&session_token))
            .map_err(|e| AppError::internal(e.to_string()))?,
    );

    Ok((response_headers, Json(build_login_response(token, user))).into_response())
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "Auth",
    responses(
        (status = 204, description = "Logout successful"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    if let Some(session_token) =
        extract_cookie_value(&headers, rustshare_auth::WEB_SESSION_COOKIE_NAME)
    {
        let token_hash = rustshare_auth::hash_web_session_token(&session_token);
        let session = state
            .metadata_store
            .find_user_session_by_token_hash(&token_hash)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;

        let user_agent = headers
            .get(header::USER_AGENT)
            .and_then(|value| value.to_str().ok());
        let ip_address =
            middleware::extract_client_ip(&headers, None).map(|value| value.to_string());

        state
            .metadata_store
            .delete_user_session_by_token_hash(&token_hash)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?;

        if let Some(session) = session {
            if let Err(error) = log_user_security_event(
                &state,
                rustshare_storage::UserSecurityEventRecord {
                    user_id: session.user_id,
                    event_type: "logout",
                    description: "Signed out of browser session",
                    ip_address: ip_address.as_deref(),
                    user_agent,
                    session_id: Some(session.id),
                },
            )
            .await
            {
                tracing::warn!("Failed to record logout security event: {:?}", error);
            }
        }
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_expired_session_cookie())
            .map_err(|e| AppError::internal(e.to_string()))?,
    );

    Ok((response_headers, StatusCode::NO_CONTENT).into_response())
}

pub async fn log_user_security_event(
    state: &AppState,
    event: rustshare_storage::UserSecurityEventRecord<'_>,
) -> anyhow::Result<()> {
    state.metadata_store.create_user_security_event(event).await
}

pub async fn ensure_optional_seed_user(
    metadata_store: &Arc<MetadataStore>,
    username_env: &str,
    email_env: &str,
    password_env: &str,
    display_name: String,
    is_admin: bool,
    default_tenant_id: uuid::Uuid,
) -> anyhow::Result<()> {
    let username = std::env::var(username_env);
    let email = std::env::var(email_env);
    let password = std::env::var(password_env);

    if username.is_err() && email.is_err() && password.is_err() {
        return Ok(());
    }

    let username = username.with_context(|| format!("Missing required env {}", username_env))?;
    let email = email.with_context(|| format!("Missing required env {}", email_env))?;
    let password = password.with_context(|| format!("Missing required env {}", password_env))?;

    if metadata_store.find_user_by_email(&email).await?.is_some() {
        return Ok(());
    }

    if metadata_store
        .find_user_by_username(&username)
        .await?
        .is_some()
    {
        tracing::warn!(
            username = %username,
            email = %email,
            "Skipping optional seed user because username already exists with a different email"
        );
        return Ok(());
    }

    let password_hash = PasswordHasher::hash(&password)?;
    let user = User::new(
        username.clone(),
        display_name,
        password_hash,
        email.clone(),
        is_admin,
        crate::default_storage_quota_bytes(),
        default_tenant_id,
    );

    metadata_store.create_user(&user).await?;

    // Seed default module preferences
    let pref_repo = rustshare_infrastructure::repositories::UserModulePreferenceRepository::new(
        metadata_store.pool().clone(),
    );
    pref_repo.seed_defaults(user.id).await.ok();

    tracing::info!("Seed user created: {} ({})", username, email);

    Ok(())
}
