//! RFC 5545 (.ics) import: parse uploaded calendar files and upsert the
//! contained VEVENTs into `calendar_events` (issue #315, Task 2).
//!
//! Import semantics are normative in `docs/specs/calendar-application-v1alpha1.md`
//! §Import semantics. Only representable UTC/IANA times, all-day dates, basic
//! event metadata, and supported UTC recurrence are accepted. Floating times,
//! custom timezone definitions, recurrence overrides, and unsupported event
//! fields are reported instead of flattened. File, event, property, RRULE, and
//! parse-time limits are enforced before import work is queued.

use std::collections::HashMap;

use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use icalendar::parser::{read_components, Component, Property};
use rustshare_core::domain::{CalendarEvent, CalendarImportJob};
use rustshare_storage::{MetadataStore, OutboxStore};
use sha2::Digest;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct ImportPreviewEvent {
    pub uid: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: bool,
    pub timezone: Option<String>,
    pub recurrence: bool,
    pub rrule: Option<String>,
    pub status: Option<String>,
    pub outcome: &'static str,
    pub diagnostic: Option<String>,
}

pub fn preview_import(bytes: &[u8]) -> Result<Vec<ImportPreviewEvent>, IcalImportError> {
    let parsed = parse_file(bytes)?;
    let mut seen = HashMap::<String, ParsedEvent>::new();
    let mut preview = parsed
        .events
        .into_iter()
        .map(|event| match event {
            Ok(event) => {
                let outcome = match seen.get(&event.external_uid) {
                    Some(previous) if previous == &event => "duplicate",
                    Some(_) => "conflict",
                    None => "new",
                };
                seen.entry(event.external_uid.clone())
                    .or_insert_with(|| event.clone());
                ImportPreviewEvent {
                    uid: Some(event.external_uid),
                    title: Some(event.title),
                    description: event.description,
                    location: event.location,
                    starts_at: Some(event.starts_at),
                    ends_at: Some(event.ends_at),
                    all_day: event.all_day,
                    timezone: Some(event.timezone),
                    recurrence: event.rrule.is_some(),
                    rrule: event.rrule,
                    status: Some(event.status),
                    outcome,
                    diagnostic: None,
                }
            }
            Err(message) => ImportPreviewEvent {
                uid: None,
                title: None,
                description: None,
                location: None,
                starts_at: None,
                ends_at: None,
                all_day: false,
                timezone: None,
                recurrence: false,
                rrule: None,
                status: None,
                outcome: "unsupported",
                diagnostic: Some(bounded_error(&message)),
            },
        })
        .collect::<Vec<_>>();
    preview.extend((0..parsed.skipped_components).map(|_| ImportPreviewEvent {
        uid: None,
        title: None,
        description: None,
        location: None,
        starts_at: None,
        ends_at: None,
        all_day: false,
        timezone: None,
        recurrence: false,
        rrule: None,
        status: None,
        outcome: "unsupported",
        diagnostic: Some("VTODO, VJOURNAL, and VFREEBUSY components are not imported".into()),
    }));
    Ok(preview)
}

pub fn preview_digest(
    bytes: &[u8],
    source_id: Uuid,
    uids: &[String],
    existing: &[CalendarEvent],
) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    hasher.update(source_id.as_bytes());
    let mut uids = uids.iter().collect::<Vec<_>>();
    uids.sort_unstable();
    uids.dedup();
    for uid in uids {
        hasher.update((uid.len() as u64).to_be_bytes());
        hasher.update(uid.as_bytes());
        if let Some(event) = existing
            .iter()
            .find(|event| event.external_uid.as_deref() == Some(uid))
        {
            let snapshot = serde_json::to_vec(event).expect("CalendarEvent serializes to JSON");
            hasher.update((snapshot.len() as u64).to_be_bytes());
            hasher.update(snapshot);
        } else {
            hasher.update(0_u64.to_be_bytes());
        }
    }
    hex::encode(hasher.finalize())
}

pub fn preview_event_matches_saved(event: &ImportPreviewEvent, saved: &CalendarEvent) -> bool {
    event.uid.as_deref() == saved.external_uid.as_deref()
        && saved.title == event.title.as_deref().unwrap_or_default()
        && saved.description == event.description
        && saved.location == event.location
        && Some(saved.starts_at) == event.starts_at
        && Some(saved.ends_at) == event.ends_at
        && saved.all_day == event.all_day
        && Some(saved.timezone.as_str()) == event.timezone.as_deref()
        && saved.rrule == event.rrule
        && Some(saved.status.as_str()) == event.status.as_deref()
}

/// Cap on the `last_error` sample persisted on the job row.
const MAX_LAST_ERROR_LEN: usize = 512;
/// Maximum VEVENT components accepted in one uploaded calendar.
const MAX_CALENDAR_IMPORT_EVENTS: usize = 1_000;
const MAX_CALENDAR_IMPORT_SIZE_BYTES: usize = 10 * 1024 * 1024;
const MAX_ICAL_PROPERTY_BYTES: usize = 16 * 1024;
const MAX_ICAL_RRULE_BYTES: usize = 1024;
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
    #[error("destination events changed after confirmation; preview and confirm again")]
    StalePreview,
    #[error("calendar import worker claim was lost")]
    ClaimLost,
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
    dtstamp: Option<DateTime<Utc>>,
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

