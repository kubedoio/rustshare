//! Memory-owned catalog store (`memory_catalog`): the consumer-side durable
//! projection of observed Buzz chat events, plus the admin reconciliation
//! repair path.

use crate::chat_observation::ChatObservationStore;
use anyhow::Result;
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_core::validation::escape_ilike;
use rustshare_integration_events::IntegrationEvent;
use rustshare_memory::event::{
    integration_event_id_for, ChatChannelKind, ObservedChatEventData, ObservedEventType,
};
use rustshare_memory::policy::{ProjectionDecision, ProjectionPolicy};
use rustshare_memory::project::{apply_event, apply_tombstone, project_record};
use rustshare_memory::record::{IndexingStatus, MemoryCatalogRecord};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Results of an [`MemoryCatalogStore::upsert_records`] reconciliation run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReconcileCounts {
    /// Records upserted by `upsert_records`; the reconcile orchestrator
    /// (`reconcile_chat_memory_for_tenant`) overrides it to observation rows
    /// examined.
    pub processed: u64,
    /// Rows newly inserted.
    pub created: u64,
    /// Existing rows updated.
    pub updated: u64,
}

const RECORD_COLUMNS: &str = "record_id, tenant_id, workspace_id, source_application, \
    source_type, source_ref, message_id, latest_event_id, event_type, community_id, \
    channel_id, channel_kind, author_pubkey, author_principal_id, occurred_at, observed_at, \
    checksum, signature, signature_verified, provenance, classification, \
    retention_policy_ref, legal_hold_ref, authorization_source, authorization_ref, \
    content_indexing, content, indexing_status, tombstoned_at, created_at, updated_at";

/// Common English words that carry no content signal for the Ask-grounding
/// term match. Small inline list (no dependency); a dropped token can still
/// match via the whole-phrase condition.
const SEARCH_STOPWORDS: &[&str] = &[
    "the", "and", "for", "what", "did", "does", "do", "is", "are", "was", "were", "how", "why",
    "who", "whom", "when", "where", "which", "that", "this", "these", "those", "with", "from",
    "about", "into", "upon", "have", "has", "had", "not", "but", "you", "your", "can", "could",
    "would", "should", "will", "there", "their", "them", "they", "then", "than", "him", "her",
    "its", "our", "out",
];

/// The terms matched against message content by [`MemoryCatalogStore::search`]
/// and the server-side chat candidate scorer: the whole trimmed query first
/// (so exact-phrase questions still hit), then each whitespace token that is
/// significant — at least 3 chars after stripping surrounding punctuation and
/// not a [`SEARCH_STOPWORDS`] stopword. Terms are deduplicated (the phrase
/// often duplicates its own tokens) and capped at [`MAX_SEARCH_TERMS`] distinct
/// terms; dropped terms are logged at debug level. An empty/whitespace `query`
/// yields no terms.
pub const MAX_SEARCH_TERMS: usize = 64;

pub fn content_match_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let phrase = query.trim();
    if !phrase.is_empty() {
        terms.push(phrase.to_string());
    }
    for token in query.split_whitespace() {
        if terms.len() >= MAX_SEARCH_TERMS {
            tracing::debug!(%query, "search term cap reached; dropping remaining terms");
            break;
        }
        let token = token.trim_matches(|c: char| c.is_ascii_punctuation());
        if token.chars().count() < 3 {
            continue;
        }
        let lower = token.to_lowercase();
        if SEARCH_STOPWORDS.contains(&lower.as_str()) {
            continue;
        }
        let term = token.to_string();
        if terms.contains(&term) {
            continue;
        }
        terms.push(term);
    }
    terms
}

#[derive(Clone)]
pub struct MemoryCatalogStore {
    pool: PgPool,
    /// Optional bridge observation index. `upsert_from_event_in_tx` consults
    /// it in the no-existing-record branch to enforce the
    /// tombstone-before-create delivery guard (a Deleted observation
    /// at-or-after the event whose delete envelope was already consumed means
    /// the message is deleted and must never be projected). `new` leaves it
    /// unset (legacy construction: reconcile-only and AppState-only users,
    /// which never call `upsert_from_event_in_tx`); consumers that project
    /// MUST use [`MemoryCatalogStore::with_observation_store`], and
    /// `upsert_from_event_in_tx` fails closed (error → retryable) when the
    /// index is absent rather than silently skipping the guard.
    chat_observation: Option<ChatObservationStore>,
}

