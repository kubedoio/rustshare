//! Independent worker consumer for durable Connector-backed Files events.

use rustshare_core::domain::ApplicationId;
use rustshare_integration_events::{
    event_types::{FILES_FILE_CREATED_V1, FILES_FILE_DELETED_V1, FILES_FILE_UPDATED_V1},
    ConsumerOutcome, IntegrationEvent, OutboxConsumer,
};
use rustshare_memory::files_source::FilesProjectionSnapshot;
use rustshare_storage::{MemoryFilesCatalogRecord, MemoryFilesCatalogStore};
use sqlx::PgPool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use crate::memory_execution_lease::MemoryExecutionFence;
use crate::memory_worker_files_client::MemoryWorkerFilesClient;
use crate::services::note_index_sink::NoteIndexSink;

pub const MEMORY_FILES_PROJECTION_CONSUMER_ID: &str = "io.elembra.memory.files-connector.v1";
const SUBSCRIPTIONS: &[&str] = &[
    FILES_FILE_CREATED_V1,
    FILES_FILE_UPDATED_V1,
    FILES_FILE_DELETED_V1,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotProjectionOutcome {
    Indexed,
    Removed,
    Unchanged,
}

pub struct MemoryFilesWorkerConsumer {
    pool: PgPool,
    catalog: MemoryFilesCatalogStore,
    files: MemoryWorkerFilesClient,
    index_sink: Arc<dyn NoteIndexSink>,
    execution_active: Arc<AtomicBool>,
    execution_fence: Option<MemoryExecutionFence>,
}

impl MemoryFilesWorkerConsumer {
    pub fn new(
        pool: PgPool,
        catalog: MemoryFilesCatalogStore,
        files: MemoryWorkerFilesClient,
        index_sink: Arc<dyn NoteIndexSink>,
    ) -> Self {
        Self {
            pool,
            catalog,
            files,
            index_sink,
            execution_active: Arc::new(AtomicBool::new(true)),
            execution_fence: None,
        }
    }

    pub fn with_execution_guard(mut self, active: Arc<AtomicBool>) -> Self {
        self.execution_active = active;
        self
    }

    pub fn with_execution_fence(mut self, fence: MemoryExecutionFence) -> Self {
        self.execution_fence = Some(fence);
        self
    }

    async fn check_execution_fence(&self) -> anyhow::Result<()> {
        if let Some(fence) = &self.execution_fence {
            fence.check().await?;
        }
        Ok(())
    }

    async fn check_execution_fence_in_transaction(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> anyhow::Result<()> {
        if let Some(fence) = &self.execution_fence {
            fence.check_in_transaction(tx).await?;
        }
        Ok(())
    }

    async fn project(&self, event: &IntegrationEvent) -> anyhow::Result<()> {
        self.check_execution_fence().await?;
        let resource = event
            .resource
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Files event has no resource"))?;
        anyhow::ensure!(
            resource.application == ApplicationId::new("io.elembra.files")
                && resource.resource_type == "file",
            "Files event has unexpected resource owner or type"
        );
        let file_id = Uuid::parse_str(&resource.resource_id)?;
        let tenant_id = event.tenant_id.0;
        let snapshot = self.files.snapshot(&event.source, event.id).await?;
        self.project_snapshot(tenant_id, file_id, snapshot).await?;
        Ok(())
    }

    pub async fn project_snapshot(
        &self,
        tenant_id: Uuid,
        file_id: Uuid,
        snapshot: FilesProjectionSnapshot,
    ) -> anyhow::Result<SnapshotProjectionOutcome> {
        anyhow::ensure!(
            self.execution_active.load(Ordering::Acquire),
            "Memory execution mode lease is inactive"
        );
        self.check_execution_fence().await?;
        match snapshot {
            FilesProjectionSnapshot::Deleted {
                tenant_id: current_tenant,
                file_id: current_file,
            } => {
                verify_resource(tenant_id, file_id, current_tenant, current_file)?;
                if self.catalog.contains(tenant_id, file_id).await? {
                    let mut tx = self.pool.begin().await?;
                    self.check_execution_fence_in_transaction(&mut tx).await?;
                    self.catalog
                        .tombstone_in_tx(&mut tx, tenant_id, file_id)
                        .await?;
                    self.index_sink
                        .remove_source_file(tenant_id, file_id)
                        .await
                        .map_err(|error| anyhow::anyhow!(error))?;
                    tx.commit().await?;
                    return Ok(SnapshotProjectionOutcome::Removed);
                }
                return Ok(SnapshotProjectionOutcome::Unchanged);
            }
            FilesProjectionSnapshot::Ignored {
                tenant_id: current_tenant,
                file_id: current_file,
            } => {
                verify_resource(tenant_id, file_id, current_tenant, current_file)?;
                if self.catalog.contains(tenant_id, file_id).await? {
                    let mut tx = self.pool.begin().await?;
                    self.check_execution_fence_in_transaction(&mut tx).await?;
                    self.catalog
                        .tombstone_in_tx(&mut tx, tenant_id, file_id)
                        .await?;
                    self.index_sink
                        .remove_source_file(tenant_id, file_id)
                        .await
                        .map_err(|error| anyhow::anyhow!(error))?;
                    tx.commit().await?;
                    return Ok(SnapshotProjectionOutcome::Removed);
                }
                return Ok(SnapshotProjectionOutcome::Unchanged);
            }
            FilesProjectionSnapshot::Available(snapshot) => {
                anyhow::ensure!(
                    snapshot.tenant_id == tenant_id && snapshot.file_id == file_id,
                    "Files snapshot does not match event resource"
                );
                let connector = snapshot
                    .connector
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Connector provenance is missing"))?;
                let record = MemoryFilesCatalogRecord {
                    tenant_id,
                    file_id,
                    current_version: snapshot.current_version.clone(),
                    current_version_number: snapshot.current_version_number,
                    connector_instance_id: connector.instance_id,
                    source_resource_ref: connector.resource_ref.clone(),
                    source_revision: connector.revision.clone(),
                    relative_path: connector.relative_path.clone(),
                    observed_modified_at: connector.observed_modified_at,
                };
                let mut tx = self.pool.begin().await?;
                self.check_execution_fence_in_transaction(&mut tx).await?;
                let current = self.catalog.upsert_in_tx(&mut tx, &record).await?;
                if current {
                    self.index_sink
                        .index_note(
                            file_id,
                            snapshot.name,
                            snapshot.path,
                            snapshot.content,
                            snapshot.mime_type,
                            snapshot.owner_id,
                            snapshot.acl,
                        )
                        .await
                        .map_err(|error| anyhow::anyhow!(error))?;
                    tx.commit().await?;
                    return Ok(SnapshotProjectionOutcome::Indexed);
                }
                tx.commit().await?;
                return Ok(SnapshotProjectionOutcome::Unchanged);
            }
        }
    }
}

fn verify_resource(
    event_tenant: Uuid,
    event_file: Uuid,
    snapshot_tenant: Uuid,
    snapshot_file: Uuid,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        event_tenant == snapshot_tenant && event_file == snapshot_file,
        "Files snapshot does not match event resource"
    );
    Ok(())
}

#[async_trait::async_trait]
impl OutboxConsumer for MemoryFilesWorkerConsumer {
    fn consumer_id(&self) -> &str {
        MEMORY_FILES_PROJECTION_CONSUMER_ID
    }

    fn subscriptions(&self) -> Vec<String> {
        SUBSCRIPTIONS
            .iter()
            .map(|event| (*event).to_owned())
            .collect()
    }

    async fn process(&self, event: &IntegrationEvent) -> ConsumerOutcome {
        if !self.execution_active.load(Ordering::Acquire) {
            return ConsumerOutcome::Retryable {
                reason: "Memory execution mode lease is inactive".into(),
            };
        }
        match self.project(event).await {
            Ok(()) => ConsumerOutcome::Processed,
            Err(_) => {
                tracing::warn!(event_id = %event.id, "Memory Files projection failed; event will retry");
                ConsumerOutcome::Retryable {
                    reason: "Memory Files projection failed".into(),
                }
            }
        }
    }
}
