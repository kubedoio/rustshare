//! Bridge-owned observation index for signed Buzz chat events.
//!
//! One row per signed Buzz event observed, content-addressed by
//! `(tenant_id, event_id)`; event ids are `sha256` of the signed event, so a
//! primary-key conflict means the identical event was already observed. The
//! only rewrite is the reconcile tombstone flip: re-observing the same event
//! id with `event_type == Deleted` (the relay's `state/events` snapshot
//! tombstone) turns the row into a deleted tombstone. Buzz remains
//! authoritative — this table holds reference metadata, the identifier-only
//! `attachment_refs` retained from each event's `elembra-ref` tags, and
//! (optionally) an indexing copy of the body only.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_memory::event::{ChatChannelKind, ObservedEventType};
use rustshare_memory::observed::ChatObservedEvent;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone)]
pub struct ChatObservationStore {
    pool: PgPool,
}

/// Distinct channel derived from the observation index for one community.
#[derive(Debug, Clone)]
pub struct ChannelSummary {
    pub channel_id: String,
    pub channel_kind: ChatChannelKind,
    pub latest_event_at: DateTime<Utc>,
}

/// Outcome of one [`ChatObservationStore::upsert_event_in_tx`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    /// A new observation row was inserted.
    Inserted,
    /// The identical event was already observed and nothing changed — the
    /// caller rolls the transaction back (no-op).
    Duplicate,
    /// The existing row was flipped to a DELETED tombstone (the relay's
    /// `state/events` snapshot tombstone re-served the same event id). The
    /// caller must COMMIT the flip but must NOT re-publish the durable
    /// envelope (the Created envelope was already published on first
    /// observation).
    FlippedToTombstone,
}

