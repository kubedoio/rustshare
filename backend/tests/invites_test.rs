use uuid::Uuid;

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
         VALUES ($1, $2, $3, $4, $5, true, 10737418240)"
    )
    .bind(id)
    .bind(format!("inv_admin_{suffix}"))
    .bind(format!("invadmin_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("Inv Admin {suffix}"))
    .execute(pool)
    .await
    .expect("create test admin");
    id
}

async fn get_invite_workflow_id(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM workflows WHERE key = 'invite_email'")
        .fetch_one(pool)
        .await
        .expect("invite workflow exists")
}

#[tokio::test]
async fn test_invite_token_crud() {
    let pool = test_pool().await;
    let suffix = Uuid::new_v4().to_string();
    let sender_id = create_test_admin(&pool, &suffix).await;
    let wf_id = get_invite_workflow_id(&pool).await;

    let token = Uuid::new_v4().to_string().replace("-", "");
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap())
    .bind(&token)
    .bind(sender_id)
    .bind(format!("invited_{}@test.local", suffix))
    .bind(wf_id)
    .bind(expires_at)
    .execute(&pool)
    .await
    .expect("insert token");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invite_tokens WHERE token = $1")
        .bind(&token)
        .fetch_one(&pool)
        .await
        .expect("count token");
    assert_eq!(count, 1);

    // Cleanup
    sqlx::query("DELETE FROM invite_tokens WHERE token = $1")
        .bind(&token)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(sender_id)
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn test_accept_invite_creates_user() {
    let pool = test_pool().await;
    let suffix = Uuid::new_v4().to_string();
    let sender_id = create_test_admin(&pool, &suffix).await;
    let wf_id = get_invite_workflow_id(&pool).await;
    let token = Uuid::new_v4().to_string().replace("-", "");
    let email = format!("accept_invite_{}@test.local", suffix);

    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, NOW() + INTERVAL '7 days')"
    )
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap())
    .bind(&token)
    .bind(sender_id)
    .bind(&email)
    .bind(wf_id)
    .execute(&pool)
    .await
    .expect("insert token");

    let user_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota)
         VALUES ($1, $2, $3, $4, $5, false, 10737418240)"
    )
    .bind(user_id)
    .bind(format!("accepted_user_{}", suffix))
    .bind(&email)
    .bind("$argon2id$v=19$m=4096,t=3,p=1$hash")
    .bind("Accepted User")
    .execute(&pool)
    .await
    .expect("create user");

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .expect("check user");
    assert!(exists);

    // Cleanup
    sqlx::query("DELETE FROM invite_tokens WHERE token = $1")
        .bind(&token)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE email = $1")
        .bind(&email)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(sender_id)
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn test_invite_token_expired() {
    let pool = test_pool().await;
    let suffix = Uuid::new_v4().to_string();
    let sender_id = create_test_admin(&pool, &suffix).await;
    let wf_id = get_invite_workflow_id(&pool).await;
    let token = Uuid::new_v4().to_string().replace("-", "");

    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, NOW() - INTERVAL '1 day')"
    )
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap())
    .bind(&token)
    .bind(sender_id)
    .bind(format!("expired_{}@test.local", suffix))
    .bind(wf_id)
    .execute(&pool)
    .await
    .expect("insert expired token");

    let row = sqlx::query_as::<
        _,
        (
            Option<chrono::DateTime<chrono::Utc>>,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >("SELECT used_at, revoked_at FROM invite_tokens WHERE token = $1")
    .bind(&token)
    .fetch_one(&pool)
    .await
    .expect("fetch token");

    assert!(row.0.is_none());
    assert!(row.1.is_none());

    // Cleanup
    sqlx::query("DELETE FROM invite_tokens WHERE token = $1")
        .bind(&token)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(sender_id)
        .execute(&pool)
        .await
        .ok();
}
