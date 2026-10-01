# ADR-0037: Calendar Application and External Calendar Sync

Status: Proposed  
Date: 2026-10-01

## Context

GitHub issue #315 asks for a Calendar application "integrated to other
calendars like Outlook, Google", with an enhancement analysis of the available
options before an ADR/spec/contracts are produced. Today the repository has no
calendar surface at all: no calendar tables exist, and the table name `events`
is already taken by the append-only domain event store.

Verified repository facts that shape the design:

- **Applications are manifest-based.** `ApplicationManifest`
  (`apiVersion: elembra.io/v1alpha1`, `kind: Application`) is defined in
  `backend/crates/core/src/domain/application.rs`; the only first-party
  identity catalogue is `first_party_manifests()` at
  `application.rs:488`. Tenant/workspace enablement lives in
  `application_enablements` and per-app API surfaces are gated on it (see
  `require_mail_enabled()` in `backend/server/src/handlers/mail.rs:25`).
- **The Mail application is the proven precedent** for an Embedded
  Application that ingests external per-user data: ~40 gated routes mounted in
  `mail_routes()` (`backend/server/src/routes.rs:265`), a dedicated service
  (`backend/server/src/services/mail_service.rs`), domain types under
  `backend/crates/core/src/domain/mail_*.rs`, and a background worker
  (`backend/server/src/mail_import_worker.rs`) driven by a DB-backed job queue
  (`mail_import_jobs`) with claim/stale-reset/watermark semantics.
- **Per-user encrypted credentials already have an approved pattern**:
  `mail_accounts.password_enc`, encrypted with AES-256-GCM via
  `SecretEncryptionKey::from_env`
  (`backend/crates/crypto/src/secret_encryption.rs`, key from
  `RUSTSHARE_SECRET_ENCRYPTION_KEY`), exposed on `AppState.secret_key`
  (`backend/server/src/state.rs:200`).
- **Durable cross-application events exist** as a transactional PostgreSQL
  outbox (`0031-durable-integration-events.md`; migration
  `20260810000001_create_integration_outbox.up.sql`) with a
  CloudEvents-compatible envelope (`docs/specs/integration-event-v1alpha1.md`).
  The FileService-facing v1alpha1 publisher adapter still hardcodes the
  `io.elembra.files` source; the Chat Application already publishes
  `io.elembra.chat.buzz.event.observed.v1` through the generic
  `OutboxStore::insert_in_tx` path (`backend/server/src/buzz_observation.rs`),
  so `0031-durable-integration-events.md`'s deferred second publisher is
  already fulfilled.
- **The Connector contract (`docs/specs/connector-contract-v1alpha1.md`,
  issue #213) is spec-only**: there is no Connector runtime and no OAuth client
  infrastructure (only login OIDC and device tokens). The contract warns: "Do
  not implement bidirectional sync merely because a provider supports
  read/write APIs."
- The issue asks for "the same share functionalities" as other apps. Elembra
  sharing (public links, internal shares) is a Files-owned, permission-governed
  surface (`0031-tenant-isolation-share-links-and-rls.md`); per-user external
  calendar data has no workspace authorization model yet.

### Options analysis (required by issue #315)

1. **First-party Embedded Application modeled on Mail** — Elembra-owned
   `calendar_events` store, iCal/.ics import, and read-only OAuth sync from
   Google/Microsoft, all behind the standard manifest/enablement model.
2. **Bundle an external CalDAV server** (e.g. Radicale/Baikal-style) as a
   Bridge application and proxy it.
3. **Implement a CalDAV server inside Elembra** so native clients (Apple
   Calendar, Thunderbird) sync against Elembra directly.
4. **Frontend-only calendar** rendered from dated notes/files, with no
   dedicated backend.

Option 1 is the only one that reuses the proven credential-encryption,
job-queue, enablement, and outbox machinery, keeps the security boundary
reviewable, and answers the Outlook/Google integration ask directly. Options 2
and 3 import or rebuild an entire protocol surface (CalDAV scheduling, ACLs,
principal discovery) that the current permission model cannot govern; option 4
does not satisfy the issue at all. See "Rejected alternatives".

## Decision

**Elembra gains a first-party Embedded Application `io.elembra.calendar`,
modeled on the Mail application, with three v1 capabilities:**

1. **Internal calendar CRUD** — Elembra-native events stored in a new
   `calendar_events` table, owned per user, created/edited/deleted through
   `/api/v1/calendar/...` routes gated on application enablement
   (`require_calendar_enabled`, same shape as `require_mail_enabled`).
2. **iCal/.ics import** — a one-shot, re-importable per-user import job
   (`calendar_import_jobs`, modeled on `mail_import_jobs`) that parses an
   uploaded RFC 5545 file with a maintained Rust parser crate and upserts
   events idempotently by `(source, UID, RECURRENCE-ID)`.
3. **Read-only external sync** — per-user OAuth2 connections to Google
   Calendar and Microsoft/Outlook (`calendar_sources` with
   `kind = internal | ical_import | google | outlook`). A sync worker modeled
   on `mail_import_worker` polls with provider cursors (Google `syncToken`,
   Microsoft `deltaToken`, stored in `calendar_sync_states`) and materializes
   remote events into `calendar_events` as read-only copies.

Additional rulings:

- **No bidirectional external sync in v1.** External sources are
  authoritative; Elembra stores a mirror (the connector contract's `mirror`
  mode). Writes never propagate to Google/Microsoft. This follows the
  `connector-contract-v1alpha1.md` warning against casual bidirectional sync.
- **Credentials follow the `mail_accounts` pattern.** Per-user OAuth refresh
  tokens are stored AES-256-GCM-encrypted in `calendar_sources` via
  `SecretEncryptionKey` (`RUSTSHARE_SECRET_ENCRYPTION_KEY`). OAuth client
  id/secret are deployment configuration (`RUSTSHARE_CALENDAR_GOOGLE_CLIENT_*`
  / `RUSTSHARE_CALENDAR_MICROSOFT_CLIENT_*`); redirect URIs derive from the
  existing `RUSTSHARE_PUBLIC_URL` (`backend/server/src/config.rs:22`).
  The OAuth authorization-code flow is backend-driven; the frontend settings
  panel only renders the consent URL and the resulting connection status.
- **Calendar is an Embedded Application, not a Connector runtime.** It is the
  first consumer of the connector *contract's* state model (encrypted
  credential refs, cursors, last_error/last_synced_at health fields) while
  remaining a normal manifest-registered application. No Connector runtime is
  built as part of this work.