fn unsupported_event_fields(component: &Component<'_>) -> Vec<String> {
    const SUPPORTED: &[&str] = &[
        "DTEND",
        "DTSTAMP",
        "DTSTART",
        "DURATION",
        "DESCRIPTION",
        "LOCATION",
        "RRULE",
        "STATUS",
        "SUMMARY",
        "UID",
    ];
    const UNSUPPORTED: &[&str] = &[
        "ATTACH",
        "ATTENDEE",
        "CATEGORIES",
        "CLASS",
        "CONFERENCE",
        "CREATED",
        "EXDATE",
        "GEO",
        "LAST-MODIFIED",
        "ORGANIZER",
        "RDATE",
        "RELATED-TO",
        "RECURRENCE-ID",
        "RESOURCES",
        "SEQUENCE",
        "TRANSP",
        "URL",
    ];

    let mut fields = Vec::new();
    for property in &component.properties {
        let name = property.name.as_str();
        if UNSUPPORTED
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known))
        {
            fields.push(name.to_ascii_uppercase());
        } else if !SUPPORTED
            .iter()
            .any(|known| name.eq_ignore_ascii_case(known))
        {
            fields.push(format!("unknown property {}", name.to_ascii_uppercase()));
        }
    }
    for child in &component.components {
        fields.push(format!("unsupported component {}", child.name));
    }
    let (recurrence_id_count, has_recurrence_range) = component
        .properties
        .iter()
        .filter(|prop| prop.name.as_str().eq_ignore_ascii_case("RECURRENCE-ID"))
        .fold((0usize, false), |(count, has_range), prop| {
            (
                count + 1,
                has_range
                    || prop
                        .params
                        .iter()
                        .any(|parameter| parameter.key.as_str().eq_ignore_ascii_case("RANGE")),
            )
        });
    if recurrence_id_count > 1 {
        fields.push("multiple RECURRENCE-ID properties".to_string());
    }
    if has_recurrence_range {
        fields.push("RECURRENCE-ID RANGE".to_string());
    }
    fields.sort();
    fields.dedup();
    fields
}

/// Resolve a local wall-clock time in an IANA zone. Ambiguous times (fall-back
/// overlap) keep the earliest occurrence; nonexistent times (spring-forward
/// gap) are pushed forward to the first valid instant.
fn resolve_iana_local(tz: &chrono_tz::Tz, naive: NaiveDateTime) -> DateTime<Utc> {
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(local) => local.with_timezone(&Utc),
        LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
        LocalResult::None => {
            let mut probe = naive;
            for _ in 0..(48 * 60) {
                probe += Duration::minutes(1);
                match tz.from_local_datetime(&probe) {
                    LocalResult::Single(local) => return local.with_timezone(&Utc),
                    LocalResult::Ambiguous(earliest, _) => return earliest.with_timezone(&Utc),
                    LocalResult::None => {}
                }
            }
            naive.and_utc()
        }
    }
}

/// Upper bound on an accepted DURATION magnitude: 100 years. RFC 5545 allows a
/// theoretically unbounded magnitude, but `chrono` panics on out-of-range spans
/// and arithmetic, so anything larger is rejected as a per-component error.
const MAX_DURATION_SECONDS: i64 = 100 * 366 * 86_400;

/// Parse an RFC 5545 DURATION (`[+-]P[nW][nD][T[nH][nM][nS]]`).
fn parse_duration(raw: &str) -> Result<Duration, String> {
    fn take(number: &mut String, raw: &str) -> Result<i64, String> {
        if number.is_empty() {
            return Err(format!("invalid DURATION '{raw}'"));
        }
        let value = number
            .parse::<i64>()
            .map_err(|_| format!("invalid DURATION '{raw}'"))?;
        number.clear();
        Ok(value)
    }

    fn accumulate(seconds: &mut i64, value: i64, factor: i64, raw: &str) -> Result<(), String> {
        let contribution = value
            .checked_mul(factor)
            .ok_or_else(|| format!("DURATION '{raw}' is out of range"))?;
        *seconds = seconds
            .checked_add(contribution)
            .filter(|total| *total <= MAX_DURATION_SECONDS)
            .ok_or_else(|| format!("DURATION '{raw}' is out of range"))?;
        Ok(())
    }

    let raw = raw.trim();
    let (sign, rest) = match raw.strip_prefix('-') {
        Some(rest) => (-1i64, rest),
        None => (1i64, raw.strip_prefix('+').unwrap_or(raw)),
    };
    let rest = rest
        .strip_prefix('P')
        .ok_or_else(|| format!("invalid DURATION '{raw}'"))?;
    let (date_part, time_part) = match rest.split_once('T') {
        Some((date, time)) => (date, Some(time)),
        None => (rest, None),
    };

    let mut seconds = 0i64;
    let mut number = String::new();
    for c in date_part.chars() {
        match c {
            '0'..='9' => number.push(c),
            'W' => accumulate(&mut seconds, take(&mut number, raw)?, 7 * 86_400, raw)?,
            'D' => accumulate(&mut seconds, take(&mut number, raw)?, 86_400, raw)?,
            _ => return Err(format!("invalid DURATION '{raw}'")),
        }
    }
    if !number.is_empty() {
        return Err(format!("invalid DURATION '{raw}'"));
    }
    if let Some(time_part) = time_part {
        for c in time_part.chars() {
            match c {
                '0'..='9' => number.push(c),
                'H' => accumulate(&mut seconds, take(&mut number, raw)?, 3600, raw)?,
                'M' => accumulate(&mut seconds, take(&mut number, raw)?, 60, raw)?,
                'S' => accumulate(&mut seconds, take(&mut number, raw)?, 1, raw)?,
                _ => return Err(format!("invalid DURATION '{raw}'")),
            }
        }
        if !number.is_empty() {
            return Err(format!("invalid DURATION '{raw}'"));
        }
    }
    // `seconds` is bounded above by `MAX_DURATION_SECONDS`, so the negation
    // cannot overflow and `try_seconds` cannot hit the `chrono` panic.
    Duration::try_seconds(sign * seconds).ok_or_else(|| format!("DURATION '{raw}' is out of range"))
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
        if tzid.is_some() {
            return Err("all-day DATE values must not include TZID".to_string());
        }
        let date = NaiveDate::parse_from_str(value, "%Y%m%d")
            .map_err(|e| format!("invalid DATE value '{value}': {e}"))?;
        let midnight = date.and_hms_opt(0, 0, 0).ok_or("invalid DATE")?;
        return Ok((midnight.and_utc(), "UTC".to_string(), true, Some(date)));
    }

    // Floating DATE-TIME has no defined instant. Assigning UTC here would
    // silently change its meaning, so accept only UTC or an explicit IANA TZID.
    if !value.ends_with('Z') && tzid.is_none() {
        return Err("floating DATE-TIME has no timezone; add TZID or use UTC".to_string());
    }
    if value.ends_with('Z') && tzid.is_some() {
        return Err("UTC DATE-TIME must not include TZID".to_string());
    }
    let naive_value = value.strip_suffix('Z').unwrap_or(value);
    let naive = NaiveDateTime::parse_from_str(naive_value, "%Y%m%dT%H%M%S")
        .map_err(|e| format!("invalid DATE-TIME value '{value}': {e}"))?;

    match tzid {
        Some(tzid) => {
            if let Ok(tz) = tzid.parse::<chrono_tz::Tz>() {
                let start = resolve_iana_local(&tz, naive);
                return Ok((start, tzid.to_string(), false, None));
            }
            Err(format!("unsupported TZID '{tzid}'; use an IANA timezone"))
        }
        None => Ok((naive.and_utc(), "UTC".to_string(), false, None)),
    }
}

