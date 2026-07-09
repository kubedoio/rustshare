use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use futures_util::FutureExt;
use rustshare_core::domain::MailImportJobId;
use rustshare_storage::MetadataStore;
use tokio::sync::broadcast;

use crate::config::AppConfig;

pub struct MailImportWorkerConfig {
    pub poll_interval: Duration,
    pub max_concurrent_jobs: usize,
    pub stale_threshold: Duration,
}

impl MailImportWorkerConfig {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            poll_interval: Duration::from_secs(config.mail_import_worker_poll_secs),
            max_concurrent_jobs: config.mail_import_worker_max_concurrent,
            stale_threshold: Duration::from_secs(config.mail_import_worker_stale_secs.max(0) as u64),
        }
    }
}

pub fn spawn_mail_import_worker(
    mail_service: Arc<crate::services::mail_service::MailService>,
    metadata_store: Arc<MetadataStore>,
    mut shutdown: broadcast::Receiver<()>,
    config: MailImportWorkerConfig,
) {
    tokio::spawn(async move {
        let mut join_set = tokio::task::JoinSet::new();
        let mut in_flight_ids: HashSet<MailImportJobId> = HashSet::new();

        loop {
            // Do not reset jobs that this worker is actively processing;
            // their updated_at is refreshed after each UID by process_import_job.
            match metadata_store
                .reset_stale_running_mail_import_jobs(
                    config.stale_threshold,
                    &in_flight_ids.iter().copied().collect::<Vec<_>>(),
                )
                .await
            {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!("Reset {} stale running mail import jobs", count);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to reset stale running mail import jobs: {e}");
                }
            }

            while in_flight_ids.len() < config.max_concurrent_jobs {
                let job = match metadata_store.claim_next_pending_mail_import_job().await {
                    Ok(Some(j)) => j,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!("Failed to claim mail import job: {e}");
                        break;
                    }
                };

                let job_id = job.id;
                in_flight_ids.insert(job_id);
                let service = Arc::clone(&mail_service);
                join_set.spawn(async move {
                    // Catch panics inside the task so the job_id is always returned
                    // and the in-flight set is cleaned up.
                    let result = AssertUnwindSafe(async move {
                        tracing::info!("Processing mail import job {}", job.id);
                        if let Err(e) = service.process_import_job(&job).await {
                            tracing::error!("Mail import job {} failed: {e}", job.id);
                        }
                        job.id
                    })
                    .catch_unwind()
                    .await;
                    match result {
                        Ok(id) => id,
                        Err(e) => {
                            tracing::error!("Mail import task {job_id} panicked: {e:?}");
                            job_id
                        }
                    }
                });
            }

            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("Mail import worker shutting down");
                    break;
                }
                _ = tokio::time::sleep(config.poll_interval) => {}
                res = join_set.join_next(), if !join_set.is_empty() => {
                    match res {
                        Some(Ok(job_id)) => {
                            in_flight_ids.remove(&job_id);
                        }
                        Some(Err(e)) => {
                            tracing::error!("Mail import task panicked or was aborted: {e}");
                        }
                        None => {}
                    }
                }
            }
        }

        if !join_set.is_empty() {
            tracing::info!(
                "Waiting for {} mail import tasks to finish",
                in_flight_ids.len()
            );
            let shutdown_timeout = Duration::from_secs(30);
            let _ = tokio::time::timeout(shutdown_timeout, async {
                while let Some(res) = join_set.join_next().await {
                    if let Err(e) = res {
                        tracing::error!("Mail import task panicked or was aborted: {e}");
                    }
                }
            })
            .await;
        }

        tracing::info!("Mail import worker stopped");
    });
}