- **Sync execution reuses the mail worker shape**: DB-backed queue, lease/
  claim, stale-job reset, bounded concurrency, env-configured via
  `RUSTSHARE_CALENDAR_*_WORKER_*` (mirroring
  `RUSTSHARE_MAIL_IMPORT_WORKER_*` in `config.rs:60-78`), spawned from
  `backend/server/src/bootstrap.rs`.
- **Integration events**: Calendar publishes namespaced outbox events
  (`io.elembra.calendar.event.imported.v1`, `.created.v1`, `.updated.v1`,
  `.deleted.v1`) atomically with state mutations, per
  `0031-durable-integration-events.md`. Calendar becomes a third outbox
  publisher (after Files and Chat) through the
  generic `OutboxStore::insert_in_tx` path, which already validates
  event-type ownership against the manifest registry
  (`backend/crates/storage/src/outbox_store.rs:359-370`) — no store changes
  are needed beyond the calendar manifest declaring its event types. The
  hardcoded `io.elembra.files` source exists only in the FileService-facing
  `publish_in_tx` adapter (`outbox_store.rs:1521`), which Calendar does not
  use.
- **Sharing**: v1 events are visible only to their owner. A read-only
  per-user `.ics` export/subscribe feed is allowed as a stretch goal.
  Workspace-shared calendars and public share links are deferred until a
  permission design exists (see "Out of scope"); we do not map Files share
  semantics onto per-user external data by default.
- **Frontend**: a hand-rolled month/week/agenda grid
  (`CalendarApplicationView.svelte`) registered in the existing renderer map
  (`frontend/src/routes/(app)/apps/[key]/ApplicationPageRenderer.svelte`),
  with `frontend/src/lib/api/calendar.ts` and a settings panel following
  `MailSettingsPanel.svelte`. No heavy third-party calendar component library.

## Consequences

### Positive

- The feature lands entirely on proven patterns: manifest + enablement gating,
  encrypted per-user credentials, DB job queues with watermarks, transactional
  outbox. Each piece has an in-repo precedent with tests.
- Read-only mirror semantics avoid the conflict/deletion/permission morass
  that `connector-contract-v1alpha1.md` warns about; deletion propagation from
  provider → Elembra is simple (the mirror follows the authoritative source),
  and Elembra-side edits to mirrored events are impossible by construction.
- Per-user credential storage reuses the reviewed AES-256-GCM path; no new
  secrets mechanism is introduced.
- Calendar becomes a third integration-event publisher (after Files and
  Chat), reusing the generic `insert_in_tx` path instead of accumulating
  another special case; the generalization deferred in
  `0031-durable-integration-events.md` (the `io.elembra.files`-hardcoded
  `publish_in_tx` adapter) remains open.
