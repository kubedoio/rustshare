//! RFC 5545 (.ics) import: parse uploaded calendar files and upsert the
//! contained VEVENTs into `calendar_events` (issue #315, Task 2).
//!
//! Import semantics are normative in `docs/specs/calendar-application-v1alpha1.md`
//! §Import semantics: upsert on `(source_id, UID, RECURRENCE-ID)`, TZID
//! converted to UTC via the system tz database (floating times are UTC),
//! all-day DATE values become UTC-midnight spans, RRULE stored verbatim,
//! VALARM discarded, VTODO/VJOURNAL/VFREEBUSY counted as skipped, per-component
//! failures counted with a bounded `last_error` sample, and a structurally
//! unreadable file fails the whole job.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use icalendar::parser::{read_calendar, Component, Property};
use rustshare_core::domain::{CalendarEvent, CalendarImportJob};
use rustshare_storage::MetadataStore;
use uuid::Uuid;

/// Cap on the `last_error` sample persisted on the job row.
const MAX_LAST_ERROR_LEN: usize = 512;
/// Heartbeat/progress flush interval in events.
const PROGRESS_FLUSH_INTERVAL: usize = 25;

/// Bounded error sample for the job row.
fn bounded_error(message: &str) -> String {
    let mut sample = message.chars().take(MAX_LAST_ERROR_LEN).collect::<String>();
    if message.chars().count() > MAX_LAST_ERROR_LEN {
        sample.push('…');
    }
    sample
}

/// Per-run counters returned to the job processor.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportOutcome {
    /// VEVENT components found in the file.
    pub total_events: i32,
    /// VEVENTs successfully upserted.
    pub processed_events: i32,
    /// VEVENTs that failed to map and were skipped.
    pub failed_events: i32,
    /// VTODO/VJOURNAL/VFREEBUSY components (ignored by design).
    pub skipped_components: i32,
}

#[derive(Debug, thiserror::Error)]
pub enum IcalImportError {
    #[error("import job has status {0} and cannot be processed")]
    InvalidJobStatus(String),
    #[error("uploaded file content is missing")]
    MissingContent,
    #[error("calendar file is unreadable: {0}")]
    Unparseable(String),
    #[error("storage error: {0}")]
    Storage(String),
}

/// One VEVENT mapped onto the `calendar_events` column set.
#[derive(Debug, Clone, PartialEq)]
struct ParsedEvent {
    external_uid: String,
    recurrence_id: Option<String>,
    title: String,
    description: Option<String>,
    location: Option<String>,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    all_day: bool,
    original_date: Option<NaiveDate>,
    timezone: String,
    rrule: Option<String>,
    status: String,
}

/// Result of mapping a single component.
enum ComponentOutcome {
    Event(Box<Result<ParsedEvent, String>>),
    Skipped,
}

fn find_prop<'a>(component: &'a Component<'_>, name: &str) -> Option<&'a Property<'a>> {
    component
        .properties
        .iter()
        .find(|prop| prop.name.as_str().eq_ignore_ascii_case(name))
}

fn find_param<'a>(prop: &'a Property<'_>, name: &str) -> Option<&'a str> {
    prop.params
        .iter()
        .find(|param| param.key.as_str().eq_ignore_ascii_case(name))
        .and_then(|param| param.val.as_ref())
        .map(|val| val.as_str())
}

