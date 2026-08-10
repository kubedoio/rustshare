//! Memory-owned catalog store (`memory_catalog`): the consumer-side durable
//! projection of observed Buzz chat events, plus the admin reconciliation
//! repair path.

use anyhow::Result;
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_integration_events::IntegrationEvent;
use rustshare_memory::event::{ChatChannelKind, ObservedChatEventData, ObservedEventType};
use rustshare_memory::policy::ProjectionPolicy;
use rustshare_memory::project::{apply_event, apply_tombstone, project_record};
use rustshare_memory::record::{IndexingStatus, MemoryCatalogRecord};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Results of an [`MemoryCatalogStore::upsert_records`] reconciliation run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReconcileCounts {
    /// Records examined.
    pub processed: u64,
    /// Rows newly inserted.
    pub created: u64,
    /// Existing rows updated.
    pub updated: u64,
}

/// The store's caller has already gated `content` by tenant policy (None
/// unless `content_indexing` is on and a body exists); the pure projection
/// functions still take a policy object, so use a fully-enabled one. The
/// never-eligible channel check (`ProjectionPolicy::decision`) still applies:
/// `dm`/`private`/`excluded` events are consumed but produce no record.
const FULL_POLICY: ProjectionPolicy = ProjectionPolicy {
    memory_projection: true,
    content_indexing: true,
};

const RECORD_COLUMNS: &str = "record_id, tenant_id, workspace_id, source_application, \
    source_type, source_ref, message_id, latest_event_id, event_type, community_id, \
    channel_id, channel_kind, author_pubkey, author_principal_id, occurred_at, observed_at, \
    checksum, signature, signature_verified, provenance, classification, \
    retention_policy_ref, legal_hold_ref, authorization_source, authorization_ref, \
    content_indexing, content, indexing_status, tombstoned_at, created_at, updated_at";

#[derive(Clone)]
pub struct MemoryCatalogStore {
    pool: PgPool,
}

