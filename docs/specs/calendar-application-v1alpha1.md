# Specification: Elembra Calendar Application v1alpha1

Status: Draft  
Date: 2026-10-01  
ADR: `docs/adr/0037-calendar-application-and-external-sync.md`  
API contract: `docs/contracts/calendar-application-api.md`  
Issue: #315

## Purpose

Define the Elembra Calendar Application: an Embedded first-party Application
that provides (a) Elembra-native calendar events, (b) one-shot iCal/.ics
import, and (c) read-only synchronization from Google Calendar and
Microsoft/Outlook.

This specification follows the Application manifest contract
(`application-manifest-v1alpha1.md`), consumes the connector state model of
`connector-contract-v1alpha1.md` without implementing a Connector runtime, and
publishes integration events per `integration-event-v1alpha1.md` and
`0031-durable-integration-events.md`.

## Application identity

```yaml
apiVersion: elembra.io/v1alpha1
kind: Application
metadata:
  id: io.elembra.calendar
  name: Calendar
  version: 1.0.0
  description: Elembra Calendar Application

runtime:
  kind: embedded

contracts:
  provides:
    - id: io.elembra.calendar.api
      version: v1alpha1
  requires: []

resources:
  - type: calendar.event
    actions:
      - calendar.read
      - calendar.write
      - calendar.delete
  - type: calendar.source
    actions:
      - calendar.read
      - calendar.write

contributions:
  navigation:
    - id: calendar.navigation
      label: Calendar
      icon: calendar-days
      route: /apps/calendar
      order: 40
  routes:
    - id: calendar.route
      route: /apps/calendar
      renderer: calendar
  settings:
    - id: calendar.settings
      route: /settings/apps/calendar

integrationEvents:
  publishes:
    - io.elembra.calendar.event.created.v1
    - io.elembra.calendar.event.updated.v1
    - io.elembra.calendar.event.deleted.v1
    - io.elembra.calendar.event.imported.v1
  subscribes: []

memory: null

configuration:
  schema: contracts/io.elembra.calendar/config-v1alpha1.schema.json

data:
  owner: io.elembra.calendar
  preserveOnDisable: true
  exportSupported: true

health: null
```

The manifest is code-owned in `first_party_manifests()`
(`backend/crates/core/src/domain/application.rs`); tenant configuration only
enables/disables it. All `/api/v1/calendar/...` JSON API routes fail closed
with 403 when the Application is disabled for the tenant (the OAuth callback
and ICS feed are the documented exceptions).

## Visibility model

v1 calendar data is **owner-only**:

- every row carries `tenant_id` and `owner_id`;
- reads and writes are scoped to the authenticated principal;
- cross-user access returns 404 (existence is not leaked);
- workspace-shared calendars, delegation, and public share links are
  out of scope (see Non-goals).

External-source events keep their provider semantics: a Google/Outlook event
visible to the user remotely becomes visible only to that same user in
Elembra.

## Data model

Table names avoid the existing `events` domain-event store. All tables follow
the repository migration conventions (`id UUID DEFAULT gen_random_uuid()`,
`tenant_id UUID NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'`,
soft delete via `deleted_at`, CHECK-constrained enums, partial indexes on
`deleted_at IS NULL`).

### `calendar_sources`

One row per connected source. The implicit internal source is materialized as
a row with `kind = 'internal'` so events always reference a source.

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | |
| `tenant_id` | UUID | default tenant constant |
| `owner_id` | UUID | `REFERENCES users(id) ON DELETE CASCADE` |
| `kind` | VARCHAR(20) | CHECK `('internal','ical_import','google','outlook')` |
| `display_name` | TEXT | user label, e.g. "Work Google" |
| `external_account` | TEXT | provider account label (email); nullable |
| `external_calendar_id` | TEXT | provider calendar id; nullable for internal/ical_import |
| `refresh_token_enc` | TEXT | AES-256-GCM via `SecretEncryptionKey`; NULL for internal/ical_import |
| `access_token_enc` | TEXT | short-lived cache; NULL allowed |
| `access_token_expires_at` | TIMESTAMPTZ | NULL allowed |
| `scopes` | TEXT | space-separated granted scopes; nullable |
| `is_enabled` | BOOLEAN | default true; disabled sources are not synced |
| `last_synced_at` | TIMESTAMPTZ | |
| `last_error` | TEXT | safe summary only; never tokens or payloads |
| `status` | VARCHAR(20) | CHECK `('healthy','degraded','auth_required','rate_limited','paused','failed')` per connector-contract health |
| `deleted_at` | TIMESTAMPTZ | soft delete; deleting a source soft-deletes its mirrored events |
| `created_at` / `updated_at` | TIMESTAMPTZ | |

