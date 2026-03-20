# Rustshare Storage Decision

## Status

Accepted for the file-sharing-lite rewrite.

## Decision

RustFS remains the authoritative primary blob store for Rustshare.

Rustshare itself owns:
- authentication and sessions
- file and folder metadata
- shares and permissions
- audit trails
- replication state
- durable replication jobs
- realtime websocket events

Rustshare does not become the primary file-placement engine on local disk.

## Request Path

For uploads and file replacements:

1. Axum receives the upload.
2. The backend writes the blob to RustFS primary storage.
3. The backend persists metadata and a `file_versions` record.
4. The backend marks the version as `primary_written`.
5. If replica targets are configured, the backend creates a durable replication job and moves the version to `queued`.
6. The API returns `200 OK` immediately after the primary write and metadata commit succeed.

The request path never waits for cross-location replication to finish.

## Replication Model

Replication is strictly asynchronous.

Background workers will:
- lease queued jobs
- copy from RustFS primary storage to configured replica targets
- retry transient failures with backoff
- update file version state to `syncing`, `fully_replicated`, `degraded`, or `failed`
- publish progress over `/api/ws`

The database is the source of truth for replication progress and recovery.

## Why This Model

This keeps the product aligned with the intended boundaries:
- RustFS provides strongly consistent primary object storage
- uploads complete fast because only the primary write is on the critical path
- replication remains resilient and observable instead of hidden in request latency
- Rustshare stays lightweight by managing metadata and workflow instead of raw storage placement

## Current Foundation Added

This slice introduces:
- `file_versions.replication_state`
- `replication_targets`
- `replication_jobs`
- service-layer queueing after successful primary RustFS writes

Worker leasing, retry execution, and admin replication views are still to be implemented.
