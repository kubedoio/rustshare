use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use rustshare_core::domain::{
    CalendarEvent, CalendarEventStatus, CalendarSource, CalendarSourceKind, UserId,
};
use rustshare_crypto::SecretEncryptionKey;
use rustshare_storage::MetadataStore;
use uuid::Uuid;

fn db_error(err: anyhow::Error) -> CalendarError {
    CalendarError::Database(err.to_string())
}

const MAX_EVENT_TITLE_LEN: usize = 512;
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
}

#[derive(Clone)]
pub struct CalendarService {
    metadata_store: Arc<MetadataStore>,
    #[allow(dead_code)]
    secret_key: Arc<SecretEncryptionKey>,
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

fn validate_timezone(timezone: &str) -> Result<(), CalendarError> {
    if timezone.parse::<chrono_tz::Tz>().is_err() {
        return Err(CalendarError::InvalidInput(format!(
            "Unknown IANA timezone: {timezone}"
        )));
    }
    Ok(())
}

fn validate_event_times(
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
        let midnight = starts_at.time() == chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()
            && ends_at.time() == chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        let whole_days = (ends_at - starts_at).num_seconds() % (24 * 3600) == 0;
        if !midnight || !whole_days {
            return Err(CalendarError::InvalidInput(
                "All-day events must align to whole days".to_string(),
            ));
        }
    }
    Ok(())
}

/// Expand a recurring master within the window. Returns occurrence starts
/// (UTC) overlapping `[from, to)`; occurrences overridden by a stored
/// `recurrence_id` row are omitted (the override row is returned as stored).
fn expand_master(
    event: &CalendarEvent,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    override_starts: &[String],
) -> Vec<DateTime<Utc>> {
    let Some(rrule) = event.rrule.as_deref() else {
        return Vec::new();
    };
    let dtstart = event.starts_at.format("%Y%m%dT%H%M%SZ");
    let Ok(set) = format!("DTSTART:{dtstart}\nRRULE:{rrule}").parse::<rrule::RRuleSet>() else {
        tracing::warn!(event_id = %event.id, "stored RRULE failed to parse; returning no instances");
        return Vec::new();
    };
    let duration = event.ends_at - event.starts_at;
    // One-second margins make the bound semantics (inclusive/exclusive) of
    // the rrule crate irrelevant; exact overlap filtering happens below.
    let occurrences = set
        .after((from - duration - Duration::seconds(1)).with_timezone(&rrule::Tz::UTC))
        .before((to + Duration::seconds(1)).with_timezone(&rrule::Tz::UTC))
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
        }
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
            if display_name.is_empty() {
                return Err(CalendarError::InvalidInput(
                    "display_name must not be empty".to_string(),
                ));
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
        if input.title.is_empty() || input.title.len() > MAX_EVENT_TITLE_LEN {
            return Err(CalendarError::InvalidInput(format!(
                "title must be 1-{MAX_EVENT_TITLE_LEN} characters"
            )));
        }
        validate_timezone(&input.timezone)?;
        validate_event_times(input.starts_at, input.ends_at, input.all_day)?;

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
        self.metadata_store
            .create_calendar_event(&event)
            .await
            .map_err(db_error)?;
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
        if patch.description.is_some() {
            event.description = patch.description;
        }
        if patch.location.is_some() {
            event.location = patch.location;
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
        if patch.rrule.is_some() {
            event.rrule = patch.rrule;
        }
        validate_event_times(event.starts_at, event.ends_at, event.all_day)?;
        event.original_date = event.all_day.then(|| event.starts_at.date_naive());
        self.metadata_store
            .update_calendar_event(&event)
            .await
            .map_err(db_error)?;
        Ok(event)
    }

    /// Soft-delete an internal event. Unknown IDs belonging to another user
    /// are 404; deleting an already-deleted (or never-existing) row is a
    /// successful no-op so retries cannot fail.
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
                self.metadata_store
                    .soft_delete_calendar_event(tenant_id, owner_id, event_id)
                    .await
                    .map_err(db_error)?;
                Ok(())
            }
            None => {
                if self
                    .metadata_store
                    .calendar_event_exists_any_owner(event_id)
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

        let mut occurrences = Vec::new();
        for row in &rows {
            let source_kind = kind_by_id
                .get(&row.source_id)
                .copied()
                .unwrap_or(CalendarSourceKind::Internal);
            if row.rrule.is_some() {
                let override_starts: Vec<String> = rows
                    .iter()
                    .filter(|other| {
                        other.source_id == row.source_id
                            && other.external_uid.is_some()
                            && other.external_uid == row.external_uid
                            && other.recurrence_id.is_some()
                    })
                    .filter_map(|other| other.recurrence_id.clone())
                    .collect();
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
