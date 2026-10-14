//! Integration tests: Admin group management SQL lifecycle (Task 6).
//!
//! Tests: create group, add member, duplicate member conflict, remove member,
//! delete group with CASCADE on group_members.
//!
//! Run with: cargo test --test admin_groups_test -- --ignored

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

// ---------------------------------------------------------------------------
// AR-01: organization-explicit principal creation (ADR-0040 D4, ADR-0043 D-D2 I1)
// ---------------------------------------------------------------------------

fn ar01_assert_disposable_database(database_url: &str) {
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

/// The admin group creation handler writes the explicitly resolved
/// organization on the INSERT: with a non-nil default configured, the stored
/// row carries that organization (nil-default behaviour is unreachable), and
/// with the default unset the stored row carries nil — the resolved default
/// organization.
#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn test_admin_group_creation_writes_resolved_organization() {
    use support::calendar_harness::{
        assert_local_database, create_auth_token, setup_test_env, SERIAL,
    };

    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL explicitly to a disposable local database");
    ar01_assert_disposable_database(&database_url);
    assert_local_database();

    let mut state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let actor_id = Uuid::new_v4();
    let configured_tenant = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, true, 10737418240, $4)",
    )
    .bind(actor_id)
    .bind(format!("ar01_admin_{}", actor_id.simple()))
    .bind(format!("ar01_admin_{}@test.local", actor_id.simple()))
    .bind(configured_tenant)
    .execute(&pool)
    .await
    .expect("create admin actor");
    let token = create_auth_token(&state, actor_id, configured_tenant);

    // Non-nil default organization: the stored row must carry it.
    state.default_tenant_config =
        rustshare_server::config::DefaultTenantConfig::Set(configured_tenant);
    let app = rustshare_server::routes::admin_routes().with_state(state.clone());
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/admin/groups")
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(r#"{"name":"ar01-scoped-group"}"#))
                .expect("build group create request"),
        )
        .await
        .expect("group create request");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .expect("read group create response");
    let group_id = Uuid::parse_str(
        serde_json::from_slice::<serde_json::Value>(&body)
            .expect("group create response must be JSON")
            .get("id")
            .and_then(|id| id.as_str())
            .expect("group create response must carry the group id"),
    )
    .expect("group id must be a UUID");
    let stored_tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM user_groups WHERE id = $1")
        .bind(group_id)
        .fetch_one(&pool)
        .await
        .expect("fetch created group tenant");
    assert_eq!(
        stored_tenant, configured_tenant,
        "the stored row must carry the resolved organization, not the column default"
    );
    assert_ne!(
        stored_tenant,
        Uuid::nil(),
        "nil-default behaviour must be unreachable when a default organization is configured"
    );

    // Unset default: nil *is* the resolved default organization (ADR-0043).
    state.default_tenant_config = rustshare_server::config::DefaultTenantConfig::Unset;
    let app = rustshare_server::routes::admin_routes().with_state(state.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/v1/admin/groups")
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    r#"{"name":"ar01-nil-default-group"}"#,
                ))
                .expect("build nil-default group create request"),
        )
        .await
        .expect("nil-default group create request");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .expect("read nil-default group create response");
    let nil_default_group_id = Uuid::parse_str(
        serde_json::from_slice::<serde_json::Value>(&body)
            .expect("nil-default group create response must be JSON")
            .get("id")
            .and_then(|id| id.as_str())
            .expect("nil-default group create response must carry the group id"),
    )
    .expect("group id must be a UUID");
    let stored_tenant: Uuid = sqlx::query_scalar("SELECT tenant_id FROM user_groups WHERE id = $1")
        .bind(nil_default_group_id)
        .fetch_one(&pool)
        .await
        .expect("fetch nil-default group tenant");
    assert_eq!(
        stored_tenant,
        Uuid::nil(),
        "with the default unset the resolved organization is the nil default organization"
    );

    cleanup_group(&pool, group_id).await;
    cleanup_group(&pool, nil_default_group_id).await;
    cleanup_users(&pool, &[actor_id]).await;
}

// ---------------------------------------------------------------------------
// AR-03: tenant-safe group membership (ADR-0040 D4 — organization = tenant,
// strict equality)
// ---------------------------------------------------------------------------

