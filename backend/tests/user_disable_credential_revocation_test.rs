//! PostgreSQL regression for credential invalidation on account deactivation.
//!
//! Two paths: the SCIM-driven disable/delete flows (sessions deleted, device
//! tokens and stale cookies rejected, content preserved) and a credential
//! minted *before* an out-of-band `UPDATE users SET disabled_at`, which must
//! stop authenticating without the credential itself being touched.
//!
//! Run against the explicitly configured disposable local PostgreSQL and
//! RustFS services used by the integration workflow:
//! `cargo test -p rustshare-server --test user_disable_credential_revocation_test -- --ignored --test-threads=1`

mod support;

use std::error::Error;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use bytes::Bytes;
use sha2::{Digest, Sha256};
use support::calendar_harness::{assert_local_database, create_auth_token, setup_test_env, SERIAL};
use tower::ServiceExt;
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// `GET /api/v1/me` authenticated with a bearer credential (JWT or device
/// token) and no session cookie, i.e. the live-token path the extractor
/// re-checks against `users.disabled_at` on every request.
async fn get_me_with_bearer(app: &axum::Router, token: &str) -> TestResult<StatusCode> {
    Ok(app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/me")
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await?
        .status())
}

struct CredentialRevocationResult {
    authenticated: StatusCode,
    scim_v1_status: StatusCode,
    v1_disabled: bool,
    v1_session_exists: bool,
    v1_device_token_revoked_after_reenable: bool,
    stale_cookie_status: StatusCode,
    stale_session_exists: bool,
    old_cookie_after_v1_reenable: StatusCode,
    scim_v2_status: StatusCode,
    v2_disabled: bool,
    v2_session_exists: bool,
    v2_device_token_revoked_after_reenable: bool,
    old_cookie_after_v2_reenable: StatusCode,
    scim_v2_delete_status: StatusCode,
    v2_delete_user_exists_and_disabled: bool,
    v2_delete_session_exists: bool,
    v2_delete_device_token_revoked_after_reenable: bool,
    old_cookie_after_v2_delete_reenable: StatusCode,
    v2_delete_user_exists_after_reenable: bool,
    v2_delete_file_metadata_preserved: bool,
    v2_delete_object_bytes_preserved: bool,
}

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
    let file_id = Uuid::new_v4();
    let object_key = format!("rustshare-test/scim-delete/{file_id}");
    let external_id = format!("scim-disable-{}", user_id.simple());
    let scim_token = std::env::var("RUSTSHARE_SCIM_BEARER_TOKEN")
        .expect("set RUSTSHARE_SCIM_BEARER_TOKEN to a disposable test token");
    assert!(!scim_token.is_empty(), "SCIM test token must not be empty");
    // The SCIM integration is bound to exactly one organization (ADR-0040 D5):
    // pin the scope to this test's tenant so the same-organization SCIM
    // disable/delete flows under test remain reachable. The guard restores
    // the previous value when the test ends.
    let _scim_scope_guard = support::env_guard::EnvGuard::set(vec![(
        "RUSTSHARE_SCIM_TENANT_ID",
        Some(tenant_id.to_string()),
    )]);
    let session_id = Uuid::new_v4();
    let session_token = rustshare_auth::generate_web_session_token();
    let session_hash = rustshare_auth::hash_web_session_token(&session_token);
    let device_token = format!("disable-credential-test-{}", Uuid::new_v4());
    let device_token_hash = hex::encode(Sha256::digest(device_token.as_bytes()));

    let attempt: TestResult<CredentialRevocationResult> = async {
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
                     storage_quota, tenant_id, external_id)
                 VALUES ($1, $2, $3, 'test-password-hash', $2, 10737418240, $4, $5)",
            )
            .bind(user_id)
            .bind(format!("disable_user_{}", user_id.simple()))
            .bind(format!("disable-user-{}@test.local", user_id.simple()))
            .bind(tenant_id)
            .bind(&external_id)
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

            let app = rustshare_server::routes::scim_routes()
                .merge(rustshare_server::routes::user_routes())
                .with_state(state.clone());
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

            let scim_v1_status = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("DELETE")
                        .uri(format!("/api/v1/scim/users/{external_id}"))
                        .header(
                            axum::http::header::AUTHORIZATION,
                            format!("Bearer {scim_token}"),
                        )
                        .body(Body::empty())?,
                )
                .await?
                .status();

            let v1_disabled = sqlx::query_scalar::<_, bool>(
                "SELECT disabled_at IS NOT NULL FROM users WHERE id = $1",
            )
            .bind(user_id)
            .fetch_one(&pool)
            .await?;

            let v1_session_exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM user_sessions WHERE id = $1)",
            )
            .bind(session_id)
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
            let v1_device_token_revoked_after_reenable = sqlx::query_scalar::<_, bool>(
                "SELECT revoked_at IS NOT NULL FROM device_tokens WHERE user_id = $1 AND token_hash = $2",
            )
            .bind(user_id)
            .bind(&device_token_hash)
            .fetch_one(&pool)
            .await?;
            let old_cookie_after_v1_reenable = app
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

            let v2_session_id = Uuid::new_v4();
            let v2_session_token = rustshare_auth::generate_web_session_token();
            let v2_session_hash = rustshare_auth::hash_web_session_token(&v2_session_token);
            let v2_device_token = format!("disable-credential-test-{}", Uuid::new_v4());
            let v2_device_token_hash = hex::encode(Sha256::digest(v2_device_token.as_bytes()));
            sqlx::query(
                "INSERT INTO user_sessions (id, user_id, session_token_hash, expires_at, tenant_id)
                 VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4)",
            )
            .bind(v2_session_id)
            .bind(user_id)
            .bind(&v2_session_hash)
            .bind(tenant_id)
            .execute(&pool)
            .await?;
            sqlx::query(
                "INSERT INTO device_tokens (id, user_id, token_hash, device_name)
                 VALUES ($1, $2, $3, 'SCIM v2 credential regression')",
            )
            .bind(Uuid::new_v4())
            .bind(user_id)
            .bind(&v2_device_token_hash)
            .execute(&pool)
            .await?;

            let scim_v2_status = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PATCH")
                        .uri(format!("/scim/v2/Users/{user_id}"))
                        .header(
                            axum::http::header::AUTHORIZATION,
                            format!("Bearer {scim_token}"),
                        )
                        .header(axum::http::header::CONTENT_TYPE, "application/json")
                        .body(Body::from(
                            r#"{"schemas":["urn:ietf:params:scim:api:messages:2.0:PatchOp"],"Operations":[{"op":"replace","path":"active","value":false}]}"#,
                        ))?,
                )
                .await?
                .status();

            let v2_disabled = sqlx::query_scalar::<_, bool>(
                "SELECT disabled_at IS NOT NULL FROM users WHERE id = $1",
            )
            .bind(user_id)
            .fetch_one(&pool)
            .await?;
            let v2_session_exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM user_sessions WHERE id = $1)",
            )
            .bind(v2_session_id)
            .fetch_one(&pool)
            .await?;
            sqlx::query("UPDATE users SET disabled_at = NULL WHERE id = $1")
                .bind(user_id)
                .execute(&pool)
                .await?;
            let v2_device_token_revoked_after_reenable = sqlx::query_scalar::<_, bool>(
                "SELECT revoked_at IS NOT NULL FROM device_tokens WHERE user_id = $1 AND token_hash = $2",
            )
            .bind(user_id)
            .bind(&v2_device_token_hash)
            .fetch_one(&pool)
            .await?;
            let old_cookie_after_v2_reenable = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/v1/me")
                        .header(
                            axum::http::header::COOKIE,
                            format!(
                                "{}={v2_session_token}",
                                rustshare_auth::WEB_SESSION_COOKIE_NAME
                            ),
                        )
                        .body(Body::empty())?,
                )
                .await?
                .status();

            let delete_session_id = Uuid::new_v4();
            let delete_session_token = rustshare_auth::generate_web_session_token();
            let delete_session_hash =
                rustshare_auth::hash_web_session_token(&delete_session_token);
            let delete_device_token = format!("scim-delete-credential-{}", Uuid::new_v4());
            let delete_device_token_hash =
                hex::encode(Sha256::digest(delete_device_token.as_bytes()));
            sqlx::query(
                "INSERT INTO user_sessions (id, user_id, session_token_hash, expires_at, tenant_id)
                 VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4)",
            )
            .bind(delete_session_id)
            .bind(user_id)
            .bind(&delete_session_hash)
            .bind(tenant_id)
            .execute(&pool)
            .await?;
            sqlx::query(
                "INSERT INTO device_tokens (id, user_id, token_hash, device_name)
                 VALUES ($1, $2, $3, 'SCIM v2 DELETE credential regression')",
            )
            .bind(Uuid::new_v4())
            .bind(user_id)
            .bind(&delete_device_token_hash)
            .execute(&pool)
            .await?;

            let file_bytes = b"SCIM v2 DELETE must preserve this exact file content.\n";
            let file_content_hash = hex::encode(Sha256::digest(file_bytes));
            state
                .object_store
                .put(&object_key, Bytes::from_static(file_bytes))
                .await?;
            sqlx::query(
                "INSERT INTO files
                    (id, name, path, size, mime_type, content_hash, storage_key,
                     owner_id, tenant_id, current_version)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 1)",
            )
            .bind(file_id)
            .bind("scim-delete-preserved.txt")
            .bind(format!("/scim-delete-preserved-{file_id}.txt"))
            .bind(file_bytes.len() as i64)
            .bind("text/plain")
            .bind(&file_content_hash)
            .bind(&object_key)
            .bind(user_id)
            .bind(tenant_id)
            .execute(&pool)
            .await?;

            let scim_v2_delete_status = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("DELETE")
                        .uri(format!("/scim/v2/Users/{user_id}"))
                        .header(
                            axum::http::header::AUTHORIZATION,
                            format!("Bearer {scim_token}"),
                        )
                        .body(Body::empty())?,
                )
                .await?
                .status();
            let v2_delete_user_exists_and_disabled = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (
                    SELECT 1 FROM users WHERE id = $1 AND disabled_at IS NOT NULL
                 )",
            )
            .bind(user_id)
            .fetch_one(&pool)
            .await?;
            let v2_delete_session_exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM user_sessions WHERE id = $1)",
            )
            .bind(delete_session_id)
            .fetch_one(&pool)
            .await?;
            sqlx::query("UPDATE users SET disabled_at = NULL WHERE id = $1")
                .bind(user_id)
                .execute(&pool)
                .await?;
            let v2_delete_device_token_revoked_after_reenable =
                sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS (
                        SELECT 1 FROM device_tokens
                        WHERE user_id = $1 AND token_hash = $2 AND revoked_at IS NOT NULL
                    )",
                )
                .bind(user_id)
                .bind(&delete_device_token_hash)
                .fetch_one(&pool)
                .await?;
            let old_cookie_after_v2_delete_reenable = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/v1/me")
                        .header(
                            axum::http::header::COOKIE,
                            format!(
                                "{}={delete_session_token}",
                                rustshare_auth::WEB_SESSION_COOKIE_NAME
                            ),
                        )
                        .body(Body::empty())?,
                )
                .await?
                .status();
            let v2_delete_user_exists_after_reenable = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND disabled_at IS NULL)",
            )
            .bind(user_id)
            .fetch_one(&pool)
            .await?;
            let preserved_file = sqlx::query_as::<_, (String, String, i64, String)>(
                "SELECT name, storage_key, size, content_hash FROM files
                 WHERE id = $1 AND owner_id = $2 AND tenant_id = $3",
            )
            .bind(file_id)
            .bind(user_id)
            .bind(tenant_id)
            .fetch_optional(&pool)
            .await?;
            let v2_delete_file_metadata_preserved = preserved_file.is_some_and(
                |(name, key, size, hash)| {
                    name == "scim-delete-preserved.txt"
                        && key == object_key
                        && size == file_bytes.len() as i64
                        && hash == file_content_hash
                },
            );
            let v2_delete_object_bytes_preserved = state
                .object_store
                .get(&object_key)
                .await?
                .as_ref()
                == file_bytes;

            Ok(CredentialRevocationResult {
                authenticated,
                scim_v1_status,
                v1_disabled,
                v1_session_exists,
                v1_device_token_revoked_after_reenable,
                stale_cookie_status,
                stale_session_exists,
                old_cookie_after_v1_reenable,
                scim_v2_status,
                v2_disabled,
                v2_session_exists,
                v2_device_token_revoked_after_reenable,
                old_cookie_after_v2_reenable,
                scim_v2_delete_status,
                v2_delete_user_exists_and_disabled,
                v2_delete_session_exists,
                v2_delete_device_token_revoked_after_reenable,
                old_cookie_after_v2_delete_reenable,
                v2_delete_user_exists_after_reenable,
                v2_delete_file_metadata_preserved,
                v2_delete_object_bytes_preserved,
            })
        }
        .await;

    let mut cleanup_errors = Vec::new();
    if let Err(error) = state.object_store.delete(&object_key).await {
        cleanup_errors.push(format!("delete fixture object: {error}"));
    }
    for (statement, id) in [
        ("DELETE FROM files WHERE id = $1 AND owner_id = $2", file_id),
        ("DELETE FROM users WHERE id = $1", user_id),
        ("DELETE FROM tenants WHERE id = $1", tenant_id),
    ] {
        let result = if statement.starts_with("DELETE FROM files") {
            sqlx::query(statement)
                .bind(id)
                .bind(user_id)
                .execute(&pool)
                .await
        } else {
            sqlx::query(statement).bind(id).execute(&pool).await
        };
        if let Err(error) = result {
            cleanup_errors.push(format!("{statement}: {error}"));
        }
    }
    if !cleanup_errors.is_empty() {
        panic!(
            "failed to clean up disposable credential-revocation fixtures: {}",
            cleanup_errors.join("; ")
        );
    }

    let CredentialRevocationResult {
        authenticated,
        scim_v1_status,
        v1_disabled,
        v1_session_exists,
        v1_device_token_revoked_after_reenable,
        stale_cookie_status,
        stale_session_exists,
        old_cookie_after_v1_reenable,
        scim_v2_status,
        v2_disabled,
        v2_session_exists,
        v2_device_token_revoked_after_reenable,
        old_cookie_after_v2_reenable,
        scim_v2_delete_status,
        v2_delete_user_exists_and_disabled,
        v2_delete_session_exists,
        v2_delete_device_token_revoked_after_reenable,
        old_cookie_after_v2_delete_reenable,
        v2_delete_user_exists_after_reenable,
        v2_delete_file_metadata_preserved,
        v2_delete_object_bytes_preserved,
    } = attempt.expect("complete credential revocation regression");
    assert_eq!(authenticated, StatusCode::OK);
    assert_eq!(scim_v1_status, StatusCode::NO_CONTENT);
    assert!(v1_disabled, "SCIM v1 deprovision disables the account");
    assert!(
        !v1_session_exists,
        "SCIM v1 deprovision trigger deletes persisted browser sessions"
    );
    assert!(
        v1_device_token_revoked_after_reenable,
        "SCIM v1 device-token revocation persists after re-enabling"
    );
    assert_eq!(stale_cookie_status, StatusCode::UNAUTHORIZED);
    assert!(
        !stale_session_exists,
        "disabled-user stale session is deleted"
    );
    assert_eq!(
        old_cookie_after_v1_reenable,
        StatusCode::UNAUTHORIZED,
        "re-enabling after SCIM v1 deprovision does not restore its old browser session"
    );
    assert_eq!(scim_v2_status, StatusCode::OK);
    assert!(v2_disabled, "SCIM v2 active=false disables the account");
    assert!(
        !v2_session_exists,
        "SCIM v2 deprovision trigger deletes persisted browser sessions"
    );
    assert!(
        v2_device_token_revoked_after_reenable,
        "SCIM v2 device-token revocation persists after re-enabling"
    );
    assert_eq!(
        old_cookie_after_v2_reenable,
        StatusCode::UNAUTHORIZED,
        "re-enabling after SCIM v2 deprovision does not restore its old browser session"
    );
    assert_eq!(scim_v2_delete_status, StatusCode::NO_CONTENT);
    assert!(
        v2_delete_user_exists_and_disabled,
        "SCIM v2 DELETE retains the user row in the disabled state"
    );
    assert!(
        !v2_delete_session_exists,
        "SCIM v2 DELETE trigger removes persisted browser sessions"
    );
    assert!(
        v2_delete_device_token_revoked_after_reenable,
        "re-enabling after SCIM v2 DELETE does not restore its device token"
    );
    assert_eq!(
        old_cookie_after_v2_delete_reenable,
        StatusCode::UNAUTHORIZED,
        "re-enabling after SCIM v2 DELETE does not restore its old browser session"
    );
    assert!(
        v2_delete_user_exists_after_reenable,
        "the retained SCIM v2 user remains present when re-enabled"
    );
    assert!(
        v2_delete_file_metadata_preserved,
        "SCIM v2 DELETE retains the owned file metadata"
    );
    assert!(
        v2_delete_object_bytes_preserved,
        "SCIM v2 DELETE retains the exact owned object bytes"
    );
}

