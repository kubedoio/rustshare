//! Memory-owned reconciliation driven by the authenticated Files API.

use std::{sync::Arc, time::Duration};

use rustshare_core::domain::ApplicationId;
use rustshare_resource_auth::ResourceRef;
use rustshare_storage::MetadataStore;
use sqlx::PgPool;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    handlers::memory_reconcile::FilesReconcileResponse,
    memory_files_reconcile_worker::{self, MemoryFilesLease},
    memory_files_worker_consumer::{MemoryFilesWorkerConsumer, SnapshotProjectionOutcome},
    memory_worker_files_client::MemoryWorkerFilesClient,
};

const PAGE_SIZE: u32 = 100;

#[derive(Clone, Copy)]
pub struct ExternalReconcileConfig {
    pub poll: Duration,
    pub interval: Duration,
    pub lease: Duration,
    pub page_timeout: Duration,
}

impl ExternalReconcileConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let config = Self {
            poll: Duration::from_secs(env_u64("MEMORY_RECONCILE_POLL_SECS", 5)?),
            interval: Duration::from_secs(env_u64("MEMORY_RECONCILE_INTERVAL_SECS", 300)?),
            lease: Duration::from_secs(env_u64("MEMORY_RECONCILE_LEASE_SECS", 90)?),
            page_timeout: Duration::from_secs(env_u64("MEMORY_RECONCILE_PAGE_TIMEOUT_SECS", 45)?),
        };
        anyhow::ensure!(
            (1..=60).contains(&config.poll.as_secs()),
            "Memory reconcile poll must be 1..=60 seconds"
        );
        anyhow::ensure!(
            (60..=86_400).contains(&config.interval.as_secs()),
            "Memory reconcile interval must be 60..=86400 seconds"
        );
        anyhow::ensure!(
            (30..=3_600).contains(&config.lease.as_secs()),
            "Memory reconcile lease must be 30..=3600 seconds"
        );
        anyhow::ensure!(
            (1..=3_600).contains(&config.page_timeout.as_secs()),
            "Memory reconcile page timeout must be 1..=3600 seconds"
        );
        Ok(config)
    }
}

fn env_u64(name: &str, default: u64) -> anyhow::Result<u64> {
    std::env::var(name).map_or(Ok(default), |value| {
        value.parse::<u64>().map_err(Into::into)
    })
}

pub fn spawn(
    pool: PgPool,
    files: MemoryWorkerFilesClient,
    consumer: Arc<MemoryFilesWorkerConsumer>,
    config: ExternalReconcileConfig,
    mut shutdown: broadcast::Receiver<()>,
) -> (
    tokio::task::JoinHandle<()>,
    Arc<std::sync::atomic::AtomicBool>,
) {
    let healthy = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_healthy = Arc::clone(&healthy);
    let task = tokio::spawn(async move {
        let metadata = MetadataStore::new(pool.clone());
        let owner = Uuid::new_v4();
        loop {
            if let Err(error) = discover(&pool, &files).await {
                worker_healthy.store(false, std::sync::atomic::Ordering::Release);
                tracing::warn!(%error, "Memory source reconciliation discovery failed");
            } else {
                worker_healthy.store(true, std::sync::atomic::Ordering::Release);
            }
            match memory_files_reconcile_worker::claim(&metadata, owner, config.lease).await {
                Ok(Some(lease)) => {
                    if let Err(error) =
                        process_page(&metadata, &files, &consumer, config, owner, lease).await
                    {
                        worker_healthy.store(false, std::sync::atomic::Ordering::Release);
                        tracing::warn!(%error, "Memory source reconciliation page failed");
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    worker_healthy.store(false, std::sync::atomic::Ordering::Release);
                    tracing::warn!(%error, "Memory source reconciliation claim failed");
                }
            }
            if let Err(error) = record_metrics(&pool).await {
                tracing::warn!(%error, "Memory reconciliation metrics query failed");
            }
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(config.poll) => {}
            }
        }
    });
    (task, healthy)
}

