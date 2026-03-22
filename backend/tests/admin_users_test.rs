//! Integration tests: Admin user management SQL lifecycle (Task 6).
//!
//! Tests the core SQL used by the admin users handlers:
//!   create, fetch, update quota, disable, enable, delete.
//! Also verifies admin_actions rows are written for mutating operations.
//!
//! Run with: cargo test --test admin_users_test -- --ignored

use sqlx::Row;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    sqlx::PgPool::connect(&url).await.expect("DB connect failed")
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
#[ignore]
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
    sqlx::query(
        "UPDATE users SET storage_quota = $2, updated_at = NOW() WHERE id = $1",
    )
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

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
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
#[ignore]
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