/// **DOCUMENTED RESIDUAL RISK — change detector, not an endorsed contract.**
/// Re-enabling a user makes a JWT minted *before* the disable valid again,
/// because JWTs are stateless and there is no server-side denylist: the only
/// bound is the token's own 24 h `exp`. The test below asserts this so the
/// exposure window stays visible. When the denylist / `tokens_valid_after`
/// design lands (issue #215, identity/revocation), this field and its
/// assertion must flip to `UNAUTHORIZED` — a failure there is the intended
/// signal, not a broken contract.
struct LiveCredentialResult {
    jwt_before_disable: StatusCode,
    device_token_before_disable: StatusCode,
    disabled: bool,
    jwt_after_disable: StatusCode,
    device_token_after_disable: StatusCode,
    device_token_revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    jwt_after_reenable_residual_risk: StatusCode,
    device_token_after_reenable: StatusCode,
}

/// A credential issued *before* the disable must stop working the moment
/// `users.disabled_at` is set, without any credential-specific cleanup running
/// on the credential itself: the extractor re-reads `disabled_at` per request
/// (`server/src/handlers/extractors.rs::resolve_bearer_token`). Covers the
/// live-token HTTP path that the SCIM-driven regression above does not: there
/// the disabler and the credential check are the same incident, here the token
/// is minted first and only the DB writes change.
#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn live_credentials_issued_before_disable_are_rejected_while_disabled() {
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
    let device_token = format!("live-credential-test-{}", Uuid::new_v4());
    let device_token_hash = hex::encode(Sha256::digest(device_token.as_bytes()));

    let attempt: TestResult<LiveCredentialResult> = async {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at)
             VALUES ($1, $2, NOW(), NOW())",
        )
        .bind(tenant_id)
        .bind(format!("Live credential test {tenant_id}"))
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO users
                (id, username, email, password_hash, display_name,
                 storage_quota, tenant_id)
             VALUES ($1, $2, $3, 'test-password-hash', $2, 10737418240, $4)",
        )
        .bind(user_id)
        .bind(format!("live_credential_{}", user_id.simple()))
        .bind(format!("live-credential-{}@test.local", user_id.simple()))
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO device_tokens (id, user_id, token_hash, device_name)
             VALUES ($1, $2, $3, 'live credential regression')",
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(&device_token_hash)
        .execute(&pool)
        .await?;

        // Minted before the disable, valid for 24h: expiry is not what
        // must reject it.
        let jwt = create_auth_token(&state, user_id, tenant_id);
        let app = rustshare_server::routes::user_routes().with_state(state.clone());

        let jwt_before_disable = get_me_with_bearer(&app, &jwt).await?;
        let device_token_before_disable = get_me_with_bearer(&app, &device_token).await?;

        sqlx::query("UPDATE users SET disabled_at = NOW() WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await?;

        let disabled = sqlx::query_scalar::<_, bool>(
            "SELECT disabled_at IS NOT NULL FROM users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await?;
        let jwt_after_disable = get_me_with_bearer(&app, &jwt).await?;
        let device_token_after_disable = get_me_with_bearer(&app, &device_token).await?;
        let device_token_revoked_at =
            sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
                "SELECT revoked_at FROM device_tokens
                 WHERE user_id = $1 AND token_hash = $2",
            )
            .bind(user_id)
            .bind(&device_token_hash)
            .fetch_one(&pool)
            .await?;

        // Re-enable: the stateless JWT has no server-side revocation list,
        // so it is honored again until its own `exp`. The device token is
        // revoked for good. Asserting both documents the residual exposure
        // window rather than endorsing it.
        sqlx::query("UPDATE users SET disabled_at = NULL WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await?;
        let jwt_after_reenable_residual_risk = get_me_with_bearer(&app, &jwt).await?;
        let device_token_after_reenable = get_me_with_bearer(&app, &device_token).await?;

        Ok(LiveCredentialResult {
            jwt_before_disable,
            device_token_before_disable,
            disabled,
            jwt_after_disable,
            device_token_after_disable,
            device_token_revoked_at,
            jwt_after_reenable_residual_risk,
            device_token_after_reenable,
        })
    }
    .await;

    let mut cleanup_errors = Vec::new();
    for (statement, id) in [
        ("DELETE FROM users WHERE id = $1", user_id),
        ("DELETE FROM tenants WHERE id = $1", tenant_id),
    ] {
        if let Err(error) = sqlx::query(statement).bind(id).execute(&pool).await {
            cleanup_errors.push(format!("{statement}: {error}"));
        }
    }
    if !cleanup_errors.is_empty() {
        panic!(
            "failed to clean up disposable live-credential fixtures: {}",
            cleanup_errors.join("; ")
        );
    }

    let LiveCredentialResult {
        jwt_before_disable,
        device_token_before_disable,
        disabled,
        jwt_after_disable,
        device_token_after_disable,
        device_token_revoked_at,
        jwt_after_reenable_residual_risk,
        device_token_after_reenable,
    } = attempt.expect("complete live-credential regression");
    assert_eq!(jwt_before_disable, StatusCode::OK);
    assert_eq!(device_token_before_disable, StatusCode::OK);
    assert!(disabled, "the disable write must stick");
    assert_eq!(
        jwt_after_disable,
        StatusCode::UNAUTHORIZED,
        "a JWT issued before the disable must be rejected while disabled_at is set"
    );
    assert_eq!(
        device_token_after_disable,
        StatusCode::UNAUTHORIZED,
        "a device token issued before the disable must be rejected while disabled_at is set"
    );
    assert!(
        device_token_revoked_at.is_some(),
        "the disable trigger revokes the device token"
    );
    // =====================================================================
    // DOCUMENTED RESIDUAL RISK — CHANGE DETECTOR, NOT A BROKEN CONTRACT.
    //
    // This asserts the *current* behavior: a stateless, 24 h-TTL JWT minted
    // before the disable is honored again after re-enable, because there is
    // no server-side JWT denylist (only `users.disabled_at`, which is NULL
    // again). It is here to keep the residual exposure window visible. When
    // the denylist / `tokens_valid_after` design lands (issue #215,
    // identity/revocation), this must become `UNAUTHORIZED` and this
    // assertion (and `LiveCredentialResult::jwt_after_reenable_residual_risk`)
    // must be updated — do not read a failure here as a regression.
    // =====================================================================
    assert_eq!(
        jwt_after_reenable_residual_risk,
        StatusCode::OK,
        "DOCUMENTED RESIDUAL RISK: re-enabling currently revives a pre-disable \
         JWT until its own expiry (stateless token, no server-side denylist). \
         Update this assertion when the denylist / tokens_valid_after design \
         (issue #215) lands."
    );
    assert_eq!(
        device_token_after_reenable,
        StatusCode::UNAUTHORIZED,
        "a revoked device token stays revoked across re-enable"
    );
}

