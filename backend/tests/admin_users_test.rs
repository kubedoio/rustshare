//! Integration tests: Admin user management SQL lifecycle (Task 6).
//!
//! Tests the core SQL used by the admin users handlers:
//!   create, fetch, update quota, disable, enable, delete.
//! Also verifies admin_actions rows are written for mutating operations.
//!
//! Run with: cargo test --test admin_users_test -- --ignored

mod support;

use sqlx::Row;
use tower::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    sqlx::PgPool::connect(&url)
        .await
        .expect("DB connect failed")
}

async fn create_test_admin(pool: &sqlx::PgPool, suffix: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, true, 10737418240)",
    )
    .bind(id)
    .bind(format!("test_admin_{suffix}"))
    .bind(format!("admin_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("Test Admin {suffix}"))
    .execute(pool)
    .await
    .expect("create test admin");
    id
}

async fn cleanup(pool: &sqlx::PgPool, user_ids: &[Uuid]) {
    for id in user_ids {
        // admin_actions rows for this user are cleaned up first since they FK to users
        sqlx::query("DELETE FROM admin_actions WHERE actor_id = $1 OR target_id = $1")
            .bind(id)
            .execute(pool)
            .await
            .ok();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await
            .ok();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Full user lifecycle: create → fetch → update quota → disable → enable → delete.
/// Verifies admin_actions are written for each mutating step.
#[tokio::test]
async fn test_admin_user_full_lifecycle() {
    let pool = test_pool().await;
    let actor_id = create_test_admin(&pool, &Uuid::new_v4().to_string()[..8]).await;
    let user_id = Uuid::new_v4();
    let suffix = &user_id.to_string()[..8];

    // --- CREATE ---
    let cols = "id, username, email, display_name, is_admin, storage_quota, disabled_at, created_at, updated_at";
    let created_row = sqlx::query(&format!(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, false, 10737418240)
         RETURNING {cols}"
    ))
    .bind(user_id)
    .bind(format!("lifecycle_user_{suffix}"))
    .bind(format!("lifecycle_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("Lifecycle User {suffix}"))
    .fetch_one(&pool)
    .await
    .expect("create user");

    let username: String = created_row.try_get("username").unwrap();
    let is_admin: bool = created_row.try_get("is_admin").unwrap();
    let quota: i64 = created_row.try_get("storage_quota").unwrap();
    assert_eq!(username, format!("lifecycle_user_{suffix}"));
    assert!(!is_admin);
    assert_eq!(quota, 10_737_418_240);

    // Log user.created admin action
    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, target_type, target_id, detail)
         VALUES ($1, 'user.created', 'user', $2, $3)",
    )
    .bind(actor_id)
    .bind(user_id)
    .bind(serde_json::json!({"username": username}))
    .execute(&pool)
    .await
    .expect("log user.created");

    // --- FETCH ---
    let fetched = sqlx::query(&format!("SELECT {cols} FROM users WHERE id = $1"))
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("fetch user");

    let fetched_email: String = fetched.try_get("email").unwrap();
    assert_eq!(fetched_email, format!("lifecycle_{suffix}@test.local"));

    // --- UPDATE QUOTA ---
    let new_quota: i64 = 21_474_836_480; // 20 GB
    sqlx::query("UPDATE users SET storage_quota = $2, updated_at = NOW() WHERE id = $1")
        .bind(user_id)
        .bind(new_quota)
        .execute(&pool)
        .await
        .expect("update quota");

    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, target_type, target_id, detail)
         VALUES ($1, 'user.quota_changed', 'user', $2, $3)",
    )
    .bind(actor_id)
    .bind(user_id)
    .bind(serde_json::json!({"old_quota": 10737418240_i64, "new_quota": new_quota}))
    .execute(&pool)
    .await
    .expect("log quota change");

    let updated_quota: i64 = sqlx::query_scalar("SELECT storage_quota FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("fetch quota");
    assert_eq!(updated_quota, new_quota);

    // --- DISABLE ---
    sqlx::query("UPDATE users SET disabled_at = NOW() WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("disable user");

    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, target_type, target_id, detail)
         VALUES ($1, 'user.disabled', 'user', $2, '{}'::jsonb)",
    )
    .bind(actor_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("log user.disabled");

    // Verify disabled_at is set (the login handler checks this condition).
    let disabled_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT disabled_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .expect("fetch disabled_at");
    assert!(
        disabled_at.is_some(),
        "disabled_at must be set after disable"
    );

    // --- ENABLE ---
    sqlx::query("UPDATE users SET disabled_at = NULL WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("enable user");

    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, target_type, target_id, detail)
         VALUES ($1, 'user.enabled', 'user', $2, '{}'::jsonb)",
    )
    .bind(actor_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("log user.enabled");

    let re_enabled_disabled_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT disabled_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .expect("fetch disabled_at after enable");
    assert!(
        re_enabled_disabled_at.is_none(),
        "disabled_at must be NULL after enable"
    );

    // --- Verify admin_actions rows exist for the lifecycle operations ---
    let action_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM admin_actions WHERE target_id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .expect("count admin_actions");
    assert!(
        action_count >= 4,
        "Expected at least 4 admin_actions for lifecycle (got {action_count})"
    );

    // Verify each specific action type was logged
    for action_type in &[
        "user.created",
        "user.quota_changed",
        "user.disabled",
        "user.enabled",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM admin_actions WHERE actor_id = $1 AND action_type = $2",
        )
        .bind(actor_id)
        .bind(action_type)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "Expected 1 row for action_type={}", action_type);
    }

    // --- HARD DELETE ---
    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, target_type, target_id, detail)
         VALUES ($1, 'user.deleted', 'user', $2, '{}'::jsonb)",
    )
    .bind(actor_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("log user.deleted");

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("delete user");

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("check user existence");
    assert!(!exists, "User must not exist after hard delete");

    cleanup(&pool, &[actor_id]).await;
    // Note: user_id already deleted above; clean up its admin_actions
    sqlx::query("DELETE FROM admin_actions WHERE target_id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .ok();
}

/// Disabled user has disabled_at IS NOT NULL — the login handler
/// checks this column to block login.
#[tokio::test]
async fn test_disabled_user_has_disabled_at_set() {
    let pool = test_pool().await;
    let user_id = Uuid::new_v4();
    let suffix = &user_id.to_string()[..8];

    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, false, 10737418240)",
    )
    .bind(user_id)
    .bind(format!("disable_check_{suffix}"))
    .bind(format!("discheck_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind("Disable Check User")
    .execute(&pool)
    .await
    .expect("insert user");

    // Initially not disabled
    let initial_disabled_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT disabled_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .expect("fetch disabled_at");
    assert!(
        initial_disabled_at.is_none(),
        "User must not be disabled initially"
    );

    // Disable
    sqlx::query("UPDATE users SET disabled_at = NOW() WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("disable user");

    let disabled_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT disabled_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .expect("fetch disabled_at");
    assert!(
        disabled_at.is_some(),
        "disabled_at must be set — the login handler rejects non-null disabled_at"
    );

    cleanup(&pool, &[user_id]).await;
}

// ---------------------------------------------------------------------------
// AR-01: organization-explicit principal creation (ADR-0040 D4, ADR-0043 D-D2 I1)
// ---------------------------------------------------------------------------

fn assert_disposable_database(database_url: &str) {
    let parsed = url::Url::parse(database_url).expect("DATABASE_URL must be a PostgreSQL URL");
    let database_name = parsed.path().trim_start_matches('/');
    assert!(
        database_name == "rustshare_test" || database_name.starts_with("rustshare_test_"),
        "refusing to mutate database outside rustshare_test or rustshare_test_*"
    );
    assert_eq!(
        std::env::var("RUSTSHARE_TEST_DISPOSABLE_DB")
            .ok()
            .as_deref(),
        Some("1"),
        "set RUSTSHARE_TEST_DISPOSABLE_DB=1 only for the disposable test database"
    );
}

async fn insert_admin_actor(pool: &sqlx::PgPool, actor_id: Uuid, tenant_id: Uuid) {
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, true, 10737418240, $4)",
    )
    .bind(actor_id)
    .bind(format!("ar01_admin_{}", actor_id.simple()))
    .bind(format!("ar01_admin_{}@test.local", actor_id.simple()))
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("create admin actor");
}

