use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::Query;
use chrono::{DateTime, Utc};
use rustshare_core::domain::{CalendarImportJob, CalendarSource, CalendarSourceKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::handlers::{AppError, AuthenticatedUser, ValidatedJson};
use crate::services::application_service::ApplicationError;
use crate::services::calendar_service::{
    CalendarError, CalendarEventOccurrence, CalendarEventPatch, NewCalendarEvent,
};
use crate::state::AppState;

const CALENDAR_APPLICATION_ID: &str = "io.elembra.calendar";
/// Calendar .ics uploads are capped at 10 MB (spec §Import semantics).
const MAX_CALENDAR_IMPORT_SIZE_BYTES: usize = 10 * 1024 * 1024;
/// Hard byte cap for the multipart `source_id` text field. A UUID is 36 bytes;
/// the small allowance tolerates surrounding whitespace without letting an
/// authenticated caller force a large allocation (the router still permits a
/// multi-GB request body, so `Field::text`'s unbounded read is not safe here).
const MAX_CALENDAR_SOURCE_ID_FIELD_BYTES: usize = 128;

/// Read a small multipart text field with a hard byte cap, returning 413 when
/// the field exceeds it. The read is bounded chunk-by-chunk so an oversized
/// field is never buffered in full.
async fn read_bounded_field_text(
    field: &mut axum::extract::multipart::Field<'_>,
    max_bytes: usize,
) -> Result<String, AppError> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| AppError::bad_request(format!("Invalid source_id field: {e}")))?
    {
        if buf.len() + chunk.len() > max_bytes {
            return Err(AppError::payload_too_large("source_id field is too large"));
        }
        buf.extend_from_slice(&chunk);
    }
    String::from_utf8(buf).map_err(|_| AppError::bad_request("Invalid source_id: expected a UUID"))
}

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
    /// PATCH convention: an absent or `null` value leaves the stored value
    /// unchanged, while an empty string clears it.
    pub description: Option<String>,
    /// PATCH convention: absent/`null` leaves unchanged, empty string clears.
    pub location: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: Option<bool>,
    pub timezone: Option<String>,
    /// PATCH convention: absent/`null` leaves the recurrence unchanged, empty
    /// string removes it (stops the series).
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

fn source_to_response(source: CalendarSource) -> CalendarSourceResponse {
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

/// `POST /api/v1/calendar/import` — upload a `.ics` file (multipart `file`
/// field, optional `source_id` text field) and enqueue a background import
/// job. The upload is spooled to a temp file and the bytes persisted on the
/// job row for the worker.
pub async fn import_calendar_file(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<CalendarImportAcceptedResponse>), AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;

    let mut file_temp: Option<(
        tempfile::NamedTempFile,
        usize,
        Option<String>,
        Option<String>,
    )> = None;
    let mut source_id: Option<Uuid> = None;

    while let Some(mut field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Failed to read multipart field: {}", e);
        AppError::internal(format!("Failed to read multipart field: {e}"))
    })? {
        match field.name().unwrap_or("") {
            "file" => {
                if file_temp.is_some() {
                    return Err(AppError::bad_request(
                        "Duplicate file field: expected a single file",
                    ));
                }
                let filename = field.file_name().map(str::to_string);
                let content_type = field.content_type().map(str::to_string);
                let (temp, size) = super::stream_multipart_field_to_temp_file(
                    &mut field,
                    MAX_CALENDAR_IMPORT_SIZE_BYTES,
                )
                .await?;
                file_temp = Some((temp, size, filename, content_type));
            }
            "source_id" => {
                if source_id.is_some() {
                    return Err(AppError::bad_request(
                        "Duplicate source_id field: expected a single value",
                    ));
                }
                let raw =
                    read_bounded_field_text(&mut field, MAX_CALENDAR_SOURCE_ID_FIELD_BYTES).await?;
                source_id =
                    Some(Uuid::parse_str(raw.trim()).map_err(|_| {
                        AppError::bad_request("Invalid source_id: expected a UUID")
                    })?);
            }
            _ => {}
        }
    }

    let (file_temp, size_bytes, filename, content_type) =
        file_temp.ok_or_else(|| AppError::bad_request("Missing file data"))?;
    let filename = filename.unwrap_or_else(|| "import.ics".to_string());

    let content_type_ok = content_type
        .as_deref()
        .map(|value| value.starts_with("text/calendar"))
        .unwrap_or(false);
    if !content_type_ok && !filename.to_lowercase().ends_with(".ics") {
        return Err(AppError::bad_request(
            "File must be a .ics (text/calendar) file",
        ));
    }

    let bytes = tokio::fs::read(file_temp.path())
        .await
        .map_err(|e| AppError::internal(format!("Failed to read uploaded file: {e}")))?;

    // Resolve the target source: the given ical_import source, or an existing
    // one named after the file, or a newly created one.
    let source = match source_id {
        Some(id) => {
            let source = state
                .calendar_service
                .get_source(auth.tenant_id, auth.user_id, id)
                .await?
                .ok_or(CalendarError::SourceNotFound(id))?;
            if source.kind != CalendarSourceKind::IcalImport.as_str() {
                return Err(AppError::bad_request(
                    "source_id must reference an ical_import source",
                ));
            }
            source
        }
        None => {
            let display_name = filename.chars().take(255).collect::<String>();
            let existing = state
                .calendar_service
                .list_sources(auth.tenant_id, auth.user_id)
                .await?;
            match existing.into_iter().find(|source| {
                source.kind == CalendarSourceKind::IcalImport.as_str()
                    && source.display_name == display_name
            }) {
                Some(source) => source,
                None => {
                    state
                        .calendar_service
                        .create_source(
                            auth.tenant_id,
                            auth.user_id,
                            CalendarSourceKind::IcalImport,
                            display_name,
                        )
                        .await?
                }
            }
        }
    };

    let now = Utc::now();
    let job = CalendarImportJob {
        id: Uuid::new_v4(),
        tenant_id: auth.tenant_id,
        owner_id: auth.user_id,
        source_id: source.id,
        status: "pending".to_string(),
        filename,
        size_bytes: size_bytes as i64,
        total_events: 0,
        processed_events: 0,
        failed_events: 0,
        last_error: None,
        started_at: None,
        completed_at: None,
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };
    state
        .metadata_store
        .create_calendar_import_job(&job, &bytes)
        .await
        .map_err(|e| AppError::internal(format!("Failed to enqueue import job: {e}")))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(CalendarImportAcceptedResponse {
            job_id: job.id,
            source_id: job.source_id,
            status: job.status.clone(),
        }),
    ))
}

