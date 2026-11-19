# H2.1 iCalendar import support

This matrix defines the import contract for the bounded Calendar import flow.
Accepted values must remain readable from RustShare's existing `CalendarEvent`
model and single-event export. Unsupported event components are reported in
preview and are not silently flattened.

| Construct | H2.1 behavior |
| --- | --- |
| `VEVENT` identity | Preserve required `UID`; key by destination source, UID, and recurrence identifier. |
| `DTSTAMP` | Preserve a valid UTC value as the stored update timestamp; missing values use the import time. |
| `CREATED`, `LAST-MODIFIED`, `SEQUENCE` | Report as unsupported because the event model cannot retain them independently. |
| `SUMMARY`, `DESCRIPTION`, `LOCATION` | Preserve as plain untrusted text; no HTML rendering or remote fetch. |
| `STATUS` | Preserve `CONFIRMED`, `TENTATIVE`, `CANCELLED`; other values use RFC default `CONFIRMED`. |
| `DTSTART` / `DTEND` | Support UTC DATE-TIME, IANA `TZID` DATE-TIME for non-recurring events, and DATE all-day spans. Support `DURATION` when `DTEND` is absent. |
| Floating DATE-TIME | Reject: assigning UTC would change its unspecified local-time meaning. |
| `RRULE` | Support only a rule parsed by the existing recurrence engine, on UTC events. Rule length and recurrence expansion are bounded. |
| `RECURRENCE-ID`, `RDATE`, `EXDATE` | Reject the event: the current model/export cannot preserve exception-set semantics. |
| Embedded/custom `VTIMEZONE` and non-IANA `TZID` | Reject events using these zones with a diagnostic; the current model/export cannot preserve the transition definition. |
| `ATTACH`, `URL`, organizer/attendee, alarms, categories, classification, GEO, resources, transparency, conference, relations | Report unsupported and do not import the affected event. Never fetch referenced resources. |
| `VTODO`, `VJOURNAL`, `VFREEBUSY` | Report as unsupported components; do not import. |
| Unknown properties/components | Ignore calendar-level metadata; report and skip an event containing an unrecognized event property/component. |

The RFC 5545 parser remains responsible for syntax. Import-specific limits cap
the file at 10 MiB, VEVENT count at 1,000, unfolded/property lengths, each
recurrence rule, and preview/confirmation parsing at 10 seconds per request.
Existing export semantics are part of this contract: all-day
DATE values and single UTC-recurring events round-trip; non-recurring IANA
events export their exact UTC instants and the display TZID. No claim is made
for recurrence overrides, non-UTC recurrence, custom timezones, alarms, or
participant/attachment metadata.
