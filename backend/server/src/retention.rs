//! Background worker for data retention cleanup.

use std::{sync::Arc, time::Duration};

use rustshare_storage::{MetadataStore, ObjectStore};
use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub struct RetentionConfig {
    pub enabled: bool,
    pub interval: Duration,
    pub audit_log_days: i64,
    pub file_version_days: i64,
    pub expired_session_days: i64,
    pub expired_share_days: i64,
    pub replication_history_days: i64,
    pub oidc_state_days: i64,
    pub device_pair_days: i64,
    pub webhook_log_days: i64,
    pub object_gc_terminal_days: i64,
}

impl RetentionConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: env_bool("RETENTION_CLEANUP_ENABLED", true),
            interval: Duration::from_secs(env_u64("RETENTION_CLEANUP_INTERVAL_SECS", 86_400)),
            audit_log_days: env_i64("RETENTION_AUDIT_LOG_DAYS", 365),
            file_version_days: env_i64("RETENTION_FILE_VERSIONS_DAYS", 180),
            expired_session_days: env_i64("RETENTION_EXPIRED_SESSIONS_DAYS", 30),
            expired_share_days: env_i64("RETENTION_EXPIRED_SHARES_DAYS", 30),
            replication_history_days: env_i64("RETENTION_REPLICATION_HISTORY_DAYS", 90),
            oidc_state_days: env_i64("RETENTION_OIDC_STATE_DAYS", 1),
            device_pair_days: env_i64("RETENTION_DEVICE_PAIR_DAYS", 7),
            webhook_log_days: env_i64("RETENTION_WEBHOOK_LOG_DAYS", 30),
            object_gc_terminal_days: env_i64("RETENTION_OBJECT_GC_TERMINAL_DAYS", 7),
        }
    }
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_i64(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

pub fn spawn_retention_cleanup_worker(
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    config: RetentionConfig,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    if !config.enabled {
        info!("Retention cleanup worker disabled");
        return;
    }

    tokio::spawn(async move {
        info!(
            interval_secs = config.interval.as_secs(),
            "Retention cleanup worker started"
        );

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    info!("Retention cleanup worker shutting down");
                    break;
                }
                _ = tokio::time::sleep(config.interval) => {
                    if let Err(error) = tick_retention_cleanup(&metadata_store, &object_store, &config).await {
                        error!(error = %error, "Retention cleanup tick failed");
                    }
                }
            }
        }
    });
}

