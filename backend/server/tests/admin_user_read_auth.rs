mod common;

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and RustFS"]
async fn admin_user_reads_require_installation_admin_and_preserve_system_scope() {
    let (state, _) = common::setup_test_server().await;
    let pool = state.db_pool.clone();
    let fixture = Uuid::new_v4();
    let admin_tenant = Uuid::new_v4();
    let other_tenant = Uuid::new_v4();
    insert_tenant(&pool, admin_tenant, &format!("admin-read-{fixture}")).await;
    insert_tenant(&pool, other_tenant, &format!("target-read-{fixture}")).await;

    let organization_admin = Uuid::new_v4();
    let installation_admin = Uuid::new_v4();
    let cross_tenant_target = Uuid::new_v4();
    insert_user(&pool, organization_admin, admin_tenant, false).await;
    insert_user(&pool, installation_admin, admin_tenant, true).await;
    insert_user(&pool, cross_tenant_target, other_tenant, false).await;
    sqlx::query("INSERT INTO organization_administrators (tenant_id, user_id, granted_by) VALUES ($1, $2, $2)")
        .bind(admin_tenant)
        .bind(organization_admin)
        .execute(&pool)
        .await
        .unwrap();

    let organization_admin_token = state
        .jwt_manager
        .generate(
            organization_admin,
            &format!("{organization_admin}@example.test"),
            admin_tenant,
        )
        .unwrap();
    assert_eq!(
        admin_user_read(&state, &organization_admin_token, "/api/v1/admin/users")
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        admin_user_read(
            &state,
            &organization_admin_token,
            &format!("/api/v1/admin/users/{cross_tenant_target}")
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    let admin_token = state
        .jwt_manager
        .generate(
            installation_admin,
            &format!("{installation_admin}@example.test"),
            admin_tenant,
        )
        .unwrap();
    let (list_status, list_body) = admin_user_read(
        &state,
        &admin_token,
        &format!("/api/v1/admin/users?search=user-{cross_tenant_target}&status=active"),
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(list_body["total"], 1);
    assert_eq!(list_body["users"][0]["id"], cross_tenant_target.to_string());

    let (detail_status, detail_body) = admin_user_read(
        &state,
        &admin_token,
        &format!("/api/v1/admin/users/{cross_tenant_target}"),
    )
    .await;
    assert_eq!(detail_status, StatusCode::OK);
    assert_eq!(detail_body["user"]["id"], cross_tenant_target.to_string());

    sqlx::query("DELETE FROM organization_administrators WHERE user_id = $1")
        .bind(organization_admin)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id = ANY($1)")
        .bind(vec![
            organization_admin,
            installation_admin,
            cross_tenant_target,
        ])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM tenants WHERE id = ANY($1)")
        .bind(vec![admin_tenant, other_tenant])
        .execute(&pool)
        .await
        .unwrap();
}

async fn admin_user_read(
    state: &rustshare_server::AppState,
    token: &str,
    uri: &str,
) -> (StatusCode, Value) {
    let response = rustshare_server::routes::admin_routes()
        .with_state(state.clone())
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

async fn insert_tenant(pool: &PgPool, tenant_id: Uuid, name: &str) {
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
}

async fn insert_user(pool: &PgPool, user_id: Uuid, tenant_id: Uuid, is_admin: bool) {
    let username = format!("user-{user_id}");
    let email = format!("{user_id}@example.test");
    sqlx::query("INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota, is_admin) VALUES ($1, $2, $3, 'test-hash', $4, $2, 0, $5)")
        .bind(user_id)
        .bind(username)
        .bind(email)
        .bind(tenant_id)
        .bind(is_admin)
        .execute(pool)
        .await
        .unwrap();
}
