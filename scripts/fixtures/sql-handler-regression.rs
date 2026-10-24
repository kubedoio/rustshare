use sqlx::{query as run_query, query_scalar as scalar, query_file_scalar as file_scalar, raw_sql as execute_raw};
use sqlx as db;
use sqlx::{self as nested_db};
use ::sqlx::QueryBuilder as Builder;
use sqlx::*;

async fn new_handler_query(pool: &sqlx::PgPool) {
    let _ = run_query("SELECT 1").fetch_one(pool).await;
    let _ = sqlx::query("SELECT 2").fetch_one(pool).await;
    let _ = db::query("SELECT 3").fetch_one(pool).await;
    let _ = nested_db::query("SELECT 3").fetch_one(pool).await;
    let _ = scalar("SELECT 4").fetch_one(pool).await;
    let _ = file_scalar!("fixture.sql");
    let _ = execute_raw("DELETE FROM fixture").execute(pool).await;
    let _ = Builder::<sqlx::Postgres>::new("SELECT 5");
    let _ = ::sqlx::query_scalar!(i64, "SELECT 6");
    let _ = sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT 7");
    let _ = query_as::<_, i64>("SELECT 8");
    let _ = pool.execute("SELECT 9").await;
    let _ = sqlx::Executor::execute(pool, "SELECT 10").await;
}
