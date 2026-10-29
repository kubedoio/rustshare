//! Durable, tenant-scoped cleanup for indexed Notes whose Files source is gone.

use std::{sync::Arc, time::Duration};

use rustshare_infrastructure::PgVectorStore;
use sqlx::{PgPool, QueryBuilder, Row};
use tokio::{sync::broadcast, task::JoinSet};
use uuid::Uuid;

const MAX_RETRIES: i32 = 8;
const PAGE_SIZE: usize = 100;

#[derive(Clone, Copy)]
pub struct MemoryOrphanCleanupConfig {
    pub interval: Duration,
    pub poll_interval: Duration,
    pub lease: Duration,
    pub max_concurrent: usize,
    pub page_timeout: Duration,
    pub dry_run: bool,
}

impl MemoryOrphanCleanupConfig {
    pub fn from_config(config: &crate::config::AppConfig) -> anyhow::Result<Self> {
        let files =
            crate::memory_files_reconcile_worker::MemoryFilesReconcileConfig::from_config(config)?;
        Ok(Self {
            interval: files.interval,
            poll_interval: files.poll_interval,
            lease: files.lease,
            max_concurrent: files.max_concurrent,
            page_timeout: files.page_timeout,
            dry_run: config.memory_files_orphan_cleanup_dry_run,
        })
    }
}

pub fn spawn_memory_orphan_cleanup_worker(
    pool: PgPool,
    vector_store: Arc<PgVectorStore>,
    config: MemoryOrphanCleanupConfig,
    mut shutdown: broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let worker_id = Uuid::new_v4();
        let mut tasks = JoinSet::new();
        'worker: loop {
            if !matches!(
                tokio::time::timeout(config.page_timeout, discover_tenant_page(&pool)).await,
                Ok(Ok(()))
            ) {
                tracing::error!("Memory orphan cleanup discovery failed or timed out");
            }
            while tasks.len() < config.max_concurrent {
                let claimed = tokio::select! {
                    biased;
                    _ = shutdown.recv() => break 'worker,
                    result = tokio::time::timeout(config.page_timeout, claim(&pool, worker_id, config.lease)) => result,
                };
                match claimed {
                    Ok(Ok(Some(lease))) => {
                        let pool = pool.clone();
                        let vector_store = Arc::clone(&vector_store);
                        tasks.spawn(async move {
                            run_page(pool, vector_store, config, worker_id, lease).await;
                        });
                    }
                    Ok(Ok(None)) => break,
                    _ => {
                        tracing::error!("Memory orphan cleanup claim failed or timed out");
                        break;
                    }
                }
            }
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(config.poll_interval) => {},
                result = tasks.join_next(), if !tasks.is_empty() => {
                    if matches!(result, Some(Err(_))) {
                        tracing::error!("Memory orphan cleanup task stopped; lease will recover");
                    }
                },
            }
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    })
}

#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct MemoryOrphanCleanupLease {
    pub tenant_id: Uuid,
    pub generation: i64,
    pub cursor: Option<Uuid>,
}

async fn lease_transaction(
    pool: &PgPool,
) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

#[doc(hidden)]
pub async fn discover_tenant_page(pool: &PgPool) -> anyhow::Result<()> {
    let mut tx = lease_transaction(pool).await?;
    let cursor: Option<Uuid> = sqlx::query_scalar(
        "SELECT cursor_chunk_id FROM memory_note_orphan_cleanup_discovery WHERE singleton=true FOR UPDATE",
    )
    .fetch_one(&mut *tx)
    .await?;
    let mut query =
        QueryBuilder::<sqlx::Postgres>::new("SELECT id, tenant_id FROM note_index_chunks");
    if let Some(cursor) = cursor {
        query.push(" WHERE id>").push_bind(cursor);
    }
    query.push(" ORDER BY id LIMIT 100");
    let rows = query.build().fetch_all(&mut *tx).await?;
    let next: Option<Uuid> = if rows.len() == 100 {
        rows.last().map(|row| row.try_get("id")).transpose()?
    } else {
        None
    };
    let mut tenants: Vec<Uuid> = Vec::with_capacity(rows.len());
    for row in &rows {
        tenants.push(row.try_get("tenant_id")?);
    }
    tenants.sort_unstable();
    tenants.dedup();
    sqlx::query("INSERT INTO memory_note_orphan_cleanup_jobs (tenant_id) SELECT unnest($1::uuid[]) ON CONFLICT (tenant_id) DO NOTHING")
        .bind(tenants)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE memory_note_orphan_cleanup_discovery SET cursor_chunk_id=$1 WHERE singleton=true",
    )
    .bind(next)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

