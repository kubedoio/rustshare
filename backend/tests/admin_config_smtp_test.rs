//! Integration tests: Admin SMTP config SQL lifecycle and email service.
//!
//! Tests:
//!   - Update the pre-seeded smtp_config row with host, port, encrypted password
//!   - Read back, verify password_enc is set (not plaintext)
//!   - Update config again and verify updated_at changes
//!   - send_test_email fails fast when SMTP is disabled or recipient is invalid
//!
//! HTTP-level tests for `POST /api/v1/admin/config/smtp/test` are not included
//! here because exercising the Axum handler requires a fully constructed
//! `AppState` (including a live S3-compatible object store). The handler itself
//! is thin error-mapping plumbing over `EmailService`, which is covered below.
//!
//! Run with: cargo test --test admin_config_smtp_test

mod support;

use axum::{body::Body, http::Request};
use rand::Rng;
use rustshare_core::services::{EmailError, EmailService};
use rustshare_crypto::{decrypt_secret, encrypt_secret, SecretEncryptionKey};
use sqlx::Row;
use std::error::Error;
use support::calendar_harness::{assert_local_database, setup_test_env, SERIAL};
use tower::ServiceExt;
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const SMTP_CONFIG_ID: &str = "00000000-0000-0000-0000-000000000002";

/// SMTP config tests mutate a single pre-seeded row, so they must run
/// serially to avoid reading state written by a concurrent test.
static SMTP_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    sqlx::PgPool::connect(&url)
        .await
        .expect("DB connect failed")
}