- The connector contract gains its first real consumer as an Embedded
  Application, informing the future Connector runtime (issue #213) without
  committing to one.

### Negative

- Three new subsystems (import worker, two OAuth sync paths) add background
  load and operational state: cursors can be invalidated (Google returns
  `410 GONE`), tokens are revoked, and providers rate-limit. The sync worker
  must handle full-resync fallback and `auth_required` health states.
- OAuth app registrations (Google Cloud Console, Microsoft Entra) are
  per-deployment operator work; self-hosters must configure client credentials
  before the sync features function.
- The owner-only visibility model means the "share like other apps" part of
  issue #315 is only partially answered in v1 (read-only export at best).
- `calendar_events` duplicates data that lives in Google/Microsoft; storage
  grows with every connected source, and reconciliation must stay cheap.

## Rejected alternatives

### Bundle an external CalDAV server (Radicale/Baikal-style) as a Bridge app

Rejected. A Bridge runtime would sit outside Elembra authorization (compare
the Chat/Buzz boundary work in `0034-elembra-chat-buzz-boundary.md`), import a
second identity/ACL model we cannot govern, and still not provide
Elembra-native CRUD or outbox integration. Operational complexity (another
stateful service per deployment) is not justified by the v1 feature set.

### Implement a CalDAV server inside Elembra

Rejected for v1. CalDAV (RFC 4791) plus scheduling (iTIP/iMIP) is a large
protocol surface — calendar-query REPORTs, principal discovery, ACLs — whose
authorization semantics have no Elembra permission mapping yet. It also solves
a different problem (native thick clients) than the issue asks (Google/Outlook
integration). A CalDAV *endpoint* may be revisited once workspace calendar
permissions exist; it is listed under Out of scope.

### Frontend-only calendar over dated notes

Rejected. It satisfies neither the import nor the external-sync requirement
and would strand calendar data in the Notes/Files namespace without recurrence,
time-zone, or attendee semantics.

### Bidirectional sync in v1

Rejected explicitly by the connector contract. Two-way sync requires conflict,
rename, deletion-propagation, and permission semantics to be designed first;
Google/Microsoft also have materially different change models (syncToken vs
delta query vs webhooks). Declaring `bidirectionalSync: true` before those
semantics exist is exactly the failure mode the contract calls out.

### Build the generic Connector runtime first (issue #213)

Rejected. The connector contract is intentionally spec-only; building a
runtime before its first consumer exists would design against imaginary
requirements. Calendar consumes the contract's *state model* as an Embedded
Application and feeds concrete requirements back into #213.

## Security

This ADR touches three safety-boundary areas listed in AGENTS.md (per-user
credentials, external connectors, indexing visibility) and therefore requires
tests plus human review before merge:

- OAuth refresh tokens are encrypted at rest with the existing
  `SecretEncryptionKey`; tokens never appear in logs, API responses, events,
  or error messages. Provider error payloads are summarized before storage in
  `last_error`.
- The OAuth callback validates `state` (single-use, short-lived, bound to the
  initiating user) to prevent login-CSRF and account-swap attacks.
- Mirrored events carry the provider's own sharing semantics; Elembra exposes
  them only to the owning user. External object visibility is not Elembra
  workspace visibility (`connector-contract-v1alpha1.md`).
- Outbound fetches to Google/Microsoft follow existing SSRF posture: fixed
  provider base URLs, no user-controlled fetch URLs (`.ics` import is an
  upload, not a URL fetch).
- All `/api/v1/calendar/...` JSON API routes are gated on tenant application
  enablement; unauthenticated and cross-tenant access fail closed. The OAuth
  callback (authenticated by its single-use `state`) and the stretch ICS feed
  (token-bound) are intentionally outside this session/enablement gate.

## Acceptance criteria

- [ ] `io.elembra.calendar` manifest registered in `first_party_manifests()`
      with navigation `/apps/calendar`, renderer `calendar`, icon
      `calendar-days`, settings `/settings/apps/calendar`; registry validation
      tests pass.
- [ ] `calendar_events`, `calendar_sources`, `calendar_sync_states`,
      `calendar_import_jobs`, and `calendar_oauth_states` migrations apply
      cleanly; SQLx metadata prepared
      (`cargo sqlx prepare --workspace --check` green).
- [ ] `/api/v1/calendar/...` JSON API routes enforce enablement (403 when
      disabled) and owner scoping (cross-user access returns 404).
- [ ] .ics import is idempotent: re-importing the same file creates no
      duplicates; malformed files fail the job with a safe `last_error`.
- [ ] Google and Microsoft sync honor cursors (`syncToken`/`deltaToken`),
      reset on `410 GONE`, and surface `auth_required` when tokens are revoked.
- [ ] Calendar outbox events publish atomically with mutations and pass the
      envelope validator (`io.elembra.calendar.*.v1`).
- [ ] Frontend month/week/agenda views render internal + imported + synced
      events with source attribution; settings panel connects/disconnects
      sources without exposing tokens.
- [ ] Security note in the PR; human review obtained per AGENTS.md safety
      boundaries.

## References

- Issue #315 — feature request.
- `0030-elembra-application-model.md` — Application model and manifests.
- `0031-durable-integration-events.md` — outbox transport and envelope
  ownership (its deferred "second publisher" is fulfilled by Chat).
- `0031-tenant-isolation-share-links-and-rls.md` — tenant isolation rules for
  the existing public-share-link surface.
- `0034-elembra-chat-buzz-boundary.md` — precedent for not inferring external
  system semantics.
- `docs/specs/connector-contract-v1alpha1.md` — mirror-mode and bidirectional
  sync rules.
- `docs/specs/calendar-application-v1alpha1.md` — normative specification.
- `docs/contracts/calendar-application-api.md` — REST contract.
- `docs/plans/2026-10-01-issue-315-calendar-application.md` — implementation
  plan.
