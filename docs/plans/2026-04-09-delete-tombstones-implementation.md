# Delete Tombstones Implementation Plan

## Goal
Stop deleted files from being recreated by the mac client after local or remote deletion, and make delete propagation reliable for root-level and subtree sync roots.

## Problem Summary
The current engine only compares local presence, remote presence, and coarse DB state. That is not enough to represent intentional deletion. As a result:

- a missing file with no trusted DB state is treated as a new create
- successful deletes do not leave durable delete intent behind
- delete operations are not idempotent when the target is already gone
- file state mixes remote content hash and remote object id

## Scope
This implementation slice covers:

1. DB schema support for delete tombstones and explicit remote file ids
2. Planner logic that respects tombstones
3. Executor behavior that records tombstones after successful delete propagation
4. Idempotent local and remote delete handling
5. Tests for the new delete semantics

## Non-Goals
- Finder/File Provider integration
- Trash/version-history UI
- Full rename detection
- Empty-file transport redesign beyond existing behavior

## Design

### DB Model
Extend `file_states` with:

- `remote_file_id`
- `tombstone_side`
- `tombstone_at`

Use `sync_status = "tombstone"` for deleted paths that should remain deleted until one side intentionally recreates them.

### Planner Rules
- `local + remote + synced-state`: existing compare logic
- `local + remote + tombstone`: treat as a conflict or newest-wins reconciliation
- `local + no remote + tombstone`: upload, because the local side intentionally recreated the path
- `no local + remote + tombstone`: download, because the remote side intentionally recreated the path
- `no local + no remote + tombstone`: no-op, deletion is stable
- `local + no remote + synced-state`: delete local, because remote delete wins
- `no local + remote + synced-state`: delete remote, because local delete wins

### Executor Rules
- On successful `DeleteLocal`, mark a tombstone with `source_side = "remote"`
- On successful `DeleteRemote`, mark a tombstone with `source_side = "local"`
- Treat `ENOENT`, `404`, and `410` as already-applied deletes and still record the tombstone
- On successful upload/download, replace any tombstone with fresh synced state

### Verification
- Unit tests for planner tombstone behavior
- DB round-trip tests for tombstone fields
- Existing sync-engine and desktop test suites
- Live `sync doctor` check on root `8cc8ba70-adb6-4898-acc0-6c04328b8157`

## Implementation Steps

1. Add tombstone fields to `client-state` schema and models.
2. Add DB helpers for writing tombstones.
3. Refactor planner DB input to include tombstone metadata.
4. Add tests for tombstone-aware planning.
5. Record tombstones after local and remote deletes.
6. Make delete execution idempotent on already-missing targets.
7. Fix file-state integrity so `remote_hash` stores content hash and `remote_file_id` stores object id.
8. Run tests and live diagnostics.
