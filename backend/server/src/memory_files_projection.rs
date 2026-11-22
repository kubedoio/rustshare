//! Memory-owned projection of Connector provenance and Files keyword content.

use chrono::{DateTime, Utc};
use rustshare_core::domain::{ApplicationId, PrincipalId, TenantId, WorkspaceId};
use rustshare_integration_events::event_types::{
    FILES_FILE_CREATED_V1, FILES_FILE_DELETED_V1, FILES_FILE_UPDATED_V1,
};
use rustshare_integration_events::{
    event_matches_subscription, redact::redact_error, ActorRef, ConsumerOutcome, IntegrationEvent,
    OutboxConsumer,
};
use rustshare_resource_auth::{PrincipalContext, Representation, ResourceRef, SourceAuthorizer};
use rustshare_storage::{MemoryFilesCatalogRecord, MemoryFilesCatalogStore, MetadataStore};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

use crate::services::note_service::NoteService;

pub const MEMORY_FILES_PROJECTION_CONSUMER_ID: &str = "io.elembra.memory.files-connector.v1";

const SUBSCRIPTIONS: &[&str] = &[
    FILES_FILE_CREATED_V1,
    FILES_FILE_UPDATED_V1,
    FILES_FILE_DELETED_V1,
];

#[derive(serde::Deserialize)]
struct ConnectorProvenance {
    connector_instance_id: Uuid,
    source_resource_ref: String,
    source_revision: String,
    relative_path: String,
    #[serde(default)]
    observed_modified_at: Option<DateTime<Utc>>,
}

pub struct MemoryFilesProjectionConsumer {
    pool: PgPool,
    metadata: Arc<MetadataStore>,
    authorizer: Arc<SourceAuthorizer>,
    notes: Arc<NoteService>,
    catalog: MemoryFilesCatalogStore,
}

impl MemoryFilesProjectionConsumer {
    pub fn new(
        pool: PgPool,
        metadata: Arc<MetadataStore>,
        authorizer: Arc<SourceAuthorizer>,
        notes: Arc<NoteService>,
        catalog: MemoryFilesCatalogStore,
    ) -> Self {
        Self {
            pool,
            metadata,
            authorizer,
            notes,
            catalog,
        }
    }

    async fn project(&self, event: &IntegrationEvent) -> anyhow::Result<()> {
        let resource = event
            .resource
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Files event has no resource reference"))?;
        anyhow::ensure!(
            resource.application == ApplicationId::new("io.elembra.files")
                && resource.resource_type == "file",
            "Files event resource has unexpected owner or type"
        );
        let file_id = Uuid::parse_str(&resource.resource_id)?;
        let tenant_id = event.tenant_id.0;

        if event.r#type == FILES_FILE_DELETED_V1 {
            if self
                .metadata
                .find_file_by_id_for_tenant(file_id, tenant_id)
                .await?
                .is_some()
            {
                // A delayed delete must not hide a File that has since been restored.
                return Ok(());
            }
            let mut tx = self.pool.begin().await?;
            self.catalog
                .tombstone_in_tx(&mut tx, tenant_id, file_id)
                .await?;
            self.notes
                .remove_connector_markdown_index(tenant_id, file_id)
                .await?;
            tx.commit().await?;
            return Ok(());
        }

        // Ordinary Files uploads do not belong to this projection.
        let Some(provenance) = event.data.get("sourceProvenance") else {
            return Ok(());
        };
        let provenance: ConnectorProvenance = serde_json::from_value(provenance.clone())?;
        anyhow::ensure!(
            !provenance.source_resource_ref.is_empty()
                && !provenance.source_revision.is_empty()
                && !provenance.relative_path.is_empty(),
            "Connector provenance fields must not be empty"
        );
        ResourceRef::from_uri(&provenance.source_resource_ref)?;

        let Some(file) = self
            .metadata
            .find_file_by_id_for_tenant(file_id, tenant_id)
            .await?
        else {
            // A delete may overtake a queued create/update. Fence it so the
            // older event cannot recreate searchable content.
            let mut tx = self.pool.begin().await?;
            self.catalog
                .tombstone_in_tx(&mut tx, tenant_id, file_id)
                .await?;
            self.notes
                .remove_connector_markdown_index(tenant_id, file_id)
                .await?;
            tx.commit().await?;
            return Ok(());
        };

