//! Meeting-owned action records. Row, command receipt and event commit atomically.

use chrono::{DateTime, NaiveDate, Utc};
use rustshare_core::events::{AggregateType, Event, EventType};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionState {
    Open,
    Completed,
}

impl ActionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Completed => "completed",
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MeetingAction {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub meeting_id: Uuid,
    pub calendar_event_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub assignee_id: Option<Uuid>,
    pub due_date: Option<NaiveDate>,
    pub state: String,
    pub related_decision_id: Option<Uuid>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
    pub completed_at: Option<DateTime<Utc>>,
    pub completed_by: Option<Uuid>,
}

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("Action unavailable")]
    Unavailable,
    #[error("Action changed; reload before retrying")]
    Conflict,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub async fn get(
    pool: &PgPool,
    tenant: Uuid,
    meeting: Uuid,
    id: Uuid,
) -> Result<MeetingAction, ActionError> {
    sqlx::query_as("SELECT * FROM meeting_actions WHERE tenant_id=$1 AND meeting_id=$2 AND id=$3")
        .bind(tenant)
        .bind(meeting)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(ActionError::Unavailable)
}

pub async fn list(
    pool: &PgPool,
    tenant: Uuid,
    meeting: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<MeetingAction>, i64), ActionError> {
    let rows = sqlx::query_as("SELECT * FROM meeting_actions WHERE tenant_id=$1 AND meeting_id=$2 ORDER BY created_at,id LIMIT $3 OFFSET $4")
        .bind(tenant).bind(meeting).bind(limit.clamp(1,100)).bind(offset.max(0)).fetch_all(pool).await?;
    let total = sqlx::query_scalar(
        "SELECT count(*) FROM meeting_actions WHERE tenant_id=$1 AND meeting_id=$2",
    )
    .bind(tenant)
    .bind(meeting)
    .fetch_one(pool)
    .await?;
    Ok((rows, total))
}

pub struct Command<'a> {
    pub tenant: Uuid,
    pub actor: Uuid,
    pub request: Uuid,
    pub action_id: Uuid,
    pub hash: &'a str,
}

/// Recover a committed result before revalidating obsolete write references.
pub async fn committed_command(
    pool: &PgPool,
    tenant: Uuid,
    actor: Uuid,
    request: Uuid,
    expected_hash: &str,
) -> Result<Option<Uuid>, ActionError> {
    let receipt: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT action_id,payload_hash FROM meeting_action_commands WHERE tenant_id=$1 AND actor_id=$2 AND request_id=$3",
    )
    .bind(tenant)
    .bind(actor)
    .bind(request)
    .fetch_optional(pool)
    .await?;
    match receipt {
        Some((_, hash)) if hash != expected_hash => Err(ActionError::Conflict),
        Some((action, _)) => Ok(Some(action)),
        None => Ok(None),
    }
}

/// Concurrent retries wait on the same unique command key, then reuse its result.
pub async fn reserve(
    tx: &mut Transaction<'static, Postgres>,
    command: &Command<'_>,
) -> Result<(Uuid, bool), ActionError> {
    let inserted = sqlx::query("INSERT INTO meeting_action_commands (tenant_id,actor_id,request_id,action_id,payload_hash) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (tenant_id,actor_id,request_id) DO NOTHING")
        .bind(command.tenant).bind(command.actor).bind(command.request).bind(command.action_id).bind(command.hash).execute(&mut **tx).await?.rows_affected()==1;
    let (id, hash): (Uuid,String) = sqlx::query_as("SELECT action_id,payload_hash FROM meeting_action_commands WHERE tenant_id=$1 AND actor_id=$2 AND request_id=$3")
        .bind(command.tenant).bind(command.actor).bind(command.request).fetch_one(&mut **tx).await?;
    if hash != command.hash {
        return Err(ActionError::Conflict);
    }
    Ok((id, inserted))
}

pub async fn insert(
    tx: &mut Transaction<'static, Postgres>,
    row: &MeetingAction,
) -> Result<(), ActionError> {
    sqlx::query("INSERT INTO meeting_actions (id,tenant_id,meeting_id,calendar_event_id,title,description,assignee_id,due_date,state,related_decision_id,version,created_at,updated_at,created_by,updated_by,completed_at,completed_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)")
        .bind(row.id).bind(row.tenant_id).bind(row.meeting_id).bind(row.calendar_event_id)
        .bind(&row.title).bind(&row.description).bind(row.assignee_id).bind(row.due_date).bind(&row.state).bind(row.related_decision_id)
        .bind(row.version).bind(row.created_at).bind(row.updated_at).bind(row.created_by).bind(row.updated_by).bind(row.completed_at).bind(row.completed_by)
        .execute(&mut **tx).await?;
    Ok(())
}

pub async fn lock(
    tx: &mut Transaction<'static, Postgres>,
    tenant: Uuid,
    meeting: Uuid,
    id: Uuid,
) -> Result<MeetingAction, ActionError> {
    sqlx::query_as(
        "SELECT * FROM meeting_actions WHERE tenant_id=$1 AND meeting_id=$2 AND id=$3 FOR UPDATE",
    )
    .bind(tenant)
    .bind(meeting)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(ActionError::Unavailable)
}

pub async fn update(
    tx: &mut Transaction<'static, Postgres>,
    row: &MeetingAction,
    expected: i64,
) -> Result<(), ActionError> {
    let affected = sqlx::query("UPDATE meeting_actions SET title=$5,description=$6,assignee_id=$7,due_date=$8,state=$9,related_decision_id=$10,version=$11,updated_at=$12,updated_by=$13,completed_at=$14,completed_by=$15 WHERE tenant_id=$1 AND meeting_id=$2 AND id=$3 AND version=$4")
        .bind(row.tenant_id).bind(row.meeting_id).bind(row.id).bind(expected).bind(&row.title).bind(&row.description).bind(row.assignee_id).bind(row.due_date).bind(&row.state).bind(row.related_decision_id).bind(row.version).bind(row.updated_at).bind(row.updated_by).bind(row.completed_at).bind(row.completed_by)
        .execute(&mut **tx).await?.rows_affected();
    if affected != 1 {
        return Err(ActionError::Conflict);
    }
    Ok(())
}

pub fn event(row: &MeetingAction, operation: &str) -> Event {
    Event::new(
        EventType::MeetingActionModified,
        row.meeting_id,
        AggregateType::Folder,
        serde_json::json!({"tenant_id":row.tenant_id,"meeting_id":row.meeting_id,"action_id":row.id,"version":row.version,"operation":operation}),
        row.updated_by,
    )
}