impl ChatObservationStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Idempotent upsert inside `tx`. PK is `(tenant_id, event_id)`; event
    /// ids are content-addressed (id == sha256 of the signed event), so a
    /// conflict means the identical event was already observed. The ONLY
    /// rewrite allowed is the tombstone flip: a re-observation of the same
    /// event id with `event_type == Deleted` (the relay's `state/events`
    /// snapshot tombstone — reconcile) turns the existing row into a deleted
    /// tombstone (`active = false`). Any other conflict — including a
    /// re-observation of the original Created event — never rewrites the
    /// row, so a delayed replay cannot resurrect a deleted message.
    pub async fn upsert_event_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        event: &ChatObservedEvent,
    ) -> Result<UpsertOutcome> {
        let result = sqlx::query(
            r#"
            INSERT INTO chat_observed_events
                (tenant_id, workspace_id, event_id, message_id, event_type,
                 supersedes_event_id, community_id, channel_id, channel_kind,
                 thread_root_id, author_pubkey, author_principal_id,
                 event_created_at, observed_at, checksum, signature,
                 signature_verified, body, attachment_refs, active)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                    $14, $15, $16, $17, $18, $19, $20)
            ON CONFLICT (tenant_id, event_id) DO UPDATE
                SET event_type = 'deleted', observed_at = EXCLUDED.observed_at,
                    active = EXCLUDED.active
                WHERE chat_observed_events.event_type <> 'deleted'
                  AND EXCLUDED.event_type = 'deleted'
            RETURNING (xmax = 0) AS inserted
            "#,
        )
        .bind(event.tenant_id.0)
        .bind(event.workspace_id.0)
        .bind(&event.event_id)
        .bind(&event.message_id)
        .bind(event_type_str(event.event_type))
        .bind(&event.supersedes_event_id)
        .bind(&event.community_id)
        .bind(&event.channel_id)
        .bind(event.channel_kind.as_str())
        .bind(&event.thread_root_id)
        .bind(&event.author_pubkey)
        .bind(event.author_principal_id.map(|p| p.0))
        .bind(event.event_created_at)
        .bind(event.observed_at)
        .bind(&event.checksum)
        .bind(&event.signature)
        .bind(event.signature_verified)
        .bind(&event.body)
        .bind(serde_json::to_value(&event.attachment_refs)?)
        .bind(event.active)
        .fetch_optional(&mut **tx)
        .await?;
        Ok(match result {
            // A returned row: genuinely inserted (xmax = 0) or the tombstone
            // flip ran (xmax <> 0 — the only update this upsert allows).
            Some(row) if row.get::<bool, _>("inserted") => UpsertOutcome::Inserted,
            Some(_) => UpsertOutcome::FlippedToTombstone,
            // No row: the conflict was excluded by the WHERE clause — the
            // identical event was already observed and nothing changed.
            None => UpsertOutcome::Duplicate,
        })
    }

    /// Latest observed event row for a message (by `event_created_at` desc,
    /// `event_id` desc as a deterministic tie-break), if any. Used by the
    /// authorization owner adapter to route a message ref to its
    /// community/channel and to detect tombstones.
    pub async fn lookup_for_auth(
        &self,
        tenant_id: TenantId,
        message_id: &str,
    ) -> Result<Option<ChatObservedEvent>> {
        let row = sqlx::query(
            "SELECT tenant_id, workspace_id, event_id, message_id, event_type,
                    supersedes_event_id, community_id, channel_id, channel_kind,
                    thread_root_id, author_pubkey, author_principal_id,
                    event_created_at, observed_at, checksum, signature,
                    signature_verified, body, attachment_refs, active
             FROM chat_observed_events
             WHERE tenant_id = $1 AND message_id = $2
             ORDER BY event_created_at DESC, event_id DESC
             LIMIT 1",
        )
        .bind(tenant_id.0)
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| row_to_event(&row)).transpose()
    }

    /// Whether a Deleted observation exists for this message with
    /// `event_created_at >= since` (authorizer-level tombstone override:
    /// a message is never exposable once it has been deleted at-or-after the
    /// candidate row's time, even if a later edit was pushed).
    pub async fn has_tombstone_since(
        &self,
        tenant_id: TenantId,
        message_id: &str,
        since: DateTime<Utc>,
    ) -> Result<bool> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM chat_observed_events
                 WHERE tenant_id = $1 AND message_id = $2
                   AND event_type = 'deleted' AND event_created_at >= $3
             )",
        )
        .bind(tenant_id.0)
        .bind(message_id)
        .bind(since)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    /// The observation row for a specific Buzz event id (used by the Memory
    /// consumer to fetch the indexing-copy body when content_indexing is on).
    /// The PK is `(tenant_id, event_id)`, so this is a point lookup; an
    /// unknown event id yields `None` (a missing body must never block
    /// projection — the record is still created with reference-only status).
    pub async fn get_by_event_id(
        &self,
        tenant_id: TenantId,
        event_id: &str,
    ) -> Result<Option<ChatObservedEvent>> {
        let row = sqlx::query(
            "SELECT tenant_id, workspace_id, event_id, message_id, event_type,
                    supersedes_event_id, community_id, channel_id, channel_kind,
                    thread_root_id, author_pubkey, author_principal_id,
                    event_created_at, observed_at, checksum, signature,
                    signature_verified, body, attachment_refs, active
             FROM chat_observed_events
             WHERE tenant_id = $1 AND event_id = $2",
        )
        .bind(tenant_id.0)
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| row_to_event(&row)).transpose()
    }

    /// All observed events for a message, oldest first (reconciliation/audit).
    pub async fn get_by_message_id(
        &self,
        tenant_id: TenantId,
        message_id: &str,
    ) -> Result<Vec<ChatObservedEvent>> {
        let rows = sqlx::query(
            "SELECT tenant_id, workspace_id, event_id, message_id, event_type,
                    supersedes_event_id, community_id, channel_id, channel_kind,
                    thread_root_id, author_pubkey, author_principal_id,
                    event_created_at, observed_at, checksum, signature,
                    signature_verified, body, attachment_refs, active
             FROM chat_observed_events
             WHERE tenant_id = $1 AND message_id = $2
             ORDER BY event_created_at, event_id",
        )
        .bind(tenant_id.0)
        .bind(message_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_event).collect()
    }

    /// Observed events for a tenant. When `since` is set, it selects messages
    /// with an observation (`observed_at`) at or after the watermark, then returns each
    /// affected message's complete history so deletion/edit folds remain
    /// correct, ordered by `event_created_at` then `event_id`.
    pub async fn list_for_reconcile(
        &self,
        tenant_id: TenantId,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<ChatObservedEvent>> {
        let rows = match since {
            Some(since) => {
                sqlx::query(
                    "WITH affected_messages AS (
                         SELECT DISTINCT message_id
                         FROM chat_observed_events
                         WHERE tenant_id = $1 AND observed_at >= $2
                     )
                     SELECT tenant_id, workspace_id, event_id, message_id, event_type,
                            supersedes_event_id, community_id, channel_id, channel_kind,
                            thread_root_id, author_pubkey, author_principal_id,
                            event_created_at, observed_at, checksum, signature,
                            signature_verified, body, attachment_refs, active
                     FROM chat_observed_events
                     WHERE tenant_id = $1
                       AND message_id IN (SELECT message_id FROM affected_messages)
                     ORDER BY event_created_at, event_id, event_type",
                )
                .bind(tenant_id.0)
                .bind(since)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT tenant_id, workspace_id, event_id, message_id, event_type,
                            supersedes_event_id, community_id, channel_id, channel_kind,
                            thread_root_id, author_pubkey, author_principal_id,
                            event_created_at, observed_at, checksum, signature,
                            signature_verified, body, attachment_refs, active
                     FROM chat_observed_events
                     WHERE tenant_id = $1
                     ORDER BY event_created_at, event_id, event_type",
                )
                .bind(tenant_id.0)
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.iter().map(row_to_event).collect()
    }

    /// Distinct observed channels for a community, newest activity first.
    /// Derived projection only — Buzz owns channel identity.
    pub async fn distinct_channels(
        &self,
        tenant_id: TenantId,
        community_id: &str,
    ) -> Result<Vec<ChannelSummary>> {
        let rows = sqlx::query(
            "SELECT channel_id, channel_kind, latest_event_at
             FROM (
                 SELECT channel_id, channel_kind,
                        event_created_at AS latest_event_at,
                        ROW_NUMBER() OVER (
                            PARTITION BY channel_id
                            ORDER BY event_created_at DESC, event_id DESC
                        ) AS rn
                 FROM chat_observed_events
                 WHERE tenant_id = $1 AND community_id = $2 AND active = true
             ) ranked
             WHERE rn = 1
             ORDER BY latest_event_at DESC",
        )
        .bind(tenant_id.0)
        .bind(community_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(ChannelSummary {
                    channel_id: row.try_get("channel_id")?,
                    channel_kind: parse_channel_kind(row.try_get("channel_kind")?)?,
                    latest_event_at: row.try_get("latest_event_at")?,
                })
            })
            .collect()
    }

    /// Timeline fold: the latest active event of each message in one channel,
    /// `before` being an exclusive `event_created_at` watermark, newest first.
    /// The limit is pushed into SQL so large channels never materialize more
    /// than `limit` folded rows.
    pub async fn list_for_timeline(
        &self,
        tenant_id: TenantId,
        community_id: &str,
        channel_id: &str,
        before: Option<(DateTime<Utc>, String)>,
        limit: i64,
    ) -> Result<Vec<ChatObservedEvent>> {
        // Fold the FULL channel history (DISTINCT ON picks the latest event
        // per message), then apply the keyset cursor after the fold. Filtering
        // before the fold would re-emit a superseded event as "latest" on the
        // next page (stale bodies, duplicates). The (event_created_at,
        // event_id) keyset — not a timestamp alone — guarantees same-second
        // messages are never skipped or revisited.
        let rows = sqlx::query(
            "SELECT * FROM (
                 SELECT DISTINCT ON (message_id)
                        tenant_id, workspace_id, event_id, message_id, event_type,
                        supersedes_event_id, community_id, channel_id, channel_kind,
                        thread_root_id, author_pubkey, author_principal_id,
                        event_created_at, observed_at, checksum, signature,
                        signature_verified, body, attachment_refs, active
                 FROM chat_observed_events
                 WHERE tenant_id = $1 AND community_id = $2 AND channel_id = $3
                 ORDER BY message_id, event_created_at DESC, event_id DESC
             ) folded
             WHERE ($4::timestamptz IS NULL
                    OR (event_created_at, event_id) < ($4, $5))
             ORDER BY event_created_at DESC, event_id DESC
             LIMIT $6",
        )
        .bind(tenant_id.0)
        .bind(community_id)
        .bind(channel_id)
        .bind(before.as_ref().map(|(created_at, _)| *created_at))
        .bind(before.as_ref().map(|(_, event_id)| event_id.clone()))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_event).collect()
    }
}