fn ar03_assert_disposable_database(database_url: &str) {
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

/// Adding a user from a different organization (users.tenant_id !=
/// user_groups.tenant_id) must be rejected with HTTP 400 and must not write
/// a group_members row.
#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn test_admin_add_member_cross_organization_refused_no_mutation() {
    use support::calendar_harness::{
        assert_local_database, create_auth_token, setup_test_env, SERIAL,
    };

    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL explicitly to a disposable local database");
    ar03_assert_disposable_database(&database_url);
    assert_local_database();

    let mut state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let actor_id = Uuid::new_v4();
    let configured_tenant = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, true, 10737418240, $4)",
    )
    .bind(actor_id)
    .bind(format!("ar03_admin_{}", actor_id.simple()))
    .bind(format!("ar03_admin_{}@test.local", actor_id.simple()))
    .bind(configured_tenant)
    .execute(&pool)
    .await
    .expect("create admin actor");
    let token = create_auth_token(&state, actor_id, configured_tenant);

    state.default_tenant_config =
        rustshare_server::config::DefaultTenantConfig::Set(configured_tenant);

    // Second organization and a user that belongs to it.
    let foreign_tenant = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(foreign_tenant)
        .bind(format!("ar03_foreign_tenant_{}", foreign_tenant.simple()))
        .execute(&pool)
        .await
        .expect("create foreign tenant");
    let foreign_user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, false, 10737418240, $4)",
    )
    .bind(foreign_user_id)
    .bind(format!("ar03_foreign_{}", foreign_user_id.simple()))
    .bind(format!(
        "ar03_foreign_{}@test.local",
        foreign_user_id.simple()
    ))
    .bind(foreign_tenant)
    .execute(&pool)
    .await
    .expect("create foreign user");

    // Group owned by the default organization.
    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, created_by, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("ar03_group_{}", group_id.simple()))
    .bind(actor_id)
    .bind(configured_tenant)
    .execute(&pool)
    .await
    .expect("create group in default tenant");

    let app = rustshare_server::routes::admin_routes().with_state(state.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/groups/{group_id}/members"))
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(format!(
                    r#"{{"user_id":"{foreign_user_id}"}}"#
                )))
                .expect("build cross-organization add-member request"),
        )
        .await
        .expect("cross-organization add-member request");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "cross-organization membership must be refused"
    );

    let membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM group_members WHERE group_id = $1 AND user_id = $2",
    )
    .bind(group_id)
    .bind(foreign_user_id)
    .fetch_one(&pool)
    .await
    .expect("count cross-organization memberships");
    assert_eq!(
        membership_count, 0,
        "cross-organization membership must not be written"
    );

    cleanup_group(&pool, group_id).await;
    cleanup_users(&pool, &[actor_id, foreign_user_id]).await;
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(foreign_tenant)
        .execute(&pool)
        .await
        .expect("remove foreign tenant");
}

/// Adding a user whose organization matches the group's succeeds and writes
/// the membership row.
#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn test_admin_add_member_same_organization_allowed() {
    use support::calendar_harness::{
        assert_local_database, create_auth_token, setup_test_env, SERIAL,
    };

    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL explicitly to a disposable local database");
    ar03_assert_disposable_database(&database_url);
    assert_local_database();

    let mut state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let actor_id = Uuid::new_v4();
    let configured_tenant = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, true, 10737418240, $4)",
    )
    .bind(actor_id)
    .bind(format!("ar03_admin_{}", actor_id.simple()))
    .bind(format!("ar03_admin_{}@test.local", actor_id.simple()))
    .bind(configured_tenant)
    .execute(&pool)
    .await
    .expect("create admin actor");
    let token = create_auth_token(&state, actor_id, configured_tenant);

    state.default_tenant_config =
        rustshare_server::config::DefaultTenantConfig::Set(configured_tenant);

    // Member user in the same organization as the group.
    let member_user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, false, 10737418240, $4)",
    )
    .bind(member_user_id)
    .bind(format!("ar03_member_{}", member_user_id.simple()))
    .bind(format!(
        "ar03_member_{}@test.local",
        member_user_id.simple()
    ))
    .bind(configured_tenant)
    .execute(&pool)
    .await
    .expect("create same-organization user");

    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, created_by, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("ar03_group_{}", group_id.simple()))
    .bind(actor_id)
    .bind(configured_tenant)
    .execute(&pool)
    .await
    .expect("create group in default tenant");

    let app = rustshare_server::routes::admin_routes().with_state(state.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/groups/{group_id}/members"))
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(format!(
                    r#"{{"user_id":"{member_user_id}"}}"#
                )))
                .expect("build same-organization add-member request"),
        )
        .await
        .expect("same-organization add-member request");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::NO_CONTENT,
        "same-organization membership must be accepted"
    );

    let membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM group_members WHERE group_id = $1 AND user_id = $2",
    )
    .bind(group_id)
    .bind(member_user_id)
    .fetch_one(&pool)
    .await
    .expect("count same-organization memberships");
    assert_eq!(
        membership_count, 1,
        "same-organization membership must be written"
    );

    cleanup_group(&pool, group_id).await;
    cleanup_users(&pool, &[actor_id, member_user_id]).await;
}

