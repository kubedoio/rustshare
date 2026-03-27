//! Replication worker for background file replication.
//!
//! TODO: This module needs to be rewritten to use the new JobRepository
//! for replication job tracking instead of PostgreSQL.

use std::{sync::Arc, time::Duration};

use rustshare_core::events::EventBroadcaster;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use tracing::info;

#[derive(Debug, Clone)]
pub struct ReplicationWorkerConfig {
    pub enabled: bool,
    #[allow(dead_code)]
    pub poll_interval: Duration,
    #[allow(dead_code)]
    pub batch_size: i64,
    #[allow(dead_code)]
    pub lease_timeout_secs: i64,
    #[allow(dead_code)]
    pub max_attempts: i32,
}

impl ReplicationWorkerConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("REPLICATION_WORKER_ENABLED")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(false), // Disabled by default in zero-PostgreSQL mode
            poll_interval: Duration::from_millis(
                std::env::var("REPLICATION_WORKER_POLL_INTERVAL_MS")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(5_000),
            ),
            batch_size: std::env::var("REPLICATION_WORKER_BATCH_SIZE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8),
            lease_timeout_secs: std::env::var("REPLICATION_WORKER_LEASE_TIMEOUT_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(120),
            max_attempts: std::env::var("REPLICATION_WORKER_MAX_ATTEMPTS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8),
        }
    }
}

pub fn spawn_replication_worker(
    _metadata_store: Arc<MetadataStore>,
    _object_store: Arc<ObjectStore>,
    _event_store: Arc<EventStore>,
    _broadcaster: Arc<EventBroadcaster>,
    config: ReplicationWorkerConfig,
) {
    if !config.enabled {
        info!("Replication worker disabled");
        return;
    }

    info!("Replication worker not yet implemented in zero-PostgreSQL mode - disabling");
    // TODO: Implement replication worker using new JobRepository
}
