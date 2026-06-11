//! HTTP handlers for authentication operations.

use anyhow::Context;
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use rustshare_auth::{PasswordHasher, DUMMY_HASH};
use rustshare_core::domain::User;
use rustshare_storage::MetadataStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::AppError;
use crate::{
    middleware, oidc,
    web_session::{
        build_csrf_cookie, build_expired_csrf_cookie, build_expired_session_cookie,
        build_session_cookie, create_user_session, extract_cookie_value,
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
    metadata_store: &MetadataStore,
    req: &LoginRequest,
    ip: Option<&str>,
) -> Result<User, AppError> {
    let user = metadata_store
        .find_user_by_email(&req.email)
        .await
        .map_err(|e| AppError::internal(e.to_string()))?;

    let user = match user {
        Some(u) => u,
        None => {
            // Constant-time path: perform a dummy Argon2 verify so that
            // non-existent emails take the same time as wrong passwords.
            drop(PasswordHasher::verify("dummy", DUMMY_HASH));

            if let Some(ip) = ip {
                if let Err(e) = metadata_store.record_login_failure(ip).await {
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
            if let Err(e) = metadata_store.record_login_failure(ip).await {
                tracing::warn!("Failed to record login failure: {}", e);
            }
        }
        return Err(AppError::Unauthorized);
    }

    if let Some(ip) = ip {
        if let Err(e) = metadata_store.clear_login_attempts(ip).await {
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

    let user = validate_credentials(&state.metadata_store, &req, client_ip.as_deref()).await?;

    let token = state
        .jwt_manager
        .generate(user.id, user.email.clone(), user.tenant_id)
        .map_err(|e| AppError::internal(e.to_string()))?;

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let ip_address = client_ip.clone();
    let (session_token, csrf_token) = create_user_session(
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
    response_headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_csrf_cookie(&csrf_token))
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
    response_headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&build_expired_csrf_cookie())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    const TEST_DATABASE_URL: &str =
        "postgres://rustshare:1f7b27220d83a11de6bca8b63c0ca491a3001c0c73471eda@localhost:5432/rustshare";

    async fn test_db_pool() -> sqlx::PgPool {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());
        sqlx::PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database")
    }

    async fn insert_test_user(pool: &sqlx::PgPool, email: &str, password_hash: &str) -> uuid::Uuid {
        let user_id = uuid::Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO users (
                id, username, email, password_hash, display_name,
                is_admin, storage_quota, tenant_id
            )
            VALUES ($1, $2, $3, $4, $5, FALSE, $6, $7)
            "#,
        )
        .bind(user_id)
        .bind(format!("timing_test_user_{}", user_id))
        .bind(email)
        .bind(password_hash)
        .bind("Timing Test User")
        .bind(10_737_418_240_i64)
        .bind(uuid::Uuid::nil())
        .execute(pool)
        .await
        .expect("Failed to insert test user");
        user_id
    }

    async fn cleanup_test_user(pool: &sqlx::PgPool, user_id: uuid::Uuid) {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .ok();
    }

    #[tokio::test]
    async fn login_timing_attack_resistance() {
        let pool = test_db_pool().await;
        let metadata_store = MetadataStore::new(pool.clone());

        // Create a test user with a real Argon2 hash.
        let correct_password = "correct_password_123";
        let hash = PasswordHasher::hash(correct_password).unwrap();
        let existing_email = format!("timing_{}@example.com", uuid::Uuid::new_v4());
        let user_id = insert_test_user(&pool, &existing_email, &hash).await;

        let non_existent_email = format!("timing_{}@example.com", uuid::Uuid::new_v4());

        // Warm up Argon2 to reduce cold-start variance.
        let _ = PasswordHasher::verify("warmup", &hash);

        // Measure non-existent email branch.
        let mut non_existent_times = Vec::with_capacity(10);
        for _ in 0..10 {
            let req = LoginRequest {
                email: non_existent_email.clone(),
                password: "wrong_password".to_string(),
            };
            let start = Instant::now();
            let _ = validate_credentials(&metadata_store, &req, None).await;
            non_existent_times.push(start.elapsed().as_millis() as f64);
        }

        // Measure existing email + wrong password branch.
        let mut wrong_password_times = Vec::with_capacity(10);
        for _ in 0..10 {
            let req = LoginRequest {
                email: existing_email.clone(),
                password: "wrong_password".to_string(),
            };
            let start = Instant::now();
            let _ = validate_credentials(&metadata_store, &req, None).await;
            wrong_password_times.push(start.elapsed().as_millis() as f64);
        }

        cleanup_test_user(&pool, user_id).await;

        let avg_non_existent: f64 =
            non_existent_times.iter().sum::<f64>() / non_existent_times.len() as f64;
        let avg_wrong_password: f64 =
            wrong_password_times.iter().sum::<f64>() / wrong_password_times.len() as f64;

        println!(
            "Avg non-existent: {:.2}ms, Avg wrong password: {:.2}ms",
            avg_non_existent, avg_wrong_password
        );

        // Assert averages are within 50% of each other.
        let ratio = if avg_non_existent > avg_wrong_password {
            avg_non_existent / avg_wrong_password
        } else {
            avg_wrong_password / avg_non_existent
        };
        assert!(
            ratio <= 1.5,
            "Timing difference too large: ratio={:.2} (non-existent={:.2}ms, wrong={:.2}ms)",
            ratio,
            avg_non_existent,
            avg_wrong_password
        );
    }
}
