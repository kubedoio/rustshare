//! Admin reconciliation of the Buzz → Elembra Memory projection.
//!
//! `POST /api/v1/admin/applications/memory/chat/reconcile` re-projects the
//! tenant's Memory catalog records. The repair source is selectable via the
//! request `source` field:
//!
//! - `"observation"` (default) — re-projects from the signature-verified
//!   observation index (`chat_observed_events`);
//! - `"buzz"` — first REPAIRS the observation index from the community's
//!   authoritative Buzz relay over the public HTTP contract (never Buzz's
//!   private DB), then re-projects from the repaired index.
//!
//! Either way this is the repair path only: no outbox replay, no consumer
//! receipts are touched. Idempotent — re-running with no drift changes
//! nothing.

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use rustshare_core::domain::{TenantId, WorkspaceId};
use rustshare_memory::event::{ChatChannelKind, ObservedEventType};
use rustshare_memory::project::rebuild_records;
use rustshare_resource_auth::BuzzChannelKind;
use rustshare_storage::{
    ChatIdentityStore, ChatObservationStore, MemoryCatalogStore, ReconcileCounts,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AdminUser, AuthenticatedUser, ErrorResponse};
use crate::buzz_gateway::BuzzGatewayClient;
use crate::buzz_observation::{
    BuzzEventPush, BuzzObservationService, BuzzPushContext, IngestOutcome,
};
use crate::AppState;

/// POST body: the tenant to reconcile, an optional `since` watermark, and the
/// repair `source` — `"observation"` (default; the signature-verified
/// observation index) or `"buzz"` (the authoritative Buzz relay over the
/// public HTTP contract).
#[derive(Debug, Deserialize)]
pub struct ReconcileRequest {
    pub tenant_id: Uuid,
    /// Optional ISO8601 timestamp. Messages with observations at or after
    /// `since` are re-projected from their complete observation history;
    /// parsed by the handler so a malformed value returns 400.
    pub since: Option<String>,
    /// Repair source selector: `"observation"` (default) or `"buzz"`.
    #[serde(default = "default_reconcile_source")]
    pub source: String,
}

/// The default reconcile source is the signature-verified observation index.
fn default_reconcile_source() -> String {
    "observation".into()
}

/// Validate the reconcile `source` selector: only the local observation index
/// (default) or the authoritative Buzz relay are valid repair sources.
fn validate_reconcile_source(source: &str) -> Result<(), &'static str> {
    match source {
        "observation" | "buzz" => Ok(()),
        _ => Err("source must be \"observation\" or \"buzz\""),
    }
}

/// POST response: observation rows examined and catalog writes performed.
#[derive(Debug, Serialize)]
pub struct ReconcileResponse {
    pub processed: u64,
    pub created: u64,
    pub updated: u64,
}

/// POST /api/v1/admin/applications/memory/chat/reconcile — admin-only.
///
/// Admin tenant-scoping mirrors `chat_identity::configure_mapping`: the
/// admin's own tenant must be the reconcile target (`tenant_id` in the body).
pub async fn reconcile_chat_memory(
    AdminUser { user_id: admin_id }: AdminUser,
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Json(body): Json<ReconcileRequest>,
) -> Result<Json<ReconcileResponse>, (StatusCode, Json<ErrorResponse>)> {
    let admin_tenant = sqlx::query_scalar::<_, Uuid>(
        "SELECT tenant_id FROM users WHERE id = $1 AND disabled_at IS NULL",
    )
    .bind(admin_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|error| {
        tracing::error!(admin = %admin_id, %error, "memory reconcile: admin lookup failed");
        internal_error()
    })?
    .ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("Unauthorized")),
        )
    })?;
    if admin_tenant != auth.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("tenant scope mismatch")),
        ));
    }
    if admin_tenant != body.tenant_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new(
                "reconcile target is outside the admin's tenant scope",
            )),
        ));
    }
    let since = parse_since(body.since.as_deref())?;
    if let Err(reason) = validate_reconcile_source(&body.source) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(format!(
                "invalid reconcile source: {reason}"
            ))),
        ));
    }

    let chat_identity = ChatIdentityStore::new(state.db_pool.clone());
    let counts = if body.source == "buzz" {
        // Repair from the authoritative Buzz relay. Fail closed when this
        // deployment has no gateway configured: reconcile-from-Buzz must not
        // silently fall back to the observation index.
        let gateway = state.buzz_gateway.as_ref().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new(
                    "Buzz source authority not configured for this deployment",
                )),
            )
        })?;
        reconcile_chat_memory_from_buzz_for_tenant(
            &state.buzz_observation_service,
            &chat_identity,
            &state.chat_observation_store,
            &state.memory_catalog_store,
            gateway,
            TenantId(body.tenant_id),
            since,
        )
        .await
        .map_err(|error| {
            tracing::error!(tenant = %body.tenant_id, %error, "memory chat reconcile from Buzz failed");
            internal_error()
        })?
    } else {
        reconcile_chat_memory_for_tenant(
            &chat_identity,
            &state.chat_observation_store,
            &state.memory_catalog_store,
            TenantId(body.tenant_id),
            since,
        )
        .await
        .map_err(|error| {
            tracing::error!(tenant = %body.tenant_id, %error, "memory chat reconcile failed");
            internal_error()
        })?
    };

    Ok(Json(ReconcileResponse {
        processed: counts.processed,
        created: counts.created,
        updated: counts.updated,
    }))
}

