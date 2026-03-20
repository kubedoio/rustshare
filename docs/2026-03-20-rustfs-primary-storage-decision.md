# RustFS Primary Storage Decision

Date: 2026-03-20
Status: Accepted

## Decision

Rustshare will use RustFS as the primary storage backend for file blobs.

Rustshare itself will not treat local filesystem paths as the authoritative file-content store.

## Why

- RustFS provides the primary blob-storage primitives we need already.
- It keeps blob storage concerns out of application business logic.
- It matches the clarified architecture boundary:
  - Rustshare owns metadata, auth, permissions, sharing, audit, and replication state.
  - RustFS owns primary blob persistence.
- It preserves the required user-facing behavior:
  - upload succeeds after the primary write succeeds
  - cross-location replication remains strictly asynchronous

## Required Runtime Behavior

1. Client uploads through Rustshare in MVP.
2. Rustshare writes the blob to RustFS primary storage.
3. Rustshare commits metadata and sets replication state to `primary_written`.
4. Rustshare returns `200 OK`.
5. A background worker performs asynchronous replication to additional targets.
6. Replication state transitions are tracked in the database and pushed over WebSocket.

## Non-Goals

- Rustshare does not manage local disk file placement as the primary source of truth.
- Rustshare does not wait for secondary replicas before acknowledging upload success.
- Rustshare does not expose long-lived raw storage credentials to web clients.

## Practical Consequences

- Keep `storage_key` as the authoritative blob locator in metadata.
- Add replication-state fields to file-version metadata.
- Keep RustFS in Docker/local environments as a first-class service.
- Prefer short-lived signed RustFS URLs or validated backend streaming for downloads.

## Follow-on Work

- implement replication jobs/attempt tracking
- add worker retry/backoff for remote target failures
- decide whether large-file direct upload to RustFS is needed after MVP correctness is stable
