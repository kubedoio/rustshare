//! Background worker for automatic trash cleanup.

use std::{sync::Arc, time::Duration};

use rustshare_storage::MetadataStore;
use tracing::{error, info, warn};

use crate::state::AppAiService;

#[derive(Debug, Clone)]
pub struct TrashCleanupConfig {
    pub enabled: bool,
    pub interval: Duration,
}

impl TrashCleanupConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("TRASH_CLEANUP_ENABLED")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(true),
            interval: Duration::from_secs(
                std::env::var("TRASH_CLEANUP_INTERVAL_SECS")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(86_400), // 24 hours
            ),
        }
    }
}

pub fn spawn_trash_cleanup_worker(
    metadata_store: Arc<MetadataStore>,
    ai_service: Option<Arc<AppAiService>>,
    config: TrashCleanupConfig,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    if !config.enabled {
        info!("Trash cleanup worker disabled");
        return;
    }

    tokio::spawn(async move {
        info!(
            interval_secs = config.interval.as_secs(),
            "Trash cleanup worker started"
        );

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    info!("Trash cleanup worker shutting down");
                    break;
                }
                _ = tokio::time::sleep(config.interval) => {
                    if let Err(error) = tick_trash_cleanup(&metadata_store, ai_service.clone()).await {
                        error!(error = %error, "Trash cleanup tick failed");
                    }
                }
            }
        }
    });
}

async fn tick_trash_cleanup(
    metadata_store: &MetadataStore,
    ai_service: Option<Arc<AppAiService>>,
) -> anyhow::Result<()> {
    let users = metadata_store
        .list_users_with_trash_retention()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list users with trash retention: {}", e))?;

    if users.is_empty() {
        return Ok(());
    }

    let mut total_cleaned = 0usize;

    for (user_id, tenant_id, days) in users {
        match metadata_store
            .clean_old_trash(user_id, tenant_id, days)
            .await
        {
            Ok(cleaned) => {
                if !cleaned.is_empty() {
                    info!(
                        user_id = %user_id,
                        cleaned = cleaned.len(),
                        retention_days = days,
                        "Auto-cleaned old trash items"
                    );
                }

                if let Some(ref ai_service) = ai_service {
                    for file_id in &cleaned {
                        if let Err(e) = ai_service.remove_file(*file_id, tenant_id).await {
                            warn!(
                                file_id = %file_id,
                                tenant_id = %tenant_id,
                                error = %e,
                                "Failed to remove file chunk from AI index during trash cleanup"
                            );
                        }
                    }
                }

                total_cleaned += cleaned.len();
            }
            Err(e) => {
                warn!(
                    user_id = %user_id,
                    error = %e,
                    "Failed to clean trash for user"
                );
            }
        }
    }

    if total_cleaned > 0 {
        info!(total_cleaned, "Trash cleanup tick completed");
    }

    Ok(())
}
