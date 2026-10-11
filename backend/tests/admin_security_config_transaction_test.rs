//! Real-route regression proving security configuration and audit are atomic.

mod support;

use axum::{body::Body, http::Request, http::StatusCode};
use serde_json::Value;
use std::error::Error;
use support::calendar_harness::{setup_test_env, SERIAL};
use tower::ServiceExt;
use uuid::Uuid;

type TestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn require_test(condition: bool, message: &'static str) -> TestResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
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

#[tokio::test]
#[ignore = "requires explicitly configured disposable local PostgreSQL and RustFS services"]
async fn security_config_update_rolls_back_when_audit_insert_fails() -> TestResult<()> {
    let _serial = SERIAL.lock().await;

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| std::io::Error::other("set DATABASE_URL to a disposable local database"))?;
    let parsed_database_url = url::Url::parse(&database_url)?;
    let database_name = parsed_database_url.path().trim_start_matches('/');
    require_test(
        database_name == "rustshare_test" || database_name.starts_with("rustshare_test_"),
        "refusing security config test outside a rustshare_test database",
    )?;
    require_test(
        matches!(
            parsed_database_url.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ),
        "refusing security config test against a non-loopback database",
    )?;
    require_test(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB").as_deref() == Ok("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_DB=1 only for a disposable database",
    )?;
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
        "refusing security config test against a non-loopback or credentialed object store",
    )?;
    let bucket = std::env::var("S3_BUCKET").or_else(|_| std::env::var("RUSTFS_BUCKET"))?;
    require_test(
        bucket == "rustshare-test"
            || bucket.starts_with("rustshare-test-")
            || bucket.starts_with("rustshare-test_"),
        "refusing security config test outside a rustshare-test bucket",
    )?;

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = state.default_tenant_id;
    let actor_id = Uuid::new_v4();
    let suffix = actor_id.simple().to_string();
    let username = format!("security_config_admin_{suffix}");
    let function_name = format!("rs_security_audit_fail_{suffix}");
    let trigger_name = function_name.clone();
    let sequence_name = format!("rs_security_audit_seq_{suffix}");
    let original = state
        .metadata_store
        .get_security_config()
        .await?
        .ok_or("security configuration row is missing")?;
    let new_enabled = !original.login_protection_enabled;
    let new_max_attempts = if original.max_login_attempts == 99 {
        98
    } else {
        99
    };
    let new_block_duration = if original.login_block_duration_minutes == 4321 {
        4320
    } else {
        4321
    };
    let expected_detail = serde_json::json!({
        "login_protection_enabled": new_enabled,
        "max_login_attempts": new_max_attempts,
        "login_block_duration_minutes": new_block_duration,
    });
    let mut test_update_timestamp: Option<chrono::DateTime<chrono::Utc>> = None;

    let attempt: TestResult<()> = async {
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
                    AND NEW.action_type = 'config.security_updated' THEN
                     PERFORM nextval(TG_ARGV[1]::regclass);
                     RAISE EXCEPTION 'injected security config audit insert failure';
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
        let body = serde_json::json!({
            "login_protection_enabled": new_enabled,
            "max_login_attempts": new_max_attempts,
            "login_block_duration_minutes": new_block_duration,
        });
        let make_request = || -> Result<Request<Body>, axum::http::Error> {
            Request::builder()
                .method(axum::http::Method::PUT)
                .uri("/api/v1/admin/config/security")
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {bearer}"),
                )
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
        };

        let failed = app.clone().oneshot(make_request()?).await?;
        require_test(
            failed.status() == StatusCode::INTERNAL_SERVER_ERROR,
            "security config update should fail when audit insertion fails",
        )?;
        let sequence_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        require_test(
            sequence_marker == 1,
            "security config audit failure trigger did not fire",
        )?;
        let rolled_back: (bool, i32, i32, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT login_protection_enabled, max_login_attempts,
                    login_block_duration_minutes, updated_at
             FROM security_config WHERE id = 1",
        )
        .fetch_one(&pool)
        .await?;
        require_test(
            rolled_back.0 == original.login_protection_enabled
                && rolled_back.1 == original.max_login_attempts
                && rolled_back.2 == original.login_block_duration_minutes
                && rolled_back.3 == original.updated_at,
            "failed audit insert must roll back all security config fields",
        )?;
        let failed_audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM admin_actions
             WHERE actor_id = $1 AND action_type = 'config.security_updated'",
        )
        .bind(actor_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            failed_audit_count == 0,
            "failed update must not leave an audit event",
        )?;

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let succeeded = app.oneshot(make_request()?).await?;
        require_test(
            succeeded.status() == StatusCode::OK,
            "security config update should succeed after removing failure trigger",
        )?;
        test_update_timestamp = sqlx::query_scalar(
            "SELECT performed_at FROM admin_actions
             WHERE actor_id = $1 AND action_type = 'config.security_updated'",
        )
        .bind(actor_id)
        .fetch_optional(&pool)
        .await?;
        require_test(
            test_update_timestamp.is_some(),
            "successful update did not create its actor-attributed audit event",
        )?;
        let persisted_timestamp = sqlx::query_scalar(
            "SELECT updated_at FROM security_config
             WHERE id = 1
               AND login_protection_enabled = $1
               AND max_login_attempts = $2
               AND login_block_duration_minutes = $3",
        )
        .bind(new_enabled)
        .bind(new_max_attempts)
        .bind(new_block_duration)
        .fetch_optional(&pool)
        .await?;
        require_test(
            persisted_timestamp == test_update_timestamp,
            "security update timestamp must match its actor-attributed audit event",
        )?;
        let response: Value = serde_json::from_slice(
            &axum::body::to_bytes(succeeded.into_body(), usize::MAX).await?,
        )?;
        require_test(
            response
                .get("login_protection_enabled")
                .and_then(Value::as_bool)
                == Some(new_enabled)
                && response.get("max_login_attempts").and_then(Value::as_i64)
                    == Some(i64::from(new_max_attempts))
                && response
                    .get("login_block_duration_minutes")
                    .and_then(Value::as_i64)
                    == Some(i64::from(new_block_duration)),
            "successful response must return the persisted security config",
        )?;
        let audit_rows: Vec<(Option<String>, Option<Uuid>, Value)> = sqlx::query_as(
            "SELECT target_type, target_id, detail FROM admin_actions
             WHERE actor_id = $1 AND action_type = 'config.security_updated'",
        )
        .bind(actor_id)
        .fetch_all(&pool)
        .await?;
        require_test(
            audit_rows.len() == 1 && audit_rows[0] == (None, None, expected_detail),
            "successful security config update must preserve one exact audit event",
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
        format!("DELETE FROM users WHERE id = '{actor_id}'"),
    ] {
        cleanup_statement(&pool, &statement, &mut cleanup_error).await;
    }
    let restore_result: Result<(), sqlx::Error> = async {
        if let Some(test_update_timestamp) = test_update_timestamp {
            let restored = sqlx::query(
                "UPDATE security_config
                 SET login_protection_enabled = $1,
                     max_login_attempts = $2,
                     login_block_duration_minutes = $3,
                     updated_at = $4
                 WHERE id = 1
                   AND login_protection_enabled = $5
                   AND max_login_attempts = $6
                   AND login_block_duration_minutes = $7
                   AND updated_at = $8",
            )
            .bind(original.login_protection_enabled)
            .bind(original.max_login_attempts)
            .bind(original.login_block_duration_minutes)
            .bind(original.updated_at)
            .bind(new_enabled)
            .bind(new_max_attempts)
            .bind(new_block_duration)
            .bind(test_update_timestamp)
            .execute(&pool)
            .await?;

            if restored.rows_affected() == 1 {
                let restored: (bool, i32, i32, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
                    "SELECT login_protection_enabled, max_login_attempts,
                            login_block_duration_minutes, updated_at
                     FROM security_config WHERE id = 1",
                )
                .fetch_one(&pool)
                .await?;
                if restored
                    != (
                        original.login_protection_enabled,
                        original.max_login_attempts,
                        original.login_block_duration_minutes,
                        original.updated_at,
                    )
                {
                    return Err(sqlx::Error::Protocol(
                        "security config fixture restoration did not match original state"
                            .to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = restore_result {
        if cleanup_error.is_none() {
            cleanup_error = Some(error);
        }
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
