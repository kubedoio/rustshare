use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine as _;
use rand::Rng as _;

use chrono::{DateTime, Duration, Timelike, Utc};
use rustshare_core::domain::{
    CalendarEvent, CalendarEventStatus, CalendarSource, CalendarSourceKind, PrincipalId, TenantId,
    UserId, WorkspaceId,
};
use rustshare_crypto::SecretEncryptionKey;
use rustshare_integration_events::event::{ActorRef, IntegrationEvent};
use rustshare_integration_events::event_types::{
    CALENDAR_EVENT_CREATED_V1, CALENDAR_EVENT_DELETED_V1, CALENDAR_EVENT_IMPORTED_V1,
    CALENDAR_EVENT_UPDATED_V1,
};
use rustshare_resource_auth::resource_ref::ResourceRef;
use rustshare_storage::{MetadataStore, OutboxStore};
use uuid::Uuid;

const CALENDAR_APPLICATION_ID: &str = "io.elembra.calendar";
const CALENDAR_EVENT_SOURCE_URI: &str = "elembra://io.elembra.calendar";

fn db_error(err: anyhow::Error) -> CalendarError {
    CalendarError::Database(err.to_string())
}

fn tx_error(err: sqlx::Error) -> CalendarError {
    CalendarError::Database(err.to_string())
}

const MAX_EVENT_TITLE_LEN: usize = 512;
const MAX_SOURCE_DISPLAY_NAME_LEN: usize = 255;
const MAX_RANGE_WINDOW_DAYS: i64 = 366;
const MAX_EXPANDED_INSTANCES_PER_MASTER: u16 = 1000;

#[derive(Debug, thiserror::Error)]
pub enum CalendarError {
    #[error("Calendar event not found: {0}")]
    NotFound(Uuid),
    #[error("Calendar source not found: {0}")]
    SourceNotFound(Uuid),
    #[error("Event is synchronized read-only from an external source")]
    ReadOnlyMirror,
    #[error("Invalid calendar input: {0}")]
    InvalidInput(String),
    #[error("An identical calendar source already exists")]
    DuplicateSource,
    #[error("Internal calendar sources cannot be deleted")]
    InternalSource,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("OAuth provider is not configured: {0}")]
    OAuthNotConfigured(String),
    #[error("OAuth state is invalid, expired, or already used")]
    OAuthStateInvalid,
    #[error("OAuth flow failed: {0}")]
    OAuthFailed(String),
    #[error("A sync is already running for this source")]
    SyncInProgress,
}

/// How long a connect-flow OAuth state stays valid.
const OAUTH_STATE_TTL: Duration = Duration::minutes(10);

/// Fallback sync-lease staleness window (seconds), matching the default of
/// `AppConfig::calendar_sync_worker_stale_secs`.
const DEFAULT_SYNC_LEASE_STALE_SECS: i64 = 300;

/// The sync-lease staleness window this deployment uses, read from the same
/// `RUSTSHARE_CALENDAR_SYNC_WORKER_STALE_SECS` environment variable that
/// `AppConfig` (via `envy`) binds into `calendar_sync_worker_stale_secs`. The
/// resync and disconnect endpoints must apply the worker's own liveness rule
/// when deciding whether an in-flight sync still holds a source, otherwise a
/// 409-vs-force decision can disagree with the worker. Absent or unparseable
/// values fall back to the config default.
fn configured_sync_lease_stale() -> Duration {
    let secs = std::env::var("RUSTSHARE_CALENDAR_SYNC_WORKER_STALE_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(DEFAULT_SYNC_LEASE_STALE_SECS);
    Duration::seconds(secs.max(0))
}

#[derive(Clone)]
pub struct CalendarService {
    metadata_store: Arc<MetadataStore>,
    #[allow(dead_code)]
    secret_key: Arc<SecretEncryptionKey>,
    google: Option<Arc<crate::services::google_calendar::GoogleCalendarClient>>,
    outlook: Option<Arc<crate::services::outlook_calendar::OutlookCalendarClient>>,
    outbox: Option<Arc<OutboxStore>>,
    sync_lease_stale: Duration,
}

/// One event as returned by range queries: the stored row (or its recurring
/// master) plus the expanded occurrence's start, if any.
#[derive(Debug, Clone)]
pub struct CalendarEventOccurrence {
    pub event: CalendarEvent,
    pub source_kind: CalendarSourceKind,
    pub instance_start: Option<DateTime<Utc>>,
}

/// Input for creating an internal calendar event.
pub struct NewCalendarEvent {
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub all_day: bool,
    pub timezone: String,
    pub rrule: Option<String>,
}

/// Partial update for an internal calendar event.
#[derive(Default)]
pub struct CalendarEventPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: Option<bool>,
    pub timezone: Option<String>,
    pub rrule: Option<String>,
}

pub(crate) fn validate_timezone(timezone: &str) -> Result<(), CalendarError> {
    if timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err(CalendarError::InvalidInput(format!(
            "Unknown IANA timezone: {timezone}"
        )));
    }
    Ok(())
}

pub(crate) fn validate_event_times(
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    all_day: bool,
) -> Result<(), CalendarError> {
    if ends_at <= starts_at {
        return Err(CalendarError::InvalidInput(
            "ends_at must be after starts_at".to_string(),
        ));
    }
    if all_day {
        let midnight = starts_at.time().num_seconds_from_midnight() == 0
            && ends_at.time().num_seconds_from_midnight() == 0;
        let whole_days = (ends_at - starts_at).num_seconds() % (24 * 3600) == 0;
        if !midnight || !whole_days {
            return Err(CalendarError::InvalidInput(
                "All-day events must align to whole days".to_string(),
            ));
        }
    }
    Ok(())
}

