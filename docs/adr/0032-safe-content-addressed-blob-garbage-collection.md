# ADR 0032: Safe Content-Addressed Blob Garbage Collection

## Status

Accepted for implementation; deletion requires human review before merge.

## Context

RustShare writes `blobs/<sha256>` before committing metadata. This is the safe
failure ordering, but failed conditional vault writes can leave unreferenced
objects. Keys are globally shared by files, versions, vaults, and mail, so
tenant-local or operation-local deletion would risk valid data. The existing
GC queue deliberately excludes content-addressed keys because reference checks
alone do not close the concurrent writer race.

## Decision

Extend the existing durable candidate queue and retention-worker pattern into
a bounded candidate-driven collector. Require an exact key shape, 24-hour
default grace period, exclusive candidate lease, global structured reference
checks, a second immediate check, idempotent deletion, and conservative retry.

All production content-addressed writers and GC share a PostgreSQL advisory
lock derived from the full key. Writers hold it from before object put through
metadata completion; GC holds it from before its first check through object
deletion and candidate completion. Database or object-store uncertainty fails
closed.

Referenced candidates become terminal `referenced` rows. A later removal or
failed write re-enqueues the same unique row, which resets it to `pending`
without shortening an active safety delay. This keeps the first implementation
bounded without periodic rescans.

## Consequences

- Dangling vault blobs are reclaimed asynchronously without changing upload or
  conflict API semantics.
- Shared same-hash content remains protected across all tenants and features.
- Blob writers briefly hold one database advisory lock while performing an
  object-store put. Contention exists only for the same hash.
- Candidate history provides operator evidence and can grow over time; bounded
  terminal-row pruning is deferred until measurements justify it.
- Bucket-wide reconciliation is deferred. It may be added only as a bounded,
  resumable candidate producer and may never bypass this deletion contract.

## Rollback

Disable the worker first. Candidate enqueueing and the additive table remain
safe. Rolling application code back leaves candidates inert because the prior
worker excludes `blobs/<sha256>`. The additive schema must remain until all
nodes that understand it are retired; no rollback deletes objects or restores
already deleted unreferenced objects.