/// The `icalendar` parser already unescapes TEXT values while parsing, so the
/// stored value is used verbatim (unescaping twice would turn a literal
/// backslash before `n` into a newline).
fn text_value(prop: &Property<'_>) -> String {
    prop.val.as_str().to_owned()
}

fn map_vevent(component: &Component<'_>) -> Result<ParsedEvent, String> {
    if let Some(property) = component
        .properties
        .iter()
        .find(|property| property.val.as_str().len() > MAX_ICAL_PROPERTY_BYTES)
    {
        return Err(format!(
            "{} exceeds the {MAX_ICAL_PROPERTY_BYTES}-byte property limit",
            property.name
        ));
    }
    for name in [
        "UID",
        "DTSTAMP",
        "DTSTART",
        "DTEND",
        "DURATION",
        "SUMMARY",
        "DESCRIPTION",
        "LOCATION",
        "STATUS",
        "RRULE",
    ] {
        if component
            .properties
            .iter()
            .filter(|property| property.name.as_str().eq_ignore_ascii_case(name))
            .count()
            > 1
        {
            return Err(format!("VEVENT contains multiple {name} properties"));
        }
    }
    let unsupported_fields = unsupported_event_fields(component);
    if !unsupported_fields.is_empty() {
        return Err(format!(
            "VEVENT uses unsupported fields {}; this event was not imported",
            unsupported_fields.join(", ")
        ));
    }

    let uid = find_prop(component, "UID")
        .map(|prop| prop.val.as_str().to_owned())
        .filter(|uid| !uid.trim().is_empty())
        .ok_or("VEVENT is missing UID")?;

    let dtstart_prop = find_prop(component, "DTSTART").ok_or("VEVENT is missing DTSTART")?;
    if find_prop(component, "DTEND").is_some() && find_prop(component, "DURATION").is_some() {
        return Err("VEVENT cannot contain both DTEND and DURATION".to_string());
    }
    let (starts_at, timezone, all_day, original_date) = parse_date_time(dtstart_prop)?;

    let (ends_at, _, end_all_day, _) = match find_prop(component, "DTEND") {
        Some(dtend) => parse_date_time(dtend)?,
        None => {
            // RFC 5545 makes DTEND optional in favour of DURATION. When neither
            // is present, default to a one-day span for all-day events and a
            // one-hour span for timed events (the table requires ends_at >
            // starts_at).
            let span = match find_prop(component, "DURATION") {
                Some(duration) => parse_duration(duration.val.as_str())?,
                None if all_day => Duration::days(1),
                None => Duration::hours(1),
            };
            let ends_at = starts_at
                .checked_add_signed(span)
                .ok_or("event end is outside the representable range")?;
            (ends_at, timezone.clone(), all_day, original_date)
        }
    };
    if let Some(dtend) = find_prop(component, "DTEND") {
        if all_day != end_all_day {
            return Err("DTSTART and DTEND must both use DATE or DATE-TIME".to_string());
        }
        if find_param(dtstart_prop, "TZID") != find_param(dtend, "TZID") {
            return Err("DTSTART and DTEND must use the same TZID".to_string());
        }
    }
    if ends_at <= starts_at {
        return Err("event end must be after its start".to_string());
    }

    let recurrence_id = find_prop(component, "RECURRENCE-ID")
        .map(|prop| parse_date_time(prop))
        .transpose()?
        .map(|(instant, _, _, _)| {
            // Stored in exactly the format range-expansion produces for
            // instance starts, so override rows match their master.
            instant.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
        });

    let title = find_prop(component, "SUMMARY")
        .map(text_value)
        .unwrap_or_default();
    let description = find_prop(component, "DESCRIPTION").map(text_value);
    let location = find_prop(component, "LOCATION").map(text_value);

    let rrule = find_prop(component, "RRULE")
        .map(|prop| prop.val.as_str().trim().to_string())
        .filter(|rrule| !rrule.is_empty());
    if let Some(rule) = rrule.as_deref() {
        if rule.len() > MAX_ICAL_RRULE_BYTES {
            return Err(format!(
                "RRULE exceeds the {MAX_ICAL_RRULE_BYTES}-byte limit"
            ));
        }
        if timezone != "UTC" {
            return Err("recurring events must use UTC to preserve export meaning".to_string());
        }
        if format!("DTSTART:20261001T000000Z\nRRULE:{rule}")
            .parse::<rrule::RRuleSet>()
            .is_err()
        {
            return Err("unsupported or invalid RRULE".to_string());
        }
    }
    let status = find_prop(component, "STATUS")
        .map(|prop| prop.val.as_str().trim().to_ascii_lowercase())
        .filter(|status| matches!(status.as_str(), "confirmed" | "tentative" | "cancelled"))
        .unwrap_or_else(|| "confirmed".to_string());
    let dtstamp = find_prop(component, "DTSTAMP")
        .map(|prop| {
            if find_param(prop, "TZID").is_some() || !prop.val.as_str().trim().ends_with('Z') {
                return Err("DTSTAMP must be a UTC DATE-TIME".to_string());
            }
            parse_date_time(prop).map(|(timestamp, _, _, _)| timestamp)
        })
        .transpose()?;

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
        dtstamp,
    })
}