/// Validate a stored-verbatim RRULE string the same way expansion parses it,
/// so a bad RRULE is rejected with a 400 at write time instead of silently
/// producing no instances at read time.
pub(crate) fn validate_rrule(rrule: &str) -> Result<(), CalendarError> {
    // Expansion always prefixes the stored value with a DTSTART line; parse
    // with a fixed DTSTART here so validation and expansion agree.
    const VALIDATION_DTSTART: &str = "DTSTART:20261001T000000Z";
    if format!("{VALIDATION_DTSTART}\nRRULE:{rrule}")
        .parse::<rrule::RRuleSet>()
        .is_err()
    {
        return Err(CalendarError::InvalidInput(format!(
            "Invalid RRULE: {rrule}"
        )));
    }
    Ok(())
}

/// Validate a listing window: `from` must precede `to`, and the span may not
/// exceed [`MAX_RANGE_WINDOW_DAYS`]. The cap is exclusive: a span of exactly
/// `MAX_RANGE_WINDOW_DAYS` days is accepted.
pub(crate) fn validate_window(from: DateTime<Utc>, to: DateTime<Utc>) -> Result<(), CalendarError> {
    if from >= to {
        return Err(CalendarError::InvalidInput(
            "from must be before to".to_string(),
        ));
    }
    if to - from > Duration::days(MAX_RANGE_WINDOW_DAYS) {
        return Err(CalendarError::InvalidInput(format!(
            "Range window must be at most {MAX_RANGE_WINDOW_DAYS} days"
        )));
    }
    Ok(())
}

/// Build the durable integration envelope for an internal calendar-event
/// mutation. Minimum-safe-data: identifiers and timing only — never
/// title/description/location.
pub fn build_event_envelope(
    tenant_id: Uuid,
    owner_id: UserId,
    event: &CalendarEvent,
    event_type: &str,
) -> Result<IntegrationEvent, CalendarError> {
    let resource = ResourceRef::new(
        rustshare_core::domain::ApplicationId::new(CALENDAR_APPLICATION_ID),
        "event",
        event.id.to_string(),
    );
    let data = serde_json::json!({
        "event_id": event.id,
        "source_id": event.source_id,
        "starts_at": event.starts_at,
        "ends_at": event.ends_at,
        "all_day": event.all_day,
        "status": event.status,
    });
    IntegrationEvent::builder()
        .source(CALENDAR_EVENT_SOURCE_URI)
        .r#type(event_type)
        .subject(resource.to_uri())
        .tenant_id(TenantId(tenant_id))
        .workspace_id(WorkspaceId(tenant_id))
        .actor(ActorRef::Principal(PrincipalId(owner_id)))
        .resource(resource)
        .data(data)
        .build()
        .map_err(|e| CalendarError::Storage(format!("envelope validation failed: {e}")))
}

/// Publish one `io.elembra.calendar.event.imported.v1` for a completed
/// import/sync run. `counts` carries counters only (plus the source
/// ResourceRef on the envelope) — never event titles/descriptions.
///
/// Best-effort by design: the run's effects are already committed; a failed
/// publication is logged and must not fail or retry the run.
pub async fn publish_imported_event(
    outbox: &OutboxStore,
    tenant_id: Uuid,
    owner_id: UserId,
    source_id: Uuid,
    counts: serde_json::Value,
) {
    let resource = ResourceRef::new(
        rustshare_core::domain::ApplicationId::new(CALENDAR_APPLICATION_ID),
        "source",
        source_id.to_string(),
    );
    let envelope = match IntegrationEvent::builder()
        .source(CALENDAR_EVENT_SOURCE_URI)
        .r#type(CALENDAR_EVENT_IMPORTED_V1)
        .subject(resource.to_uri())
        .tenant_id(TenantId(tenant_id))
        .workspace_id(WorkspaceId(tenant_id))
        .actor(ActorRef::Principal(PrincipalId(owner_id)))
        .resource(resource)
        .data(counts)
        .build()
    {
        Ok(envelope) => envelope,
        Err(e) => {
            tracing::warn!(source_id = %source_id, "calendar imported-event envelope invalid: {e}");
            return;
        }
    };
    let mut tx = match outbox.pool().begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!(source_id = %source_id, "calendar imported-event publish failed to begin tx: {e}");
            return;
        }
    };
    if let Err(e) = outbox.insert_in_tx(&mut tx, &envelope).await {
        tracing::warn!(source_id = %source_id, "calendar imported-event publish failed: {e}");
        return;
    }
    if let Err(e) = tx.commit().await {
        tracing::warn!(source_id = %source_id, "calendar imported-event publish commit failed: {e}");
    }
}