fn test_encryption_key() -> SecretEncryptionKey {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    SecretEncryptionKey::from_bytes(bytes)
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

fn require_test(condition: bool, message: &'static str) -> TestResult<()> {
    if condition {
        Ok(())
    } else {
        Err(std::io::Error::other(message).into())
    }
}

async fn create_test_admin(pool: &sqlx::PgPool, suffix: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, true, 10737418240)",
    )
    .bind(id)
    .bind(format!("smtp_admin_{suffix}"))
    .bind(format!("smtpadmin_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("SMTP Admin {suffix}"))
    .execute(pool)
    .await
    .expect("create test admin");
    id
}

async fn cleanup_users(pool: &sqlx::PgPool, user_ids: &[Uuid]) {
    for id in user_ids {
        sqlx::query("DELETE FROM admin_actions WHERE actor_id = $1 OR target_id = $1")
            .bind(id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await
            .ok();
    }
}

async fn reset_smtp_config(pool: &sqlx::PgPool) {
    let smtp_id: Uuid = SMTP_CONFIG_ID.parse().unwrap();
    sqlx::query(
        "UPDATE smtp_config
         SET enabled = false, host = NULL, port = NULL, username = NULL,
             password_enc = NULL, from_address = NULL, from_name = NULL,
             tls_mode = NULL, updated_by = NULL, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(smtp_id)
    .execute(pool)
    .await
    .expect("reset smtp_config");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Update SMTP config with encrypted password, verify password_enc is stored (not plaintext).
#[tokio::test]
async fn test_smtp_config_update_stores_encrypted_password() {
    let _guard = SMTP_TEST_LOCK.lock().await;

    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let key = test_encryption_key();
    let smtp_id: Uuid = SMTP_CONFIG_ID.parse().unwrap();
    let plaintext_password = "smtp-plaintext-password-123";

    // Start from a known empty state in case a previous run left data behind.
    reset_smtp_config(&pool).await;

    let password_enc = encrypt_secret(plaintext_password, &key).expect("encrypt SMTP password");

    // Same SQL as the handler uses
    sqlx::query(
        "UPDATE smtp_config
         SET enabled      = $2,
             host         = $3,
             port         = $4,
             username     = $5,
             password_enc = $6,
             from_address = $7,
             from_name    = $8,
             tls_mode     = $9,
             updated_by   = $10,
             updated_at   = NOW()
         WHERE id = $1",
    )
    .bind(smtp_id)
    .bind(true)
    .bind("smtp.example.com")
    .bind(587_i32)
    .bind("noreply@example.com")
    .bind(&password_enc)
    .bind("noreply@example.com")
    .bind("RustShare Notifications")
    .bind("starttls")
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("update smtp_config");

    // Read back
    let row = sqlx::query(
        "SELECT enabled, host, port, username, password_enc, tls_mode, updated_by
         FROM smtp_config WHERE id = $1",
    )
    .bind(smtp_id)
    .fetch_one(&pool)
    .await
    .expect("fetch smtp_config");

    let enabled: bool = row.try_get("enabled").unwrap();
    let host: Option<String> = row.try_get("host").unwrap();
    let port: Option<i32> = row.try_get("port").unwrap();
    let stored_enc: Option<String> = row.try_get("password_enc").unwrap();
    let tls_mode: Option<String> = row.try_get("tls_mode").unwrap();

    assert!(enabled, "SMTP must be enabled after update");
    assert_eq!(host.as_deref(), Some("smtp.example.com"));
    assert_eq!(port, Some(587));
    assert_eq!(tls_mode.as_deref(), Some("starttls"));

    let stored = stored_enc.as_deref().expect("password_enc must be set");
    assert_ne!(
        stored, plaintext_password,
        "Plaintext password must not be stored"
    );

    // Round-trip decrypt must recover the original password
    let recovered = decrypt_secret(stored, &key).expect("decrypt stored password");
    assert_eq!(
        recovered, plaintext_password,
        "Decrypted password must match original"
    );

    // Cleanup
    reset_smtp_config(&pool).await;
    cleanup_users(&pool, &[actor_id]).await;
}

/// Second update must change updated_at.
#[tokio::test]
async fn test_smtp_config_update_changes_updated_at() {
    let _guard = SMTP_TEST_LOCK.lock().await;

    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let smtp_id: Uuid = SMTP_CONFIG_ID.parse().unwrap();

    reset_smtp_config(&pool).await;

    // First update — set a timestamp we can compare against
    sqlx::query(
        "UPDATE smtp_config
         SET host = $2, port = $3, updated_by = $4, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(smtp_id)
    .bind("smtp1.example.com")
    .bind(25_i32)
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("first update");

    let first_updated_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM smtp_config WHERE id = $1")
            .bind(smtp_id)
            .fetch_one(&pool)
            .await
            .expect("fetch updated_at after first update");

    // Sleep briefly to ensure NOW() advances
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Second update
    sqlx::query(
        "UPDATE smtp_config
         SET host = $2, port = $3, updated_by = $4, updated_at = NOW()
         WHERE id = $1",
    )
    .bind(smtp_id)
    .bind("smtp2.example.com")
    .bind(587_i32)
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("second update");

    let second_updated_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT updated_at FROM smtp_config WHERE id = $1")
            .bind(smtp_id)
            .fetch_one(&pool)
            .await
            .expect("fetch updated_at after second update");

    assert!(
        second_updated_at > first_updated_at,
        "updated_at must increase after second update"
    );

    // Cleanup
    reset_smtp_config(&pool).await;
    cleanup_users(&pool, &[actor_id]).await;
}

async fn configure_smtp(
    pool: &sqlx::PgPool,
    key: &SecretEncryptionKey,
    actor_id: Uuid,
    enabled: bool,
) {
    let smtp_id: Uuid = SMTP_CONFIG_ID.parse().unwrap();
    let password_enc = encrypt_secret("smtp-password-123", key).expect("encrypt SMTP password");

    sqlx::query(
        "UPDATE smtp_config
         SET enabled      = $2,
             host         = $3,
             port         = $4,
             username     = $5,
             password_enc = $6,
             from_address = $7,
             from_name    = $8,
             tls_mode     = $9,
             updated_by   = $10,
             updated_at   = NOW()
         WHERE id = $1",
    )
    .bind(smtp_id)
    .bind(enabled)
    .bind("smtp.example.com")
    .bind(587_i32)
    .bind("noreply@example.com")
    .bind(&password_enc)
    .bind("noreply@example.com")
    .bind("RustShare Notifications")
    .bind("starttls")
    .bind(actor_id)
    .execute(pool)
    .await
    .expect("configure smtp_config");
}

/// When SMTP is disabled, send_test_email must fail with SmtpNotConfigured.
#[tokio::test]
async fn test_send_test_email_not_configured() {
    let _guard = SMTP_TEST_LOCK.lock().await;

    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let key = test_encryption_key();

    reset_smtp_config(&pool).await;
    configure_smtp(&pool, &key, actor_id, false).await;

    let service = EmailService::new(pool.clone(), key);
    let err = service
        .send_test_email("admin@example.com")
        .await
        .expect_err("expected SmtpNotConfigured error");

    assert!(
        matches!(err, EmailError::SmtpNotConfigured),
        "Expected SmtpNotConfigured, got {:?}",
        err
    );

    reset_smtp_config(&pool).await;
    cleanup_users(&pool, &[actor_id]).await;
}

/// An invalid recipient address must fail fast before talking to the SMTP server.
#[tokio::test]
async fn test_send_test_email_invalid_recipient() {
    let _guard = SMTP_TEST_LOCK.lock().await;

    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let key = test_encryption_key();

    reset_smtp_config(&pool).await;
    configure_smtp(&pool, &key, actor_id, true).await;

    let service = EmailService::new(pool.clone(), key);
    let err = service
        .send_test_email("not-an-email")
        .await
        .expect_err("expected invalid recipient error");

    assert!(
        matches!(err, EmailError::SmtpSendFailed(_)),
        "Expected SmtpSendFailed, got {:?}",
        err
    );

    reset_smtp_config(&pool).await;
    cleanup_users(&pool, &[actor_id]).await;
}

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS services"]
async fn smtp_config_update_rolls_back_when_audit_insert_fails() -> TestResult<()> {
    let _serial = SERIAL.lock().await;
    let _smtp_lock = SMTP_TEST_LOCK.lock().await;

    let database_url = std::env::var("DATABASE_URL").map_err(|_| {
        std::io::Error::other("set DATABASE_URL explicitly to a disposable local database")
    })?;
    let parsed_database_url = url::Url::parse(&database_url)?;
    let database_name = parsed_database_url.path().trim_start_matches('/');
    require_test(
        database_name == "rustshare_test" || database_name.starts_with("rustshare_test_"),
        "refusing SMTP route test outside a rustshare_test database",
    )?;
    require_test(
        matches!(
            parsed_database_url.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ),
        "refusing SMTP route test against a non-loopback database",
    )?;
    require_test(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref() == Ok("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_DB=1 only for a disposable database",
    )?;
    assert_local_database();

    require_test(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE").as_deref() == Ok("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1 only for a disposable object store",
    )?;
    let endpoint = std::env::var("S3_ENDPOINT").or_else(|_| std::env::var("RUSTFS_ENDPOINT"))?;
    let parsed_endpoint = url::Url::parse(&endpoint)?;
    require_test(
        matches!(
            parsed_endpoint.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ) && parsed_endpoint.username().is_empty()
            && parsed_endpoint.password().is_none(),
        "refusing SMTP route test against a non-loopback or credentialed object store",
    )?;
    let bucket = std::env::var("S3_BUCKET").or_else(|_| std::env::var("RUSTFS_BUCKET"))?;
    require_test(
        bucket == "rustshare-test"
            || bucket.starts_with("rustshare-test-")
            || bucket.starts_with("rustshare-test_"),
        "refusing SMTP route test outside a rustshare-test bucket",
    )?;

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let suffix = actor_id.simple().to_string();
    let username = format!("smtp_audit_admin_{suffix}");
    let function_name = format!("rs_smtp_audit_fail_{suffix}");
    let trigger_name = function_name.clone();
    let sequence_name = format!("rs_smtp_audit_seq_{suffix}");
    let smtp_id: Uuid = SMTP_CONFIG_ID.parse()?;

    let attempt: TestResult<()> = async {
        reset_smtp_config(&pool).await;
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at)
             VALUES ($1, $2, NOW(), NOW())",
        )
        .bind(tenant_id)
        .bind(format!("SMTP audit transaction test {tenant_id}"))
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
                    AND NEW.action_type = 'config.smtp_updated' THEN
                     PERFORM nextval(TG_ARGV[1]::regclass);
                     RAISE EXCEPTION 'injected SMTP audit insert failure';
                 END IF;
                 RETURN NEW;
             END;
             $trigger$"
        ))
        .execute(&pool)
        .await?;
        sqlx::query(&format!(
            "CREATE TRIGGER {trigger_name} BEFORE INSERT ON admin_actions
             FOR EACH ROW EXECUTE FUNCTION {function_name}('{actor_id}', '{sequence_name}')"
        ))
        .execute(&pool)
        .await?;

        let app = rustshare_server::routes::admin_routes().with_state(state.clone());
        let bearer = support::calendar_harness::create_auth_token(&state, actor_id, tenant_id);
        let secret = format!("smtp-password-{suffix}");
        let body = serde_json::json!({
            "enabled": true,
            "host": "smtp.example.test",
            "port": 587,
            "username": "pilot@example.test",
            "password": secret,
            "from_address": "pilot@example.test",
            "from_name": "RustShare Pilot",
            "tls_mode": "starttls"
        })
        .to_string();
        let request = || -> Result<Request<Body>, axum::http::Error> {
            Request::builder()
                .method(axum::http::Method::PUT)
                .uri("/api/v1/admin/config/smtp")
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {bearer}"),
                )
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.clone()))
        };

        let failed = app.clone().oneshot(request()?).await?;
        require_test(
            failed.status() == axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "SMTP config update should fail when audit insertion fails",
        )?;
        let sequence_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        require_test(
            sequence_marker == 1,
            "SMTP audit failure trigger did not fire",
        )?;
        let rolled_back: bool = sqlx::query_scalar(
            "SELECT NOT enabled AND host IS NULL AND password_enc IS NULL AND updated_by IS NULL
             FROM smtp_config WHERE id = $1",
        )
        .bind(smtp_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            rolled_back,
            "failed audit insert must roll back SMTP config",
        )?;
        let no_failure_audit: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS (SELECT 1 FROM admin_actions
             WHERE actor_id = $1 AND action_type = 'config.smtp_updated')",
        )
        .bind(actor_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            no_failure_audit,
            "failed audit insert must not create an audit event",
        )?;

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let succeeded = app.oneshot(request()?).await?;
        require_test(
            succeeded.status() == axum::http::StatusCode::OK,
            "SMTP config update should succeed after audit failure trigger is removed",
        )?;
        let response: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(succeeded.into_body(), usize::MAX).await?,
        )?;
        require_test(
            response.get("password") == Some(&serde_json::Value::String("***".to_string())),
            "SMTP update response must preserve the masked-password behavior",
        )?;

        let stored_secret: String =
            sqlx::query_scalar("SELECT password_enc FROM smtp_config WHERE id = $1")
                .bind(smtp_id)
                .fetch_one(&pool)
                .await?;
        require_test(
            stored_secret != secret,
            "stored SMTP password must be encrypted",
        )?;
        require_test(
            decrypt_secret(&stored_secret, &state.secret_key)? == secret,
            "stored SMTP password must decrypt to the submitted value",
        )?;
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM admin_actions
             WHERE actor_id = $1 AND action_type = 'config.smtp_updated'",
        )
        .bind(actor_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            audit_count == 1,
            "successful SMTP update must create one audit event",
        )?;
        let audit_detail: serde_json::Value = sqlx::query_scalar(
            "SELECT detail FROM admin_actions
             WHERE actor_id = $1 AND action_type = 'config.smtp_updated'",
        )
        .bind(actor_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            audit_detail == serde_json::json!({}) && !audit_detail.to_string().contains(&secret),
            "SMTP audit details must be empty and exclude credentials",
        )?;
        Ok(())
    }
    .await;

    let mut cleanup_error = None;
    for statement in [
        format!("DROP TRIGGER IF EXISTS {trigger_name} ON admin_actions"),
        format!("DROP FUNCTION IF EXISTS {function_name}()"),
        format!("DROP SEQUENCE IF EXISTS {sequence_name}"),
        format!("DELETE FROM admin_actions WHERE actor_id = '{actor_id}'"),
        format!(
            "UPDATE smtp_config SET enabled = false, host = NULL, port = NULL,
             username = NULL, password_enc = NULL, from_address = NULL,
             from_name = NULL, tls_mode = NULL, updated_by = NULL, updated_at = NOW()
             WHERE id = '{smtp_id}'"
        ),
        format!("DELETE FROM users WHERE id = '{actor_id}'"),
        format!("DELETE FROM tenants WHERE id = '{tenant_id}'"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }

    if let Err(test_error) = attempt {
        if let Some(cleanup_error) = cleanup_error {
            return Err(format!("{test_error}; cleanup also failed: {cleanup_error}").into());
        }
        return Err(test_error);
    }
    if let Some(cleanup_error) = cleanup_error {
        return Err(Box::new(cleanup_error) as Box<dyn Error + Send + Sync>);
    }
    Ok(())
}