/// `202` body for `POST /api/v1/calendar/import`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CalendarImportAcceptedResponse {
    pub job_id: Uuid,
    pub source_id: Uuid,
    pub status: String,
}

/// `200` body for `GET /api/v1/calendar/sources/{kind}/connect`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CalendarConnectResponse {
    pub authorize_url: String,
}

/// `GET /api/v1/calendar/sources/{kind}/connect` — begin the OAuth connect
/// flow for `google` / `outlook` and return the provider consent URL.
/// `503` when the deployment lacks the client id/secret env config.
pub async fn connect_calendar_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(kind): Path<CalendarSourceKind>,
) -> Result<Json<CalendarConnectResponse>, AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    let authorize_url = state
        .calendar_service
        .begin_connect(auth.tenant_id, auth.user_id, kind)
        .await?;
    Ok(Json(CalendarConnectResponse { authorize_url }))
}

/// Query parameters of the provider redirect target.
#[derive(Debug, Deserialize)]
pub struct CalendarOauthCallbackQuery {
    pub state: Option<String>,
    pub code: Option<String>,
    pub error: Option<String>,
}

/// Build the contract-mandated `302` redirect. Axum's `Redirect` helpers
/// emit 303/307/308, so the status code and Location header are set
/// manually. The response never renders token data.
fn oauth_redirect(target: &str) -> axum::response::Response {
    axum::http::Response::builder()
        .status(StatusCode::FOUND)
        .header(axum::http::header::LOCATION, target)
        .body(axum::body::Body::empty())
        .expect("redirect response is always buildable")
}

/// `GET /api/v1/calendar/oauth/{kind}/callback` — provider redirect target,
/// authenticated by the single-use `state` rather than a session. Exchanges
/// the code, stores encrypted tokens, enqueues the initial sync, and always
/// answers with a browser redirect: `?connected={kind}` on success,
/// `?error=oauth_*` on failure.
pub async fn calendar_oauth_callback(
    State(state): State<AppState>,
    Path(kind): Path<CalendarSourceKind>,
    Query(query): Query<CalendarOauthCallbackQuery>,
) -> axum::response::Response {
    let base = "/settings/apps/calendar";
    let redirect = |reason: &str| oauth_redirect(&format!("{base}?error={reason}"));

    if !matches!(
        kind,
        CalendarSourceKind::Google | CalendarSourceKind::Outlook
    ) {
        return redirect("oauth_unconfigured");
    }
    // Provider-side denial (user declined consent).
    if query.error.is_some() {
        return redirect("oauth_denied");
    }
    let (Some(state_param), Some(code)) = (query.state.as_deref(), query.code.as_deref()) else {
        return redirect("oauth_state");
    };

    let result = if kind == CalendarSourceKind::Google {
        state
            .calendar_service
            .complete_google_connect(state_param, code)
            .await
    } else {
        state
            .calendar_service
            .complete_outlook_connect(state_param, code)
            .await
    };
    match result {
        Ok(_) => oauth_redirect(&format!("{base}?connected={}", kind.as_str())),
        Err(CalendarError::OAuthNotConfigured(_)) => redirect("oauth_unconfigured"),
        Err(CalendarError::OAuthStateInvalid) => redirect("oauth_state"),
        Err(CalendarError::OAuthFailed(message)) => {
            tracing::warn!("{} calendar connect failed: {message}", kind.as_str());
            redirect("oauth_exchange")
        }
        // Storage/database errors likewise redirect rather than render
        // anything sensitive.
        Err(_) => redirect("oauth_state"),
    }
}