        let Some(ActorRef::Principal(actor)) = event.actor.as_ref() else {
            anyhow::bail!("Connector Files event has no user Principal actor");
        };
        anyhow::ensure!(
            actor.0 == file.owner_id,
            "Connector actor is not the Files owner"
        );

        let version = format!("sha256:{}", file.content_hash);
        // Connector import revisions are content hashes. If this is an old
        // out-of-order event, its newer Files event remains responsible for
        // projection; never combine current bytes with stale provenance.
        if provenance.source_revision != version {
            return Ok(());
        }
        let file_ref = ResourceRef::new(
            ApplicationId::new("io.elembra.files"),
            "file",
            file.id.to_string(),
        )
        .with_version(version.clone());
        let context = PrincipalContext::user(
            PrincipalId(file.owner_id),
            TenantId(tenant_id),
            WorkspaceId(tenant_id),
        );
        tracing::info!(
            tenant_id = %tenant_id,
            file_id = %file_id,
            event_id = %event.id,
            actor_id = %actor.0,
            file_version = %version,
            source_revision = %provenance.source_revision,
            "Memory requesting Connector source through Files authorization"
        );
        let fetched = self
            .authorizer
            .fetch(&context, &file_ref, Representation::Text)
            .await;
        self.catalog
            .record_source_access(
                event.id,
                tenant_id,
                file_id,
                file.owner_id,
                &version,
                fetched.is_ok(),
            )
            .await?;
        let fetched = fetched?;
        let content = String::from_utf8(fetched.data.to_vec())?;

        let record = MemoryFilesCatalogRecord {
            tenant_id,
            file_id,
            current_version: version,
            current_version_number: i64::from(file.current_version),
            connector_instance_id: provenance.connector_instance_id,
            source_resource_ref: provenance.source_resource_ref,
            source_revision: provenance.source_revision,
            relative_path: provenance.relative_path,
            observed_modified_at: provenance.observed_modified_at,
        };
        let mut tx = self.pool.begin().await?;
        if !self.catalog.upsert_in_tx(&mut tx, &record).await? {
            tx.commit().await?;
            return Ok(());
        }
        self.notes
            .index_connector_markdown(file_id, file.owner_id, tenant_id, &content)
            .await?;
        tx.commit().await?;

        // This structured audit record contains resource identity and version
        // only; source content and connector credentials never enter logs.
        tracing::info!(
            tenant_id = %tenant_id,
            file_id = %file_id,
            event_id = %event.id,
            file_version = %record.current_version,
            connector_instance_id = %record.connector_instance_id,
            source_resource_ref = %record.source_resource_ref,
            source_revision = %record.source_revision,
            "Memory fetched a Connector source through current Files authorization"
        );
        Ok(())
    }
}

#[async_trait::async_trait]
impl OutboxConsumer for MemoryFilesProjectionConsumer {
    fn consumer_id(&self) -> &str {
        MEMORY_FILES_PROJECTION_CONSUMER_ID
    }

    fn subscriptions(&self) -> Vec<String> {
        SUBSCRIPTIONS
            .iter()
            .map(|event| (*event).to_string())
            .collect()
    }

    async fn process(&self, event: &IntegrationEvent) -> ConsumerOutcome {
        if let Err(error) = event.validate() {
            return ConsumerOutcome::Permanent {
                reason: redact_error(&error.to_string(), 512),
            };
        }
        if !event_matches_subscription(&event.r#type, &self.subscriptions()) {
            return ConsumerOutcome::Permanent {
                reason: "event type is outside Memory Files subscriptions".to_string(),
            };
        }
        match self.project(event).await {
            Ok(()) => ConsumerOutcome::Processed,
            Err(error) => ConsumerOutcome::Retryable {
                reason: redact_error(&error.to_string(), 512),
            },
        }
    }
}
