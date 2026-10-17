# Object Blob Garbage Collection

## Purpose

Reclaim unreferenced global content-addressed objects without changing the safe
blob-before-metadata write order or deleting shared data.

## Candidate flow

1. A failed/superseded blob-producing operation or reference-removal trigger
   coalesces the exact object key into `object_gc_queue`.
2. The configurable grace period expires (24 hours by default).
3. One worker leases a bounded batch with `FOR UPDATE SKIP LOCKED`; stale
   processing leases are reclaimable.
4. The worker rejects any key except `blobs/<64 lowercase hex>`.
5. The worker obtains the same per-key PostgreSQL advisory lock held by all
   production content-addressed writers.
6. A global exact-match query counts file, version, vault, mail, attachment,
   body-part, and active replication references.
7. The worker checks object existence and immediately repeats the reference
   query.
8. Zero references on both checks permits idempotent S3 deletion; referenced,
   missing, invalid, and retry outcomes are recorded explicitly.

No request path deletes blobs. The deletion path uses no bucket scan;
read-only orphan discovery (Phase 3.4A) lists `blobs/` pages in bounded,
budgeted invocations and never mutates. Database or object-store
uncertainty schedules retry and leaves the object intact.

## Orphan reconciliation producer (Phase 3.4B)

A blob written by a mutation whose transaction rolls back (or whose
acknowledgement is lost) has no committed reference and no trigger-fired
queue row, so the flow above never sees it. The discovery scan (3.4A)
measures this population read-only; the reconciliation producer (3.4B)
closes the loop by enqueueing proven orphans as ordinary candidates through
the same queue.

The producer is disabled by default
(`RUSTSHARE_ORPHAN_RECONCILIATION_ENABLED=false`,
`RUSTSHARE_ORPHAN_RECONCILE_MAX_ENQUEUE=100`, grace from
`RUSTSHARE_OBJECT_GC_GRACE_PERIOD_HOURS`, listing budgets reused from
`RUSTSHARE_ORPHAN_SCAN_*`). Each listed key is canonical-filtered,
age-floored, then revalidated under the per-key advisory lock
(HeadObject existence and whole-second `LastModified` match, then the
global reference count) before an insert-only enqueue with reason
`orphan_reconciliation`; existing queue rows are never modified. A
`GetBucketVersioning` probe gates the run: only a confirmed unversioned
bucket proceeds. Non-goals: no deletion outside the worker flow above, no
support for versioned buckets (they block the run), no per-key detail in
any API response, and no automatic activation.

## Concurrency and recovery

Candidate leases prevent concurrent processing. The per-key advisory lock
prevents a writer from entering the put-to-metadata window while GC checks and
deletes, and prevents GC from entering while a writer owns that window.
Transactions release writer/GC locks on connection termination. A crashed
worker's durable candidate becomes claimable after the lease timeout.

## Operations

GC is disabled by default. Enqueueing remains enabled. Operators use Prometheus
counters/gauges for enqueue, processing, deleted, missing, referenced, invalid,
reference-check failures, delete failures, backlog, and oldest-candidate age.
There is no manual destructive API. Disable the worker for emergency response;
candidate history and enqueueing remain intact.

## Compatibility

The migration is additive over the existing queue. File, version, vault, mail,
attachment, note, upload, trash, retention, and API semantics are unchanged.
Rollback disables the worker and deploys the prior binary while retaining the
additive table columns; the prior worker continues to exclude content-addressed
keys.

