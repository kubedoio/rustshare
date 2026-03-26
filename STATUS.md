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

- the light mobile client now exists as an aligned standalone foundation, but active mobile product work is postponed for now
- the Apple-first desktop app is only an early separate prototype, not a production client
- deeper admin dashboards and alerting remain partial

### Technical debt

- the frontend toolchain is now aligned on Svelte 5 without the previous runtime mismatch warning
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

1. Decide whether the current conditional web-first pilot result is sufficient for rollout scope.
2. Re-run Phase 6 in the real launch environment if OIDC, alerting, or replication targets are added there.
3. Begin Phase 7 only within the limits described by the current gate decision.

## Contract Freeze

As of 2026-03-21:

- `/api/v1/...` is the frozen client-facing API surface
- `/api/ws` is the frozen realtime endpoint
- `/api/v1/ws` and `/api/sync` have been removed
- legacy `/api/auth/...` aliases have been removed
- unversioned file, folder, share, notification, and public-share aliases have been removed
- remaining unversioned `/api/...` routes are limited to narrower compatibility or internal/operator surfaces

Use [API Contract Freeze](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-api-contract-freeze.md) as the source of truth for new client work.
Use [Compatibility Surface Inventory](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-compatibility-surface.md) to track what remains transitional and what should be removed later.

## Phase Status

- Phase 1: complete
- Phase 2: complete
- Phase 3: complete
- Phase 4: complete at the aligned mobile-foundation level
- Phase 5: complete at the repo hardening level
- Phase 6: executed for the current Docker-based pilot profile
- Launch Gate: conditionally passed for the current password-login web-first pilot profile
- Phase 7: active within the limits of the current gate decision

Current Phase 7 progress:

- Wave 1 complete: realtime compatibility aliases removed
- Wave 2 complete: legacy auth aliases removed
- Wave 3 complete: unversioned resource aliases removed
- UI/UX: Settings page refactored with tabbed interface (Dropbox-style)
  - Tabs: General, Security, Notifications, Devices, Appearance, Sharing
  - Reusable settings components (SettingsTabs, SettingsSection, SettingsRow, ToggleRow)
  - Responsive layout for mobile/desktop
  - Theme selection, device pairing, and profile management integrated

Phase 3 completion means:

- stable `/api/v1` contract documented
- stable `/api/ws` realtime endpoint documented
- client integration rules documented
- compatibility-only surface inventoried
- current web client and active websocket references aligned to canonical paths

Phase 4 completion means:

- standalone Android and iOS client trees aligned to the frozen `/api/v1` and `/api/ws` contract
- native mobile OIDC callback handling in place
- secure mobile token storage in place
- explicit offline downloads tracked locally
- queued photo/video backup implemented in the active mobile client path

Phase 5 completion means:

- OIDC production validation checklist is written
- alerting and incident threshold guidance is written
- post-restore expected outcomes are written
- compatibility removal planning is written
- frontend runtime dependency drift is reduced materially and the Svelte runtime mismatch warning is removed

Environment-specific launch sign-off is still required after this repo work:

- real IdP validation
- target-environment restore drill evidence
- operator alerting implementation in the chosen monitoring stack

## Roadmap Decision

Mobile is postponed as the next active delivery phase.

Use these docs as the source of truth:

- [Mobile Postponement Decision](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-mobile-postponement-decision.md)
- [Phase 5 Launch Hardening Spec](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-phase-5-launch-hardening-spec.md)
- [Phase 6 Environment Sign-Off Spec](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-phase-6-environment-signoff-spec.md)
- [Launch Gate: Web-First Pilot](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-launch-gate-web-first-pilot.md)
- [Phase 7 Post-Launch And Client Roadmap](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-phase-7-post-launch-and-client-roadmap.md)
- [Phase 6 Execution Report](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-phase-6-execution-report.md)
- [Web-First Pilot Gate Decision](/Users/scolak/Projects/x/rustshare/docs/2026-03-21-web-first-pilot-gate-decision.md)

## Phase 7 Progress

- compatibility cleanup wave 1 is complete
- realtime alias routes `/api/v1/ws` and `/api/sync` are removed
- the next cleanup target is legacy auth aliases

## Validation Snapshot

The main validation loop has already been exercised successfully in this workspace:

- `cargo check --workspace`
- `cargo test --workspace`
- `npm run check`
- `npm test`
- `docker compose build --no-cache backend`

The remaining work is no longer “make the project basically work.” It is launch hardening, technical debt cleanup, and the next client phases.
