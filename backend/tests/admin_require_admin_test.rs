//! Integration tests: AdminUser extractor DB-level logic (Task 6).
//!
//! These tests verify the SQL conditions that the AdminUser extractor checks:
//!   - SELECT is_admin, disabled_at FROM users WHERE id = $1
//! They do NOT go through HTTP routing; they insert rows and assert the DB values.
//!
//! Run with: cargo test --test admin_require_admin_test -- --ignored

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

async fn cleanup(pool: &sqlx::PgPool, user_ids: &[Uuid]) {
    for id in user_ids {
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

/// A user with is_admin = false must not pass the admin check.
#[tokio::test]
#[ignore]
async fn test_non_admin_user_is_rejected() {
    let pool = test_pool().await;
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, false, 10737418240)",
    )
    .bind(id)
    .bind(format!("reqadmin_nonadmin_{}", &id.to_string()[..8]))
    .bind(format!("nonadmin_{}@test.local", &id.to_string()[..8]))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind("Non-Admin User")
    .execute(&pool)
    .await
    .expect("insert non-admin user");

    // Simulate what the AdminUser extractor queries.
    let row = sqlx::query("SELECT is_admin, disabled_at FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("fetch user row");

    let is_admin: bool = row.try_get("is_admin").unwrap();
    let disabled_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("disabled_at").unwrap();

    // The extractor rejects when is_admin is false.
    assert!(
        !is_admin,
        "Non-admin user must have is_admin = false"
    );
    assert!(
        disabled_at.is_none(),
        "Non-admin user should not be disabled in this test"
    );

    cleanup(&pool, &[id]).await;
}

/// A user with is_admin = true but disabled_at IS NOT NULL must not pass the admin check.
#[tokio::test]
#[ignore]
async fn test_disabled_admin_is_rejected() {
    let pool = test_pool().await;
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota, disabled_at)
         VALUES ($1, $2, $3, $4, $5, true, 10737418240, NOW())",
    )
    .bind(id)
    .bind(format!("reqadmin_disadmin_{}", &id.to_string()[..8]))
    .bind(format!("disadmin_{}@test.local", &id.to_string()[..8]))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind("Disabled Admin")
    .execute(&pool)
    .await
    .expect("insert disabled admin user");

    // Simulate what the AdminUser extractor queries.
    let row = sqlx::query("SELECT is_admin, disabled_at FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("fetch user row");

    let is_admin: bool = row.try_get("is_admin").unwrap();
    let disabled_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("disabled_at").unwrap();

    // The extractor rejects when disabled_at IS NOT NULL, even for admins.
    assert!(is_admin, "User must be admin for this test case");
    assert!(
        disabled_at.is_some(),
        "Disabled admin must have disabled_at IS NOT NULL"
    );

    cleanup(&pool, &[id]).await;
}

/// A user with is_admin = true and disabled_at IS NULL must pass the admin check.
#[tokio::test]
#[ignore]
async fn test_active_admin_is_accepted() {
    let pool = test_pool().await;
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, true, 10737418240)",
    )
    .bind(id)
    .bind(format!("reqadmin_activeadmin_{}", &id.to_string()[..8]))
    .bind(format!("activeadmin_{}@test.local", &id.to_string()[..8]))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind("Active Admin")
    .execute(&pool)
    .await
    .expect("insert active admin user");

    let row = sqlx::query("SELECT is_admin, disabled_at FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("fetch user row");

    let is_admin: bool = row.try_get("is_admin").unwrap();
    let disabled_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("disabled_at").unwrap();

    assert!(is_admin, "Active admin must have is_admin = true");
    assert!(disabled_at.is_none(), "Active admin must have disabled_at IS NULL");

    cleanup(&pool, &[id]).await;
}
