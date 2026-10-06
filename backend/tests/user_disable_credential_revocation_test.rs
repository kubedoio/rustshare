//! PostgreSQL regression for credential invalidation on account deactivation.
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
use support::calendar_harness::{assert_local_database, setup_test_env, SERIAL};
use tower::ServiceExt;
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

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
