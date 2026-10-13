# ADR-0039: Orphan Blob Discovery and Reconciliation

Status: Accepted (Phase 3.4A discovery); Phase 3.4B reconciliation producer Proposed
Date: 2026-10-13

## Context

PR #348 (`cb429f56`) pinned an object-lifecycle gap: a blob written by a
mutation whose transaction rolls back (or whose acknowledgement is lost) is
never referenced by any committed row, so the DELETE/UPDATE-triggered
`object_gc_queue` never sees it — no committed row ever existed to fire a
trigger. `object_store_failure_atomicity_test` asserts the orphan is present in
the bucket and absent from the queue, and states reclamation "needs a separate
design".

Reclaiming such blobs requires *finding* them. Today `ObjectStore` has no list
method and the deletion path deliberately relies on exact, durable queue keys
rather than bucket scans. Any discovery mechanism must not weaken the existing
safety story:

- All writers hold the per-key advisory lock across the put-to-metadata window;
  the GC worker re-checks references under that lock before deleting.
- `potential_orphan`-style observations are point-in-time; a blob observed with
  zero references can be referenced a moment later.
- Query failure is never read as zero references.
- The only path that may delete a blob is the existing worker flow
  (advisory lock → lease validation → reference checks → existence → delete).

## Decision

Phase 3.4A adds a **read-only, dry-run orphan-discovery scanner**. It never
deletes, never enqueues, and never mutates anything.

- A new `BlobLister` trait in `backend/server/src/object_gc_scan.rs` is a
  list-only seam: one method returning a page of keys with `last_modified`
  and size. `ObjectStore` implements it with a bounded `list_objects_v2`
  call under the `blobs/` prefix; the concrete `ObjectStore` type never
  enters the scan entry point as anything other than `&dyn BlobLister`, so
  the scanner is unable to mutate object storage by construction. The
  `ObjectStoreOps` trait used by mocks is untouched.
- `scan_orphan_blobs_dry_run` classifies each listed key:
  1. Non-canonical keys fail `is_content_addressed_blob_key` and are
     `excluded_invalid_key` (legacy uppercase hex remains valid).
  2. Objects younger than `min_object_age` (default 24 h, configurable) are
     `recent` and never considered for orphanhood. Age is a floor for
     *consideration*, not proof of safety.
  3. A failed global reference query (`count_blob_references`) is
     `uncertain_db_error`; the report still returns with
     `db_reachable: false` and bounded reasons. Failure is never read as
     zero references.
  4. More than zero durable references is `referenced`.
  5. Zero references with a `object_gc_queue` row is `queued_<state>`;
     without one it is `potential_orphan` — "zero durable references
     observed at inspection time", **not** deletion-safe.
  A hard listing failure aborts the invocation with `ScanError::List` rather
  than returning a partial picture.
- Reports carry aggregate counts only. Per-key classifications exist in
  memory for tests but are never serialized into the HTTP response, and no
  object contents or per-key sizes leave the process.
- An admin-only endpoint `POST /api/admin/object-gc/orphan-scan` (same
  `/api/admin/...` diagnostics surface and `AdminUser` gating as the
  replication diagnostics) accepts `{ dry_run, limit, min_age_hours,
  continuation_token }`. `dry_run` must be present and true — automatic
  deletion does not exist in this build; anything else is a 400. The limit
  is clamped to a 1000-key hard cap. Each invocation classifies at most
  `limit` keys (one page may overshoot) and returns a continuation token so
  the operator can page through large buckets. Like the replication
  diagnostics, the route is not part of the committed OpenAPI spec.
- `MetadataStore` gains a read-only `get_object_gc_candidate(object_key)`
  returning the full queue row (all columns, no mutation).

### Point-in-time state distinction

The lifecycle now distinguishes five observation states, in order of
increasing strength:

1. **listed** — a key appeared in a listing page. No claim is made about it.
2. **potential orphan** — zero durable references were observed at inspection
   time. This is a statistic, not a verdict.
3. **confirmed zero at inspection** — Phase 3.4B's enqueue-time re-check under
   the per-key advisory lock observes zero references again. Still not
   deletion-safe: a writer can acquire the lock immediately after the check.
4. **durable GC candidate** — a row in `object_gc_queue` produced only by the
   existing enqueue path, carrying a `not_before` grace that is never
   shortened by discovery.
5. **deletion-eligible** — the existing worker flow validates the lease,
   re-checks references twice under the advisory lock, confirms existence,
   and deletes.

Discovery operates at levels 1–2 only. Nothing at level 2 may skip levels
3–5.

## Consequences

- Operators can measure the orphan population (and the `recent`/queued/
  referenced mix) before any irreversible mechanism exists.
- The scanner is safe to run against production buckets: bounded pages,
  read-only database queries, no locks taken, no queue writes.
- `ObjectStore` gains a `list_page` method. The deletion path continues to
  use no bucket scan; only discovery lists, and only under `blobs/`.
- Two new `RUSTSHARE_ORPHAN_SCAN_*` env vars bound scan behaviour; bogus
  values surface as scan-time configuration errors, not startup failures.
- The classification vocabulary is fixed and serialized
  (`referenced`, `queued_<state>`, `potential_orphan`, `recent`,
  `excluded_invalid_key`, `uncertain_db_error`) so operators can
  build dashboards on stable JSON keys. A listing failure does not appear in
  the report at all: it aborts the invocation with a bounded error surfaced
  as HTTP 503, while per-key database failures classify `uncertain_db_error`
  and are reported in-band.

