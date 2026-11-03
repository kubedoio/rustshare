use sqlx::PgPool;

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn operations_table_is_tenant_scoped_and_cannot_hold_plaintext() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point at a disposable PostgreSQL database");
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("../migrations").run(&pool).await.unwrap();

    let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
        .bind("managed_chat_identity_operations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        exists,
        "managed_chat_identity_operations table is missing; run the Phase 2B.2 Task 1 migration"
    );

    // No bytea / no free-form large text columns: key material cannot be persisted.
    let plaintext_capable: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns \
         WHERE table_name='managed_chat_identity_operations' \
           AND (data_type IN ('bytea','text') AND column_name NOT IN ('status','error_code','admission_event_id'))",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(plaintext_capable, 0);

    let rls: bool = sqlx::query_scalar(
        "SELECT relrowsecurity AND relforcerowsecurity \
         FROM pg_class WHERE relname='managed_chat_identity_operations'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(rls);
    pool.close().await;
}
