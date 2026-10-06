//! PostgreSQL regression for credential invalidation on account deactivation.
//!
//! Run against the explicitly configured disposable local PostgreSQL and
//! RustFS services used by the integration workflow:
//! `cargo test -p rustshare-server --test user_disable_credential_revocation_test -- --ignored --test-threads=1`

mod support;

use std::error::Error;

use axum::{body::Body, http::Request};
use sha2::{Digest, Sha256};
use support::calendar_harness::{assert_local_database, setup_test_env, SERIAL};
use tower::ServiceExt;
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn assert_disposable_database(database_url: &str) {
    let parsed = url::Url::parse(database_url).expect("DATABASE_URL must be a PostgreSQL URL");
    let database_name = parsed.path().trim_start_matches('/');
    assert!(
        database_name == "rustshare_test" || database_name.starts_with("rustshare_test_"),
        "refusing to mutate database outside rustshare_test or rustshare_test_*"
    );
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB")
            .ok()
            .as_deref(),
        Some("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_DB=1 only for the disposable test database"
    );
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn disabling_user_revokes_credentials_and_stale_cookie_cannot_authenticate() {
    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL to an explicitly disposable local database");
    assert_disposable_database(&database_url);
    assert_local_database();
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE")
            .ok()
            .as_deref(),
        Some("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1 only for disposable local RustFS"
    );

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let session_token = rustshare_auth::generate_web_session_token();
    let session_hash = rustshare_auth::hash_web_session_token(&session_token);
    let device_token = format!("disable-credential-test-{}", Uuid::new_v4());
    let device_token_hash = hex::encode(Sha256::digest(device_token.as_bytes()));

    let attempt: TestResult<(
        axum::http::StatusCode,
        bool,
        bool,
        axum::http::StatusCode,
        bool,
        axum::http::StatusCode,
    )> = async {
            sqlx::query(
                "INSERT INTO tenants (id, name, created_at, updated_at)
                 VALUES ($1, $2, NOW(), NOW())",
            )
            .bind(tenant_id)
            .bind(format!("Credential revocation test {tenant_id}"))
            .execute(&pool)
            .await?;

            sqlx::query(
                "INSERT INTO users
                    (id, username, email, password_hash, display_name,
                     storage_quota, tenant_id)
                 VALUES ($1, $2, $3, 'test-password-hash', $2, 10737418240, $4)",
            )
            .bind(user_id)
            .bind(format!("disable_user_{}", user_id.simple()))
            .bind(format!("disable-user-{}@test.local", user_id.simple()))
            .bind(tenant_id)
            .execute(&pool)
            .await?;

            sqlx::query(
                "INSERT INTO user_sessions (id, user_id, session_token_hash, expires_at, tenant_id)
                 VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4)",
            )
            .bind(session_id)
            .bind(user_id)
            .bind(&session_hash)
            .bind(tenant_id)
            .execute(&pool)
            .await?;
            sqlx::query(
                "INSERT INTO device_tokens (id, user_id, token_hash, device_name)
                 VALUES ($1, $2, $3, 'disable credential regression')",
            )
            .bind(Uuid::new_v4())
            .bind(user_id)
            .bind(&device_token_hash)
            .execute(&pool)
            .await?;

            let app = rustshare_server::routes::user_routes().with_state(state.clone());
            let authenticated = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/v1/me")
                        .header(
                            axum::http::header::COOKIE,
                            format!("{}={session_token}", rustshare_auth::WEB_SESSION_COOKIE_NAME),
                        )
                        .body(Body::empty())?,
                )
                .await?
                .status();

            sqlx::query("UPDATE users SET disabled_at = NOW() WHERE id = $1")
                .bind(user_id)
                .execute(&pool)
                .await?;

            let session_exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM user_sessions WHERE id = $1)",
            )
            .bind(session_id)
            .fetch_one(&pool)
            .await?;
            let device_token_revoked = sqlx::query_scalar::<_, bool>(
                "SELECT revoked_at IS NOT NULL FROM device_tokens WHERE user_id = $1 AND token_hash = $2",
            )
            .bind(user_id)
            .bind(&device_token_hash)
            .fetch_one(&pool)
            .await?;

            // Simulate a pre-trigger/legacy session that remained in storage.
            // Cookie authentication must still fail closed and remove it.
            let stale_session_id = Uuid::new_v4();
            let stale_session_token = rustshare_auth::generate_web_session_token();
            let stale_session_hash = rustshare_auth::hash_web_session_token(&stale_session_token);
            sqlx::query(
                "INSERT INTO user_sessions (id, user_id, session_token_hash, expires_at, tenant_id)
                 VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4)",
            )
            .bind(stale_session_id)
            .bind(user_id)
            .bind(&stale_session_hash)
            .bind(tenant_id)
            .execute(&pool)
            .await?;
            let stale_cookie_status = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/v1/me")
                        .header(
                            axum::http::header::COOKIE,
                            format!(
                                "{}={stale_session_token}",
                                rustshare_auth::WEB_SESSION_COOKIE_NAME
                            ),
                        )
                        .body(Body::empty())?,
                )
                .await?
                .status();
            let stale_session_exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM user_sessions WHERE id = $1)",
            )
            .bind(stale_session_id)
            .fetch_one(&pool)
            .await?;

            sqlx::query("UPDATE users SET disabled_at = NULL WHERE id = $1")
                .bind(user_id)
                .execute(&pool)
                .await?;
            let old_cookie_after_reenable = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/v1/me")
                        .header(
                            axum::http::header::COOKIE,
                            format!("{}={session_token}", rustshare_auth::WEB_SESSION_COOKIE_NAME),
                        )
                        .body(Body::empty())?,
                )
                .await?
                .status();

            Ok((
                authenticated,
                session_exists,
                device_token_revoked,
                stale_cookie_status,
                stale_session_exists,
                old_cookie_after_reenable,
            ))
        }
        .await;

    let mut cleanup_error = None;
    for statement in [
        format!("DELETE FROM users WHERE id = '{user_id}'"),
        format!("DELETE FROM tenants WHERE id = '{tenant_id}'"),
    ] {
        if let Err(error) = sqlx::query(&statement).execute(&pool).await {
            cleanup_error.get_or_insert(error);
        }
    }
    if let Some(error) = cleanup_error {
        panic!("failed to clean up disposable credential-revocation fixtures: {error}");
    }

    let (
        authenticated,
        session_exists,
        device_token_revoked,
        stale_cookie_status,
        stale_session_exists,
        old_cookie_after_reenable,
    ) = attempt.expect("complete credential revocation regression");
    assert_eq!(authenticated, axum::http::StatusCode::OK);
    assert!(
        !session_exists,
        "disable trigger deletes persisted browser sessions"
    );
    assert!(
        device_token_revoked,
        "disable trigger revokes device tokens"
    );
    assert_eq!(stale_cookie_status, axum::http::StatusCode::UNAUTHORIZED);
    assert!(
        !stale_session_exists,
        "disabled-user stale session is deleted"
    );
    assert_eq!(
        old_cookie_after_reenable,
        axum::http::StatusCode::UNAUTHORIZED,
        "re-enabling an account does not restore its old browser session"
    );
}
