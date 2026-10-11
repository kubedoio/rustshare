//! Real-route regression for atomic application enablement and audit updates.

mod support;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    response::Response,
    Router,
};
use rustshare_server::services::application_service::ApplicationError;
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

fn request(method: Method, uri: String, bearer: &str) -> Result<Request<Body>, axum::http::Error> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {bearer}"),
        )
        .body(Body::empty())
}

fn json_request(
    method: Method,
    uri: String,
    bearer: &str,
    body: &Value,
) -> Result<Request<Body>, Box<dyn Error + Send + Sync>> {
    Ok(Request::builder()
        .method(method)
        .uri(uri)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {bearer}"),
        )
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(body)?))?)
}

async fn send(
    app: &Router,
    method: Method,
    uri: String,
    bearer: &str,
) -> Result<Response, Box<dyn Error + Send + Sync>> {
    Ok(app.clone().oneshot(request(method, uri, bearer)?).await?)
}

async fn send_json(
    app: &Router,
    method: Method,
    uri: String,
    bearer: &str,
    body: &Value,
) -> Result<Response, Box<dyn Error + Send + Sync>> {
    Ok(app
        .clone()
        .oneshot(json_request(method, uri, bearer, body)?)
        .await?)
}

async fn audit_rows(
    pool: &sqlx::PgPool,
    actor_id: Uuid,
    action_type: &str,
) -> Result<Vec<(Option<String>, Option<Uuid>, Value)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT target_type, target_id, detail FROM admin_actions
         WHERE actor_id = $1 AND action_type = $2",
    )
    .bind(actor_id)
    .bind(action_type)
    .fetch_all(pool)
    .await
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
async fn application_enablement_rolls_back_when_audit_insert_fails() -> TestResult<()> {
    let _serial = SERIAL.lock().await;

    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| std::io::Error::other("set DATABASE_URL to a disposable local database"))?;
    let parsed_database_url = url::Url::parse(&database_url)?;
    let database_name = parsed_database_url.path().trim_start_matches('/');
    require_test(
        database_name == "rustshare_test" || database_name.starts_with("rustshare_test_"),
        "refusing application enablement test outside a rustshare_test database",
    )?;
    require_test(
        matches!(
            parsed_database_url.host_str(),
            Some("localhost" | "127.0.0.1" | "::1")
        ),
        "refusing application enablement test against a non-loopback database",
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
        "refusing application enablement test against a non-loopback or credentialed object store",
    )?;
    let bucket = std::env::var("S3_BUCKET").or_else(|_| std::env::var("RUSTFS_BUCKET"))?;
    require_test(
        bucket == "rustshare-test"
            || bucket.starts_with("rustshare-test-")
            || bucket.starts_with("rustshare-test_"),
        "refusing application enablement test outside a rustshare-test bucket",
    )?;

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let other_tenant_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let suffix = actor_id.simple().to_string();
    let username = format!("application_enable_admin_{suffix}");
    let function_name = format!("rs_application_enable_audit_fail_{suffix}");
    let trigger_name = function_name.clone();
    let sequence_name = format!("rs_application_enable_audit_seq_{suffix}");
    let application_key = "io.elembra.notes";

    let attempt: TestResult<()> = async {
        for (id, label) in [(tenant_id, "target"), (other_tenant_id, "other")] {
            sqlx::query(
                "INSERT INTO tenants (id, name, created_at, updated_at)
                 VALUES ($1, $2, NOW(), NOW())",
            )
            .bind(id)
            .bind(format!("Application enablement {label} test {id}"))
            .execute(&pool)
            .await?;
            state
                .application_service
                .ensure_default_applications(id)
                .await?;
        }
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

        sqlx::query(
            "UPDATE application_enablements SET enabled = false
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(tenant_id)
        .bind(application_key)
        .execute(&pool)
        .await?;
        let other_tenant_enabled: bool = sqlx::query_scalar(
            "SELECT enabled FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(other_tenant_id)
        .bind(application_key)
        .fetch_one(&pool)
        .await?;
        require_test(
            other_tenant_enabled,
            "fixture's independent tenant application should begin enabled",
        )?;
        let folders_before_enable: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM folders WHERE owner_id = $1 AND tenant_id = $2",
        )
        .bind(actor_id)
        .bind(tenant_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            folders_before_enable == 0,
            "enable fixture unexpectedly already has application root folders",
        )?;

        let nested_root_path = "/Workspace/Notes/Archive";
        sqlx::query(
            "UPDATE application_enablements
             SET configuration = jsonb_set(configuration, '{rootPath}', to_jsonb($1::text))
             WHERE tenant_id = $2 AND workspace_id = $2 AND application_id = $3",
        )
        .bind(nested_root_path)
        .bind(tenant_id)
        .bind(application_key)
        .execute(&pool)
        .await?;
        let prepared_root_path = state
            .application_service
            .prepare_application_enable(application_key, actor_id, tenant_id)
            .await?;
        let legacy_root_folder_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM folders root
             JOIN folders workspace ON workspace.id = root.parent_folder_id
             WHERE root.name = 'Archive' AND workspace.name = 'Workspace'
               AND root.owner_id = $1 AND root.tenant_id = $2",
        )
        .bind(actor_id)
        .bind(tenant_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            prepared_root_path == nested_root_path && legacy_root_folder_count == 1,
            "existing nested-root folder mapping must remain compatible",
        )?;
        sqlx::query(
            "UPDATE application_enablements
             SET configuration = jsonb_set(configuration, '{rootPath}', to_jsonb($1::text))
             WHERE tenant_id = $2 AND workspace_id = $2 AND application_id = $3",
        )
        .bind("/Workspace/Concurrent")
        .bind(tenant_id)
        .bind(application_key)
        .execute(&pool)
        .await?;
        let mut concurrent_tx = pool.begin().await?;
        let concurrent_enable = state
            .application_service
            .set_application_enabled_in_transaction(
                application_key,
                true,
                tenant_id,
                Some(&prepared_root_path),
                &mut concurrent_tx,
            )
            .await;
        require_test(
            matches!(concurrent_enable, Err(ApplicationError::ConfigurationChanged)),
            "enable must reject a root-path change after folder preparation",
        )?;
        concurrent_tx.rollback().await?;
        let enabled_after_config_race: bool = sqlx::query_scalar(
            "SELECT enabled FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(tenant_id)
        .bind(application_key)
        .fetch_one(&pool)
        .await?;
        require_test(
            !enabled_after_config_race,
            "root-path race must leave application disabled",
        )?;
        sqlx::query(
            "UPDATE application_enablements
             SET configuration = jsonb_set(configuration, '{rootPath}', to_jsonb($1::text))
             WHERE tenant_id = $2 AND workspace_id = $2 AND application_id = $3",
        )
        .bind(&prepared_root_path)
        .bind(tenant_id)
        .bind(application_key)
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
                    AND NEW.action_type IN ('application.enabled', 'application.disabled') THEN
                     PERFORM nextval(TG_ARGV[1]::regclass);
                     RAISE EXCEPTION 'injected application enablement audit failure';
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
        let enable_uri = format!("/api/v1/admin/applications/{application_key}/enable");
        let failed_enable = send(&app, Method::POST, enable_uri.clone(), &bearer).await?;
        require_test(
            failed_enable.status() == StatusCode::INTERNAL_SERVER_ERROR,
            "application enable should fail when its audit insert fails",
        )?;
        let enabled_after_failure: bool = sqlx::query_scalar(
            "SELECT enabled FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(tenant_id)
        .bind(application_key)
        .fetch_one(&pool)
        .await?;
        require_test(
            !enabled_after_failure,
            "failed enable audit insert must roll back the application status",
        )?;
        let root_folders_after_failure: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM folders
             WHERE owner_id = $1 AND tenant_id = $2 AND name IN ('Workspace', 'Archive')",
        )
        .bind(actor_id)
        .bind(tenant_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            root_folders_after_failure == 2,
            "folder provisioning should remain an explicit retry-safe side effect",
        )?;
        let folder_events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM events WHERE user_id = $1 AND event_type = '{\"type\":\"FolderCreated\"}'",
        )
        .bind(actor_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            folder_events == 2,
            "root-folder creation and its events should commit before enablement",
        )?;
        require_test(
            audit_rows(&pool, actor_id, "application.enabled")
                .await?
                .is_empty(),
            "failed enable must not leave an application.enabled audit row",
        )?;
        let sequence_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        require_test(
            sequence_marker == 1,
            "enable audit failure trigger did not fire",
        )?;

        drop_failure_trigger(&pool, &trigger_name).await?;
        let enabled = send(&app, Method::POST, enable_uri, &bearer).await?;
        require_test(
            enabled.status() == StatusCode::OK,
            "application enable retry should succeed",
        )?;
        let enabled_body: Value =
            serde_json::from_slice(&axum::body::to_bytes(enabled.into_body(), usize::MAX).await?)?;
        let application_id = enabled_body
            .get("id")
            .and_then(Value::as_str)
            .ok_or("enabled application response has no id")?
            .parse::<Uuid>()?;
        require_test(
            enabled_body.get("enabled").and_then(Value::as_bool) == Some(true),
            "successful enable response must report enabled state",
        )?;
        require_test(
            audit_rows(&pool, actor_id, "application.enabled").await?
                == vec![(
                    Some("application".to_string()),
                    Some(application_id),
                    serde_json::json!({ "application_id": application_key }),
                )],
            "successful enable must preserve its existing audit contract",
        )?;
        let folders_after_retry: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM folders
             WHERE owner_id = $1 AND tenant_id = $2 AND name IN ('Workspace', 'Archive')",
        )
        .bind(actor_id)
        .bind(tenant_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            folders_after_retry == 2,
            "enable retry should reuse already provisioned root folders",
        )?;

        let unsupported_nested_update = send_json(
            &app,
            Method::PATCH,
            format!("/api/v1/admin/applications/{application_key}"),
            &bearer,
            &serde_json::json!({ "root_path": "/Workspace/Updated/Notes" }),
        )
        .await?;
        require_test(
            unsupported_nested_update.status() == StatusCode::BAD_REQUEST,
            "new nested root paths must be rejected until their storage mapping is supported",
        )?;

        let updated_root_path = "/Workspace/Updated";
        let updated = send_json(
            &app,
            Method::PATCH,
            format!("/api/v1/admin/applications/{application_key}"),
            &bearer,
            &serde_json::json!({ "root_path": updated_root_path }),
        )
        .await?;
        require_test(
            updated.status() == StatusCode::OK,
            "application root-path update should succeed after provisioning the requested path",
        )?;
        let updated_body: Value =
            serde_json::from_slice(&axum::body::to_bytes(updated.into_body(), usize::MAX).await?)?;
        require_test(
            updated_body.get("root_path").and_then(Value::as_str) == Some(updated_root_path),
            "application update response should retain the configured root path",
        )?;
        let updated_path_folders: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM folders root
             JOIN folders workspace ON workspace.id = root.parent_folder_id
             WHERE root.name = 'Updated' AND workspace.name = 'Workspace'
               AND root.owner_id = $1 AND root.tenant_id = $2",
        )
        .bind(actor_id)
        .bind(tenant_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            updated_path_folders == 1,
            "updating an enabled application's root path should provision its folder before commit",
        )?;

        install_failure_trigger(
            &pool,
            &trigger_name,
            &function_name,
            actor_id,
            &sequence_name,
        )
        .await?;
        let disable_uri = format!("/api/v1/admin/applications/{application_key}/disable");
        let failed_disable = send(&app, Method::POST, disable_uri.clone(), &bearer).await?;
        require_test(
            failed_disable.status() == StatusCode::INTERNAL_SERVER_ERROR,
            "application disable should fail when its audit insert fails",
        )?;
        let enabled_after_disable_failure: bool = sqlx::query_scalar(
            "SELECT enabled FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(tenant_id)
        .bind(application_key)
        .fetch_one(&pool)
        .await?;
        require_test(
            enabled_after_disable_failure,
            "failed disable audit insert must roll back the application status",
        )?;
        require_test(
            audit_rows(&pool, actor_id, "application.disabled")
                .await?
                .is_empty(),
            "failed disable must not leave an application.disabled audit row",
        )?;
        let sequence_marker: i64 =
            sqlx::query_scalar(&format!("SELECT last_value FROM {sequence_name}"))
                .fetch_one(&pool)
                .await?;
        require_test(
            sequence_marker == 2,
            "disable audit failure trigger did not fire",
        )?;

        drop_failure_trigger(&pool, &trigger_name).await?;
        let disabled = send(&app, Method::POST, disable_uri, &bearer).await?;
        require_test(
            disabled.status() == StatusCode::OK,
            "application disable retry should succeed",
        )?;
        let disabled_body: Value =
            serde_json::from_slice(&axum::body::to_bytes(disabled.into_body(), usize::MAX).await?)?;
        let disabled_application_id = disabled_body
            .get("id")
            .and_then(Value::as_str)
            .ok_or("disabled application response has no id")?
            .parse::<Uuid>()?;
        require_test(
            disabled_body.get("enabled").and_then(Value::as_bool) == Some(false),
            "successful disable response must report disabled state",
        )?;
        let disable_audit_rows = audit_rows(&pool, actor_id, "application.disabled").await?;
        require_test(
            disable_audit_rows == vec![(
                    Some("application".to_string()),
                    Some(disabled_application_id),
                    serde_json::json!({ "application_id": application_key }),
                )],
            "successful disable must preserve its existing audit contract",
        )?;
        let enabled_after_disable: bool = sqlx::query_scalar(
            "SELECT enabled FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(tenant_id)
        .bind(application_key)
        .fetch_one(&pool)
        .await?;
        require_test(
            !enabled_after_disable,
            "successful disable must persist disabled state",
        )?;
        let folders_after_disable: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM folders root
             JOIN folders workspace ON workspace.id = root.parent_folder_id
             WHERE root.name = 'Updated' AND workspace.name = 'Workspace'
               AND root.owner_id = $1 AND root.tenant_id = $2",
        )
        .bind(actor_id)
        .bind(tenant_id)
        .fetch_one(&pool)
        .await?;
        require_test(
            folders_after_disable == 1,
            "disabling an application must not delete its configured root folder",
        )?;
        let other_tenant_still_enabled: bool = sqlx::query_scalar(
            "SELECT enabled FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(other_tenant_id)
        .bind(application_key)
        .fetch_one(&pool)
        .await?;
        require_test(
            other_tenant_still_enabled,
            "application state mutation must not affect a different tenant",
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
        format!(
            "DELETE FROM application_enablements WHERE tenant_id IN ('{tenant_id}', '{other_tenant_id}')"
        ),
        format!("DELETE FROM users WHERE id = '{actor_id}'"),
        format!("DELETE FROM tenants WHERE id IN ('{tenant_id}', '{other_tenant_id}')"),
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