/// Removing a user whose organization differs from the group's
/// (users.tenant_id != user_groups.tenant_id) must be rejected with HTTP 400
/// and must not delete the seeded group_members row: a mismatched legacy row
/// must not be "repaired" into a privilege by deleting it.
#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn test_admin_remove_member_cross_organization_refused_no_mutation() {
    use support::calendar_harness::{
        assert_local_database, create_auth_token, setup_test_env, SERIAL,
    };

    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL explicitly to a disposable local database");
    ar03_assert_disposable_database(&database_url);
    assert_local_database();

    let mut state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let actor_id = Uuid::new_v4();
    let configured_tenant = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, true, 10737418240, $4)",
    )
    .bind(actor_id)
    .bind(format!("ar03_admin_{}", actor_id.simple()))
    .bind(format!("ar03_admin_{}@test.local", actor_id.simple()))
    .bind(configured_tenant)
    .execute(&pool)
    .await
    .expect("create admin actor");
    let token = create_auth_token(&state, actor_id, configured_tenant);

    state.default_tenant_config =
        rustshare_server::config::DefaultTenantConfig::Set(configured_tenant);

    // Second organization and a user that belongs to it.
    let foreign_tenant = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(foreign_tenant)
        .bind(format!("ar03_foreign_tenant_{}", foreign_tenant.simple()))
        .execute(&pool)
        .await
        .expect("create foreign tenant");
    let foreign_user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, false, 10737418240, $4)",
    )
    .bind(foreign_user_id)
    .bind(format!("ar03_foreign_{}", foreign_user_id.simple()))
    .bind(format!(
        "ar03_foreign_{}@test.local",
        foreign_user_id.simple()
    ))
    .bind(foreign_tenant)
    .execute(&pool)
    .await
    .expect("create foreign user");

    // Group owned by the default organization.
    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, created_by, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("ar03_group_{}", group_id.simple()))
    .bind(actor_id)
    .bind(configured_tenant)
    .execute(&pool)
    .await
    .expect("create group in default tenant");

    // Seed the mismatched membership row directly, as a legacy row would exist.
    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(foreign_user_id)
        .bind(actor_id)
        .execute(&pool)
        .await
        .expect("seed mismatched group_members row");

    let app = rustshare_server::routes::admin_routes().with_state(state.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/api/v1/admin/groups/{group_id}/members/{foreign_user_id}"
                ))
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("build cross-organization remove-member request"),
        )
        .await
        .expect("cross-organization remove-member request");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "cross-organization membership removal must be refused"
    );

    let membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM group_members WHERE group_id = $1 AND user_id = $2",
    )
    .bind(group_id)
    .bind(foreign_user_id)
    .fetch_one(&pool)
    .await
    .expect("count cross-organization memberships");
    assert_eq!(
        membership_count, 1,
        "mismatched legacy membership row must not be deleted"
    );

    cleanup_group(&pool, group_id).await;
    cleanup_users(&pool, &[actor_id, foreign_user_id]).await;
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(foreign_tenant)
        .execute(&pool)
        .await
        .expect("remove foreign tenant");
}

