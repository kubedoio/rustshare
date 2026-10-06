//! Real-route regression proving template mutations and audit events are atomic.

mod support;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::Response,
    Router,
};
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

fn request(
    method: Method,
    uri: String,
    bearer: &str,
    body: Option<Value>,
) -> Result<Request<Body>, axum::http::Error> {
    let mut builder = Request::builder().method(method).uri(uri).header(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {bearer}"),
    );
    let body = match body {
        Some(body) => {
            builder = builder.header(axum::http::header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    builder.body(body)
}

async fn send(
    app: &Router,
    method: Method,
    uri: String,
    bearer: &str,
    body: Option<Value>,
) -> Result<Response, Box<dyn Error + Send + Sync>> {
    Ok(app
        .clone()
        .oneshot(request(method, uri, bearer, body)?)
        .await?)
}

fn create_body(template_key: &str) -> Value {
    serde_json::json!({
        "template_key": template_key,
        "name": "Audit transaction fixture",
        "application_id": "io.elembra.notes",
        "description": "Fixture for atomic template audit tests",
        "ui_config": {},
        "folder_structure": [],
        "default_files": [],
        "metadata_schema": {},
        "visibility_policy": "workspace",
        "application_config": {}
    })
}

async fn install_failure_trigger(
    pool: &sqlx::PgPool,
    trigger_name: &str,
    function_name: &str,
    actor_id: Uuid,
    sequence_name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(
        "CREATE TRIGGER {trigger_name} BEFORE INSERT ON admin_actions
         FOR EACH ROW EXECUTE FUNCTION {function_name}('{actor_id}', '{sequence_name}')"
    ))
    .execute(pool)
    .await?;
    Ok(())
}

async fn drop_failure_trigger(pool: &sqlx::PgPool, trigger_name: &str) -> Result<(), sqlx::Error> {
    sqlx::query(&format!("DROP TRIGGER {trigger_name} ON admin_actions"))
        .execute(pool)
        .await?;
    Ok(())
}

async fn audit_row(
    pool: &sqlx::PgPool,
    actor_id: Uuid,
    action_type: &str,
) -> Result<(i64, Option<String>, Option<Uuid>, Value), sqlx::Error> {
    sqlx::query_as(
        "SELECT count(*) OVER (), target_type, target_id, detail FROM admin_actions
         WHERE actor_id = $1 AND action_type = $2",
    )
    .bind(actor_id)
    .bind(action_type)
    .fetch_one(pool)
    .await
}

async fn audit_count(
    pool: &sqlx::PgPool,
    actor_id: Uuid,
    action_type: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM admin_actions WHERE actor_id = $1 AND action_type = $2",
    )
    .bind(actor_id)
    .bind(action_type)
    .fetch_one(pool)
    .await
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
async fn template_mutations_roll_back_when_audit_insert_fails() -> TestResult<()> {
    let _serial = SERIAL.lock().await;

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| std::io::Error::other("set DATABASE_URL to a disposable local database"))?;
    let parsed_database_url = url::Url::parse(&database_url)?;
    let database_name = parsed_database_url.path().trim_start_matches('/');
    require_test(
        database_name == "rustshare_test" || database_name.starts_with("rustshare_test_"),
        "refusing template transaction test outside a rustshare_test database",
    )?;
    require_test(
        matches!(
            parsed_database_url.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ),
        "refusing template transaction test against a non-loopback database",
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
        "refusing template transaction test against a non-loopback or credentialed object store",
    )?;
    let bucket = std::env::var("S3_BUCKET").or_else(|_| std::env::var("RUSTFS_BUCKET"))?;
    require_test(
        bucket == "rustshare-test"
            || bucket.starts_with("rustshare-test-")
            || bucket.starts_with("rustshare-test_"),
        "refusing template transaction test outside a rustshare-test bucket",
    )?;

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = state.default_tenant_id;
    let actor_id = Uuid::new_v4();
    let suffix = actor_id.simple().to_string();
    let username = format!("template_audit_admin_{suffix}");
    let template_key = format!("rs_template_audit_{suffix}");
    let duplicate_key = format!("{template_key}_copy");
    let function_name = format!("rs_template_audit_fail_{suffix}");
    let trigger_name = function_name.clone();
    let sequence_name = format!("rs_template_audit_seq_{suffix}");

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
                    AND NEW.action_type LIKE 'template.%' THEN
                     PERFORM nextval(TG_ARGV[1]::regclass);
                     RAISE EXCEPTION 'injected template audit insert failure';
                 END IF;
                 RETURN NEW;
             END;
             $trigger$"
        ))
        .execute(&pool)
        .await?;
        install_failure_trigger(
            &pool,
            &trigger_name,
            &function_name,
            actor_id,
            &sequence_name,
        )
        .await?;

        let app = rustshare_server::routes::admin_routes().with_state(state.clone());
        let bearer = support::calendar_harness::create_auth_token(&state, actor_id, tenant_id);

        let failed_create = send(
            &app,
            Method::POST,
            "/api/v1/admin/templates".to_string(),
            &bearer,
            Some(create_body(&template_key)),
        )
        .await?;
        require_test(
            failed_create.status() == StatusCode::INTERNAL_SERVER_ERROR,
            "template create should fail when its audit insert fails",
        )?;
        require_test(
            audit_count(&pool, actor_id, "template.created").await? == 0,
            "failed template create must not leave an audit row",
        )?;
        let create_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM templates WHERE template_key = $1 AND tenant_id = $2",
        )
        .bind(&template_key)
        .bind(tenant_id)
        .fetch_one(&pool)
        .await?;
        require_test(create_count == 0, "failed template create must roll back")?;
        let sequence_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        require_test(
            sequence_marker == 1,
            "template create audit trigger did not fire",
        )?;

        drop_failure_trigger(&pool, &trigger_name).await?;
        let created = send(
            &app,
            Method::POST,
            "/api/v1/admin/templates".to_string(),
            &bearer,
            Some(create_body(&template_key)),
        )
        .await?;
        require_test(
            created.status() == StatusCode::OK,
            "template create retry should succeed",
        )?;
        let created_body: Value =
            serde_json::from_slice(&axum::body::to_bytes(created.into_body(), usize::MAX).await?)?;
        let template_id = created_body
            .get("id")
            .and_then(Value::as_str)
            .ok_or("created template response has no id")?
            .parse::<Uuid>()?;
        let create_audit = audit_row(&pool, actor_id, "template.created").await?;
        require_test(
            create_audit
                == (
                    1,
                    Some("template".to_string()),
                    Some(template_id),
                    serde_json::json!({
                        "template_key": template_key,
                        "application_id": "io.elembra.notes"
                    }),
                ),
            "template create should preserve its existing audit contract",
        )?;

        install_failure_trigger(
            &pool,
            &trigger_name,
            &function_name,
            actor_id,
            &sequence_name,
        )
        .await?;
        let failed_update = send(
            &app,
            Method::PUT,
            format!("/api/v1/admin/templates/{template_key}"),
            &bearer,
            Some(serde_json::json!({ "name": "Updated audit fixture" })),
        )
        .await?;
        require_test(
            failed_update.status() == StatusCode::INTERNAL_SERVER_ERROR,
            "template update should fail when its audit insert fails",
        )?;
        require_test(
            audit_count(&pool, actor_id, "template.updated").await? == 0,
            "failed template update must not leave an audit row",
        )?;
        let rolled_back_name: String = sqlx::query_scalar(
            "SELECT name FROM templates WHERE template_key = $1 AND tenant_id = $2",
        )
        .bind(&template_key)
        .bind(tenant_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            rolled_back_name == "Audit transaction fixture",
            "failed template update must preserve the old name",
        )?;
        let sequence_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        require_test(
            sequence_marker == 2,
            "template update audit trigger did not fire",
        )?;

        drop_failure_trigger(&pool, &trigger_name).await?;
        let updated = send(
            &app,
            Method::PUT,
            format!("/api/v1/admin/templates/{template_key}"),
            &bearer,
            Some(serde_json::json!({ "name": "Updated audit fixture" })),
        )
        .await?;
        require_test(
            updated.status() == StatusCode::OK,
            "template update retry should succeed",
        )?;
        let update_audit = audit_row(&pool, actor_id, "template.updated").await?;
        require_test(
            update_audit
                == (
                    1,
                    Some("template".to_string()),
                    Some(template_id),
                    serde_json::json!({
                        "template_key": template_key,
                        "application_id": "io.elembra.notes"
                    }),
                ),
            "template update should preserve its existing audit contract",
        )?;

        install_failure_trigger(
            &pool,
            &trigger_name,
            &function_name,
            actor_id,
            &sequence_name,
        )
        .await?;
        let failed_duplicate = send(
            &app,
            Method::POST,
            format!("/api/v1/admin/templates/{template_key}/duplicate"),
            &bearer,
            None,
        )
        .await?;
        require_test(
            failed_duplicate.status() == StatusCode::INTERNAL_SERVER_ERROR,
            "template duplicate should fail when its audit insert fails",
        )?;
        require_test(
            audit_count(&pool, actor_id, "template.duplicated").await? == 0,
            "failed template duplicate must not leave an audit row",
        )?;
        let duplicate_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM templates WHERE template_key = $1 AND tenant_id = $2",
        )
        .bind(&duplicate_key)
        .bind(tenant_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            duplicate_count == 0,
            "failed template duplicate must roll back",
        )?;
        let sequence_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        require_test(
            sequence_marker == 3,
            "template duplicate audit trigger did not fire",
        )?;

        drop_failure_trigger(&pool, &trigger_name).await?;
        let duplicated = send(
            &app,
            Method::POST,
            format!("/api/v1/admin/templates/{template_key}/duplicate"),
            &bearer,
            None,
        )
        .await?;
        require_test(
            duplicated.status() == StatusCode::OK,
            "template duplicate retry should succeed",
        )?;
        let duplicated_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(duplicated.into_body(), usize::MAX).await?,
        )?;
        let duplicate_id = duplicated_body
            .get("id")
            .and_then(Value::as_str)
            .ok_or("duplicated template response has no id")?
            .parse::<Uuid>()?;
        let duplicate_audit = audit_row(&pool, actor_id, "template.duplicated").await?;
        require_test(
            duplicate_audit
                == (
                    1,
                    Some("template".to_string()),
                    Some(template_id),
                    serde_json::json!({
                        "original_key": template_key,
                        "new_key": duplicate_key,
                        "new_id": duplicate_id
                    }),
                ),
            "template duplicate should retain its original-template audit target and detail",
        )?;

        install_failure_trigger(
            &pool,
            &trigger_name,
            &function_name,
            actor_id,
            &sequence_name,
        )
        .await?;
        let failed_delete = send(
            &app,
            Method::DELETE,
            format!("/api/v1/admin/templates/{duplicate_key}"),
            &bearer,
            None,
        )
        .await?;
        require_test(
            failed_delete.status() == StatusCode::INTERNAL_SERVER_ERROR,
            "template delete should fail when its audit insert fails",
        )?;
        require_test(
            audit_count(&pool, actor_id, "template.deleted").await? == 0,
            "failed template delete must not leave an audit row",
        )?;
        let preserved_duplicate: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM templates WHERE template_key = $1 AND tenant_id = $2",
        )
        .bind(&duplicate_key)
        .bind(tenant_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            preserved_duplicate == 1,
            "failed template delete must preserve the row",
        )?;
        let sequence_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        require_test(
            sequence_marker == 4,
            "template delete audit trigger did not fire",
        )?;

        drop_failure_trigger(&pool, &trigger_name).await?;
        let deleted = send(
            &app,
            Method::DELETE,
            format!("/api/v1/admin/templates/{duplicate_key}"),
            &bearer,
            None,
        )
        .await?;
        require_test(
            deleted.status() == StatusCode::NO_CONTENT,
            "template delete retry should succeed",
        )?;
        let delete_audit = audit_row(&pool, actor_id, "template.deleted").await?;
        require_test(
            delete_audit
                == (
                    1,
                    Some("template".to_string()),
                    Some(duplicate_id),
                    serde_json::json!({ "key": duplicate_key }),
                ),
            "template delete should preserve its existing audit contract",
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
            "DELETE FROM templates WHERE tenant_id = '{tenant_id}' AND template_key IN ('{template_key}', '{duplicate_key}')"
        ),
        format!("DELETE FROM users WHERE id = '{actor_id}'"),
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
