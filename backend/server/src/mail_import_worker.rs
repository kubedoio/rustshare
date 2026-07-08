use std::sync::Arc;
use std::time::Duration;

use rustshare_storage::MetadataStore;
use tokio::sync::{broadcast, Semaphore};

pub struct MailImportWorkerConfig {
    pub poll_interval: Duration,
    pub max_concurrent_jobs: usize,
}

impl Default for MailImportWorkerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(10),
            max_concurrent_jobs: 2,
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
        let sem = Arc::new(Semaphore::new(config.max_concurrent_jobs));
        loop {
            let timeout = tokio::time::sleep(config.poll_interval);
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("Mail import worker shutting down");
                    break;
                }
                _ = timeout => {}
            }

            loop {
                let permit = match sem.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => break,
                };

                let job = match metadata_store.claim_next_pending_mail_import_job().await {
                    Ok(Some(j)) => j,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!("Failed to claim mail import job: {e}");
                        break;
                    }
                };

                let service = Arc::clone(&mail_service);
                tokio::spawn(async move {
                    let _permit = permit;
                    tracing::info!("Processing mail import job {}", job.id);
                    if let Err(e) = service.process_import_job(&job).await {
                        tracing::error!("Mail import job {} failed: {e}", job.id);
                    }
                });
            }
        }
    });
}