/// Removing a member whose organization matches the group's succeeds and
/// deletes the membership row.
#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn test_admin_remove_member_same_organization_allowed() {
    use support::calendar_harness::{
        assert_local_database, create_auth_token, setup_test_env, SERIAL,
    };

    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL explicitly to a disposable local database");
    ar03_assert_disposable_database(&database_url);
    assert_local_database();

    let mut state = setup_test_env().await;
    let pool = state.db_pool.clone();
    let actor_id = Uuid::new_v4();
    let configured_tenant = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, true, 10737418240, $4)",
    )
    .bind(actor_id)
    .bind(format!("ar03_admin_{}", actor_id.simple()))
    .bind(format!("ar03_admin_{}@test.local", actor_id.simple()))
    .bind(configured_tenant)
    .execute(&pool)
    .await
    .expect("create admin actor");
    let token = create_auth_token(&state, actor_id, configured_tenant);

    state.default_tenant_config =
        rustshare_server::config::DefaultTenantConfig::Set(configured_tenant);

    // Member user in the same organization as the group.
    let member_user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, false, 10737418240, $4)",
    )
    .bind(member_user_id)
    .bind(format!("ar03_member_{}", member_user_id.simple()))
    .bind(format!(
        "ar03_member_{}@test.local",
        member_user_id.simple()
    ))
    .bind(configured_tenant)
    .execute(&pool)
    .await
    .expect("create same-organization user");

    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, created_by, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("ar03_group_{}", group_id.simple()))
    .bind(actor_id)
    .bind(configured_tenant)
    .execute(&pool)
    .await
    .expect("create group in default tenant");

    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(member_user_id)
        .bind(actor_id)
        .execute(&pool)
        .await
        .expect("seed same-organization group_members row");

    let app = rustshare_server::routes::admin_routes().with_state(state.clone());
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/api/v1/admin/groups/{group_id}/members/{member_user_id}"
                ))
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("build same-organization remove-member request"),
        )
        .await
        .expect("same-organization remove-member request");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::NO_CONTENT,
        "same-organization membership removal must be accepted"
    );

    let membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM group_members WHERE group_id = $1 AND user_id = $2",
    )
    .bind(group_id)
    .bind(member_user_id)
    .fetch_one(&pool)
    .await
    .expect("count same-organization memberships");
    assert_eq!(
        membership_count, 0,
        "same-organization membership must be deleted"
    );

    cleanup_group(&pool, group_id).await;
    cleanup_users(&pool, &[actor_id, member_user_id]).await;
}
// ---------------------------------------------------------------------------
// AR-03 read path: user group reads must stay inside the requester
// organization (ADR-0040 D4 — organization = tenant, strict equality;
// fail-closed so foreign groups are indistinguishable from missing ones)
// ---------------------------------------------------------------------------

async fn ar03_create_tenant(pool: &sqlx::PgPool, tenant_id: Uuid, label: &str) {
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(format!("ar03_{label}_{}", tenant_id.simple()))
        .execute(pool)
        .await
        .expect("create tenant");
}

async fn ar03_create_user_in_tenant(pool: &sqlx::PgPool, tenant_id: Uuid, label: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin,
                            storage_quota, tenant_id)
         VALUES ($1, $2, $3, 'test-password-hash', $2, false, 10737418240, $4)",
    )
    .bind(id)
    .bind(format!("ar03_{label}_{}", id.simple()))
    .bind(format!("ar03_{label}_{}@test.local", id.simple()))
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("create user");
    id
}

