# Files and Notes Memory reconciliation contract (M3.1)

## Audit and design decision

Chat already has an independent recovery path: the admin Chat reconcile handler
folds the verified `chat_observed_events` history into `memory_catalog`, and
can page the authoritative Buzz relay to repair that observation index. Its
consumer receipts and behavior remain unchanged.

Files-backed Notes use a separate projection in `note_index_chunks`. Note
mutations invoke `NoteIndexSink` directly; the production sink writes the
shared PostgreSQL content index but currently logs and swallows failures. AI
disabled does not disable policy-permitted keyword indexing; keyword-only rows
have no vector. There is no
durable Files/Notes event consumer. M3.1's rebuild endpoint remains a manual,
admin-triggered repair path. Ordinary Files search uses authoritative metadata
queries; the rebuildable content projection is the existing OKF note index.

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
