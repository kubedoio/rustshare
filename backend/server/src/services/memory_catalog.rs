//! Permission-aware view of the Memory records that currently have durable
//! catalog entries. Files/Notes and live Mail remain searchable without being
//! represented by this catalog endpoint.

use chrono::{DateTime, Utc};
use rustshare_core::domain::{ApplicationId, PrincipalId, TenantId, WorkspaceId};
use rustshare_resource_auth::{PrincipalContext, Purpose, ResourceRef, SourceAuthorizer};
use rustshare_storage::{MemoryCatalogListRecord, MemoryCatalogStore, MemoryFilesCatalogStore};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MemoryCatalogEntry {
    pub source_application: String,
    pub source_type: String,
    pub resource_ref: String,
    pub title: String,
    pub indexing_status: String,
    pub occurred_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub source_version: Option<String>,
    pub provenance: serde_json::Value,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MemoryCatalogCoverage {
    /// These source record kinds are currently materialized in the catalog.
    pub cataloged: [&'static str; 2],
    /// These sources can be searched, but this endpoint does not track them.
    pub not_tracked: [&'static str; 2],
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MemoryCatalogResponse {
    pub coverage: MemoryCatalogCoverage,
    pub entries: Vec<MemoryCatalogEntry>,
}

/// List recent catalog records only after their current source owners confirm
/// that the requesting principal can still see each source.
pub async fn list_for_principal(
    chat_catalog: &MemoryCatalogStore,
    files_catalog: &MemoryFilesCatalogStore,
    authorizer: &SourceAuthorizer,
    principal_id: Uuid,
    tenant_id: Uuid,
) -> anyhow::Result<MemoryCatalogResponse> {
    let context = PrincipalContext::user(
        PrincipalId(principal_id),
        TenantId(tenant_id),
        WorkspaceId(tenant_id),
    );
    let chat_records = chat_catalog
        .list_live_for_tenant(TenantId(tenant_id), 100)
        .await?;
    let file_records = files_catalog.list_connector_records(tenant_id, 100).await?;
    let mut entries = Vec::new();

    for record in chat_records {
        if let Some(entry) = authorized_chat_entry(authorizer, &context, record).await {
            entries.push(entry);
        }
    }
    for record in file_records {
        let resource = ResourceRef::new(
            ApplicationId::new("io.elembra.files"),
            "file",
            record.file_id.to_string(),
        )
        .with_version(record.current_version.clone());
        let Ok(resolved) = authorizer
            .resolve(&context, &resource, Purpose::SearchPreview)
            .await
        else {
            continue;
        };
        if !resolved.available {
            continue;
        }
        entries.push(MemoryCatalogEntry {
            source_application: "io.elembra.files".into(),
            source_type: "connector_publication".into(),
            resource_ref: resolved.resource.to_uri(),
            title: resolved.display_name,
            indexing_status: if record.indexed { "indexed" } else { "pending" }.into(),
            occurred_at: None,
            updated_at: Some(record.updated_at),
            source_version: Some(record.current_version),
            provenance: serde_json::json!({
                "connector_instance_id": record.connector_instance_id,
                "source_resource_ref": record.source_resource_ref,
                "source_revision": record.source_revision,
                "relative_path": record.relative_path
            }),
        });
    }

    entries.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.resource_ref.cmp(&b.resource_ref))
    });
    entries.truncate(100);

    Ok(MemoryCatalogResponse {
        coverage: MemoryCatalogCoverage {
            cataloged: ["chat_messages", "connector_published_files"],
            not_tracked: ["ordinary_files_and_notes", "live_mail_messages"],
        },
        entries,
    })
}

async fn authorized_chat_entry(
    authorizer: &SourceAuthorizer,
    context: &PrincipalContext,
    record: MemoryCatalogListRecord,
) -> Option<MemoryCatalogEntry> {
    let resource = ResourceRef::from_uri(&record.source_ref).ok()?;
    let resolved = authorizer
        .resolve(context, &resource, Purpose::SearchPreview)
        .await
        .ok()?;
    if !resolved.available {
        return None;
    }
    Some(MemoryCatalogEntry {
        source_application: record.source_application,
        source_type: record.source_type,
        resource_ref: resolved.resource.to_uri(),
        title: resolved.display_name,
        indexing_status: record.indexing_status,
        occurred_at: Some(record.occurred_at),
        updated_at: Some(record.updated_at),
        source_version: None,
        provenance: serde_json::json!({
            "message_id": record.message_id,
            "latest_event_id": record.latest_event_id,
            "community_id": record.community_id,
            "channel_id": record.channel_id,
            "channel_kind": record.channel_kind,
            "author_pubkey": record.author_pubkey,
            "provenance": record.provenance
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_coverage_names_untracked_sources_explicitly() {
        let coverage = MemoryCatalogCoverage {
            cataloged: ["chat_messages", "connector_published_files"],
            not_tracked: ["ordinary_files_and_notes", "live_mail_messages"],
        };
        let json = serde_json::to_value(coverage).unwrap();
        assert_eq!(json["cataloged"][0], "chat_messages");
        assert_eq!(json["cataloged"][1], "connector_published_files");
        assert_eq!(json["not_tracked"][0], "ordinary_files_and_notes");
        assert_eq!(json["not_tracked"][1], "live_mail_messages");
    }
}
