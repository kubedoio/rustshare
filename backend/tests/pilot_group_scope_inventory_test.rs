//! PostgreSQL integration coverage for the pilot group-scope inventory query.

use sqlx::{PgPool, Row};
use uuid::Uuid;

fn inventory_select() -> String {
    let script = include_str!("../../scripts/pilot-group-scope-inventory.sql");
    let (_, query) = script
        .split_once("WITH membership_scope AS")
        .expect("inventory script contains its query");
    let (query, _) = query
        .split_once("\nROLLBACK;")
        .expect("inventory script rolls back its read-only transaction");

    format!("WITH membership_scope AS{query}")
}

fn inventory_rls_guard() -> String {
    let script = include_str!("../../scripts/pilot-group-scope-inventory.sql");
    let (_, after_begin) = script
        .split_once("BEGIN TRANSACTION READ ONLY;")
        .expect("inventory script starts a read-only transaction");
    let (setup, _) = after_begin
        .split_once("WITH membership_scope AS")
        .expect("inventory script configures its session before querying");
    setup
        .lines()
        .find(|line| line.trim() == "SET LOCAL row_security = off;")
        .expect("inventory script disables filtered RLS results")
        .trim()
        .trim_end_matches(';')
        .to_owned()
}

#[tokio::test]
#[ignore = "requires migrated PostgreSQL; run by the integration-tests workflow"]
async fn inventory_reports_scope_anomalies_and_fails_closed_under_rls() {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPool::connect(&database_url)
        .await
        .expect("connect to migrated integration database");
    let mut transaction = pool.begin().await.expect("begin fixture transaction");

    let tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("Inventory test tenant {tenant_id}"))
        .execute(&mut *transaction)
        .await
        .expect("insert fixture tenant");

    let file_tenant_id = Uuid::new_v4();
    let share_tenant_id = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2), ($3, $4)")
        .bind(file_tenant_id)
        .bind(format!("Inventory file tenant {file_tenant_id}"))
        .bind(share_tenant_id)
        .bind(format!("Inventory share tenant {share_tenant_id}"))
        .execute(&mut *transaction)
        .await
        .expect("insert resource and share tenants");

    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(user_id)
    .bind(format!("inventory-{user_id}"))
    .bind(format!("inventory-{user_id}@example.test"))
    .bind("test-only-password-hash")
    .bind(tenant_id)
    .bind("Inventory test member")
    .bind(1_048_576_i64)
    .execute(&mut *transaction)
    .await
    .expect("insert fixture user");

    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, tenant_id, created_by) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("inventory-{group_id}"))
    .bind(Uuid::nil())
    .bind(user_id)
    .execute(&mut *transaction)
    .await
    .expect("insert legacy-tenant fixture group");

    sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ($1, $2)")
        .bind(group_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .expect("insert cross-tenant fixture membership");

    let file_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO files (id, name, path, content_hash, size, mime_type, owner_id, tenant_id, current_version, storage_key) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(file_id)
    .bind("inventory-fixture.txt")
    .bind(format!("/{file_id}.txt"))
    .bind("fixture-hash")
    .bind(0_i64)
    .bind("text/plain")
    .bind(user_id)
    .bind(file_tenant_id)
    .bind(1_i32)
    .bind(format!("{file_tenant_id}/files/{file_id}"))
    .execute(&mut *transaction)
    .await
    .expect("insert cross-tenant fixture file");

    let expired_file_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO files (id, name, path, content_hash, size, mime_type, owner_id, tenant_id, current_version, storage_key) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(expired_file_id)
    .bind("inventory-expired-share.txt")
    .bind(format!("/{expired_file_id}.txt"))
    .bind("fixture-hash")
    .bind(0_i64)
    .bind("text/plain")
    .bind(user_id)
    .bind(share_tenant_id)
    .bind(1_i32)
    .bind(format!("{share_tenant_id}/files/{expired_file_id}"))
    .execute(&mut *transaction)
    .await
    .expect("insert expired-share fixture file");

    let folder_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO folders (id, name, path, owner_id, tenant_id) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(folder_id)
    .bind("inventory-fixture-folder")
    .bind(format!("/{folder_id}"))
    .bind(user_id)
    .bind(share_tenant_id)
    .execute(&mut *transaction)
    .await
    .expect("insert cross-tenant fixture folder");

    let active_share_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO shares (id, file_id, share_token, created_by, permissions, recipient_group_id, tenant_id) VALUES ($1, $2, NULL, $3, 'View', $4, $5)",
    )
    .bind(active_share_id)
    .bind(file_id)
    .bind(user_id)
    .bind(group_id)
    .bind(share_tenant_id)
    .execute(&mut *transaction)
    .await
    .expect("insert active group-share fixture");

    let expired_share_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO shares (id, file_id, share_token, created_by, permissions, expires_at, recipient_group_id, tenant_id) VALUES ($1, $2, NULL, $3, 'View', CURRENT_TIMESTAMP - INTERVAL '1 day', $4, $5)",
    )
    .bind(expired_share_id)
    .bind(expired_file_id)
    .bind(user_id)
    .bind(group_id)
    .bind(share_tenant_id)
    .execute(&mut *transaction)
    .await
    .expect("insert expired group-share fixture");

    let revoked_share_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO shares (id, file_id, share_token, created_by, permissions, revoked_at, recipient_group_id, tenant_id) VALUES ($1, $2, NULL, $3, 'View', CURRENT_TIMESTAMP, $4, $5)",
    )
    .bind(revoked_share_id)
    .bind(expired_file_id)
    .bind(user_id)
    .bind(group_id)
    .bind(share_tenant_id)
    .execute(&mut *transaction)
    .await
    .expect("insert revoked group-share fixture");

    let folder_share_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO shares (id, file_id, folder_id, share_token, created_by, permissions, recipient_group_id, tenant_id) VALUES ($1, NULL, $2, NULL, $3, 'View', $4, $5)",
    )
    .bind(folder_share_id)
    .bind(folder_id)
    .bind(user_id)
    .bind(group_id)
    .bind(share_tenant_id)
    .execute(&mut *transaction)
    .await
    .expect("insert folder group-share fixture");

    let hidden_owner_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, tenant_id, display_name, storage_quota) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(hidden_owner_id)
    .bind(format!("inventory-hidden-{hidden_owner_id}"))
    .bind(format!("inventory-hidden-{hidden_owner_id}@example.test"))
    .bind("test-only-password-hash")
    .bind(tenant_id)
    .bind("Inventory hidden owner")
    .bind(1_048_576_i64)
    .execute(&mut *transaction)
    .await
    .expect("insert a second owner for the RLS-filtering fixture");

    let hidden_file_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO files (id, name, path, content_hash, size, mime_type, owner_id, tenant_id, current_version, storage_key) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(hidden_file_id)
    .bind("inventory-hidden-owner.txt")
    .bind(format!("/{hidden_file_id}.txt"))
    .bind("hidden-owner-fixture-hash")
    .bind(0_i64)
    .bind("text/plain")
    .bind(hidden_owner_id)
    .bind(tenant_id)
    .bind(1_i32)
    .bind(format!("{tenant_id}/files/{hidden_file_id}"))
    .execute(&mut *transaction)
    .await
    .expect("insert file hidden from the selected owner");

    let hidden_folder_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO folders (id, name, path, owner_id, tenant_id) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(hidden_folder_id)
    .bind("inventory-hidden-owner-folder")
    .bind(format!("/{hidden_folder_id}"))
    .bind(hidden_owner_id)
    .bind(tenant_id)
    .execute(&mut *transaction)
    .await
    .expect("insert folder hidden from the selected owner");

    let query = format!(
        "SELECT * FROM ({}) AS inventory WHERE group_id = $1",
        inventory_select().trim().trim_end_matches(';')
    );
    sqlx::query(&inventory_rls_guard())
        .execute(&mut *transaction)
        .await
        .expect("execute the inventory script's fail-closed RLS guard");
    let row = sqlx::query(&query)
        .bind(group_id)
        .fetch_one(&mut *transaction)
        .await
        .expect("inventory query returns the fixture group");

    let observed = (
        row.try_get::<bool, _>("has_legacy_zero_tenant")
            .expect("legacy tenant flag is a boolean"),
        row.try_get::<i64, _>("membership_rows")
            .expect("membership count is an integer"),
        row.try_get::<i64, _>("membership_group_tenant_mismatches")
            .expect("tenant mismatch count is an integer"),
        row.try_get::<i64, _>("expired_unrevoked_share_references")
            .expect("expired share count is an integer"),
        row.try_get::<i64, _>("revoked_share_references")
            .expect("revoked share count is an integer"),
        row.try_get::<i64, _>("share_group_tenant_mismatches")
            .expect("share tenant mismatch count is an integer"),
        row.try_get::<i64, _>("unrevoked_unexpired_share_references")
            .expect("active share count is an integer"),
        row.try_get::<i64, _>("share_tenant_count")
            .expect("share tenant count is an integer"),
        row.try_get::<i64, _>("share_resource_tenant_mismatches")
            .expect("resource tenant mismatch count is an integer"),
        row.try_get::<i64, _>("file_owner_tenant_mismatches")
            .expect("file owner mismatch count is an integer"),
        row.try_get::<i64, _>("folder_owner_tenant_mismatches")
            .expect("folder owner mismatch count is an integer"),
        row.try_get::<i64, _>("share_creator_tenant_mismatches")
            .expect("share creator mismatch count is an integer"),
    );
    assert_eq!(observed, (true, 1, 1, 1, 1, 4, 2, 1, 1, 3, 1, 4));

    let unresolved_reference_counts = (
        row.try_get::<i64, _>("unresolved_user_rows")
            .expect("missing member reference count is an integer"),
        row.try_get::<i64, _>("missing_file_references")
            .expect("missing file reference count is an integer"),
        row.try_get::<i64, _>("missing_folder_references")
            .expect("missing folder reference count is an integer"),
        row.try_get::<i64, _>("missing_file_owner_user_references")
            .expect("missing file owner count is an integer"),
        row.try_get::<i64, _>("missing_folder_owner_user_references")
            .expect("missing folder owner count is an integer"),
        row.try_get::<i64, _>("missing_share_creator_user_references")
            .expect("missing share creator count is an integer"),
    );
    assert_eq!(unresolved_reference_counts, (0, 0, 0, 0, 0, 0));

    sqlx::query(
        "CREATE ROLE rustshare_inventory_rls_probe NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS",
    )
        .execute(&mut *transaction)
        .await
        .expect("create transaction-scoped non-owner RLS probe role");
    sqlx::query(
        "GRANT SELECT ON users, user_groups, group_members, shares, files, folders TO rustshare_inventory_rls_probe",
    )
    .execute(&mut *transaction)
    .await
    .expect("grant the probe role read access to inventory relations");
    sqlx::query("SAVEPOINT before_restricted_inventory_read")
        .execute(&mut *transaction)
        .await
        .expect("savepoint before enabling the restricted role");
    sqlx::query("SET LOCAL ROLE rustshare_inventory_rls_probe")
        .execute(&mut *transaction)
        .await
        .expect("assume the non-owner probe role");
    sqlx::query_scalar::<_, String>("SELECT set_config('app.current_user_id', $1, true)")
        .bind(user_id.to_string())
        .fetch_one(&mut *transaction)
        .await
        .expect("scope the owner-only policy to one fixture owner");
    sqlx::query("SET LOCAL row_security = on")
        .execute(&mut *transaction)
        .await
        .expect("first exercise the normal owner-isolation policy");

    let hidden_file_count =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM files WHERE id = $1")
            .bind(hidden_file_id)
            .fetch_one(&mut *transaction)
            .await
            .expect("read the file count under normal owner isolation");
    let hidden_folder_count =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM folders WHERE id = $1")
            .bind(hidden_folder_id)
            .fetch_one(&mut *transaction)
            .await
            .expect("read the folder count under normal owner isolation");
    assert_eq!((hidden_file_count, hidden_folder_count), (0, 0));

    sqlx::query(&inventory_rls_guard())
        .execute(&mut *transaction)
        .await
        .expect("execute the actual inventory script guard as the restricted role");

    let error = sqlx::query(&query)
        .bind(group_id)
        .fetch_one(&mut *transaction)
        .await
        .expect_err("inventory must fail rather than omit RLS-hidden files or folders");
    let code = error.as_database_error().and_then(|error| error.code());
    assert_eq!(
        code.as_deref(),
        Some("42501"),
        "expected PostgreSQL insufficient_privilege for rows hidden by RLS"
    );
    assert!(error.to_string().contains("row-level security"));

    sqlx::query("ROLLBACK TO SAVEPOINT before_restricted_inventory_read")
        .execute(&mut *transaction)
        .await
        .expect("recover the transaction after the expected RLS error");

    transaction
        .rollback()
        .await
        .expect("remove all fixture rows");
    pool.close().await;
}