async fn record_metrics(pool: &PgPool) -> anyhow::Result<()> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT status, count(*) FROM memory_files_reconcile_jobs GROUP BY status")
            .fetch_all(pool)
            .await?;
    for status in ["idle", "running", "failed", "paused"] {
        let count = rows
            .iter()
            .find(|(row_status, _)| row_status == status)
            .map_or(0, |(_, count)| *count);
        metrics::gauge!("memory_files_reconcile_jobs", "status" => status.to_owned())
            .set(count as f64);
    }
    let lag_seconds: Option<f64> = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM (NOW()-MIN(next_run_at)))::double precision \
         FROM memory_files_reconcile_jobs WHERE status='idle' AND next_run_at<=NOW()",
    )
    .fetch_one(pool)
    .await?;
    metrics::gauge!("memory_files_reconcile_lag_seconds").set(lag_seconds.unwrap_or(0.0));
    Ok(())
}

async fn discover(pool: &PgPool, files: &MemoryWorkerFilesClient) -> anyhow::Result<()> {
    let cursor: Option<Uuid> = sqlx::query_scalar(
        "SELECT cursor_tenant_id FROM memory_files_reconcile_discovery WHERE singleton=true",
    )
    .fetch_one(pool)
    .await?;
    let page = files.tenant_page(cursor, PAGE_SIZE).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO memory_files_reconcile_jobs (tenant_id) SELECT unnest($1::uuid[]) ON CONFLICT (tenant_id) DO NOTHING")
        .bind(&page.tenant_ids)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE memory_files_reconcile_discovery SET cursor_tenant_id=$1 WHERE singleton=true",
    )
    .bind(page.next_cursor)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn process_page(
    metadata: &MetadataStore,
    files: &MemoryWorkerFilesClient,
    consumer: &MemoryFilesWorkerConsumer,
    config: ExternalReconcileConfig,
    owner: Uuid,
    lease: MemoryFilesLease,
) -> anyhow::Result<()> {
    let work = async {
        let page = files
            .reconcile_page(lease.tenant_id, lease.cursor, PAGE_SIZE)
            .await?;
        let mut counts = FilesReconcileResponse {
            processed: 0,
            indexed: 0,
            removed: 0,
            unavailable: 0,
            next_cursor: page.next_cursor.map(|id| {
                ResourceRef::new(
                    ApplicationId::new("io.elembra.files"),
                    "file",
                    id.to_string(),
                )
                .to_uri()
            }),
        };
        for snapshot in page.snapshots {
            counts.processed += 1;
            let unavailable = matches!(
                &snapshot,
                rustshare_memory::files_source::FilesProjectionSnapshot::Ignored { .. }
            );
            let file_id = match &snapshot {
                rustshare_memory::files_source::FilesProjectionSnapshot::Available(value) => {
                    value.file_id
                }
                rustshare_memory::files_source::FilesProjectionSnapshot::Deleted {
                    file_id,
                    ..
                }
                | rustshare_memory::files_source::FilesProjectionSnapshot::Ignored {
                    file_id,
                    ..
                } => *file_id,
            };
            let outcome = consumer
                .project_snapshot(lease.tenant_id, file_id, snapshot)
                .await?;
            if outcome == SnapshotProjectionOutcome::Indexed {
                counts.indexed += 1;
            }
            if outcome == SnapshotProjectionOutcome::Removed {
                counts.removed += 1;
            }
            if unavailable {
                counts.unavailable += 1;
            }
        }
        memory_files_reconcile_worker::finish_page(metadata, &lease, owner, counts, config.interval)
            .await
    };
    tokio::pin!(work);
    let deadline = tokio::time::sleep(config.page_timeout);
    tokio::pin!(deadline);
    let heartbeat = tokio::time::sleep(config.lease / 3);
    tokio::pin!(heartbeat);
    loop {
        tokio::select! {
            biased;
            result = &mut work => {
                if let Err(error) = result {
                    memory_files_reconcile_worker::fail_page(metadata, &lease, owner).await?;
                    return Err(error);
                }
                return Ok(());
            }
            _ = &mut deadline => {
                memory_files_reconcile_worker::fail_page(metadata, &lease, owner).await?;
                anyhow::bail!("Memory reconciliation page timed out");
            }
            _ = &mut heartbeat => {
                memory_files_reconcile_worker::heartbeat(metadata, &lease, owner, config.lease).await?;
                heartbeat.as_mut().reset(tokio::time::Instant::now() + config.lease / 3);
            }
        }
    }
}
