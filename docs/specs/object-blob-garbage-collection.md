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

No request path deletes blobs. No bucket scan is used. Database or object-store
uncertainty schedules retry and leaves the object intact.

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