/// AR-01 / ADR-0040 D8: disabling through the admin handler produces
/// identical credential revocation to the SCIM-shaped `UPDATE users SET
/// disabled_at`: the shared `set_user_disabled_at` write lets the
/// `users_disable_revokes_credentials` trigger delete the web session and
/// revoke the device token, and re-enable does not resurrect them. This is a
/// coverage/parity and handler-consistency property — the trigger, not any
/// handler code, performs the revocation.
#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn admin_disable_produces_identical_credential_revocation() {
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
    let admin_id = Uuid::new_v4();
    let target_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let session_token = rustshare_auth::generate_web_session_token();
    let session_hash = rustshare_auth::hash_web_session_token(&session_token);
    let device_token = format!("admin-disable-parity-{}", Uuid::new_v4());
    let device_token_hash = hex::encode(Sha256::digest(device_token.as_bytes()));

    sqlx::query(
        "INSERT INTO tenants (id, name, created_at, updated_at)
         VALUES ($1, $2, NOW(), NOW())",
    )
    .bind(tenant_id)
    .bind(format!("Admin disable parity test {tenant_id}"))
    .execute(&pool)
    .await
    .expect("insert test tenant");

    for (user_id, username, is_admin) in [
        (admin_id, format!("ar01_admin_{}", admin_id.simple()), true),
        (
            target_id,
            format!("ar01_target_{}", target_id.simple()),
            false,
        ),
    ] {
        sqlx::query(
            "INSERT INTO users
                (id, username, email, password_hash, display_name, is_admin,
                 storage_quota, tenant_id)
             VALUES ($1, $2, $3, 'test-password-hash', $2, $4, 10737418240, $5)",
        )
        .bind(user_id)
        .bind(&username)
        .bind(format!("{username}@test.local"))
        .bind(is_admin)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .expect("insert test user");
    }

    sqlx::query(
        "INSERT INTO user_sessions (id, user_id, session_token_hash, expires_at, tenant_id)
         VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4)",
    )
    .bind(session_id)
    .bind(target_id)
    .bind(&session_hash)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .expect("insert live web session");

    sqlx::query(
        "INSERT INTO device_tokens (id, user_id, token_hash, device_name)
         VALUES ($1, $2, $3, 'admin disable parity')",
    )
    .bind(Uuid::new_v4())
    .bind(target_id)
    .bind(&device_token_hash)
    .execute(&pool)
    .await
    .expect("insert live device token");

    let app = rustshare_server::routes::admin_routes()
        .merge(rustshare_server::routes::user_routes())
        .with_state(state.clone());
    let admin_bearer = create_auth_token(&state, admin_id, tenant_id);

    let disable_status = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/users/{target_id}/disable"))
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {admin_bearer}"),
                )
                .body(Body::empty())
                .expect("build disable request"),
        )
        .await
        .expect("disable request")
        .status();
    assert_eq!(disable_status, StatusCode::NO_CONTENT);

    let disabled_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT disabled_at FROM users WHERE id = $1")
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .expect("fetch disabled_at");
    assert!(disabled_at.is_some(), "disable must set disabled_at");
    let session_row_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM user_sessions
            WHERE user_id = $1 AND session_token_hash = $2 AND expires_at > NOW()
        )",
    )
    .bind(target_id)
    .bind(&session_hash)
    .fetch_one(&pool)
    .await
    .expect("check session rows");
    assert!(
        !session_row_exists,
        "disable must leave no user_sessions row — identical to the SCIM-shaped UPDATE"
    );
    let device_revoked_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT revoked_at FROM device_tokens WHERE user_id = $1 AND token_hash = $2",
    )
    .bind(target_id)
    .bind(&device_token_hash)
    .fetch_one(&pool)
    .await
    .expect("fetch device token revocation");
    assert!(
        device_revoked_at.is_some(),
        "disable must revoke the device token — identical to the SCIM-shaped UPDATE"
    );
    assert_eq!(
        get_me_with_bearer(&app, &device_token)
            .await
            .expect("device probe"),
        StatusCode::UNAUTHORIZED,
        "the revoked device token must stop authenticating"
    );
    let stale_cookie_status = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/me/sessions")
                .header(
                    axum::http::header::COOKIE,
                    format!(
                        "{}={session_token}",
                        rustshare_auth::WEB_SESSION_COOKIE_NAME
                    ),
                )
                .body(Body::empty())
                .expect("build stale cookie request"),
        )
        .await
        .expect("stale cookie request")
        .status();
    assert_eq!(
        stale_cookie_status,
        StatusCode::UNAUTHORIZED,
        "a session deleted by the disable trigger must not authenticate"
    );

    // Re-enable: server-side credentials must stay revoked (A6).
    let enable_status = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/users/{target_id}/enable"))
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {admin_bearer}"),
                )
                .body(Body::empty())
                .expect("build enable request"),
        )
        .await
        .expect("enable request")
        .status();
    assert_eq!(enable_status, StatusCode::NO_CONTENT);
    let device_revoked_after_reenable: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT revoked_at FROM device_tokens WHERE user_id = $1 AND token_hash = $2",
    )
    .bind(target_id)
    .bind(&device_token_hash)
    .fetch_one(&pool)
    .await
    .expect("fetch device token revocation after re-enable");
    assert!(
        device_revoked_after_reenable.is_some(),
        "a revoked device token must stay revoked across re-enable"
    );
    let session_row_after_reenable: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM user_sessions
            WHERE user_id = $1 AND session_token_hash = $2 AND expires_at > NOW()
        )",
    )
    .bind(target_id)
    .bind(&session_hash)
    .fetch_one(&pool)
    .await
    .expect("check session rows after re-enable");
    assert!(
        !session_row_after_reenable,
        "deleted sessions must not come back on re-enable"
    );

    // Cleanup (mirror the suites' disposable-data hygiene).
    sqlx::query("DELETE FROM admin_actions WHERE target_id = $1 OR actor_id = $1")
        .bind(target_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM admin_actions WHERE actor_id = $1")
        .bind(admin_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM device_tokens WHERE user_id = $1")
        .bind(target_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM user_sessions WHERE user_id = $1")
        .bind(target_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
        .bind(target_id)
        .bind(admin_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .ok();
}

/// ADR-0040 negative test 6 / invariant A7: tenant drift fails closed.
///
/// - A web session whose `tenant_id` disagrees with the user's row is
///   invalidated at resolution (deleted, not honoured).
/// - A WebSocket credential whose JWT `tenant_id` claim disagrees with the
///   user's row is rejected; a matching claim resolves to the row's
///   organization, never the claim's.
#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn tenant_drift_fails_closed_for_sessions_and_websockets() {
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
    let user_tenant = Uuid::new_v4();
    let foreign_tenant = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let session_token = rustshare_auth::generate_web_session_token();
    let session_hash = rustshare_auth::hash_web_session_token(&session_token);

    sqlx::query(
        "INSERT INTO users
            (id, username, email, password_hash, display_name, is_admin,
             storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, false, 10737418240, $4)",
    )
    .bind(user_id)
    .bind(format!("ar01_drift_{}", user_id.simple()))
    .bind(format!("ar01-drift-{}@test.local", user_id.simple()))
    .bind(user_tenant)
    .execute(&pool)
    .await
    .expect("insert test user");

    // The drifted session: recorded organization differs from the user's row.
    sqlx::query(
        "INSERT INTO user_sessions (id, user_id, session_token_hash, expires_at, tenant_id)
         VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4)",
    )
    .bind(session_id)
    .bind(user_id)
    .bind(&session_hash)
    .bind(foreign_tenant)
    .execute(&pool)
    .await
    .expect("insert drifted web session");

    // Session resolution must fail closed and invalidate the session.
    let resolved = rustshare_server::web_session::resolve_user_session(&state, &session_token)
        .await
        .expect("session resolution must not error");
    assert_eq!(
        resolved, None,
        "a session whose tenant disagrees with the user's row must be invalidated"
    );
    let session_row_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_sessions WHERE id = $1)")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .expect("check invalidated session row");
    assert!(
        !session_row_exists,
        "the drifted session must be deleted, not merely refused"
    );

    // WebSocket: a foreign tenant claim must be rejected (A7).
    let foreign_claim_token = state
        .jwt_manager
        .generate(user_id, "drift@test.local", foreign_tenant)
        .expect("mint foreign-claim JWT");
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {foreign_claim_token}").parse().unwrap(),
    );
    let rejected = rustshare_server::handlers::ws_auth::resolve_ws_client_identity(
        &state,
        &headers,
        &rustshare_server::handlers::ws_auth::WsAuthQuery { token: None },
    )
    .await;
    assert!(
        matches!(rejected, Err((StatusCode::UNAUTHORIZED, _))),
        "a WebSocket credential with a foreign tenant claim must be rejected, got {:?}",
        rejected.map(|identity| format!("{identity:?}"))
    );

    // Control: a matching claim resolves to the row's organization.
    let matching_claim_token = state
        .jwt_manager
        .generate(user_id, "match@test.local", user_tenant)
        .expect("mint matching-claim JWT");
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {matching_claim_token}").parse().unwrap(),
    );
    let identity = rustshare_server::handlers::ws_auth::resolve_ws_client_identity(
        &state,
        &headers,
        &rustshare_server::handlers::ws_auth::WsAuthQuery { token: None },
    )
    .await
    .expect("matching-claim credential must resolve");
    match identity {
        rustshare_server::handlers::ws_auth::ClientIdentity::User {
            user_id: resolved_user,
            tenant_id,
            ..
        } => {
            assert_eq!(resolved_user, user_id);
            assert_eq!(
                tenant_id, user_tenant,
                "the WebSocket identity's organization must come from the users row"
            );
        }
        other => panic!("expected a user identity, got {other:?}"),
    }

    // Cleanup.
    sqlx::query("DELETE FROM user_sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .ok();
}