/// Expand a recurring master within the window. Returns occurrence starts
/// (UTC) overlapping `[from, to)`; occurrences overridden by a stored
/// `recurrence_id` row are omitted (the override row is returned as stored).
///
/// Iteration is wall-clock in the event's IANA `TZID` (normative per the
/// spec): a DST-spanning RRULE expands on local wall-clock times, and each
/// occurrence is converted to a UTC instant only afterwards.
fn expand_master(
    event: &CalendarEvent,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    override_starts: &[String],
) -> Vec<DateTime<Utc>> {
    let Some(rrule) = event.rrule.as_deref() else {
        return Vec::new();
    };
    let tz: rrule::Tz = event
        .timezone
        .parse::<chrono_tz::Tz>()
        .map(rrule::Tz::from)
        .unwrap_or(rrule::Tz::UTC);
    // The stored starts_at is a UTC instant; converting it into the event's
    // timezone yields the wall-clock time DTSTART must carry for wall-clock
    // iteration.
    let dtstart_line = if tz == rrule::Tz::UTC {
        format!("DTSTART:{}Z", event.starts_at.format("%Y%m%dT%H%M%S"))
    } else {
        format!(
            "DTSTART;TZID={}:{}",
            tz.name(),
            event.starts_at.with_timezone(&tz).format("%Y%m%dT%H%M%S")
        )
    };
    let Ok(set) = format!("{dtstart_line}\nRRULE:{rrule}").parse::<rrule::RRuleSet>() else {
        tracing::warn!(event_id = %event.id, "stored RRULE failed to parse; returning no instances");
        return Vec::new();
    };
    let duration = event.ends_at - event.starts_at;
    // One-second margins make the bound semantics (inclusive/exclusive) of
    // the rrule crate irrelevant; exact overlap filtering happens below.
    let occurrences = set
        .after((from - duration - Duration::seconds(1)).with_timezone(&tz))
        .before((to + Duration::seconds(1)).with_timezone(&tz))
        .all(MAX_EXPANDED_INSTANCES_PER_MASTER);
    occurrences
        .dates
        .into_iter()
        .map(|dt| dt.with_timezone(&Utc))
        .filter(|start| *start < to && *start + duration > from)
        .filter(|start| {
            let rfc3339 = start.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true);
            !override_starts.contains(&rfc3339)
        })
        .collect()
}

impl CalendarService {
    pub fn new(metadata_store: Arc<MetadataStore>, secret_key: Arc<SecretEncryptionKey>) -> Self {
        Self {
            metadata_store,
            secret_key,
            google: None,
            outlook: None,
            outbox: None,
            sync_lease_stale: configured_sync_lease_stale(),
        }
    }

    /// The sync-lease staleness window this service applies to resync/
    /// disconnect conflict decisions (see `configured_sync_lease_stale`).
    pub fn sync_lease_stale_threshold(&self) -> Duration {
        self.sync_lease_stale
    }

    /// Attach the transactional integration outbox. When configured, internal
    /// event create/update/delete publish `io.elembra.calendar.event.*.v1`
    /// envelopes atomically with the mutation (issue #315).
    pub fn configure_outbox(&mut self, outbox: Arc<OutboxStore>) {
        self.outbox = Some(outbox);
    }

    /// Attach the Google OAuth client; absent when the deployment has no
    /// Google client id/secret (connect then returns 503, not a startup error).
    pub fn configure_google(
        &mut self,
        client: Option<crate::services::google_calendar::GoogleCalendarClient>,
    ) {
        self.google = client.map(Arc::new);
    }

    /// Attach the Microsoft/Outlook OAuth client; absent when the deployment
    /// has no Microsoft client id/secret.
    pub fn configure_outlook(
        &mut self,
        client: Option<crate::services::outlook_calendar::OutlookCalendarClient>,
    ) {
        self.outlook = client.map(Arc::new);
    }

    /// The configured Microsoft/Outlook client, if any (sync worker path).
    pub fn outlook_client(
        &self,
    ) -> Option<Arc<crate::services::outlook_calendar::OutlookCalendarClient>> {
        self.outlook.clone()
    }

    /// The configured Google client, if any (sync worker + revoke paths).
    pub fn google_client(
        &self,
    ) -> Option<Arc<crate::services::google_calendar::GoogleCalendarClient>> {
        self.google.clone()
    }