/// Parse one DTSTART/DTEND/RECURRENCE-ID value into a UTC instant plus the
/// IANA timezone it was expressed in (`UTC` for floating/UTC values).
///
/// Returns `all_day = true` for DATE values; those become UTC-midnight spans
/// with `original_date` preserved.
fn parse_date_time(
    prop: &Property<'_>,
) -> Result<(DateTime<Utc>, String, bool, Option<NaiveDate>), String> {
    let value = prop.val.as_str().trim();
    let tzid = find_param(prop, "TZID");
    let value_type = find_param(prop, "VALUE");

    // DATE values are 8 chars without a time; VALUE=DATE also marks all-day.
    let is_date = value.len() == 8
        || value_type
            .map(|vt| vt.eq_ignore_ascii_case("date"))
            .unwrap_or(false);

    if is_date {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d")
            .map_err(|e| format!("invalid DATE value '{value}': {e}"))?;
        let midnight = date.and_hms_opt(0, 0, 0).ok_or("invalid DATE")?;
        return Ok((midnight.and_utc(), "UTC".to_string(), true, Some(date)));
    }

    // A trailing Z marks a UTC instant; a bare value is floating time. Both
    // are interpreted as UTC per the spec.
    let naive_value = value.strip_suffix('Z').unwrap_or(value);
    let naive = chrono::NaiveDateTime::parse_from_str(naive_value, "%Y%m%dT%H%M%S")
        .map_err(|e| format!("invalid DATE-TIME value '{value}': {e}"))?;

    match tzid {
        Some(tzid) => {
            let tz: chrono_tz::Tz = tzid.parse().map_err(|_| format!("unknown TZID '{tzid}'"))?;
            let start = tz
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| format!("ambiguous local time '{value}' in {tzid}"))?;
            Ok((start.with_timezone(&Utc), tzid.to_string(), false, None))
        }
        None => Ok((naive.and_utc(), "UTC".to_string(), false, None)),
    }
}

fn unescaped(prop: &Property<'_>) -> String {
    let value = prop.val.clone().unescape_text();
    value.as_str().to_owned()
}

fn map_component(component: &Component<'_>) -> ComponentOutcome {
    if !component.name.as_str().eq_ignore_ascii_case("VEVENT") {
        return ComponentOutcome::Skipped;
    }

    ComponentOutcome::Event(Box::new(map_vevent(component)))
}

fn map_vevent(component: &Component<'_>) -> Result<ParsedEvent, String> {
    let uid = find_prop(component, "UID")
        .map(|prop| prop.val.as_str().trim().to_string())
        .filter(|uid| !uid.is_empty())
        .ok_or("VEVENT is missing UID")?;

    let dtstart_prop = find_prop(component, "DTSTART").ok_or("VEVENT is missing DTSTART")?;
    let (starts_at, timezone, all_day, original_date) = parse_date_time(dtstart_prop)?;

    let (ends_at, _, _, _) = match find_prop(component, "DTEND") {
        Some(dtend) => parse_date_time(dtend)?,
        None => {
            // RFC 5545 makes DTEND optional. The table requires ends_at >
            // starts_at, so default to a one-day span for all-day events and
            // a one-hour span for timed events.
            let default = if all_day {
                Duration::days(1)
            } else {
                Duration::hours(1)
            };
            (
                starts_at + default,
                timezone.clone(),
                all_day,
                original_date,
            )
        }
    };

    let recurrence_id = find_prop(component, "RECURRENCE-ID")
        .map(parse_date_time)
        .transpose()?
        .map(|(instant, _, _, _)| {
            // Stored in exactly the format range-expansion produces for
            // instance starts, so override rows match their master.
            instant.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
        });

    let title = find_prop(component, "SUMMARY")
        .map(unescaped)
        .unwrap_or_default();
    let description = find_prop(component, "DESCRIPTION").map(unescaped);
    let location = find_prop(component, "LOCATION").map(unescaped);

    let rrule = find_prop(component, "RRULE")
        .map(|prop| prop.val.as_str().trim().to_string())
        .filter(|rrule| !rrule.is_empty());
    let status = find_prop(component, "STATUS")
        .map(|prop| prop.val.as_str().trim().to_ascii_lowercase())
        .filter(|status| matches!(status.as_str(), "confirmed" | "tentative" | "cancelled"))
        .unwrap_or_else(|| "confirmed".to_string());

    Ok(ParsedEvent {
        external_uid: uid,
        recurrence_id,
        title,
        description,
        location,
        starts_at,
        ends_at,
        all_day,
        original_date,
        timezone,
        rrule,
        status,
    })
}