impl MemoryCatalogStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            chat_observation: None,
        }
    }

    /// Construct the store with the bridge observation index so
    /// [`MemoryCatalogStore::upsert_from_event_in_tx`] can enforce the
    /// tombstone-before-create delivery guard. The consumer
    /// (`MemoryChatProjectionConsumer`) and bootstrap wiring use this; stores
    /// built with [`MemoryCatalogStore::new`] are for reconcile/AppState-only
    /// callers and fail closed if they ever reach `upsert_from_event_in_tx`.
    pub fn with_observation_store(pool: PgPool, chat_observation: ChatObservationStore) -> Self {
        Self {
            pool,
            chat_observation: Some(chat_observation),
        }
    }

    /// Consumer-side durable projection: in ONE tx — (1) idempotency receipt
    /// into `integration_consumer_receipts` (`ON CONFLICT
    /// (consumer_id, source, event_id) DO NOTHING`; `rows_affected() == 1`
    /// gates the effect), (2) per-tenant policy gate
    /// (`policy.decision(channel_kind)`; a skip keeps the receipt — the event
    /// is durably processed with no effect), (3) load the existing record by
    /// `(tenant_id, message_id)`, (4) insert via `project_record` if none,
    /// else `apply_tombstone` for Deleted events or `apply_event` otherwise
    /// (pure fns from `rustshare-memory`), (5) persist. Returns the persisted
    /// record, or `None` when the event was (a) a duplicate delivery (receipt
    /// already present), (b) skipped by policy (projection disabled, or a
    /// never-eligible `dm`/`private`/`excluded` channel), (c) a Deleted
    /// event with no existing catalog record — a tombstone for a message that
    /// was never projected is a no-op; the deletion already lives in the
    /// observation index, or (d) a Deleted observation exists at-or-after this
    /// event for the message AND its delete envelope was already consumed
    /// (tombstone-before-create delivery). `content`
    /// must already be gated by policy by the caller (None unless
    /// `content_indexing` on and body exists).
    pub async fn upsert_from_event_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        consumer_id: &str,
        event: &IntegrationEvent,
        data: &ObservedChatEventData,
        policy: &ProjectionPolicy,
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

        // Per-tenant policy gate, centralized here so the consumer does not
        // need a separate receipt-only path. A skip keeps the receipt: the
        // event is durably consumed, its effect is "nothing" (the tenant is
        // not opted in, or the channel is never eligible for projection).
        if matches!(
            policy.decision(data.context.channel_kind),
            ProjectionDecision::Skip(_)
        ) {
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
                // Tombstone-before-create delivery guard: if the create's
                // first processing transiently failed (Retryable/backoff) and
                // the delete was CONSUMED first (a no-op above), the later
                // create retry would otherwise build a LIVE record for a
                // deleted message. Consult the observation index BEFORE
                // projecting: when a Deleted observation exists at-or-after
                // this event's time AND that delete envelope was already
                // consumed (its durable receipt exists), the message is
                // deleted — never project it (this receipt stays). The
                // consumed-receipt condition keeps the delivery-order
                // distinction: a create processed BEFORE its delete still
                // projects (the delete then tombstones the record — preserving
                // the tombstoned-provenance path); only a create retry that
                // arrives AFTER the delete was processed is suppressed. DB
                // errors propagate (the caller retries; a rollback undoes the
                // receipt). A store without the observation index cannot
                // enforce this guard and fails closed rather than silently
                // projecting.
                let observations = self.chat_observation.as_ref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "MemoryCatalogStore has no ChatObservationStore; \
                         cannot enforce the tombstone-before-create guard"
                    )
                })?;
                if let Some(latest) = observations
                    .lookup_for_auth(event.tenant_id, &data.buzz.message_id)
                    .await?
                {
                    if latest.event_type == ObservedEventType::Deleted
                        && latest.event_created_at >= data.buzz.created_at
                    {
                        // The durable event id for the delete is the
                        // deterministic UUIDv5 of its Buzz event id (see
                        // `build_envelope`); its receipt proves the delete was
                        // consumed before this create retry.
                        let deleted_event_id = integration_event_id_for(&latest.event_id);
                        let consumed = sqlx::query_scalar::<_, bool>(
                            "SELECT EXISTS(
                                 SELECT 1 FROM integration_consumer_receipts
                                 WHERE consumer_id = $1 AND source = $2 AND event_id = $3
                             )",
                        )
                        .bind(consumer_id)
                        .bind(&event.source)
                        .bind(deleted_event_id)
                        .fetch_one(&mut **tx)
                        .await?;
                        if consumed {
                            return Ok(None);
                        }
                    }
                }
                let record =
                    project_record(event.tenant_id, event.workspace_id, data, policy, content)
                        // The policy gate above already decided Project, so a Skip
                        // here is unreachable; keep the Option shape of the pure fn
                        // and fail closed rather than persist a record the policy
                        // would refuse.
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "policy gate passed but project_record skipped the event"
                            )
                        })?;
                insert_in_tx(tx, &record).await?;
                record
            }
        };
        Ok(Some(persisted))
    }

    /// Reconciliation repair path (admin): upsert a set of records keyed by
    /// the unique `(tenant_id, source_application, source_type, message_id)` —
    /// insert new, update existing. Does NOT touch receipts. Returns counts.
    ///
    /// Tombstoned records are immutable: the `DO UPDATE` is guarded by
    /// `WHERE indexing_status <> 'tombstoned'`, so a conflict row that is
    /// already tombstoned is never re-written. (a) Deletes are irreversible
    /// per the projection semantics — a relay that backdates a delete below a
    /// reconcile `since` window must never re-flip a tombstoned record back to
    /// `created`/`content_stored`. (b) The insert-vs-update counters come from
    /// `RETURNING (xmax = 0)` and are advisory for admin reporting, not
    /// correctness: a skipped (WHERE-false) tombstoned conflict row returns NO
    /// row (`INSERT 0 0`), so it counts as neither created nor updated.
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
                 WHERE memory_catalog.indexing_status <> 'tombstoned' \
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
            .bind(record.channel_kind.as_str())
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
            .fetch_optional(&self.pool)
            .await?;
            let Some(row) = row else {
                // A skipped (WHERE-false) tombstoned conflict row: immutable,
                // counted as neither created nor updated (see the docstring).
                continue;
            };
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

    /// Memory-owned keyword search over catalog records (candidates only; final
    /// authorization is the source owner's). Tombstoned records never appear.
    ///
    /// Matches the message content when ANY significant query term appears in
    /// it (the whole query phrase is kept as one of the terms, so exact-phrase
    /// questions still hit), and the `message_id`/`author_pubkey` exactly or
    /// the `channel_id` as a substring of the whole query; returns at most
    /// `limit` rows ordered newest-first. An empty/whitespace `query` returns
    /// no rows.
    pub async fn search(
        &self,
        tenant_id: TenantId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryCatalogRecord>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        // ILIKE wildcards are escaped so the user's input matches literally.
        // The whole phrase and every significant term are bound as escaped
        // ILIKE patterns; the escaped whole query is bound for the channel_id
        // substring clause, and the raw trimmed query is bound separately for
        // the exact message_id/author_pubkey match.
        let terms = content_match_terms(query);
        let content_clauses: Vec<String> = terms
            .iter()
            .enumerate()
            .map(|(index, _)| format!("content ILIKE '%' || ${} || '%'", index + 2))
            .collect();
        let channel_param = terms.len() + 2;
        let exact_param = channel_param + 1;
        let limit_param = exact_param + 1;
        let limit = limit as i64;
        let sql = format!(
            "SELECT {RECORD_COLUMNS} FROM memory_catalog
             WHERE tenant_id = $1
               AND source_application = 'io.elembra.chat'
               AND source_type = 'message'
               AND indexing_status <> 'tombstoned'
               AND ({}
                    OR message_id = ${}
                    OR author_pubkey = ${}
                    OR channel_id ILIKE '%' || ${} || '%')
             ORDER BY occurred_at DESC, message_id
             LIMIT ${}",
            content_clauses.join(" OR "),
            exact_param,
            exact_param,
            channel_param,
            limit_param,
        );
        let mut query_builder = sqlx::query(&sql).bind(tenant_id.0);
        for term in &terms {
            query_builder = query_builder.bind(escape_ilike(term));
        }
        let rows = query_builder
            .bind(escape_ilike(query))
            .bind(query)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(row_to_record).collect()
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
    .bind(record.channel_kind.as_str())
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
    .bind(record.channel_kind.as_str())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_match_terms_keeps_whole_phrase_first_and_significant_tokens() {
        let terms = content_match_terms("what did the team say about the budget");
        assert_eq!(
            terms.first().map(String::as_str),
            Some("what did the team say about the budget"),
            "the whole phrase stays the first term"
        );
        for expected in ["team", "say", "budget"] {
            assert!(
                terms.iter().any(|term| term == expected),
                "significant term `{expected}` must be kept, got: {terms:?}"
            );
        }
        for dropped in ["what", "did", "the", "about"] {
            assert!(
                terms.iter().all(|term| term != dropped),
                "stopword `{dropped}` must be dropped, got: {terms:?}"
            );
        }
    }

    #[test]
    fn content_match_terms_strips_surrounding_punctuation_from_tokens() {
        assert_eq!(
            content_match_terms("what did dave say about the budget?"),
            vec![
                "what did dave say about the budget?",
                "dave",
                "say",
                "budget"
            ],
            "trailing `?` is stripped so `budget` matches `the budget is capped at 10k`"
        );
        assert_eq!(
            content_match_terms("(quarterly plan) approved!"),
            vec![
                "(quarterly plan) approved!",
                "quarterly",
                "plan",
                "approved"
            ]
        );
    }

    #[test]
    fn content_match_terms_drops_short_and_stopword_only_tokens() {
        assert_eq!(
            content_match_terms("ab cd ef"),
            vec!["ab cd ef"],
            "tokens shorter than 3 chars are dropped; only the phrase remains"
        );
        assert_eq!(
            content_match_terms("what did the"),
            vec!["what did the"],
            "stopword-only tokens are dropped; only the phrase remains"
        );
        assert_eq!(
            content_match_terms("to be or not to be"),
            vec!["to be or not to be"],
            "a phrase of only stopwords/short tokens keeps no term beyond itself"
        );
    }

    #[test]
    fn content_match_terms_empty_and_whitespace_questions_have_no_terms() {
        assert!(content_match_terms("").is_empty());
        assert!(content_match_terms("   ").is_empty());
        assert!(content_match_terms("\t\n").is_empty());
    }

    #[test]
    fn content_match_terms_is_case_insensitive_for_stopwords() {
        let terms = content_match_terms("What Did Dave Say");
        assert!(!terms.iter().any(|term| term.eq_ignore_ascii_case("what")));
        assert!(!terms.iter().any(|term| term.eq_ignore_ascii_case("did")));
        assert!(terms.iter().any(|term| term == "Dave"));
        assert!(terms.iter().any(|term| term == "Say"));
    }

    #[test]
    fn content_match_terms_deduplicates_phrase_and_tokens() {
        assert_eq!(
            content_match_terms("budget"),
            vec!["budget"],
            "a single-token query must not duplicate the phrase and the token"
        );
        assert_eq!(
            content_match_terms("budget budget"),
            vec!["budget budget", "budget"],
            "repeated tokens are deduplicated"
        );
    }

    #[test]
    fn content_match_terms_caps_at_max_search_terms() {
        let query = (0..80)
            .map(|i| format!("alpha{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let terms = content_match_terms(&query);
        assert_eq!(terms.len(), MAX_SEARCH_TERMS, "the term list is capped");
        assert_eq!(
            terms.first().map(String::as_str),
            Some(query.as_str()),
            "the whole phrase always stays the first term"
        );
        assert!(
            terms.iter().any(|term| term == "alpha0"),
            "the first significant token is kept"
        );
        assert!(
            terms.iter().any(|term| term == "alpha62"),
            "the last token inside the cap is kept"
        );
        assert!(
            terms.iter().all(|term| term != "alpha63"),
            "tokens beyond the cap are dropped"
        );
        assert!(
            terms.iter().all(|term| term != "alpha79"),
            "the tail of the query is dropped"
        );
    }
}