Partial unique index: one `internal` source per `(tenant_id, owner_id)`; one
active row per `(owner_id, kind, external_account, external_calendar_id)` for
external kinds.

### `calendar_events`

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | |
| `tenant_id` | UUID | |
| `owner_id` | UUID | |
| `source_id` | UUID | `REFERENCES calendar_sources(id)` |
| `external_uid` | TEXT | iCalendar UID / provider event id; NULL only for unsynced internal drafts |
| `external_etag` | TEXT | provider ETag/change key for cheap change detection; nullable |
| `recurrence_id` | TEXT | RECURRENCE-ID for overridden instances; NULL for the master. Override rows are returned as stored (own `id`); master expansion omits occurrences covered by an override row in the window |
| `title` | TEXT | |
| `description` | TEXT | nullable |
| `location` | TEXT | nullable |
| `starts_at` | TIMESTAMPTZ | UTC-normalized |
| `ends_at` | TIMESTAMPTZ | UTC-normalized; `ends_at > starts_at` |
| `all_day` | BOOLEAN | default false; all-day events stored as UTC midnight spans with the original date kept in `original_date` |
| `original_date` | DATE | original DTSTART date for all-day events; nullable |
| `timezone` | TEXT | IANA TZID of the wall-clock time; default `'UTC'` |
| `rrule` | TEXT | raw RRULE string for recurring masters; nullable. Expansion happens at read time, bounded (see Range queries) |
| `status` | VARCHAR(20) | CHECK `('confirmed','tentative','cancelled')`; cancelled mirrors are retained as tombstones until source deletion |
| `read_only` | BOOLEAN | true for all non-internal sources; mirrored events reject writes |
| `raw` | JSONB | lossless provider/component payload for diagnostics; nullable; never returned by list endpoints |
| `deleted_at`, `created_at`, `updated_at` | | |

Unique partial index on `(source_id, external_uid, COALESCE(recurrence_id, ''))`
where `deleted_at IS NULL` — this is the import/sync idempotency key.

Range-query index on `(owner_id, starts_at)` where `deleted_at IS NULL`.

`instance_start` is a derived read-model field, not a stored column: it is the
expanded occurrence's `DTSTART` (RFC 3339), present only on expanded
recurrence instances and null on stored (non-expanded) rows.

Range queries are two-part (see also the API contract): (a) non-recurring
events with `starts_at < to AND ends_at > from` (overlap), plus (b) ALL
recurring masters (`rrule IS NOT NULL`) for the owner regardless of
`starts_at`, expanded in the window with expanded instances filtered by the
same overlap predicate. Overridden occurrences covered by a stored
`recurrence_id` row replace the master's expansion for that slot.

Supporting partial index for part (b):
`calendar_events_recurring_owner_idx ON (owner_id) WHERE rrule IS NOT NULL AND deleted_at IS NULL`.

### `calendar_sync_states`

Per-source sync cursor, modeled on the mail archive watermarks
(`last_imported_uid`). Separate from `calendar_sources` so cursor churn does
not update the credential row.

| Column | Type | Notes |
|---|---|---|
| `source_id` | UUID PK/FK | one row per source |
| `next_sync_at` | TIMESTAMPTZ | drives due-ness; set on connect and updated after every run (with backoff on rate-limit) |
| `locked_at` | TIMESTAMPTZ | lease heartbeat; NULL when unclaimed. Stale leases are reset by the worker |
| `locked_by` | TEXT | lease holder id (worker instance id); NULL when unclaimed |
| `cursor_kind` | VARCHAR(20) | CHECK `('google_sync_token','ms_delta_token')` |
| `cursor_value` | TEXT | opaque provider token; NULL = full sync required |
| `cursor_expires_at` | TIMESTAMPTZ | nullable |
| `last_synced_at` / `last_error` | | mirrored onto `calendar_sources` for status reads |
| `updated_at` | | |

Worker scheduling/lease: due sources are claimed with
`SELECT ... FOR UPDATE SKIP LOCKED WHERE next_sync_at <= now() AND (locked_at IS NULL OR locked_at < now() - stale)`
(the stale threshold comes from env, same pattern as the mail stale-job reset).
The lease holder refreshes `locked_at` as a heartbeat while running and clears
`locked_at`/`locked_by` on completion or failure. Only the lease holder may
refresh OAuth tokens for a source (see Connection).

On Google `410 GONE` (invalidated syncToken) the worker nulls `cursor_value`
and performs a bounded full resync of the source's sync window.

### `calendar_import_jobs`