struct ParsedFile {
    events: Vec<Result<ParsedEvent, String>>,
    skipped_components: i32,
}

/// Parse the file content. A structurally unreadable file is a hard error
/// (fails the job); malformed individual components come back as `Err`
/// entries inside `events`.
fn parse_file(bytes: &[u8]) -> Result<ParsedFile, IcalImportError> {
    let text = String::from_utf8_lossy(bytes);
    let unfolded = icalendar::parser::unfold(&text);
    let calendar =
        read_calendar(&unfolded).map_err(|e| IcalImportError::Unparseable(bounded_error(&e)))?;

    let mut events = Vec::new();
    let mut skipped_components = 0i32;
    for component in &calendar.components {
        match map_component(component) {
            ComponentOutcome::Event(outcome) => events.push(*outcome),
            ComponentOutcome::Skipped => skipped_components += 1,
        }
    }

    if events.is_empty() && skipped_components == 0 {
        return Err(IcalImportError::Unparseable(
            "file contains no calendar components".to_string(),
        ));
    }

    Ok(ParsedFile {
        events,
        skipped_components,
    })
}

fn to_event(job: &CalendarImportJob, parsed: &ParsedEvent) -> CalendarEvent {
    let now = Utc::now();
    CalendarEvent {
        id: Uuid::new_v4(),
        tenant_id: job.tenant_id,
        owner_id: job.owner_id,
        source_id: job.source_id,
        external_uid: Some(parsed.external_uid.clone()),
        external_etag: None,
        recurrence_id: parsed.recurrence_id.clone(),
        title: parsed.title.clone(),
        description: parsed.description.clone(),
        location: parsed.location.clone(),
        starts_at: parsed.starts_at,
        ends_at: parsed.ends_at,
        all_day: parsed.all_day,
        original_date: parsed.original_date,
        timezone: parsed.timezone.clone(),
        rrule: parsed.rrule.clone(),
        status: parsed.status.clone(),
        read_only: true,
        raw: None,
        deleted_at: None,
        created_at: now,
        updated_at: now,
    }
}

/// Parse the uploaded bytes and upsert every VEVENT into `calendar_events`.
/// Progress (and the `updated_at` heartbeat) is flushed every
/// [`PROGRESS_FLUSH_INTERVAL`] events.
///
/// Per-component semantics apply at every level: a mapping failure or a
/// single failing upsert counts against that event only (with a bounded
/// `last_error` sample) and the loop continues; only failures of the
/// progress-flush path itself abort the job.
pub async fn parse_and_upsert(
    metadata_store: &MetadataStore,
    job: &CalendarImportJob,
    bytes: &[u8],
) -> Result<ImportOutcome, IcalImportError> {
    let parsed = parse_file(bytes)?;
    let total = parsed.events.len() as i32;

    let mut outcome = ImportOutcome {
        total_events: total,
        skipped_components: parsed.skipped_components,
        ..ImportOutcome::default()
    };
    let mut last_error: Option<String> = None;
    let mut dirty = false;

    for (index, event) in parsed.events.into_iter().enumerate() {
        match event {
            Ok(parsed) => {
                let result = metadata_store
                    .upsert_calendar_imported_event(&to_event(job, &parsed))
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string());
                record_upsert_result(&mut outcome, result, &mut last_error);
            }
            Err(message) => {
                outcome.failed_events += 1;
                last_error = Some(bounded_error(&message));
            }
        }
        dirty = true;

        if (index + 1) % PROGRESS_FLUSH_INTERVAL == 0 {
            flush_progress(metadata_store, job, &outcome, last_error.as_deref()).await?;
            dirty = false;
        }
    }

    if dirty {
        flush_progress(metadata_store, job, &outcome, last_error.as_deref()).await?;
    }

    Ok(outcome)
}

