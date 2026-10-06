//! PostgreSQL-backed regression for transactional admin user lifecycle audits.
//!
//! Run explicitly against a disposable local database and S3-compatible
//! service. Set `RUSTSHARE_TEST_DISPOSABLE_DB=1` and
//! `RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1`; the S3 endpoint must be loopback
//! and its bucket must start with `rustshare-test`:
//! `cargo test -p rustshare-server --test admin_user_disable_transaction_test -- --ignored --test-threads=1`

mod support;

use std::error::Error;

use axum::{body::Body, http::Request};
use futures_util::FutureExt;
use sha2::{Digest, Sha256};
use support::calendar_harness::{assert_local_database, setup_test_env, SERIAL};
use tower::ServiceExt;
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn is_disposable_test_database_name(database_name: &str) -> bool {
    database_name == "rustshare_test" || database_name.starts_with("rustshare_test_")
}

fn is_disposable_object_store_target(endpoint: &str, bucket: &str) -> bool {
    let Ok(endpoint) = url::Url::parse(endpoint) else {
        return false;
    };
    let local_endpoint = matches!(endpoint.host_str(), Some("localhost" | "127.0.0.1"));
    local_endpoint
        && matches!(endpoint.scheme(), "http" | "https")
        && endpoint.username().is_empty()
        && endpoint.password().is_none()
        && (bucket == "rustshare-test"
            || bucket.starts_with("rustshare-test-")
            || bucket.starts_with("rustshare-test_"))
}

fn assert_disposable_test_database(database_url: &str) {
    let parsed_url = url::Url::parse(database_url)
        .expect("DATABASE_URL must be a valid PostgreSQL URL for this test");
    let database_name = parsed_url.path().trim_start_matches('/');
    assert!(
        is_disposable_test_database_name(database_name),
        "refusing to run destructive regression setup unless the database name is `rustshare_test` or starts with `rustshare_test_`"
    );
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB").ok().as_deref(),
        Some("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_DB=1 only after confirming this specifically named database is disposable"
    );
}

fn assert_disposable_test_object_store() {
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE")
            .ok()
            .as_deref(),
        Some("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1 only after confirming the object store is disposable"
    );
    let endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .expect("an explicit local S3_ENDPOINT or RUSTFS_ENDPOINT is required");
    let bucket = std::env::var("S3_BUCKET")
        .or_else(|_| std::env::var("RUSTFS_BUCKET"))
        .expect("an explicit rustshare-test S3_BUCKET or RUSTFS_BUCKET is required");
    assert!(
        is_disposable_object_store_target(&endpoint, &bucket),
        "refusing object-store writes unless the endpoint is loopback and the bucket is rustshare-test[-_]*"
    );
}

async fn cleanup_statement(
    pool: &sqlx::PgPool,
    statement: &str,
    first_error: &mut Option<sqlx::Error>,
) {
    if let Err(error) = sqlx::query(statement).execute(pool).await {
        if first_error.is_none() {
            *first_error = Some(error);
        }
    }
}