DB-backed queue modeled on `mail_import_jobs`
(`backend/migrations/20260708160003_create_mail_import_jobs_table.sql`).

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | |
| `tenant_id` / `owner_id` | UUID | |
| `source_id` | UUID | the `ical_import` source this job feeds |
| `status` | VARCHAR(20) | CHECK `('pending','running','completed','failed','cancelled')` |
| `filename` | TEXT | original upload name, sanitized |
| `size_bytes` | BIGINT | |
| `total_events` / `processed_events` / `failed_events` | INTEGER | |
| `last_error` | TEXT | safe summary |
| `started_at` / `completed_at` | TIMESTAMPTZ | |
| `deleted_at`, `created_at`, `updated_at` | | `updated_at` heartbeat drives stale-job reset |

Claims use `SELECT ... FOR UPDATE SKIP LOCKED`; the worker resets `running`
jobs whose `updated_at` is older than the configured stale threshold
(same contract as `reset_stale_running_mail_import_jobs`).

### `calendar_oauth_states`

Single-use OAuth `state` nonce for the connect flow.

| Column | Type | Notes |
|---|---|---|
| `state` | TEXT PK | 256-bit random value (base64url) |
| `tenant_id` | UUID | default tenant constant |
| `owner_id` | UUID | `REFERENCES users(id) ON DELETE CASCADE`; binds the nonce to the initiating user |
| `kind` | VARCHAR(20) | CHECK `('google','outlook')` |
| `expires_at` | TIMESTAMPTZ | 10 minutes after creation |
| `created_at` | TIMESTAMPTZ | |

Rows are deleted on consume (single use); expired rows are ignored/swept.

## Import semantics (iCal/.ics)

- Upload is a multipart file to `POST /api/v1/calendar/import`; the handler
  streams to a temp file via the same mechanism as the mail upload path
  (`stream_multipart_field_to_temp_file`, whose mail cap is 25 MiB), with a
  calendar cap of 10 MB.
- The file is parsed with a maintained RFC 5545 parser crate (implementation
  decision recorded in the plan; `icalendar` preferred, `ical` and a
  hand-rolled parser rejected). Recurrence expansion uses the maintained
  `rrule` crate (new dependency alongside `icalendar`).
- Each VEVENT maps to one `calendar_events` row keyed by
  `(source_id, UID, RECURRENCE-ID)`. Re-importing the same file is an
  idempotent upsert: unchanged events are untouched, changed events update,
  and duplicates are never created.
- Time handling: `DTSTART;TZID=...` is converted to UTC using the embedded
  VTIMEZONE definition — including non-IANA Windows zone names such as
  `W. Europe Standard Time`, whose STANDARD/DAYLIGHT transition rules are
  resolved — or the system IANA tz database; floating times are interpreted
  as UTC and marked `timezone = 'UTC'`. All-day `DATE` values become
  UTC-midnight spans with `all_day = true` and `original_date` preserved.
- Timezone limitation for recurrence: an event imported with a non-IANA
  `TZID` stores that raw TZID string, which the read-time expander cannot
  resolve, so a recurring master with such a timezone expands on UTC
  wall-clock (the stored master `starts_at` instant and non-recurring events
  remain correct). Recurrence expansion is exact for IANA `TZID` values and
  for floating/UTC times.
- RRULE strings are stored verbatim; v1 does not validate every RRULE form
  and never expands more than the requested range window at read time.
- Expansion semantics (normative): iteration is wall-clock in the event's
  IANA `TZID` (not UTC-instant); DST gaps push forward and overlaps keep the
  first occurrence; `UNTIL`/`COUNT` expansion is capped at 1000 instances per
  master; floating times (no `TZID`) are interpreted as UTC, matching
  import-time storage.
- VEVENT `STATUS` maps verbatim to `status` (`CONFIRMED`/`TENTATIVE`/
  `CANCELLED`); a missing `STATUS` defaults to `confirmed`.
- Change detection: the importer compares a normalized column set (or
  `external_etag` when present) and updates the row only on actual change.
- VALARM is parsed but discarded in v1 (reminders are out of scope); VTODO,
  VJOURNAL, VFREEBUSY components are ignored and counted as skipped.
- Malformed components fail only their own event (`failed_events += 1` with a
  bounded error sample in `last_error`); a structurally unreadable file fails
  the job.

## External sync semantics (Google / Microsoft)

Mirror mode per `connector-contract-v1alpha1.md`: the external provider is
authoritative; Elembra stores a read-only cache.

### Connection

