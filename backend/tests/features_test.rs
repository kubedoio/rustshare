use axum::response::IntoResponse;
use rustshare_infrastructure::repositories::WorkflowRepository;
use rustshare_server::handlers::AppError;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    PgPool::connect(&url).await.expect("connect to PostgreSQL")
}

#[tokio::test]
async fn invite_feature_reflects_current_installation_workflow_state() {
    let admin_pool = pool().await;
    let schema = format!("features_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin_pool)
        .await
        .expect("create isolated test schema");

    let schema_for_connection = schema.clone();
    let test_pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |connection, _| {
            let search_path = format!("SET search_path TO {schema_for_connection}");
            Box::pin(async move {
                sqlx::query(&search_path).execute(connection).await?;
                Ok(())
            })
        })
        .connect(&std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://rustshare:changeme@localhost:5432/rustshare".to_string()
        }))
        .await
        .expect("connect to isolated test schema");
    sqlx::query(
        "CREATE TABLE workflows (tenant_id UUID NOT NULL, key TEXT NOT NULL, status TEXT NOT NULL)",
    )
    .execute(&test_pool)
    .await
    .expect("create workflow fixture table");

    let repository = WorkflowRepository::new(test_pool.clone());
    assert!(
        !repository.invite_enabled().await.unwrap(),
        "missing is disabled"
    );

    let tenant_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workflows (tenant_id, key, status) VALUES ($1, 'invite_email', 'draft')",
    )
    .bind(tenant_id)
    .execute(&test_pool)
    .await
    .unwrap();
    assert!(
        !repository.invite_enabled().await.unwrap(),
        "draft is disabled"
    );

    sqlx::query("UPDATE workflows SET status = 'active' WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&test_pool)
        .await
        .unwrap();
    assert!(
        repository.invite_enabled().await.unwrap(),
        "active is enabled for any tenant"
    );

    sqlx::query("UPDATE workflows SET status = 'draft' WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&test_pool)
        .await
        .unwrap();
    assert!(
        !repository.invite_enabled().await.unwrap(),
        "subsequent reads see state changes"
    );

    sqlx::query("DROP TABLE workflows")
        .execute(&test_pool)
        .await
        .unwrap();
    let error = repository.invite_enabled().await.unwrap_err();
    let response = AppError::from(error).into_response();
    assert_eq!(
        response.status(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("Internal server error"));
    assert!(!String::from_utf8_lossy(&body).contains("workflows"));

    test_pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin_pool)
        .await
        .expect("drop isolated test schema");
}