/// Parse the optional `since` watermark (ISO8601); malformed ⇒ 400.
fn parse_since(
    raw: Option<&str>,
) -> Result<Option<DateTime<Utc>>, (StatusCode, Json<ErrorResponse>)> {
    match raw {
        None => Ok(None),
        Some(raw) => DateTime::parse_from_rfc3339(raw)
            .map(|timestamp| Some(timestamp.with_timezone(&Utc)))
            .map_err(|error| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(format!(
                        "since must be an ISO8601 timestamp: {error}"
                    ))),
                )
            }),
    }
}

/// Rebuild the tenant's Memory catalog from the observation index (no outbox
/// replay, no receipts). `workspace == tenant` per the platform invariant, so
/// the projection policy is read for `WorkspaceId(tenant_id)`. Exported for
/// the DB-backed integration suite.
///
/// `ReconcileCounts::processed` counts the observation rows examined (the
/// source of the rebuild), not the resulting catalog records.
pub async fn reconcile_chat_memory_for_tenant(
    chat_identity: &ChatIdentityStore,
    observations: &ChatObservationStore,
    catalog: &MemoryCatalogStore,
    tenant_id: TenantId,
    since: Option<DateTime<Utc>>,
) -> anyhow::Result<ReconcileCounts> {
    let policy = chat_identity
        .projection_policy(tenant_id, WorkspaceId(tenant_id.0))
        .await?;
    let rows = observations.list_for_reconcile(tenant_id, since).await?;
    let records = rebuild_records(&rows, &policy);
    let mut counts = catalog.upsert_records(&records).await?;
    counts.processed = rows.len() as u64;
    Ok(counts)
}

/// Maximum state pages a single reconcile is willing to consume from the
/// authoritative relay; a misbehaving relay must not loop a repair forever.
/// 10_000 pages × [`BUZZ_PAGE_LIMIT`] (200/page) caps the repair at 2M
/// entries. If the cap aborts the repair, the observation index may be
/// partially repaired and the catalog fold is skipped — re-running is safe
/// (idempotent).
const MAX_RECONCILE_PAGES: u32 = 10_000;
/// Page size for the authoritative relay's state endpoint.
const BUZZ_PAGE_LIMIT: u32 = 200;

