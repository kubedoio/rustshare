# Contract: Calendar Application API v1alpha1

Status: Draft  
Date: 2026-10-01  
Spec: `docs/specs/calendar-application-v1alpha1.md`  
ADR: `docs/adr/0037-calendar-application-and-external-sync.md`

REST contract for the `io.elembra.calendar` Application. Base path
`/api/v1/calendar`. Every JSON API route requires an authenticated session and
an enabled Calendar Application for the caller's tenant. The OAuth callback
(`GET /api/v1/calendar/oauth/{kind}/callback`) is invoked by the provider's
browser redirect and authenticated by the single-use `state` rather than a
session:

- `401` — unauthenticated.
- `403 { "error": "Calendar module is disabled" }` — Application disabled for
  the tenant (same gate shape as `require_mail_enabled`).
- Owner scoping: every resource belongs to the authenticated user; resources
  owned by another user are indistinguishable from missing ones (`404`).

Errors use the standard `ErrorResponse` body:

```json
{ "error": "human-readable summary" }
```

Timestamps are RFC 3339 UTC. IDs are UUIDs. No response ever contains OAuth
token material; token columns (`refresh_token_enc`, `access_token_enc`) are
write-only storage internals.

## Events

### `GET /api/v1/calendar/events`

List events overlapping a time range. `from` and `to` are **required** and the
window must be ≤ 366 days (`400` otherwise). The query is two-part: (a)
non-recurring events with `starts_at < to AND ends_at > from` (overlap), plus
(b) ALL recurring masters (`rrule IS NOT NULL`) owned by the caller,
regardless of `starts_at`, expanded server-side within the window; expanded
instances are filtered by the same overlap predicate. Each expanded instance
carries the master's `id` plus `recurrence_id`/`instance_start` (the expanded
occurrence's `DTSTART`, RFC 3339; null on stored non-expanded rows). Stored
override rows (`recurrence_id` NOT NULL) are returned as stored (own `id`),
and master expansion omits occurrences suppressed by an override row (the
suppression set spans all of the caller's override rows, not only those in
the requested window).

The UI requests its Day, work-week (Monday–Friday), Month, and Agenda windows
through this same endpoint — each view is a `from`/`to` window (local-midnight
boundaries) of at most 366 days, and the API is window-agnostic; there are no
view-specific parameters.

Query parameters:

| Param | Type | Notes |
|---|---|---|
| `from` | RFC 3339 | required, inclusive |
| `to` | RFC 3339 | required, exclusive |
| `source_id` | UUID | optional, repeatable; filter to given sources |
| `include_cancelled` | bool | default `false` |

Response `200`:

```json
{
  "events": [
    {
      "id": "uuid",
      "source_id": "uuid",
      "source_kind": "internal | ical_import | google | outlook",
      "title": "Sprint review",
      "description": "…",
      "location": "…",
      "starts_at": "2026-10-05T14:00:00Z",
      "ends_at": "2026-10-05T15:00:00Z",
      "all_day": false,
      "original_date": null,
      "timezone": "Europe/Berlin",
      "rrule": "FREQ=WEEKLY;BYDAY=MO",
      "recurrence_id": null,
      "instance_start": null,
      "status": "confirmed",
      "read_only": false,
      "created_at": "…",
      "updated_at": "…"
    }
  ]
}
```

`read_only` is `true` for every non-internal source; `PATCH`/`DELETE` on such
events returns `409 { "error": "Event is synchronized read-only from an external source" }`.

### `POST /api/v1/calendar/events`

Create an internal event. Body:

```json
{
  "title": "Dentist",
  "description": null,
  "location": null,
  "starts_at": "2026-10-03T08:00:00Z",
  "ends_at": "2026-10-03T08:30:00Z",
  "all_day": false,
  "timezone": "Europe/Berlin",
  "rrule": null
}
```

Validation: `title` 1–512 chars; `ends_at > starts_at`; `timezone` must be a
known IANA name (`400` otherwise); all-day events must align to whole days.
Response `201` with the event object. Publishes
`io.elembra.calendar.event.created.v1` atomically with the insert.

### `GET /api/v1/calendar/events/{id}`

Single event, including `description`. `200` / `404`.

### `GET /api/v1/calendar/events/{id}/export`

Exports one event readable by the authenticated owner as a private, non-cacheable
`.ics` attachment (`200`, `Content-Type: text/calendar; charset=utf-8`,
`Cache-Control: private, no-store`, filename based on the event UUID). Requires
Calendar enabled and uses the same tenant/owner-scoped lookup as the event read
route (`401` unauthenticated, `403` disabled, `404` foreign or missing event).
The stable `UID` preserves a non-empty source `external_uid` for imported or
provider-backed events; internal events use RustShare's event UUID;
`DTSTAMP` uses `updated_at`. Timed values are UTC instants. All-day values use
DATE properties and the stored exclusive UTC-midnight end date.