async fn ar03_delete_tenant(pool: &sqlx::PgPool, tenant_id: Uuid) {
    sqlx::query("DELETE FROM tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .expect("remove tenant");
}

/// The roster of GET /api/v1/groups/my/{id} must list only members whose
/// organization matches the requester's; a legacy mismatched membership row
/// must not leak a foreign user into the roster.
#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn test_my_group_roster_excludes_foreign_organization_members() {
    use support::calendar_harness::{
        assert_local_database, create_auth_token, setup_test_env, SERIAL,
    };

    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL explicitly to a disposable local database");
    ar03_assert_disposable_database(&database_url);
    assert_local_database();

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();

    let home_tenant = Uuid::new_v4();
    ar03_create_tenant(&pool, home_tenant, "home").await;
    let user_a = ar03_create_user_in_tenant(&pool, home_tenant, "member").await;

    let foreign_tenant = Uuid::new_v4();
    ar03_create_tenant(&pool, foreign_tenant, "foreign").await;
    let user_b = ar03_create_user_in_tenant(&pool, foreign_tenant, "foreign").await;

    let group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, created_by, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(group_id)
    .bind(format!("ar03_roster_{}", group_id.simple()))
    .bind(user_a)
    .bind(home_tenant)
    .execute(&pool)
    .await
    .expect("create group in home tenant");

    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(user_a)
        .bind(user_a)
        .execute(&pool)
        .await
        .expect("add same-organization member");

    // Seed the mismatched legacy row directly, as a pre-AR-03 row would exist.
    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(group_id)
        .bind(user_b)
        .bind(user_a)
        .execute(&pool)
        .await
        .expect("seed mismatched group_members row");

    let token = create_auth_token(&state, user_a, home_tenant);
    let app = rustshare_server::routes::group_routes().with_state(state);
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri(format!("/api/v1/groups/my/{group_id}"))
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("build roster request"),
        )
        .await
        .expect("roster request");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "the same-organization member must see the group"
    );

    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .expect("read roster response");
    let json =
        serde_json::from_slice::<serde_json::Value>(&body).expect("roster response must be JSON");
    let member_ids: Vec<Uuid> = json
        .get("members")
        .and_then(serde_json::Value::as_array)
        .expect("roster response must include a members array")
        .iter()
        .map(|member| {
            Uuid::parse_str(
                member
                    .get("user_id")
                    .and_then(serde_json::Value::as_str)
                    .expect("member entry must carry a user_id"),
            )
            .expect("member user_id must be a UUID")
        })
        .collect();

    assert!(
        member_ids.contains(&user_a),
        "roster must include the same-organization member"
    );
    assert!(
        !member_ids.contains(&user_b),
        "roster must exclude the foreign-organization member"
    );

    cleanup_group(&pool, group_id).await;
    cleanup_users(&pool, &[user_a, user_b]).await;
    ar03_delete_tenant(&pool, home_tenant).await;
    ar03_delete_tenant(&pool, foreign_tenant).await;
}

/// GET /api/v1/groups/my must not include groups outside the requester's
/// organization.
#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn test_list_my_groups_excludes_foreign_tenant_groups() {
    use support::calendar_harness::{
        assert_local_database, create_auth_token, setup_test_env, SERIAL,
    };

    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL explicitly to a disposable local database");
    ar03_assert_disposable_database(&database_url);
    assert_local_database();

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();

    let home_tenant = Uuid::new_v4();
    ar03_create_tenant(&pool, home_tenant, "home").await;
    let user_a = ar03_create_user_in_tenant(&pool, home_tenant, "member").await;

    let foreign_tenant = Uuid::new_v4();
    ar03_create_tenant(&pool, foreign_tenant, "foreign").await;
    let user_b = ar03_create_user_in_tenant(&pool, foreign_tenant, "foreign").await;

    // A group in the requester's organization that must be listed.
    let home_group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, created_by, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(home_group_id)
    .bind(format!("ar03_home_{}", home_group_id.simple()))
    .bind(user_a)
    .bind(home_tenant)
    .execute(&pool)
    .await
    .expect("create group in home tenant");
    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(home_group_id)
        .bind(user_a)
        .bind(user_a)
        .execute(&pool)
        .await
        .expect("add same-organization member");

    // A group in another organization with only its own member (direct SQL).
    let foreign_group_id = Uuid::new_v4();
    let foreign_group_name = format!("ar03_foreign_{}", foreign_group_id.simple());
    sqlx::query(
        "INSERT INTO user_groups (id, name, created_by, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(foreign_group_id)
    .bind(foreign_group_name.clone())
    .bind(user_b)
    .bind(foreign_tenant)
    .execute(&pool)
    .await
    .expect("create group in foreign tenant");
    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(foreign_group_id)
        .bind(user_b)
        .bind(user_b)
        .execute(&pool)
        .await
        .expect("add foreign-organization member");

    // Seed a mismatched legacy membership row directly — the requester
    // belongs to the group even though the group belongs to another
    // organization. The unscoped pre-AR-03 query would list the foreign
    // group through this row; the tenant predicate must exclude it.
    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(foreign_group_id)
        .bind(user_a)
        .bind(user_b)
        .execute(&pool)
        .await
        .expect("seed mismatched legacy group_members row");

    let token = create_auth_token(&state, user_a, home_tenant);
    let app = rustshare_server::routes::group_routes().with_state(state);
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/v1/groups/my")
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("build list-my-groups request"),
        )
        .await
        .expect("list-my-groups request");
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .expect("read list-my-groups response");
    let groups = serde_json::from_slice::<serde_json::Value>(&body)
        .expect("list-my-groups response must be JSON")
        .as_array()
        .cloned()
        .expect("list-my-groups response must be a JSON array");

    assert!(
        groups.iter().any(|group| {
            group
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| id == home_group_id.to_string())
        }),
        "the requester's own-organization group must be listed"
    );
    assert!(
        groups.iter().all(|group| {
            group
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|id| id != foreign_group_id.to_string())
        }),
        "a foreign-organization group must not be listed"
    );
    assert!(
        groups.iter().all(|group| {
            group
                .get("name")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|name| name != foreign_group_name)
        }),
        "a foreign-organization group name must not be listed"
    );

    cleanup_group(&pool, home_group_id).await;
    cleanup_group(&pool, foreign_group_id).await;
    cleanup_users(&pool, &[user_a, user_b]).await;
    ar03_delete_tenant(&pool, home_tenant).await;
    ar03_delete_tenant(&pool, foreign_tenant).await;
}

