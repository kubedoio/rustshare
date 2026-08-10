//! Admin reconciliation of the Buzz → Elembra Memory projection.
//!
//! `POST /api/v1/admin/applications/memory/chat/reconcile` re-projects the
//! tenant's Memory catalog records from the signature-verified observation
//! index (`chat_observed_events`). This is the repair path only: no outbox
//! replay, no private Buzz database access, no consumer receipts are touched.
//! Idempotent — re-running with no drift changes nothing.

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use rustshare_core::domain::{TenantId, WorkspaceId};
use rustshare_memory::project::rebuild_records;
use rustshare_storage::{
    ChatIdentityStore, ChatObservationStore, MemoryCatalogStore, ReconcileCounts,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AdminUser, AuthenticatedUser, ErrorResponse};
use crate::AppState;

/// POST body: the tenant to reconcile and an optional `since` watermark.
#[derive(Debug, Deserialize)]
pub struct ReconcileRequest {
    pub tenant_id: Uuid,
    /// Optional ISO8601 timestamp. Only observations with
    /// `event_created_at >= since` are re-projected; parsed by the handler so
    /// a malformed value returns 400.
    pub since: Option<String>,
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

    let counts = reconcile_chat_memory_for_tenant(
        &ChatIdentityStore::new(state.db_pool.clone()),
        &state.chat_observation_store,
        &state.memory_catalog_store,
        TenantId(body.tenant_id),
        since,
    )
    .await
    .map_err(|error| {
        tracing::error!(tenant = %body.tenant_id, %error, "memory chat reconcile failed");
        internal_error()
    })?;

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

/// Static-details 500, mirroring the buzz_events handler's convention: the
/// specifics are logged, never echoed to the client.
fn internal_error() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new("Internal server error")),
    )
}