#[doc(hidden)]
pub async fn claim(
    pool: &PgPool,
    owner: Uuid,
    lease: Duration,
) -> anyhow::Result<Option<MemoryOrphanCleanupLease>> {
    let mut tx = lease_transaction(pool).await?;
    let row = sqlx::query(
        "UPDATE memory_note_orphan_cleanup_jobs SET status='running', lease_owner=$1, lease_until=clock_timestamp()+($2 * INTERVAL '1 second'), heartbeat_at=clock_timestamp(), lease_generation=lease_generation+1, last_claimed_at=clock_timestamp(), updated_at=clock_timestamp() WHERE tenant_id=(SELECT tenant_id FROM memory_note_orphan_cleanup_jobs WHERE status IN ('idle','running') AND next_run_at<=clock_timestamp() AND (lease_until IS NULL OR lease_until<=clock_timestamp()) ORDER BY next_run_at, last_claimed_at NULLS FIRST, tenant_id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING tenant_id, lease_generation, cursor_chunk_id",
    )
    .bind(owner)
    .bind(lease.as_secs() as i64)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    row.map(|row| {
        Ok(MemoryOrphanCleanupLease {
            tenant_id: row.try_get("tenant_id")?,
            generation: row.try_get("lease_generation")?,
            cursor: row.try_get("cursor_chunk_id")?,
        })
    })
    .transpose()
}

#[doc(hidden)]
pub async fn heartbeat(
    pool: &PgPool,
    lease: &MemoryOrphanCleanupLease,
    owner: Uuid,
    duration: Duration,
) -> anyhow::Result<()> {
    let mut tx = lease_transaction(pool).await?;
    let result = sqlx::query("UPDATE memory_note_orphan_cleanup_jobs SET heartbeat_at=clock_timestamp(), lease_until=clock_timestamp()+($4 * INTERVAL '1 second'), updated_at=clock_timestamp() WHERE tenant_id=$1 AND lease_owner=$2 AND lease_generation=$3 AND lease_until>clock_timestamp()")
        .bind(lease.tenant_id).bind(owner).bind(lease.generation).bind(duration.as_secs() as i64)
        .execute(&mut *tx).await?;
    anyhow::ensure!(
        result.rows_affected() == 1,
        "orphan cleanup lease lost before heartbeat"
    );
    tx.commit().await?;
    Ok(())
}

async fn run_page(
    pool: PgPool,
    vector_store: Arc<PgVectorStore>,
    config: MemoryOrphanCleanupConfig,
    owner: Uuid,
    lease: MemoryOrphanCleanupLease,
) {
    // Drop an interrupted page transaction before recording its retry on another connection.
    let result = {
        let page = process_page_and_checkpoint(&pool, &vector_store, config, owner, &lease);
        tokio::pin!(page);
        let deadline = tokio::time::sleep(config.page_timeout);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                biased;
                _ = &mut deadline => break None,
                result = &mut page => break Some(result),
                _ = tokio::time::sleep(config.lease / 3) => {
                    let beat = tokio::select! {
                        biased;
                        _ = &mut deadline => break None,
                        result = &mut page => break Some(result),
                        result = tokio::time::timeout(config.lease / 3, heartbeat(&pool, &lease, owner, config.lease)) => result,
                    };
                    if !matches!(beat, Ok(Ok(()))) {
                        tracing::warn!(tenant_id = %lease.tenant_id, "Memory orphan cleanup lease lost; page will replay");
                        metrics::counter!("memory_orphan_cleanup_pages_total", "outcome" => "lease_lost").increment(1);
                        return;
                    }
                }
            }
        }
    };

    match result {
        Some(Ok(())) => {}
        _ => {
            tracing::warn!(tenant_id = %lease.tenant_id, "Memory orphan cleanup page failed or timed out");
            if !matches!(
                tokio::time::timeout(config.lease / 3, fail_page(&pool, &lease, owner)).await,
                Ok(Ok(()))
            ) {
                tracing::error!(tenant_id = %lease.tenant_id, "Memory orphan cleanup retry update failed; page will replay");
            }
        }
    }
}

