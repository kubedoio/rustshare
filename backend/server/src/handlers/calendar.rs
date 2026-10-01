use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::Query;
use chrono::{DateTime, Utc};
use rustshare_core::domain::CalendarSourceKind;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::handlers::{AppError, AuthenticatedUser, ValidatedJson};
use crate::services::application_service::ApplicationError;
use crate::services::calendar_service::{
    CalendarError, CalendarEventOccurrence, CalendarEventPatch, NewCalendarEvent,
};
use crate::state::AppState;

const CALENDAR_APPLICATION_ID: &str = "io.elembra.calendar";

async fn require_calendar_enabled(state: &AppState, tenant_id: Uuid) -> Result<(), AppError> {
    let module = state
        .application_service
        .get_application(CALENDAR_APPLICATION_ID, tenant_id)
        .await;
    let module = match module {
        Ok(module) => module,
        Err(ApplicationError::NotFound(_)) => {
            return Err(AppError::forbidden("Calendar module is disabled"));
        }
        Err(err) => return Err(AppError::internal(err.to_string())),
    };

    if !module.enabled {
        return Err(AppError::forbidden("Calendar module is disabled"));
    }

    Ok(())
}

#[derive(Debug, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct CreateCalendarEventRequest {
    #[validate(length(min = 1, max = 512))]
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    #[serde(default)]
    pub all_day: bool,
    pub timezone: String,
    pub rrule: Option<String>,
}

#[derive(Debug, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct UpdateCalendarEventRequest {
    #[validate(length(min = 1, max = 512))]
    pub title: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: Option<bool>,
    pub timezone: Option<String>,
    pub rrule: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CalendarEventResponse {
    pub id: Uuid,
    pub source_id: Uuid,
    /// Null only when the owning source row is somehow missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub all_day: bool,
    pub original_date: Option<chrono::NaiveDate>,
    pub timezone: String,
    pub rrule: Option<String>,
    pub recurrence_id: Option<String>,
    /// Expanded occurrence start (RFC 3339); null on stored non-expanded rows.
    pub instance_start: Option<DateTime<Utc>>,
    pub status: String,
    pub read_only: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CalendarEventListResponse {
    pub events: Vec<CalendarEventResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CalendarSourceResponse {
    pub id: Uuid,
    pub kind: String,
    pub display_name: String,
    pub external_account: Option<String>,
    pub external_calendar_id: Option<String>,
    pub is_enabled: bool,
    pub status: String,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CalendarSourceListResponse {
    pub sources: Vec<CalendarSourceResponse>,
}

#[derive(Debug, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct CreateCalendarSourceRequest {
    pub kind: CalendarSourceKind,
    #[validate(length(min = 1, max = 255))]
    pub display_name: String,
}

#[derive(Debug, Deserialize, validator::Validate, utoipa::ToSchema)]
pub struct UpdateCalendarSourceRequest {
    #[validate(length(min = 1, max = 255))]
    pub display_name: Option<String>,
    pub is_enabled: Option<bool>,
}

fn occurrence_to_response(occurrence: CalendarEventOccurrence) -> CalendarEventResponse {
    let event = occurrence.event;
    CalendarEventResponse {
        id: event.id,
        source_id: event.source_id,
        source_kind: Some(occurrence.source_kind.as_str().to_string()),
        title: event.title,
        description: event.description,
        location: event.location,
        starts_at: event.starts_at,
        ends_at: event.ends_at,
        all_day: event.all_day,
        original_date: event.original_date,
        timezone: event.timezone,
        rrule: event.rrule,
        recurrence_id: event.recurrence_id,
        instance_start: occurrence.instance_start,
        status: event.status,
        read_only: event.read_only,
        created_at: event.created_at,
        updated_at: event.updated_at,
    }
}

fn source_to_response(source: rustshare_core::domain::CalendarSource) -> CalendarSourceResponse {
    CalendarSourceResponse {
        id: source.id,
        kind: source.kind,
        display_name: source.display_name,
        external_account: source.external_account,
        external_calendar_id: source.external_calendar_id,
        is_enabled: source.is_enabled,
        status: source.status,
        last_synced_at: source.last_synced_at,
        last_error: source.last_error,
        created_at: source.created_at,
    }
}

/// Query parameters for `GET /api/v1/calendar/events`.
#[derive(Debug, Deserialize)]
pub struct ListCalendarEventsQuery {
    /// Inclusive range start (RFC 3339).
    pub from: DateTime<Utc>,
    /// Exclusive range end (RFC 3339); the window must be at most 366 days.
    pub to: DateTime<Utc>,
    /// Repeatable; restricts the list to the given sources (disabled sources
    /// are included when explicitly requested).
    #[serde(default)]
    pub source_id: Vec<Uuid>,
    #[serde(default)]
    pub include_cancelled: bool,
}

/// `GET /api/v1/calendar/events` — list events overlapping `[from, to)`.
///
/// `from`/`to` are required RFC 3339 timestamps and the window must be at
/// most 366 days (enforced by the service, which rejects larger windows with
/// a 400). `source_id` is repeatable and restricts to those sources;
/// `include_cancelled` defaults to false.
pub async fn list_calendar_events(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ListCalendarEventsQuery>,
) -> Result<Json<CalendarEventListResponse>, AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;

    let occurrences = state
        .calendar_service
        .list_events(
            auth.tenant_id,
            auth.user_id,
            query.from,
            query.to,
            &query.source_id,
            query.include_cancelled,
        )
        .await?;

    Ok(Json(CalendarEventListResponse {
        events: occurrences
            .into_iter()
            .map(occurrence_to_response)
            .collect(),
    }))
}

/// `POST /api/v1/calendar/events` — create an internal event (the internal
/// source is lazily created on first use).
pub async fn create_calendar_event(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    ValidatedJson(req): ValidatedJson<CreateCalendarEventRequest>,
) -> Result<(StatusCode, Json<CalendarEventResponse>), AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    let event = state
        .calendar_service
        .create_event(
            auth.tenant_id,
            auth.user_id,
            NewCalendarEvent {
                title: req.title,
                description: req.description,
                location: req.location,
                starts_at: req.starts_at,
                ends_at: req.ends_at,
                all_day: req.all_day,
                timezone: req.timezone,
                rrule: req.rrule,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(occurrence_to_response(CalendarEventOccurrence {
            source_kind: CalendarSourceKind::Internal,
            instance_start: None,
            event,
        })),
    ))
}

