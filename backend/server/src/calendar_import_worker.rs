use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt;
use rustshare_storage::{MetadataStore, OutboxStore};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::config::AppConfig;

pub struct CalendarImportWorkerConfig {
    pub poll_interval: Duration,
    pub max_concurrent_jobs: usize,
    pub stale_threshold: Duration,
}

impl CalendarImportWorkerConfig {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            poll_interval: Duration::from_secs(config.calendar_import_worker_poll_secs),
            max_concurrent_jobs: config.calendar_import_worker_max_concurrent,
            stale_threshold: Duration::from_secs(
                config.calendar_import_worker_stale_secs.max(0) as u64
            ),
        }
    }
}

pub fn spawn_calendar_import_worker(
    metadata_store: Arc<MetadataStore>,
    outbox: Arc<OutboxStore>,
    mut shutdown: broadcast::Receiver<()>,
    config: CalendarImportWorkerConfig,
) {
    tokio::spawn(async move {
        let mut join_set = tokio::task::JoinSet::new();
        let mut in_flight_ids: HashSet<Uuid> = HashSet::new();

        loop {
            // Do not reset jobs that this worker is actively processing;
            // their updated_at is refreshed after each batch by the active
            // job processor.
            match metadata_store
                .reset_stale_running_calendar_import_jobs(
                    config.stale_threshold,
                    &in_flight_ids.iter().copied().collect::<Vec<_>>(),
                )
                .await
            {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!("Reset {} stale running calendar import jobs", count);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to reset stale running calendar import jobs: {e}");
                }
            }

            while in_flight_ids.len() < config.max_concurrent_jobs {
                let job = match metadata_store
                    .claim_next_pending_calendar_import_job()
                    .await
                {
                    Ok(Some(j)) => j,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!("Failed to claim calendar import job: {e}");
                        break;
                    }
                };

                let job_id = job.id;
                in_flight_ids.insert(job_id);
                let store = Arc::clone(&metadata_store);
                let outbox = Arc::clone(&outbox);
                join_set.spawn(async move {
                    // Catch panics inside the task so the job_id is always
                    // returned and the in-flight set is cleaned up.
                    let result = AssertUnwindSafe(async move {
                        tracing::info!("Processing calendar import job {}", job.id);
                        let result =
                            crate::services::ical_import::process_import_job(&store, &outbox, &job)
                                .await;
                        if let Err(e) = result {
                            tracing::error!("Calendar import job {} failed: {e}", job.id);
                        }
                        job.id
                    })
                    .catch_unwind()
                    .await;
                    match result {
                        Ok(id) => id,
                        Err(e) => {
                            tracing::error!("Calendar import task {job_id} panicked: {e:?}");
                            job_id
                        }
                    }
                });
            }

            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("Calendar import worker shutting down");
                    break;
                }
                _ = tokio::time::sleep(config.poll_interval) => {}
                res = join_set.join_next(), if !join_set.is_empty() => {
                    match res {
                        Some(Ok(job_id)) => {
                            in_flight_ids.remove(&job_id);
                        }
                        Some(Err(e)) => {
                            tracing::error!("Calendar import task panicked or was aborted: {e}");
                        }
                        None => {}
                    }
                }
            }
        }

        if !join_set.is_empty() {
            tracing::info!(
                "Waiting for {} calendar import tasks to finish",
                in_flight_ids.len()
            );
            let shutdown_timeout = Duration::from_secs(30);
            let _ = tokio::time::timeout(shutdown_timeout, async {
                while let Some(res) = join_set.join_next().await {
                    if let Err(e) = res {
                        tracing::error!("Calendar import task panicked or was aborted: {e}");
                    }
                }
            })
            .await;
        }

        tracing::info!("Calendar import worker stopped");
    });
}
