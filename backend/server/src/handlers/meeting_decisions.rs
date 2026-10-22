//! Meeting context delegates each Decision to its owning Application.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use rustshare_core::events::{AggregateType, DecisionModifiedPayload, Event, EventType};
use rustshare_resource_auth::{Purpose, ResourceRef, SourceError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{AppError, AuthenticatedUser, PaginationQuery};
use crate::{
    services::{
        decision_service::{DecisionError, DecisionSummary},
        meeting_decisions as store,
        meeting_service::{MeetingError, MeetingResourceLink},
    },
    AppState,
};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateMeetingDecisionRequest {
    pub request_id: Uuid,
    pub title: String,
    pub category: String,
    pub content: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LinkMeetingDecisionRequest {
    pub decision_id: Uuid,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeetingDecisionSummary {
    pub id: Uuid,
    pub name: String,
    pub title: String,
    pub status: String,
    pub decision_date: Option<DateTime<Utc>>,
    pub owner_id: Uuid,
    pub owner_display_name: Option<String>,
    pub open_url: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeetingDecisionLink {
    pub id: Uuid,
    pub available: bool,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<MeetingDecisionSummary>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeetingDecisionPage {
    pub items: Vec<MeetingDecisionLink>,
    pub total: i64,
    pub page: u32,
    pub per_page: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DecisionMeetingContext {
    pub id: Uuid,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar_event_id: Option<Uuid>,
    pub is_origin: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DecisionMeetingContexts {
    pub items: Vec<DecisionMeetingContext>,
    pub next_page: Option<u32>,
}

pub(crate) fn decision_read_error(error: DecisionError) -> AppError {
    match error {
        DecisionError::NotFound(_)
        | DecisionError::PermissionDenied
        | DecisionError::InvalidData(_) => AppError::not_found("Resource unavailable"),
        error => {
            tracing::error!(%error, "Decision access failed");
            AppError::internal("Resource access failed")
        }
    }
}

fn resource_uri(id: Uuid) -> String {
    format!("elembra://io.elembra.files/file/{id}")
}

pub(crate) async fn authorize_decision(
    state: &AppState,
    auth: &AuthenticatedUser,
    id: Uuid,
) -> Result<DecisionSummary, AppError> {
    let resource = ResourceRef::from_uri(&resource_uri(id))
        .map_err(|_| AppError::not_found("Resource unavailable"))?;
    let resolved = state
        .source_authorizer
        .resolve(
            &super::meetings::principal(auth),
            &resource,
            Purpose::UserOpen,
        )
        .await
        .map_err(|error| match error {
            SourceError::OwnerUnavailable => {
                AppError::service_unavailable("Resource owner unavailable")
            }
            SourceError::Internal(_) => AppError::internal("Resource access failed"),
            _ => AppError::not_found("Resource unavailable"),
        })?;
    if !resolved.available {
        return Err(AppError::not_found("Resource unavailable"));
    }
    state
        .decision_service
        .get_decision_summary(id, auth.user_id, auth.tenant_id)
        .await
        .map_err(decision_read_error)
}

async fn link_view(
    state: &AppState,
    auth: &AuthenticatedUser,
    link: MeetingResourceLink,
) -> Result<MeetingDecisionLink, AppError> {
    let mut view = MeetingDecisionLink {
        id: link.id,
        available: false,
        created_at: link.created_at,
        decision: None,
    };
    let id = ResourceRef::from_uri(&link.resource_ref_uri)
        .ok()
        .filter(|r| {
            r.application.0 == "io.elembra.files"
                && r.resource_type == "file"
                && r.version.is_none()
        })
        .and_then(|r| Uuid::parse_str(&r.resource_id).ok());
    let Some(id) = id else {
        return Ok(view);
    };
    match authorize_decision(state, auth, id).await {
        Ok(decision) => {
            let owner_display_name = sqlx::query_scalar::<_, String>("SELECT COALESCE(NULLIF(display_name,''),username) FROM users WHERE id=$1 AND tenant_id=$2")
                .bind(decision.owner_id).bind(auth.tenant_id).fetch_optional(&state.db_pool).await
                .map_err(|_|AppError::internal("Decision author unavailable"))?;
            view.available = true;
            view.decision = Some(MeetingDecisionSummary {
                id: decision.id,
                name: decision.name,
                title: decision.metadata.title,
                status: decision.metadata.status,
                decision_date: decision.metadata.decision_date,
                owner_id: decision.owner_id,
                owner_display_name,
                open_url: format!("/apps/decisions/{id}"),
            });
        }
        Err(AppError::NotFound(_) | AppError::Forbidden(_)) => {}
        Err(error) => return Err(error),
    }
    Ok(view)
}

#[utoipa::path(get,path="/api/v1/meetings/{id}/decisions",tag="Meetings",
    params(("id"=Uuid,Path),("page"=Option<u32>,Query),("per_page"=Option<u32>,Query)),
    responses((status=200,body=MeetingDecisionPage),(status=401,description="Unauthorized"),(status=404,description="Unavailable")))]
pub async fn list_meeting_decisions(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Query(page): Query<PaginationQuery>,
) -> Result<Json<MeetingDecisionPage>, AppError> {
    state
        .meeting_service
        .get_meeting(id, auth.user_id, auth.tenant_id)
        .await
        .map_err(super::meetings::meeting_read_error)?;
    let (links, total) = store::list_links(
        &state.db_pool,
        auth.tenant_id,
        id,
        page.limit(),
        page.offset(),
    )
    .await
    .map_err(|_| AppError::internal("Meeting links unavailable"))?;
    let mut items = Vec::with_capacity(links.len());
    for link in links {
        items.push(link_view(&state, &auth, link).await?);
    }
    Ok(Json(MeetingDecisionPage {
        items,
        total,
        page: page.page.max(1),
        per_page: page.limit(),
    }))
}

#[utoipa::path(post,path="/api/v1/meetings/{id}/decisions/links",tag="Meetings",params(("id"=Uuid,Path)),request_body=LinkMeetingDecisionRequest,
    responses((status=200,body=MeetingDecisionLink),(status=403,description="Meeting mutation denied"),(status=404,description="Resource unavailable"),(status=409,description="Resource already has a different meeting link")))]
pub async fn link_meeting_decision(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(request): Json<LinkMeetingDecisionRequest>,
) -> Result<Json<MeetingDecisionLink>, AppError> {
    super::meetings::require_meeting_edit(&state, &auth, id).await?;
    authorize_decision(&state, &auth, request.decision_id).await?;
    let link = store::link_decision(
        &state.db_pool,
        auth.tenant_id,
        id,
        &resource_uri(request.decision_id),
    )
    .await
    .map_err(|_| AppError::internal("Meeting link failed"))?
    .ok_or_else(|| AppError::conflict("Resource already has a different meeting link"))?;
    Ok(Json(link_view(&state, &auth, link).await?))
}

#[utoipa::path(post,path="/api/v1/meetings/{id}/decisions",tag="Meetings",params(("id"=Uuid,Path)),request_body=CreateMeetingDecisionRequest,
    responses((status=200,body=MeetingDecisionLink),(status=400,description="Invalid draft"),(status=403,description="Meeting mutation denied"),(status=409,description="Retry key conflict")))]
pub async fn create_meeting_decision(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(request): Json<CreateMeetingDecisionRequest>,
) -> Result<Json<MeetingDecisionLink>, AppError> {
    super::meetings::require_meeting_edit(&state, &auth, id).await?;
    if request.request_id.is_nil()
        || request.title.trim().is_empty()
        || request.title.chars().count() > 255
        || request.category.trim().is_empty()
        || request.category.chars().count() > 128
        || request.category.contains(['/', '\0'])
        || request.content.len() > 1024 * 1024
    {
        return Err(AppError::bad_request("Invalid decision draft"));
    }
    let payload = serde_json::to_vec(&(id, &request.title, &request.category, &request.content))
        .map_err(|_| AppError::bad_request("Invalid draft"))?;
    let hash = hex::encode(Sha256::digest(payload));
    let command = store::reserve_creation(
        &state.db_pool,
        auth.tenant_id,
        auth.user_id,
        request.request_id,
        id,
        &hash,
    )
    .await
    .map_err(|_| AppError::internal("Decision reservation failed"))?;
    if command.meeting_folder_id != id || command.payload_hash != hash {
        return Err(AppError::conflict(
            "Retry key belongs to a different decision draft",
        ));
    }
    if !command.ready {
        state
            .decision_service
            .create_for_meeting_command(
                auth.user_id,
                auth.tenant_id,
                request.title,
                request.category,
                request.content,
                command.decision_file_id,
                command.sidecar_file_id,
                command.created_at,
            )
            .await?;
    }
    let decision = authorize_decision(&state, &auth, command.decision_file_id).await?;
    super::meetings::require_meeting_edit(&state, &auth, id).await?;
    let event = Event::new(
        EventType::DecisionModified,
        decision.id,
        AggregateType::File,
        serde_json::to_value(DecisionModifiedPayload {
            decision_id: decision.id.to_string(),
            title: decision.metadata.title,
            modified_by: auth.user_id,
        })
        .map_err(|_| AppError::internal("Decision event failed"))?,
        auth.user_id,
    );
    let link = store::finish_creation(
        &state.db_pool,
        &state.event_store,
        &state.broadcaster,
        auth.tenant_id,
        auth.user_id,
        request.request_id,
        &resource_uri(decision.id),
        &event,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error,"Decision association failed");
        AppError::internal("Decision association failed; retry the same draft")
    })?
    .ok_or_else(|| AppError::conflict("Decision resource already has a different meeting link"))?;
    Ok(Json(link_view(&state, &auth, link).await?))
}

