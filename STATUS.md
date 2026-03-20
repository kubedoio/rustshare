# RustShare Status

## Current State

RustShare is a late-MVP / pre-release file-sharing platform with a strong web product core and a narrower scope than full Nextcloud. The current emphasis is secure file and folder sharing, public links, internal collaboration, realtime updates, and asynchronous replication.

As of 2026-03-20, the implemented platform includes:

- secure web sessions with HTTP-only cookies
- OIDC groundwork for SSO
- Axum-served SvelteKit SPA runtime
- file and folder CRUD with version history and trash
- internal user-to-user sharing
- public file links
- public folder links
- upload-only public folder links
- persistent notifications and unread counts
- websocket-driven realtime updates
- RustFS-compatible primary storage
- asynchronous replication worker with database-tracked replication states
- backup, restore, verification, and post-restore smoke tooling

## What Is Solid

### Backend

- `/api/...` routing is in place, with `/api/v1/...` as the main versioned surface
- Axum serves the SPA for all non-API routes
- replication is asynchronous and no longer tied to request completion
- internal and public sharing routes are mounted and in use
- notification routes are live
- admin replication visibility endpoints exist

### Frontend

- authenticated file browser is complete enough for daily use
- shared-with-me flows are implemented
- notifications are backend-backed and realtime-aware
- share modal supports internal recipients and public links
- public share pages support files, folders, and upload-only file-drop behavior
- realtime toasts and replication-state badges are implemented

### Operations

- rate limiting exists for auth and public-share hot paths
- backup, restore, backup verification, and post-restore smoke scripts exist
- replication observability has summary and target-health endpoints plus a CLI helper

## What Is Still Partial

### Product scope

- the light mobile client with photo backup and offline flows is still outstanding
- anonymous/public upload attribution in audits is still not as strong as it should be
- deeper admin dashboards and alerting remain partial

### Operational proof

- a real restore drill against a production-like backup artifact should still be executed and recorded
- OIDC should be validated end to end against the actual identity provider intended for launch

### Documentation cleanup

- the current top-level docs are now aligned, but some historical notes elsewhere in the repository may still describe older architecture phases

## Recommended Maturity Label

Use this project status in docs and discussions:

**Late MVP / pre-release, web product nearing a careful launch**

That is more accurate than:

- “flat-list MVP only” because the product is far beyond that
- “fully production-ready platform” because mobile and some hardening work remain

## Current Completion Estimate

- Web file-sharing product: roughly `93-96%`
- Broader product including light mobile sync/photos: roughly `68-73%`

These are directional engineering estimates, not a guarantee of launch readiness.

## Immediate Priorities

1. Run and document a real restore drill against a realistic backup bundle.
2. Tighten anonymous/public upload attribution in audits and events.
3. Continue observability and alerting work around replication health.
4. Start the light mobile client as the next major product phase.

## Validation Snapshot

The main validation loop has already been exercised successfully in this workspace:

- `cargo check --workspace`
- `cargo test --workspace`
- `npm run check`
- `npm test`

The remaining work is no longer “make the project basically work.” It is mostly launch hardening, operational proof, and the next product phase.