    /// Begin the OAuth connect flow for `kind` (`google` / `outlook`):
    /// persist a single-use 256-bit state bound to the user and return the
    /// provider consent URL.
    pub async fn begin_connect(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        kind: CalendarSourceKind,
    ) -> Result<String, CalendarError> {
        let mut state_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut state_bytes);
        let state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(state_bytes);
        let authorize_url = match kind {
            CalendarSourceKind::Google => self
                .google
                .as_ref()
                .map(|client| client.authorize_url(&state)),
            CalendarSourceKind::Outlook => self
                .outlook
                .as_ref()
                .map(|client| client.authorize_url(&state)),
            _ => None,
        };
        let Some(authorize_url) = authorize_url else {
            return Err(CalendarError::OAuthNotConfigured(format!(
                "{} OAuth is not configured",
                kind.as_str()
            )));
        };
        self.metadata_store
            .insert_calendar_oauth_state(
                &state,
                tenant_id,
                owner_id,
                kind.as_str(),
                Utc::now() + OAUTH_STATE_TTL,
            )
            .await
            .map_err(db_error)?;
        Ok(authorize_url)
    }

    /// Complete the OAuth flow: validate-and-consume the state, exchange the
    /// code, resolve the account email, and store encrypted tokens. The new
    /// source is due for its initial sync immediately.
    pub async fn complete_google_connect(
        &self,
        state: &str,
        code: &str,
    ) -> Result<CalendarSource, CalendarError> {
        use crate::services::google_calendar::GoogleError;
        let oauth_state = self
            .metadata_store
            .consume_calendar_oauth_state(state)
            .await
            .map_err(db_error)?
            .ok_or(CalendarError::OAuthStateInvalid)?;
        if oauth_state.kind != CalendarSourceKind::Google.as_str() {
            return Err(CalendarError::OAuthStateInvalid);
        }
        let client = self.google.clone().ok_or_else(|| {
            CalendarError::OAuthNotConfigured("google OAuth is not configured".to_string())
        })?;
        let tokens = client.exchange_code(code).await.map_err(|err| match err {
            GoogleError::AuthRequired => {
                CalendarError::OAuthFailed("provider rejected the grant".to_string())
            }
            other => CalendarError::OAuthFailed(other.to_string()),
        })?;
        // Without a refresh token the sync worker could never renew the
        // short-lived access token; an encrypted empty string here would
        // brick the source silently. Fail closed and make the user re-run
        // the consent flow.
        let Some(refresh_token) = tokens.rotated_refresh_token() else {
            return Err(CalendarError::OAuthFailed(
                "provider did not return a refresh token; run the connect flow again".to_string(),
            ));
        };
        let external_account = client
            .user_email(tokens.access_token())
            .await
            .map_err(|e| CalendarError::OAuthFailed(e.to_string()))?;
        let refresh_enc = rustshare_crypto::encrypt_secret(refresh_token, &self.secret_key)
            .map_err(|e| CalendarError::Storage(e.to_string()))?;
        let access_enc = rustshare_crypto::encrypt_secret(tokens.access_token(), &self.secret_key)
            .map_err(|e| CalendarError::Storage(e.to_string()))?;
        let display_name = format!("Google ({external_account})");
        self.metadata_store
            .create_oauth_calendar_source(
                oauth_state.tenant_id,
                oauth_state.owner_id,
                CalendarSourceKind::Google.as_str(),
                &display_name,
                &external_account,
                "primary",
                &refresh_enc,
                &access_enc,
                tokens.expires_at(),
                "https://www.googleapis.com/auth/calendar.readonly",
            )
            .await
            .map_err(db_error)
    }

    /// Complete the Microsoft/Outlook OAuth flow: validate-and-consume the
    /// state, exchange the code, resolve the account email, and store
    /// encrypted tokens. The new source is due for its initial sync
    /// immediately.
    pub async fn complete_outlook_connect(
        &self,
        state: &str,
        code: &str,
    ) -> Result<CalendarSource, CalendarError> {
        use crate::services::outlook_calendar::OutlookError;
        let oauth_state = self
            .metadata_store
            .consume_calendar_oauth_state(state)
            .await
            .map_err(db_error)?
            .ok_or(CalendarError::OAuthStateInvalid)?;
        if oauth_state.kind != CalendarSourceKind::Outlook.as_str() {
            return Err(CalendarError::OAuthStateInvalid);
        }
        let client = self.outlook.clone().ok_or_else(|| {
            CalendarError::OAuthNotConfigured("outlook OAuth is not configured".to_string())
        })?;
        let tokens = client.exchange_code(code).await.map_err(|err| match err {
            OutlookError::AuthRequired => {
                CalendarError::OAuthFailed("provider rejected the grant".to_string())
            }
            other => CalendarError::OAuthFailed(other.to_string()),
        })?;
        let Some(refresh_token) = tokens.rotated_refresh_token() else {
            return Err(CalendarError::OAuthFailed(
                "provider did not return a refresh token; run the connect flow again".to_string(),
            ));
        };
        let external_account = client
            .user_email(tokens.access_token())
            .await
            .map_err(|e| CalendarError::OAuthFailed(e.to_string()))?;
        let refresh_enc = rustshare_crypto::encrypt_secret(refresh_token, &self.secret_key)
            .map_err(|e| CalendarError::Storage(e.to_string()))?;
        let access_enc = rustshare_crypto::encrypt_secret(tokens.access_token(), &self.secret_key)
            .map_err(|e| CalendarError::Storage(e.to_string()))?;
        let display_name = format!("Outlook ({external_account})");
        self.metadata_store
            .create_oauth_calendar_source(
                oauth_state.tenant_id,
                oauth_state.owner_id,
                CalendarSourceKind::Outlook.as_str(),
                &display_name,
                &external_account,
                "primary",
                &refresh_enc,
                &access_enc,
                tokens.expires_at(),
                "offline_access Calendars.Read",
            )
            .await
            .map_err(db_error)
    }

    /// Disconnect an OAuth source: reject with a conflict while a live sync
    /// lease holds it, otherwise wipe stored tokens and mark the source
    /// `auth_required`. Events remain until the source is deleted. Local token
    /// wipe is the effective revocation — Outlook never had the
    /// `User.RevokeSessions.All` permission for a Microsoft-wide session
    /// revoke, so no provider call is attempted for it; Google revocation
    /// stays best-effort.
    pub async fn disconnect_source(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        source_id: Uuid,
    ) -> Result<(), CalendarError> {
        let source = self
            .metadata_store
            .get_calendar_source(tenant_id, owner_id, source_id)
            .await
            .map_err(db_error)?
            .ok_or(CalendarError::SourceNotFound(source_id))?;
        let kind: CalendarSourceKind = source
            .kind
            .parse()
            .map_err(|_| CalendarError::InvalidInput("unknown source kind".to_string()))?;
        if !matches!(
            kind,
            CalendarSourceKind::Google | CalendarSourceKind::Outlook
        ) {
            return Err(CalendarError::InvalidInput(
                "only OAuth-connected sources can be disconnected".to_string(),
            ));
        }
        // Refuse while a live sync lease holds the source: the in-flight run's
        // terminal status write would otherwise resurrect `healthy` over the
        // token wipe (and could leave the source with no refresh token). The
        // pre-check avoids a pointless Google revocation below; the wipe itself
        // is lease-guarded too, so a worker that claims the source in the
        // window between this check and the wipe still wins (see below).
        let stale =
            std::time::Duration::from_secs(self.sync_lease_stale.num_seconds().max(0) as u64);
        if self
            .metadata_store
            .calendar_source_is_locked(source_id, stale)
            .await
            .map_err(db_error)?
        {
            return Err(CalendarError::SyncInProgress);
        }
        // Best-effort Google revocation; local wipe happens regardless.
        // Revocation failures are logged, not propagated.
        if kind == CalendarSourceKind::Google {
            if let (Some(client), Some(refresh_enc)) =
                (self.google.clone(), source.refresh_token_enc)
            {
                if let Ok(refresh_token) =
                    rustshare_crypto::decrypt_secret(&refresh_enc, &self.secret_key)
                {
                    if !client.revoke_token(&refresh_token).await {
                        tracing::warn!(source_id = %source_id, "google token revocation was not accepted");
                    }
                }
            }
        }
        // The lease guard is inside the wipe: 0 rows affected means a worker
        // claimed the source after the pre-check (or the row vanished), so we
        // must not report a successful disconnect.
        if !self
            .metadata_store
            .wipe_calendar_source_tokens(source_id, stale)
            .await
            .map_err(db_error)?
        {
            return Err(CalendarError::SyncInProgress);
        }
        Ok(())
    }

    /// Clear the cursor and force a full resync now; rejected while a sync
    /// lease is live (409).
    pub async fn resync_source(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        source_id: Uuid,
        stale: Duration,
    ) -> Result<(), CalendarError> {
        let source = self
            .metadata_store
            .get_calendar_source(tenant_id, owner_id, source_id)
            .await
            .map_err(db_error)?
            .ok_or(CalendarError::SourceNotFound(source_id))?;
        if !matches!(source.kind.as_str(), "google" | "outlook") {
            return Err(CalendarError::InvalidInput(
                "only external sources can be resynced".to_string(),
            ));
        }
        let stale = std::time::Duration::from_secs(stale.num_seconds().max(0) as u64);
        let forced = self
            .metadata_store
            .force_calendar_source_resync(source_id, stale)
            .await
            .map_err(db_error)?;
        if forced {
            return Ok(());
        }
        // Not forced: either a live lease holds the source, or no
        // sync-state row exists yet. Only the lease case is a conflict; a
        // missing row is repaired (and the source forced due) instead.
        if self
            .metadata_store
            .calendar_source_is_locked(source_id, stale)
            .await
            .map_err(db_error)?
        {
            return Err(CalendarError::SyncInProgress);
        }
        self.metadata_store
            .ensure_calendar_sync_state(source_id)
            .await
            .map_err(db_error)?;
        let forced = self
            .metadata_store
            .force_calendar_source_resync(source_id, stale)
            .await
            .map_err(db_error)?;
        if !forced {
            return Err(CalendarError::SyncInProgress);
        }
        Ok(())
    }

    /// The implicit `internal` source, created lazily on first use.
    pub async fn ensure_internal_source(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
    ) -> Result<CalendarSource, CalendarError> {
        self.metadata_store
            .ensure_internal_calendar_source(tenant_id, owner_id)
            .await
            .map_err(db_error)
    }

    pub async fn list_sources(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
    ) -> Result<Vec<CalendarSource>, CalendarError> {
        self.metadata_store
            .list_calendar_sources(tenant_id, owner_id)
            .await
            .map_err(db_error)
    }

    /// Create a non-OAuth source. `ical_import` creates a named import
    /// container; `internal` returns the existing internal source.
    pub async fn create_source(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        kind: CalendarSourceKind,
        display_name: String,
    ) -> Result<CalendarSource, CalendarError> {
        match kind {
            CalendarSourceKind::Internal => self.ensure_internal_source(tenant_id, owner_id).await,
            CalendarSourceKind::IcalImport => {
                if display_name.is_empty() {
                    return Err(CalendarError::InvalidInput(
                        "display_name must not be empty".to_string(),
                    ));
                }
                let existing = self
                    .metadata_store
                    .list_calendar_sources(tenant_id, owner_id)
                    .await
                    .map_err(db_error)?;
                let duplicate = existing.iter().any(|source| {
                    source.kind == CalendarSourceKind::IcalImport.as_str()
                        && source.display_name == display_name
                });
                if duplicate {
                    return Err(CalendarError::DuplicateSource);
                }
                self.metadata_store
                    .create_ical_import_source(tenant_id, owner_id, &display_name)
                    .await
                    .map_err(db_error)
            }
            CalendarSourceKind::Google | CalendarSourceKind::Outlook => {
                Err(CalendarError::InvalidInput(
                    "OAuth sources are created via the connect flow".to_string(),
                ))
            }
        }
    }

    /// Look up one of the caller's sources by ID.
    pub async fn get_source(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        source_id: Uuid,
    ) -> Result<Option<CalendarSource>, CalendarError> {
        self.metadata_store
            .get_calendar_source(tenant_id, owner_id, source_id)
            .await
            .map_err(db_error)
    }

    pub async fn update_source(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        source_id: Uuid,
        display_name: Option<String>,
        is_enabled: Option<bool>,
    ) -> Result<CalendarSource, CalendarError> {
        let mut source = self
            .metadata_store
            .get_calendar_source(tenant_id, owner_id, source_id)
            .await
            .map_err(db_error)?
            .ok_or(CalendarError::SourceNotFound(source_id))?;
        if let Some(display_name) = display_name {
            if display_name.is_empty() || display_name.len() > MAX_SOURCE_DISPLAY_NAME_LEN {
                return Err(CalendarError::InvalidInput(format!(
                    "display_name must be 1-{MAX_SOURCE_DISPLAY_NAME_LEN} characters"
                )));
            }
            source.display_name = display_name;
        }
        if let Some(is_enabled) = is_enabled {
            source.is_enabled = is_enabled;
        }
        self.metadata_store
            .update_calendar_source(&source)
            .await
            .map_err(db_error)?;
        Ok(source)
    }

    /// Soft-delete a source and its mirrored events. Internal sources cannot
    /// be deleted.
    pub async fn delete_source(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        source_id: Uuid,
    ) -> Result<(), CalendarError> {
        let source = self
            .metadata_store
            .get_calendar_source(tenant_id, owner_id, source_id)
            .await
            .map_err(db_error)?
            .ok_or(CalendarError::SourceNotFound(source_id))?;
        if source.kind == CalendarSourceKind::Internal.as_str() {
            return Err(CalendarError::InternalSource);
        }
        self.metadata_store
            .soft_delete_calendar_source(tenant_id, owner_id, source_id)
            .await
            .map_err(db_error)?;
        Ok(())
    }

    pub async fn create_event(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        input: NewCalendarEvent,
    ) -> Result<CalendarEvent, CalendarError> {
        // Create has nothing to clear, so an empty optional string means "no
        // value" (the frontend sends either an empty string or null).
        let mut input = input;
        input.description = input.description.filter(|value| !value.is_empty());
        input.location = input.location.filter(|value| !value.is_empty());
        input.rrule = input.rrule.filter(|value| !value.is_empty());
        if input.title.is_empty() || input.title.len() > MAX_EVENT_TITLE_LEN {
            return Err(CalendarError::InvalidInput(format!(
                "title must be 1-{MAX_EVENT_TITLE_LEN} characters"
            )));
        }
        validate_timezone(&input.timezone)?;
        validate_event_times(input.starts_at, input.ends_at, input.all_day)?;
        if let Some(rrule) = input.rrule.as_deref() {
            validate_rrule(rrule)?;
        }

        let source = self.ensure_internal_source(tenant_id, owner_id).await?;
        let now = Utc::now();
        let event = CalendarEvent {
            id: Uuid::new_v4(),
            tenant_id,
            owner_id,
            source_id: source.id,
            external_uid: None,
            external_etag: None,
            recurrence_id: None,
            title: input.title,
            description: input.description,
            location: input.location,
            starts_at: input.starts_at,
            ends_at: input.ends_at,
            all_day: input.all_day,
            original_date: input.all_day.then(|| input.starts_at.date_naive()),
            timezone: input.timezone,
            rrule: input.rrule,
            status: CalendarEventStatus::Confirmed.as_str().to_string(),
            read_only: false,
            raw: None,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        };
        if let Some(outbox) = &self.outbox {
            // One transaction: the mutation and the durable envelope commit
            // or roll back together (integration-event-v1alpha1 contract).
            let mut tx = self.metadata_store.pool().begin().await.map_err(tx_error)?;
            self.metadata_store
                .create_calendar_event_in_tx(&mut tx, &event)
                .await
                .map_err(db_error)?;
            let envelope =
                build_event_envelope(tenant_id, owner_id, &event, CALENDAR_EVENT_CREATED_V1)?;
            outbox
                .insert_in_tx(&mut tx, &envelope)
                .await
                .map_err(|e| CalendarError::Storage(e.to_string()))?;
            tx.commit().await.map_err(tx_error)?;
        } else {
            self.metadata_store
                .create_calendar_event(&event)
                .await
                .map_err(db_error)?;
        }
        Ok(event)
    }

    pub async fn get_event(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        event_id: Uuid,
    ) -> Result<CalendarEvent, CalendarError> {
        self.metadata_store
            .get_calendar_event(tenant_id, owner_id, event_id)
            .await
            .map_err(db_error)?
            .ok_or(CalendarError::NotFound(event_id))
    }

    pub async fn update_event(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        event_id: Uuid,
        patch: CalendarEventPatch,
    ) -> Result<CalendarEvent, CalendarError> {
        let mut event = self.get_event(tenant_id, owner_id, event_id).await?;
        if event.read_only {
            return Err(CalendarError::ReadOnlyMirror);
        }
        if let Some(title) = patch.title {
            if title.is_empty() || title.len() > MAX_EVENT_TITLE_LEN {
                return Err(CalendarError::InvalidInput(format!(
                    "title must be 1-{MAX_EVENT_TITLE_LEN} characters"
                )));
            }
            event.title = title;
        }
        if let Some(description) = patch.description {
            // PATCH wire convention: an empty string clears the field; a
            // non-empty string sets it. (serde cannot distinguish an absent
            // key from an explicit null, both of which mean "leave
            // unchanged".)
            event.description = (!description.is_empty()).then_some(description);
        }
        if let Some(location) = patch.location {
            event.location = (!location.is_empty()).then_some(location);
        }
        if let Some(starts_at) = patch.starts_at {
            event.starts_at = starts_at;
        }
        if let Some(ends_at) = patch.ends_at {
            event.ends_at = ends_at;
        }
        if let Some(all_day) = patch.all_day {
            event.all_day = all_day;
        }
        if let Some(timezone) = patch.timezone {
            validate_timezone(&timezone)?;
            event.timezone = timezone;
        }
        if let Some(value) = patch.rrule {
            if value.is_empty() {
                event.rrule = None;
            } else {
                validate_rrule(&value)?;
                event.rrule = Some(value);
            }
        }
        validate_event_times(event.starts_at, event.ends_at, event.all_day)?;
        event.original_date = event.all_day.then(|| event.starts_at.date_naive());
        if let Some(outbox) = &self.outbox {
            let mut tx = self.metadata_store.pool().begin().await.map_err(tx_error)?;
            let updated = self
                .metadata_store
                .update_calendar_event_in_tx(&mut tx, &event)
                .await
                .map_err(db_error)?;
            if !updated {
                // The row was concurrently soft-deleted (or no longer belongs
                // to the caller) after the read above; do not announce an
                // update that did not land.
                tx.rollback().await.map_err(tx_error)?;
                return Err(CalendarError::NotFound(event_id));
            }
            let envelope =
                build_event_envelope(tenant_id, owner_id, &event, CALENDAR_EVENT_UPDATED_V1)?;
            outbox
                .insert_in_tx(&mut tx, &envelope)
                .await
                .map_err(|e| CalendarError::Storage(e.to_string()))?;
            tx.commit().await.map_err(tx_error)?;
        } else {
            // Use the in-transaction helper even without an outbox: its
            // rows-affected result detects a row concurrently soft-deleted (or
            // no longer owned by the caller) after the read above, which the
            // non-transactional wrapper would discard — returning 200 with
            // stale values instead of 404.
            let mut tx = self.metadata_store.pool().begin().await.map_err(tx_error)?;
            let updated = self
                .metadata_store
                .update_calendar_event_in_tx(&mut tx, &event)
                .await
                .map_err(db_error)?;
            if !updated {
                tx.rollback().await.map_err(tx_error)?;
                return Err(CalendarError::NotFound(event_id));
            }
            tx.commit().await.map_err(tx_error)?;
        }
        Ok(event)
    }

    /// Soft-delete an internal event. An ID that names a live row for the
    /// tenant but not the caller is a 404; an already-deleted, never-existing,
    /// or foreign-tenant ID is a successful no-op, so a cross-tenant ID is
    /// indistinguishable from a random one.
    pub async fn delete_event(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        event_id: Uuid,
    ) -> Result<(), CalendarError> {
        match self
            .metadata_store
            .get_calendar_event(tenant_id, owner_id, event_id)
            .await
            .map_err(db_error)?
        {
            Some(event) => {
                if event.read_only {
                    return Err(CalendarError::ReadOnlyMirror);
                }
                if let Some(outbox) = &self.outbox {
                    let mut tx = self.metadata_store.pool().begin().await.map_err(tx_error)?;
                    let deleted = self
                        .metadata_store
                        .soft_delete_calendar_event_in_tx(&mut tx, tenant_id, owner_id, event_id)
                        .await
                        .map_err(db_error)?;
                    if deleted {
                        let envelope = build_event_envelope(
                            tenant_id,
                            owner_id,
                            &event,
                            CALENDAR_EVENT_DELETED_V1,
                        )?;
                        outbox
                            .insert_in_tx(&mut tx, &envelope)
                            .await
                            .map_err(|e| CalendarError::Storage(e.to_string()))?;
                    }
                    tx.commit().await.map_err(tx_error)?;
                } else {
                    self.metadata_store
                        .soft_delete_calendar_event(tenant_id, owner_id, event_id)
                        .await
                        .map_err(db_error)?;
                }
                Ok(())
            }
            None => {
                if self
                    .metadata_store
                    .calendar_event_exists_in_tenant(tenant_id, event_id)
                    .await
                    .map_err(db_error)?
                {
                    return Err(CalendarError::NotFound(event_id));
                }
                Ok(())
            }
        }
    }

    /// Range list with server-side recurrence expansion. The window must be
    /// at most 366 days.
    pub async fn list_events(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        source_ids: &[Uuid],
        include_cancelled: bool,
    ) -> Result<Vec<CalendarEventOccurrence>, CalendarError> {
        validate_window(from, to)?;

        let sources = self.list_sources(tenant_id, owner_id).await?;
        let kind_by_id: HashMap<Uuid, CalendarSourceKind> = sources
            .into_iter()
            .map(|source| {
                let kind = source
                    .kind
                    .parse::<CalendarSourceKind>()
                    .unwrap_or(CalendarSourceKind::Internal);
                (source.id, kind)
            })
            .collect();

        let rows = self
            .metadata_store
            .list_calendar_events_in_range(
                tenant_id,
                owner_id,
                from,
                to,
                source_ids,
                include_cancelled,
            )
            .await
            .map_err(db_error)?;

        // Override keys are fetched independently of the listing's status and
        // time-window filters: a cancelled or moved occurrence of a recurring
        // master is stored as an override row that may carry
        // `status = 'cancelled'` or fall outside `[from, to)`, so deriving the
        // suppression set only from `rows` would let `expand_master` regenerate
        // a phantom occurrence at the original slot.
        let overrides = self
            .metadata_store
            .list_calendar_event_overrides(tenant_id, owner_id, source_ids)
            .await
            .map_err(db_error)?;
        // Keyed by `(source_id, external_uid)`: two sources may expose the
        // same external UID, and an override in one must not suppress the
        // other source's legitimate occurrence.
        let mut suppressed_by_uid: HashMap<(Uuid, &str), Vec<String>> = HashMap::new();
        for key in &overrides {
            suppressed_by_uid
                .entry((key.source_id, key.external_uid.as_str()))
                .or_default()
                .push(key.recurrence_id.clone());
        }

        let mut occurrences = Vec::new();
        for row in &rows {
            let source_kind = kind_by_id
                .get(&row.source_id)
                .copied()
                .unwrap_or(CalendarSourceKind::Internal);
            if row.rrule.is_some() {
                // Both the full override set (above) and any override row
                // returned alongside its master in-window suppress the
                // master's occurrence at that recurrence id.
                let mut override_starts: Vec<String> = row
                    .external_uid
                    .as_deref()
                    .and_then(|uid| suppressed_by_uid.get(&(row.source_id, uid)))
                    .cloned()
                    .unwrap_or_default();
                override_starts.extend(
                    rows.iter()
                        .filter(|other| {
                            other.source_id == row.source_id
                                && other.external_uid.is_some()
                                && other.external_uid == row.external_uid
                                && other.recurrence_id.is_some()
                        })
                        .filter_map(|other| other.recurrence_id.clone()),
                );
                for instance_start in expand_master(row, from, to, &override_starts) {
                    occurrences.push(CalendarEventOccurrence {
                        event: row.clone(),
                        source_kind,
                        instance_start: Some(instance_start),
                    });
                }
            } else {
                occurrences.push(CalendarEventOccurrence {
                    event: row.clone(),
                    source_kind,
                    instance_start: None,
                });
            }
        }
        occurrences.sort_by_key(|occurrence| {
            occurrence
                .instance_start
                .unwrap_or(occurrence.event.starts_at)
        });
        Ok(occurrences)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recurring_master(
        timezone: &str,
        starts_at: &str,
        ends_at: &str,
        rrule: &str,
    ) -> CalendarEvent {
        CalendarEvent {
            id: Uuid::new_v4(),
            tenant_id: Uuid::nil(),
            owner_id: Uuid::nil(),
            source_id: Uuid::new_v4(),
            external_uid: None,
            external_etag: None,
            recurrence_id: None,
            title: "Recurring".to_string(),
            description: None,
            location: None,
            starts_at: starts_at.parse().unwrap(),
            ends_at: ends_at.parse().unwrap(),
            all_day: false,
            original_date: None,
            timezone: timezone.to_string(),
            rrule: Some(rrule.to_string()),
            status: CalendarEventStatus::Confirmed.as_str().to_string(),
            read_only: false,
            raw: None,
            deleted_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn utc(value: &str) -> DateTime<Utc> {
        value.parse().unwrap()
    }

    #[test]
    fn expands_dst_spanning_weekly_rrule_on_berlin_wall_clock() {
        // 2026-10-23 14:00 in Europe/Berlin is 12:00Z (CEST, UTC+2).
        let event = recurring_master(
            "Europe/Berlin",
            "2026-10-23T12:00:00Z",
            "2026-10-23T13:00:00Z",
            "FREQ=WEEKLY;COUNT=2",
        );
        let starts = expand_master(
            &event,
            utc("2026-10-01T00:00:00Z"),
            utc("2026-11-30T00:00:00Z"),
            &[],
        );
        // Wall-clock iteration keeps 14:00 Berlin local: the second
        // occurrence lands on 13:00Z (CET, UTC+1), not 12:00Z as a UTC
        // instant iteration would produce.
        assert_eq!(
            starts,
            vec![utc("2026-10-23T12:00:00Z"), utc("2026-10-30T13:00:00Z")]
        );
    }

    #[test]
    fn utc_rrule_expansion_is_unchanged() {
        let event = recurring_master(
            "UTC",
            "2026-10-05T14:00:00Z",
            "2026-10-05T15:00:00Z",
            "FREQ=WEEKLY;COUNT=3",
        );
        let starts = expand_master(
            &event,
            utc("2026-10-01T00:00:00Z"),
            utc("2026-11-30T00:00:00Z"),
            &[],
        );
        assert_eq!(
            starts,
            vec![
                utc("2026-10-05T14:00:00Z"),
                utc("2026-10-12T14:00:00Z"),
                utc("2026-10-19T14:00:00Z"),
            ]
        );
    }

    #[test]
    fn overridden_occurrence_is_omitted_from_expansion() {
        let event = recurring_master(
            "UTC",
            "2026-10-05T14:00:00Z",
            "2026-10-05T15:00:00Z",
            "FREQ=WEEKLY;COUNT=2",
        );
        let starts = expand_master(
            &event,
            utc("2026-10-01T00:00:00Z"),
            utc("2026-11-30T00:00:00Z"),
            &["2026-10-12T14:00:00Z".to_string()],
        );
        assert_eq!(starts, vec![utc("2026-10-05T14:00:00Z")]);
    }

    #[test]
    fn validate_timezone_accepts_iana_and_rejects_unknown() {
        assert!(validate_timezone("Europe/Berlin").is_ok());
        assert!(validate_timezone("UTC").is_ok());
        assert!(validate_timezone("Mars/Olympus").is_err());
        assert!(validate_timezone("").is_err());
    }

    #[test]
    fn validate_event_times_rejects_end_before_or_equal_start() {
        let start = utc("2026-10-14T09:00:00Z");
        assert!(validate_event_times(start, start, false).is_err());
        assert!(validate_event_times(start, utc("2026-10-14T08:00:00Z"), false).is_err());
        assert!(validate_event_times(start, utc("2026-10-14T10:00:00Z"), false).is_ok());
    }

    #[test]
    fn validate_event_times_requires_whole_day_alignment_for_all_day() {
        // All-day events must both start and end at UTC midnight and span a
        // whole number of days. The existing rule is midnight alignment, not
        // merely whole-day duration.
        let start = utc("2026-10-14T00:00:00Z");
        assert!(validate_event_times(start, utc("2026-10-16T00:00:00Z"), true).is_ok());
        assert!(validate_event_times(start, utc("2026-10-14T12:00:00Z"), true).is_err());
        // Whole-day span but not midnight-aligned: still rejected.
        assert!(validate_event_times(
            utc("2026-10-14T06:00:00Z"),
            utc("2026-10-16T06:00:00Z"),
            true
        )
        .is_err());
    }

    #[test]
    fn validate_window_enforces_order_and_the_366_day_cap() {
        let from = utc("2026-01-01T00:00:00Z");
        assert!(validate_window(from, from).is_err());
        assert!(validate_window(from, utc("2026-01-01T00:00:01Z")).is_ok());
        assert!(validate_window(from, utc("2027-01-01T00:00:00Z")).is_ok()); // exactly 366 days
        assert!(validate_window(from, utc("2027-01-02T00:00:01Z")).is_err()); // beyond the cap
    }
}
