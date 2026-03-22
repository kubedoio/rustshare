# Compatibility Removal Plan

Date: 2026-03-21

## Purpose

Turn the compatibility surface inventory into an actual removal plan for later cleanup.

Use this with:

- [Compatibility Surface Inventory](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-compatibility-surface.md)
- [API Contract Freeze](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-api-contract-freeze.md)

## Removal Order

### Wave 1: Realtime aliases

Candidates:

- `GET /api/v1/ws`
- `GET /api/sync`

Why first:

- canonical replacement is clear: `GET /api/ws`
- current frontend and active docs are already aligned
- lowest client-surface ambiguity

Risk:

- old local tooling or historical test scripts may still reference them

Required check before removal:

- search scripts, tests, and clients for remaining consumers

Status:

- completed on 2026-03-21
- backend websocket aliases removed
- active frontend, tests, and current source-of-truth docs aligned to `GET /api/ws`

### Wave 2: Legacy auth aliases

Candidates:

- `/api/auth/config`
- `/api/auth/login`
- `/api/auth/logout`
- `/api/auth/oidc/login`
- `/api/auth/oidc/callback`
- `/api/auth/oidc/mobile/authorize`
- `/api/auth/oidc/mobile/exchange`

Why second:

- stable versioned replacements already exist under `/api/v1/auth/...`
- new clients should not depend on the legacy family

Risk:

- local scripts and older clients may still assume these paths

Required check before removal:

- audit browser, desktop, mobile, and helper tooling

Status:

- completed on 2026-03-21
- backend legacy auth aliases removed
- first-party web client already aligned to `/api/v1/auth/...`
- current scripts/docs updated to versioned auth paths

### Wave 3: Unversioned resource aliases

Candidates:

- unversioned `/api/files/...`
- unversioned `/api/folders/...`
- unversioned `/api/shares/...`
- unversioned `/api/notifications/...`
- unversioned public share routes

Why last:

- largest blast radius
- most likely to have historical consumers

Risk:

- breaking internal tooling and forgotten compatibility tests

Required check before removal:

- route-by-route usage audit
- explicit release note

Status:

- completed on 2026-03-21
- backend unversioned file, folder, share, notification, and public-share aliases removed
- active helper scripts updated to `/api/v1/...`
- source-of-truth docs updated to the versioned contract

## Rules Before Any Removal

- do not remove a compatibility route without confirming the canonical replacement is documented
- do not remove a compatibility route in the same change that introduces unrelated product behavior
- do not remove compatibility routes while active client migrations are still in progress

## Minimum Evidence Required

Before removal of a route family, capture:

- code search results showing no active consumer in first-party clients
- test/tooling updates completed
- release note or operator note prepared

## Current Recommendation

Waves 1 and 2 are now complete.

Next:

- treat additional unversioned removals as separate follow-up work only if they still matter
- keep release notes and operator notes clear if user/profile or operator-only aliases are removed later