- Backend-driven OAuth2 authorization-code flow. The backend builds the
  consent URL from deployment config (`RUSTSHARE_CALENDAR_GOOGLE_CLIENT_ID` /
  `RUSTSHARE_CALENDAR_GOOGLE_CLIENT_SECRET`,
  `RUSTSHARE_CALENDAR_MICROSOFT_CLIENT_ID` /
  `RUSTSHARE_CALENDAR_MICROSOFT_CLIENT_SECRET`) and
  `RUSTSHARE_PUBLIC_URL`; the frontend only redirects to it.
- `state` is a single-use, short-lived, server-side value bound to the
  initiating user; the callback rejects unknown/expired/mismatched state.
- Granted scopes are read-only: Google
  `https://www.googleapis.com/auth/calendar.readonly`; Microsoft
  `Calendars.Read` (offline_access for refresh tokens).
- v1 syncs only the provider's primary calendar (`primary` / default
  calendar); multi-calendar discovery (`calendarList`) is deferred.
- Token refresh is serialized per source: only the sync lease holder
  (see `calendar_sync_states` scheduling) may refresh OAuth tokens. If the
  provider returns a rotated refresh token it is written unconditionally
  (newer token wins).
- Refresh tokens (and cached access tokens) are encrypted with
  `SecretEncryptionKey` before storage. Responses, logs, and events never
  contain token material.

### Sync loop

- A `calendar_sync_worker` (spawned from `bootstrap.rs`, env-configured via
  `RUSTSHARE_CALENDAR_SYNC_WORKER_*`, same shape as
  `RUSTSHARE_MAIL_IMPORT_WORKER_*`) claims due sources with bounded
  concurrency.
- Google: incremental `events.list` with `syncToken`; on `410 GONE` the
  cursor is cleared and a bounded full resync runs (default sync window:
  90 days back, 365 days forward).
- Microsoft: `calendarView/delta` with `deltaToken`; an expired/invalid delta
  token triggers the same full-resync fallback.
- Provider deletions propagate two ways: cancelled instances arrive as
  `status = 'cancelled'` tombstone entries in delta payloads, and on FULL
  runs the absent-entry sweep soft-deletes in-window mirror rows missing
  from the complete window payload. Incremental deltas carry only changed
  entries, so the sweep never runs there — an unchanged event absent from
  a delta is untouched.
- Change detection: the worker compares a normalized column set (or
  `external_etag` when present) and updates the row only on actual change.
- Rate limits (429 / `Retry-After`) pause the source (`status =
  'rate_limited'`) with backoff; revoked grants set `auth_required` and stop
  syncing until the user reconnects.
- Every sync run is idempotent via the `(source_id, external_uid,
  recurrence_id)` key; retries never duplicate events.

## Integration events

The three internal-mutation events are published transactionally with the
mutation (outbox row in the same commit). The per-run `imported.v1` is
published best-effort *after* the import/sync run commits — a failed
publication is logged and does not fail or retry the run, so an event that
never reaches the outbox is not redelivered: the per-run guarantee is
at-most-once at publish time, and the outbox provides at-least-once only once
a row is persisted. Consumers must deduplicate by envelope id. Envelope
per `integration-event-v1alpha1.md`:

- `io.elembra.calendar.event.created.v1` — internal event created.
- `io.elembra.calendar.event.updated.v1` — internal event updated.
- `io.elembra.calendar.event.deleted.v1` — internal event deleted.
- `io.elembra.calendar.event.imported.v1` — an import or sync run
  materialized events; `data` carries counts and the source ResourceRef, not
  event bodies. Best-effort after the run commits (see above).

Event payloads contain identifiers and provenance only (`ResourceRef`,
counts, source kind). Sensitive content (titles, descriptions, attendees) is
represented by reference, per the outbox ADR's minimum-safe-data rule.

Calendar is the third outbox publisher, after Files (the FileService-facing
`publish_in_tx` adapter) and Chat (the Buzz observation path). Publishing uses
the generic
`OutboxStore::insert_in_tx`, which validates that the source Application's
manifest owns the event type (`owns_event_type`); the hardcoded
`io.elembra.files` source in the FileService-facing `publish_in_tx` adapter
(deferred generalization in `0031-durable-integration-events.md`) is not on
the Calendar path and is unchanged.

## API surface

Normative request/response definitions live in
`docs/contracts/calendar-application-api.md`. Summary:

- `GET/POST /api/v1/calendar/events`, `GET/PATCH/DELETE
  /api/v1/calendar/events/{id}` — internal CRUD; `GET` supports `from`/`to`
  range queries (required, bounded to 366 days) and expands recurrences
  server-side within the window.
- `GET/POST /api/v1/calendar/sources`, `PATCH/DELETE
  /api/v1/calendar/sources/{id}` — source management (no token material ever
  returned).
