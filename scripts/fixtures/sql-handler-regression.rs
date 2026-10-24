use sqlx::query as run_query;

async fn new_handler_query(pool: &sqlx::PgPool) {
    let _ = run_query("SELECT 1").fetch_one(pool).await;
    let _ = sqlx::query("SELECT 2").fetch_one(pool).await;
}