async fn post_admin_user(
    app: &axum::Router,
    token: &str,
    username: &str,
) -> (axum::http::StatusCode, Option<Uuid>) {
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/admin/users")
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(format!(
                    r#"{{"username":"{username}","email":"{username}@test.local","password":"ar01-password-123","display_name":"{username}"}}"#
                )))
                .expect("build admin user create request"),
        )
        .await
        .expect("admin user create request");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .expect("read admin user create response");
    let id = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| {
            value
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_string)
        })
        .and_then(|id| Uuid::parse_str(&id).ok());
    (status, id)
}

/// The admin creation handler writes the explicitly resolved organization on
/// the INSERT: with a non-nil default configured, the stored row carries that
/// organization (nil-default behaviour is unreachable), and with the default
/// unset the stored row carries nil — the resolved default organization.
#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn test_admin_user_creation_writes_resolved_organization() {
    use support::calendar_harness::{assert_local_database, setup_test_env, SERIAL};

    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL explicitly to a disposable local database");
    assert_disposable_database(&database_url);
    assert_local_database();

    let mut state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let actor_id = Uuid::new_v4();
    let configured_tenant = Uuid::new_v4();
    insert_admin_actor(&pool, actor_id, configured_tenant).await;

    // Non-nil default organization: the stored row must carry it.
    state.default_tenant_config =
        rustshare_server::config::DefaultTenantConfig::Set(configured_tenant);
    let app = rustshare_server::routes::admin_routes().with_state(state.clone());
    let token = support::calendar_harness::create_auth_token(&state, actor_id, configured_tenant);

    let (status, user_id) = post_admin_user(&app, &token, "ar01_scoped_user").await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let user_id = user_id.expect("create response must carry the new user id");
    let stored_tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("fetch created user tenant");
    assert_eq!(
        stored_tenant, configured_tenant,
        "the stored row must carry the resolved organization, not the column default"
    );
    assert_ne!(
        stored_tenant,
        Uuid::nil(),
        "nil-default behaviour must be unreachable when a default organization is configured"
    );
    let audit_actions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM admin_actions WHERE action_type = 'user.created' AND target_id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("count user.created audit rows");
    assert_eq!(audit_actions, 1, "user.created audit row must be written");

    // Unset default: nil *is* the resolved default organization (ADR-0043).
    state.default_tenant_config = rustshare_server::config::DefaultTenantConfig::Unset;
    let app = rustshare_server::routes::admin_routes().with_state(state.clone());
    let (status, nil_default_user_id) =
        post_admin_user(&app, &token, "ar01_nil_default_user").await;
    assert_eq!(status, axum::http::StatusCode::CREATED);
    let nil_default_user_id =
        nil_default_user_id.expect("create response must carry the new user id");
    let stored_tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM users WHERE id = $1")
        .bind(nil_default_user_id)
        .fetch_one(&pool)
        .await
        .expect("fetch nil-default user tenant");
    assert_eq!(
        stored_tenant,
        Uuid::nil(),
        "with the default unset the resolved organization is the nil default organization"
    );

    cleanup(&pool, &[user_id, nil_default_user_id, actor_id]).await;
}
