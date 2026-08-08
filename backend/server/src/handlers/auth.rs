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
    #[validate(length(
        min = 1,
        max = 128,
        message = "Password must not be empty and at most 128 characters"
    ))]
    pub password: String,
    /// Optional tenant ID. When provided, login is scoped to that tenant.
    /// Existing clients may omit this field for backward compatibility.
    #[serde(default)]
    pub tenant_id: Option<uuid::Uuid>,
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
    let user = match req.tenant_id {
        Some(tenant_id) => metadata_store
            .find_user_by_email_and_tenant(&req.email, tenant_id)
            .await
            .map_err(|e| AppError::internal(e.to_string()))?,
        None => {
            tracing::warn!(
                email = %req.email,
                "Password login performed without tenant scoping; falling back to unscoped email lookup"
            );
            // After dropping the global email unique constraint, an unscoped
            // lookup could return an arbitrary user when the same email exists
            // in multiple tenants. Count first and reject ambiguous
            // case-insensitive matches before selecting a user.
            let count = metadata_store
                .count_users_by_email(&req.email)
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
            if count > 1 {
                tracing::warn!(
                    email = %req.email,
                    count,
                    "Rejecting unscoped login because email is ambiguous across tenants"
                );
                // Constant-time path: keep timing indistinguishable.
                drop(PasswordHasher::verify("dummy", DUMMY_HASH));
                if let Some(ip) = ip {
                    if let Err(e) = metadata_store.record_login_failure(ip).await {
                        tracing::warn!("Failed to record login failure: {}", e);
                    }
                }
                return Err(AppError::Unauthorized);
            }

            let user = metadata_store
                .find_user_by_email(&req.email)
                .await
                .map_err(|e| AppError::internal(e.to_string()))?;
            user
        }
    };

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