struct ParsedFile {
    events: Vec<Result<ParsedEvent, String>>,
    skipped_components: i32,
}

fn is_counted_skip(name: &str) -> bool {
    name.eq_ignore_ascii_case("VTODO")
        || name.eq_ignore_ascii_case("VJOURNAL")
        || name.eq_ignore_ascii_case("VFREEBUSY")
}

/// Split unfolded calendar text into top-level component blocks.
///
/// The `icalendar` crate's `read_calendar`/`read_components` grammar
/// (`complete(many1(all_consuming(component)))`) only accepts a single root
/// component, so concatenated VCALENDAR blocks (an RFC 5545 stream) have to be
/// parsed individually. A leading UTF-8 BOM is removed separately because it
/// would otherwise make the first line unparseable.
fn split_top_level_components(unfolded: &str) -> Result<Vec<String>, IcalImportError> {
    let mut blocks = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut depth: i32 = 0;
    let mut event_count = 0;
    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r');
        if line.len() > MAX_ICAL_PROPERTY_BYTES {
            return Err(IcalImportError::Unparseable(format!(
                "calendar property exceeds the {MAX_ICAL_PROPERTY_BYTES}-byte limit"
            )));
        }
        let trimmed = line.trim_start();
        if trimmed.eq_ignore_ascii_case("BEGIN:VEVENT") {
            event_count += 1;
            if event_count > MAX_CALENDAR_IMPORT_EVENTS {
                return Err(IcalImportError::Unparseable(format!(
                    "calendar exceeds the {MAX_CALENDAR_IMPORT_EVENTS}-VEVENT import limit"
                )));
            }
        }
        let upper = trimmed.to_ascii_uppercase();
        let is_begin = upper.starts_with("BEGIN:");
        let is_end = upper.starts_with("END:");

        if is_begin {
            depth += 1;
        }
        if depth > 0 || is_begin {
            current.push(line);
        }
        if is_end {
            depth -= 1;
            if depth <= 0 {
                if !current.is_empty() {
                    blocks.push(current.join("\n"));
                    current.clear();
                }
                depth = 0;
            }
        }
    }
    if !current.is_empty() {
        blocks.push(current.join("\n"));
    }
    Ok(blocks)
}

