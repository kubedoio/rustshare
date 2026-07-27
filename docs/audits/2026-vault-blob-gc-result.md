# Vault Blob Garbage Collection Result (2026)

## Result

RustShare now records vault blob puts whose metadata write conflicts or fails
as durable, coalesced GC candidates. A disabled-by-default background worker
can delete a candidate only after a conservative grace period, exclusive lease,
shared writer/collector lock, two successful global reference checks, and an
idempotent object-store operation. Database or object-store uncertainty fails
closed.

Human review is required before merge because this phase introduces deletion
of content-addressed objects.

## Reference-source inventory

The authoritative inventory is
[`2026-vault-blob-lifecycle-audit.md`](2026-vault-blob-lifecycle-audit.md).
The global query protects:

- `files.storage_key` and `file_versions.storage_key`;
- `vault_files.sha256`, including retained tombstones;
- `mail_messages.blob_key` / `object_key`;
- `mail_message_parts.blob_key`;
- `mail_attachments.blob_key`;
- replication jobs in `queued`, `retrying`, `syncing`, or `failed` state.

Notes, note bundles, and draft attachments resolve through file/version or mail
rows. Thumbnails and resumable chunks use separate namespaces and are never
eligible for this collector.

## Architecture and state model

The existing `object_gc_queue` was extended additively with candidate UUID,
reason, first/last observation, attempt/error fields, explicit state, processing
lease, completion timestamp, update timestamp, and operator hold. States are
`pending`, `processing`, `referenced`, `deleted`, `missing`, `retry`,
`invalid_key`, and `operator_hold`.

Re-enqueue updates `last_seen_at`, resets a terminal occurrence to pending, and
uses the later `not_before`; it never shortens a safety delay. Referenced rows
are retained as terminal evidence and can be reactivated by a later enqueue.

## Grace, references, and races

The default grace period is 24 hours. Leasing also checks `last_seen_at`, so an
operator-configured longer grace applies to trigger- and application-produced
candidates alike.

All production `blobs/<sha256>` writers—normal file writes, resumable assembly,
vault sync, and mail artifact writes—take a PostgreSQL advisory transaction
lock derived from the full key before object put and hold it through metadata
completion. GC takes the same lock through both reference checks, deletion,
and candidate completion. This closes the writer window after the second
reference check; unrelated hashes do not contend.

Vault enqueue reasons implemented:

- `vault_revision_conflict`;
- `vault_concurrent_create_conflict`;
- `metadata_write_failure_after_blob_put`.

The original conflict/database result remains unchanged and enqueue failure is
logged separately. No request path deletes an object.

## Worker behavior

Workers lease bounded batches using `FOR UPDATE SKIP LOCKED`, identify ownership
with `locked_by`, and reclaim expired processing leases. Invalid/non-lowercase
keys are recorded without a delete call. Reference and existence query errors
retry. Explicit missing objects complete as `missing`; successful deletes
complete as `deleted`. Retry uses bounded exponential backoff with jitter;
maximum-attempt candidates move to `operator_hold` and remain operator-visible.

Shutdown stops new ticks; an interrupted candidate is recovered by lease
expiry. Multiple instances are safe, and a late expired worker cannot complete
another worker's lease.

## Metrics and configuration

Metrics:

- `object_gc_candidates_enqueued_total`
- `object_gc_candidates_processed_total`
- `object_gc_blobs_deleted_total`
- `object_gc_blobs_missing_total`
- `object_gc_candidates_referenced_total`
- `object_gc_candidates_invalid_total`
- `object_gc_reference_check_failures_total`
- `object_gc_delete_failures_total`
- `object_gc_pending_candidates`
- `object_gc_oldest_pending_seconds`

Configuration is documented in `.env.example` and `docs/DEPLOYMENT.md`. Invalid
ranges fail startup clearly. GC is disabled by default; enqueueing is not.

## Integration and compatibility evidence

Real PostgreSQL 16/pgvector and RustFS Compose validation passed:

- additive migration on the existing database;
- duplicate candidate coalescing without delay shortening;
- exclusive concurrent-worker leasing;
- stale processing lease recovery;
- exact global counts across files, versions, vault, mail, parts, attachments,
  and replication;
- referenced same-SHA blob retained;
- unreferenced blob deleted;
- missing object completed idempotently;
- invalid key retained and marked invalid.

The focused infrastructure suite ran 7/7 passing with
`cargo test -p rustshare-server --test object_gc_test -- --ignored
--test-threads=1`. Unit coverage proves a reference appearing between the first
and second check cancels deletion.

Compose built the release image, applied migrations on the existing database,
and reported healthy. Startup passed with GC disabled and with GC enabled at a
one-hour test interval. The deployed service remains disabled by default.

File, version, mail, note, attachment, vault, sync, trash, retention,
replication, and HTTP contracts are unchanged. No frontend or generated API
contract changed.

## Upgrade, rollback, and remaining risks

Upgrade applies one additive migration before the worker starts. Enable GC only
after human review and metrics collection. Rollback first disables the worker,
then deploys the prior binary while retaining the additive schema; the prior
worker excludes content-addressed keys.

Known remaining risks:

- terminal candidate pruning is deferred until table-growth measurements exist;
- bucket-wide reconciliation is deferred and must only create candidates;
- operator-held maximum-attempt candidates require alerting and explicit re-enqueue;
- the safety boundary depends on future content-addressed writers using the
  shared object-store lock API.

## Data-safety statement

This PR introduces deletion of content-addressed objects. A blob is deleted
only after a durable grace period, successful global reference checks, an
immediate second reference check, and exclusive candidate acquisition. A
cross-process per-key lock also excludes concurrent writers across the complete
put-to-metadata window. Database or object-store uncertainty fails closed.
