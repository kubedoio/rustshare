# ADR 005: Conflict Handling Policy

## Status: Accepted
## Date: 2026-04-05

## Context
When both local and remote files change between sync cycles, the client cannot safely determine which version should "win."

## Decision
For Phase 1, we will adopt a conservative conflict-handling policy. 
- All conflicts will result in the creation of a "conflict copy." 
- No automatic text-level merging (even for `.txt` or `.md` files). 
- Filename pattern: `<filename> (Conflict <timestamp>).<extension>`. 
- Both the local version and the remote version will be preserved. 
- Conflict events will be recorded in the `ConflictRecord` in the local metadata store.

## Alternatives Considered
- **Www-level (Last Writer Wins)**: Causes silent data loss; unacceptable for a sync client.
- **Auto-merge**: High complexity for Phase 1; requires specific merge logic per file type.

## Consequences
- **Pros**: Zero data loss, simple to implement and test.
- **Cons**: Users must manually resolve (delete) the unwanted copy.
