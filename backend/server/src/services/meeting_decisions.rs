//! Durable creation commands and associations for Decisions owned by the Decisions Application.

use chrono::{DateTime, Utc};
use rustshare_core::events::{Event, EventBroadcaster};
use rustshare_storage::EventStore;
use sqlx::PgPool;
use uuid::Uuid;

use super::meeting_service::MeetingResourceLink;

#[derive(Debug, sqlx::FromRow)]
pub struct DecisionCreationCommand {
    pub meeting_folder_id: Uuid,
    pub decision_file_id: Uuid,
    pub sidecar_file_id: Uuid,
    pub payload_hash: String,
    pub created_at: DateTime<Utc>,
    pub ready: bool,
}

pub async fn reserve_creation(
    pool: &PgPool,
    tenant_id: Uuid,
    actor_id: Uuid,
    request_id: Uuid,
    meeting_id: Uuid,
    payload_hash: &str,
) -> Result<DecisionCreationCommand, sqlx::Error> {
    sqlx::query("INSERT INTO meeting_decision_creations
        (tenant_id, actor_id, request_id, meeting_folder_id, decision_file_id, sidecar_file_id, payload_hash)
        VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (tenant_id, actor_id, request_id) DO NOTHING")
        .bind(tenant_id).bind(actor_id).bind(request_id).bind(meeting_id)
        .bind(Uuid::new_v4()).bind(Uuid::new_v4()).bind(payload_hash)
        .execute(pool).await?;
    sqlx::query_as("SELECT meeting_folder_id, decision_file_id, sidecar_file_id, payload_hash, created_at, ready
        FROM meeting_decision_creations WHERE tenant_id=$1 AND actor_id=$2 AND request_id=$3")
        .bind(tenant_id).bind(actor_id).bind(request_id).fetch_one(pool).await
}

pub async fn link_decision(
    pool: &PgPool,
    tenant_id: Uuid,
    meeting_id: Uuid,
    resource_ref: &str,
) -> Result<Option<MeetingResourceLink>, sqlx::Error> {
    let inserted = sqlx::query_as(
        "INSERT INTO meeting_resource_links
        (tenant_id, meeting_folder_id, resource_ref_uri, link_kind) VALUES ($1,$2,$3,'decision')
        ON CONFLICT (tenant_id, meeting_folder_id, resource_ref_uri)
        DO NOTHING RETURNING id, resource_ref_uri, created_at",
    )
    .bind(tenant_id)
    .bind(meeting_id)
    .bind(resource_ref)
    .fetch_optional(pool)
    .await?;
    match inserted {
        Some(link) => Ok(Some(link)),
        None => sqlx::query_as(
            "SELECT id, resource_ref_uri, created_at FROM meeting_resource_links
            WHERE tenant_id=$1 AND meeting_folder_id=$2 AND resource_ref_uri=$3 AND link_kind='decision'",
        )
        .bind(tenant_id)
        .bind(meeting_id)
        .bind(resource_ref)
        .fetch_optional(pool)
        .await,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn finish_creation(
    pool: &PgPool,
    event_store: &EventStore,
    broadcaster: &EventBroadcaster,
    tenant_id: Uuid,
    actor_id: Uuid,
    request_id: Uuid,
    resource_ref: &str,
    event: &Event,
) -> anyhow::Result<Option<MeetingResourceLink>> {
    let mut tx = pool.begin().await?;
    let (meeting_id, ready): (Uuid, bool) = sqlx::query_as("SELECT meeting_folder_id, ready
        FROM meeting_decision_creations WHERE tenant_id=$1 AND actor_id=$2 AND request_id=$3 FOR UPDATE")
        .bind(tenant_id).bind(actor_id).bind(request_id).fetch_one(&mut *tx).await?;
    if !ready {
        let inserted = sqlx::query("INSERT INTO meeting_resource_links (tenant_id,meeting_folder_id,resource_ref_uri,link_kind)
            VALUES ($1,$2,$3,'decision') ON CONFLICT (tenant_id,meeting_folder_id,resource_ref_uri)
            DO NOTHING RETURNING id")
            .bind(tenant_id).bind(meeting_id).bind(resource_ref).fetch_optional(&mut *tx).await?;
        if inserted.is_none() {
            let existing_kind: Option<String> = sqlx::query_scalar(
                "SELECT link_kind FROM meeting_resource_links
                WHERE tenant_id=$1 AND meeting_folder_id=$2 AND resource_ref_uri=$3",
            )
            .bind(tenant_id)
            .bind(meeting_id)
            .bind(resource_ref)
            .fetch_optional(&mut *tx)
            .await?;
            if existing_kind.as_deref() != Some("decision") {
                tx.rollback().await?;
                return Ok(None);
            }
        }
        event_store.append_in_tx(&mut tx, event).await?;
        sqlx::query(
            "UPDATE meeting_decision_creations SET ready=true
            WHERE tenant_id=$1 AND actor_id=$2 AND request_id=$3",
        )
        .bind(tenant_id)
        .bind(actor_id)
        .bind(request_id)
        .execute(&mut *tx)
        .await?;
    }
    let link = sqlx::query_as("SELECT id, resource_ref_uri, created_at FROM meeting_resource_links
        WHERE tenant_id=$1 AND meeting_folder_id=$2 AND resource_ref_uri=$3 AND link_kind='decision'")
        .bind(tenant_id).bind(meeting_id).bind(resource_ref).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    if !ready {
        broadcaster.publish(event.clone());
    }
    Ok(link)
}

pub async fn list_links(
    pool: &PgPool,
    tenant_id: Uuid,
    meeting_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<MeetingResourceLink>, i64), sqlx::Error> {
    let links = sqlx::query_as(
        "SELECT id, resource_ref_uri, created_at FROM meeting_resource_links
        WHERE tenant_id=$1 AND meeting_folder_id=$2 AND link_kind='decision'
        ORDER BY created_at,id LIMIT $3 OFFSET $4",
    )
    .bind(tenant_id)
    .bind(meeting_id)
    .bind(limit.clamp(1, 100))
    .bind(offset.max(0))
    .fetch_all(pool)
    .await?;
    let total = sqlx::query_scalar(
        "SELECT count(*) FROM meeting_resource_links
        WHERE tenant_id=$1 AND meeting_folder_id=$2 AND link_kind='decision'",
    )
    .bind(tenant_id)
    .bind(meeting_id)
    .fetch_one(pool)
    .await?;
    Ok((links, total))
}

pub async fn meeting_links(
    pool: &PgPool,
    tenant_id: Uuid,
    resource_ref: &str,
    decision_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<(Uuid, bool)>, sqlx::Error> {
    sqlx::query_as("SELECT l.meeting_folder_id, EXISTS(SELECT 1 FROM meeting_decision_creations c
            WHERE c.tenant_id=l.tenant_id AND c.meeting_folder_id=l.meeting_folder_id
              AND c.decision_file_id=$3 AND c.ready) AS is_origin
        FROM meeting_resource_links l WHERE l.tenant_id=$1 AND l.resource_ref_uri=$2 AND l.link_kind='decision'
        ORDER BY l.created_at,l.id LIMIT $4 OFFSET $5")
        .bind(tenant_id).bind(resource_ref).bind(decision_id).bind(limit.clamp(1,100)).bind(offset.max(0)).fetch_all(pool).await
}