struct AdminUserLifecycleAttempt {
    response_status: axum::http::StatusCode,
    disabled_at: Option<chrono::DateTime<chrono::Utc>>,
    session_row_exists: bool,
    device_token_revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    session_auth_status: axum::http::StatusCode,
    disabled_audit_actions: i64,
    audit_failure_status: axum::http::StatusCode,
    audit_failure_trigger_fired: bool,
    audit_failure_state_unchanged: bool,
    audit_failure_session_auth_status: axum::http::StatusCode,
    retry_status: axum::http::StatusCode,
    disabled_at_after_retry: Option<chrono::DateTime<chrono::Utc>>,
    session_row_exists_after_retry: bool,
    device_token_revoked_at_after_retry: Option<chrono::DateTime<chrono::Utc>>,
    session_auth_status_after_retry: axum::http::StatusCode,
    authored_file_exists_after_retry: bool,
    authored_file_bytes_match_after_retry: bool,
    disabled_audit_actions_after_retry: i64,
    enable_status: axum::http::StatusCode,
    enabled_at_after_enable: Option<chrono::DateTime<chrono::Utc>>,
    enabled_audit_actions: i64,
    session_auth_status_after_enable: axum::http::StatusCode,
    enable_failure_status: axum::http::StatusCode,
    enable_failure_trigger_fired: bool,
    enable_failure_state_unchanged: bool,
    role_failure_status: axum::http::StatusCode,
    role_failure_trigger_fired: bool,
    role_failure_state_unchanged: bool,
    role_failure_session_auth_status: axum::http::StatusCode,
    role_retry_status: axum::http::StatusCode,
    is_admin_after_role_retry: bool,
    role_audit_actions: i64,
    password_changed_after_role_retry: bool,
    password_audit_actions_after_role_retry: i64,
    role_retry_session_auth_status: axum::http::StatusCode,
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn user_lifecycle_audit_mutations_are_atomic_and_preserve_content() {
    let _serial = SERIAL.lock().await;

    let database_url = std::env::var("DATABASE_URL").expect(
        "set DATABASE_URL explicitly to a disposable local database; the harness fallback is not safe for this test"
    );
    assert_disposable_test_database(&database_url);
    assert!(
        std::env::var("RUSTSHARE_TEST_ALLOW_REMOTE_DB")
            .ok()
            .as_deref()
            != Some("1"),
        "this regression must not override the harness local-database guard"
    );
    assert_local_database();
    assert_disposable_test_object_store();

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let admin_id = Uuid::new_v4();
    let target_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let storage_key = format!("test/{file_id}");
    let authored_file_bytes = b"Pilot-authored note content survives offboarding.\n";
    let content_hash = Sha256::digest(authored_file_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let function_name = format!("rs_test_disable_fail_{}", target_id.simple());
    let trigger_name = format!("rs_test_disable_fail_{}", target_id.simple());
    let audit_function_name = format!("rs_test_disable_audit_fail_{}", target_id.simple());
    let audit_trigger_name = format!("rs_test_disable_audit_fail_{}", target_id.simple());
    let audit_sequence_name = format!("rs_test_disable_audit_seq_{}", target_id.simple());
    let web_session_token = rustshare_auth::generate_web_session_token();
    let web_session_hash = rustshare_auth::hash_web_session_token(&web_session_token);
    let device_token_hash = format!("admin-disable-test-{}", Uuid::new_v4());
    let role_session_id = Uuid::new_v4();
    let role_session_token = rustshare_auth::generate_web_session_token();
    let role_session_hash = rustshare_auth::hash_web_session_token(&role_session_token);
    let role_device_token_hash = format!("admin-role-test-{}", Uuid::new_v4());

    // Keep setup, request, and observations in a fallible block so cleanup is
    // still attempted before any assertion can fail the test.
    let attempt: TestResult<AdminUserLifecycleAttempt> = async {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at)
             VALUES ($1, $2, NOW(), NOW())",
        )
        .bind(tenant_id)
        .bind(format!("Admin disable transaction test {tenant_id}"))
        .execute(&pool)
        .await?;

        for (user_id, is_admin) in [(admin_id, true), (target_id, false)] {
            let username = format!("disable_user_{}", user_id.simple());
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
            .await?;
        }

        state
            .object_store
            .put(
                &storage_key,
                axum::body::Bytes::from_static(authored_file_bytes),
            )
            .await?;

        sqlx::query(
            "INSERT INTO files
                (id, name, path, size, mime_type, content_hash, storage_key,
                 owner_id, tenant_id)
             VALUES ($1, 'disable-regression.md', $2, $3, 'text/markdown',
                     $4, $5, $6, $7)",
        )
        .bind(file_id)
        .bind(format!("/disable-regression-{file_id}.md"))
        .bind(authored_file_bytes.len() as i64)
        .bind(&content_hash)
        .bind(&storage_key)
        .bind(target_id)
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO user_sessions (id, user_id, session_token_hash, expires_at, tenant_id)
             VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4)",
        )
        .bind(session_id)
        .bind(target_id)
        .bind(&web_session_hash)
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO device_tokens (id, user_id, token_hash, device_name)
             VALUES ($1, $2, $3, 'disable transaction regression')",
        )
        .bind(Uuid::new_v4())
        .bind(target_id)
        .bind(&device_token_hash)
        .execute(&pool)
        .await?;

        let create_function = format!(
            "CREATE FUNCTION {function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $trigger$
             BEGIN
                 IF OLD.user_id = TG_ARGV[0]::uuid
                    AND OLD.revoked_at IS NULL
                    AND NEW.revoked_at IS NOT NULL THEN
                     RAISE EXCEPTION 'injected device-token revocation failure';
                 END IF;
                 RETURN NEW;
             END;
             $trigger$"
        );
        sqlx::query(&create_function).execute(&pool).await?;

        let create_trigger = format!(
            "CREATE TRIGGER {trigger_name}
             BEFORE UPDATE OF revoked_at ON device_tokens
             FOR EACH ROW EXECUTE FUNCTION {function_name}('{}')",
            target_id
        );
        sqlx::query(&create_trigger).execute(&pool).await?;

        let app = rustshare_server::routes::admin_routes()
            .merge(rustshare_server::routes::user_routes())
            .with_state(state.clone());
        let admin_bearer =
            support::calendar_harness::create_auth_token(&state, admin_id, tenant_id);
        let disable_request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/admin/users/{target_id}/disable"))
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {admin_bearer}"),
            )
            .body(Body::empty())?;
        let response_status = app.clone().oneshot(disable_request).await?.status();

        let disabled_at = sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
            "SELECT disabled_at FROM users WHERE id = $1",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await?;
        let session_row_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1 FROM user_sessions
                WHERE id = $1 AND user_id = $2
                  AND session_token_hash = $3 AND expires_at > NOW()
            )",
        )
        .bind(session_id)
        .bind(target_id)
        .bind(&web_session_hash)
        .fetch_one(&pool)
        .await?;
        let device_token_revoked_at =
            sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
                "SELECT revoked_at FROM device_tokens WHERE user_id = $1 AND token_hash = $2",
            )
            .bind(target_id)
            .bind(&device_token_hash)
            .fetch_one(&pool)
            .await?;
        let disabled_audit_actions = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'user.disabled' AND target_id = $1",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await?;

        let session_request = Request::builder()
            .method("GET")
            .uri("/api/v1/me/sessions")
            .header(
                axum::http::header::COOKIE,
                format!(
                    "{}={web_session_token}",
                    rustshare_auth::WEB_SESSION_COOKIE_NAME
                ),
            )
            .body(Body::empty())?;
        let session_auth_status = app.clone().oneshot(session_request).await?.status();

        // Restore the failed dependency, then retry the same disable request.
        let mut trigger_cleanup_error = None;
        cleanup_statement(
            &pool,
            &format!("DROP TRIGGER IF EXISTS {trigger_name} ON device_tokens"),
            &mut trigger_cleanup_error,
        )
        .await;
        cleanup_statement(
            &pool,
            &format!("DROP FUNCTION IF EXISTS {function_name}()"),
            &mut trigger_cleanup_error,
        )
        .await;
        if let Some(error) = trigger_cleanup_error {
            return Err(Box::new(error) as Box<dyn Error + Send + Sync>);
        }

        let create_audit_function = format!(
            "CREATE FUNCTION {audit_function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $trigger$
             BEGIN
                 IF NEW.action_type = TG_ARGV[1]
                    AND NEW.target_id = TG_ARGV[0]::uuid THEN
                     PERFORM nextval(TG_ARGV[2]::regclass);
                     RAISE EXCEPTION 'injected admin-audit insert failure';
                 END IF;
                 RETURN NEW;
             END;
             $trigger$"
        );
        sqlx::query(&create_audit_function).execute(&pool).await?;
        sqlx::query(&format!("CREATE SEQUENCE {audit_sequence_name}"))
            .execute(&pool)
            .await?;
        let create_audit_trigger = format!(
             "CREATE TRIGGER {audit_trigger_name}
             BEFORE INSERT ON admin_actions
             FOR EACH ROW EXECUTE FUNCTION {audit_function_name}('{}', 'user.disabled', '{}')",
            target_id, audit_sequence_name
        );
        sqlx::query(&create_audit_trigger).execute(&pool).await?;

        let audit_failure_request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/admin/users/{target_id}/disable"))
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {admin_bearer}"),
            )
            .body(Body::empty())?;
        let audit_failure_status = app.clone().oneshot(audit_failure_request).await?.status();
        let audit_failure_trigger_fired =
            sqlx::query_scalar::<_, bool>(&format!("SELECT is_called FROM {audit_sequence_name}"))
                .fetch_one(&pool)
                .await?;
        let audit_failure_state_unchanged = sqlx::query_scalar::<_, bool>(
            "SELECT users.disabled_at IS NULL
                AND EXISTS (
                    SELECT 1 FROM user_sessions
                    WHERE id = $2 AND user_id = $1
                      AND session_token_hash = $3 AND expires_at > NOW()
                )
                AND EXISTS (
                    SELECT 1 FROM device_tokens
                    WHERE user_id = $1 AND token_hash = $4 AND revoked_at IS NULL
                )
                AND NOT EXISTS (
                    SELECT 1 FROM admin_actions
                    WHERE action_type = 'user.disabled' AND target_id = $1
                )
             FROM users WHERE id = $1",
        )
        .bind(target_id)
        .bind(session_id)
        .bind(&web_session_hash)
        .bind(&device_token_hash)
        .fetch_one(&pool)
        .await?;
        let audit_failure_session_auth_status = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/me/sessions")
                    .header(
                        axum::http::header::COOKIE,
                        format!(
                            "{}={web_session_token}",
                            rustshare_auth::WEB_SESSION_COOKIE_NAME
                        ),
                    )
                    .body(Body::empty())?,
            )
            .await?
            .status();

        let mut audit_cleanup_error = None;
        cleanup_statement(
            &pool,
            &format!("DROP TRIGGER IF EXISTS {audit_trigger_name} ON admin_actions"),
            &mut audit_cleanup_error,
        )
        .await;
        cleanup_statement(
            &pool,
            &format!("DROP FUNCTION IF EXISTS {audit_function_name}()"),
            &mut audit_cleanup_error,
        )
        .await;
        cleanup_statement(
            &pool,
            &format!("DROP SEQUENCE IF EXISTS {audit_sequence_name}"),
            &mut audit_cleanup_error,
        )
        .await;
        if let Some(error) = audit_cleanup_error {
            return Err(Box::new(error) as Box<dyn Error + Send + Sync>);
        }

        let retry_request = Request::builder()
            .method("POST")
            .uri(format!("/api/v1/admin/users/{target_id}/disable"))
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {admin_bearer}"),
            )
            .body(Body::empty())?;
        let retry_status = app.clone().oneshot(retry_request).await?.status();
        let disabled_at_after_retry =
            sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
                "SELECT disabled_at FROM users WHERE id = $1",
            )
            .bind(target_id)
            .fetch_one(&pool)
            .await?;
        let session_row_exists_after_retry = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM user_sessions WHERE id = $1 AND user_id = $2)",
        )
        .bind(session_id)
        .bind(target_id)
        .fetch_one(&pool)
        .await?;
        let device_token_revoked_at_after_retry =
            sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
                "SELECT revoked_at FROM device_tokens WHERE user_id = $1 AND token_hash = $2",
            )
            .bind(target_id)
            .bind(&device_token_hash)
            .fetch_one(&pool)
            .await?;
        let authored_file_exists_after_retry = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1 FROM files WHERE id = $1 AND owner_id = $2 AND tenant_id = $3
            )",
        )
        .bind(file_id)
        .bind(target_id)
        .bind(tenant_id)
        .fetch_one(&pool)
        .await?;
        let authored_file_bytes_match_after_retry =
            state.object_store.get(&storage_key).await?.as_ref() == authored_file_bytes;
        let disabled_audit_actions_after_retry = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'user.disabled' AND target_id = $1",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await?;
        let session_auth_status_after_retry = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/me/sessions")
                    .header(
                        axum::http::header::COOKIE,
                        format!(
                            "{}={web_session_token}",
                            rustshare_auth::WEB_SESSION_COOKIE_NAME
                        ),
                    )
                    .body(Body::empty())?,
            )
            .await?
            .status();
        let create_enable_audit_function = format!(
            "CREATE FUNCTION {audit_function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $trigger$
             BEGIN
                 IF NEW.action_type = TG_ARGV[1]
                    AND NEW.target_id = TG_ARGV[0]::uuid THEN
                     PERFORM nextval(TG_ARGV[2]::regclass);
                     RAISE EXCEPTION 'injected enable-audit insert failure';
                 END IF;
                 RETURN NEW;
             END;
             $trigger$"
        );
        sqlx::query(&create_enable_audit_function)
            .execute(&pool)
            .await?;
        sqlx::query(&format!("CREATE SEQUENCE {audit_sequence_name}"))
            .execute(&pool)
            .await?;
        let create_enable_audit_trigger = format!(
            "CREATE TRIGGER {audit_trigger_name}
             BEFORE INSERT ON admin_actions
             FOR EACH ROW EXECUTE FUNCTION {audit_function_name}('{}', 'user.enabled', '{}')",
            target_id, audit_sequence_name
        );
        sqlx::query(&create_enable_audit_trigger)
            .execute(&pool)
            .await?;
        let enable_failure_status = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/admin/users/{target_id}/enable"))
                    .header(
                        axum::http::header::AUTHORIZATION,
                        format!("Bearer {admin_bearer}"),
                    )
                    .body(Body::empty())?,
            )
            .await?
            .status();
        let enable_failure_trigger_fired = sqlx::query_scalar::<_, bool>(&format!(
            "SELECT is_called FROM {audit_sequence_name}"
        ))
        .fetch_one(&pool)
        .await?;
        let enable_failure_state_unchanged = sqlx::query_scalar::<_, bool>(
            "SELECT users.disabled_at IS NOT NULL
                AND NOT EXISTS (
                    SELECT 1 FROM admin_actions
                    WHERE action_type = 'user.enabled' AND target_id = $1
                )
             FROM users WHERE id = $1",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await?;
        let mut enable_audit_cleanup_error = None;
        cleanup_statement(
            &pool,
            &format!("DROP TRIGGER IF EXISTS {audit_trigger_name} ON admin_actions"),
            &mut enable_audit_cleanup_error,
        )
        .await;
        cleanup_statement(
            &pool,
            &format!("DROP FUNCTION IF EXISTS {audit_function_name}()"),
            &mut enable_audit_cleanup_error,
        )
        .await;
        cleanup_statement(
            &pool,
            &format!("DROP SEQUENCE IF EXISTS {audit_sequence_name}"),
            &mut enable_audit_cleanup_error,
        )
        .await;
        if let Some(error) = enable_audit_cleanup_error {
            return Err(Box::new(error) as Box<dyn Error + Send + Sync>);
        }

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
                    .body(Body::empty())?,
            )
            .await?
            .status();
        let enabled_at_after_enable =
            sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
                "SELECT disabled_at FROM users WHERE id = $1",
            )
            .bind(target_id)
            .fetch_one(&pool)
            .await?;
        let enabled_audit_actions = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'user.enabled' AND target_id = $1",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await?;
        let session_auth_status_after_enable = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/me/sessions")
                    .header(
                        axum::http::header::COOKIE,
                        format!(
                            "{}={web_session_token}",
                            rustshare_auth::WEB_SESSION_COOKIE_NAME
                        ),
                    )
                    .body(Body::empty())?,
            )
            .await?
            .status();

        sqlx::query(
            "INSERT INTO user_sessions (id, user_id, session_token_hash, expires_at, tenant_id)
             VALUES ($1, $2, $3, NOW() + INTERVAL '1 hour', $4)",
        )
        .bind(role_session_id)
        .bind(target_id)
        .bind(&role_session_hash)
        .bind(tenant_id)
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO device_tokens (id, user_id, token_hash, device_name)
             VALUES ($1, $2, $3, 'admin role audit regression')",
        )
        .bind(Uuid::new_v4())
        .bind(target_id)
        .bind(&role_device_token_hash)
        .execute(&pool)
        .await?;

        let create_role_audit_function = format!(
            "CREATE FUNCTION {audit_function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $trigger$
             BEGIN
                 IF NEW.action_type = TG_ARGV[1]
                    AND NEW.target_id = TG_ARGV[0]::uuid THEN
                     PERFORM nextval(TG_ARGV[2]::regclass);
                     RAISE EXCEPTION 'injected role-audit insert failure';
                 END IF;
                 RETURN NEW;
             END;
             $trigger$"
        );
        sqlx::query(&create_role_audit_function)
            .execute(&pool)
            .await?;
        sqlx::query(&format!("CREATE SEQUENCE {audit_sequence_name}"))
            .execute(&pool)
            .await?;
        let create_role_audit_trigger = format!(
            "CREATE TRIGGER {audit_trigger_name}
             BEFORE INSERT ON admin_actions
             FOR EACH ROW EXECUTE FUNCTION {audit_function_name}('{}', 'user.admin_status_changed', '{}')",
            target_id, audit_sequence_name
        );
        sqlx::query(&create_role_audit_trigger)
            .execute(&pool)
            .await?;

        let role_failure_status = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/admin/users/{target_id}"))
                    .header(
                        axum::http::header::AUTHORIZATION,
                        format!("Bearer {admin_bearer}"),
                    )
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"is_admin":true,"password":"new-pilot-password"}"#,
                    ))?,
            )
            .await?
            .status();
        let role_failure_trigger_fired = sqlx::query_scalar::<_, bool>(&format!(
            "SELECT is_called FROM {audit_sequence_name}"
        ))
        .fetch_one(&pool)
        .await?;
        let role_failure_state_unchanged = sqlx::query_scalar::<_, bool>(
            "SELECT users.is_admin IS FALSE
                AND users.password_hash = 'test-password-hash'
                AND EXISTS (
                    SELECT 1 FROM user_sessions
                    WHERE id = $2 AND user_id = $1 AND session_token_hash = $3
                )
                AND EXISTS (
                    SELECT 1 FROM device_tokens
                    WHERE user_id = $1 AND token_hash = $4 AND revoked_at IS NULL
                )
                AND NOT EXISTS (
                    SELECT 1 FROM admin_actions
                    WHERE action_type = 'user.admin_status_changed' AND target_id = $1
                )
                AND NOT EXISTS (
                    SELECT 1 FROM admin_actions
                    WHERE action_type = 'user.password_changed' AND target_id = $1
                )
             FROM users WHERE id = $1",
        )
        .bind(target_id)
        .bind(role_session_id)
        .bind(&role_session_hash)
        .bind(&role_device_token_hash)
        .fetch_one(&pool)
        .await?;
        let role_failure_session_auth_status = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/me/sessions")
                    .header(
                        axum::http::header::COOKIE,
                        format!(
                            "{}={role_session_token}",
                            rustshare_auth::WEB_SESSION_COOKIE_NAME
                        ),
                    )
                    .body(Body::empty())?,
            )
            .await?
            .status();

        let mut role_audit_cleanup_error = None;
        cleanup_statement(
            &pool,
            &format!("DROP TRIGGER IF EXISTS {audit_trigger_name} ON admin_actions"),
            &mut role_audit_cleanup_error,
        )
        .await;
        cleanup_statement(
            &pool,
            &format!("DROP FUNCTION IF EXISTS {audit_function_name}()"),
            &mut role_audit_cleanup_error,
        )
        .await;
        cleanup_statement(
            &pool,
            &format!("DROP SEQUENCE IF EXISTS {audit_sequence_name}"),
            &mut role_audit_cleanup_error,
        )
        .await;
        if let Some(error) = role_audit_cleanup_error {
            return Err(Box::new(error) as Box<dyn Error + Send + Sync>);
        }

        let role_retry_status = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/v1/admin/users/{target_id}"))
                    .header(
                        axum::http::header::AUTHORIZATION,
                        format!("Bearer {admin_bearer}"),
                    )
                    .header(axum::http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"is_admin":true,"password":"new-pilot-password"}"#,
                    ))?,
            )
            .await?
            .status();
        let is_admin_after_role_retry = sqlx::query_scalar::<_, bool>(
            "SELECT is_admin FROM users WHERE id = $1",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await?;
        let role_audit_actions = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'user.admin_status_changed' AND target_id = $1
               AND detail->>'old_is_admin' = 'false'
               AND detail->>'new_is_admin' = 'true'",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await?;
        let password_changed_after_role_retry = sqlx::query_scalar::<_, bool>(
            "SELECT users.password_hash <> 'test-password-hash'
                AND NOT EXISTS (
                    SELECT 1 FROM user_sessions WHERE id = $2 AND user_id = $1
                )
                AND EXISTS (
                    SELECT 1 FROM device_tokens
                    WHERE user_id = $1 AND token_hash = $3 AND revoked_at IS NOT NULL
                )
                AND NOT EXISTS (
                    SELECT 1 FROM admin_actions
                    WHERE action_type = 'user.password_changed'
                      AND target_id = $1 AND detail::text LIKE '%new-pilot-password%'
                )
             FROM users WHERE id = $1",
        )
        .bind(target_id)
        .bind(role_session_id)
        .bind(&role_device_token_hash)
        .fetch_one(&pool)
        .await?;
        let password_audit_actions_after_role_retry = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'user.password_changed' AND target_id = $1",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await?;
        let role_retry_session_auth_status = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/me/sessions")
                    .header(
                        axum::http::header::COOKIE,
                        format!(
                            "{}={role_session_token}",
                            rustshare_auth::WEB_SESSION_COOKIE_NAME
                        ),
                    )
                    .body(Body::empty())?,
            )
            .await?
            .status();

        Ok(AdminUserLifecycleAttempt {
            response_status,
            disabled_at,
            session_row_exists,
            device_token_revoked_at,
            session_auth_status,
            disabled_audit_actions,
            audit_failure_status,
            audit_failure_trigger_fired,
            audit_failure_state_unchanged,
            audit_failure_session_auth_status,
            retry_status,
            disabled_at_after_retry,
            session_row_exists_after_retry,
            device_token_revoked_at_after_retry,
            session_auth_status_after_retry,
            authored_file_exists_after_retry,
            authored_file_bytes_match_after_retry,
            disabled_audit_actions_after_retry,
            enable_status,
            enabled_at_after_enable,
            enabled_audit_actions,
            session_auth_status_after_enable,
            enable_failure_status,
            enable_failure_trigger_fired,
            enable_failure_state_unchanged,
            role_failure_status,
            role_failure_trigger_fired,
            role_failure_state_unchanged,
            role_failure_session_auth_status,
            role_retry_status,
            is_admin_after_role_retry,
            role_audit_actions,
            password_changed_after_role_retry,
            password_audit_actions_after_role_retry,
            role_retry_session_auth_status,
        })
    }
    .await;

    let object_cleanup_result = state.object_store.delete(&storage_key).await;
    let mut cleanup_error = None;
    cleanup_statement(
        &pool,
        &format!("DROP TRIGGER IF EXISTS {trigger_name} ON device_tokens"),
        &mut cleanup_error,
    )
    .await;
    cleanup_statement(
        &pool,
        &format!("DROP FUNCTION IF EXISTS {function_name}()"),
        &mut cleanup_error,
    )
    .await;
    cleanup_statement(
        &pool,
        &format!("DROP TRIGGER IF EXISTS {audit_trigger_name} ON admin_actions"),
        &mut cleanup_error,
    )
    .await;
    cleanup_statement(
        &pool,
        &format!("DROP FUNCTION IF EXISTS {audit_function_name}()"),
        &mut cleanup_error,
    )
    .await;
    cleanup_statement(
        &pool,
        &format!("DROP SEQUENCE IF EXISTS {audit_sequence_name}"),
        &mut cleanup_error,
    )
    .await;
    for statement in [
        format!(
            "DELETE FROM admin_actions WHERE actor_id = '{admin_id}' OR target_id = '{target_id}'"
        ),
        format!("DELETE FROM users WHERE id IN ('{admin_id}', '{target_id}')"),
        format!("DELETE FROM tenants WHERE id = '{tenant_id}'"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }
    let cleanup_result = cleanup_error.map_or(Ok(()), Err);

    cleanup_result.expect("clean up trigger, function, and isolated fixtures");
    object_cleanup_result.expect("clean up the isolated authored-file object");
    let attempt = attempt.expect("complete disablement failure-injection scenario");

    assert_eq!(
        attempt.response_status,
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "the targeted device-token trigger must fail the disable route"
    );
    assert!(
        attempt.disabled_at.is_none(),
        "the user update must roll back when device-token revocation fails"
    );
    assert!(
        attempt.session_row_exists,
        "the original unexpired web-session row and token hash must survive rollback"
    );
    assert!(
        attempt.device_token_revoked_at.is_none(),
        "the device token must remain unrevoked after rollback"
    );
    assert_eq!(
        attempt.session_auth_status,
        axum::http::StatusCode::OK,
        "the original web-session cookie must still authenticate"
    );
    assert_eq!(
        attempt.disabled_audit_actions, 0,
        "a failed disable request must not write user.disabled audit action"
    );
    assert_eq!(
        attempt.audit_failure_status,
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "an audit persistence failure must fail the disable request"
    );
    assert!(
        attempt.audit_failure_trigger_fired,
        "the injected admin-audit failure trigger must have executed"
    );
    assert!(
        attempt.audit_failure_state_unchanged,
        "an audit persistence failure must roll back the user, session, token, and audit changes"
    );
    assert_eq!(
        attempt.audit_failure_session_auth_status,
        axum::http::StatusCode::OK,
        "the original session must still authenticate after an audit persistence failure"
    );
    assert_eq!(
        attempt.retry_status,
        axum::http::StatusCode::NO_CONTENT,
        "disable should succeed after the token-revocation dependency recovers"
    );
    assert!(
        attempt.disabled_at_after_retry.is_some(),
        "a successful retry must disable the target user"
    );
    assert!(
        !attempt.session_row_exists_after_retry,
        "a successful retry must delete the target's web session"
    );
    assert!(
        attempt.device_token_revoked_at_after_retry.is_some(),
        "a successful retry must revoke the target's device token"
    );
    assert_eq!(
        attempt.session_auth_status_after_retry,
        axum::http::StatusCode::UNAUTHORIZED,
        "the previously valid web-session cookie must stop authenticating"
    );
    assert!(
        attempt.authored_file_exists_after_retry,
        "offboarding must preserve the target's authored Markdown file"
    );
    assert!(
        attempt.authored_file_bytes_match_after_retry,
        "offboarding must preserve the target's authored Markdown object bytes"
    );
    assert_eq!(
        attempt.disabled_audit_actions_after_retry, 1,
        "a successful retry must write one user.disabled audit action"
    );
    assert_eq!(
        attempt.enable_status,
        axum::http::StatusCode::NO_CONTENT,
        "an administrator must be able to re-enable the user"
    );
    assert_eq!(
        attempt.enable_failure_status,
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "an enable-audit insert failure must fail the enable request"
    );
    assert!(
        attempt.enable_failure_trigger_fired,
        "the injected enable-audit trigger must have executed"
    );
    assert!(
        attempt.enable_failure_state_unchanged,
        "an enable-audit failure must leave the user disabled and create no success event"
    );
    assert!(
        attempt.enabled_at_after_enable.is_none(),
        "a successful enable must clear disabled_at"
    );
    assert_eq!(
        attempt.enabled_audit_actions, 1,
        "a successful enable must write one user.enabled audit action"
    );
    assert_eq!(
        attempt.session_auth_status_after_enable,
        axum::http::StatusCode::UNAUTHORIZED,
        "re-enabling the user must not restore a revoked session"
    );
    assert_eq!(
        attempt.role_failure_status,
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "a failed role-audit insert must fail the role update"
    );
    assert!(
        attempt.role_failure_trigger_fired,
        "the injected role-audit trigger must have executed"
    );
    assert!(
        attempt.role_failure_state_unchanged,
        "an audit failure must roll back the global admin flag, password, session, token, and both audit events"
    );
    assert_eq!(
        attempt.role_failure_session_auth_status,
        axum::http::StatusCode::OK,
        "the preexisting session must authenticate after the failed user update"
    );
    assert_eq!(
        attempt.role_retry_status,
        axum::http::StatusCode::OK,
        "the role change must succeed after audit storage recovers"
    );
    assert!(
        attempt.is_admin_after_role_retry,
        "the successful retry must apply the existing global admin flag"
    );
    assert_eq!(
        attempt.role_audit_actions, 1,
        "the successful role change must record old and new admin status once"
    );
    assert!(
        attempt.password_changed_after_role_retry,
        "the successful retry must change the password, revoke sessions/tokens, and exclude the password from audit details"
    );
    assert_eq!(
        attempt.password_audit_actions_after_role_retry, 1,
        "the successful password update must write one audit action"
    );
    assert_eq!(
        attempt.role_retry_session_auth_status,
        axum::http::StatusCode::UNAUTHORIZED,
        "the successful password update must invalidate the fresh session"
    );
}

