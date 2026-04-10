# ADR 008: Delete Tombstones and Idempotent Delete Propagation

## Status: Accepted
## Date: 2026-04-09

## Context
The current desktop client can recreate files after the user deletes them locally or remotely. The root cause is that the planner only understands three inputs:

- present locally
- present remotely
- present in local DB state

That model is not enough to represent intentional deletion. If a path disappears on one side and the client no longer has trustworthy synced state for it, the planner falls back to "upload the existing copy" or "download the existing copy." That makes deleted files reappear.

The issue is amplified by two implementation details:

- successful deletes do not persist a durable tombstone in the local state store
- delete operations are not treated as idempotent when the target is already gone

## Decision
The shared sync engine will persist delete tombstones for previously synced files and treat delete operations as idempotent.

- `file_states` will store explicit tombstone metadata per path
- successful delete propagation will update the DB to a tombstoned state instead of silently leaving stale synced state behind
- planner decisions will consider tombstones before treating a one-sided file as a new create
- delete operations that hit `ENOENT`, `404`, or `410` will be treated as already applied
- synced file state will distinguish remote content hash from remote file identifier

## Alternatives Considered

### Keep inferring delete intent from absence only
- Pros: Minimal schema change
- Cons: Cannot distinguish intentional delete from missing state or partial re-index
- Rejected: this is the direct cause of file resurrection

### Delete DB rows immediately after every successful delete
- Pros: Simpler state store
- Cons: Loses the only durable signal that the path was intentionally deleted
- Rejected: reintroduces recreate loops after restart or laggy backend state

## Consequences
- **Pros**: Delete propagation becomes deterministic, repeated deletes become safe, and planner decisions are explainable
- **Cons**: The local DB schema and planner model become more explicit, and tombstones need pruning logic in later work
