# ADR-0039: Orphan Blob Discovery and Reconciliation

Status: Accepted (Phase 3.4A discovery); Phase 3.4B reconciliation producer Implemented 2026-10-13 (disabled by default, not activated)
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
  scope for discovery. The 3.4B producer resolves the caveat by hard-gating:
  it probes `GetBucketVersioning` before listing and refuses to run
  (HTTP 409) unless the bucket reports `Unversioned`.
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

## Phase 3.4B reconciliation producer (implemented 2026-10-13; disabled by default, not activated)

Discovery alone reclaims nothing. Phase 3.4B implements the proposed
**durable candidate producer** as `backend/server/src/object_gc_reconcile.rs`,
exposed as `POST /api/admin/object-gc/orphan-reconcile` (same `/api/admin/...`
surface and `AdminUser` gating as the dry-run scan; 403 when disabled, 409
when the bucket's versioning state is unsafe or unknown, 400 on invalid
configuration). The dry-run scanner is untouched and remains strictly
read-only. The proposal's safety properties hold, with three deliberate
strengthenings over the original sketch:

- **Insert-only enqueue.** The producer never calls
  `enqueue_object_gc_candidate`: its conflict clause resets
  `operator_hold`, worker leases, attempt history, and terminal states, so a
  discovery pass could silently release an operator hold or revive a
  terminal row (characterization evidence:
  `characterization_enqueue_resets_operator_held_row` in
  `backend/tests/object_gc_test.rs`). Instead it uses the new
  `MetadataStore::enqueue_object_gc_candidate_if_absent` — `INSERT ...
  ON CONFLICT (object_key) DO NOTHING` returning whether a fresh `pending`
  row was inserted. An existing row in any state — `pending`, leased,
  backed-off, terminal, or operator-held — is left completely untouched, and
  `not_before` is never shortened for an existing row.
- **Versioning gate, fail closed.** Before listing anything the producer
  probes `GetBucketVersioning`. Only a bucket reporting `Unversioned`
  proceeds; `Enabled` or `Suspended` (a suspended bucket was previously
  versioned and may still hold non-current versions) blocks the run, as do
  an `Unknown` state or a failed probe (409). This resolves the "versioning
  unresolved" caveat by hard-gating on it: the producer does not support
  versioned buckets, it refuses to run against them.
- **Object-replacement revalidation.** Under the per-key lock the producer
  re-`HeadObject`s each candidate and compares `LastModified` with the
  listed value at whole-second precision (the coarsest precision
  S3-compatible stores report via HeadObject), re-checking the age floor
  from the head timestamp. A missing object counts `skipped_missing`; a
  changed or unreadable timestamp counts `uncertain_s3` — never an enqueue
  decision.

Per-key sequence (each step's failure degrades into its report counter
instead of aborting the run):

1. Canonical `blobs/<64 hex>` filter; non-canonical keys count
   `keys_invalid`.
2. Listed-age floor (`RUSTSHARE_ORPHAN_SCAN_MIN_AGE_HOURS`, default 24 h);
   younger objects count `objects_too_recent`.
3. Per-key PostgreSQL advisory lock (`hashtextextended(key, 7277856)`),
   held through step 6 — the same lock every writer holds across its
   put-to-metadata window.
4. HeadObject revalidation: existence, whole-second `LastModified` match
   against the listing, then the age floor again.
5. Global seven-table reference count; any error is fail-closed
   (`uncertain_db`, `db_reachable: false`) and never read as zero
   references; non-zero references count `skipped_referenced`.
6. Insert-only enqueue: `Ok(true)` counts `enqueued`; `Ok(false)` counts
   `skipped_operator_holds` when the pre-existing row is operator-held and
   `skipped_existing_queue_rows` otherwise.

Budgets are the scanner's (≤1000 keys per invocation shared with the
operator `limit`, listing-call cap `max(keys, 100)`, resumable opaque
continuation token) plus a new per-run enqueue budget
`RUSTSHARE_ORPHAN_RECONCILE_MAX_ENQUEUE` (default 100, hard-capped at the
scan key budget — an enqueue cap above what one run can inspect would hide
misconfiguration). When the enqueue cap trips, the run stops mid-page and
returns the token that yielded the current page; resuming re-scans that
page, which is safe because enqueueing is insert-only and idempotent
(already-enqueued keys classify `skipped_existing_queue_rows`). A listing
failure ends the run `partial_listing_error` with partial counters intact —
a partial picture never poses as a complete one.

