//! Integration tests: Admin group management SQL lifecycle (Task 6).
//!
//! Tests: create group, add member, duplicate member conflict, remove member,
//! delete group with CASCADE on group_members.
//!
//! Run with: cargo test --test admin_groups_test -- --ignored

use sqlx::Row;
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

async fn create_test_user(pool: &sqlx::PgPool, suffix: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, false, 10737418240)",
    )
    .bind(id)
    .bind(format!("grp_user_{suffix}"))
    .bind(format!("grp_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("Group User {suffix}"))
    .execute(pool)
    .await
    .expect("create test user");
    id
}

async fn create_test_admin(pool: &sqlx::PgPool, suffix: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, true, 10737418240)",
    )
    .bind(id)
    .bind(format!("grp_admin_{suffix}"))
    .bind(format!("grpadmin_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("Group Admin {suffix}"))
    .execute(pool)
    .await
    .expect("create test admin");
    id
}

async fn cleanup_users(pool: &sqlx::PgPool, user_ids: &[Uuid]) {
    for id in user_ids {
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

async fn cleanup_group(pool: &sqlx::PgPool, group_id: Uuid) {
    sqlx::query("DELETE FROM admin_actions WHERE target_id = $1")
        .bind(group_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM user_groups WHERE id = $1")
        .bind(group_id)
        .execute(pool)
        .await
        .ok();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Create a group and verify it appears in user_groups.
#[tokio::test]
async fn test_create_group() {
    let pool = test_pool().await;
    let actor_id = create_test_admin(&pool, &Uuid::new_v4().to_string()[..8]).await;
    let group_id = Uuid::new_v4();

    let row = sqlx::query(
        "INSERT INTO user_groups (id, name, description, created_by)
         VALUES ($1, $2, $3, $4)
         RETURNING id, name, description, created_by, created_at, updated_at",
    )
    .bind(group_id)
    .bind(format!("Test Group {}", &group_id.to_string()[..8]))
    .bind("A test group")
    .bind(actor_id)
    .fetch_one(&pool)
    .await
    .expect("insert group");

    let name: String = row.try_get("name").unwrap();
    let description: Option<String> = row.try_get("description").unwrap();
    let created_by: Option<Uuid> = row.try_get("created_by").unwrap();

    assert!(name.starts_with("Test Group"));
    assert_eq!(description.as_deref(), Some("A test group"));
    assert_eq!(created_by, Some(actor_id));

    // Verify it's in the table
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM user_groups WHERE id = $1)")
        .bind(group_id)
        .fetch_one(&pool)
        .await
        .expect("check group exists");
    assert!(exists, "Group must be in user_groups after insert");

    cleanup_group(&pool, group_id).await;
    cleanup_users(&pool, &[actor_id]).await;
}

/// Add a member to a group and verify member count increases.
#[tokio::test]
async fn test_add_member_to_group() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let user_id = create_test_user(&pool, suffix).await;
    let group_id = Uuid::new_v4();

    sqlx::query("INSERT INTO user_groups (id, name, created_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(format!("MemberTest {suffix}"))
        .bind(actor_id)
        .execute(&pool)
        .await
        .expect("insert group");

    // Add member
    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(user_id)
        .bind(actor_id)
        .execute(&pool)
        .await
        .expect("add member");

    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = $1")
            .bind(group_id)
            .fetch_one(&pool)
            .await
            .expect("count members");

    assert_eq!(
        member_count, 1,
        "Group must have exactly 1 member after add"
    );

    cleanup_group(&pool, group_id).await;
    cleanup_users(&pool, &[actor_id, user_id]).await;
}

/// Inserting the same user twice into a group must fail with a unique constraint violation.
#[tokio::test]
async fn test_duplicate_member_violates_unique_constraint() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let user_id = create_test_user(&pool, suffix).await;
    let group_id = Uuid::new_v4();

    sqlx::query("INSERT INTO user_groups (id, name, created_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(format!("DupTest {suffix}"))
        .bind(actor_id)
        .execute(&pool)
        .await
        .expect("insert group");

    // First insert — must succeed
    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(user_id)
        .bind(actor_id)
        .execute(&pool)
        .await
        .expect("first add member");

    // Second insert of the same user — must fail with unique constraint error
    let result =
        sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
            .bind(group_id)
            .bind(user_id)
            .bind(actor_id)
            .execute(&pool)
            .await;

    assert!(
        result.is_err(),
        "Duplicate member insert must return a DB error"
    );
    if let Err(sqlx::Error::Database(ref db_err)) = result {
        // PostgreSQL returns constraint name "group_members_group_id_user_id_key"
        assert!(
            db_err.constraint().is_some(),
            "Error must be a constraint violation"
        );
    }

    cleanup_group(&pool, group_id).await;
    cleanup_users(&pool, &[actor_id, user_id]).await;
}

/// Remove a member from a group and verify they are gone.
#[tokio::test]
async fn test_remove_member_from_group() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let user_id = create_test_user(&pool, suffix).await;
    let group_id = Uuid::new_v4();

    sqlx::query("INSERT INTO user_groups (id, name, created_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(format!("RemoveMemberTest {suffix}"))
        .bind(actor_id)
        .execute(&pool)
        .await
        .expect("insert group");

    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(user_id)
        .bind(actor_id)
        .execute(&pool)
        .await
        .expect("add member");

    sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND user_id = $2")
        .bind(group_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("remove member");

    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = $1")
            .bind(group_id)
            .fetch_one(&pool)
            .await
            .expect("count members after remove");

    assert_eq!(member_count, 0, "Group must have 0 members after removal");

    cleanup_group(&pool, group_id).await;
    cleanup_users(&pool, &[actor_id, user_id]).await;
}

/// Deleting a group must CASCADE-delete all group_members rows.
#[tokio::test]
async fn test_delete_group_cascades_members() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let user1_id = create_test_user(&pool, &format!("{suffix}a")).await;
    let user2_id = create_test_user(&pool, &format!("{suffix}b")).await;
    let group_id = Uuid::new_v4();

    sqlx::query("INSERT INTO user_groups (id, name, created_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(format!("CascadeTest {suffix}"))
        .bind(actor_id)
        .execute(&pool)
        .await
        .expect("insert group");

    for &uid in &[user1_id, user2_id] {
        sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
            .bind(group_id)
            .bind(uid)
            .bind(actor_id)
            .execute(&pool)
            .await
            .expect("add member");
    }

    // Confirm 2 members before deletion
    let count_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = $1")
            .bind(group_id)
            .fetch_one(&pool)
            .await
            .expect("count before");
    assert_eq!(count_before, 2);

    // Delete the group
    sqlx::query("DELETE FROM user_groups WHERE id = $1")
        .bind(group_id)
        .execute(&pool)
        .await
        .expect("delete group");

    // group_members rows must have been CASCADE-deleted
    let count_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id = $1")
            .bind(group_id)
            .fetch_one(&pool)
            .await
            .expect("count after");
    assert_eq!(
        count_after, 0,
        "group_members rows must be CASCADE-deleted when group is deleted"
    );

    cleanup_users(&pool, &[actor_id, user1_id, user2_id]).await;
}
