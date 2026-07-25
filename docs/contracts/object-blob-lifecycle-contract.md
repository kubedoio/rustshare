# Object Blob Lifecycle Contract

## Scope

This contract governs deletion of globally deduplicated objects whose exact key
is `blobs/<64 lowercase hexadecimal characters>`. It does not govern upload
chunks, thumbnails, metadata-store objects, or arbitrary object-store keys.

The database is authoritative for references and candidates. Object-store
listing is not required for deletion eligibility.

## States

| State | Meaning | Allowed next states |
|---|---|---|
| `pending` | Durable reason to test the key after `not_before` | `processing` |
| `processing` | Exclusively leased by one worker | `referenced`, `deleted`, `missing`, `retry`, `invalid_key`, `operator_hold` |
| `retry` | A check or delete was uncertain/failed; `not_before` contains backoff | `processing` |
| `referenced` | A durable reference was found; terminal for this candidate occurrence | `pending` on a later enqueue |
| `deleted` | Object-store deletion succeeded | `pending` on a later enqueue |
| `missing` | Object store confirmed the key absent | `pending` on a later enqueue |
| `invalid_key` | Key is outside the approved exact namespace | none without operator correction |
| `operator_hold` | Operator policy forbids deletion | `pending` after the hold is removed |

`Candidate` never means safe to delete. Re-enqueueing a terminal row resets it
to `pending`; re-enqueueing a live row updates observation metadata but never
shortens `not_before`.

## Reference contract

A blob is referenced if any durable, live or retention-protected record points
to it. The global structured reference summary counts exact matches from:

- `files.storage_key`;
- `file_versions.storage_key`;
- `vault_files.sha256`, compared to the digest portion of the key, including
  retained tombstones;
- `mail_messages.blob_key` and `mail_messages.object_key`;
- `mail_message_parts.blob_key`;
- `mail_attachments.blob_key`;
- `replication_jobs.storage_key` while status is `queued`, `retrying`,
  `syncing`, or `failed`.

No tenant predicate is permitted. Soft-deleted file, vault, and mail rows are
not excluded while they remain durable. Query failure is an unsafe result, not
zero references.

## Writer/collector exclusion

Every production writer of `blobs/<sha256>` must acquire the same PostgreSQL
advisory lock derived from the full object key before its object-store put and
hold it until the metadata write commits or fails. The collector acquires that
lock before its first reference check and holds it through deletion and the
candidate state update.

This cross-process lock closes both unsafe windows:

1. GC cannot delete while a writer is between object put and metadata commit.
2. A writer cannot begin a put between GC's second reference check and delete.

The lock is an execution guard, not a reference. Failure to acquire or retain
it fails closed. Transaction/session termination releases stale locks; the
durable candidate lease separately handles worker restart.

## Grace period and eligibility

The default grace period is 24 hours and is configurable with
`RUSTSHARE_OBJECT_GC_GRACE_PERIOD_HOURS`. A candidate is eligible only when:

1. GC is enabled.
2. Its key has the exact approved lowercase shape.
3. `not_before` has passed.
4. The worker atomically owns its unexpired candidate lease.
5. No operator hold is active.
6. The shared writer/collector lock is held.
7. The first global reference query succeeds and totals zero.
8. The immediate second global reference query succeeds and totals zero.
9. Object deletion returns success or an explicit not-found result.

Any uncertainty leaves the object intact and schedules a retry.

## Candidate creation

Reasons are bounded operator-safe identifiers:

- `vault_revision_conflict`;
- `vault_concurrent_create_conflict`;
- `metadata_write_failure_after_blob_put`;
- `reference_replaced`;
- `retention_expired`;
- `manual_reconciliation`.

Candidate persistence contains keys and operational metadata only—never object
content, credentials, presigned URLs, or document metadata.

Vault enqueueing occurs after a successful put whenever the conditional
metadata operation does not establish a durable reference. Enqueue failure is
logged separately while the original conflict/database result is preserved.
No request path deletes a blob.

## Leasing, retry, and shutdown

Workers claim a bounded batch atomically with row locking and `SKIP LOCKED`.
Processing leases expire and are reclaimable. Lease ownership is checked on
every state update so an expired worker cannot complete another worker's
candidate.

Transient database or object-store failures increment `attempt_count`, record
a bounded safe error, and schedule exponential backoff with jitter. Backoff and
attempts are capped. Reaching the configured maximum leaves the candidate in a
visible retry/permanent-failure condition; it is never reported as completed.

Shutdown stops claiming work. An in-flight row remains recoverable through its
lease timeout.

## Deletion results

| Result | Durable candidate outcome |
|---|---|
| Delete succeeded | `deleted` |
| Explicit object not found | `missing` |
| Reference found on either check | `referenced` |
| Invalid key | `invalid_key`, no delete call |
| Database, network, authentication, permission, or unknown error | `retry` and operator-visible failure |

Deletion is idempotent. It never mutates file, version, vault, mail, note,
attachment, trash, replication, or audit records.

## Operations

Candidate enqueueing remains enabled when the worker is disabled. Operators
can disable GC with `RUSTSHARE_OBJECT_GC_ENABLED=false` and inspect backlog,
age, outcomes, and failures through metrics. This phase exposes no manual
delete endpoint. A later reconciliation scan or manual retry must reuse the
same eligibility path.