#[utoipa::path(delete,path="/api/v1/meetings/{id}/decisions/{link_id}",tag="Meetings",params(("id"=Uuid,Path),("link_id"=Uuid,Path)),
    responses((status=204,description="Link removed; Decision preserved"),(status=403,description="Meeting mutation denied")))]
pub async fn unlink_meeting_decision(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path((id, link_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    super::meetings::require_meeting_edit(&state, &auth, id).await?;
    sqlx::query("DELETE FROM meeting_resource_links WHERE tenant_id=$1 AND meeting_folder_id=$2 AND id=$3 AND link_kind='decision'")
        .bind(auth.tenant_id).bind(id).bind(link_id).execute(&state.db_pool).await.map_err(|_|AppError::internal("Unlink failed"))?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get,path="/api/v1/decisions/{id}/meetings",tag="Decisions",params(("id"=Uuid,Path),("page"=Option<u32>,Query),("per_page"=Option<u32>,Query)),
    responses((status=200,body=DecisionMeetingContexts),(status=404,description="Resource unavailable")))]
pub async fn decision_meetings(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Query(page): Query<PaginationQuery>,
) -> Result<Json<DecisionMeetingContexts>, AppError> {
    authorize_decision(&state, &auth, id).await?;
    let mut raw_offset = 0;
    let mut skip = page.offset();
    let mut next_page = None;
    let mut items = Vec::new();
    // Pagination counts authorized contexts only; hidden associations cannot affect its markers.
    'scan: loop {
        let links = store::meeting_links(
            &state.db_pool,
            auth.tenant_id,
            &resource_uri(id),
            id,
            100,
            raw_offset,
        )
        .await
        .map_err(|_| AppError::internal("Meeting context unavailable"))?;
        let exhausted = links.len() < 100;
        raw_offset += links.len() as i64;
        for (meeting_id, is_origin) in links {
            let meeting = match state
                .meeting_service
                .get_meeting(meeting_id, auth.user_id, auth.tenant_id)
                .await
            {
                Ok(meeting) => meeting,
                Err(MeetingError::NotFound(_) | MeetingError::PermissionDenied) => continue,
                Err(_) => return Err(AppError::internal("Meeting context unavailable")),
            };
            if skip > 0 {
                skip -= 1;
                continue;
            }
            if items.len() == page.limit() as usize {
                next_page = page.page.max(1).checked_add(1);
                break 'scan;
            }
            let event_id:Option<Uuid>=sqlx::query_scalar("SELECT event_id FROM calendar_event_meeting_links WHERE tenant_id=$1 AND meeting_folder_id=$2 AND state='ready'")
            .bind(auth.tenant_id).bind(meeting_id).fetch_optional(&state.db_pool).await.map_err(|_|AppError::internal("Meeting context unavailable"))?;
            let calendar_event_id = if let Some(event_id) = event_id {
                match state
                    .calendar_service
                    .get_event(auth.tenant_id, auth.user_id, event_id)
                    .await
                {
                    Ok(_) => Some(event_id),
                    Err(crate::services::calendar_service::CalendarError::NotFound(_)) => None,
                    Err(_) => return Err(AppError::internal("Calendar context unavailable")),
                }
            } else {
                None
            };
            items.push(DecisionMeetingContext {
                id: meeting.id,
                title: meeting.metadata.title,
                calendar_event_id,
                is_origin,
            });
        }
        if exhausted {
            break;
        }
    }
    Ok(Json(DecisionMeetingContexts { items, next_page }))
}
