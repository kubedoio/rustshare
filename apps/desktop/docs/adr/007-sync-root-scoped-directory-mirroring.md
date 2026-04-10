# ADR 007: Sync Root Scoped Directory Mirroring

## Status: Accepted
## Date: 2026-04-09

## Context
The macOS desktop client was syncing file contents without treating directories as first-class sync state. In practice that caused three bad outcomes:

- uploads could be flattened into the sync root instead of preserving nested parents
- empty directories could not be mirrored because only files existed in the sync plan
- remote discovery could leak outside the configured `remote_path`, which made one sync root observe unrelated server content

This was not a one-line `mkdir -p` bug. The planner, executor, and remote discovery path all needed the same model of "a sync root mirrors exactly one remote subtree."

## Decision
The shared sync engine will treat directory structure as explicit sync state.

- Sync is scoped to the configured `SyncRoot.remote_path`
- Remote folder discovery is built from the backend folder tree, then filtered to that subtree
- Sync planning creates directory operations separately from file operations
- Directory creation runs before upload/download
- Directory deletion runs after child files are removed
- Uploads resolve and create the remote parent folder chain before sending file content

## Alternatives Considered

### Infer directories only from file parents
- Pros: Less planner surface area
- Cons: Empty directories disappear, ordering stays implicit, flattening bugs remain easy to reintroduce
- Rejected: not complete enough for a mirror-style sync client

### Keep global remote file listing and filter by hash/path heuristics
- Pros: Smaller change
- Cons: Wrong abstraction, cross-root bleed remains possible, hides path bugs instead of fixing them
- Rejected: too fragile

## Consequences
- **Pros**: Nested paths are preserved, empty folders sync, sync roots stay isolated, execution order is deterministic
- **Cons**: Planner and manager are more explicit, and remote sync now depends on both file listing and folder tree APIs
