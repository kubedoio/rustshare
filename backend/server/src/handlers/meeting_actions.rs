//! Actions are owned by Meetings; assignees and Decision references grant no access.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{AppError, AuthenticatedUser, PaginationQuery};
use crate::{
    services::meeting_actions::{self as store, ActionError, ActionState, MeetingAction},
    AppState,
};

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateMeetingActionRequest {
    pub request_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub assignee_id: Option<Uuid>,
    /// Calendar day (YYYY-MM-DD), never converted between timezones.
    pub due_date: Option<NaiveDate>,
    pub related_decision_id: Option<Uuid>,
}

fn optional_reference<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<Uuid>>, D::Error> {
    Option::<Uuid>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateMeetingActionRequest {
    pub request_id: Uuid,
    pub expected_version: i64,
    pub title: String,
    pub description: Option<String>,
    pub assignee_id: Option<Uuid>,
    pub due_date: Option<NaiveDate>,
    pub state: ActionState,
    /// Omit to preserve historical provenance; null clears; UUID sets after authorization.
    #[serde(
        default,
        deserialize_with = "optional_reference",
        skip_serializing_if = "Option::is_none"
    )]
    pub related_decision_id: Option<Option<Uuid>>,
}

#[derive(Debug, Serialize, utoipa::ToSchema, sqlx::FromRow)]
pub struct ActionAssignee {
    pub id: Uuid,
    pub display_name: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ActionDecision {
    pub id: Uuid,
    pub title: String,
    pub open_url: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeetingActionView {
    pub id: Uuid,
    pub meeting_id: Uuid,
    pub calendar_event_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub assignee_id: Option<Uuid>,
    pub assignee: Option<ActionAssignee>,
    pub due_date: Option<NaiveDate>,
    pub state: String,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by: Uuid,
    pub updated_by: Uuid,
    pub completed_at: Option<DateTime<Utc>>,
    pub completed_by: Option<Uuid>,
    pub related_decision_available: bool,
    pub related_decision: Option<ActionDecision>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeetingActionPage {
    pub items: Vec<MeetingActionView>,
    pub total: i64,
    pub page: u32,
    pub per_page: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ActionAssigneePage {
    pub items: Vec<ActionAssignee>,
    pub total: i64,
    pub page: u32,
    pub per_page: i64,
}

fn action_error(error: ActionError) -> AppError {
    match error {
        ActionError::Unavailable => AppError::not_found("Action unavailable"),
        ActionError::Conflict => {
            AppError::conflict("Action changed or retry key conflicts; reload before retrying")
        }
        error => {
            tracing::error!(%error,"Meeting action operation failed");
            AppError::internal("Action operation failed")
        }
    }
}

async fn meeting(
    state: &AppState,
    auth: &AuthenticatedUser,
    id: Uuid,
    edit: bool,
) -> Result<(), AppError> {
    if edit {
        // Reuse the existing owner evaluator, concealing mutation denials as well as reads.
        super::meetings::require_meeting_edit(state, auth, id)
            .await
            .map_err(|error| match error {
                AppError::Forbidden(_) => AppError::not_found("Meeting unavailable"),
                error => error,
            })?;
    } else {
        state
            .meeting_service
            .get_meeting(id, auth.user_id, auth.tenant_id)
            .await
            .map_err(super::meetings::meeting_read_error)?;
    }
    Ok(())
}

async fn validate_assignee(
    state: &AppState,
    auth: &AuthenticatedUser,
    assignee: Option<Uuid>,
) -> Result<(), AppError> {
    if let Some(id) = assignee {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id=$1 AND tenant_id=$2 AND disabled_at IS NULL)")
            .bind(id).bind(auth.tenant_id).fetch_one(&state.db_pool).await.map_err(|_|AppError::internal("Assignment validation failed"))?;
        if !exists {
            return Err(AppError::bad_request("Assignee unavailable"));
        }
    }
    Ok(())
}

fn validate_draft(request: Uuid, title: &str, description: Option<&str>) -> Result<(), AppError> {
    if request.is_nil()
        || title.trim().is_empty()
        || title.chars().count() > 255
        || title.contains('\0')
        || description.is_some_and(|value| value.len() > 16 * 1024 || value.contains('\0'))
    {
        return Err(AppError::bad_request("Invalid action draft"));
    }
    Ok(())
}

async fn view(
    state: &AppState,
    auth: &AuthenticatedUser,
    row: MeetingAction,
) -> Result<MeetingActionView, AppError> {
    let assignee = if let Some(id) = row.assignee_id {
        sqlx::query_as("SELECT id,COALESCE(NULLIF(display_name,''),username) AS display_name FROM users WHERE id=$1 AND tenant_id=$2 AND disabled_at IS NULL")
            .bind(id).bind(auth.tenant_id).fetch_optional(&state.db_pool).await.map_err(|_|AppError::internal("Assignment lookup failed"))?
    } else {
        None
    };
    let related_decision = if let Some(id) = row.related_decision_id {
        match super::meeting_decisions::authorize_decision(state, auth, id).await {
            Ok(decision) => Some(ActionDecision {
                id,
                title: decision.metadata.title,
                open_url: format!("/apps/decisions/{id}"),
            }),
            Err(AppError::NotFound(_) | AppError::Forbidden(_)) => None,
            Err(error) => return Err(error),
        }
    } else {
        None
    };
    Ok(MeetingActionView {
        id: row.id,
        meeting_id: row.meeting_id,
        calendar_event_id: row.calendar_event_id,
        title: row.title,
        description: row.description,
        assignee_id: row.assignee_id,
        assignee,
        due_date: row.due_date,
        state: row.state,
        version: row.version,
        created_at: row.created_at,
        updated_at: row.updated_at,
        created_by: row.created_by,
        updated_by: row.updated_by,
        completed_at: row.completed_at,
        completed_by: row.completed_by,
        related_decision_available: related_decision.is_some(),
        related_decision,
    })
}

fn hash(value: impl Serialize) -> Result<String, AppError> {
    let bytes =
        serde_json::to_vec(&value).map_err(|_| AppError::bad_request("Invalid action draft"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

#[utoipa::path(get,path="/api/v1/meetings/{id}/actions",tag="Meetings",params(("id"=Uuid,Path),("page"=Option<u32>,Query),("per_page"=Option<u32>,Query)),responses((status=200,body=MeetingActionPage),(status=404,description="Unavailable")))]
pub async fn list_meeting_actions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Query(page): Query<PaginationQuery>,
) -> Result<Json<MeetingActionPage>, AppError> {
    meeting(&state, &auth, id, false).await?;
    let (rows, total) = store::list(
        &state.db_pool,
        auth.tenant_id,
        id,
        page.limit(),
        page.offset(),
    )
    .await
    .map_err(action_error)?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(view(&state, &auth, row).await?);
    }
    Ok(Json(MeetingActionPage {
        items,
        total,
        page: page.page.max(1),
        per_page: page.limit(),
    }))
}

#[utoipa::path(get,path="/api/v1/meetings/{id}/actions/{action_id}",tag="Meetings",params(("id"=Uuid,Path),("action_id"=Uuid,Path)),responses((status=200,body=MeetingActionView),(status=404,description="Unavailable")))]
pub async fn get_meeting_action(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((id, action_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<MeetingActionView>, AppError> {
    meeting(&state, &auth, id, false).await?;
    Ok(Json(
        view(
            &state,
            &auth,
            store::get(&state.db_pool, auth.tenant_id, id, action_id)
                .await
                .map_err(action_error)?,
        )
        .await?,
    ))
}

#[utoipa::path(get,path="/api/v1/meetings/{id}/action-assignees",tag="Meetings",params(("id"=Uuid,Path),("page"=Option<u32>,Query),("per_page"=Option<u32>,Query)),responses((status=200,body=ActionAssigneePage),(status=404,description="Unavailable")))]
pub async fn action_assignees(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Query(page): Query<PaginationQuery>,
) -> Result<Json<ActionAssigneePage>, AppError> {
    meeting(&state, &auth, id, true).await?;
    let items=sqlx::query_as("SELECT id,COALESCE(NULLIF(display_name,''),username) AS display_name FROM users WHERE tenant_id=$1 AND disabled_at IS NULL ORDER BY id LIMIT $2 OFFSET $3")
        .bind(auth.tenant_id).bind(page.limit()).bind(page.offset()).fetch_all(&state.db_pool).await.map_err(|_|AppError::internal("Assignee list unavailable"))?;
    let total =
        sqlx::query_scalar("SELECT count(*) FROM users WHERE tenant_id=$1 AND disabled_at IS NULL")
            .bind(auth.tenant_id)
            .fetch_one(&state.db_pool)
            .await
            .map_err(|_| AppError::internal("Assignee list unavailable"))?;
    Ok(Json(ActionAssigneePage {
        items,
        total,
        page: page.page.max(1),
        per_page: page.limit(),
    }))
}

#[utoipa::path(post,path="/api/v1/meetings/{id}/actions",tag="Meetings",params(("id"=Uuid,Path)),request_body=CreateMeetingActionRequest,responses((status=200,body=MeetingActionView),(status=400,description="Invalid draft or assignee"),(status=404,description="Unavailable"),(status=409,description="Retry conflict")))]
pub async fn create_meeting_action(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(request): Json<CreateMeetingActionRequest>,
) -> Result<Json<MeetingActionView>, AppError> {
    meeting(&state, &auth, id, true).await?;
    validate_draft(
        request.request_id,
        &request.title,
        request.description.as_deref(),
    )?;
    let payload_hash = hash(("create", id, &request))?;
    if let Some(action_id) = store::committed_command(
        &state.db_pool,
        auth.tenant_id,
        auth.user_id,
        request.request_id,
        &payload_hash,
    )
    .await
    .map_err(action_error)?
    {
        let row = store::get(&state.db_pool, auth.tenant_id, id, action_id)
            .await
            .map_err(action_error)?;
        return Ok(Json(view(&state, &auth, row).await?));
    }
    validate_assignee(&state, &auth, request.assignee_id).await?;
    if let Some(decision) = request.related_decision_id {
        super::meeting_decisions::authorize_decision(&state, &auth, decision).await?;
    }
    // Canonical relation is authoritative; copied sidecar provenance cannot create event authority.
    let event_id=sqlx::query_scalar("SELECT event_id FROM calendar_event_meeting_links WHERE tenant_id=$1 AND meeting_folder_id=$2 AND state='ready'")
        .bind(auth.tenant_id).bind(id).fetch_optional(&state.db_pool).await.map_err(|_|AppError::internal("Meeting provenance unavailable"))?;
    let mut tx = state
        .event_store
        .begin_transaction()
        .await
        .map_err(|_| AppError::internal("Action operation failed"))?;
    let (action_id, inserted) = store::reserve(
        &mut tx,
        &store::Command {
            tenant: auth.tenant_id,
            actor: auth.user_id,
            request: request.request_id,
            action_id: Uuid::new_v4(),
            hash: &payload_hash,
        },
    )
    .await
    .map_err(action_error)?;
    let now = Utc::now();
    let row = MeetingAction {
        id: action_id,
        tenant_id: auth.tenant_id,
        meeting_id: id,
        calendar_event_id: event_id,
        title: request.title,
        description: request.description,
        assignee_id: request.assignee_id,
        due_date: request.due_date,
        state: "open".into(),
        related_decision_id: request.related_decision_id,
        version: 1,
        created_at: now,
        updated_at: now,
        created_by: auth.user_id,
        updated_by: auth.user_id,
        completed_at: None,
        completed_by: None,
    };
    let event = store::event(&row, "created");
    if inserted {
        store::insert(&mut tx, &row).await.map_err(action_error)?;
        state
            .event_store
            .append_in_tx(&mut tx, &event)
            .await
            .map_err(|_| AppError::internal("Action operation failed"))?;
    }
    tx.commit()
        .await
        .map_err(|_| AppError::internal("Action operation failed"))?;
    if inserted {
        state.broadcaster.publish(event);
    }
    let row = store::get(&state.db_pool, auth.tenant_id, id, action_id)
        .await
        .map_err(action_error)?;
    Ok(Json(view(&state, &auth, row).await?))
}

#[utoipa::path(put,path="/api/v1/meetings/{id}/actions/{action_id}",tag="Meetings",params(("id"=Uuid,Path),("action_id"=Uuid,Path)),request_body=UpdateMeetingActionRequest,responses((status=200,body=MeetingActionView),(status=400,description="Invalid draft or assignee"),(status=404,description="Unavailable"),(status=409,description="Version or retry conflict")))]
pub async fn update_meeting_action(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((id, action_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateMeetingActionRequest>,
) -> Result<Json<MeetingActionView>, AppError> {
    meeting(&state, &auth, id, true).await?;
    validate_draft(
        request.request_id,
        &request.title,
        request.description.as_deref(),
    )?;
    if request.expected_version < 1 {
        return Err(AppError::bad_request("Invalid action version"));
    }
    let payload_hash = hash(("update", id, action_id, &request))?;
    if let Some(receipt_action) = store::committed_command(
        &state.db_pool,
        auth.tenant_id,
        auth.user_id,
        request.request_id,
        &payload_hash,
    )
    .await
    .map_err(action_error)?
    {
        if receipt_action != action_id {
            return Err(action_error(ActionError::Conflict));
        }
        let row = store::get(&state.db_pool, auth.tenant_id, id, action_id)
            .await
            .map_err(action_error)?;
        return Ok(Json(view(&state, &auth, row).await?));
    }
    let old = store::get(&state.db_pool, auth.tenant_id, id, action_id)
        .await
        .map_err(action_error)?;
    if old.assignee_id != request.assignee_id {
        validate_assignee(&state, &auth, request.assignee_id).await?;
    }
    if let Some(Some(decision)) = request.related_decision_id {
        super::meeting_decisions::authorize_decision(&state, &auth, decision).await?;
    }
    let mut tx = state
        .event_store
        .begin_transaction()
        .await
        .map_err(|_| AppError::internal("Action operation failed"))?;
    let (command_action, inserted) = store::reserve(
        &mut tx,
        &store::Command {
            tenant: auth.tenant_id,
            actor: auth.user_id,
            request: request.request_id,
            action_id,
            hash: &payload_hash,
        },
    )
    .await
    .map_err(action_error)?;
    if command_action != action_id {
        return Err(AppError::conflict("Retry key conflicts"));
    }
    let mut event = None;
    if inserted {
        let mut row = store::lock(&mut tx, auth.tenant_id, id, action_id)
            .await
            .map_err(action_error)?;
        if row.version != request.expected_version {
            return Err(action_error(ActionError::Conflict));
        }
        let now = Utc::now();
        let new_state = request.state.as_str();
        let operation = match (row.state.as_str(), new_state) {
            ("open", "completed") => "completed",
            ("completed", "open") => "reopened",
            _ => "updated",
        };
        if new_state == "completed" && row.state != "completed" {
            row.completed_at = Some(now);
            row.completed_by = Some(auth.user_id);
        }
        if new_state == "open" {
            row.completed_at = None;
            row.completed_by = None;
        }
        row.title = request.title;
        row.description = request.description;
        row.assignee_id = request.assignee_id;
        row.due_date = request.due_date;
        row.state = new_state.into();
        if let Some(reference) = request.related_decision_id {
            row.related_decision_id = reference;
        }
        row.version = row
            .version
            .checked_add(1)
            .ok_or_else(|| AppError::conflict("Action version limit reached"))?;
        row.updated_at = now;
        row.updated_by = auth.user_id;
        store::update(&mut tx, &row, request.expected_version)
            .await
            .map_err(action_error)?;
        let changed = store::event(&row, operation);
        state
            .event_store
            .append_in_tx(&mut tx, &changed)
            .await
            .map_err(|_| AppError::internal("Action operation failed"))?;
        event = Some(changed);
    }
    tx.commit()
        .await
        .map_err(|_| AppError::internal("Action operation failed"))?;
    if let Some(event) = event {
        state.broadcaster.publish(event);
    }
    Ok(Json(
        view(
            &state,
            &auth,
            store::get(&state.db_pool, auth.tenant_id, id, action_id)
                .await
                .map_err(action_error)?,
        )
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_reference_update_distinguishes_omitted_null_and_id() {
        let body = serde_json::json!({"request_id":Uuid::new_v4(),"expected_version":1,"title":"Action","state":"open"});
        let omitted: UpdateMeetingActionRequest = serde_json::from_value(body.clone()).unwrap();
        assert_eq!(omitted.related_decision_id, None);
        let mut body = body;
        body["related_decision_id"] = serde_json::Value::Null;
        let cleared: UpdateMeetingActionRequest = serde_json::from_value(body.clone()).unwrap();
        assert_eq!(cleared.related_decision_id, Some(None));
        let id = Uuid::new_v4();
        body["related_decision_id"] = serde_json::json!(id);
        let linked: UpdateMeetingActionRequest = serde_json::from_value(body).unwrap();
        assert_eq!(linked.related_decision_id, Some(Some(id)));
        assert_ne!(hash(&omitted).unwrap(), hash(&cleared).unwrap());
    }

    #[test]
    fn due_date_is_a_calendar_day_and_draft_is_bounded() {
        let body = serde_json::json!({"request_id":Uuid::new_v4(),"title":"Action","due_date":"2026-10-25"});
        let request: CreateMeetingActionRequest = serde_json::from_value(body).unwrap();
        assert_eq!(request.due_date.unwrap().to_string(), "2026-10-25");
        assert!(validate_draft(Uuid::new_v4(), " ", None).is_err());
        assert!(validate_draft(Uuid::nil(), "Action", None).is_err());
        assert!(
            validate_draft(Uuid::new_v4(), "Action", Some(&"a".repeat(16 * 1024 + 1))).is_err()
        );
    }
}
