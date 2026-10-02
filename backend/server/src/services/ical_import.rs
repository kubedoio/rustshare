//! RFC 5545 (.ics) import: parse uploaded calendar files and upsert the
//! contained VEVENTs into `calendar_events` (issue #315, Task 2).
//!
//! Import semantics are normative in `docs/specs/calendar-application-v1alpha1.md`
//! §Import semantics: upsert on `(source_id, UID, RECURRENCE-ID)`, TZID
//! converted to UTC via the embedded VTIMEZONE or the system tz database
//! (floating times are UTC), all-day DATE values become UTC-midnight spans,
//! RRULE stored verbatim, VALARM discarded, VTODO/VJOURNAL/VFREEBUSY counted
//! as skipped, per-component failures counted with a bounded `last_error`
//! sample, and a structurally unreadable file fails the whole job.
//!
//! Non-IANA TZIDs (for example the Windows zone names Outlook emits) are
//! resolved from the file's embedded VTIMEZONE definitions, including their
//! yearly STANDARD/DAYLIGHT transition rules. DST transitions are resolved
//! conservatively: ambiguous local times (fall-back overlap) keep the earliest
//! occurrence and nonexistent local times (spring-forward gap) are pushed
//! forward to the first valid instant, matching the expansion semantics in the
//! spec.

use std::collections::HashMap;

use chrono::{
    DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc,
    Weekday,
};
use icalendar::parser::{read_components, Component, Property};
use rustshare_core::domain::{CalendarEvent, CalendarImportJob};
use rustshare_storage::{MetadataStore, OutboxStore};
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

/// One `VTIMEZONE` definition extracted from the file, keyed by its `TZID`.
#[derive(Debug, Clone)]
struct VTimezoneDef {
    tzid: String,
    observances: Vec<Observance>,
}

/// A single STANDARD/DAYLIGHT observance inside a `VTIMEZONE`.
#[derive(Debug, Clone)]
struct Observance {
    /// Offset that takes effect at the transition (`TZOFFSETTO`), seconds east.
    offset_to: i32,
    /// Offset in effect before the transition (`TZOFFSETFROM`), seconds east.
    offset_from: i32,
    dtstart: NaiveDateTime,
    rule: Option<YearlyRule>,
    rdates: Vec<NaiveDateTime>,
}

/// The subset of a yearly RRULE needed to place one observance in a year.
#[derive(Debug, Clone)]
struct YearlyRule {
    month: u32,
    by_day: Option<(i32, Weekday)>,
    by_month_day: Option<u32>,
}

type VTimezoneIndex = HashMap<String, VTimezoneDef>;

/// Parse an RFC 5545 UTC offset such as `+0200` or `-053000` into seconds east.
fn parse_utc_offset(raw: &str) -> Result<i32, String> {
    let raw = raw.trim();
    let (sign, rest) = match raw.strip_prefix('-') {
        Some(rest) => (-1i32, rest),
        None => (1i32, raw.strip_prefix('+').unwrap_or(raw)),
    };
    let (hours, minutes, seconds) = match rest.len() {
        4 => (&rest[0..2], &rest[2..4], "00"),
        6 => (&rest[0..2], &rest[2..4], &rest[4..6]),
        _ => return Err(format!("invalid UTC offset '{raw}'")),
    };
    let hours: i32 = hours
        .parse()
        .map_err(|_| format!("invalid UTC offset '{raw}'"))?;
    let minutes: i32 = minutes
        .parse()
        .map_err(|_| format!("invalid UTC offset '{raw}'"))?;
    let seconds: i32 = seconds
        .parse()
        .map_err(|_| format!("invalid UTC offset '{raw}'"))?;
    Ok(sign * (hours * 3600 + minutes * 60 + seconds))
}

/// Parse a floating (or `Z`-suffixed) DATE-TIME used inside a `VTIMEZONE`.
fn parse_local_datetime(value: &str) -> Result<NaiveDateTime, String> {
    let value = value.trim();
    let naive = value.strip_suffix('Z').unwrap_or(value);
    NaiveDateTime::parse_from_str(naive, "%Y%m%dT%H%M%S")
        .map_err(|e| format!("invalid local DATE-TIME '{value}': {e}"))
}

fn parse_weekday(raw: &str) -> Option<Weekday> {
    match raw.to_ascii_uppercase().as_str() {
        "MO" => Some(Weekday::Mon),
        "TU" => Some(Weekday::Tue),
        "WE" => Some(Weekday::Wed),
        "TH" => Some(Weekday::Thu),
        "FR" => Some(Weekday::Fri),
        "SA" => Some(Weekday::Sat),
        "SU" => Some(Weekday::Sun),
        _ => None,
    }
}

