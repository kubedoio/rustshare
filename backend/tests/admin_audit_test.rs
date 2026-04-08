//! Integration tests: Admin audit log SQL queries (Task 6).
//!
//! Tests the UNION ALL CTE that the list_audit_log handler runs:
//!   - admin_action rows
//!   - security_event rows
//!   - Filtering by type
//!   - Filtering by user_id
//!   - Date-range filtering
//!   - Pagination (LIMIT/OFFSET)
//!
//! Note: share_access rows require a valid shares row (FK → shares → files → users).
//! The share_access branch is excluded from these tests to keep setup simple;
//! the CTE union logic for that branch is structurally identical to the other branches.
//!
//! Run with: cargo test --test admin_audit_test -- --ignored

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
    .bind(format!("audit_user_{suffix}"))
    .bind(format!("audituser_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("Audit User {suffix}"))
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
    .bind(format!("audit_admin_{suffix}"))
    .bind(format!("auditadmin_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("Audit Admin {suffix}"))
    .execute(pool)
    .await
    .expect("create test admin");
    id
}

async fn cleanup(pool: &sqlx::PgPool, user_ids: &[Uuid]) {
    for id in user_ids {
        sqlx::query("DELETE FROM user_security_events WHERE user_id = $1")
            .bind(id)
            .execute(pool)
            .await
            .ok();
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
// CTE union SQL (mirrors the handler logic for the two testable branches)
// ---------------------------------------------------------------------------

const SECURITY_BRANCH: &str = "SELECT
    use2.id,
    use2.occurred_at,
    'security_event'::text AS event_type,
    COALESCE(u.username, 'deleted_user')::text AS actor_label,
    use2.event_type::text AS action_type,
    NULL::text AS target_label,
    json_build_object('description', use2.description)::jsonb AS detail,
    use2.user_id AS actor_id
FROM user_security_events use2
LEFT JOIN users u ON u.id = use2.user_id";

const ADMIN_ACTION_BRANCH: &str = "SELECT
    aa.id,
    aa.performed_at AS occurred_at,
    'admin_action'::text AS event_type,
    COALESCE(u.username, 'deleted_user')::text AS actor_label,
    aa.action_type::text AS action_type,
    aa.target_id::text AS target_label,
    COALESCE(aa.detail, '{}'::jsonb) AS detail,
    aa.actor_id AS actor_id
FROM admin_actions aa
LEFT JOIN users u ON u.id = aa.actor_id";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Query type=admin_action returns admin_actions rows only.
#[tokio::test]
#[ignore]
async fn test_audit_filter_admin_action_type() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let target_id = Uuid::new_v4();

    // Insert one admin_action
    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, target_type, target_id, detail)
         VALUES ($1, 'user.created', 'user', $2, $3)",
    )
    .bind(actor_id)
    .bind(target_id)
    .bind(serde_json::json!({"test": true}))
    .execute(&pool)
    .await
    .expect("insert admin_action");

    // Query using the admin_action branch only
    let cte = format!("WITH all_events AS (\n{ADMIN_ACTION_BRANCH}\n)");
    let sql = format!(
        "{cte}
         SELECT id, occurred_at, event_type, actor_label, action_type, target_label, detail, actor_id
         FROM all_events
         WHERE actor_id = $1
         ORDER BY occurred_at DESC
         LIMIT 100 OFFSET 0"
    );

    let rows = sqlx::query(&sql)
        .bind(actor_id)
        .fetch_all(&pool)
        .await
        .expect("query admin_action branch");

    assert!(
        !rows.is_empty(),
        "admin_action branch must return at least one row for our actor"
    );

    for row in &rows {
        let et: String = row.try_get("event_type").unwrap();
        assert_eq!(
            et, "admin_action",
            "All rows must have event_type = admin_action"
        );
    }

    cleanup(&pool, &[actor_id]).await;
}

/// Query type=security_event returns user_security_events rows only.
#[tokio::test]
#[ignore]
async fn test_audit_filter_security_event_type() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let user_id = create_test_user(&pool, suffix).await;
    let event_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO user_security_events (id, user_id, event_type, description, occurred_at)
         VALUES ($1, $2, 'login_failed', 'Test login failure', NOW())",
    )
    .bind(event_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect("insert security_event");

    let cte = format!("WITH all_events AS (\n{SECURITY_BRANCH}\n)");
    let sql = format!(
        "{cte}
         SELECT id, occurred_at, event_type, actor_label, action_type, target_label, detail, actor_id
         FROM all_events
         WHERE actor_id = $1
         ORDER BY occurred_at DESC
         LIMIT 100 OFFSET 0"
    );

    let rows = sqlx::query(&sql)
        .bind(user_id)
        .fetch_all(&pool)
        .await
        .expect("query security_event branch");

    assert!(
        !rows.is_empty(),
        "security_event branch must return rows for our user"
    );

    for row in &rows {
        let et: String = row.try_get("event_type").unwrap();
        assert_eq!(et, "security_event");
    }

    cleanup(&pool, &[user_id]).await;
}

/// UNION ALL of both branches returns rows from both tables.
#[tokio::test]
#[ignore]
async fn test_audit_all_type_union() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;
    let event_id = Uuid::new_v4();

    // Insert one of each
    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, detail)
         VALUES ($1, 'config.oidc_updated', '{}'::jsonb)",
    )
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("insert admin_action");

    sqlx::query(
        "INSERT INTO user_security_events (id, user_id, event_type, description, occurred_at)
         VALUES ($1, $2, 'password_changed', 'Test password change', NOW())",
    )
    .bind(event_id)
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("insert security_event");

    let union_sql = format!("{SECURITY_BRANCH}\nUNION ALL\n{ADMIN_ACTION_BRANCH}");
    let cte = format!("WITH all_events AS (\n{union_sql}\n)");
    let sql = format!(
        "{cte}
         SELECT id, occurred_at, event_type, actor_label, action_type, target_label, detail, actor_id
         FROM all_events
         WHERE actor_id = $1
         ORDER BY occurred_at DESC
         LIMIT 100 OFFSET 0"
    );

    let rows = sqlx::query(&sql)
        .bind(actor_id)
        .fetch_all(&pool)
        .await
        .expect("query union");

    let event_types: Vec<String> = rows
        .iter()
        .map(|r| r.try_get("event_type").unwrap())
        .collect();

    assert!(
        event_types.contains(&"admin_action".to_string()),
        "Union must include admin_action rows"
    );
    assert!(
        event_types.contains(&"security_event".to_string()),
        "Union must include security_event rows"
    );

    cleanup(&pool, &[actor_id]).await;
}

