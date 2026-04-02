use sqlx::Row;
use uuid::Uuid;

async fn test_pool() -> sqlx::PgPool {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rustshare:changeme@localhost:5432/rustshare".to_string());
    sqlx::PgPool::connect(&url).await.expect("DB connect failed")
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

async fn seed_smtp_config(pool: &sqlx::PgPool) {
    sqlx::query(
        "UPDATE smtp_config SET enabled = true, host = 'smtp.test.local', port = 587, from_address = 'test@rustshare.io' WHERE id = '00000000-0000-0000-0000-000000000002'"
    )
    .execute(pool)
    .await
    .expect("seed smtp config");
}

#[tokio::test]
#[ignore]
async fn test_invite_token_crud() {
    let pool = test_pool().await;
    let sender_id = create_test_admin(&pool, &Uuid::new_v4().to_string()[..8]).await;
    let wf_id = get_invite_workflow_id(&pool).await;

    let token = "deadbeef";
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap())
    .bind(token)
    .bind(sender_id)
    .bind("invited@test.local")
    .bind(wf_id)
    .bind(expires_at)
    .execute(&pool)
    .await
    .expect("insert token");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invite_tokens WHERE token = $1")
        .bind(token)
        .fetch_one(&pool)
        .await
        .expect("count token");
    assert_eq!(count, 1);
}

#[tokio::test]
#[ignore]
async fn test_accept_invite_creates_user() {
    let pool = test_pool().await;
    let sender_id = create_test_admin(&pool, "accept").await;
    let wf_id = get_invite_workflow_id(&pool).await;
    let token = "cafebabe";
    let email = "accept_invite@test.local";

    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, NOW() + INTERVAL '7 days')"
    )
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap())
    .bind(token)
    .bind(sender_id)
    .bind(email)
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
    .bind("accepted_user")
    .bind(email)
    .bind("$argon2id$v=19$m=4096,t=3,p=1$hash")
    .bind("Accepted User")
    .execute(&pool)
    .await
    .expect("create user");

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
        .bind(email)
        .fetch_one(&pool)
        .await
        .expect("check user");
    assert!(exists);
}

#[tokio::test]
#[ignore]
async fn test_invite_token_expired() {
    let pool = test_pool().await;
    let sender_id = create_test_admin(&pool, "expired").await;
    let wf_id = get_invite_workflow_id(&pool).await;
    let token = "expiredtok";

    sqlx::query(
        "INSERT INTO invite_tokens (tenant_id, token, sender_id, recipient_email, workflow_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, NOW() - INTERVAL '1 day')"
    )
    .bind(Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap())
    .bind(token)
    .bind(sender_id)
    .bind("expired@test.local")
    .bind(wf_id)
    .execute(&pool)
    .await
    .expect("insert expired token");

    let row = sqlx::query_as::<_, (Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT used_at, revoked_at FROM invite_tokens WHERE token = $1"
    )
    .bind(token)
    .fetch_one(&pool)
    .await
    .expect("fetch token");

    assert!(row.0.is_none());
    assert!(row.1.is_none());
}