/// Parse the file content. A structurally unreadable file is a hard error
/// (fails the job); malformed individual components come back as `Err`
/// entries inside `events`.
fn parse_file(bytes: &[u8]) -> Result<ParsedFile, IcalImportError> {
    if bytes.len() > MAX_CALENDAR_IMPORT_SIZE_BYTES {
        return Err(IcalImportError::Unparseable(
            "calendar file exceeds the 10 MiB import limit".to_string(),
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| IcalImportError::Unparseable("calendar file must be valid UTF-8".into()))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let unfolded = icalendar::parser::unfold(text);

    let blocks = split_top_level_components(&unfolded)?;
    if blocks.is_empty() {
        return Err(IcalImportError::Unparseable(
            "file contains no calendar components".to_string(),
        ));
    }

    let mut events = Vec::new();
    let mut skipped_components = 0i32;

    for block in &blocks {
        let roots =
            read_components(block).map_err(|e| IcalImportError::Unparseable(bounded_error(&e)))?;
        for root in &roots {
            let children: Vec<&Component<'_>> =
                if root.name.as_str().eq_ignore_ascii_case("VCALENDAR") {
                    root.components.iter().collect()
                } else {
                    vec![root]
                };

            // VTIMEZONE may appear after the VEVENTs that reference it, so
            // collect the definitions for this calendar first.
            // Our single-event exporter uses UTC DATE-TIME properties and
            // carries the stored display timezone on VCALENDAR.
            let calendar_timezone = find_prop(root, "X-WR-TIMEZONE")
                .map(|property| property.val.as_str().trim())
                .filter(|timezone| timezone.parse::<chrono_tz::Tz>().is_ok())
                .map(str::to_owned);

            for child in &children {
                if child.name.as_str().eq_ignore_ascii_case("VEVENT") {
                    events.push(map_vevent(child).map(|mut event| {
                        let utc_start_without_tzid =
                            find_prop(child, "DTSTART").is_some_and(|start| {
                                find_param(start, "TZID").is_none()
                                    && start.val.as_str().trim().ends_with('Z')
                            });
                        if !event.all_day
                            && event.rrule.is_none()
                            && event.timezone == "UTC"
                            && utc_start_without_tzid
                        {
                            if let Some(timezone) = &calendar_timezone {
                                event.timezone.clone_from(timezone);
                            }
                        }
                        event
                    }));
                } else if is_counted_skip(child.name.as_str()) {
                    skipped_components += 1;
                } else {
                    // VTIMEZONE and calendar-level metadata are ignored; no
                    // arbitrary component URLs or attachments are dereferenced.
                }
            }
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
        updated_at: parsed.dtstamp.unwrap_or(now),
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
    claim_token: Uuid,
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
                    .upsert_calendar_imported_event(
                        job.id,
                        &to_event(job, &parsed),
                        claim_token,
                        total,
                    )
                    .await;
                match result {
                    Ok(Some(_)) => record_upsert_result(&mut outcome, Ok(()), &mut last_error),
                    Ok(None) => return Err(IcalImportError::ClaimLost),
                    Err(error) => {
                        record_upsert_result(&mut outcome, Err(error.to_string()), &mut last_error)
                    }
                }
            }
            Err(message) => {
                outcome.failed_events += 1;
                last_error = Some(bounded_error(&message));
            }
        }
        dirty = true;

        if (index + 1) % PROGRESS_FLUSH_INTERVAL == 0 {
            flush_progress(
                metadata_store,
                job,
                claim_token,
                &outcome,
                last_error.as_deref(),
            )
            .await?;
            dirty = false;
        }
    }

    if dirty {
        flush_progress(
            metadata_store,
            job,
            claim_token,
            &outcome,
            last_error.as_deref(),
        )
        .await?;
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
    claim_token: Uuid,
    outcome: &ImportOutcome,
    last_error: Option<&str>,
) -> Result<(), IcalImportError> {
    metadata_store
        .update_calendar_import_job_progress(
            job.id,
            claim_token,
            outcome.total_events,
            outcome.processed_events,
            outcome.failed_events,
            last_error,
        )
        .await
        .map_err(|e| IcalImportError::Storage(e.to_string()))
        .and_then(|updated| updated.then_some(()).ok_or(IcalImportError::ClaimLost))
}

/// Worker entry point for one claimed job: read the spooled bytes, parse and
/// upsert, then move the job to `completed` (or `failed`). A completed run
/// publishes one `io.elembra.calendar.event.imported.v1` with counts and the
/// source ResourceRef (identifiers/counts only, never titles/descriptions).
pub async fn process_import_job(
    metadata_store: &MetadataStore,
    outbox: &OutboxStore,
    job: &CalendarImportJob,
) -> Result<ImportOutcome, IcalImportError> {
    if !matches!(job.status.as_str(), "pending" | "running") {
        return Err(IcalImportError::InvalidJobStatus(job.status.clone()));
    }
    let claim_token = job.claim_token.ok_or(IcalImportError::ClaimLost)?;

    let bytes = metadata_store
        .get_calendar_import_job_content(job.id)
        .await
        .map_err(|e| IcalImportError::Storage(e.to_string()))?
        .ok_or(IcalImportError::MissingContent)?;

    let result = async {
        if job.processed_events == 0 {
            if let Some(expected_digest) = metadata_store
                .get_calendar_import_job_preview_digest(job.id)
                .await
                .map_err(|e| IcalImportError::Storage(e.to_string()))?
            {
                let preview = preview_import(&bytes)?;
                let uids = preview
                    .iter()
                    .filter_map(|event| event.uid.clone())
                    .collect::<Vec<_>>();
                let existing = metadata_store
                    .list_calendar_import_matches(job.tenant_id, job.owner_id, job.source_id, &uids)
                    .await
                    .map_err(|e| IcalImportError::Storage(e.to_string()))?;
                if preview_digest(&bytes, job.source_id, &uids, &existing) != expected_digest {
                    return Err(IcalImportError::StalePreview);
                }
            }
        }
        parse_and_upsert(metadata_store, job, claim_token, &bytes).await
    }
    .await;
    match &result {
        Ok(outcome) => {
            if metadata_store
                .mark_calendar_import_job_completed(job.id, claim_token)
                .await
                .map_err(|e| IcalImportError::Storage(e.to_string()))?
            {
                crate::services::calendar_service::publish_imported_event(
                    outbox,
                    job.tenant_id,
                    job.owner_id,
                    job.source_id,
                    serde_json::json!({
                        "total_events": outcome.total_events,
                        "processed_events": outcome.processed_events,
                        "failed_events": outcome.failed_events,
                        "skipped_components": outcome.skipped_components,
                    }),
                )
                .await;
            } else {
                tracing::info!(
                    job_id = %job.id,
                    "Calendar import job finished but is no longer running; leaving status untouched and not publishing"
                );
            }
        }
        Err(error) => {
            let marked = metadata_store
                .mark_calendar_import_job_failed(
                    job.id,
                    claim_token,
                    &bounded_error(&error.to_string()),
                )
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
    fn preserves_utc_dtstamp_for_export_round_trip() {
        let ics = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:stamped\nDTSTAMP:20261118T120000Z\nDTSTART:20261119T090000Z\nDTEND:20261119T100000Z\nEND:VEVENT\nEND:VCALENDAR\n";
        let event = parse_file(ics).unwrap().events.remove(0).unwrap();
        assert_eq!(
            event.dtstamp.unwrap().to_rfc3339(),
            "2026-11-18T12:00:00+00:00"
        );
    }

    #[test]
    fn rejects_floating_times_and_custom_timezones() {
        let floating = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:floating\nDTSTART:20261119T090000\nDTEND:20261119T100000\nEND:VEVENT\nEND:VCALENDAR\n";
        assert!(parse_file(floating).unwrap().events[0]
            .as_ref()
            .unwrap_err()
            .contains("floating DATE-TIME"));

        let custom = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:custom-zone\nDTSTART;TZID=Custom/Zone:20261119T090000\nDTEND;TZID=Custom/Zone:20261119T100000\nEND:VEVENT\nEND:VCALENDAR\n";
        assert!(parse_file(custom).unwrap().events[0]
            .as_ref()
            .unwrap_err()
            .contains("unsupported TZID"));
    }

    #[test]
    fn rejects_conflicting_timezone_value_forms() {
        let utc_with_tzid = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:bad-utc-zone\nDTSTART;TZID=Europe/Berlin:20261119T090000Z\nDTEND;TZID=Europe/Berlin:20261119T100000Z\nEND:VEVENT\nEND:VCALENDAR\n";
        assert!(parse_file(utc_with_tzid).unwrap().events[0]
            .as_ref()
            .unwrap_err()
            .contains("UTC DATE-TIME must not include TZID"));

        let date_with_tzid = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:bad-date-zone\nDTSTART;VALUE=DATE;TZID=Europe/Berlin:20261119\nDTEND;VALUE=DATE;TZID=Europe/Berlin:20261120\nEND:VEVENT\nEND:VCALENDAR\n";
        assert!(parse_file(date_with_tzid).unwrap().events[0]
            .as_ref()
            .unwrap_err()
            .contains("all-day DATE values must not include TZID"));
    }

    #[test]
    fn rejects_ambiguous_time_and_identity_properties() {
        let both_ends = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:both-ends\nDTSTART:20261119T090000Z\nDTEND:20261119T100000Z\nDURATION:PT1H\nEND:VEVENT\nEND:VCALENDAR\n";
        assert!(parse_file(both_ends).unwrap().events[0]
            .as_ref()
            .unwrap_err()
            .contains("both DTEND and DURATION"));

        let duplicate_uid = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:first\nUID:second\nDTSTART:20261119T090000Z\nDTEND:20261119T100000Z\nEND:VEVENT\nEND:VCALENDAR\n";
        assert!(parse_file(duplicate_uid).unwrap().events[0]
            .as_ref()
            .unwrap_err()
            .contains("multiple UID"));
    }

    #[test]
    fn rejects_recurrence_overrides_and_long_properties() {
        let override_event = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:override\nDTSTART:20261119T090000Z\nDTEND:20261119T100000Z\nRECURRENCE-ID:20261119T090000Z\nEND:VEVENT\nEND:VCALENDAR\n";
        assert!(parse_file(override_event).unwrap().events[0]
            .as_ref()
            .unwrap_err()
            .contains("RECURRENCE-ID"));

        let long = format!(
            "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:long\nSUMMARY:{}\nDTSTART:20261119T090000Z\nDTEND:20261119T100000Z\nEND:VEVENT\nEND:VCALENDAR\n",
            "x".repeat(MAX_ICAL_PROPERTY_BYTES + 1)
        );
        assert!(parse_file(long.as_bytes()).is_err());
    }

    #[test]
    fn conflicting_duplicate_uids_are_not_confirmable() {
        let repeated = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:same\nDTSTART:20261119T090000Z\nDTEND:20261119T100000Z\nSUMMARY:Same\nEND:VEVENT\nBEGIN:VEVENT\nUID:same\nDTSTART:20261119T090000Z\nDTEND:20261119T100000Z\nSUMMARY:Same\nEND:VEVENT\nEND:VCALENDAR\n";
        assert_eq!(preview_import(repeated).unwrap()[1].outcome, "duplicate");

        let conflicting = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:same\nDTSTART:20261119T090000Z\nDTEND:20261119T100000Z\nSUMMARY:First\nEND:VEVENT\nBEGIN:VEVENT\nUID:same\nDTSTART:20261119T090000Z\nDTEND:20261119T100000Z\nSUMMARY:Second\nEND:VEVENT\nEND:VCALENDAR\n";
        assert_eq!(preview_import(conflicting).unwrap()[1].outcome, "conflict");
    }

    #[test]
    fn rejects_non_utf8_calendar_input() {
        assert!(parse_file(b"BEGIN:VCALENDAR\n\xff\nEND:VCALENDAR\n").is_err());
    }

    #[test]
    fn preserves_uid_text_whitespace_and_escapes() {
        let ics = "\
BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID: item\\,weekly\\;source@example.test\x20
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
END:VEVENT
END:VCALENDAR
";

        let parsed = parse_file(ics.as_bytes()).expect("parses");
        let event = parsed.events[0].as_ref().expect("event maps");

        assert_eq!(event.external_uid, " item,weekly;source@example.test ");
    }

    #[test]
    fn rejects_whitespace_only_uid() {
        let ics = "\
BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:\x20\x20
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
END:VEVENT
END:VCALENDAR
";

        let parsed = parse_file(ics.as_bytes()).expect("calendar structure parses");

        assert!(matches!(
            parsed.events[0],
            Err(ref message) if message == "VEVENT is missing UID"
        ));
    }

    #[test]
    fn reports_unsupported_event_fields_without_including_values() {
        let ics = "\
BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:unsupported-1
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
ORGANIZER:mailto:private@example.test
ATTENDEE;CN=Private:mailto:private@example.test
PRIORITY:1
X-CLIENT-DATA:private payload
BEGIN:X-CLIENT-COMPONENT
END:X-CLIENT-COMPONENT
END:VEVENT
END:VCALENDAR
";

        let parsed = parse_file(ics.as_bytes()).expect("calendar structure parses");
        let error = parsed.events[0]
            .as_ref()
            .expect_err("unsupported fields fail the event");

        assert!(error.contains("ATTENDEE, ORGANIZER"), "error was {error}");
        assert!(
            error.contains("unknown property PRIORITY"),
            "error was {error}"
        );
        assert!(
            error.contains("unknown property X-CLIENT-DATA"),
            "error was {error}"
        );
        assert!(
            error.contains("unsupported component X-CLIENT-COMPONENT"),
            "error was {error}"
        );
        assert!(
            error.contains("event was not imported"),
            "error was {error}"
        );
        assert!(!error.contains("private@example.test"));
    }

    #[test]
    fn rejects_recurrence_id_range_without_including_parameter_value() {
        let ics = "\
BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:range-master
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
RRULE:FREQ=WEEKLY;COUNT=4
END:VEVENT
BEGIN:VEVENT
UID:range-master
RECURRENCE-ID:20261012T140000Z
RECURRENCE-ID;range=thisandfuture:20261012T140000Z
DTSTART:20261012T160000Z
DTEND:20261012T170000Z
END:VEVENT
END:VCALENDAR
";

        let parsed = parse_file(ics.as_bytes()).expect("calendar structure parses");
        let error = parsed.events[1]
            .as_ref()
            .expect_err("duplicate and ranged recurrence IDs are unsupported");

        assert!(
            error.contains("multiple RECURRENCE-ID properties"),
            "error was {error}"
        );
        assert!(error.contains("RECURRENCE-ID RANGE"), "error was {error}");
        assert!(
            error.contains("event was not imported"),
            "error was {error}"
        );
        assert!(!error.contains("thisandfuture"));
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
    fn rejects_recurrence_overrides_in_supported_imports() {
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
        let parsed = parse_file(ics.as_bytes()).expect("calendar parses");
        assert_eq!(parsed.events.len(), 2);
        let master = parsed.events[0].as_ref().expect("master maps");
        assert_eq!(master.rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO"));
        assert!(master.recurrence_id.is_none());
        assert!(parsed.events[1]
            .as_ref()
            .unwrap_err()
            .contains("RECURRENCE-ID"));
    }

    #[test]
    fn malformed_component_and_valarm_are_reported_per_event() {
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
        assert!(parsed.events[0].as_ref().unwrap_err().contains("VALARM"));
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
    fn too_many_events_fails_before_component_parsing() {
        let mut ics = String::from("BEGIN:VCALENDAR\n");
        for index in 0..=MAX_CALENDAR_IMPORT_EVENTS {
            if index == MAX_CALENDAR_IMPORT_EVENTS / 2 {
                // The limit applies to the whole upload, not per concatenated
                // VCALENDAR root.
                ics.push_str("END:VCALENDAR\nBEGIN:VCALENDAR\n");
            }
            ics.push_str("BEGIN:VEVENT\nEND:VEVENT\n");
        }
        ics.push_str("END:VCALENDAR\n");

        let result = parse_file(ics.as_bytes());
        assert!(matches!(
            result,
            Err(IcalImportError::Unparseable(message))
                if message == format!("calendar exceeds the {MAX_CALENDAR_IMPORT_EVENTS}-VEVENT import limit")
        ));
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

    #[test]
    fn text_values_are_not_double_unescaped() {
        // The parser already turns `\\` into a literal backslash and `\n` into
        // a newline, so a literal backslash must survive verbatim instead of
        // being unescaped a second time.
        let ics = r"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:escape-1
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
SUMMARY:path\\new file
DESCRIPTION:C:\\temp\\notes
LOCATION:C:\\temp
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        let event = parsed.events[0].as_ref().expect("event maps");
        assert_eq!(event.title, "path\\new file");
        assert_eq!(event.description.as_deref(), Some("C:\\temp\\notes"));
        assert_eq!(event.location.as_deref(), Some("C:\\temp"));
    }

    #[test]
    fn escaped_newline_still_decodes_once() {
        let ics = r"BEGIN:VCALENDAR
BEGIN:VEVENT
UID:escape-2
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
SUMMARY:line one\nline two
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        let event = parsed.events[0].as_ref().expect("event maps");
        assert_eq!(event.title, "line one\nline two");
    }

    #[test]
    fn concatenated_vcalendars_both_import() {
        let first = "\
BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:block-one-1
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
SUMMARY:Block one
END:VEVENT
END:VCALENDAR
";
        let second = "\
BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:block-two-1
DTSTART:20261006T140000Z
DTEND:20261006T150000Z
SUMMARY:Block two
END:VEVENT
END:VCALENDAR
";
        let ics = format!("{first}{second}");
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        assert_eq!(parsed.events.len(), 2);
        let titles: Vec<&str> = parsed
            .events
            .iter()
            .map(|event| event.as_ref().expect("event maps").title.as_str())
            .collect();
        assert_eq!(titles, vec!["Block one", "Block two"]);
    }

    #[test]
    fn bom_prefixed_calendar_imports() {
        let ics = "\u{feff}BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:bom-1
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
SUMMARY:Byte order mark
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        let event = parsed.events[0].as_ref().expect("event maps");
        assert_eq!(event.external_uid, "bom-1");
        assert_eq!(event.title, "Byte order mark");
    }

    #[test]
    fn unknown_tzid_without_vtimezone_is_a_component_failure() {
        let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:unknown-tzid-1
DTSTART;TZID=Nowhere Standard Time:20261005T140000
DTEND;TZID=Nowhere Standard Time:20261005T150000
SUMMARY:Unknown zone
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        let error = parsed.events[0].as_ref().unwrap_err();
        assert!(
            error.contains("unsupported TZID"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn dst_overlap_keeps_the_earliest_occurrence() {
        // Europe/Berlin 2026-10-25 02:30 occurs twice; keep the first (CEST).
        let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:dst-overlap-1
DTSTART;TZID=Europe/Berlin:20261025T023000
DTEND;TZID=Europe/Berlin:20261025T033000
SUMMARY:Fall back
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        let event = parsed.events[0].as_ref().expect("event maps");
        assert_eq!(event.starts_at.to_rfc3339(), "2026-10-25T00:30:00+00:00");
    }

    #[test]
    fn dst_gap_pushes_forward() {
        // Europe/Berlin 2026-03-29 02:30 does not exist; push to 03:00 CEST.
        let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:dst-gap-1
DTSTART;TZID=Europe/Berlin:20260329T023000
DTEND;TZID=Europe/Berlin:20260329T033000
SUMMARY:Spring forward
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        let event = parsed.events[0].as_ref().expect("event maps");
        assert_eq!(event.starts_at.to_rfc3339(), "2026-03-29T01:00:00+00:00");
    }

    #[test]
    fn ignores_vtimezone_and_rejects_valarm_without_counting_skipped() {
        let ics = "\
BEGIN:VCALENDAR
BEGIN:VTIMEZONE
TZID:Europe/Berlin
BEGIN:STANDARD
DTSTART:19701025T030000
TZOFFSETFROM:+0200
TZOFFSETTO:+0100
END:STANDARD
END:VTIMEZONE
BEGIN:VEVENT
UID:ignored-components-1
DTSTART:20261005T140000Z
DTEND:20261005T150000Z
BEGIN:VALARM
TRIGGER:-PT15M
END:VALARM
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        assert_eq!(parsed.events.len(), 1);
        assert!(parsed.events[0].as_ref().unwrap_err().contains("VALARM"));
        assert_eq!(parsed.skipped_components, 0);
    }

    #[test]
    fn duration_supplies_end_when_dtend_absent() {
        let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:duration-1
DTSTART:20261005T140000Z
DURATION:PT2H30M
SUMMARY:Two and a half hours
END:VEVENT
BEGIN:VEVENT
UID:duration-2
DTSTART;VALUE=DATE:20261005
DURATION:P2D
SUMMARY:Two days
END:VEVENT
BEGIN:VEVENT
UID:duration-3
DTSTART:20261005T140000Z
SUMMARY:One hour default
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        let timed = parsed.events[0].as_ref().expect("event maps");
        assert_eq!(timed.ends_at.to_rfc3339(), "2026-10-05T16:30:00+00:00");
        let all_day = parsed.events[1].as_ref().expect("event maps");
        assert_eq!(all_day.ends_at.to_rfc3339(), "2026-10-07T00:00:00+00:00");
        assert!(all_day.all_day);
        let default = parsed.events[2].as_ref().expect("event maps");
        assert_eq!(default.ends_at.to_rfc3339(), "2026-10-05T15:00:00+00:00");
    }

    #[test]
    fn parses_rfc5545_durations() {
        assert_eq!(parse_duration("P1D").unwrap(), Duration::days(1));
        assert_eq!(parse_duration("P2W").unwrap(), Duration::days(14));
        assert_eq!(
            parse_duration("P1DT2H3M4S").unwrap(),
            Duration::days(1) + Duration::hours(2) + Duration::minutes(3) + Duration::seconds(4)
        );
        assert_eq!(parse_duration("-PT15M").unwrap(), Duration::minutes(-15));
        assert_eq!(parse_duration("PT0S").unwrap(), Duration::seconds(0));
        assert!(parse_duration("1 day").is_err());
    }

    #[test]
    fn out_of_range_durations_are_rejected() {
        // Magnitudes that would overflow intermediate math or panic `chrono`.
        assert!(parse_duration("P1000000000000D").is_err());
        assert!(parse_duration("P99999999999999999999D").is_err());
        assert!(parse_duration("PT9223372036854775807S").is_err());
        // Reasonable magnitudes still parse, including negative spans.
        assert_eq!(parse_duration("P3650D").unwrap(), Duration::days(3650));
        assert_eq!(parse_duration("-P365D").unwrap(), Duration::days(-365));
    }

    #[test]
    fn huge_duration_is_a_component_failure_not_a_panic() {
        let ics = "\
BEGIN:VCALENDAR
BEGIN:VEVENT
UID:huge-duration-1
DTSTART:20261005T140000Z
DURATION:P1000000000000D
SUMMARY:Absurd duration
END:VEVENT
END:VCALENDAR
";
        let parsed = parse_file(ics.as_bytes()).expect("parses");
        let error = parsed.events[0].as_ref().unwrap_err();
        assert!(error.contains("out of range"), "unexpected error: {error}");
    }
}