/// user_id filter returns only rows belonging to that actor.
#[tokio::test]
#[ignore]
async fn test_audit_user_id_filter() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor1_id = create_test_admin(&pool, &format!("{suffix}a")).await;
    let actor2_id = create_test_admin(&pool, &format!("{suffix}b")).await;

    // Insert admin_actions for both actors
    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, detail)
         VALUES ($1, 'user.created', '{}'::jsonb)",
    )
    .bind(actor1_id)
    .execute(&pool)
    .await
    .expect("action for actor1");

    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, detail)
         VALUES ($1, 'user.disabled', '{}'::jsonb)",
    )
    .bind(actor2_id)
    .execute(&pool)
    .await
    .expect("action for actor2");

    let cte = format!("WITH all_events AS (\n{ADMIN_ACTION_BRANCH}\n)");
    let sql = format!(
        "{cte}
         SELECT id, occurred_at, event_type, actor_label, action_type, target_label, detail, actor_id
         FROM all_events
         WHERE actor_id = $1
         ORDER BY occurred_at DESC
         LIMIT 100 OFFSET 0"
    );

    let rows = sqlx::query(&sql)
        .bind(actor1_id)
        .fetch_all(&pool)
        .await
        .expect("query with user_id filter");

    // All rows must belong to actor1
    for row in &rows {
        let actor_id: Option<Uuid> = row.try_get("actor_id").unwrap();
        assert_eq!(
            actor_id,
            Some(actor1_id),
            "All rows must have actor_id = actor1_id"
        );
    }

    cleanup(&pool, &[actor1_id, actor2_id]).await;
}