impl MemoryCatalogStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Consumer-side durable projection: in ONE tx — (1) idempotency receipt
    /// into `integration_consumer_receipts` (`ON CONFLICT
    /// (consumer_id, source, event_id) DO NOTHING`; `rows_affected() == 1`
    /// gates the effect), (2) load the existing record by
    /// `(tenant_id, message_id)`, (3) insert via `project_record` if none,
    /// else `apply_tombstone` for Deleted events or `apply_event` otherwise
    /// (pure fns from `rustshare-memory`), (4) persist. Returns the persisted
    /// record, or `None` when the event was (a) a duplicate delivery (receipt
    /// already present), (b) not projected (never-eligible channel), or
    /// (c) a Deleted event with no existing catalog record — a tombstone for
    /// a message that was never projected is a no-op; the deletion already
    /// lives in the observation index. `content` must already be gated by
    /// policy by the caller (None unless `content_indexing` on and body
    /// exists).
    pub async fn upsert_from_event_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        consumer_id: &str,
        event: &IntegrationEvent,
        data: &ObservedChatEventData,
        content: Option<String>,
    ) -> Result<Option<MemoryCatalogRecord>> {
        let receipt = sqlx::query(
            r#"
            INSERT INTO integration_consumer_receipts
                (consumer_id, source, event_id, event_type, tenant_id, workspace_id, processed_at)
            VALUES ($1, $2, $3, $4, $5, $6, now())
            ON CONFLICT (consumer_id, source, event_id) DO NOTHING
            "#,
        )
        .bind(consumer_id)
        .bind(&event.source)
        .bind(event.id)
        .bind(&event.r#type)
        .bind(event.tenant_id.0)
        .bind(event.workspace_id.0)
        .execute(&mut **tx)
        .await?;
        if receipt.rows_affected() != 1 {
            // Duplicate delivery: the effect was already applied on the first
            // processing of this (consumer, source, event_id).
            return Ok(None);
        }

        let row = sqlx::query(&format!(
            "SELECT {RECORD_COLUMNS} FROM memory_catalog
             WHERE tenant_id = $1 AND source_application = 'io.elembra.chat'
               AND source_type = 'message' AND message_id = $2"
        ))
        .bind(event.tenant_id.0)
        .bind(&data.buzz.message_id)
        .fetch_optional(&mut **tx)
        .await?;
        let existing = row.map(|row| row_to_record(&row)).transpose()?;

        let persisted = match existing {
            Some(existing) => {
                let updated = if data.buzz.event_type == ObservedEventType::Deleted {
                    apply_tombstone(&existing, data)
                } else {
                    apply_event(&existing, data, content)
                };
                update_in_tx(tx, &updated).await?;
                updated
            }
            None => {
                if data.buzz.event_type == ObservedEventType::Deleted {
                    // A tombstone for a message that was never projected is a
                    // no-op: the durable fact of the deletion already lives in
                    // the observation index. The receipt above records that
                    // the event was processed; its effect is "nothing".
                    return Ok(None);
                }
                let Some(record) = project_record(
                    event.tenant_id,
                    event.workspace_id,
                    data,
                    &FULL_POLICY,
                    content,
                ) else {
                    // Never-eligible channel: consumed, but not projected.
                    return Ok(None);
                };
                insert_in_tx(tx, &record).await?;
                record
            }
        };
        Ok(Some(persisted))
    }

    /// Reconciliation repair path (admin): upsert a set of records keyed by
    /// the unique `(tenant_id, source_application, source_type, message_id)` —
    /// insert new, update existing. Does NOT touch receipts. Returns counts.
    pub async fn upsert_records(&self, records: &[MemoryCatalogRecord]) -> Result<ReconcileCounts> {
        let mut counts = ReconcileCounts {
            processed: records.len() as u64,
            ..ReconcileCounts::default()
        };
        for record in records {
            let row = sqlx::query(&format!(
                "INSERT INTO memory_catalog ({RECORD_COLUMNS}) VALUES \
                 ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, \
                  $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, \
                  $27, $28, $29, $30, $31) \
                 ON CONFLICT (tenant_id, source_application, source_type, message_id) \
                 DO UPDATE SET \
                     workspace_id = EXCLUDED.workspace_id, \
                     source_ref = EXCLUDED.source_ref, \
                     latest_event_id = EXCLUDED.latest_event_id, \
                     event_type = EXCLUDED.event_type, \
                     community_id = EXCLUDED.community_id, \
                     channel_id = EXCLUDED.channel_id, \
                     channel_kind = EXCLUDED.channel_kind, \
                     author_pubkey = EXCLUDED.author_pubkey, \
                     author_principal_id = EXCLUDED.author_principal_id, \
                     occurred_at = EXCLUDED.occurred_at, \
                     observed_at = EXCLUDED.observed_at, \
                     checksum = EXCLUDED.checksum, \
                     signature = EXCLUDED.signature, \
                     signature_verified = EXCLUDED.signature_verified, \
                     provenance = EXCLUDED.provenance, \
                     classification = EXCLUDED.classification, \
                     retention_policy_ref = EXCLUDED.retention_policy_ref, \
                     legal_hold_ref = EXCLUDED.legal_hold_ref, \
                     authorization_source = EXCLUDED.authorization_source, \
                     authorization_ref = EXCLUDED.authorization_ref, \
                     content_indexing = EXCLUDED.content_indexing, \
                     content = EXCLUDED.content, \
                     indexing_status = EXCLUDED.indexing_status, \
                     tombstoned_at = EXCLUDED.tombstoned_at, \
                     updated_at = EXCLUDED.updated_at \
                 RETURNING (xmax = 0) AS inserted"
            ))
            .bind(record.record_id)
            .bind(record.tenant_id.0)
            .bind(record.workspace_id.0)
            .bind(&record.source_application)
            .bind(&record.source_type)
            .bind(&record.source_ref)
            .bind(&record.message_id)
            .bind(&record.latest_event_id)
            .bind(event_type_str(record.event_type))
            .bind(&record.community_id)
            .bind(&record.channel_id)
            .bind(channel_kind_str(record.channel_kind))
            .bind(&record.author_pubkey)
            .bind(record.author_principal_id.map(|p| p.0))
            .bind(record.occurred_at)
            .bind(record.observed_at)
            .bind(&record.checksum)
            .bind(&record.signature)
            .bind(record.signature_verified)
            .bind(serde_json::to_value(&record.provenance)?)
            .bind(&record.classification)
            .bind(&record.retention_policy_ref)
            .bind(&record.legal_hold_ref)
            .bind(&record.authorization_source)
            .bind(&record.authorization_ref)
            .bind(record.content_indexing)
            .bind(&record.content)
            .bind(indexing_status_str(record.indexing_status))
            .bind(record.tombstoned_at)
            .bind(record.created_at)
            .bind(record.updated_at)
            .fetch_one(&self.pool)
            .await?;
            if row.try_get::<bool, _>("inserted")? {
                counts.created += 1;
            } else {
                counts.updated += 1;
            }
        }
        Ok(counts)
    }

    /// The catalog record for `message_id` in `tenant_id`, if any.
    pub async fn get(
        &self,
        tenant_id: TenantId,
        message_id: &str,
    ) -> Result<Option<MemoryCatalogRecord>> {
        let row = sqlx::query(&format!(
            "SELECT {RECORD_COLUMNS} FROM memory_catalog
             WHERE tenant_id = $1 AND source_application = 'io.elembra.chat'
               AND source_type = 'message' AND message_id = $2"
        ))
        .bind(tenant_id.0)
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| row_to_record(&row)).transpose()
    }

    /// Number of catalog records for `tenant_id`.
    pub async fn count_for_tenant(&self, tenant_id: TenantId) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT count(*)::bigint FROM memory_catalog WHERE tenant_id = $1")
                .bind(tenant_id.0)
                .fetch_one(&self.pool)
                .await?,
        )
    }
}