/// `POST /api/v1/calendar/sources/{id}/disconnect` — wipe stored tokens and
/// mark the source `status: auth_required`; `409` while a live sync lease
/// holds the source (the in-flight run would otherwise overwrite the wipe),
/// `400` for non-OAuth sources. Local token wipe is the effective revocation;
/// no Microsoft-wide session revoke is attempted. Events remain until the
/// source is deleted.
pub async fn disconnect_calendar_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(source_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    state
        .calendar_service
        .disconnect_source(auth.tenant_id, auth.user_id, source_id)
        .await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /api/v1/calendar/sources/{id}/resync` — clear the stored cursor and
/// force a full resync of the configured window now. `202`; `400` for
/// non-external kinds; `409` while a sync lease is live.
pub async fn resync_calendar_source(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(source_id): Path<Uuid>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    let stale = state.calendar_service.sync_lease_stale_threshold();
    state
        .calendar_service
        .resync_source(auth.tenant_id, auth.user_id, source_id, stale)
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "ok": true })),
    ))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CalendarImportJobResponse {
    pub id: Uuid,
    pub source_id: Uuid,
    pub filename: String,
    pub status: String,
    pub total_events: i32,
    pub processed_events: i32,
    pub failed_events: i32,
    pub last_error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<CalendarImportJob> for CalendarImportJobResponse {
    fn from(job: CalendarImportJob) -> Self {
        Self {
            id: job.id,
            source_id: job.source_id,
            filename: job.filename,
            status: job.status,
            total_events: job.total_events,
            processed_events: job.processed_events,
            failed_events: job.failed_events,
            last_error: job.last_error,
            started_at: job.started_at,
            completed_at: job.completed_at,
            created_at: job.created_at,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CalendarImportJobListResponse {
    pub jobs: Vec<CalendarImportJobResponse>,
}

/// `GET /api/v1/calendar/import-jobs` — the caller's import jobs, newest
/// first.
pub async fn list_calendar_import_jobs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<CalendarImportJobListResponse>, AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    let jobs = state
        .metadata_store
        .list_calendar_import_jobs_by_owner(auth.tenant_id, auth.user_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to list import jobs: {e}")))?;

    Ok(Json(CalendarImportJobListResponse {
        jobs: jobs
            .into_iter()
            .map(CalendarImportJobResponse::from)
            .collect(),
    }))
}

/// `GET /api/v1/calendar/import-jobs/{id}` — single import job. `200` / `404`.
pub async fn get_calendar_import_job(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(job_id): Path<Uuid>,
) -> Result<Json<CalendarImportJobResponse>, AppError> {
    require_calendar_enabled(&state, auth.tenant_id).await?;
    let job = state
        .metadata_store
        .get_calendar_import_job(auth.tenant_id, auth.user_id, job_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to load import job: {e}")))?
        .ok_or_else(|| AppError::NotFound("Import job not found".to_string()))?;

    Ok(Json(CalendarImportJobResponse::from(job)))
}

impl From<CalendarError> for AppError {
    fn from(err: CalendarError) -> Self {
        match err {
            CalendarError::NotFound(_) | CalendarError::SourceNotFound(_) => {
                AppError::NotFound(err.to_string())
            }
            CalendarError::ReadOnlyMirror
            | CalendarError::DuplicateSource
            | CalendarError::InternalSource
            | CalendarError::SyncInProgress => AppError::Conflict(err.to_string()),
            CalendarError::InvalidInput(_) => AppError::BadRequest(err.to_string()),
            CalendarError::OAuthNotConfigured(_) => AppError::service_unavailable(err.to_string()),
            CalendarError::OAuthStateInvalid | CalendarError::OAuthFailed(_) => {
                AppError::bad_request(err.to_string())
            }
            CalendarError::Storage(_) | CalendarError::Database(_) => {
                AppError::Internal("Internal server error".to_string())
            }
        }
    }
}
