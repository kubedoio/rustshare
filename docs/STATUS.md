# RustShare Status

## Current State

RustShare is a late-MVP / pre-release file-sharing platform with a strong web product core and a deliberately narrower scope than Nextcloud. The project is no longer a prototype shell: the main web app, sharing flows, realtime updates, async replication, and operator recovery tooling are all implemented and working.

### Major Architecture Update: Zero-PostgreSQL (2026-03-27)

RustShare now supports a **zero-PostgreSQL architecture**. The system can run entirely without PostgreSQL, using RustFS as the durable system of record for all metadata. This enables:

- **Simpler deployments**: Just rustshare + rustfs (optionally + redis)
- **Two runtime profiles**: Standalone (single-node) and Distributed (multi-node with Redis)
- **Flexible scaling**: Start with standalone, migrate to distributed as needed

See [Zero-PostgreSQL Architecture](docs/ZERO_POSTGRES_ARCHITECTURE.md) for details.

### Implemented Features

As of 2026-05-22, the implemented platform includes:

- **Zero-PostgreSQL architecture** with RustFS as canonical store
- **Dual runtime profiles**: Standalone and Distributed with Redis coordination
- **Multi-Tenant Schema Hardening**: Composite tenant-scoped unique constraints on `modules` and `templates` tables to prevent key collisions across tenants.
- **Decoupled NoteService**: Path generalization utilizing custom workspace and folder names via builder, fully separating notes storage.
- **Frontend CSS Alignment**: Migrated hardcoded hex/purple colors to design-system theme tokens in Svelte components.
- secure web sessions with HTTP-only cookies (JWT-based, stateless)
- OIDC groundwork for SSO, including mobile-oriented PKCE endpoints
- Axum-served SvelteKit SPA runtime
- file and folder CRUD with version history and trash
- internal user-to-user sharing
- public file links
- public folder links
- upload-only public folder links
- persistent notifications and unread counts (RustFS-backed)
- websocket-driven realtime updates
- RustFS-compatible primary storage (metadata + content)
- asynchronous replication worker with RustFS-tracked job states
- rate limiting for auth and public-share hot paths (Redis or memory)
- backup, restore, verification, and post-restore smoke tooling
- CoordinationStore abstraction for distributed locks and job claims
- first-class notes with markdown editor, autosave, and public sharing

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

- Web file-sharing product: roughly `98%`
- Broader product including mobile and desktop clients: roughly `75-80%`

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

Use [API Contract Freeze](docs/2026-03-21-api-contract-freeze.md) as the source of truth for new client work.
Use [Compatibility Surface Inventory](docs/2026-03-21-compatibility-surface.md) to track what remains transitional and what should be removed later.

## Phase Status

- Phase 1: complete
- Phase 2: complete
- Phase 3: complete
- Phase 4: complete at the aligned mobile-foundation level
- Phase 5: complete at the repo hardening level
- Phase 6: executed for the current Docker-based pilot profile
- Launch Gate: conditionally passed for the current password-login web-first pilot profile
- Phase 7: active within the limits of the current gate decision
- **Metadata Refactor**: implemented (2026-03-27), pending migration
- **Notes MVP-1**: implemented (2026-04-03)

## Metadata V2 Implementation

The metadata v2 refactor is **complete** and ready for phased migration:

### Architecture

- **New storage model**: Metadata stored in RustFS/S3 as JSON documents
- **Consistency model**: Synchronous writes with event append for audit
- **Object layout**: Hierarchical structure with indexing for folder children
- **Runtime cache**: In-memory caching with automatic invalidation

### Migration Stages

1. `postgres` - PostgreSQL only (current, default)
2. `dual_write` - Write to both, read from PostgreSQL
3. `rustfs_reads` - Write to both, read from RustFS
4. `rustfs` - RustFS only (target)

### Configuration

```bash
# Select backend
RUSTSHARE_METADATA_BACKEND=postgres  # Options: postgres, rustfs, dual_write, rustfs_reads, localfs

# Optional tuning
RUSTSHARE_METADATA_CACHE=true
RUSTSHARE_METADATA_PREFIX=apps/rustshare
RUSTSHARE_METADATA_NAMESPACE=default
```

### Admin Endpoints

- `GET /admin/metadata/health` - Health check
- `GET /admin/metadata/stats` - Storage statistics
- `GET /admin/metadata/verify/parity` - PostgreSQL vs RustFS parity
- `GET /admin/metadata/verify/consistency` - Internal consistency check
- `POST /admin/metadata/repair` - Repair inconsistencies
- `POST /admin/metadata/rebuild/index` - Rebuild indexes from objects

### Verification Tools

Use the admin metadata endpoints (`/admin/metadata/verify/*` and `/admin/metadata/repair`) for parity, consistency, and repair operations. The legacy standalone verification/repair scripts have been removed.

### Documentation

- [Metadata Refactor Design](docs/2026-03-27-metadata-refactor-design.md)
- [Metadata Refactor Architecture Decision](docs/2026-03-27-metadata-refactor-adr.md)

Current Phase 7 progress:

- Wave 1 complete: realtime compatibility aliases removed
- Wave 2 complete: legacy auth aliases removed
- Wave 3 complete: unversioned resource aliases removed
- Wave 4 complete: legacy auth aliases removed
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

- [Mobile Postponement Decision](docs/2026-03-21-mobile-postponement-decision.md)
- [Phase 5 Launch Hardening Spec](docs/2026-03-21-phase-5-launch-hardening-spec.md)
- [Phase 6 Environment Sign-Off Spec](docs/2026-03-21-phase-6-environment-signoff-spec.md)
- [Launch Gate: Web-First Pilot](docs/2026-03-21-launch-gate-web-first-pilot.md)
- [Phase 7 Post-Launch And Client Roadmap](docs/2026-03-21-phase-7-post-launch-and-client-roadmap.md)
- [Phase 6 Execution Report](docs/2026-03-21-phase-6-execution-report.md)
- [Web-First Pilot Gate Decision](docs/2026-03-21-web-first-pilot-gate-decision.md)

## Phase 7 Progress

- compatibility cleanup waves 1-4 are complete
- realtime alias routes `/api/v1/ws` and `/api/sync` are removed
- legacy auth aliases are removed
- unversioned resource aliases are removed
- notes MVP-1 shipped with dedicated editor and public sharing

## Validation Snapshot

The main validation loop has already been exercised successfully in this workspace:

- `cargo check --workspace`
- `cargo test --workspace`
- `npm run check`
- `npm test`
- `docker compose build --no-cache backend`

The remaining work is no longer “make the project basically work.” It is launch hardening, technical debt cleanup, and the next client phases.

## Dependency Management

Automated dependency management is now configured:

- **Dependabot**: Weekly PRs for Cargo and npm updates
- **CI Checks**: Weekly automated dependency audits
- **Local Tools**: `cargo-outdated`, `cargo-audit` for manual checks
See [docs/DEPENDENCY_MANAGEMENT.md](docs/DEPENDENCY_MANAGEMENT.md) for details.