#[doc(hidden)]
pub async fn process_page_and_checkpoint(
    pool: &PgPool,
    vector_store: &PgVectorStore,
    config: MemoryOrphanCleanupConfig,
    owner: Uuid,
    lease: &MemoryOrphanCleanupLease,
) -> anyhow::Result<()> {
    let mut tx = lease_transaction(pool).await?;
    let page = vector_store
        .cleanup_orphan_chunks_page_in_tx(
            &mut tx,
            lease.tenant_id,
            lease.cursor,
            PAGE_SIZE,
            config.dry_run,
        )
        .await?;
    checkpoint_page_in_tx(&mut tx, lease, owner, &page, config.interval).await?;
    tx.commit().await?;
    record_page_metrics(&page);
    Ok(())
}

#[doc(hidden)]
pub async fn finish_page(
    pool: &PgPool,
    lease: &MemoryOrphanCleanupLease,
    owner: Uuid,
    page: rustshare_infrastructure::vector::pg_vector_store::OrphanCleanupPage,
    interval: Duration,
) -> anyhow::Result<()> {
    let mut tx = lease_transaction(pool).await?;
    checkpoint_page_in_tx(&mut tx, lease, owner, &page, interval).await?;
    tx.commit().await?;
    record_page_metrics(&page);
    Ok(())
}

async fn checkpoint_page_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    lease: &MemoryOrphanCleanupLease,
    owner: Uuid,
    page: &rustshare_infrastructure::vector::pg_vector_store::OrphanCleanupPage,
    interval: Duration,
) -> anyhow::Result<()> {
    let complete = page.next_cursor.is_none();
    let result = sqlx::query("UPDATE memory_note_orphan_cleanup_jobs SET cursor_chunk_id=$4, scanned=scanned+$5, candidates=candidates+$6, removed=removed+$7, unresolved=unresolved+$8, status='idle', retry_count=0, last_error=NULL, last_success_at=CASE WHEN $9 THEN clock_timestamp() ELSE last_success_at END, next_run_at=CASE WHEN $9 THEN clock_timestamp()+($10 * INTERVAL '1 second') ELSE clock_timestamp() END, lease_owner=NULL, lease_until=NULL, heartbeat_at=NULL, updated_at=clock_timestamp() WHERE tenant_id=$1 AND lease_owner=$2 AND lease_generation=$3 AND lease_until>clock_timestamp()")
        .bind(lease.tenant_id).bind(owner).bind(lease.generation).bind(page.next_cursor).bind(page.scanned as i64).bind(page.candidates as i64).bind(page.removed as i64).bind(page.unresolved as i64).bind(complete).bind(interval.as_secs() as i64)
        .execute(&mut **tx).await?;
    anyhow::ensure!(
        result.rows_affected() == 1,
        "orphan cleanup lease lost before checkpoint"
    );
    Ok(())
}

fn record_page_metrics(
    page: &rustshare_infrastructure::vector::pg_vector_store::OrphanCleanupPage,
) {
    metrics::counter!("memory_orphan_cleanup_candidates_total").increment(page.candidates as u64);
    metrics::counter!("memory_orphan_cleanup_removed_total").increment(page.removed as u64);
    metrics::counter!("memory_orphan_cleanup_unresolved_total").increment(page.unresolved as u64);
    metrics::counter!("memory_orphan_cleanup_pages_total", "outcome" => "success").increment(1);
}

#[doc(hidden)]
pub async fn fail_page(
    pool: &PgPool,
    lease: &MemoryOrphanCleanupLease,
    owner: Uuid,
) -> anyhow::Result<()> {
    let mut tx = lease_transaction(pool).await?;
    let result = sqlx::query("UPDATE memory_note_orphan_cleanup_jobs SET retry_count=retry_count+1, last_error='orphan cleanup page failed', status=CASE WHEN retry_count+1 >= $4 THEN 'failed' ELSE 'idle' END, next_run_at=clock_timestamp()+(LEAST(3600, (30 * (1 << LEAST(retry_count, 7)))::int) * INTERVAL '1 second'), lease_owner=NULL, lease_until=NULL, heartbeat_at=NULL, updated_at=clock_timestamp() WHERE tenant_id=$1 AND lease_owner=$2 AND lease_generation=$3 AND lease_until>clock_timestamp()")
        .bind(lease.tenant_id).bind(owner).bind(lease.generation).bind(MAX_RETRIES).execute(&mut *tx).await?;
    anyhow::ensure!(
        result.rows_affected() == 1,
        "orphan cleanup lease lost before retry update"
    );
    tx.commit().await?;
    metrics::counter!("memory_orphan_cleanup_pages_total", "outcome" => "failure").increment(1);
    Ok(())
}