/// Repair the tenant's chat projection from the authoritative Buzz relay.
///
/// Admin repair over the public HTTP contract ([`BuzzGatewayClient::page_state`]):
/// the relay's signed state pages (kind-19030 envelopes pinned to the mapping's
/// `relay_pubkey`) are paged, and every entry event is independently
/// signature-verified by [`BuzzObservationService::ingest_without_outbox`] as
/// it is written into the observation index. This never reads Buzz's private
/// DB and never publishes outbox events — the observation rows and the Memory
/// catalog are written directly, so consumer receipts stay untouched and the
/// durable pipeline is never replayed. Idempotent: re-running with no drift
/// changes nothing.
///
/// Fail closed: a missing/inactive community mapping, a missing pinned relay
/// pubkey, or an empty relay URL aborts the repair before any write. A single
/// bad entry (invalid context, unverifiable signature, unbound author) is
/// skipped and logged loudly — one poisoned entry must not abort the repair —
/// but an unknown `channel_kind` on the wire fails the whole signed page
/// (treated as a relay contract violation → 500), whereas unknown `event_type`
/// and per-entry ingest errors skip just that entry. A relay that never
/// terminates its page stream aborts at [`MAX_RECONCILE_PAGES`].
///
/// `ReconcileCounts::processed` counts the entries paged from Buzz (the repair
/// source), not observation rows.
#[allow(clippy::too_many_arguments)]
pub async fn reconcile_chat_memory_from_buzz_for_tenant(
    service: &BuzzObservationService,
    chat_identity: &ChatIdentityStore,
    observations: &ChatObservationStore,
    catalog: &MemoryCatalogStore,
    gateway: &BuzzGatewayClient,
    tenant_id: TenantId,
    since: Option<DateTime<Utc>>,
) -> anyhow::Result<ReconcileCounts> {
    // Fail closed on the community mapping: reconcile-from-Buzz is only
    // meaningful for a tenant whose mapping is active AND pinned.
    let mapping = chat_identity
        .mapping(tenant_id, WorkspaceId(tenant_id.0))
        .await?
        .ok_or_else(|| anyhow::anyhow!("no chat community mapping for tenant"))?;
    if !mapping.active {
        anyhow::bail!("chat community mapping is not active");
    }
    if mapping.relay_url.is_empty() {
        anyhow::bail!("chat community mapping has no relay_url");
    }
    let relay_pubkey = mapping
        .relay_pubkey
        .as_deref()
        .filter(|pubkey| !pubkey.is_empty())
        .ok_or_else(|| anyhow::anyhow!("chat community mapping has no pinned relay_pubkey"))?;

    // Stored `event_created_at` values are whole-second, so flooring `since`
    // keeps the fetch and fold windows identical.
    let since = since
        .map(|t| {
            DateTime::<Utc>::from_timestamp(t.timestamp(), 0)
                .ok_or_else(|| anyhow::anyhow!("since timestamp out of range"))
        })
        .transpose()?;
    // Page the relay's signed state; the gateway verifies each page envelope
    // (kind 19030 + pinned relay pubkey), and each entry event is verified by
    // `ingest_without_outbox` below.
    let since_ts = since.map(|timestamp| timestamp.timestamp());
    let mut cursor: Option<String> = None;
    let mut pages: u32 = 0;
    let mut first_observations: u64 = 0;
    let mut duplicate_observations: u64 = 0;
    let mut skipped: u64 = 0;
    let mut processed: u64 = 0;
    loop {
        pages += 1;
        if pages > MAX_RECONCILE_PAGES {
            anyhow::bail!(
                "Buzz state paging exceeded {MAX_RECONCILE_PAGES} pages; aborting reconcile"
            );
        }
        let page = gateway
            .page_state(
                &mapping.relay_url,
                relay_pubkey,
                since_ts,
                BUZZ_PAGE_LIMIT,
                cursor.as_deref(),
            )
            .await?;
        for entry in &page.entries {
            processed += 1;
            // Log-only, untrusted: the raw JSON `id` is never used for
            // identity — `validate_and_build` derives the observation identity
            // from the parsed, signature-verified event.
            let event_id = entry.event.get("id").and_then(serde_json::Value::as_str);
            let Some(event_type) = parse_observed_event_type(&entry.context.event_type) else {
                skipped += 1;
                tracing::warn!(
                    tenant = %tenant_id,
                    event_id = event_id.unwrap_or("<missing>"),
                    event_type = %entry.context.event_type,
                    "reconcile-from-buzz: skipping entry with invalid event_type"
                );
                continue;
            };
            // Tenant-scope guard: `validate_and_build` routes each entry via
            // the global `mapping_by_community`, deriving the row's tenant from
            // the community, so a shared relay serving multiple communities
            // could otherwise write rows into another tenant's observation
            // index during this tenant's admin repair.
            if entry.context.community_id != mapping.community_id {
                skipped += 1;
                tracing::warn!(
                    tenant = %tenant_id,
                    community = %entry.context.community_id,
                    event_id = event_id.unwrap_or("<missing>"),
                    "reconcile skipped entry from a community outside the target mapping"
                );
                continue;
            }
            let push = BuzzEventPush {
                event: entry.event.clone(),
                context: BuzzPushContext {
                    community_id: entry.context.community_id.clone(),
                    channel_id: entry.context.channel_id.clone(),
                    channel_kind: map_channel_kind(entry.context.channel_kind),
                    thread_root_id: entry.context.thread_root_id.clone(),
                    message_id: entry.context.message_id.clone(),
                    event_type,
                    supersedes_event_id: entry.context.supersedes_event_id.clone(),
                },
            };
            match service.ingest_without_outbox(&push).await {
                Ok(IngestOutcome::FirstObservation) => first_observations += 1,
                Ok(IngestOutcome::DuplicateObservation) => duplicate_observations += 1,
                Err(error) => {
                    // A repair must not be aborted by one bad entry: the event
                    // is rejected (or skipped) loudly and the rest of the
                    // stream is still repaired.
                    skipped += 1;
                    tracing::warn!(
                        tenant = %tenant_id,
                        event_id = event_id.unwrap_or("<missing>"),
                        %error,
                        "reconcile-from-buzz: entry rejected by ingestion, skipping"
                    );
                }
            }
        }
        if page.complete {
            break;
        }
        cursor = page.cursor;
    }
    tracing::info!(
        tenant = %tenant_id,
        pages,
        processed,
        first_observations,
        duplicate_observations,
        skipped,
        "reconcile-from-buzz: observation index repaired from authoritative Buzz state"
    );

    // Reuse the existing fold: re-projection from the (now repaired)
    // observation index rebuilds the Memory catalog idempotently.
    let policy = chat_identity
        .projection_policy(tenant_id, WorkspaceId(tenant_id.0))
        .await?;
    let rows = observations.list_for_reconcile(tenant_id, since).await?;
    let records = rebuild_records(&rows, &policy);
    let mut counts = catalog.upsert_records(&records).await?;
    counts.processed = processed;
    Ok(counts)
}

