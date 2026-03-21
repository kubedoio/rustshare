# RustShare Status

## Current State

RustShare is a late-MVP / pre-release file-sharing platform with a strong web product core and a deliberately narrower scope than Nextcloud. The project is no longer a prototype shell: the main web app, sharing flows, realtime updates, async replication, and operator recovery tooling are all implemented and working.

As of 2026-03-21, the implemented platform includes:

- secure web sessions with HTTP-only cookies
- OIDC groundwork for SSO, including mobile-oriented PKCE endpoints
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
- rate limiting for auth and public-share hot paths
- backup, restore, verification, and post-restore smoke tooling

## What Is Solid

### Backend

- `/api/...` routing is in place, with `/api/v1/...` as the main versioned surface
- Axum serves the SPA for all non-API routes
- replication is asynchronous and decoupled from upload response latency
- internal and public sharing routes are mounted and in use
- notification routes are live
- admin replication visibility endpoints exist

### Web app

- authenticated file browser is complete enough for daily use
- shared-with-me flows are implemented
- notifications are backend-backed and realtime-aware
- share modal supports internal recipients and public links
- public share pages support files, folders, and upload-only file-drop behavior
- realtime toasts and replication-state badges are implemented
- Docker builds serve the compiled frontend through the backend image

### Operations

- rate limiting exists for auth and public-share hot paths
- backup, restore, backup verification, and post-restore smoke scripts exist
- replication observability has summary and target-health endpoints plus a CLI helper
- a restore-drill workflow exists and has been exercised locally

## What Is Still Partial

### Product scope

- the light mobile client with photo backup and offline flows is still outstanding
- the Apple-first desktop app is only an early separate prototype, not a production client
- deeper admin dashboards and alerting remain partial

### Technical debt

- the frontend currently builds successfully, but still emits SvelteKit/Svelte runtime mismatch warnings during production build
- some legacy or compatibility endpoints and historical notes still exist and need cleanup
- OIDC still needs end-to-end validation against the intended production identity provider

### Production hardening

- alerting and long-term observability need to go beyond the current summary endpoints and helper scripts
- a full production-like disaster recovery rehearsal should be repeated and documented periodically

## Recommended Maturity Label

Use this project status in docs and discussions:

**Late MVP / pre-release, web product nearing a careful launch**

That is more accurate than:

- “basic MVP” because the product is well beyond that
- “production-ready platform” because client work and some hardening remain

## Current Completion Estimate

- Web file-sharing product: roughly `94-96%`
- Broader product including mobile and desktop clients: roughly `70-75%`

These are directional engineering estimates, not a guarantee of launch readiness.

## Immediate Priorities

1. Stabilize the frontend dependency mismatch so Docker/frontend builds stop emitting runtime compatibility warnings.
2. Validate OIDC end to end against the actual identity provider intended for launch.
3. Continue observability and alerting work around replication health.
4. Continue the light mobile client as the next major product phase.

## Validation Snapshot

The main validation loop has already been exercised successfully in this workspace:

- `cargo check --workspace`
- `cargo test --workspace`
- `npm run check`
- `npm test`
- `docker compose build --no-cache backend`

The remaining work is no longer “make the project basically work.” It is launch hardening, technical debt cleanup, and the next client phases.