/// Parse the yearly portion of a `VTIMEZONE` RRULE. Only `FREQ=YEARLY` with
/// `BYMONTH` plus `BYDAY`/`BYMONTHDAY` (or the DTSTART day) is understood, which
/// covers the forms Outlook, Google, and Apple emit. Anything else is ignored
/// and the observance falls back to its DTSTART.
fn parse_yearly_rule(raw: &str) -> Option<YearlyRule> {
    let mut month = None;
    let mut by_day = None;
    let mut by_month_day = None;
    for part in raw.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                if !value.trim().eq_ignore_ascii_case("YEARLY") {
                    return None;
                }
            }
            "BYMONTH" => {
                month = value.trim().split(',').next()?.parse::<u32>().ok();
            }
            "BYMONTHDAY" => {
                by_month_day = value.trim().split(',').next()?.parse::<u32>().ok();
            }
            "BYDAY" => {
                let token = value.trim().split(',').next()?;
                let split = token
                    .find(|c: char| c.is_ascii_alphabetic())
                    .unwrap_or(token.len());
                let (ordinal, weekday) = token.split_at(split);
                let ordinal: i32 = ordinal.parse().unwrap_or(1);
                by_day = parse_weekday(weekday).map(|weekday| (ordinal, weekday));
            }
            _ => {}
        }
    }
    Some(YearlyRule {
        month: month?,
        by_day,
        by_month_day,
    })
}

fn parse_observance(component: &Component<'_>) -> Option<Observance> {
    let offset_to = parse_utc_offset(find_prop(component, "TZOFFSETTO")?.val.as_str()).ok()?;
    let offset_from = find_prop(component, "TZOFFSETFROM")
        .and_then(|prop| parse_utc_offset(prop.val.as_str()).ok())
        .unwrap_or(offset_to);
    let dtstart = parse_local_datetime(find_prop(component, "DTSTART")?.val.as_str()).ok()?;
    let rule = find_prop(component, "RRULE").and_then(|prop| parse_yearly_rule(prop.val.as_str()));
    let rdates = find_prop(component, "RDATE")
        .map(|prop| {
            prop.val
                .as_str()
                .split(',')
                .filter_map(|value| parse_local_datetime(value).ok())
                .collect()
        })
        .unwrap_or_default();
    Some(Observance {
        offset_to,
        offset_from,
        dtstart,
        rule,
        rdates,
    })
}

fn parse_vtimezone(component: &Component<'_>) -> Option<VTimezoneDef> {
    let tzid = find_prop(component, "TZID")?
        .val
        .as_str()
        .trim()
        .to_string();
    if tzid.is_empty() {
        return None;
    }
    let observances = component
        .components
        .iter()
        .filter(|child| {
            child.name.as_str().eq_ignore_ascii_case("STANDARD")
                || child.name.as_str().eq_ignore_ascii_case("DAYLIGHT")
        })
        .filter_map(parse_observance)
        .collect::<Vec<_>>();
    if observances.is_empty() {
        return None;
    }
    Some(VTimezoneDef { tzid, observances })
}

/// The n-th (or, for a negative ordinal, last-n) weekday of a month.
fn nth_weekday_of_month(
    year: i32,
    month: u32,
    ordinal: i32,
    weekday: Weekday,
) -> Option<NaiveDate> {
    if ordinal > 0 {
        let first = NaiveDate::from_ymd_opt(year, month, 1)?;
        let delta =
            (weekday.num_days_from_monday() + 7 - first.weekday().num_days_from_monday()) % 7;
        NaiveDate::from_ymd_opt(year, month, 1 + delta + (ordinal as u32 - 1) * 7)
    } else {
        let next_month = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)?
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)?
        };
        let last = next_month.pred_opt()?;
        let delta =
            (last.weekday().num_days_from_monday() + 7 - weekday.num_days_from_monday()) % 7;
        let day = last.day() as i32 - delta as i32 - ((-ordinal) - 1) * 7;
        NaiveDate::from_ymd_opt(year, month, day as u32)
    }
}

/// Local transition instants for one observance in a given year.
fn observance_occurrences(observance: &Observance, year: i32) -> Vec<NaiveDateTime> {
    let time = observance.dtstart.time();
    let mut occurrences = Vec::new();
    if observance.dtstart.year() == year {
        occurrences.push(observance.dtstart);
    }
    occurrences.extend(
        observance
            .rdates
            .iter()
            .filter(|date| date.year() == year)
            .copied(),
    );
    if let Some(rule) = &observance.rule {
        let date = if let Some((ordinal, weekday)) = rule.by_day {
            nth_weekday_of_month(year, rule.month, ordinal, weekday)
        } else if let Some(day) = rule.by_month_day {
            NaiveDate::from_ymd_opt(year, rule.month, day)
        } else {
            NaiveDate::from_ymd_opt(year, rule.month, observance.dtstart.day())
        };
        if let Some(date) = date {
            if let Some(instant) = date.and_hms_opt(time.hour(), time.minute(), time.second()) {
                occurrences.push(instant);
            }
        }
    }
    occurrences
}