/// Treat empty or whitespace-only environment values as unset. Docker Compose
/// forwards `${VAR}` references as set-but-empty when `.env` leaves the
/// variable blank, and `std::env::var` then returns `Ok("")`.
fn non_empty_env_value(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
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
    // Docker Compose forwards `${VAR}` references as set-but-empty when `.env`
    // leaves them blank, so treat empty or whitespace-only values as unset.
    let username = non_empty_env_value(std::env::var(username_env).ok());
    let email = non_empty_env_value(std::env::var(email_env).ok());
    let password = non_empty_env_value(std::env::var(password_env).ok());

    if username.is_none() && email.is_none() && password.is_none() {
        return Ok(());
    }

    let username =
        username.with_context(|| format!("Missing or empty required env {}", username_env))?;
    let email = email.with_context(|| format!("Missing or empty required env {}", email_env))?;
    let password =
        password.with_context(|| format!("Missing or empty required env {}", password_env))?;

    if metadata_store
        .find_user_by_email_and_tenant(&email, default_tenant_id)
        .await?
        .is_some()
    {
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

    // Seed default Application preferences
    let pref_repo =
        rustshare_infrastructure::repositories::ApplicationUserPreferenceRepository::new(
            metadata_store.pool().clone(),
        );
    pref_repo.seed_defaults(user.id).await.ok();

    tracing::info!("Seed user created: {} ({})", username, email);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::time::{Duration, Instant};

    #[test]
    fn non_empty_env_value_treats_blank_values_as_unset() {
        assert_eq!(non_empty_env_value(None), None);
        assert_eq!(non_empty_env_value(Some(String::new())), None);
        assert_eq!(non_empty_env_value(Some("   ".to_string())), None);
        assert_eq!(non_empty_env_value(Some("\t\n".to_string())), None);
        assert_eq!(
            non_empty_env_value(Some("viewer-secret".to_string())),
            Some("viewer-secret".to_string())
        );
    }

    async fn test_db_pool() -> sqlx::PgPool {
        // Load a local .env file if present, then require DATABASE_URL to be set.
        // Avoids checking hardcoded credentials into source control.
        dotenvy::dotenv().ok();
        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set for tests (or via a .env file)");

        PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(60))
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database")
    }

    async fn insert_test_user(
        pool: &sqlx::PgPool,
        email: &str,
        password_hash: &str,
        tenant_id: uuid::Uuid,
    ) -> uuid::Uuid {
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
        .bind(tenant_id)
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
    async fn login_scopes_to_tenant_when_provided() {
        let pool = test_db_pool().await;
        let metadata_store = MetadataStore::new(pool.clone());

        let email = "shared@example.com".to_string();
        let password = "tenant_scoped_password";
        let hash = PasswordHasher::hash(password).unwrap();

        let tenant_a = uuid::Uuid::new_v4();
        let tenant_b = uuid::Uuid::new_v4();

        let user_a_id = insert_test_user(&pool, &email, &hash, tenant_a).await;
        let user_b_id = insert_test_user(&pool, &email, &hash, tenant_b).await;

        let req = LoginRequest {
            email: email.clone(),
            password: password.to_string(),
            tenant_id: Some(tenant_a),
        };
        let user = validate_credentials(&metadata_store, &req, None)
            .await
            .expect("login should succeed");
        assert_eq!(user.id, user_a_id);
        assert_eq!(user.tenant_id, tenant_a);

        cleanup_test_user(&pool, user_a_id).await;
        cleanup_test_user(&pool, user_b_id).await;
    }

    #[tokio::test]
    async fn login_without_tenant_id_still_works() {
        let pool = test_db_pool().await;
        let metadata_store = MetadataStore::new(pool.clone());

        let email = "legacy@example.com".to_string();
        let password = "legacy_password";
        let hash = PasswordHasher::hash(password).unwrap();
        let tenant_id = uuid::Uuid::new_v4();

        let user_id = insert_test_user(&pool, &email, &hash, tenant_id).await;

        let req = LoginRequest {
            email: email.clone(),
            password: password.to_string(),
            tenant_id: None,
        };
        let user = validate_credentials(&metadata_store, &req, None)
            .await
            .expect("login should succeed without tenant_id");
        assert_eq!(user.id, user_id);

        cleanup_test_user(&pool, user_id).await;
    }

    #[tokio::test]
    async fn login_with_wrong_tenant_id_returns_unauthorized() {
        let pool = test_db_pool().await;
        let metadata_store = MetadataStore::new(pool.clone());

        let email = "scoped@example.com".to_string();
        let password = "scoped_password";
        let hash = PasswordHasher::hash(password).unwrap();

        let user_tenant = uuid::Uuid::new_v4();
        let other_tenant = uuid::Uuid::new_v4();

        let user_id = insert_test_user(&pool, &email, &hash, user_tenant).await;

        let req = LoginRequest {
            email: email.clone(),
            password: password.to_string(),
            tenant_id: Some(other_tenant),
        };
        let result = validate_credentials(&metadata_store, &req, None).await;
        assert!(
            matches!(result, Err(AppError::Unauthorized)),
            "expected Unauthorized for wrong tenant, got {:?}",
            result
        );

        cleanup_test_user(&pool, user_id).await;
    }

    #[tokio::test]
    async fn login_without_tenant_id_rejects_ambiguous_email() {
        let pool = test_db_pool().await;
        let metadata_store = MetadataStore::new(pool.clone());

        let email = "ambiguous@example.com".to_string();
        let password = "ambiguous_password";
        let hash = PasswordHasher::hash(password).unwrap();

        let tenant_a = uuid::Uuid::new_v4();
        let tenant_b = uuid::Uuid::new_v4();

        let user_a_id = insert_test_user(&pool, &email, &hash, tenant_a).await;
        let user_b_id = insert_test_user(&pool, &email, &hash, tenant_b).await;

        let req = LoginRequest {
            email: email.clone(),
            password: password.to_string(),
            tenant_id: None,
        };
        let result = validate_credentials(&metadata_store, &req, None).await;
        assert!(
            matches!(result, Err(AppError::Unauthorized)),
            "expected Unauthorized for ambiguous email without tenant_id, got {:?}",
            result
        );

        cleanup_test_user(&pool, user_a_id).await;
        cleanup_test_user(&pool, user_b_id).await;
    }

    #[tokio::test]
    async fn login_without_tenant_id_rejects_case_insensitive_ambiguous_email() {
        let pool = test_db_pool().await;
        let metadata_store = MetadataStore::new(pool.clone());

        let password = "ambiguous_case_password";
        let hash = PasswordHasher::hash(password).unwrap();

        let tenant_a = uuid::Uuid::new_v4();
        let tenant_b = uuid::Uuid::new_v4();

        let user_a_id =
            insert_test_user(&pool, "Case.Ambiguous@example.com", &hash, tenant_a).await;
        let user_b_id =
            insert_test_user(&pool, "case.ambiguous@example.com", &hash, tenant_b).await;

        let req = LoginRequest {
            email: "case.ambiguous@example.com".to_string(),
            password: password.to_string(),
            tenant_id: None,
        };
        let result = validate_credentials(&metadata_store, &req, None).await;
        assert!(
            matches!(result, Err(AppError::Unauthorized)),
            "expected Unauthorized for case-insensitive ambiguous email without tenant_id, got {:?}",
            result
        );

        cleanup_test_user(&pool, user_a_id).await;
        cleanup_test_user(&pool, user_b_id).await;
    }

    #[tokio::test]
    async fn login_with_tenant_id_is_case_insensitive() {
        let pool = test_db_pool().await;
        let metadata_store = MetadataStore::new(pool.clone());

        let canonical_email = "Mixed.Case@Example.COM";
        let login_email = "mixed.case@example.com";
        let password = "case_insensitive_password";
        let hash = PasswordHasher::hash(password).unwrap();
        let tenant_id = uuid::Uuid::new_v4();

        let user_id = insert_test_user(&pool, canonical_email, &hash, tenant_id).await;

        let req = LoginRequest {
            email: login_email.to_string(),
            password: password.to_string(),
            tenant_id: Some(tenant_id),
        };
        let user = validate_credentials(&metadata_store, &req, None)
            .await
            .expect("login with differently-cased email should succeed");
        assert_eq!(user.id, user_id);

        cleanup_test_user(&pool, user_id).await;
    }

    #[tokio::test]
    async fn login_timing_attack_resistance() {
        let pool = test_db_pool().await;
        let metadata_store = MetadataStore::new(pool.clone());

        // Create a test user with a real Argon2 hash.
        let correct_password = "correct_password_123";
        let hash = PasswordHasher::hash(correct_password).unwrap();
        let existing_email = format!("timing_{}@example.com", uuid::Uuid::new_v4());
        let user_id = insert_test_user(&pool, &existing_email, &hash, uuid::Uuid::nil()).await;

        let non_existent_email = format!("timing_{}@example.com", uuid::Uuid::new_v4());

        // Warm up Argon2 to reduce cold-start variance.
        let _ = PasswordHasher::verify("warmup", &hash);

        // Measure non-existent email branch.
        let mut non_existent_times = Vec::with_capacity(10);
        for _ in 0..10 {
            let req = LoginRequest {
                email: non_existent_email.clone(),
                password: "wrong_password".to_string(),
                tenant_id: None,
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
                tenant_id: None,
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