- `GET /api/v1/calendar/sources/{kind}/connect`,
  `GET /api/v1/calendar/oauth/{kind}/callback` — OAuth flow.
- `POST /api/v1/calendar/import` (multipart), `GET
  /api/v1/calendar/import-jobs[/{id}]` — import.
- `POST /api/v1/calendar/sources/{id}/resync` — manual full resync; `409`
  while a live sync lease holds the source.
- `POST /api/v1/calendar/sources/{id}/disconnect` — wipe the stored tokens and
  park the source at `auth_required`; `409` while a live sync lease holds the
  source. No Microsoft session revoke is attempted; Google's best-effort token
  revocation is unchanged.
- Stretch: `GET /api/v1/calendar/feed/{token}` — read-only per-user ICS export
  feed; feed tokens are created/revoked via session-authenticated `POST`/`DELETE
  /api/v1/calendar/feed-token`. (axum 0.8 matches the whole final segment, so a
  request for `/{token}.ics` arrives as `token = "<token>.ics"`; the handler
  must strip a trailing `.ics`, and the `{token}.ics` route pattern must not be
  registered.)

Every JSON API route requires an authenticated principal and an enabled
`io.elembra.calendar` Application for the tenant (403 otherwise), mirroring
`require_mail_enabled`. The OAuth callback is authenticated by its single-use
`state` instead of a session, and the stretch ICS feed by its feed token, so
both are exempt from the session/enablement gate.

## Failure and health

Source status follows the connector contract vocabulary:

```text
healthy | degraded | auth_required | rate_limited | paused | failed
last_synced_at
last_error (safe summary)
```

A failed or revoked source never affects other sources, internal events, or
other Applications. Worker crashes are recovered by the stale-job reset;
partially imported files resume idempotently because event identity is
content-keyed, not job-progress-keyed.

## Security rules

- OAuth tokens encrypted at rest (AES-256-GCM, `RUSTSHARE_SECRET_ENCRYPTION_KEY`);
  never logged, never in API responses, never in event payloads.
- OAuth `state` is single-use and user-bound; callbacks validate it before any
  token exchange.
- Provider endpoints are fixed allowlisted base URLs; no user-controlled
  outbound fetch URLs exist anywhere in the calendar surface.
- Uploaded .ics content is untrusted input: size-capped, parsed defensively,
  never written to the filesystem beyond a temp file, never rendered as HTML.
- Owner scoping is enforced in the storage layer (every query filters
  `tenant_id` + `owner_id`); cross-owner access is indistinguishable from
  absence (404).
- Provider error bodies are summarized before persisting `last_error`; raw
  responses may contain account metadata and must not be stored verbatim.

## Non-goals v1alpha1

- Bidirectional sync with any provider (explicitly, per the connector
  contract warning).
- Workspace-shared calendars, calendar delegation, attendee/invitation
  management (iTIP), and public share links for calendars.
- A CalDAV server or CalDAV client.
- Reminders/notifications (VALARM execution), including push/email delivery.
- Free/busy queries and scheduling assistance.
- Memory/Search indexing of events (the manifest declares no memory policy —
  `memory: None` — until permission-aware indexing is designed for owner-only
  data).
- Recurring-event editing semantics beyond "edit the master" (per-instance
  edits are provider-side for mirrors and out of scope internally).
- Google/Outlook webhooks/push channels (polling cursors only).

## Acceptance tests

- Manifest: `io.elembra.calendar` validates in the first-party registry;
  contribution ids and action namespaces pass `validate_manifest`.
- Enablement: every calendar route returns 403 when the Application is
  disabled for the tenant; 401 unauthenticated.
- Ownership: user B cannot read, modify, or enumerate user A's events,
  sources, or jobs (404, not 403, for existence-hiding).
- Import: importing the same .ics twice yields a stable event set (zero new
  rows on the second run); a truncated/invalid file fails the job with a
  bounded `last_error`; events with `TZID` land at the correct UTC instant.
- Sync: a Google syncToken flow processes incremental changes; a forced
  `410 GONE` triggers exactly one full resync; a revoked token flips the
  source to `auth_required` and further sync attempts are no-ops.
- Encryption: `refresh_token_enc` round-trips through
  `SecretEncryptionKey`; the plaintext token appears in no API response,
  log capture, or outbox payload in tests.
- Outbox: creating an internal event commits `calendar_events` and the
  `io.elembra.calendar.event.created.v1` outbox row atomically (verified by
  transaction rollback test).
- Range queries reject windows > 366 days and unbounded recurrence expansion
  is impossible (expansion is capped per request).