/// A group outside the requester's organization must be indistinguishable
/// from a missing one on GET /api/v1/groups/my/{id}: both fail closed with
/// 404, never 403 (existence disclosure).
#[tokio::test]
#[ignore = "requires an explicitly configured disposable local PostgreSQL database and S3-compatible storage"]
async fn test_get_my_group_foreign_tenant_group_not_found() {
    use support::calendar_harness::{
        assert_local_database, create_auth_token, setup_test_env, SERIAL,
    };

    let _serial = SERIAL.lock().await;
    let database_url = std::env::var("DATABASE_URL")
        .expect("set DATABASE_URL explicitly to a disposable local database");
    ar03_assert_disposable_database(&database_url);
    assert_local_database();

    let state = setup_test_env().await;
    let pool = state.db_pool.clone();

    let home_tenant = Uuid::new_v4();
    ar03_create_tenant(&pool, home_tenant, "home").await;
    let user_a = ar03_create_user_in_tenant(&pool, home_tenant, "member").await;

    let foreign_tenant = Uuid::new_v4();
    ar03_create_tenant(&pool, foreign_tenant, "foreign").await;
    let user_b = ar03_create_user_in_tenant(&pool, foreign_tenant, "foreign").await;

    let foreign_group_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO user_groups (id, name, created_by, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(foreign_group_id)
    .bind(format!("ar03_foreign_{}", foreign_group_id.simple()))
    .bind(user_b)
    .bind(foreign_tenant)
    .execute(&pool)
    .await
    .expect("create group in foreign tenant");
    sqlx::query("INSERT INTO group_members (group_id, user_id, added_by) VALUES ($1, $2, $3)")
        .bind(foreign_group_id)
        .bind(user_b)
        .bind(user_b)
        .execute(&pool)
        .await
        .expect("add foreign-organization member");

    let token = create_auth_token(&state, user_a, home_tenant);
    let app = rustshare_server::routes::group_routes().with_state(state);

    // Foreign-tenant group: must fail closed exactly like a missing group.
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri(format!("/api/v1/groups/my/{foreign_group_id}"))
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("build foreign-group request"),
        )
        .await
        .expect("foreign-group request");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::NOT_FOUND,
        "a foreign-organization group must be indistinguishable from a missing one"
    );

    // Parity: a truly missing group must return the same status.
    let missing_group_id = Uuid::new_v4();
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri(format!("/api/v1/groups/my/{missing_group_id}"))
                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .expect("build missing-group request"),
        )
        .await
        .expect("missing-group request");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::NOT_FOUND,
        "a missing group must return 404"
    );

    cleanup_group(&pool, foreign_group_id).await;
    cleanup_users(&pool, &[user_a, user_b]).await;
    ar03_delete_tenant(&pool, home_tenant).await;
    ar03_delete_tenant(&pool, foreign_tenant).await;
}