### Evaluation of limits and interactions

- **Pagination.** Listing follows S3 continuation tokens and classifies
  whole pages; a budget-limited invocation may overshoot by one page and
  returns the token that resumes after the last classified page. Resumption
  is best-effort consistent: keys added behind the token may be skipped in
  this pass, keys re-created in an already-scanned region may be observed
  again on a later pass. Repeated full scans converge to a stable result
  under a quiescent bucket (asserted by test).
- **`LastModified` limits.** `LastModified` is server-assigned at write
  time; it cannot be backdated, so tests use a tiny `min_object_age` or
  canned `last_modified` values. Clock skew between bucket and application
  hosts makes age a hint; a young-looking `LastModified` only suppresses
  orphan consideration, never enables deletion.
- **Consistency.** No snapshot isolation exists across listing and the
  per-key reference query; classification is explicitly point-in-time
  (test M pins that a reference acquired between scans flips the result).
- **Concurrent PUTs.** A concurrent writer holds the advisory lock only
  around its own window; discovery does not take the lock, so it can
  observe a key mid-write (listed but not yet referenced). `recent` (age
  floor) plus the Phase 3.4B enqueue-time re-check cover the steady-state
  risk; no discovery observation authorises deletion.
- **Versioning.** Buckets are treated as unversioned (current RustFS
  deployments). With versioning enabled, list-and-delete of the latest
  object could strand non-current versions; that configuration is out of
  scope for discovery and would need explicit handling before 3.4B.
- **DB outages.** Per-key query failures degrade to `uncertain_db_error`
  with `db_reachable: false` and the report still returns; a listing
  failure aborts with `Err`. Query failure is never interpreted as zero
  references.
- **Global dedup.** References are counted globally with no tenant
  predicate; another tenant's row protects a blob (test F). This mirrors
  `count_blob_references` and the writer/collector contract.
- **Budgets.** Each invocation is bounded by `max_keys_per_invocation`
  (default 500, hard cap 1000, operator `limit` clamped to the same cap)
  and one page of listing at a time, keeping database query load linear
  and cancellable between invocations.
- **Resume.** The continuation token is the object-store's opaque token,
  never a key-derived marker; the next invocation starts exactly one page
  after the last classified page. Tokens are best-effort under concurrent
  writes (see Pagination). An invocation also stops after at most
  `max(max_keys_per_invocation, 100)` listing calls, so empty-but-tokened
  pages under churn cannot inflate call count unboundedly; the current
  continuation token is returned and the scan resumes normally.
- **Multi-instance.** Any number of servers can scan concurrently: all
  queries are read-only, no leases are taken, and identical scans return
  identical reports under a quiescent bucket. There is no coordination
  requirement.
- **Repeated scans.** Cheap and idempotent in effect: identical inputs
  produce identical classifications when nothing changed (test J), and
  nothing accumulates — no queue rows, no audit rows, no metrics with
  unbounded cardinality.
- **Holds.** A queued key in `operator_hold` classifies as
  `queued_operator_hold`; discovery never overrides an operator hold and
  never reports a held key as a potential orphan.

## Phase 3.4B proposal (not implemented)

Discovery alone reclaims nothing. The proposed Phase 3.4B is a **durable
candidate producer** with these properties:

- Reuses `enqueue_object_gc_candidate` with reason
  `orphan_reconciliation` — the existing queue and worker stay the only
  path to deletion; the worker's advisory-lock/double-reference-check flow
  is never bypassed.
- Before enqueueing a discovered `potential_orphan`, re-checks references
  **under the per-key advisory lock** (level 3) and enqueues only on a
  second zero. The enqueue's `not_before` is never shorter than the
  standard grace period; discovery never shortens `not_before`/grace.
- Disabled by default (`RUSTSHARE_ORPHAN_RECONCILIATION_ENABLED=false`);
  enabling is an explicit operator action. Rollback is disabling the flag;
  already-enqueued durable candidates remain ordinary queue rows governed
  by the existing worker and operator-hold semantics.
- Auditable through the queue: reason `orphan_reconciliation` distinguishes
  discovery-produced candidates from trigger-produced ones in metrics and
  row history.
- Race handling: a reference created after the enqueue-time re-check is
  caught by the worker's own first reference check; a reference created
  between the two worker checks is caught by the second check; a writer in
  its put-window cannot overlap either check because all writers hold the
  same per-key advisory lock.

### GO/NO-GO criteria for 3.4B

GO requires all of:

1. Phase 3.4A dry-run evidence: measured orphan population, no
   `uncertain_db_error` storms, and operator review of sample keys from
   the production bucket.
2. A 3.4B implementation that enqueues only after an advisory-locked
   zero-reference re-check and never shortens grace.
3. Contract tests proving: a reference acquired after discovery but before
   enqueue prevents the enqueue; enqueueing uses only
   `enqueue_object_gc_candidate`; the worker remains the only deleter; and
   disabling the flag leaves queued candidates untouched.
4. Updated `docs/contracts/object-blob-lifecycle-contract.md` making the
   producer an allowed candidate source and documenting the new reason.

NO-GO if any of: listing proves too expensive or slow at production bucket
sizes; `recent` misclassification is observed around `min_object_age`
boundaries; the enqueue-time re-check cannot close the race within the
existing lock discipline; or operators cannot reliably distinguish
`orphan_reconciliation` candidates from trigger candidates in audits.