Until the exporter can emit a complete timezone definition and preserve
detached recurrence identity, it returns `409` for recurrence overrides and
for recurring masters whose timezone is not exactly `UTC`. RRULE values with
line breaks are also refused. These are explicit interoperability limits,
not silently flattened exports; recurring export semantics remain a follow-up
for #329.

### `PATCH /api/v1/calendar/events/{id}`

Partial update of internal events (same fields as create, all optional).
`200` with the updated event; `404` unknown; `409` if the event belongs to a
non-internal source. Publishes `io.elembra.calendar.event.updated.v1`.

For the nullable string fields `description`, `location`, and `rrule`, an
absent or `null` value leaves the stored value unchanged, while an empty
string clears it (for `rrule`, this stops the recurrence). `title` cannot be
cleared (empty is a `400`).

### `DELETE /api/v1/calendar/events/{id}`

Soft-delete an internal event. `200 { "ok": true }`. Deletion is idempotent:
an already-deleted, never-existing, or foreign-tenant ID is a successful no-op
(`200`), so a cross-tenant ID is indistinguishable from a random one. A live
row in the caller's tenant that is owned by someone else is a `404`; a
read-only mirror is a `409`. Publishes `io.elembra.calendar.event.deleted.v1`
only when a row was actually deleted.

## Sources

### `GET /api/v1/calendar/sources`

```json
{
  "sources": [
    {
      "id": "uuid",
      "kind": "internal | ical_import | google | outlook",
      "display_name": "Work Google",
      "external_account": "user@example.com",
      "external_calendar_id": "primary",
      "is_enabled": true,
      "status": "healthy | degraded | auth_required | rate_limited | paused | failed",
      "last_synced_at": "…",
      "last_error": null,
      "created_at": "…"
    }
  ]
}
```

The internal source is auto-created lazily on first internal event creation
and is always present in this list once it exists. Token fields never appear.

### `POST /api/v1/calendar/sources`

Create a non-OAuth source. v1 supports only `kind: "ical_import"` (a named
container for imports) and explicit re-creation of the `internal` source is a
no-op returning the existing row (`200`).

```json
{ "kind": "ical_import", "display_name": "Exported from Apple" }
```

`201` with the source object; `400` for `google`/`outlook` (those are created
exclusively via the OAuth flow below); `409` if an identical active source
already exists.

### `PATCH /api/v1/calendar/sources/{id}`

`{ "display_name"?: string, "is_enabled"?: boolean }`. Disabling a source
stops its sync; events of disabled sources are excluded from `GET /events`
unless that source's `source_id` is explicitly passed. `200` / `404`.

### `DELETE /api/v1/calendar/sources/{id}`

Soft-deletes the source, destroys its stored tokens, and soft-deletes all
mirrored events imported from it. Internal sources cannot be deleted (`409`).
`200 { "ok": true }` / `404`.

## OAuth connections

### `GET /api/v1/calendar/sources/{kind}/connect`

`kind` is `google` or `outlook`. Returns the provider consent URL; the
frontend navigates the browser to it.

`200`:

```json
{ "authorize_url": "https://accounts.google.com/o/oauth2/v2/auth?…" }
```

`503 { "error": "Google OAuth is not configured" }` when the deployment lacks
the client id/secret env config — the settings panel renders the disabled
state from this.

The `state` parameter embedded in the URL is single-use, expires after 10
minutes, and is bound to the initiating user.

### `GET /api/v1/calendar/oauth/{kind}/callback`

Provider redirect target (`{RUSTSHARE_PUBLIC_URL}/api/v1/calendar/oauth/{kind}/callback`).
Validates `state`, exchanges the code, stores encrypted tokens, creates the
`calendar_sources` row, enqueues an initial full sync, and redirects the
browser to `/settings/apps/calendar?connected={kind}` (success) or
`/settings/apps/calendar?error=oauth_{reason}` (failure). Always a `302`
(`StatusCode::FOUND` plus a `Location` header — axum's `Redirect` helpers
emit `303`/`307`/`308`, so a `302` requires the manual status-code-plus-
header construction); the callback never renders or returns token data.

Failure codes in the redirect: `oauth_state`, `oauth_exchange`,
`oauth_denied`, `oauth_unconfigured`.

### `GET /api/v1/calendar/providers`

Read-only provider configuration status for the deployment, for operator
diagnosis of connection problems. Requires an authenticated session and an
enabled Calendar Application for the tenant (same gate as the rest of the JSON
API). `200`:

```json
{
  "public_url": "https://app.rustshare.io",
  "providers": [
    {
      "kind": "google",
      "configured": false,
      "redirect_uri": "https://app.rustshare.io/api/v1/calendar/oauth/google/callback"
    },
    {
      "kind": "outlook",
      "configured": false,
      "redirect_uri": "https://app.rustshare.io/api/v1/calendar/oauth/outlook/callback"
    }
  ]
}
```

`public_url` is the effective `RUSTSHARE_PUBLIC_URL`; `configured` is `true`
only when both the client id and client secret are present for that provider
(a partial configuration reports `false`); `redirect_uri` is derived as
`{public_url}/api/v1/calendar/oauth/{kind}/callback` and is the exact value an
operator must register in the provider console. The response **never** contains
client ids, client secrets, or any token material. This endpoint reports
configuration only — it is not a connect surface and does not start a flow.

### `POST /api/v1/calendar/sources/{id}/disconnect`

Deletes the stored tokens and sets `status: "auth_required"` (events remain
until the source is deleted). No Microsoft session revoke is attempted;
Google's best-effort token revocation is unchanged.
`409` while a live sync lease holds the source; `200 { "ok": true }` / `404` /
`400` for non-OAuth sources.

### `POST /api/v1/calendar/sources/{id}/resync`

Clears the stored cursor and enqueues a full resync of the configured sync
window. `202 { "ok": true }`; `404` unknown source; `400` for non-external
kinds; `409` if a sync is already running for the source.

## Import

### `POST /api/v1/calendar/import`

`multipart/form-data` with fields:

- `file` — the `.ics` file (required, ≤ 10 MB, `Content-Type:
  text/calendar` or extension `.ics`; `400` otherwise, `413` over limit).
- The complete multipart request is limited to 11 MiB; multipart framing and
  all additional fields count toward this total (`413` over limit).
- `source_id` — target `ical_import` source (optional; omitted creates a
  source named after the file).

Files containing more than 10,000 VEVENT components are rejected by the
asynchronous import job with `status: "failed"` and a bounded `last_error`;
the limit is checked before any events are written, so this failure does not
partially import the file.

The upload is spooled to a temp file and an import job is enqueued; parsing
and upsert run in the worker. `202`:

```json
{ "job_id": "uuid", "source_id": "uuid", "status": "pending" }
```

Each event is imported only when its semantics fit the Calendar event model.
Events containing unsupported properties (`ATTACH`, `ATTENDEE`, `CATEGORIES`,
`CLASS`, `CONFERENCE`, `EXDATE`, `GEO`, `ORGANIZER`, `RDATE`, `RELATED-TO`,
`RESOURCES`, `TRANSP`, `URL`, or `VALARM`), duplicate `RECURRENCE-ID`
properties, and `RECURRENCE-ID` with a `RANGE` parameter are skipped and
counted in `failed_events`; other events in the file continue. The job's
`last_error` reports unsupported property names/parameters and says the event
was not imported; it does not include property values such as attendee
addresses or recurrence-range values. A completed job may therefore contain
per-event failures, so clients should inspect both the counters and
`last_error`.

### `GET /api/v1/calendar/import-jobs`

```json
{
  "jobs": [
    {
      "id": "uuid",
      "source_id": "uuid",
      "filename": "export.ics",
      "status": "pending | running | completed | failed | cancelled",
      "total_events": 0,
      "processed_events": 0,
      "failed_events": 0,
      "last_error": null,
      "started_at": null,
      "completed_at": null,
      "created_at": "…"
    }
  ]
}
```

### `GET /api/v1/calendar/import-jobs/{id}`

Single job. `200` / `404`.

Re-importing the same file into the same source is safe: event identity is
`(source_id, UID, RECURRENCE-ID)`, so the second import updates in place and
creates no duplicates.

## Read-only subscription feed (deferred)

No calendar feed route or feed-token lifecycle endpoint is implemented. A
previous draft described `GET /api/v1/calendar/feed/{token}` and
`POST/DELETE /api/v1/calendar/feed-token` as available; those descriptions were
aspirational and are withdrawn. The capability URL cannot be enabled safely
until request-path secrets are redacted across application tracing, bundled
Nginx, and the external TLS load balancer. See the proposed Issue #329
assessment in [ADR-0037](../adr/0037-calendar-application-and-external-sync.md).
Use the authenticated single-event export route above for now.

## Status and error summary

| Code | Meaning |
|---|---|
| 200 | success |
| 201 | event/source created |
| 202 | job accepted (import, resync) |
| 302 | OAuth callback redirect |
| 400 | validation failure (bad range, bad kind, non-OAuth source, …) |
| 401 | unauthenticated |
| 403 | Calendar Application disabled for tenant |
| 404 | unknown or foreign-owned resource |
| 409 | state conflict (read-only mirror write, duplicate source, sync in flight) |
| 413 | upload over limit |
| 503 | OAuth provider not configured on this deployment |