/// Map a wire [`BuzzChannelKind`] to the Memory crate's [`ChatChannelKind`];
/// both are the same 4-variant snake_case classification, so this is a pure
/// identity mapping across the crate boundary.
fn map_channel_kind(kind: BuzzChannelKind) -> ChatChannelKind {
    match kind {
        BuzzChannelKind::Workspace => ChatChannelKind::Workspace,
        BuzzChannelKind::Dm => ChatChannelKind::Dm,
        BuzzChannelKind::Private => ChatChannelKind::Private,
        BuzzChannelKind::Excluded => ChatChannelKind::Excluded,
    }
}

/// Parse a wire `event_type` string (`created|edited|deleted`) into
/// [`ObservedEventType`]; `None` for any other value.
fn parse_observed_event_type(raw: &str) -> Option<ObservedEventType> {
    match raw {
        "created" => Some(ObservedEventType::Created),
        "edited" => Some(ObservedEventType::Edited),
        "deleted" => Some(ObservedEventType::Deleted),
        _ => None,
    }
}

/// Static-details 500, mirroring the buzz_events handler's convention: the
/// specifics are logged, never echoed to the client.
fn internal_error() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new("Internal server error")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_source_defaults_to_observation() {
        // Serde default: an absent `source` field selects the observation
        // index...
        let request: ReconcileRequest = serde_json::from_value(serde_json::json!({
            "tenant_id": Uuid::new_v4(),
            "since": null,
        }))
        .unwrap();
        assert_eq!(request.source, "observation");
        // ...and an explicit value is honored.
        let request: ReconcileRequest = serde_json::from_value(serde_json::json!({
            "tenant_id": Uuid::new_v4(),
            "source": "buzz",
        }))
        .unwrap();
        assert_eq!(request.source, "buzz");
    }

    #[test]
    fn validate_reconcile_source_accepts_only_known_sources() {
        assert!(validate_reconcile_source("observation").is_ok());
        assert!(validate_reconcile_source("buzz").is_ok());
        assert!(validate_reconcile_source("").is_err());
        assert!(validate_reconcile_source("relay").is_err());
        assert!(validate_reconcile_source("Observation").is_err());
        assert!(validate_reconcile_source("buzz ").is_err());
    }

    #[test]
    fn parse_observed_event_type_maps_wire_strings() {
        assert_eq!(
            parse_observed_event_type("created"),
            Some(ObservedEventType::Created)
        );
        assert_eq!(
            parse_observed_event_type("edited"),
            Some(ObservedEventType::Edited)
        );
        assert_eq!(
            parse_observed_event_type("deleted"),
            Some(ObservedEventType::Deleted)
        );
        assert_eq!(parse_observed_event_type("expired"), None);
        assert_eq!(parse_observed_event_type(""), None);
    }

    #[test]
    fn map_channel_kind_mirrors_all_four_variants() {
        assert_eq!(
            map_channel_kind(BuzzChannelKind::Workspace),
            ChatChannelKind::Workspace
        );
        assert_eq!(map_channel_kind(BuzzChannelKind::Dm), ChatChannelKind::Dm);
        assert_eq!(
            map_channel_kind(BuzzChannelKind::Private),
            ChatChannelKind::Private
        );
        assert_eq!(
            map_channel_kind(BuzzChannelKind::Excluded),
            ChatChannelKind::Excluded
        );
    }
}
