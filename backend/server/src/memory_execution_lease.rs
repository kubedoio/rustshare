//! Renewable process lease for the configured Memory execution mode.

use std::{
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    time::Duration,
};

use rustshare_storage::{MemoryExecutionMode, MemoryExecutionModeStore};
use sqlx::{Postgres, Transaction};
use tokio::{sync::broadcast, task::JoinHandle};
use uuid::Uuid;

pub const MODE_LEASE_DURATION: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct MemoryExecutionFence {
    store: MemoryExecutionModeStore,
    mode: MemoryExecutionMode,
    process_id: Uuid,
    generation: i64,
}

impl MemoryExecutionFence {
    pub fn new(
        store: MemoryExecutionModeStore,
        mode: MemoryExecutionMode,
        process_id: Uuid,
        generation: i64,
    ) -> Self {
        Self {
            store,
            mode,
            process_id,
            generation,
        }
    }

    pub async fn check(&self) -> anyhow::Result<()> {
        self.store
            .renew(
                self.mode,
                self.process_id,
                self.generation,
                MODE_LEASE_DURATION,
            )
            .await?;
        Ok(())
    }

    pub async fn check_in_transaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> anyhow::Result<()> {
        self.store
            .fence_in_transaction(
                tx,
                self.mode,
                self.process_id,
                self.generation,
                MODE_LEASE_DURATION,
            )
            .await?;
        Ok(())
    }
}

pub fn spawn_memory_execution_lease(
    store: MemoryExecutionModeStore,
    mode: MemoryExecutionMode,
    process_id: Uuid,
    generation: i64,
    active: Arc<AtomicBool>,
    mut shutdown: broadcast::Receiver<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut heartbeat = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(10),
            Duration::from_secs(10),
        );
        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    active.store(false, Ordering::Release);
                    if let Err(error) = store.release(mode, process_id, generation).await {
                        tracing::warn!(%error, "Memory execution mode lease release failed; it will expire");
                    }
                    return;
                }
                _ = heartbeat.tick() => {
                    if let Err(error) = store.renew(mode, process_id, generation, MODE_LEASE_DURATION).await {
                        active.store(false, Ordering::Release);
                        metrics::counter!("memory_execution_mode_lease_lost_total").increment(1);
                        tracing::error!(%error, "Memory execution mode lease lost; processing fenced until restart");
                        return;
                    }
                }
            }
        }
    })
}