/// `GET /api/v1/calendar/events/{id}` — single event.
pub async fn get_calendar_event(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(event_id): Path<Uuid>,
) -> Result<Json<CalendarEventResponse>, AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    let event = state
        .calendar_service
        .get_event(auth.tenant_id, auth.user_id, event_id)
        .await?;
    // The source row is the authority for `source_kind`; omit the field
    // rather than guess when it is missing.
    let source_kind = state
        .calendar_service
        .get_source(auth.tenant_id, auth.user_id, event.source_id)
        .await?
        .and_then(|source| source.kind.parse::<CalendarSourceKind>().ok())
        .map(|kind| kind.as_str().to_string());

    let mut response = occurrence_to_response(CalendarEventOccurrence {
        source_kind: CalendarSourceKind::Internal,
        instance_start: None,
        event,
    });
    response.source_kind = source_kind;

    Ok(Json(response))
}

/// `PATCH /api/v1/calendar/events/{id}` — partial update of an internal
/// event; mirrored read-only events are rejected with 409.
pub async fn update_calendar_event(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(event_id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateCalendarEventRequest>,
) -> Result<Json<CalendarEventResponse>, AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    let event = state
        .calendar_service
        .update_event(
            auth.tenant_id,
            auth.user_id,
            event_id,
            CalendarEventPatch {
                title: req.title,
                description: req.description,
                location: req.location,
                starts_at: req.starts_at,
                ends_at: req.ends_at,
                all_day: req.all_day,
                timezone: req.timezone,
                rrule: req.rrule,
            },
        )
        .await?;

    Ok(Json(occurrence_to_response(CalendarEventOccurrence {
        source_kind: CalendarSourceKind::Internal,
        instance_start: None,
        event,
    })))
}

/// `DELETE /api/v1/calendar/events/{id}` — soft-delete an internal event
/// (idempotent; mirrored read-only events are rejected with 409).
pub async fn delete_calendar_event(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(event_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    state
        .calendar_service
        .delete_event(auth.tenant_id, auth.user_id, event_id)
        .await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `GET /api/v1/calendar/sources` — list the caller's calendar sources.
pub async fn list_calendar_sources(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<CalendarSourceListResponse>, AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    let sources = state
        .calendar_service
        .list_sources(auth.tenant_id, auth.user_id)
        .await?;

    Ok(Json(CalendarSourceListResponse {
        sources: sources.into_iter().map(source_to_response).collect(),
    }))
}

/// `POST /api/v1/calendar/sources` — create an `ical_import` source, or
/// no-op back to the existing row for `kind: "internal"`.
pub async fn create_calendar_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    ValidatedJson(req): ValidatedJson<CreateCalendarSourceRequest>,
) -> Result<(StatusCode, Json<CalendarSourceResponse>), AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    let source = state
        .calendar_service
        .create_source(auth.tenant_id, auth.user_id, req.kind, req.display_name)
        .await?;

    let status = if req.kind == CalendarSourceKind::Internal {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, Json(source_to_response(source))))
}

/// `PATCH /api/v1/calendar/sources/{id}` — rename or enable/disable a source.
pub async fn update_calendar_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(source_id): Path<Uuid>,
    ValidatedJson(req): ValidatedJson<UpdateCalendarSourceRequest>,
) -> Result<Json<CalendarSourceResponse>, AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    let source = state
        .calendar_service
        .update_source(
            auth.tenant_id,
            auth.user_id,
            source_id,
            req.display_name,
            req.is_enabled,
        )
        .await?;

    Ok(Json(source_to_response(source)))
}

/// `DELETE /api/v1/calendar/sources/{id}` — soft-delete a source, destroy its
/// stored tokens, and soft-delete its mirrored events. Internal sources
/// cannot be deleted (409).
pub async fn delete_calendar_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(source_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    state
        .calendar_service
        .delete_source(auth.tenant_id, auth.user_id, source_id)
        .await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

impl From<CalendarError> for AppError {
    fn from(err: CalendarError) -> Self {
        match err {
            CalendarError::NotFound(_) | CalendarError::SourceNotFound(_) => {
                AppError::NotFound(err.to_string())
            }
            CalendarError::ReadOnlyMirror
            | CalendarError::DuplicateSource
            | CalendarError::InternalSource => AppError::Conflict(err.to_string()),
            CalendarError::InvalidInput(_) => AppError::BadRequest(err.to_string()),
            CalendarError::Storage(_) | CalendarError::Database(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}
