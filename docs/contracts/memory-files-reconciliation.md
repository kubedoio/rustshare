# Files and Notes Memory reconciliation contract (M3.1/M3.3)

## M3.3 durable automation

The optional Files-backed Notes scheduler is disabled by default. When enabled,
it discovers tenants from authoritative `files` rows and schedules one bounded
page at a time from PostgreSQL. Each tenant has a durable cursor, page counters,
retry state, last completion, next run, heartbeat, lease owner, and monotonically
increasing lease generation. A checkpoint is written only after the complete
M3.1 page succeeds; a crash replays that page. A generation and live-lease
predicate fence checkpoint, retry, and heartbeat writes from stale owners.

A completed sweep clears its cursor and schedules the next full sweep from the
beginning. This is required because UUIDs created after an earlier cursor can
sort before it. Source enumeration includes soft-deleted rows so those
tombstones remove their index chunks. Hard-deleted rows are absent from the
source scan; M3.3 does not claim orphan-index cleanup for them.

The scheduler uses the M3.1 100-row page and source-row lock and calls
`NoteService::reconcile_index_file` for current content, ACL, and frontmatter
policy. Keyword-only mode remains supported. A failed page keeps its input
cursor and uses bounded exponential retry; exhausted jobs require an explicit
admin retry. File/Notes writes remain independent of the worker.

Operational status and retry are available to an authenticated tenant admin:

- `GET /api/v1/admin/applications/memory/files/reconcile/status`
- `POST /api/v1/admin/applications/memory/files/reconcile/retry`

Status excludes source content and exposes only a generic, sanitized error.
The retry endpoint schedules the caller's tenant; it cannot select another
tenant.

## Audit and design decision

Chat already has an independent recovery path: the admin Chat reconcile handler
folds the verified `chat_observed_events` history into `memory_catalog`, and
can page the authoritative Buzz relay to repair that observation index. Its
consumer receipts and behavior remain unchanged.

Files-backed Notes use a separate projection in `note_index_chunks`. Note
mutations invoke `NoteIndexSink` directly; the production sink writes the
shared PostgreSQL content index but currently logs and swallows failures. AI
disabled does not disable policy-permitted keyword indexing; keyword-only rows
have no vector. There is no durable Files/Notes event consumer. M3.1 added a
manual repair endpoint; M3.3 optionally schedules the same page logic for
automatic repair. Ordinary Files search uses authoritative metadata queries;
the rebuildable content projection is the existing OKF note index.

M3.1 adds an admin-triggered, cursor-paged sweep over authoritative `files`
rows for the caller's tenant. The cursor is a canonical Files ResourceRef
(`elembra://io.elembra.files/file/{uuid}`). Active, available note owners are
resolved through NoteService, which fetches current OKF content and computes
the current ACL projection. Soft-deleted, unavailable-owner, and no-longer-
indexable source files have their chunks removed by `source_file_id`. The
projection never decides access: retrieval continues to authorize through
Files before returning content or citations.

While refreshing or removing chunks, reconciliation holds a row lock on the
authoritative Files record. A concurrent edit, deletion, or restore therefore
runs after the repair or cleanup and its normal index update/removal, preventing
a late reconciliation write from undoing that lifecycle change.

The sweep is stateless and resumable. It processes at most 100 source rows per
request and returns the last processed ResourceRef only when another page is
available. Index writes and removals are idempotent; after an interrupted
request, retry its input cursor. A failed row fails the page without returning
a cursor, so the page can be replayed safely. Re-running from the beginning
repairs missed NoteService callbacks and stale ACL/content projections.

## API

`POST /api/v1/admin/applications/memory/files/reconcile`

Request:

```json
{
  "tenant_id": "uuid",
  "after": "elembra://io.elembra.files/file/uuid",
  "limit": 100
}
```

`after` and `limit` are optional. The authenticated administrator and target
tenant must match. The response reports processed/indexed/removed/unavailable
counts and an optional `next_cursor`. Every indexed chunk remains attributable
to its Files source file ID; the normal search path renders that ID as the
canonical ResourceRef and performs live Files authorization.

The underlying `files.deleted_at` record is the deletion tombstone. The sweep
removes any associated index chunks, including rows whose OKF note ID differs
from the file ID. It does not recreate or copy authoritative source records.
