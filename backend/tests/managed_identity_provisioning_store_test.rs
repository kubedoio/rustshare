use sqlx::PgPool;

const TABLE: &str = "managed_chat_identity_operations";
const TENANT_POLICY: &str = "managed_chat_identity_operations_tenant_isolation";
/// The only columns allowed to carry text; each is a bounded catalog value or
/// a fixed-format event id, so key material cannot be persisted.
const ALLOWED_TEXT_COLUMNS: [&str; 3] = ["admission_event_id", "error_code", "status"];

#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn operations_table_is_tenant_scoped_and_cannot_hold_plaintext() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point at a disposable PostgreSQL database");
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("../migrations").run(&pool).await.unwrap();

    let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
        .bind(TABLE)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        exists,
        "{TABLE} table is missing; run the Phase 2B.2 Task 1 migration"
    );

    // No bytea and no free-form text, varchar, json, or text-array columns:
    // the only text-capable columns are the pinned allowlist.
    let plaintext_capable: Vec<(String, String)> = sqlx::query_as(
        "SELECT column_name, data_type \
         FROM information_schema.columns \
         WHERE table_schema = current_schema() AND table_name = $1 \
           AND (data_type IN ('bytea','text','character varying','character','json','jsonb') \
                OR (data_type = 'ARRAY' AND udt_name IN ('_text','_varchar','_json','_jsonb'))) \
         ORDER BY column_name",
    )
    .bind(TABLE)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        plaintext_capable
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ALLOWED_TEXT_COLUMNS,
        "unexpected plaintext-capable columns (column, type): {plaintext_capable:?}"
    );

    let (rowsecurity, forcerowsecurity): (bool, bool) = sqlx::query_as(
        "SELECT c.relrowsecurity, c.relforcerowsecurity \
         FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = current_schema() AND c.relname = $1",
    )
    .bind(TABLE)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(rowsecurity, "RLS must be enabled on {TABLE}");
    assert!(forcerowsecurity, "RLS must be forced on {TABLE}");

    // Exactly one tenant-isolation policy, in both qual and with_check.
    let policies: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT policyname, qual, with_check \
         FROM pg_policies \
         WHERE schemaname = current_schema() AND tablename = $1 \
         ORDER BY policyname",
    )
    .bind(TABLE)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        policies.len(),
        1,
        "expected exactly one RLS policy on {TABLE}: {policies:?}"
    );
    let (policy_name, qual, with_check) = &policies[0];
    assert_eq!(policy_name, TENANT_POLICY);
    let qual = qual.as_deref().unwrap_or_default();
    let with_check = with_check.as_deref().unwrap_or_default();
    assert!(
        qual.contains("app.current_tenant_id"),
        "policy qual must reference app.current_tenant_id: {qual}"
    );
    assert!(
        with_check.contains("app.current_tenant_id"),
        "policy with_check must reference app.current_tenant_id: {with_check}"
    );
    pool.close().await;
}