/// Date-range filter (occurred_at >= from AND occurred_at <= to) returns only matching rows.
#[tokio::test]
#[ignore]
async fn test_audit_date_range_filter() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;

    // Insert a past event with a specific timestamp
    let past_ts = chrono::Utc::now() - chrono::Duration::hours(2);
    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, detail, performed_at)
         VALUES ($1, 'user.created', '{}'::jsonb, $2)",
    )
    .bind(actor_id)
    .bind(past_ts)
    .execute(&pool)
    .await
    .expect("insert past action");

    // Insert a recent event
    let recent_ts = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO admin_actions (actor_id, action_type, detail, performed_at)
         VALUES ($1, 'user.deleted', '{}'::jsonb, $2)",
    )
    .bind(actor_id)
    .bind(recent_ts)
    .execute(&pool)
    .await
    .expect("insert recent action");

    // Query for only the last hour
    let from = chrono::Utc::now() - chrono::Duration::minutes(30);
    let to = chrono::Utc::now() + chrono::Duration::minutes(5);

    let cte = format!("WITH all_events AS (\n{ADMIN_ACTION_BRANCH}\n)");
    let sql = format!(
        "{cte}
         SELECT id, occurred_at, event_type, actor_label, action_type, target_label, detail, actor_id
         FROM all_events
         WHERE actor_id = $1 AND occurred_at >= $2 AND occurred_at <= $3
         ORDER BY occurred_at DESC
         LIMIT 100 OFFSET 0"
    );

    let rows = sqlx::query(&sql)
        .bind(actor_id)
        .bind(from)
        .bind(to)
        .fetch_all(&pool)
        .await
        .expect("query with date range");

    // Must include the recent event
    let action_types: Vec<String> = rows
        .iter()
        .map(|r| r.try_get("action_type").unwrap())
        .collect();
    assert!(
        action_types.contains(&"user.deleted".to_string()),
        "Recent event must be in date-range results"
    );
    // Must NOT include the past event (2 hours ago)
    assert!(
        !action_types.contains(&"user.created".to_string()),
        "Past event must not be in date-range results"
    );

    cleanup(&pool, &[actor_id]).await;
}

/// Pagination: LIMIT/OFFSET returns correct slice of results.
#[tokio::test]
#[ignore]
async fn test_audit_pagination() {
    let pool = test_pool().await;
    let suffix = &Uuid::new_v4().to_string()[..8];
    let actor_id = create_test_admin(&pool, suffix).await;

    // Insert 5 admin_actions
    for i in 0..5_i32 {
        sqlx::query(
            "INSERT INTO admin_actions (actor_id, action_type, detail)
             VALUES ($1, $2, '{}'::jsonb)",
        )
        .bind(actor_id)
        .bind(format!("test.action_{i}"))
        .execute(&pool)
        .await
        .expect("insert action");
    }

    // Count total for this actor
    let cte = format!("WITH all_events AS (\n{ADMIN_ACTION_BRANCH}\n)");
    let count_sql = format!("{cte} SELECT COUNT(*) FROM all_events WHERE actor_id = $1");
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(actor_id)
        .fetch_one(&pool)
        .await
        .expect("count total");

    assert!(total >= 5, "Total must be at least 5 (inserted 5 rows)");

    // Page 1 (LIMIT 2 OFFSET 0) must return 2 rows
    let page_sql = format!(
        "{cte}
         SELECT id, occurred_at, event_type, actor_label, action_type, target_label, detail, actor_id
         FROM all_events
         WHERE actor_id = $1
         ORDER BY occurred_at DESC
         LIMIT $2 OFFSET $3"
    );

    let page1 = sqlx::query(&page_sql)
        .bind(actor_id)
        .bind(2_i64)
        .bind(0_i64)
        .fetch_all(&pool)
        .await
        .expect("page 1");

    let page2 = sqlx::query(&page_sql)
        .bind(actor_id)
        .bind(2_i64)
        .bind(2_i64)
        .fetch_all(&pool)
        .await
        .expect("page 2");

    assert_eq!(page1.len(), 2, "Page 1 must return 2 rows");
    assert_eq!(page2.len(), 2, "Page 2 must return 2 rows");

    // The rows on page 1 and page 2 must be different
    let page1_ids: Vec<Uuid> = page1.iter().map(|r| r.try_get("id").unwrap()).collect();
    let page2_ids: Vec<Uuid> = page2.iter().map(|r| r.try_get("id").unwrap()).collect();

    for id in &page2_ids {
        assert!(!page1_ids.contains(id), "Pages must not overlap");
    }

    // Verify ordering: fetch all results and assert occurred_at is descending
    let all_sql = format!(
        "{cte}
         SELECT id, occurred_at, event_type, actor_label, action_type, target_label, detail, actor_id
         FROM all_events
         WHERE actor_id = $1
         ORDER BY occurred_at DESC
         LIMIT 100 OFFSET 0"
    );
    let entries = sqlx::query(&all_sql)
        .bind(actor_id)
        .fetch_all(&pool)
        .await
        .expect("fetch all entries for ordering check");

    let timestamps: Vec<chrono::DateTime<chrono::Utc>> = entries
        .iter()
        .map(|r| r.try_get("occurred_at").unwrap())
        .collect();

    // Verify ordering: each entry should have occurred_at <= previous
    for window in timestamps.windows(2) {
        assert!(
            window[0] >= window[1],
            "Results should be ordered by occurred_at DESC"
        );
    }

    cleanup(&pool, &[actor_id]).await;
}
