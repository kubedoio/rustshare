//! HTTP handlers for authentication operations.

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

use crate::{
    middleware,
    oidc,
    web_session::{
        build_expired_session_cookie, build_session_cookie, create_user_session, extract_cookie_value,
    },
    AppState,
};

/// Login request
#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Login response
#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserResponse,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub is_admin: bool,
}

/// Login handler
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Response, (StatusCode, String)> {
    if !oidc::password_login_enabled() {
        return Err((
            StatusCode::FORBIDDEN,
            "Password login is disabled for this deployment".to_string(),
        ));
    }

    // Find user
    let user = state
        .metadata_store
        .find_user_by_email(&req.email)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()))?;

    // Verify password
    let is_valid = PasswordHasher::verify(&req.password, &user.password_hash)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !is_valid {
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()));
    }

    // Reject disabled accounts
    if user.disabled_at.is_some() {
        return Ok((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "account_disabled" })),
        )
            .into_response());
    }

    // Keep JWT generation temporarily for compatibility while the web app migrates to cookies.
    let token = state
        .jwt_manager
        .generate(user.id, user.email.clone())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let ip_address = middleware::extract_client_ip(&headers, None).map(|value| value.to_string());
    let session_token =
        create_user_session(&state, user.id, user.tenant_id, user_agent.clone(), ip_address.clone())
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

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
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
    );

    Ok((
        response_headers,
        Json(LoginResponse {
            token,
            user: UserResponse {
                id: user.id.to_string(),
                email: user.email,
                display_name: user.display_name,
                is_admin: user.is_admin,
            },
        }),
    )
        .into_response())
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    if let Some(session_token) =
        extract_cookie_value(&headers, rustshare_auth::WEB_SESSION_COOKIE_NAME)
    {
        let token_hash = rustshare_auth::hash_web_session_token(&session_token);
        let session = state
            .metadata_store
            .find_user_session_by_token_hash(&token_hash)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let user_agent = headers
            .get(header::USER_AGENT)
            .and_then(|value| value.to_str().ok());
        let ip_address =
            middleware::extract_client_ip(&headers, None).map(|value| value.to_string());

        state
            .metadata_store
            .delete_user_session_by_token_hash(&token_hash)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
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
    let username = std::env::var(username_env).ok();
    let email = std::env::var(email_env).ok();
    let password = std::env::var(password_env).ok();

    if username.is_none() && email.is_none() && password.is_none() {
        return Ok(());
    }

    let username =
        username.ok_or_else(|| anyhow::anyhow!("Missing required env {}", username_env))?;
    let email = email.ok_or_else(|| anyhow::anyhow!("Missing required env {}", email_env))?;
    let password =
        password.ok_or_else(|| anyhow::anyhow!("Missing required env {}", password_env))?;

    if metadata_store.find_user_by_email(&email).await?.is_some() {
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

    tracing::info!("Seed user created: {} ({})", username, email);

    Ok(())
}
