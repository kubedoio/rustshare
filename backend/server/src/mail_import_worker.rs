use std::sync::Arc;
use std::time::Duration;

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
        let mut in_flight: usize = 0;

        loop {
            match metadata_store
                .reset_stale_running_mail_import_jobs(config.stale_threshold)
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

            while in_flight < config.max_concurrent_jobs {
                let job = match metadata_store.claim_next_pending_mail_import_job().await {
                    Ok(Some(j)) => j,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!("Failed to claim mail import job: {e}");
                        break;
                    }
                };

                in_flight += 1;
                let service = Arc::clone(&mail_service);
                join_set.spawn(async move {
                    tracing::info!("Processing mail import job {}", job.id);
                    if let Err(e) = service.process_import_job(&job).await {
                        tracing::error!("Mail import job {} failed: {e}", job.id);
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
                    in_flight = in_flight.saturating_sub(1);
                    if let Some(Err(e)) = res {
                        tracing::error!("Mail import task panicked or was aborted: {e}");
                    }
                }
            }
        }

        if !join_set.is_empty() {
            tracing::info!("Waiting for {in_flight} mail import tasks to finish");
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