#[test]
fn disposable_database_guard_rejects_ordinary_database_names() {
    assert!(is_disposable_test_database_name("rustshare_test"));
    assert!(is_disposable_test_database_name("rustshare_test_issue_331"));
    assert!(!is_disposable_test_database_name("rustshare"));
    assert!(!is_disposable_test_database_name("rustshare_backup"));
}

#[test]
fn disposable_object_store_guard_requires_loopback_and_test_bucket() {
    assert!(is_disposable_object_store_target(
        "http://127.0.0.1:19000",
        "rustshare-test-offboarding"
    ));
    assert!(is_disposable_object_store_target(
        "http://localhost:9000",
        "rustshare-test"
    ));
    assert!(!is_disposable_object_store_target(
        "https://objects.example.com",
        "rustshare-test-offboarding"
    ));
    assert!(!is_disposable_object_store_target(
        "http://127.0.0.1:19000",
        "rustshare"
    ));
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS services"]
async fn group_membership_changes_and_audits_commit_or_roll_back_together() -> TestResult<()> {
    let _serial = SERIAL.lock().await;

    let database_url = std::env::var("DATABASE_URL").expect(
        "set DATABASE_URL explicitly to a disposable local database; the harness fallback is not safe for this test",
    );
    assert_disposable_test_database(&database_url);
    assert!(
        std::env::var("RUSTSHARE_TEST_ALLOW_REMOTE_DB")
            .ok()
            .as_deref()
            != Some("1"),
        "this regression must not override the harness local-database guard"
    );
    assert_local_database();
    assert_disposable_test_object_store();

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    let group_id = Uuid::new_v4();
    let suffix = group_id.simple();
    let function_name = format!("rs_group_audit_fail_{suffix}");
    let trigger_name = format!("rs_group_audit_fail_{suffix}");
    let sequence_name = format!("rs_group_audit_seq_{suffix}");

    let attempt: TestResult<()> = async {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at)
             VALUES ($1, $2, NOW(), NOW())",
        )
        .bind(tenant_id)
        .bind(format!("Group audit transaction test {tenant_id}"))
        .execute(&pool)
        .await?;

        for (user_id, is_admin) in [(actor_id, true), (member_id, false)] {
            let username = format!("group_audit_{}", user_id.simple());
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
            .await?;
        }

        sqlx::query(
            "INSERT INTO user_groups (id, name, created_by, tenant_id)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(group_id)
        .bind(format!("Group audit {suffix}"))
        .bind(actor_id)
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(&format!("CREATE SEQUENCE {sequence_name}"))
            .execute(&pool)
            .await?;
        sqlx::query(&format!(
            "CREATE FUNCTION {function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $trigger$
             BEGIN
                 IF NEW.target_id = TG_ARGV[0]::uuid
                    AND NEW.action_type IN ('group.member_added', 'group.member_removed') THEN
                     PERFORM nextval(TG_ARGV[1]::regclass);
                     RAISE EXCEPTION 'injected group audit insert failure';
                 END IF;
                 RETURN NEW;
             END;
             $trigger$"
        ))
        .execute(&pool)
        .await?;
        let create_trigger = format!(
            "CREATE TRIGGER {trigger_name} BEFORE INSERT ON admin_actions
             FOR EACH ROW EXECUTE FUNCTION {function_name}('{group_id}', '{sequence_name}')"
        );
        sqlx::query(&create_trigger).execute(&pool).await?;

        let app = rustshare_server::routes::admin_routes().with_state(state.clone());
        let bearer = support::calendar_harness::create_auth_token(&state, actor_id, tenant_id);
        let add_request = || {
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/groups/{group_id}/members"))
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {bearer}"),
                )
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"user_id":"{member_id}"}}"#)))
                .expect("build group member add request")
        };

        let failed_add_status = app.clone().oneshot(add_request()).await?.status();
        assert_eq!(
            failed_add_status,
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let add_rolled_back: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS (
                 SELECT 1 FROM group_members WHERE group_id = $1 AND user_id = $2
             ) AND NOT EXISTS (
                 SELECT 1 FROM admin_actions
                 WHERE action_type = 'group.member_added' AND target_id = $1
             )",
        )
        .bind(group_id)
        .bind(member_id)
        .fetch_one(&pool)
        .await?;
        assert!(
            add_rolled_back,
            "failed add must roll back membership and audit"
        );
        let first_failure_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        assert_eq!(first_failure_marker, 1, "add audit trigger must have fired");

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let added_status = app.clone().oneshot(add_request()).await?.status();
        assert_eq!(added_status, axum::http::StatusCode::NO_CONTENT);
        let add_audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'group.member_added' AND target_id = $1
               AND actor_id = $2 AND detail->>'user_id' = $3",
        )
        .bind(group_id)
        .bind(actor_id)
        .bind(member_id.to_string())
        .fetch_one(&pool)
        .await?;
        assert_eq!(
            add_audit_count, 1,
            "successful add must have one audit event"
        );

        sqlx::query(&create_trigger).execute(&pool).await?;
        let remove_request = Request::builder()
            .method("DELETE")
            .uri(format!(
                "/api/v1/admin/groups/{group_id}/members/{member_id}"
            ))
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {bearer}"),
            )
            .body(Body::empty())?;
        let failed_remove_status = app.clone().oneshot(remove_request).await?.status();
        assert_eq!(
            failed_remove_status,
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let remove_rolled_back: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM group_members WHERE group_id = $1 AND user_id = $2
             ) AND NOT EXISTS (
                 SELECT 1 FROM admin_actions
                 WHERE action_type = 'group.member_removed' AND target_id = $1
             )",
        )
        .bind(group_id)
        .bind(member_id)
        .fetch_one(&pool)
        .await?;
        assert!(
            remove_rolled_back,
            "failed remove must preserve membership and roll back its audit"
        );
        let second_failure_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        assert_eq!(
            second_failure_marker, 2,
            "remove audit trigger must have fired"
        );

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let remove_request = Request::builder()
            .method("DELETE")
            .uri(format!(
                "/api/v1/admin/groups/{group_id}/members/{member_id}"
            ))
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {bearer}"),
            )
            .body(Body::empty())?;
        let removed_status = app.oneshot(remove_request).await?.status();
        assert_eq!(removed_status, axum::http::StatusCode::NO_CONTENT);
        let remove_audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'group.member_removed' AND target_id = $1
               AND actor_id = $2 AND detail->>'user_id' = $3",
        )
        .bind(group_id)
        .bind(actor_id)
        .bind(member_id.to_string())
        .fetch_one(&pool)
        .await?;
        assert_eq!(
            remove_audit_count, 1,
            "successful remove must have one audit event"
        );

        Ok(())
    }
    .await;

    let mut cleanup_error = None;
    for statement in [
        format!("DROP TRIGGER IF EXISTS {trigger_name} ON admin_actions"),
        format!("DROP FUNCTION IF EXISTS {function_name}()"),
        format!("DROP SEQUENCE IF EXISTS {sequence_name}"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }
    for statement in [
        format!(
            "DELETE FROM admin_actions WHERE target_id = '{group_id}' OR actor_id = '{actor_id}'"
        ),
        format!("DELETE FROM user_groups WHERE id = '{group_id}'"),
        format!("DELETE FROM users WHERE id IN ('{actor_id}', '{member_id}')"),
        format!("DELETE FROM tenants WHERE id = '{tenant_id}'"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }

    attempt?;
    if let Some(error) = cleanup_error {
        return Err(Box::new(error) as Box<dyn Error + Send + Sync>);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS services"]
async fn group_lifecycle_changes_and_audits_commit_or_roll_back_together() -> TestResult<()> {
    let _serial = SERIAL.lock().await;

    let database_url = std::env::var("DATABASE_URL").expect(
        "set DATABASE_URL explicitly to a disposable local database; the harness fallback is not safe for this test",
    );
    assert_disposable_test_database(&database_url);
    assert!(
        std::env::var("RUSTSHARE_TEST_ALLOW_REMOTE_DB")
            .ok()
            .as_deref()
            != Some("1"),
        "this regression must not override the harness local-database guard"
    );
    assert_local_database();
    assert_disposable_test_object_store();

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let group_suffix = Uuid::new_v4().simple().to_string();
    let original_name = format!("group lifecycle {group_suffix}");
    let updated_name = format!("group lifecycle updated {group_suffix}");
    let username = format!("group_lifecycle_{}", actor_id.simple());
    let function_name = format!("rs_group_lifecycle_fail_{group_suffix}");
    let trigger_name = format!("rs_group_lifecycle_fail_{group_suffix}");
    let sequence_name = format!("rs_group_lifecycle_seq_{group_suffix}");
    let mut group_id = None;

    let attempt: TestResult<()> = async {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at)
             VALUES ($1, $2, NOW(), NOW())",
        )
        .bind(tenant_id)
        .bind(format!("Group lifecycle transaction test {tenant_id}"))
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO users
                (id, username, email, password_hash, display_name, is_admin,
                 storage_quota, tenant_id)
             VALUES ($1, $2, $3, 'test-password-hash', $2, true, 10737418240, $4)",
        )
        .bind(actor_id)
        .bind(&username)
        .bind(format!("{username}@test.local"))
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(&format!("CREATE SEQUENCE {sequence_name}"))
            .execute(&pool)
            .await?;
        sqlx::query(&format!(
            "CREATE FUNCTION {function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $trigger$
             BEGIN
                 IF NEW.actor_id = TG_ARGV[0]::uuid
                    AND NEW.action_type = TG_ARGV[1]
                    AND (TG_ARGV[2] = '*' OR NEW.target_id = TG_ARGV[2]::uuid) THEN
                     PERFORM nextval(TG_ARGV[3]::regclass);
                     RAISE EXCEPTION 'injected group lifecycle audit failure';
                 END IF;
                 RETURN NEW;
             END;
             $trigger$"
        ))
        .execute(&pool)
        .await?;

        let app = rustshare_server::routes::admin_routes().with_state(state.clone());
        let bearer = support::calendar_harness::create_auth_token(&state, actor_id, tenant_id);

        let arm_audit_failure = |action: &str, target: &str| {
            format!(
                "CREATE TRIGGER {trigger_name} BEFORE INSERT ON admin_actions
                 FOR EACH ROW EXECUTE FUNCTION {function_name}('{actor_id}', '{action}', '{target}', '{sequence_name}')"
            )
        };
        let create_request = || {
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/groups")
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {bearer}"),
                )
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"name":"{original_name}"}}"#)))
                .expect("build group create request")
        };

        sqlx::query(&arm_audit_failure("group.created", "*"))
            .execute(&pool)
            .await?;
        let failed_create_status = app.clone().oneshot(create_request()).await?.status();
        assert_eq!(
            failed_create_status,
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let create_rolled_back: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS (
                 SELECT 1 FROM user_groups WHERE name = $1 AND created_by = $2
             ) AND NOT EXISTS (
                 SELECT 1 FROM admin_actions
                 WHERE action_type = 'group.created' AND actor_id = $2
             )",
        )
        .bind(&original_name)
        .bind(actor_id)
        .fetch_one(&pool)
        .await?;
        assert!(
            create_rolled_back,
            "failed create must roll back both group and audit"
        );
        let first_failure_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        assert_eq!(first_failure_marker, 1, "create audit trigger must have fired");

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let create_response = app.clone().oneshot(create_request()).await?;
        assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);
        let response_body = axum::body::to_bytes(create_response.into_body(), 1_000_000).await?;
        let created_group: serde_json::Value = serde_json::from_slice(&response_body)?;
        let created_id = created_group["id"]
            .as_str()
            .ok_or("create response omitted group id")?
            .parse::<Uuid>()?;
        group_id = Some(created_id);
        let create_audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'group.created' AND actor_id = $1 AND target_id = $2
               AND detail->>'name' = $3",
        )
        .bind(actor_id)
        .bind(created_id)
        .bind(&original_name)
        .fetch_one(&pool)
        .await?;
        assert_eq!(create_audit_count, 1, "successful create must have one audit event");

        sqlx::query(&arm_audit_failure("group.updated", &created_id.to_string()))
            .execute(&pool)
            .await?;
        let update_request = || {
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/admin/groups/{created_id}"))
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {bearer}"),
                )
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"name":"{updated_name}"}}"#)))
                .expect("build group update request")
        };
        let failed_update_status = app.clone().oneshot(update_request()).await?.status();
        assert_eq!(
            failed_update_status,
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let update_rolled_back: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM user_groups WHERE id = $1 AND name = $2
             ) AND NOT EXISTS (
                 SELECT 1 FROM admin_actions
                 WHERE action_type = 'group.updated' AND target_id = $1
             )",
        )
        .bind(created_id)
        .bind(&original_name)
        .fetch_one(&pool)
        .await?;
        assert!(
            update_rolled_back,
            "failed update must preserve group data and roll back its audit"
        );
        let second_failure_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        assert_eq!(second_failure_marker, 2, "update audit trigger must have fired");

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let updated_status = app.clone().oneshot(update_request()).await?.status();
        assert_eq!(updated_status, axum::http::StatusCode::OK);
        let update_audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'group.updated' AND actor_id = $1 AND target_id = $2
               AND detail->>'name' = $3",
        )
        .bind(actor_id)
        .bind(created_id)
        .bind(&updated_name)
        .fetch_one(&pool)
        .await?;
        assert_eq!(update_audit_count, 1, "successful update must have one audit event");

        sqlx::query(&arm_audit_failure("group.deleted", &created_id.to_string()))
            .execute(&pool)
            .await?;
        let remove_group_request = || {
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/admin/groups/{created_id}"))
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {bearer}"),
                )
                .body(Body::empty())
                .expect("build group delete request")
        };
        let failed_delete_status = app
            .clone()
            .oneshot(remove_group_request())
            .await?
            .status();
        assert_eq!(
            failed_delete_status,
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let delete_rolled_back: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM user_groups WHERE id = $1)
             AND NOT EXISTS (
                 SELECT 1 FROM admin_actions
                 WHERE action_type = 'group.deleted' AND target_id = $1
             )",
        )
        .bind(created_id)
        .fetch_one(&pool)
        .await?;
        assert!(
            delete_rolled_back,
            "failed delete must preserve group and roll back its audit"
        );
        let third_failure_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        assert_eq!(third_failure_marker, 3, "delete audit trigger must have fired");

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let deleted_status = app.oneshot(remove_group_request()).await?.status();
        assert_eq!(deleted_status, axum::http::StatusCode::NO_CONTENT);
        let delete_audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'group.deleted' AND actor_id = $1 AND target_id = $2
               AND detail->>'name' = $3",
        )
        .bind(actor_id)
        .bind(created_id)
        .bind(&updated_name)
        .fetch_one(&pool)
        .await?;
        assert_eq!(delete_audit_count, 1, "successful delete must have one audit event");

        Ok(())
    }
    .await;

    let mut cleanup_error = None;
    for statement in [
        format!("DROP TRIGGER IF EXISTS {trigger_name} ON admin_actions"),
        format!("DROP FUNCTION IF EXISTS {function_name}()"),
        format!("DROP SEQUENCE IF EXISTS {sequence_name}"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }
    for statement in [
        format!(
            "DELETE FROM admin_actions WHERE actor_id = '{actor_id}' OR target_id = '{}'",
            group_id.unwrap_or_default()
        ),
        format!(
            "DELETE FROM user_groups WHERE created_by = '{actor_id}' OR id = '{}'",
            group_id.unwrap_or_default()
        ),
        format!("DELETE FROM users WHERE id = '{actor_id}'"),
        format!("DELETE FROM tenants WHERE id = '{tenant_id}'"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }

    attempt?;
    if let Some(error) = cleanup_error {
        return Err(Box::new(error) as Box<dyn Error + Send + Sync>);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS services"]
async fn admin_user_create_and_delete_audits_are_transactional() -> TestResult<()> {
    let _serial = SERIAL.lock().await;

    let database_url = std::env::var("DATABASE_URL").expect(
        "set DATABASE_URL explicitly to a disposable local database; the harness fallback is not safe for this test",
    );
    assert_disposable_test_database(&database_url);
    assert!(
        std::env::var("RUSTSHARE_TEST_ALLOW_REMOTE_DB")
            .ok()
            .as_deref()
            != Some("1"),
        "this regression must not override the harness local-database guard"
    );
    assert_local_database();
    assert_disposable_test_object_store();

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let fixture_id = Uuid::new_v4();
    let suffix = fixture_id.simple().to_string();
    let username = format!("atomic_user_{suffix}");
    let email = format!("atomic_{suffix}@test.local");
    let password = "pilot-test-password-avoid-logging";
    let function_name = format!("rs_user_audit_fail_{suffix}");
    let trigger_name = format!("rs_user_audit_fail_{suffix}");
    let sequence_name = format!("rs_user_audit_seq_{suffix}");

    let attempt: TestResult<()> = async {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at)
             VALUES ($1, $2, NOW(), NOW())",
        )
        .bind(tenant_id)
        .bind(format!("Admin user audit transaction test {tenant_id}"))
        .execute(&pool)
        .await?;

        let admin_username = format!("admin_user_audit_{}", actor_id.simple());
        sqlx::query(
            "INSERT INTO users
                (id, username, email, password_hash, display_name, is_admin,
                 storage_quota, tenant_id)
             VALUES ($1, $2, $3, 'test-password-hash', $2, true, 10737418240, $4)",
        )
        .bind(actor_id)
        .bind(&admin_username)
        .bind(format!("{admin_username}@test.local"))
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(&format!("CREATE SEQUENCE {sequence_name}"))
            .execute(&pool)
            .await?;
        sqlx::query(&format!(
            "CREATE FUNCTION {function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $trigger$
             BEGIN
                 IF NEW.actor_id = TG_ARGV[0]::uuid
                    AND NEW.action_type = TG_ARGV[1]
                    AND (TG_ARGV[2] = '*' OR NEW.target_id = TG_ARGV[2]::uuid) THEN
                     PERFORM nextval(TG_ARGV[3]::regclass);
                     RAISE EXCEPTION 'injected user lifecycle audit failure';
                 END IF;
                 RETURN NEW;
             END;
             $trigger$"
        ))
        .execute(&pool)
        .await?;

        let app = rustshare_server::routes::admin_routes().with_state(state.clone());
        let bearer = support::calendar_harness::create_auth_token(&state, actor_id, tenant_id);
        let missing_user_id = Uuid::new_v4();
        let missing_user_request = |method: &str, path: String| {
            Request::builder()
                .method(method)
                .uri(path)
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {bearer}"),
                )
                .body(Body::empty())
                .expect("build missing-user lifecycle request")
        };
        for (method, path) in [
            (
                "POST",
                format!("/api/v1/admin/users/{missing_user_id}/disable"),
            ),
            (
                "POST",
                format!("/api/v1/admin/users/{missing_user_id}/enable"),
            ),
            (
                "DELETE",
                format!("/api/v1/admin/users/{missing_user_id}"),
            ),
        ] {
            let response = app
                .clone()
                .oneshot(missing_user_request(method, path))
                .await?;
            assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
        }
        let missing_user_audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_actions WHERE target_id = $1
             AND action_type IN ('user.disabled', 'user.enabled', 'user.deleted')",
        )
        .bind(missing_user_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(missing_user_audit_count, 0);

        let create_audit_trigger = format!(
            "CREATE TRIGGER {trigger_name} BEFORE INSERT ON admin_actions
             FOR EACH ROW EXECUTE FUNCTION {function_name}('{actor_id}', 'user.created', '*', '{sequence_name}')"
        );
        sqlx::query(&create_audit_trigger).execute(&pool).await?;

        let create_request = || {
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/users")
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {bearer}"),
                )
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(
                    r#"{{"username":"{username}","email":"{email}","password":"{password}","display_name":"Pilot Test User"}}"#
                )))
                .expect("build user create request")
        };
        let failed_create_status = app.clone().oneshot(create_request()).await?.status();
        assert_eq!(
            failed_create_status,
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let create_rolled_back: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS (
                 SELECT 1 FROM users WHERE username = $1 OR email = $2
             ) AND NOT EXISTS (
                 SELECT 1 FROM admin_actions
                 WHERE action_type = 'user.created' AND actor_id = $3
             )",
        )
        .bind(&username)
        .bind(&email)
        .bind(actor_id)
        .fetch_one(&pool)
        .await?;
        assert!(
            create_rolled_back,
            "failed create must roll back user and audit event"
        );
        let first_failure_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        assert_eq!(first_failure_marker, 1, "create audit trigger must have fired");

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let create_response = app.clone().oneshot(create_request()).await?;
        assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);
        let response_body = axum::body::to_bytes(create_response.into_body(), 1_000_000).await?;
        let created_user: serde_json::Value = serde_json::from_slice(&response_body)?;
        let created_user_id = created_user["id"]
            .as_str()
            .ok_or_else(|| std::io::Error::other("create response omitted user id"))?
            .parse::<Uuid>()?;
        let create_audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'user.created' AND actor_id = $1 AND target_id = $2
               AND detail->>'username' = $3 AND detail->>'password' IS NULL",
        )
        .bind(actor_id)
        .bind(created_user_id)
        .bind(&username)
        .fetch_one(&pool)
        .await?;
        assert_eq!(create_audit_count, 1, "successful create must have one safe audit event");

        let delete_audit_trigger = format!(
            "CREATE TRIGGER {trigger_name} BEFORE INSERT ON admin_actions
             FOR EACH ROW EXECUTE FUNCTION {function_name}('{actor_id}', 'user.deleted', '*', '{sequence_name}')"
        );
        sqlx::query(&delete_audit_trigger).execute(&pool).await?;
        let delete_request = || {
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/admin/users/{created_user_id}"))
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {bearer}"),
                )
                .body(Body::empty())
                .expect("build user delete request")
        };
        let failed_delete_status = app.clone().oneshot(delete_request()).await?.status();
        assert_eq!(
            failed_delete_status,
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let delete_rolled_back: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM users WHERE id = $1 AND username = $2
             ) AND NOT EXISTS (
                 SELECT 1 FROM admin_actions
                 WHERE action_type = 'user.deleted' AND target_id = $1
             )",
        )
        .bind(created_user_id)
        .bind(&username)
        .fetch_one(&pool)
        .await?;
        assert!(
            delete_rolled_back,
            "failed delete must preserve the user and roll back its audit event"
        );
        let second_failure_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        assert_eq!(second_failure_marker, 2, "delete audit trigger must have fired");

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let deleted_status = app.oneshot(delete_request()).await?.status();
        assert_eq!(deleted_status, axum::http::StatusCode::NO_CONTENT);
        let delete_audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_actions
             WHERE action_type = 'user.deleted' AND actor_id = $1 AND target_id = $2
               AND detail->>'storage_keys_count' = '0'",
        )
        .bind(actor_id)
        .bind(created_user_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(delete_audit_count, 1, "successful delete must have one audit event");

        Ok(())
    }
    .await;

    let mut cleanup_error = None;
    for statement in [
        format!("DROP TRIGGER IF EXISTS {trigger_name} ON admin_actions"),
        format!("DROP FUNCTION IF EXISTS {function_name}()"),
        format!("DROP SEQUENCE IF EXISTS {sequence_name}"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }
    for statement in [
        format!("DELETE FROM admin_actions WHERE actor_id = '{actor_id}'"),
        format!("DELETE FROM users WHERE id = '{actor_id}' OR username = '{username}'"),
        format!("DELETE FROM tenants WHERE id = '{tenant_id}'"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }

    attempt?;
    if let Some(error) = cleanup_error {
        return Err(Box::new(error) as Box<dyn Error + Send + Sync>);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS services"]
async fn admin_webhook_mutations_rollback_when_audit_insert_fails() -> TestResult<()> {
    let _serial = SERIAL.lock().await;

    let database_url = std::env::var("DATABASE_URL").expect(
        "set DATABASE_URL explicitly to a disposable local database; the harness fallback is not safe for this test",
    );
    assert_disposable_test_database(&database_url);
    assert_local_database();
    assert_disposable_test_object_store();

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let suffix = Uuid::new_v4().simple().to_string();
    let admin_username = format!("webhook_audit_admin_{suffix}");
    let webhook_name = format!("webhook_audit_{suffix}");
    let webhook_url = format!("https://example.test/hook?token={suffix}");
    let signing_secret = format!("signing-secret-{suffix}");
    let function_name = format!("rs_webhook_audit_fail_{suffix}");
    let trigger_name = format!("rs_webhook_audit_fail_{suffix}");
    let sequence_name = format!("rs_webhook_audit_seq_{suffix}");

    let attempt: TestResult<()> = async {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at)
             VALUES ($1, $2, NOW(), NOW())",
        )
        .bind(tenant_id)
        .bind(format!("Webhook audit transaction test {tenant_id}"))
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO users
                (id, username, email, password_hash, display_name, is_admin,
                 storage_quota, tenant_id)
             VALUES ($1, $2, $3, 'test-password-hash', $2, true, 10737418240, $4)",
        )
        .bind(actor_id)
        .bind(&admin_username)
        .bind(format!("{admin_username}@test.local"))
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(&format!("CREATE SEQUENCE {sequence_name}"))
            .execute(&pool)
            .await?;
        sqlx::query(&format!(
            "CREATE FUNCTION {function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $trigger$
             BEGIN
                 IF NEW.actor_id = TG_ARGV[0]::uuid
                    AND NEW.action_type = TG_ARGV[1] THEN
                     PERFORM nextval(TG_ARGV[2]::regclass);
                     RAISE EXCEPTION 'injected webhook audit insert failure';
                 END IF;
                 RETURN NEW;
             END;
             $trigger$"
        ))
        .execute(&pool)
        .await?;
        let create_trigger = |action: &str| {
            format!(
                "CREATE TRIGGER {trigger_name} BEFORE INSERT ON admin_actions
                 FOR EACH ROW EXECUTE FUNCTION {function_name}('{actor_id}', '{action}', '{sequence_name}')"
            )
        };

        let app = rustshare_server::routes::admin_routes().with_state(state.clone());
        let bearer = support::calendar_harness::create_auth_token(&state, actor_id, tenant_id);
        let request = |method, uri: String, body: String| {
            Request::builder()
                .method(method)
                .uri(uri)
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {bearer}"),
                )
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("build webhook admin request")
        };
        let endpoint = "/api/v1/admin/integrations/webhooks";

        sqlx::query(&create_trigger("webhook.created"))
            .execute(&pool)
            .await?;
        let create_body = serde_json::json!({
            "name": webhook_name,
            "url": webhook_url,
            "secret": signing_secret,
            "events": ["file.uploaded"]
        })
        .to_string();
        let failed_create = app
            .clone()
            .oneshot(request(
                axum::http::Method::POST,
                endpoint.to_string(),
                create_body.clone(),
            ))
            .await?;
        assert_eq!(
            failed_create.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let create_failure_marker: i64 = sqlx::query_scalar(&format!(
            "SELECT last_value FROM {sequence_name}"
        ))
        .fetch_one(&pool)
        .await?;
        assert_eq!(create_failure_marker, 1, "create audit failure trigger did not fire");
        let failed_create_rolled_back: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS (SELECT 1 FROM webhook_configs WHERE created_by = $1 AND name = $2)
               AND NOT EXISTS (SELECT 1 FROM admin_actions WHERE actor_id = $1 AND action_type = 'webhook.created')",
        )
        .bind(actor_id)
        .bind(&webhook_name)
        .fetch_one(&pool)
        .await?;
        assert!(failed_create_rolled_back, "failed create must roll back config and audit");

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let created = app
            .clone()
            .oneshot(request(
                axum::http::Method::POST,
                endpoint.to_string(),
                create_body,
            ))
            .await?;
        assert_eq!(created.status(), axum::http::StatusCode::CREATED);
        let created_body = axum::body::to_bytes(created.into_body(), usize::MAX).await?;
        let webhook_id = serde_json::from_slice::<serde_json::Value>(&created_body)?["id"]
            .as_str()
            .ok_or("create response did not include webhook id")?
            .parse::<Uuid>()?;
        let create_detail: String = sqlx::query_scalar(
            "SELECT detail::text FROM admin_actions
             WHERE actor_id = $1 AND action_type = 'webhook.created' AND target_id = $2",
        )
        .bind(actor_id)
        .bind(webhook_id)
        .fetch_one(&pool)
        .await?;
        assert!(!create_detail.contains(&webhook_url));
        assert!(!create_detail.contains(&signing_secret));

        sqlx::query(&create_trigger("webhook.updated"))
            .execute(&pool)
            .await?;
        let update_body = serde_json::json!({
            "name": format!("{webhook_name}_updated"),
            "url": format!("{webhook_url}&updated=true"),
            "secret": format!("{signing_secret}_updated")
        })
        .to_string();
        let failed_update = app
            .clone()
            .oneshot(request(
                axum::http::Method::PATCH,
                format!("{endpoint}/{webhook_id}"),
                update_body.clone(),
            ))
            .await?;
        assert_eq!(
            failed_update.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let update_failure_marker: i64 = sqlx::query_scalar(&format!(
            "SELECT last_value FROM {sequence_name}"
        ))
        .fetch_one(&pool)
        .await?;
        assert_eq!(update_failure_marker, 2, "update audit failure trigger did not fire");
        let failed_update_preserved_config: bool = sqlx::query_scalar(
            "SELECT name = $2 AND url = $3 FROM webhook_configs WHERE id = $1",
        )
        .bind(webhook_id)
        .bind(&webhook_name)
        .bind(&webhook_url)
        .fetch_one(&pool)
        .await?;
        assert!(failed_update_preserved_config, "failed update must roll back config");
        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let updated = app
            .clone()
            .oneshot(request(
                axum::http::Method::PATCH,
                format!("{endpoint}/{webhook_id}"),
                update_body,
            ))
            .await?;
        assert_eq!(updated.status(), axum::http::StatusCode::OK);
        let update_detail: String = sqlx::query_scalar(
            "SELECT detail::text FROM admin_actions
             WHERE actor_id = $1 AND action_type = 'webhook.updated' AND target_id = $2",
        )
        .bind(actor_id)
        .bind(webhook_id)
        .fetch_one(&pool)
        .await?;
        assert!(!update_detail.contains("updated=true"));
        assert!(!update_detail.contains(&signing_secret));

        sqlx::query(&create_trigger("webhook.deleted"))
            .execute(&pool)
            .await?;
        let failed_delete = app
            .clone()
            .oneshot(request(
                axum::http::Method::DELETE,
                format!("{endpoint}/{webhook_id}"),
                String::new(),
            ))
            .await?;
        assert_eq!(
            failed_delete.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let delete_failure_marker: i64 = sqlx::query_scalar(&format!(
            "SELECT last_value FROM {sequence_name}"
        ))
        .fetch_one(&pool)
        .await?;
        assert_eq!(delete_failure_marker, 3, "delete audit failure trigger did not fire");
        let failed_delete_preserved_config: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM webhook_configs WHERE id = $1)
               AND NOT EXISTS (SELECT 1 FROM admin_actions
                               WHERE actor_id = $2 AND action_type = 'webhook.deleted' AND target_id = $1)",
        )
        .bind(webhook_id)
        .bind(actor_id)
        .fetch_one(&pool)
        .await?;
        assert!(failed_delete_preserved_config, "failed delete must roll back config and audit");
        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let deleted = app
            .oneshot(request(
                axum::http::Method::DELETE,
                format!("{endpoint}/{webhook_id}"),
                String::new(),
            ))
            .await?;
        assert_eq!(deleted.status(), axum::http::StatusCode::NO_CONTENT);
        let event_counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT
                count(*) FILTER (WHERE action_type = 'webhook.created'),
                count(*) FILTER (WHERE action_type = 'webhook.updated'),
                count(*) FILTER (WHERE action_type = 'webhook.deleted')
             FROM admin_actions WHERE actor_id = $1 AND target_id = $2",
        )
        .bind(actor_id)
        .bind(webhook_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(event_counts, (1, 1, 1));

        Ok(())
    }
    .await;

    let mut cleanup_error = None;
    for statement in [
        format!("DROP TRIGGER IF EXISTS {trigger_name} ON admin_actions"),
        format!("DROP FUNCTION IF EXISTS {function_name}()"),
        format!("DROP SEQUENCE IF EXISTS {sequence_name}"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }
    for statement in [
        format!("DELETE FROM admin_actions WHERE actor_id = '{actor_id}'"),
        format!("DELETE FROM webhook_configs WHERE created_by = '{actor_id}'"),
        format!("DELETE FROM users WHERE id = '{actor_id}'"),
        format!("DELETE FROM tenants WHERE id = '{tenant_id}'"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }

    attempt?;
    if let Some(error) = cleanup_error {
        return Err(Box::new(error) as Box<dyn Error + Send + Sync>);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS services"]
async fn admin_workflow_mutations_rollback_when_audit_insert_fails() -> TestResult<()> {
    let _serial = SERIAL.lock().await;

    let database_url = std::env::var("DATABASE_URL").expect(
        "set DATABASE_URL explicitly to a disposable local database; the harness fallback is not safe for this test",
    );
    assert_disposable_test_database(&database_url);
    assert_local_database();
    assert_disposable_test_object_store();

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let workflow_id = Uuid::new_v4();
    let suffix = workflow_id.simple().to_string();
    let workflow_key = "invite_email";
    let original_body = "original workflow body";
    let updated_body = format!("updated private body {suffix}");
    let admin_username = format!("workflow_audit_admin_{suffix}");
    let function_name = format!("rs_workflow_audit_fail_{suffix}");
    let trigger_name = format!("rs_workflow_audit_fail_{suffix}");
    let sequence_name = format!("rs_workflow_audit_seq_{suffix}");
    let original_smtp_config: (bool, Option<String>, Option<i32>, Option<String>) = sqlx::query_as(
        "SELECT enabled, host, port, from_address FROM smtp_config
         WHERE id = '00000000-0000-0000-0000-000000000002'",
    )
    .fetch_one(&pool)
    .await?;

    let attempt = std::panic::AssertUnwindSafe(async {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at)
             VALUES ($1, $2, NOW(), NOW())",
        )
        .bind(tenant_id)
        .bind(format!("Workflow audit transaction test {tenant_id}"))
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO users
                (id, username, email, password_hash, display_name, is_admin,
                 storage_quota, tenant_id)
             VALUES ($1, $2, $3, 'test-password-hash', $2, true, 10737418240, $4)",
        )
        .bind(actor_id)
        .bind(&admin_username)
        .bind(format!("{admin_username}@test.local"))
        .bind(tenant_id)
        .execute(&pool)
        .await?;

        sqlx::query(
            "INSERT INTO workflows (id, tenant_id, key, name, trigger_type, status, body)
             VALUES ($1, $2, $3, 'Audit Test Workflow', 'manual', 'active', $4)",
        )
        .bind(workflow_id)
        .bind(tenant_id)
        .bind(workflow_key)
        .bind(original_body)
        .execute(&pool)
        .await?;

        sqlx::query(&format!("CREATE SEQUENCE {sequence_name}"))
            .execute(&pool)
            .await?;
        sqlx::query(&format!(
            "CREATE FUNCTION {function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $trigger$
             BEGIN
                 IF NEW.actor_id = TG_ARGV[0]::uuid
                    AND NEW.target_id = TG_ARGV[1]::uuid
                    AND NEW.action_type = TG_ARGV[2] THEN
                     PERFORM nextval(TG_ARGV[3]::regclass);
                     RAISE EXCEPTION 'injected workflow audit insert failure';
                 END IF;
                 RETURN NEW;
             END;
             $trigger$"
        ))
        .execute(&pool)
        .await?;
        let create_trigger = |action: &str| {
            format!(
                "CREATE TRIGGER {trigger_name} BEFORE INSERT ON admin_actions
                 FOR EACH ROW EXECUTE FUNCTION {function_name}('{actor_id}', '{workflow_id}', '{action}', '{sequence_name}')"
            )
        };

        let app = rustshare_server::routes::admin_routes().with_state(state.clone());
        let bearer = support::calendar_harness::create_auth_token(&state, actor_id, tenant_id);
        let request = |method, uri: String, body: String| {
            Request::builder()
                .method(method)
                .uri(uri)
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {bearer}"),
                )
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("build workflow admin request")
        };
        let endpoint = format!("/api/v1/admin/workflows/{workflow_id}");
        let update_request = || {
            request(
                axum::http::Method::PUT,
                endpoint.clone(),
                serde_json::json!({"body": updated_body}).to_string(),
            )
        };

        sqlx::query(&create_trigger("workflow.updated"))
            .execute(&pool)
            .await?;
        let failed_update = app.clone().oneshot(update_request()).await?;
        assert_eq!(
            failed_update.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let update_failure_marker: i64 = sqlx::query_scalar(&format!(
            "SELECT last_value FROM {sequence_name}"
        ))
        .fetch_one(&pool)
        .await?;
        assert_eq!(update_failure_marker, 1, "update audit failure trigger did not fire");
        let body_after_failure: String = sqlx::query_scalar(
            "SELECT body FROM workflows WHERE id = $1",
        )
        .bind(workflow_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(body_after_failure, original_body);
        let failed_update_has_no_audit: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS (SELECT 1 FROM admin_actions
             WHERE actor_id = $1 AND target_id = $2 AND action_type = 'workflow.updated')",
        )
        .bind(actor_id)
        .bind(workflow_id)
        .fetch_one(&pool)
        .await?;
        assert!(failed_update_has_no_audit);

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let updated = app.clone().oneshot(update_request()).await?;
        assert_eq!(updated.status(), axum::http::StatusCode::OK);
        let update_detail: serde_json::Value = sqlx::query_scalar(
            "SELECT detail FROM admin_actions
             WHERE actor_id = $1 AND target_id = $2 AND action_type = 'workflow.updated'",
        )
        .bind(actor_id)
        .bind(workflow_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(update_detail, serde_json::json!({}));
        assert!(!update_detail.to_string().contains(&updated_body));

        sqlx::query(&create_trigger("workflow.disabled"))
            .execute(&pool)
            .await?;
        let disable_request = || {
            request(
                axum::http::Method::POST,
                format!("{endpoint}/disable"),
                String::new(),
            )
        };
        let failed_disable = app.clone().oneshot(disable_request()).await?;
        assert_eq!(
            failed_disable.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let disable_failure_marker: i64 = sqlx::query_scalar(&format!(
            "SELECT last_value FROM {sequence_name}"
        ))
        .fetch_one(&pool)
        .await?;
        assert_eq!(disable_failure_marker, 2, "disable audit failure trigger did not fire");
        let status_after_failure: String = sqlx::query_scalar(
            "SELECT status FROM workflows WHERE id = $1",
        )
        .bind(workflow_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(status_after_failure, "active");

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let disabled = app.clone().oneshot(disable_request()).await?;
        assert_eq!(disabled.status(), axum::http::StatusCode::OK);
        let event_counts: (i64, i64) = sqlx::query_as(
            "SELECT
                count(*) FILTER (WHERE action_type = 'workflow.updated'),
                count(*) FILTER (WHERE action_type = 'workflow.disabled')
             FROM admin_actions WHERE actor_id = $1 AND target_id = $2",
        )
        .bind(actor_id)
        .bind(workflow_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(event_counts, (1, 1));

        sqlx::query(
            "UPDATE smtp_config
             SET enabled = true, host = 'smtp.test.local', port = 587,
                 from_address = 'test@example.com'
             WHERE id = '00000000-0000-0000-0000-000000000002'",
        )
        .execute(&pool)
        .await?;

        sqlx::query(&create_trigger("workflow.enabled"))
            .execute(&pool)
            .await?;
        let enable_request = || {
            request(
                axum::http::Method::POST,
                format!("{endpoint}/enable"),
                String::new(),
            )
        };
        let failed_enable = app.clone().oneshot(enable_request()).await?;
        assert_eq!(
            failed_enable.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        let enable_failure_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        assert_eq!(
            enable_failure_marker, 3,
            "enable audit failure trigger did not fire"
        );
        let status_after_enable_failure: String = sqlx::query_scalar(
            "SELECT status FROM workflows WHERE id = $1",
        )
        .bind(workflow_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(status_after_enable_failure, "draft");
        let failed_enable_has_no_audit: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS (SELECT 1 FROM admin_actions
             WHERE actor_id = $1 AND target_id = $2 AND action_type = 'workflow.enabled')",
        )
        .bind(actor_id)
        .bind(workflow_id)
        .fetch_one(&pool)
        .await?;
        assert!(failed_enable_has_no_audit);

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let enabled = app.oneshot(enable_request()).await?;
        assert_eq!(enabled.status(), axum::http::StatusCode::OK);
        let status_after_success: String = sqlx::query_scalar(
            "SELECT status FROM workflows WHERE id = $1",
        )
        .bind(workflow_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(status_after_success, "active");
        let event_counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT
                count(*) FILTER (WHERE action_type = 'workflow.updated'),
                count(*) FILTER (WHERE action_type = 'workflow.disabled'),
                count(*) FILTER (WHERE action_type = 'workflow.enabled')
             FROM admin_actions WHERE actor_id = $1 AND target_id = $2",
        )
        .bind(actor_id)
        .bind(workflow_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(event_counts, (1, 1, 1));

        Ok::<(), Box<dyn Error + Send + Sync>>(())
    })
    .catch_unwind()
    .await;

    let mut cleanup_error = None;
    for statement in [
        format!("DROP TRIGGER IF EXISTS {trigger_name} ON admin_actions"),
        format!("DROP FUNCTION IF EXISTS {function_name}()"),
        format!("DROP SEQUENCE IF EXISTS {sequence_name}"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }
    for statement in [
        format!("DELETE FROM admin_actions WHERE actor_id = '{actor_id}' OR target_id = '{workflow_id}'"),
        format!("DELETE FROM workflows WHERE id = '{workflow_id}'"),
        format!("DELETE FROM users WHERE id = '{actor_id}'"),
        format!("DELETE FROM tenants WHERE id = '{tenant_id}'"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }
    if let Err(error) = sqlx::query(
        "UPDATE smtp_config
         SET enabled = $1, host = $2, port = $3, from_address = $4
         WHERE id = '00000000-0000-0000-0000-000000000002'",
    )
    .bind(original_smtp_config.0)
    .bind(original_smtp_config.1)
    .bind(original_smtp_config.2)
    .bind(original_smtp_config.3)
    .execute(&pool)
    .await
    {
        if cleanup_error.is_none() {
            cleanup_error = Some(error);
        }
    }

    match attempt {
        Ok(result) => {
            if let Some(error) = cleanup_error {
                return match result {
                    Ok(()) => Err(Box::new(error) as Box<dyn Error + Send + Sync>),
                    Err(attempt_error) => Err(Box::new(std::io::Error::other(format!(
                        "workflow audit attempt failed: {attempt_error}; cleanup also failed: {error}"
                    )))
                        as Box<dyn Error + Send + Sync>),
                };
            }
            result?;
        }
        Err(panic) => {
            if let Some(error) = cleanup_error {
                eprintln!("workflow audit test cleanup also failed: {error}");
            }
            std::panic::resume_unwind(panic);
        }
    }
    Ok(())
}