Reports carry aggregate counts only: no object contents, no per-key detail,
no first/last keys. Metrics:
`object_gc_orphan_reconcile_runs_total{status, listing_complete}`,
`object_gc_orphan_reconcile_enqueued_total`, and — on each fresh insert —
the existing `object_gc_candidates_enqueued_total{reason="orphan_reconciliation"}`,
which distinguishes discovery-produced candidates from trigger-produced
ones in metrics and row history.

Race handling is as proposed: a reference created after the enqueue-time
re-check is caught by the worker's first reference check; one created
between the worker's two checks is caught by the second; a writer in its
put-window cannot overlap any producer or worker check because all of them
take the same per-key advisory lock (tests d and e). Residual caveat: the
HeadObject revalidation compares timestamps at whole-second precision, so a
replacement whose head timestamp lands in the *same* wall-clock second as
the listing is invisible to the producer. That window is negligible for
content-addressed keys and is backstopped by the GC worker, which
re-validates references under the same lock immediately before any delete.

Configuration is parsed per request and validated only when enabled — a
disabled invocation fails 403 even with leftover bogus values, mirroring the
scanner:

- `RUSTSHARE_ORPHAN_RECONCILIATION_ENABLED` — master switch, default off.
- `RUSTSHARE_ORPHAN_RECONCILE_MAX_ENQUEUE` — default 100, ≤ scan key budget.
- `RUSTSHARE_OBJECT_GC_GRACE_PERIOD_HOURS` — grace recorded on freshly
  enqueued rows, default 24, minimum 1.
- `RUSTSHARE_ORPHAN_SCAN_MAX_KEYS` / `RUSTSHARE_ORPHAN_SCAN_MIN_AGE_HOURS`
  — reused listing budgets.

Rollback is `RUSTSHARE_ORPHAN_RECONCILIATION_ENABLED=false` (or unsetting
it); no other rollback is needed. The producer only ever inserts fresh
`pending` candidates, so rows already produced remain ordinary queue rows
governed by the established GC contract — worker eligibility, grace,
operator holds, and audit semantics are unchanged.

### Privacy-preserving operator key review (proposal)

Before activation an operator may want to inspect sample keys behind a
`potential_orphan` count. Reports intentionally never expose per-key detail
through the admin API, and that boundary must not be relaxed. Proposed
procedure (not yet implemented): the operator runs the dry-run scan
server-side with per-key classifications written to an operator-only log or
file on the server host, or runs a psql query joining `object_gc_queue`
against a store listing — either way access stays inside the operator's
existing server/database credentials, and per-key classifications never
appear in any API response.

### GO/NO-GO status for 3.4B

Satisfied by implementation and tests (20 tests in
`backend/tests/object_gc_orphan_reconcile_test.rs`, all green against
disposable PostgreSQL/RustFS; mutation probes M1–M6 all detected):

- Insert-only semantics: existing rows are never disturbed — operator hold
  (test h), in-flight lease, attempts, and schedule (test i), backoff floor
  (test j), terminal states (test k), concurrent producers elect a single
  inserter (test l).
- Disabled by default: a disabled invocation enqueues nothing (test q) and
  surfaces 403, not a configuration error.
- Auditability: every inserted row carries reason `orphan_reconciliation`;
  report and metric cardinality stays bounded.
- Bounded execution: scanner budgets plus the per-run enqueue cap, with
  resumable continuation (cap and partial-listing tests).
- Race contract: a reference created between discovery and enqueue prevents
  the enqueue (test d); reference creation serializes with enqueue through
  the shared advisory lock (test e); cross-tenant and all other reference
  families protect (tests f, g); database outage produces no candidate
  (tests m, m2).
- Versioning gate: non-unversioned states are rejected and a probe failure
  fails closed (tests p), and a real versioned bucket is blocked with no
  enqueue (test p2).

Still open before production activation — activation remains **NO-GO**
until all are done:

1. Operator review of bounded dry-run evidence on the target production
   bucket (measured orphan population, no `uncertain_db_error` storms,
   sample keys inspected via the privacy-preserving procedure above).
2. Independent security review sign-off.
3. Target-bucket versioning verification (`GetBucketVersioning` confirms
   `Unversioned` on the production bucket before the first enabled run).
4. Operational rollback walkthrough (disable the flag, confirm queued
   candidates drain per the established contract).
