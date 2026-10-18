//! PostgreSQL regressions for installation-operator managed tenant admin grants.
//!
//! Run explicitly against disposable local PostgreSQL and S3-compatible test
//! infrastructure with `RUSTSHARE_TEST_DISPOSABLE_DB=1` and
//! `RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1`.

mod support;

use axum::{body::Body, http::Request, Router};
use serde_json::Value;
use sqlx::PgPool;
use support::calendar_harness::{assert_local_database, create_auth_token, setup_test_env, SERIAL};
use tower::ServiceExt;
use uuid::Uuid;

fn assert_disposable_database(database_url: &str) {
    let parsed = url::Url::parse(database_url).expect("DATABASE_URL must be a PostgreSQL URL");
    let database = parsed.path().trim_start_matches('/');
    assert!(
        database == "rustshare_test" || database.starts_with("rustshare_test_"),
        "refusing writes outside a rustshare_test database"
    );
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB")
            .ok()
            .as_deref(),
        Some("1")
    );
}

async fn create_user(pool: &PgPool, tenant_id: Uuid, is_admin: bool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-hash', $2, $4, 10737418240, $5)",
    )
    .bind(id)
    .bind(format!("grant_test_{}", id.simple()))
    .bind(format!("grant_test_{}@test.local", id.simple()))
    .bind(is_admin)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("create test user");
    id
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    request_id: Option<&str>,
) -> (axum::http::StatusCode, Value) {
    send_with_json(app, method, uri, token, request_id, None).await
}

async fn send_json(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    request_id: Option<&str>,
    body: Value,
) -> (axum::http::StatusCode, Value) {
    send_with_json(app, method, uri, token, request_id, Some(body)).await
}

