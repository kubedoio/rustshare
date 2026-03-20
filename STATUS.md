# RustShare Status

## Current State

RustShare is a feature-rich late-MVP file sharing application with a working Rust backend and a substantially implemented SvelteKit frontend. The project is no longer at the "flat file list only" stage, but it is also not fully production-ready.

As of 2026-03-19, the codebase supports:

- JWT login and authenticated app routes
- File upload, download, rename, delete, move, preview, and version history
- Folder creation, navigation, rename, delete, and move
- Public share links with optional password and expiry
- WebSocket real-time sync with browser-compatible token auth
- Dockerized local deployment with PostgreSQL, MinIO, frontend, backend, and nginx

## What Is Solid

### Backend

- Cargo workspace builds successfully with `cargo check --workspace`
- Default Rust test suite now runs successfully with `cargo test --workspace`
- Server unit tests pass
- Core, storage, auth, and infrastructure crates all participate in the passing workspace run

### Frontend

- Main authenticated file browser is implemented
- Folder-aware navigation is implemented
- Preview and version history modals are implemented
- WebSocket client and query invalidation are implemented
- Settings, shares, notifications, and dashboard routes exist

## What Is Still Partial

### Backend routes not wired into the live server

The following backend handlers exist in source, but their routes are currently commented out in `backend/server/src/main.rs`:

- User-to-user share creation and recipient management
- Received shares listing
- Persistent notification APIs

Because those routes are not mounted, related frontend pages cannot be considered complete.

### Frontend placeholders

- `Shared with Me` is still a "Coming Soon" page
- `Notifications` currently shows local activity feed data, not backend notifications
- `Settings` still has placeholder account actions such as password change
- The "all user shares" API client is still a placeholder and returns an empty list

### Verification gaps

- Frontend automated tests were not run from this workspace because local JS test tooling is not installed
- Docker services are defined, but no local containers were running during the latest verification pass

## Recommended Maturity Label

Use this project status in docs and discussions:

**Late MVP / pre-release**

That is a better fit than either of these older descriptions:

- "flat-list MVP only" - outdated
- "fully production-ready" - overstated

## Immediate Priorities

1. Wire or hide unfinished user-sharing and notification features
2. Replace placeholder frontend APIs with real backend endpoints
3. Install frontend dependencies and verify `npm test`, `npm run check`, and `npm run build`
4. Start the Docker stack and validate the documented end-to-end flow

## Verification Snapshot

Verified locally on 2026-03-19:

- `cargo check --workspace`: passed
- `cargo test --workspace`: passed
- `cargo test -p rustshare-server`: passed
- `npm test`: not runnable in this workspace because `vitest` is not installed locally
- `docker compose ps`: no running services at time of check