/// Resolve a local wall-clock time against an embedded `VTIMEZONE`, applying the
/// observance transition rules. Overlaps (fall-back) keep the earliest
/// occurrence and gaps (spring-forward) push forward to the transition instant.
fn resolve_vtimezone(def: &VTimezoneDef, naive: NaiveDateTime) -> Result<DateTime<Utc>, String> {
    let year = naive.year();
    let mut transitions: Vec<(NaiveDateTime, i32, i32)> = Vec::new();
    for observance in &def.observances {
        for occurrence in observance_occurrences(observance, year - 1)
            .into_iter()
            .chain(observance_occurrences(observance, year))
        {
            transitions.push((occurrence, observance.offset_from, observance.offset_to));
        }
    }
    transitions.sort_by_key(|(local, _, _)| *local);

    if let Some(&(local, offset_from, offset_to)) = transitions
        .iter()
        .rev()
        .find(|(local, _, _)| *local <= naive)
    {
        if offset_from < offset_to {
            let gap = Duration::seconds((offset_to - offset_from) as i64);
            if naive < local + gap {
                // Nonexistent local time: the transition instant itself.
                return Ok((local - Duration::seconds(offset_from as i64)).and_utc());
            }
        } else if offset_from > offset_to {
            let overlap = Duration::seconds((offset_from - offset_to) as i64);
            if naive < local + overlap {
                // Repeated local time: keep the earliest occurrence.
                return Ok((naive - Duration::seconds(offset_from as i64)).and_utc());
            }
        }
        return Ok((naive - Duration::seconds(offset_to as i64)).and_utc());
    }

    // Before the first known transition, use the earliest observance's starting
    // offset (`TZOFFSETFROM`).
    let earliest = def
        .observances
        .iter()
        .min_by_key(|observance| observance.dtstart)
        .ok_or_else(|| format!("VTIMEZONE '{}' has no observances", def.tzid))?;
    Ok((naive - Duration::seconds(earliest.offset_from as i64)).and_utc())
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
            'W' => seconds += take(&mut number, raw)? * 7 * 86_400,
            'D' => seconds += take(&mut number, raw)? * 86_400,
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
                'H' => seconds += take(&mut number, raw)? * 3600,
                'M' => seconds += take(&mut number, raw)? * 60,
                'S' => seconds += take(&mut number, raw)?,
                _ => return Err(format!("invalid DURATION '{raw}'")),
            }
        }
        if !number.is_empty() {
            return Err(format!("invalid DURATION '{raw}'"));
        }
    }
    Ok(Duration::seconds(sign * seconds))
}