async fn tick_retention_cleanup(
    store: &MetadataStore,
    object_store: &ObjectStore,
    config: &RetentionConfig,
) -> anyhow::Result<()> {
    let mut total_cleaned = 0u64;

    match store
        .clean_audit_logs_older_than(config.audit_log_days)
        .await
    {
        Ok(cleaned) => {
            if cleaned > 0 {
                info!(cleaned, category = "audit_logs", "Retention cleanup");
            }
            total_cleaned += cleaned;
        }
        Err(e) => {
            warn!(error = %e, category = "audit_logs", "Retention cleanup failed");
        }
    }

    match store
        .clean_old_file_versions(config.file_version_days)
        .await
    {
        Ok(cleaned) => {
            if cleaned > 0 {
                info!(cleaned, category = "file_versions", "Retention cleanup");
            }
            total_cleaned += cleaned;
        }
        Err(e) => {
            warn!(error = %e, category = "file_versions", "Retention cleanup failed");
        }
    }

    match store
        .clean_expired_sessions_older_than(config.expired_session_days)
        .await
    {
        Ok(cleaned) => {
            if cleaned > 0 {
                info!(cleaned, category = "expired_sessions", "Retention cleanup");
            }
            total_cleaned += cleaned;
        }
        Err(e) => {
            warn!(error = %e, category = "expired_sessions", "Retention cleanup failed");
        }
    }

    match store
        .clean_expired_shares_older_than(config.expired_share_days)
        .await
    {
        Ok(cleaned) => {
            if cleaned > 0 {
                info!(cleaned, category = "expired_shares", "Retention cleanup");
            }
            total_cleaned += cleaned;
        }
        Err(e) => {
            warn!(error = %e, category = "expired_shares", "Retention cleanup failed");
        }
    }

    match store
        .clean_replication_history_older_than(config.replication_history_days)
        .await
    {
        Ok(cleaned) => {
            if cleaned > 0 {
                info!(
                    cleaned,
                    category = "replication_history",
                    "Retention cleanup"
                );
            }
            total_cleaned += cleaned;
        }
        Err(e) => {
            warn!(
                error = %e,
                category = "replication_history",
                "Retention cleanup failed"
            );
        }
    }

    match store
        .clean_oidc_states_older_than(config.oidc_state_days)
        .await
    {
        Ok(cleaned) => {
            if cleaned > 0 {
                info!(cleaned, category = "oidc_states", "Retention cleanup");
            }
            total_cleaned += cleaned;
        }
        Err(e) => {
            warn!(error = %e, category = "oidc_states", "Retention cleanup failed");
        }
    }

    match store
        .clean_device_pairs_older_than(config.device_pair_days)
        .await
    {
        Ok(cleaned) => {
            if cleaned > 0 {
                info!(cleaned, category = "device_pairs", "Retention cleanup");
            }
            total_cleaned += cleaned;
        }
        Err(e) => {
            warn!(error = %e, category = "device_pairs", "Retention cleanup failed");
        }
    }

    match store
        .clean_webhook_logs_older_than(config.webhook_log_days)
        .await
    {
        Ok(cleaned) => {
            if cleaned > 0 {
                info!(cleaned, category = "webhook_logs", "Retention cleanup");
            }
            total_cleaned += cleaned;
        }
        Err(e) => {
            warn!(error = %e, category = "webhook_logs", "Retention cleanup failed");
        }
    }

    match store
        .clean_terminal_object_gc_candidates(config.object_gc_terminal_days)
        .await
    {
        Ok(cleaned) => {
            if cleaned > 0 {
                info!(cleaned, category = "object_gc_queue", "Retention cleanup");
            }
            total_cleaned += cleaned;
        }
        Err(e) => {
            warn!(error = %e, category = "object_gc_queue", "Retention cleanup failed");
        }
    }

    match store.list_ready_object_gc_candidates(100).await {
        Ok(keys) => {
            for key in keys {
                // Backstop: the ready-candidate query already excludes
                // content-addressed blobs. They can be reused by a writer
                // between the database reference check and the external
                // object deletion, so keep them queued until writers and GC
                // share a cross-process lease.
                if is_content_addressed_blob_key(&key) {
                    continue;
                }
                if !store.is_unreferenced_object_gc_candidate(&key).await? {
                    continue;
                }
                match object_store.delete(&key).await {
                    Ok(()) => {
                        store.remove_object_gc_candidate(&key).await?;
                        total_cleaned += 1;
                    }
                    Err(e) => {
                        warn!(error = %e, object_key = %key, "Object garbage collection failed")
                    }
                }
            }
        }
        Err(e) => warn!(error = %e, category = "object_gc", "Retention cleanup failed"),
    }

    if total_cleaned > 0 {
        info!(total_cleaned, "Retention cleanup tick completed");
    }

    Ok(())
}

fn is_content_addressed_blob_key(key: &str) -> bool {
    key.strip_prefix("blobs/").is_some_and(|digest| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

#[cfg(test)]
mod tests {
    use super::is_content_addressed_blob_key;

    #[test]
    fn recognizes_only_content_addressed_blob_keys() {
        assert!(is_content_addressed_blob_key(&format!(
            "blobs/{}",
            "a".repeat(64)
        )));
        assert!(is_content_addressed_blob_key(&format!(
            "blobs/{}",
            "A".repeat(64)
        )));
        assert!(is_content_addressed_blob_key(&format!(
            "blobs/{}{}",
            "a".repeat(32),
            "A".repeat(32)
        )));
        assert!(!is_content_addressed_blob_key("blobs/not-a-sha256"));
        assert!(!is_content_addressed_blob_key(&format!(
            "mail/{}",
            "a".repeat(64)
        )));
    }
}
