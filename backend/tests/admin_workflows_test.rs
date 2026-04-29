use tokio::sync::Mutex;
use uuid::Uuid;

static WORKFLOW_TEST_LOCK: Mutex<()> = Mutex::const_new(());

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
    .bind(format!("wf_admin_{suffix}"))
    .bind(format!("wfadmin_{suffix}@test.local"))
    .bind("$argon2id$v=19$m=4096,t=3,p=1$placeholder_hash")
    .bind(format!("WF Admin {suffix}"))
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
        "UPDATE smtp_config SET enabled = true, host = 'smtp.test.local', port = 587, from_address = 'test@example.com' WHERE id = '00000000-0000-0000-0000-000000000002'"
    )
    .execute(pool)
    .await
    .expect("seed smtp config");
}

async fn reset_workflow_state(pool: &sqlx::PgPool, workflow_id: Uuid) {
    sqlx::query("UPDATE workflows SET status = 'draft' WHERE id = $1")
        .bind(workflow_id)
        .execute(pool)
        .await
        .expect("reset workflow status");
    sqlx::query(
        "UPDATE smtp_config
         SET enabled = false, host = NULL, port = NULL, from_address = NULL
         WHERE id = '00000000-0000-0000-0000-000000000002'",
    )
    .execute(pool)
    .await
    .expect("reset smtp config");
}

#[tokio::test]
#[ignore]
async fn test_list_workflows() {
    let pool = test_pool().await;
    let wf_id = get_invite_workflow_id(&pool).await;
    assert!(!wf_id.to_string().is_empty());
}

#[tokio::test]
#[ignore]
async fn test_enable_workflow_requires_smtp() {
    let _guard = WORKFLOW_TEST_LOCK.lock().await;
    let pool = test_pool().await;
    let wf_id = get_invite_workflow_id(&pool).await;

    reset_workflow_state(&pool, wf_id).await;

    let status: String = sqlx::query_scalar("SELECT status FROM workflows WHERE id = $1")
        .bind(wf_id)
        .fetch_one(&pool)
        .await
        .expect("fetch status");
    assert_eq!(status, "draft");

    reset_workflow_state(&pool, wf_id).await;
}

#[tokio::test]
#[ignore]
async fn test_enable_workflow_with_smtp() {
    let _guard = WORKFLOW_TEST_LOCK.lock().await;
    let pool = test_pool().await;
    let wf_id = get_invite_workflow_id(&pool).await;
    reset_workflow_state(&pool, wf_id).await;
    seed_smtp_config(&pool).await;

    sqlx::query("UPDATE workflows SET status = 'active' WHERE id = $1")
        .bind(wf_id)
        .execute(&pool)
        .await
        .expect("enable workflow");

    let status: String = sqlx::query_scalar("SELECT status FROM workflows WHERE id = $1")
        .bind(wf_id)
        .fetch_one(&pool)
        .await
        .expect("fetch status");
    assert_eq!(status, "active");

    reset_workflow_state(&pool, wf_id).await;
}

#[tokio::test]
#[ignore]
async fn test_disable_workflow() {
    let _guard = WORKFLOW_TEST_LOCK.lock().await;
    let pool = test_pool().await;
    let wf_id = get_invite_workflow_id(&pool).await;
    reset_workflow_state(&pool, wf_id).await;
    seed_smtp_config(&pool).await;

    sqlx::query("UPDATE workflows SET status = 'active' WHERE id = $1")
        .bind(wf_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("UPDATE workflows SET status = 'draft' WHERE id = $1")
        .bind(wf_id)
        .execute(&pool)
        .await
        .ok();

    let status: String = sqlx::query_scalar("SELECT status FROM workflows WHERE id = $1")
        .bind(wf_id)
        .fetch_one(&pool)
        .await
        .expect("fetch status");
    assert_eq!(status, "draft");

    reset_workflow_state(&pool, wf_id).await;
}