/// Parse one DTSTART/DTEND/RECURRENCE-ID value into a UTC instant plus the
/// IANA timezone it was expressed in (`UTC` for floating/UTC values).
///
/// Returns `all_day = true` for DATE values; those become UTC-midnight spans
/// with `original_date` preserved.
fn parse_date_time(
    prop: &Property<'_>,
    timezones: &VTimezoneIndex,
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
    let naive = NaiveDateTime::parse_from_str(naive_value, "%Y%m%dT%H%M%S")
        .map_err(|e| format!("invalid DATE-TIME value '{value}': {e}"))?;

    match tzid {
        Some(tzid) => {
            if let Ok(tz) = tzid.parse::<chrono_tz::Tz>() {
                let start = resolve_iana_local(&tz, naive);
                return Ok((start, tzid.to_string(), false, None));
            }
            let definition = timezones
                .get(tzid)
                .or_else(|| {
                    timezones
                        .iter()
                        .find(|(key, _)| key.eq_ignore_ascii_case(tzid))
                        .map(|(_, value)| value)
                })
                .ok_or_else(|| format!("unknown TZID '{tzid}'"))?;
            let start = resolve_vtimezone(definition, naive)
                .map_err(|e| format!("cannot resolve TZID '{tzid}': {e}"))?;
            Ok((start, tzid.to_string(), false, None))
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

fn map_vevent(
    component: &Component<'_>,
    timezones: &VTimezoneIndex,
) -> Result<ParsedEvent, String> {
    let uid = find_prop(component, "UID")
        .map(|prop| prop.val.as_str().trim().to_string())
        .filter(|uid| !uid.is_empty())
        .ok_or("VEVENT is missing UID")?;

    let dtstart_prop = find_prop(component, "DTSTART").ok_or("VEVENT is missing DTSTART")?;
    let (starts_at, timezone, all_day, original_date) = parse_date_time(dtstart_prop, timezones)?;

    let (ends_at, _, _, _) = match find_prop(component, "DTEND") {
        Some(dtend) => parse_date_time(dtend, timezones)?,
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
            (starts_at + span, timezone.clone(), all_day, original_date)
        }
    };

    let recurrence_id = find_prop(component, "RECURRENCE-ID")
        .map(|prop| parse_date_time(prop, timezones))
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
fn split_top_level_components(unfolded: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut depth: i32 = 0;
    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim_start();
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
    blocks
}

/// Parse the file content. A structurally unreadable file is a hard error
/// (fails the job); malformed individual components come back as `Err`
/// entries inside `events`.
fn parse_file(bytes: &[u8]) -> Result<ParsedFile, IcalImportError> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let unfolded = icalendar::parser::unfold(text);

    let blocks = split_top_level_components(&unfolded);
    if blocks.is_empty() {
        return Err(IcalImportError::Unparseable(
            "file contains no calendar components".to_string(),
        ));
    }

    let mut events = Vec::new();
    let mut skipped_components = 0i32;
    let mut ignored_components = 0i32;

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
            let mut timezones: VTimezoneIndex = HashMap::new();
            for child in &children {
                if child.name.as_str().eq_ignore_ascii_case("VTIMEZONE") {
                    if let Some(definition) = parse_vtimezone(child) {
                        timezones.insert(definition.tzid.clone(), definition);
                    }
                }
            }

            for child in &children {
                if child.name.as_str().eq_ignore_ascii_case("VEVENT") {
                    events.push(map_vevent(child, &timezones));
                } else if is_counted_skip(child.name.as_str()) {
                    skipped_components += 1;
                } else {
                    // VTIMEZONE, VALARM, and unknown components are ignored
                    // silently (and are not counted as skipped).
                    ignored_components += 1;
                }
            }
        }
    }

    if events.is_empty() && skipped_components == 0 && ignored_components == 0 {
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

    let bytes = metadata_store
        .get_calendar_import_job_content(job.id)
        .await
        .map_err(|e| IcalImportError::Storage(e.to_string()))?
        .ok_or(IcalImportError::MissingContent)?;

    let result = parse_and_upsert(metadata_store, job, &bytes).await;
    match &result {
        Ok(outcome) => {
            if metadata_store
                .mark_calendar_import_job_completed(job.id)
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

    const OUTLOOK_TZID_ICS: &str = "\
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Microsoft Corporation//Outlook 16.0 MIMEDIR//EN
BEGIN:VTIMEZONE
TZID:W. Europe Standard Time
BEGIN:STANDARD
DTSTART:16011028T030000
TZOFFSETFROM:+0200
TZOFFSETTO:+0100
RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU
END:STANDARD
BEGIN:DAYLIGHT
DTSTART:16010325T020000
TZOFFSETFROM:+0100
TZOFFSETTO:+0200
RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU
END:DAYLIGHT
END:VTIMEZONE
BEGIN:VEVENT
UID:outlook-summer-1
DTSTART;TZID=W. Europe Standard Time:20260715T140000
DTEND;TZID=W. Europe Standard Time:20260715T150000
SUMMARY:Summer meeting
END:VEVENT
BEGIN:VEVENT
UID:outlook-winter-1
DTSTART;TZID=W. Europe Standard Time:20260115T140000
DTEND;TZID=W. Europe Standard Time:20260115T150000
SUMMARY:Winter meeting
END:VEVENT
END:VCALENDAR
";

    #[test]
    fn outlook_windows_tzid_resolved_from_embedded_vtimezone() {
        let parsed = parse_file(OUTLOOK_TZID_ICS.as_bytes()).expect("parses");
        assert_eq!(parsed.skipped_components, 0);
        assert_eq!(parsed.events.len(), 2);
        let summer = parsed.events[0].as_ref().expect("summer maps");
        // 2026-07-15 is CEST (UTC+2).
        assert_eq!(summer.starts_at.to_rfc3339(), "2026-07-15T12:00:00+00:00");
        assert_eq!(summer.timezone, "W. Europe Standard Time");
        let winter = parsed.events[1].as_ref().expect("winter maps");
        // 2026-01-15 is CET (UTC+1).
        assert_eq!(winter.starts_at.to_rfc3339(), "2026-01-15T13:00:00+00:00");
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
        assert!(error.contains("unknown TZID"), "unexpected error: {error}");
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
    fn ignores_vtimezone_and_valarm_without_counting_skipped() {
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
}
