use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug)]
pub struct MemoryOrphanCleanupStatus {
    pub status: String,
    pub lease_expired: Option<bool>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub cursor_chunk_id: Option<Uuid>,
    pub scanned: i64,
    pub candidates: i64,
    pub removed: i64,
    pub unresolved: i64,
    pub last_error: Option<String>,
    pub retry_count: i32,
    pub next_run_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct MemoryOrphanCleanupRepository {
    pool: PgPool,
}

impl MemoryOrphanCleanupRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn status(
        &self,
        tenant_id: Uuid,
    ) -> anyhow::Result<Option<MemoryOrphanCleanupStatus>> {
        let row = sqlx::query("SELECT status, lease_until<=clock_timestamp() AS lease_expired, last_success_at, cursor_chunk_id, scanned, candidates, removed, unresolved, last_error, retry_count, next_run_at FROM memory_note_orphan_cleanup_jobs WHERE tenant_id=$1")
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| {
            Ok(MemoryOrphanCleanupStatus {
                status: row.try_get("status")?,
                lease_expired: row.try_get("lease_expired")?,
                last_success_at: row.try_get("last_success_at")?,
                cursor_chunk_id: row.try_get("cursor_chunk_id")?,
                scanned: row.try_get("scanned")?,
                candidates: row.try_get("candidates")?,
                removed: row.try_get("removed")?,
                unresolved: row.try_get("unresolved")?,
                last_error: row.try_get("last_error")?,
                retry_count: row.try_get("retry_count")?,
                next_run_at: row.try_get("next_run_at")?,
            })
        })
        .transpose()
    }

    pub async fn retry(&self, tenant_id: Uuid) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET LOCAL statement_timeout='5s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL lock_timeout='5s'")
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query("INSERT INTO memory_note_orphan_cleanup_jobs (tenant_id, status, next_run_at) VALUES ($1,'idle',clock_timestamp()) ON CONFLICT (tenant_id) DO UPDATE SET status='idle', retry_count=0, last_error=NULL, next_run_at=clock_timestamp(), lease_owner=NULL, lease_until=NULL, heartbeat_at=NULL, lease_generation=memory_note_orphan_cleanup_jobs.lease_generation+1, updated_at=clock_timestamp() WHERE memory_note_orphan_cleanup_jobs.status <> 'running' OR memory_note_orphan_cleanup_jobs.lease_until<=clock_timestamp()")
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected() == 1)
    }
}