fn row_to_event(row: &sqlx::postgres::PgRow) -> Result<ChatObservedEvent> {
    Ok(ChatObservedEvent {
        tenant_id: TenantId(row.try_get("tenant_id")?),
        workspace_id: WorkspaceId(row.try_get("workspace_id")?),
        event_id: row.try_get("event_id")?,
        message_id: row.try_get("message_id")?,
        event_type: parse_event_type(row.try_get("event_type")?)?,
        supersedes_event_id: row.try_get("supersedes_event_id")?,
        community_id: row.try_get("community_id")?,
        channel_id: row.try_get("channel_id")?,
        channel_kind: parse_channel_kind(row.try_get("channel_kind")?)?,
        thread_root_id: row.try_get("thread_root_id")?,
        author_pubkey: row.try_get("author_pubkey")?,
        author_principal_id: row
            .try_get::<Option<Uuid>, _>("author_principal_id")?
            .map(PrincipalId),
        event_created_at: row.try_get("event_created_at")?,
        observed_at: row.try_get("observed_at")?,
        checksum: row.try_get("checksum")?,
        signature: row.try_get("signature")?,
        signature_verified: row.try_get("signature_verified")?,
        body: row.try_get("body")?,
        attachment_refs: serde_json::from_value(row.try_get("attachment_refs")?)?,
        active: row.try_get("active")?,
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
        other => anyhow::bail!("unknown chat_observed_events.event_type `{other}`"),
    })
}

fn parse_channel_kind(value: String) -> Result<ChatChannelKind> {
    Ok(match value.as_str() {
        "workspace" => ChatChannelKind::Workspace,
        "dm" => ChatChannelKind::Dm,
        "private" => ChatChannelKind::Private,
        "excluded" => ChatChannelKind::Excluded,
        other => anyhow::bail!("unknown chat_observed_events.channel_kind `{other}`"),
    })
}