async fn send_with_json(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    request_id: Option<&str>,
    body: Option<Value>,
) -> (axum::http::StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"));
    if let Some(request_id) = request_id {
        request = request.header("x-request-id", request_id);
    }
    let request = if body.is_some() {
        request.header(axum::http::header::CONTENT_TYPE, "application/json")
    } else {
        request
    };
    let request_body = body.map_or_else(Body::empty, |body| Body::from(body.to_string()));
    let response = app
        .clone()
        .oneshot(request.body(request_body).expect("build request"))
        .await
        .expect("send request");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .expect("read response body");
    let json = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).expect("response body is JSON")
    };
    (status, json)
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn organization_administrator_grants_are_scoped_audited_and_recoverable() {
    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL to a disposable local PostgreSQL database");
    assert_disposable_database(&database_url);
    assert_local_database();
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE")
            .ok()
            .as_deref(),
        Some("1"),
        "set this only for a disposable test object store"
    );
    let endpoint = std::env::var("S3_ENDPOINT")
        .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
        .expect("set an explicit local S3_ENDPOINT or RUSTFS_ENDPOINT");
    let endpoint = url::Url::parse(&endpoint).expect("S3 endpoint must be a URL");
    let bucket = std::env::var("S3_BUCKET")
        .or_else(|_| std::env::var("RUSTFS_BUCKET"))
        .expect("set an explicit rustshare-test bucket");
    assert!(
        matches!(endpoint.host_str(), Some("localhost" | "127.0.0.1"))
            && matches!(endpoint.scheme(), "http" | "https")
            && (bucket == "rustshare-test"
                || bucket.starts_with("rustshare-test-")
                || bucket.starts_with("rustshare-test_")),
        "refusing object-store writes outside a loopback rustshare-test bucket"
    );

    let mut state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant_id = Uuid::new_v4();
    let foreign_tenant_id = Uuid::new_v4();
    let nil_tenant_id = Uuid::nil();
    let operator_id = create_user(&pool, tenant_id, true).await;
    let member_id = create_user(&pool, tenant_id, false).await;
    let successor_id = create_user(&pool, tenant_id, false).await;
    let recovery_id = create_user(&pool, tenant_id, false).await;
    let third_id = create_user(&pool, tenant_id, false).await;
    let foreign_member_id = create_user(&pool, foreign_tenant_id, false).await;
    let nil_member_id = create_user(&pool, nil_tenant_id, false).await;
    let ordinary_operator_id = create_user(&pool, tenant_id, false).await;
    let operator_token = create_auth_token(&state, operator_id, tenant_id);
    let ordinary_token = create_auth_token(&state, ordinary_operator_id, tenant_id);
    state.default_tenant_config = rustshare_server::config::DefaultTenantConfig::Set(tenant_id);
    let app = rustshare_server::routes::admin_routes().with_state(state.clone());
    let grant_uri = |target_id: Uuid| {
        format!("/api/v1/admin/organizations/{tenant_id}/administrators/{target_id}")
    };

    let (status, grant) = send_json(
        &app,
        "POST",
        &grant_uri(member_id),
        &operator_token,
        Some("grant-start-01"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    assert_eq!(grant["tenant_id"], tenant_id.to_string());
    assert_eq!(grant["user_id"], member_id.to_string());
    assert_eq!(grant["granted_by"], operator_id.to_string());
    assert_eq!(
        sqlx::query_as::<
            _,
            (
                Option<Uuid>,
                Uuid,
                Option<String>,
                String,
                serde_json::Value
            ),
        >(
            "SELECT tenant_id, actor_principal_id, correlation_id, action_type, detail
             FROM admin_actions WHERE target_id = $1 ORDER BY performed_at DESC LIMIT 1",
        )
        .bind(member_id)
        .fetch_one(&pool)
        .await
        .expect("read bootstrap audit"),
        (
            Some(tenant_id),
            operator_id,
            Some("grant-start-01".to_owned()),
            "organization_admin.bootstrap".to_owned(),
            serde_json::json!({ "grant_id": grant["grant_id"] })
        )
    );

    let (status, _) = send_json(
        &app,
        "POST",
        &grant_uri(member_id),
        &operator_token,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM admin_actions WHERE target_id = $1 AND action_type LIKE 'organization_admin.%'",
        )
        .bind(member_id)
        .fetch_one(&pool)
        .await
        .expect("count grant audit records"),
        1,
        "failed grant attempts do not claim successful audit outcomes"
    );

    let (status, _) = send_json(
        &app,
        "POST",
        &grant_uri(foreign_member_id),
        &operator_token,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    let (status, _) = send_json(
        &app,
        "POST",
        &grant_uri(successor_id),
        &ordinary_token,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
    let (status, _) = send_json(
        &app,
        "POST",
        &grant_uri(operator_id),
        &operator_token,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);

    let (status, _) = send(&app, "DELETE", &grant_uri(member_id), &operator_token, None).await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM organization_administrators
             WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(tenant_id)
        .bind(member_id)
        .fetch_one(&pool)
        .await
        .expect("check last-admin grant"),
        1
    );

    assert_eq!(
        send_json(
            &app,
            "POST",
            &grant_uri(successor_id),
            &operator_token,
            None,
            serde_json::json!({})
        )
        .await
        .0,
        axum::http::StatusCode::CREATED
    );
    let (status, list) = send(
        &app,
        "GET",
        &format!("/api/v1/admin/organizations/{tenant_id}/administrators?page=1&per_page=1"),
        &operator_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(list["total"], 2);
    assert_eq!(list["per_page"], 1);
    assert_eq!(list["administrators"].as_array().map(Vec::len), Some(1));
    let (status, foreign_list) = send(
        &app,
        "GET",
        &format!("/api/v1/admin/organizations/{foreign_tenant_id}/administrators"),
        &operator_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(foreign_list["total"], 0);
    assert_eq!(
        foreign_list["administrators"].as_array().map(Vec::len),
        Some(0)
    );
    assert_eq!(
        send(&app, "DELETE", &grant_uri(member_id), &operator_token, None)
            .await
            .0,
        axum::http::StatusCode::NO_CONTENT
    );
    let (status, _) = send_json(
        &app,
        "POST",
        &grant_uri(successor_id),
        &operator_token,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);

    // Disabling the final remaining admin is rejected; after an operator-side
    // recovery grant exists, disabling an admin revokes its grant atomically.
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/admin/users/{successor_id}/disable"),
            &operator_token,
            None,
        )
        .await
        .0,
        axum::http::StatusCode::CONFLICT
    );
    // Model a pre-existing disabled sole administrator (for example, a
    // recovery incident on a deployment before the guard shipped).
    sqlx::query("UPDATE users SET disabled_at = NOW() WHERE id = $1")
        .bind(successor_id)
        .execute(&pool)
        .await
        .expect("simulate disabled administrator needing recovery");
    assert_eq!(
        send_json(
            &app,
            "POST",
            &grant_uri(recovery_id),
            &operator_token,
            None,
            serde_json::json!({}),
        )
        .await
        .0,
        axum::http::StatusCode::BAD_REQUEST
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM organization_administrators
             WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(tenant_id)
        .bind(recovery_id)
        .fetch_one(&pool)
        .await
        .expect("check rejected recovery grant"),
        0,
        "a recovery grant without a reason must not be persisted"
    );
    assert_eq!(
        send_json(
            &app,
            "POST",
            &grant_uri(recovery_id),
            &operator_token,
            None,
            serde_json::json!({ "recovery_reason": "administrator_unavailable" }),
        )
        .await
        .0,
        axum::http::StatusCode::CREATED
    );
    assert_eq!(
        send_json(
            &app,
            "POST",
            &grant_uri(third_id),
            &operator_token,
            None,
            serde_json::json!({}),
        )
        .await
        .0,
        axum::http::StatusCode::CREATED
    );
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/admin/users/{recovery_id}/disable"),
            &operator_token,
            None,
        )
        .await
        .0,
        axum::http::StatusCode::NO_CONTENT
    );
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT revoked_at IS NOT NULL FROM organization_administrators
             WHERE tenant_id = $1 AND user_id = $2 ORDER BY granted_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(recovery_id)
    .fetch_one(&pool)
    .await
    .expect("disabled administrator grant must be revoked"));
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT detail->>'recovery_reason' FROM admin_actions
             WHERE target_id = $1 AND action_type = 'organization_admin.recovered'
             ORDER BY performed_at DESC LIMIT 1",
        )
        .bind(recovery_id)
        .fetch_one(&pool)
        .await
        .expect("read recovery reason"),
        "administrator_unavailable"
    );

    // Nil remains a concrete tenant for grants, just as for principals.
    let nil_uri =
        format!("/api/v1/admin/organizations/{nil_tenant_id}/administrators/{nil_member_id}");
    assert_eq!(
        send_json(
            &app,
            "POST",
            &nil_uri,
            &operator_token,
            None,
            serde_json::json!({}),
        )
        .await
        .0,
        axum::http::StatusCode::CREATED
    );

    let mut unaudited_revoke = pool.begin().await.expect("begin unaudited revoke");
    sqlx::query(
        "UPDATE organization_administrators
         SET revoked_by = $3, revoked_at = NOW(), revocation_source = 'operator'
         WHERE tenant_id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(nil_tenant_id)
    .bind(nil_member_id)
    .bind(operator_id)
    .execute(&mut *unaudited_revoke)
    .await
    .expect("write test revoke before deferred audit check");
    let audit_integrity_error = unaudited_revoke
        .commit()
        .await
        .expect_err("revocation without mandatory audit evidence must not commit");
    assert!(audit_integrity_error
        .to_string()
        .contains("requires a matching audit record"));
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT revoked_at IS NULL FROM organization_administrators
         WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(nil_tenant_id)
    .bind(nil_member_id)
    .fetch_one(&pool)
    .await
    .expect("check unaudited revoke rollback"));

    // Grant persistence and its required audit event share a transaction.
    let failing_member_id = create_user(&pool, tenant_id, false).await;
    let trigger_suffix = Uuid::new_v4().simple().to_string();
    let function = format!("reject_org_admin_audit_{trigger_suffix}");
    let trigger = format!("reject_org_admin_audit_{trigger_suffix}");
    let sequence = format!("reject_org_admin_audit_seq_{trigger_suffix}");
    sqlx::query(&format!(
        "CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
           IF NEW.action_type = 'organization_admin.granted' AND NEW.target_id = '{failing_member_id}'::uuid THEN
             PERFORM nextval('{sequence}');
             RAISE EXCEPTION 'injected audit failure';
           END IF;
           RETURN NEW;
         END; $$"
    ))
    .execute(&pool)
    .await
    .expect("create audit failure trigger function");
    sqlx::query(&format!("CREATE SEQUENCE {sequence}"))
        .execute(&pool)
        .await
        .expect("create audit failure sequence");
    sqlx::query(&format!(
        "CREATE TRIGGER {trigger} BEFORE INSERT ON admin_actions
         FOR EACH ROW EXECUTE FUNCTION {function}()"
    ))
    .execute(&pool)
    .await
    .expect("create audit failure trigger");
    let failed_grant_status = send_json(
        &app,
        "POST",
        &grant_uri(failing_member_id),
        &operator_token,
        None,
        serde_json::json!({}),
    )
    .await
    .0;
    let grant_and_audit_rolled_back = sqlx::query_scalar::<_, bool>(
        "SELECT NOT EXISTS(SELECT 1 FROM organization_administrators
                           WHERE tenant_id = $1 AND user_id = $2)
                AND NOT EXISTS(SELECT 1 FROM admin_actions
                               WHERE target_id = $2 AND action_type LIKE 'organization_admin.%')",
    )
    .bind(tenant_id)
    .bind(failing_member_id)
    .fetch_one(&pool)
    .await
    .expect("check rollback after audit failure");

    // Remove the injected failure even when the behavior under test regresses.
    sqlx::query(&format!("DROP TRIGGER {trigger} ON admin_actions"))
        .execute(&pool)
        .await
        .expect("drop audit failure trigger");
    sqlx::query(&format!("DROP FUNCTION {function}()"))
        .execute(&pool)
        .await
        .expect("drop audit failure function");
    sqlx::query(&format!("DROP SEQUENCE {sequence}"))
        .execute(&pool)
        .await
        .expect("drop audit failure sequence");
    assert_eq!(
        failed_grant_status,
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(grant_and_audit_rolled_back);

    // Direct rewrites and deletes cannot erase grant history.
    let integrity_error = sqlx::query(
        "DELETE FROM organization_administrators WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(nil_tenant_id)
    .bind(nil_member_id)
    .execute(&pool)
    .await
    .expect_err("grant history must reject deletion");
    assert!(integrity_error.to_string().contains("append-only"));
}

#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn organization_admin_http_scope_and_immediate_revocation() {
    let _serial = SERIAL.lock().await;
    let mut state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let tenant = Uuid::new_v4();
    let foreign_tenant = Uuid::new_v4();
    let operator = create_user(&pool, tenant, true).await;
    let org_admin = create_user(&pool, tenant, false).await;
    let member = create_user(&pool, tenant, false).await;
    let foreign = create_user(&pool, foreign_tenant, false).await;
    let nil_admin = create_user(&pool, Uuid::nil(), false).await;
    let nil_member = create_user(&pool, Uuid::nil(), false).await;
    let op_token = create_auth_token(&state, operator, tenant);
    let org_token = create_auth_token(&state, org_admin, tenant);
    let nil_token = create_auth_token(&state, nil_admin, Uuid::nil());
    state.default_tenant_config = rustshare_server::config::DefaultTenantConfig::Set(tenant);
    let app = rustshare_server::routes::admin_routes().with_state(state.clone());

    let (status, _) = send_json(
        &app,
        "POST",
        &format!("/api/v1/admin/organizations/{tenant}/administrators/{org_admin}"),
        &op_token,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let (status, _) = send_json(
        &app,
        "POST",
        &format!("/api/v1/admin/organizations/{tenant}/administrators/{member}"),
        &op_token,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let (status, users) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/users?per_page=100",
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(users["users"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u["id"] == member.to_string()));
    assert!(!users["users"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u["id"] == foreign.to_string()));
    assert!(users["users"]
        .as_array()
        .unwrap()
        .iter()
        .all(|u| u.get("is_admin").is_none()));
    let (status, _) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/users",
        &op_token,
        None,
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::FORBIDDEN,
        "installation operator status alone does not grant tenant administration"
    );
    let (status, _) = send(
        &app,
        "GET",
        "/api/v1/admin/config/security",
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
    let (status, _) = send_json(
        &app,
        "PATCH",
        &format!("/api/v1/admin/users/{member}"),
        &org_token,
        None,
        serde_json::json!({"is_admin":true}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
    let (status, _) = send_json(
        &app,
        "POST",
        &format!("/api/v1/admin/organizations/{tenant}/administrators/{member}"),
        &org_token,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
    let (status, _) = send(
        &app,
        "DELETE",
        &format!("/api/v1/admin/organizations/{tenant}/administrators/{org_admin}"),
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);

    let (status, _) = send_json(
        &app,
        "POST",
        "/api/v1/organization-admin/groups",
        &org_token,
        None,
        serde_json::json!({"name":format!("org-group-{tenant}"),"description":"test"}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let (status, paged_groups) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/groups?per_page=1",
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(paged_groups["groups"].as_array().unwrap().len(), 1);
    assert_eq!(paged_groups["per_page"], 1);
    let (status, _) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/groups?page=100001&per_page=100",
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    let group_id: Uuid =
        sqlx::query_scalar("SELECT id FROM user_groups WHERE tenant_id=$1 AND name=$2")
            .bind(tenant)
            .bind(format!("org-group-{tenant}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    let (status, _) = send_json(
        &app,
        "POST",
        "/api/v1/organization-admin/groups",
        &org_token,
        None,
        serde_json::json!({"name":format!("org-group-{tenant}")}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    let (status, _) = send_json(
        &app,
        "POST",
        "/api/v1/organization-admin/groups",
        &org_token,
        None,
        serde_json::json!({"name":format!("other-group-{tenant}")}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let other_group_id: Uuid =
        sqlx::query_scalar("SELECT id FROM user_groups WHERE tenant_id=$1 AND name=$2")
            .bind(tenant)
            .bind(format!("other-group-{tenant}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    let (status, _) = send_json(
        &app,
        "PATCH",
        &format!("/api/v1/organization-admin/groups/{other_group_id}"),
        &org_token,
        None,
        serde_json::json!({"name":format!("org-group-{tenant}")}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    let (status, _) = send(
        &app,
        "DELETE",
        &format!("/api/v1/organization-admin/groups/{other_group_id}"),
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    let foreign_group_name = format!("foreign-collision-{tenant}");
    sqlx::query("INSERT INTO user_groups (id,name,tenant_id,created_by) VALUES ($1,$2,$3,$4)")
        .bind(Uuid::new_v4())
        .bind(&foreign_group_name)
        .bind(foreign_tenant)
        .bind(foreign)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send_json(
        &app,
        "POST",
        "/api/v1/organization-admin/groups",
        &org_token,
        None,
        serde_json::json!({"name":foreign_group_name}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    let (status, _) = send_json(
        &app,
        "POST",
        "/api/v1/organization-admin/groups",
        &org_token,
        None,
        serde_json::json!({"name":format!("foreign-rename-target-{tenant}")}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let rename_target_id: Uuid =
        sqlx::query_scalar("SELECT id FROM user_groups WHERE tenant_id=$1 AND name=$2")
            .bind(tenant)
            .bind(format!("foreign-rename-target-{tenant}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    let (status, _) = send_json(
        &app,
        "PATCH",
        &format!("/api/v1/organization-admin/groups/{rename_target_id}"),
        &org_token,
        None,
        serde_json::json!({"name":foreign_group_name}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM admin_actions WHERE tenant_id=$1 AND action_type IN ('group.created','group.updated') AND detail->>'name'=$2")
            .bind(tenant)
            .bind(&foreign_group_name)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
        "failed cross-tenant name collisions do not create local audit events"
    );
    let (status, _) = send(
        &app,
        "DELETE",
        &format!("/api/v1/organization-admin/groups/{rename_target_id}"),
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    let private_file_id = Uuid::new_v4();
    sqlx::query("INSERT INTO files (id,name,path,size,mime_type,content_hash,storage_key,owner_id,tenant_id) VALUES ($1,'group-private.txt','/group-private.txt',5,'text/plain','hash','key',$2,$3)")
        .bind(private_file_id).bind(member).bind(tenant)
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO shares (id,file_id,share_token,created_by,permissions,recipient_group_id,tenant_id) VALUES ($1,$2,NULL,$3,'View',$4,$5)")
        .bind(Uuid::new_v4()).bind(private_file_id).bind(member).bind(group_id).bind(tenant)
        .execute(&pool).await.unwrap();
    let (status, _) = send(
        &app,
        "GET",
        &format!("/api/v1/files/{private_file_id}"),
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    let (status, _) = send_json(
        &app,
        "POST",
        &format!("/api/v1/organization-admin/groups/{group_id}/members"),
        &org_token,
        None,
        serde_json::json!({"user_id":org_admin}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM group_members WHERE group_id=$1 AND user_id=$2"
        )
        .bind(group_id)
        .bind(org_admin)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    let (status, _) = send(
        &app,
        "GET",
        &format!("/api/v1/files/{private_file_id}"),
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    sqlx::query("INSERT INTO admin_actions (tenant_id,actor_id,actor_principal_id,action_type,detail) VALUES ($1,$2,$2,'config.security_updated',$3)")
        .bind(tenant).bind(operator).bind(serde_json::json!({"login_protection_enabled":true,"max_login_attempts":2}))
        .execute(&pool).await.unwrap();
    let (status, audit) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/audit?per_page=100",
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(!audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action_type"]
            .as_str()
            .unwrap_or("")
            .starts_with("config.")));
    let (status, operator_audit) = send(
        &app,
        "GET",
        "/api/v1/admin/audit?type=admin_action",
        &op_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(operator_audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action_type"] == "config.security_updated"));
    let (status, _) = send_json(
        &app,
        "POST",
        &format!("/api/v1/organization-admin/groups/{group_id}/members"),
        &org_token,
        None,
        serde_json::json!({"user_id":foreign}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    let (status, _) = send_json(
        &app,
        "POST",
        &format!("/api/v1/organization-admin/groups/{group_id}/members"),
        &org_token,
        None,
        serde_json::json!({"user_id":member}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    let (status, group_detail) = send(
        &app,
        "GET",
        &format!("/api/v1/organization-admin/groups/{group_id}?per_page=1"),
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(group_detail["members"].as_array().unwrap().len(), 1);
    let (status, members_page) = send(
        &app,
        "GET",
        &format!("/api/v1/organization-admin/groups/{group_id}/members?per_page=1"),
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(members_page["members"].as_array().unwrap().len(), 1);
    let (status, _) = send_json(
        &app,
        "PATCH",
        &format!("/api/v1/organization-admin/groups/{group_id}"),
        &org_token,
        None,
        serde_json::json!({"name":format!("renamed-org-group-{tenant}")}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>("SELECT description FROM user_groups WHERE id=$1")
            .bind(group_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        Some("test".to_owned()),
        "omitting description preserves its current value"
    );
    let (status, _) = send_json(
        &app,
        "PATCH",
        &format!("/api/v1/organization-admin/groups/{group_id}"),
        &org_token,
        None,
        serde_json::json!({"description":null}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(
        sqlx::query_scalar::<_, Option<String>>("SELECT description FROM user_groups WHERE id=$1")
            .bind(group_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        None,
        "explicit null clears the description"
    );
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM admin_actions WHERE tenant_id=$1 AND action_type='group.member_added' AND target_id=$2").bind(tenant).bind(group_id).fetch_one(&pool).await.unwrap(),1);
    let (status, audit) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/audit?per_page=100",
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action_type"] == "group.member_added"
            && e["detail"]["user_id"] == member.to_string()));
    let (status, _) = send(
        &app,
        "DELETE",
        &format!("/api/v1/organization-admin/groups/{group_id}/members/{member}"),
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    let (status, audit) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/audit?per_page=100",
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action_type"] == "group.member_removed"
            && e["detail"]["user_id"] == member.to_string()));
    let (status, _) = send(
        &app,
        "DELETE",
        &format!("/api/v1/organization-admin/groups/{group_id}"),
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    let (status, audit) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/audit?per_page=100",
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["action_type"] == "group.deleted"));

    sqlx::query("UPDATE users SET disabled_at=now() WHERE id=$1")
        .bind(org_admin)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/users",
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
    sqlx::query("UPDATE users SET disabled_at=NULL WHERE id=$1")
        .bind(org_admin)
        .execute(&pool)
        .await
        .unwrap();
    let (status, _) = send_json(
        &app,
        "POST",
        &format!(
            "/api/v1/admin/organizations/{}/administrators/{}",
            Uuid::nil(),
            nil_admin
        ),
        &op_token,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let (status, _) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/users",
        &nil_token,
        None,
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "nil tenant remains a valid tenant scope"
    );
    let (status, _) = send_json(
        &app,
        "POST",
        &format!(
            "/api/v1/admin/organizations/{}/administrators/{}",
            Uuid::nil(),
            nil_member
        ),
        &op_token,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED);

    let (status, _) = send(
        &app,
        "DELETE",
        &format!("/api/v1/admin/organizations/{tenant}/administrators/{org_admin}"),
        &op_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);
    let (status, _) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/users",
        &org_token,
        None,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED);
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let fresh_org_token = rustshare_server::web_session::generate_user_jwt(
        &state,
        org_admin,
        "test@example.com",
        tenant,
    )
    .await
    .expect("reactivated organization admin receives a fresh JWT");
    let (status, _) = send(
        &app,
        "GET",
        "/api/v1/organization-admin/users",
        &fresh_org_token,
        None,
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::FORBIDDEN,
        "revocation must apply on the next request"
    );
}