/// Fold one event's upsert result into the run counters. A failing upsert
/// must not abort the import: it counts as that event's failure only.
fn record_upsert_result(
    outcome: &mut ImportOutcome,
    result: Result<(), String>,
    last_error: &mut Option<String>,
) {
    match result {
        Ok(()) => outcome.processed_events += 1,
        Err(message) => {
            outcome.failed_events += 1;
            *last_error = Some(bounded_error(&message));
        }
    }
}

async fn flush_progress(
    metadata_store: &MetadataStore,
    job: &CalendarImportJob,
    outcome: &ImportOutcome,
    last_error: Option<&str>,
) -> Result<(), IcalImportError> {
    metadata_store
        .update_calendar_import_job_progress(
            job.id,
            outcome.total_events,
            outcome.processed_events,
            outcome.failed_events,
            last_error,
        )
        .await
        .map_err(|e| IcalImportError::Storage(e.to_string()))
}

/// Worker entry point for one claimed job: read the spooled bytes, parse and
/// upsert, then move the job to `completed` (or `failed`).
pub async fn process_import_job(
    metadata_store: &MetadataStore,
    job: &CalendarImportJob,
) -> Result<ImportOutcome, IcalImportError> {
    if !matches!(job.status.as_str(), "pending" | "running") {
        return Err(IcalImportError::InvalidJobStatus(job.status.clone()));
    }

    let bytes = metadata_store
        .get_calendar_import_job_content(job.id)
        .await
        .map_err(|e| IcalImportError::Storage(e.to_string()))?
        .ok_or(IcalImportError::MissingContent)?;

    let result = parse_and_upsert(metadata_store, job, &bytes).await;
    match &result {
        Ok(_) => {
            if !metadata_store
                .mark_calendar_import_job_completed(job.id)
                .await
                .map_err(|e| IcalImportError::Storage(e.to_string()))?
            {
                tracing::info!(
                    job_id = %job.id,
                    "Calendar import job finished but is no longer running; leaving status untouched"
                );
            }
        }
        Err(error) => {
            let marked = metadata_store
                .mark_calendar_import_job_failed(job.id, &bounded_error(&error.to_string()))
                .await
                .map_err(|e| IcalImportError::Storage(e.to_string()))?;
            if !marked {
                tracing::info!(
                    job_id = %job.id,
                    "Calendar import job failed but is no longer active; leaving status untouched"
                );
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const TZID_ICS: &str = "\
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Test//Test//EN
BEGIN:VEVENT
UID:tzid-1
DTSTART;TZID=Europe/Berlin:20261005T140000
DTEND;TZID=Europe/Berlin:20261005T150000
SUMMARY:Berlin sync
END:VEVENT
END:VCALENDAR
";

    #[test]
    fn parses_minimal_vevent() {
        let ics = "\
BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:minimal-1
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
SUMMARY:Minimal
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        assert_eq!(parsed.skipped_components, 0);
        assert_eq!(parsed.events.len(), 1);
        let event = parsed.events[0].as_ref().expect("event maps");
        assert_eq!(event.external_uid, "minimal-1");
        assert_eq!(event.title, "Minimal");
        assert_eq!(event.timezone, "UTC");
        assert!(!event.all_day);
        assert_eq!(event.starts_at.to_rfc3339(), "2026-10-05T14:00:00+00:00");
        assert_eq!(event.status, "confirmed");
        assert!(event.rrule.is_none());
    }

    #[test]
    fn converts_tzid_to_utc() {
        let parsed = parse_file(TZID_ICS.as_bytes()).expect("parses");
        let event = parsed.events[0].as_ref().expect("event maps");
        assert_eq!(event.timezone, "Europe/Berlin");
        // 2026-10-05 is CEST (UTC+2).
        assert_eq!(event.starts_at.to_rfc3339(), "2026-10-05T12:00:00+00:00");
        assert_eq!(event.ends_at.to_rfc3339(), "2026-10-05T13:00:00+00:00");
    }

    #[test]
    fn parses_all_day_date_as_utc_midnight_span() {
        let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:allday-1
DTSTART;VALUE=DATE:20261005
DTEND;VALUE=DATE:20261006
SUMMARY:All day
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        let event = parsed.events[0].as_ref().expect("event maps");
        assert!(event.all_day);
        assert_eq!(
            event.original_date,
            Some(NaiveDate::from_ymd_opt(2026, 10, 5).unwrap())
        );
        assert_eq!(event.starts_at.to_rfc3339(), "2026-10-05T00:00:00+00:00");
        assert_eq!(event.ends_at.to_rfc3339(), "2026-10-06T00:00:00+00:00");
    }

    #[test]
    fn parses_rrule_master_and_recurrence_override() {
        let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:recur-1
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
RRULE:FREQ=WEEKLY;BYDAY=MO
SUMMARY:Weekly
END:VEVENT
BEGIN:VEVENT
UID:recur-1
RECURRENCE-ID:20261012T140000Z
DTSTART:20261012T160000Z
DTEND:20261012T170000Z
SUMMARY:Weekly moved
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        assert_eq!(parsed.events.len(), 2);
        let master = parsed.events[0].as_ref().expect("master maps");
        assert_eq!(master.rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO"));
        assert!(master.recurrence_id.is_none());
        let override_event = parsed.events[1].as_ref().expect("override maps");
        assert_eq!(
            override_event.recurrence_id.as_deref(),
            Some("2026-10-12T14:00:00Z")
        );
        assert_eq!(
            override_event.starts_at.to_rfc3339(),
            "2026-10-12T16:00:00+00:00"
        );
    }

    #[test]
    fn malformed_component_counts_as_failed_and_valarm_is_ignored() {
        let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:ok-1
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
BEGIN:VALARM
TRIGGER:-PT15M
END:VALARM
END:VEVENT
BEGIN:VEVENT
UID:broken-1
SUMMARY:No DTSTART
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        assert_eq!(parsed.events.len(), 2);
        assert!(parsed.events[0].is_ok());
        let error = parsed.events[1].as_ref().unwrap_err();
        assert!(error.contains("DTSTART"), "unexpected error: {error}");
    }

    #[test]
    fn counts_vtodo_vjournal_vfreebusy_as_skipped() {
        let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:evt-1
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
END:VEVENT
BEGIN:VTODO
UID:todo-1
SUMMARY:Task
END:VTODO
BEGIN:VJOURNAL
UID:journal-1
END:VJOURNAL
BEGIN:VFREEBUSY
UID:fb-1
END:VFREEBUSY
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.skipped_components, 3);
    }

    #[test]
    fn structurally_unreadable_file_fails() {
        let result = parse_file(b"this is not a calendar at all");
        assert!(matches!(result, Err(IcalImportError::Unparseable(_))));
    }

    #[test]
    fn single_upsert_failure_does_not_abort_the_run() {
        let mut outcome = ImportOutcome::default();
        let mut last_error = None;

        // A mapping/upsert failure counts against its event only…
        record_upsert_result(
            &mut outcome,
            Err("duplicate key".to_string()),
            &mut last_error,
        );
        assert_eq!(outcome.processed_events, 0);
        assert_eq!(outcome.failed_events, 1);
        assert_eq!(last_error.as_deref(), Some("duplicate key"));

        // …and subsequent events still process.
        record_upsert_result(&mut outcome, Ok(()), &mut last_error);
        record_upsert_result(&mut outcome, Ok(()), &mut last_error);
        assert_eq!(outcome.processed_events, 2);
        assert_eq!(outcome.failed_events, 1);
        // The error sample keeps the most recent failure.
        assert_eq!(last_error.as_deref(), Some("duplicate key"));
    }

    #[test]
    fn folded_lines_unfold_before_parsing() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:fold-1\r\nDTSTART:20261005T140000Z\r\nDTEND:20261005T150000Z\r\nSUMMARY:A very long summary line that has been\r\n  folded across lines\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        let event = parsed.events[0].as_ref().expect("event maps");
        assert_eq!(
            event.title,
            "A very long summary line that has been folded across lines"
        );
    }
}