/// Persist a record that is known not to exist yet (caller already loaded and
/// found none). A concurrent duplicate surfaces as a unique-violation error
/// rather than being silently dropped.
async fn insert_in_tx(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    record: &MemoryCatalogRecord,
) -> Result<()> {
    sqlx::query(&format!(
        "INSERT INTO memory_catalog ({RECORD_COLUMNS}) VALUES \
         ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, \
          $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, \
          $27, $28, $29, $30, $31)"
    ))
    .bind(record.record_id)
    .bind(record.tenant_id.0)
    .bind(record.workspace_id.0)
    .bind(&record.source_application)
    .bind(&record.source_type)
    .bind(&record.source_ref)
    .bind(&record.message_id)
    .bind(&record.latest_event_id)
    .bind(event_type_str(record.event_type))
    .bind(&record.community_id)
    .bind(&record.channel_id)
    .bind(channel_kind_str(record.channel_kind))
    .bind(&record.author_pubkey)
    .bind(record.author_principal_id.map(|p| p.0))
    .bind(record.occurred_at)
    .bind(record.observed_at)
    .bind(&record.checksum)
    .bind(&record.signature)
    .bind(record.signature_verified)
    .bind(serde_json::to_value(&record.provenance)?)
    .bind(&record.classification)
    .bind(&record.retention_policy_ref)
    .bind(&record.legal_hold_ref)
    .bind(&record.authorization_source)
    .bind(&record.authorization_ref)
    .bind(record.content_indexing)
    .bind(&record.content)
    .bind(indexing_status_str(record.indexing_status))
    .bind(record.tombstoned_at)
    .bind(record.created_at)
    .bind(record.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Update every mutable column of an existing record (identity fields
/// `record_id`/`created_at` are preserved).
async fn update_in_tx(
    tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
    record: &MemoryCatalogRecord,
) -> Result<()> {
    sqlx::query(
        "UPDATE memory_catalog SET \
             workspace_id = $2, \
             source_ref = $3, \
             latest_event_id = $4, \
             event_type = $5, \
             community_id = $6, \
             channel_id = $7, \
             channel_kind = $8, \
             author_pubkey = $9, \
             author_principal_id = $10, \
             occurred_at = $11, \
             observed_at = $12, \
             checksum = $13, \
             signature = $14, \
             signature_verified = $15, \
             provenance = $16, \
             classification = $17, \
             retention_policy_ref = $18, \
             legal_hold_ref = $19, \
             authorization_source = $20, \
             authorization_ref = $21, \
             content_indexing = $22, \
             content = $23, \
             indexing_status = $24, \
             tombstoned_at = $25, \
             updated_at = $26 \
         WHERE tenant_id = $1 AND source_application = 'io.elembra.chat' \
           AND source_type = 'message' AND message_id = $27",
    )
    .bind(record.tenant_id.0)
    .bind(record.workspace_id.0)
    .bind(&record.source_ref)
    .bind(&record.latest_event_id)
    .bind(event_type_str(record.event_type))
    .bind(&record.community_id)
    .bind(&record.channel_id)
    .bind(channel_kind_str(record.channel_kind))
    .bind(&record.author_pubkey)
    .bind(record.author_principal_id.map(|p| p.0))
    .bind(record.occurred_at)
    .bind(record.observed_at)
    .bind(&record.checksum)
    .bind(&record.signature)
    .bind(record.signature_verified)
    .bind(serde_json::to_value(&record.provenance)?)
    .bind(&record.classification)
    .bind(&record.retention_policy_ref)
    .bind(&record.legal_hold_ref)
    .bind(&record.authorization_source)
    .bind(&record.authorization_ref)
    .bind(record.content_indexing)
    .bind(&record.content)
    .bind(indexing_status_str(record.indexing_status))
    .bind(record.tombstoned_at)
    .bind(record.updated_at)
    .bind(&record.message_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn row_to_record(row: &sqlx::postgres::PgRow) -> Result<MemoryCatalogRecord> {
    Ok(MemoryCatalogRecord {
        record_id: row.try_get("record_id")?,
        tenant_id: TenantId(row.try_get("tenant_id")?),
        workspace_id: WorkspaceId(row.try_get("workspace_id")?),
        source_application: row.try_get("source_application")?,
        source_type: row.try_get("source_type")?,
        source_ref: row.try_get("source_ref")?,
        message_id: row.try_get("message_id")?,
        latest_event_id: row.try_get("latest_event_id")?,
        event_type: parse_event_type(row.try_get("event_type")?)?,
        community_id: row.try_get("community_id")?,
        channel_id: row.try_get("channel_id")?,
        channel_kind: parse_channel_kind(row.try_get("channel_kind")?)?,
        author_pubkey: row.try_get("author_pubkey")?,
        author_principal_id: row
            .try_get::<Option<Uuid>, _>("author_principal_id")?
            .map(PrincipalId),
        occurred_at: row.try_get("occurred_at")?,
        observed_at: row.try_get("observed_at")?,
        checksum: row.try_get("checksum")?,
        signature: row.try_get("signature")?,
        signature_verified: row.try_get("signature_verified")?,
        provenance: serde_json::from_value(row.try_get("provenance")?)?,
        classification: row.try_get("classification")?,
        retention_policy_ref: row.try_get("retention_policy_ref")?,
        legal_hold_ref: row.try_get("legal_hold_ref")?,
        authorization_source: row.try_get("authorization_source")?,
        authorization_ref: row.try_get("authorization_ref")?,
        content_indexing: row.try_get("content_indexing")?,
        content: row.try_get("content")?,
        indexing_status: parse_indexing_status(row.try_get("indexing_status")?)?,
        tombstoned_at: row.try_get("tombstoned_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn event_type_str(event_type: ObservedEventType) -> &'static str {
    match event_type {
        ObservedEventType::Created => "created",
        ObservedEventType::Edited => "edited",
        ObservedEventType::Deleted => "deleted",
    }
}

fn parse_event_type(value: String) -> Result<ObservedEventType> {
    Ok(match value.as_str() {
        "created" => ObservedEventType::Created,
        "edited" => ObservedEventType::Edited,
        "deleted" => ObservedEventType::Deleted,
        other => anyhow::bail!("unknown memory_catalog.event_type `{other}`"),
    })
}

fn channel_kind_str(channel_kind: ChatChannelKind) -> &'static str {
    match channel_kind {
        ChatChannelKind::Workspace => "workspace",
        ChatChannelKind::Dm => "dm",
        ChatChannelKind::Private => "private",
        ChatChannelKind::Excluded => "excluded",
    }
}

fn parse_channel_kind(value: String) -> Result<ChatChannelKind> {
    Ok(match value.as_str() {
        "workspace" => ChatChannelKind::Workspace,
        "dm" => ChatChannelKind::Dm,
        "private" => ChatChannelKind::Private,
        "excluded" => ChatChannelKind::Excluded,
        other => anyhow::bail!("unknown memory_catalog.channel_kind `{other}`"),
    })
}

fn indexing_status_str(status: IndexingStatus) -> &'static str {
    match status {
        IndexingStatus::ReferenceOnly => "reference_only",
        IndexingStatus::ContentStored => "content_stored",
        IndexingStatus::Tombstoned => "tombstoned",
    }
}

fn parse_indexing_status(value: String) -> Result<IndexingStatus> {
    Ok(match value.as_str() {
        "reference_only" => IndexingStatus::ReferenceOnly,
        "content_stored" => IndexingStatus::ContentStored,
        "tombstoned" => IndexingStatus::Tombstoned,
        other => anyhow::bail!("unknown memory_catalog.indexing_status `{other}`"),
    })
}
