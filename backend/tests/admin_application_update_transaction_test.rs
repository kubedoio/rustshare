//! Real-handler regression for atomic application configuration and audit updates.
//!
//! Run against explicitly disposable loopback PostgreSQL and RustFS services:
//! `cargo test -p rustshare-server --test admin_application_update_transaction_test -- --ignored --exact application_update_rolls_back_when_audit_insert_fails --test-threads=1`

mod support;

use axum::{body::Body, http::Request};
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
async fn application_update_rolls_back_when_audit_insert_fails() -> TestResult<()> {
    let _serial = SERIAL.lock().await;

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| std::io::Error::other("set DATABASE_URL to a disposable local database"))?;
    let parsed_database_url = url::Url::parse(&database_url)?;
    let database_name = parsed_database_url.path().trim_start_matches('/');
    require_test(
        database_name == "rustshare_test" || database_name.starts_with("rustshare_test_"),
        "refusing application update test outside a rustshare_test database",
    )?;
    require_test(
        matches!(
            parsed_database_url.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ),
        "refusing application update test against a non-loopback database",
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
        "refusing application update test against a non-loopback or credentialed object store",
    )?;
    let bucket = std::env::var("S3_BUCKET").or_else(|_| std::env::var("RUSTFS_BUCKET"))?;
    require_test(
        bucket == "rustshare-test"
            || bucket.starts_with("rustshare-test-")
            || bucket.starts_with("rustshare-test_"),
        "refusing application update test outside a rustshare-test bucket",
    )?;

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let suffix = actor_id.simple().to_string();
    let username = format!("application_update_admin_{suffix}");
    let function_name = format!("rs_application_audit_fail_{suffix}");
    let trigger_name = function_name.clone();
    let sequence_name = format!("rs_application_audit_seq_{suffix}");
    let application_key = "io.elembra.notes";

    let attempt: TestResult<()> = async {
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at, updated_at)
             VALUES ($1, $2, NOW(), NOW())",
        )
        .bind(tenant_id)
        .bind(format!("Application update transaction test {tenant_id}"))
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
        state
            .application_service
            .ensure_default_applications(tenant_id)
            .await?;

        let original_configuration: Value = sqlx::query_scalar(
            "SELECT configuration FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(tenant_id)
        .bind(application_key)
        .fetch_one(&pool)
        .await?;

        sqlx::query(&format!("CREATE SEQUENCE {sequence_name}"))
            .execute(&pool)
            .await?;
        sqlx::query(&format!(
            "CREATE FUNCTION {function_name}() RETURNS trigger
             LANGUAGE plpgsql AS $trigger$
             BEGIN
                 IF NEW.actor_id = TG_ARGV[0]::uuid
                    AND NEW.action_type = 'application.updated' THEN
                     PERFORM nextval(TG_ARGV[1]::regclass);
                     RAISE EXCEPTION 'injected application audit insert failure';
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
        let body = serde_json::json!({ "display_name": "Pilot Notes" }).to_string();
        let request = || -> Result<Request<Body>, axum::http::Error> {
            Request::builder()
                .method(axum::http::Method::PATCH)
                .uri(format!("/api/v1/admin/applications/{application_key}"))
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
            "application update should fail when audit insertion fails",
        )?;
        let sequence_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        require_test(
            sequence_marker == 1,
            "application audit failure trigger did not fire",
        )?;
        let rolled_back_configuration: Value = sqlx::query_scalar(
            "SELECT configuration FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(tenant_id)
        .bind(application_key)
        .fetch_one(&pool)
        .await?;
        require_test(
            rolled_back_configuration == original_configuration,
            "failed audit insert must roll back application configuration",
        )?;
        let failure_audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM admin_actions
             WHERE actor_id = $1 AND action_type = 'application.updated'",
        )
        .bind(actor_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            failure_audit_count == 0,
            "failed application update must not create an audit event",
        )?;

        sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
            .execute(&pool)
            .await?;
        let succeeded = app.oneshot(request()?).await?;
        require_test(
            succeeded.status() == axum::http::StatusCode::OK,
            "application update should succeed after audit failure trigger is removed",
        )?;
        let response: Value = serde_json::from_slice(
            &axum::body::to_bytes(succeeded.into_body(), usize::MAX).await?,
        )?;
        require_test(
            response.get("display_name").and_then(Value::as_str) == Some("Pilot Notes"),
            "application update response should preserve the normalized display name",
        )?;

        let saved_configuration: Value = sqlx::query_scalar(
            "SELECT configuration FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(tenant_id)
        .bind(application_key)
        .fetch_one(&pool)
        .await?;
        require_test(
            saved_configuration
                .get("displayName")
                .and_then(Value::as_str)
                == Some("Pilot Notes")
                && saved_configuration.pointer("/ui/sidebar/label")
                    == original_configuration.pointer("/ui/sidebar/label"),
            "successful update should persist the normalized application configuration",
        )?;

        let audit_rows: Vec<(String, Option<String>, Option<Uuid>, Value)> = sqlx::query_as(
            "SELECT action_type, target_type, target_id, detail FROM admin_actions
             WHERE actor_id = $1 AND action_type = 'application.updated'",
        )
        .bind(actor_id)
        .fetch_all(&pool)
        .await?;
        require_test(
            audit_rows.len() == 1,
            "successful application update must create exactly one audit event",
        )?;
        let (action_type, target_type, target_id, detail) = &audit_rows[0];
        require_test(
            action_type == "application.updated"
                && target_type.as_deref() == Some("application")
                && target_id.map(|id| id.to_string())
                    == response
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                && detail == &serde_json::json!({ "application_id": application_key }),
            "successful application audit event must preserve action, target, and detail",
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
        format!("DELETE FROM events WHERE user_id = '{actor_id}'"),
        format!("DELETE FROM application_enablements WHERE tenant_id = '{tenant_id}'"),
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