/// AR-01 review regression: a *valid* user JWT whose principal is rejected
/// (disabled account, or tenant drift) must fail fast on the WebSocket path.
/// A second credential presented in the same request — here another user's
/// valid session cookie — must not be honoured as a side channel: resolution
/// must not fall through to it.
#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS"]
async fn ws_identity_rejected_principal_fails_fast_despite_a_valid_session_cookie() {
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
    let foreign_tenant = Uuid::new_v4();
    let disabled_id = Uuid::new_v4();
    let drifted_id = Uuid::new_v4();
    let cookie_user_id = Uuid::new_v4();
    let cookie_session_id = Uuid::new_v4();
    let cookie_session_token = rustshare_auth::generate_web_session_token();
    let cookie_session_hash = rustshare_auth::hash_web_session_token(&cookie_session_token);

    for (user_id, username, disabled) in [
        (
            disabled_id,
            format!("ar01_ws_disabled_{}", disabled_id.simple()),
            true,
        ),
        (
            drifted_id,
            format!("ar01_ws_drift_{}", drifted_id.simple()),
            false,
        ),
        (
            cookie_user_id,
            format!("ar01_ws_cookie_{}", cookie_user_id.simple()),
            false,
        ),
    ] {
        sqlx::query(
            "INSERT INTO users
                (id, username, email, password_hash, display_name, is_admin,
                 storage_quota, tenant_id, disabled_at)
             VALUES ($1, $2, $3, 'test-password-hash', $2, false, 10737418240, $4, $5)",
        )
        .bind(user_id)
        .bind(&username)
        .bind(format!("{username}@test.local"))
        .bind(tenant_id)
        .bind(if disabled {
            Some(chrono::Utc::now())
        } else {
            None
        })
        .execute(&pool)
        .await
        .expect("insert test user");
    }

    sqlx::query(
        "INSERT INTO user_sessions (id, user_id, session_token_hash, expires_at, tenant_id)
         VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4)",
    )
    .bind(cookie_session_id)
    .bind(cookie_user_id)
    .bind(&cookie_session_hash)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .expect("insert cookie user's session");

    let headers_with_cookie = |bearer: &str| {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {bearer}").parse().unwrap(),
        );
        headers.insert(
            axum::http::header::COOKIE,
            format!(
                "{}={cookie_session_token}",
                rustshare_auth::WEB_SESSION_COOKIE_NAME
            )
            .parse()
            .unwrap(),
        );
        headers
    };

    // Case 1: valid JWT for a *disabled* user + another user's live cookie.
    let disabled_user_token = create_auth_token(&state, disabled_id, tenant_id);
    let rejected = rustshare_server::handlers::ws_auth::resolve_ws_client_identity(
        &state,
        &headers_with_cookie(&disabled_user_token),
        &rustshare_server::handlers::ws_auth::WsAuthQuery { token: None },
    )
    .await;
    assert!(
        matches!(rejected, Err((StatusCode::FORBIDDEN, _))),
        "a disabled user's JWT must fail fast (403) and must not fall through \
         to another user's session cookie, got {:?}",
        rejected.map(|identity| format!("{identity:?}"))
    );

    // Case 2: valid JWT with a *foreign tenant claim* + the same live cookie.
    let drifted_user_token = state
        .jwt_manager
        .generate(drifted_id, "drift@test.local", foreign_tenant)
        .expect("mint foreign-claim JWT");
    let rejected = rustshare_server::handlers::ws_auth::resolve_ws_client_identity(
        &state,
        &headers_with_cookie(&drifted_user_token),
        &rustshare_server::handlers::ws_auth::WsAuthQuery { token: None },
    )
    .await;
    assert!(
        matches!(rejected, Err((StatusCode::UNAUTHORIZED, _))),
        "a tenant-drifted JWT must fail fast (401) and must not fall through \
         to another user's session cookie, got {:?}",
        rejected.map(|identity| format!("{identity:?}"))
    );

    // Control: the cookie alone still authenticates its own user, so the
    // rejections above are due to fail-fast and not to a broken cookie.
    let mut cookie_only = axum::http::HeaderMap::new();
    cookie_only.insert(
        axum::http::header::COOKIE,
        format!(
            "{}={cookie_session_token}",
            rustshare_auth::WEB_SESSION_COOKIE_NAME
        )
        .parse()
        .unwrap(),
    );
    let identity = rustshare_server::handlers::ws_auth::resolve_ws_client_identity(
        &state,
        &cookie_only,
        &rustshare_server::handlers::ws_auth::WsAuthQuery { token: None },
    )
    .await
    .expect("the cookie user's own session must still authenticate");
    match identity {
        rustshare_server::handlers::ws_auth::ClientIdentity::User { user_id, .. } => {
            assert_eq!(user_id, cookie_user_id)
        }
        other => panic!("expected the cookie user's identity, got {other:?}"),
    }

    // Cleanup.
    sqlx::query("DELETE FROM user_sessions WHERE id = $1")
        .bind(cookie_session_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2 OR id = $3")
        .bind(disabled_id)
        .bind(drifted_id)
        .bind(cookie_user_id)
        .execute(&pool)
        .await
        .ok();
}
