# Issue #315 — Calendar Application with iCal Import and Google/Outlook Sync: Design Proposal & Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Each `## Task N:` block is a self-contained executor prompt: it lists the files, the exact verification commands with expected output, and the commit message. Do not start Task 4/5 (OAuth) before Task 1–3 are merged; Tasks 4 and 5 are independent of each other.

**Goal:** Implement GitHub issue #315 — a Calendar application integrated with external calendars (Google, Outlook) — as a first-party Embedded Application `io.elembra.calendar` with three v1 capabilities: internal event CRUD, one-shot iCal/.ics import, and per-user read-only sync from Google Calendar and Microsoft/Outlook via OAuth2.

**Architecture:** Everything is modeled on the Mail application. Manifest registration in `first_party_manifests()` (`backend/crates/core/src/domain/application.rs:488`); per-tenant enablement gating via a `require_calendar_enabled()` guard cloned from `require_mail_enabled()` (`backend/server/src/handlers/mail.rs:25-43`); five new tables (`calendar_events`, `calendar_sources`, `calendar_sync_states`, `calendar_import_jobs`, `calendar_oauth_states`); per-user OAuth tokens encrypted with the existing AES-256-GCM `SecretEncryptionKey` (`mail_accounts.password_enc` pattern); background import/sync workers cloned from `mail_import_worker.rs` (DB queue, claim/stale-reset/watermark); integration events through the generic `OutboxStore::insert_in_tx` path (`backend/crates/storage/src/outbox_store.rs:359`). No bidirectional sync, per the connector contract warning.

**Tech Stack:** Rust 1.97.1 / Axum / SQLx (offline metadata) / PostgreSQL 16; `icalendar` crate for RFC 5545 parsing (new dependency); `rrule` crate (~0.13) for recurrence expansion at read time (new dependency); `reqwest` (already a workspace dependency) for OAuth + provider APIs; Svelte 5 runes + TanStack Query (`$lib/query-compat`) frontend with a hand-rolled month/week/agenda grid (no calendar component library).

**Companion documents (read first):**

- `docs/adr/0037-calendar-application-and-external-sync.md` — decision record
- `docs/specs/calendar-application-v1alpha1.md` — normative spec (schema, sync semantics, security rules)
- `docs/contracts/calendar-application-api.md` — REST contract (request/response shapes, status codes)

---

## Background: comprehensive analysis

### What issue #315 asks

> "there is no calendar application, implement a calender application that will have the same share functionalities … run an enhancement analyse on all the available options for Calender solutions. find at least 2-3 options, than create ADR, SPEC and contracts"

Three deliverables are explicit (options analysis, ADR, SPEC + contracts); they are `docs/adr/0037-…`, `docs/specs/calendar-application-v1alpha1.md`, `docs/contracts/calendar-application-api.md`. The options analysis (4 options) is in the ADR's Context/Rejected-alternatives sections. The "share functionalities" ask is scoped honestly: v1 events are owner-only with a read-only ICS export feed as stretch, because Elembra's share-link surface (`0031-tenant-isolation-share-links-and-rls.md`) is Files-owned and no permission model exists yet for per-user mirrored external data. Workspace-shared calendars are listed under Out of scope.

### Existing building blocks (verified in-repo)

| Building block | Evidence |
|---|---|
| Manifest-based applications | `ApplicationManifest` (`apiVersion: elembra.io/v1alpha1`, `kind: Application`), `backend/crates/core/src/domain/application.rs:150`; only first-party catalogue is `first_party_manifests()` at `application.rs:488` |
| Icon allowlist already contains `calendar-days` | `backend/server/src/services/icon_registry.rs:6` (currently used by the `meetings` app) |
| Enablement gating | `require_mail_enabled()` in `backend/server/src/handlers/mail.rs:25-43` → 403 "Mail module is disabled" via `application_service.get_application(...)` |
| Route mounting | `mail_routes()` at `backend/server/src/routes.rs:265`, merged at `backend/server/src/main.rs:122` |
| Per-user encrypted credentials | `mail_accounts.password_enc` (`backend/migrations/20260708160002_create_mail_accounts_table.sql`); AES-256-GCM `SecretEncryptionKey::from_env` from `RUSTSHARE_SECRET_ENCRYPTION_KEY` (`backend/crates/crypto/src/secret_encryption.rs`); `AppState.secret_key` at `backend/server/src/state.rs:200` |
| DB-backed job queue with watermark | `mail_import_jobs` (`backend/migrations/20260708160003_create_mail_import_jobs_table.sql`); claim/stale-reset in `MetadataStore` (`backend/crates/storage/src/metadata.rs:1468` `claim_next_pending_mail_import_job`, `:1816` `reset_stale_running_mail_import_jobs`) |
| Background worker shape | `backend/server/src/mail_import_worker.rs` — `MailImportWorkerConfig::from_config`, `JoinSet` with bounded concurrency, stale reset skipping in-flight jobs; spawned from `backend/server/src/bootstrap.rs:768` |
| Worker env config pattern | `RUSTSHARE_MAIL_IMPORT_WORKER_{ENABLED,POLL_SECS,MAX_CONCURRENT,STALE_SECS}` at `backend/server/src/config.rs:60-78` |
| Integration events | Transactional outbox (`backend/migrations/20260810000001_create_integration_outbox.up.sql`); envelope spec `docs/specs/integration-event-v1alpha1.md`; generic publish path `OutboxStore::insert_in_tx` validates manifest event ownership (`outbox_store.rs:359-370`). Only the FileService-facing `publish_in_tx` adapter hardcodes `io.elembra.files` (`outbox_store.rs:1521`) — Calendar does not use it |
| Connector contract (issue #213) | SPEC-ONLY, no runtime; `docs/specs/connector-contract-v1alpha1.md` warns "Do not implement bidirectional sync merely because a provider supports read/write APIs". Calendar is its first consumer as an Embedded app with connector-style state (encrypted credential refs, cursors, health fields) |
| Migration conventions | `backend/migrations/YYYYMMDDHHMMSS_snake_case.sql`; `id UUID DEFAULT gen_random_uuid()`; `tenant_id UUID NOT NULL DEFAULT '00000000-…'`; `owner_id REFERENCES users(id) ON DELETE CASCADE`; CHECK enums; soft delete `deleted_at`; partial indexes. WARNING: table name `events` is taken (append-only domain event store) — hence `calendar_events` |
| Frontend renderer registration | `rendererMap` in `frontend/src/routes/(app)/apps/[key]/ApplicationPageRenderer.svelte:16-30` (the `mail` alias at line 28 is the precedent for manifest-renderer aliasing) |
| Frontend API module pattern | `frontend/src/lib/api/mail.ts` + colocated `mail.test.ts` |
| Settings panel pattern | `frontend/src/lib/settings/MailSettingsPanel.svelte`; per-app settings routed by `frontend/src/routes/(app)/settings/apps/[slug]/+page.svelte` (`application.id === 'io.elembra.mail'` branch) |
| OAuth-adjacent config | `RUSTSHARE_PUBLIC_URL` already exists (`backend/server/src/config.rs:22`) — redirect URIs derive from it. No OAuth client infrastructure exists today (only login OIDC + device tokens) |
| HTTP client | `reqwest` already in `backend/server/Cargo.toml:54` and `backend/crates/core/Cargo.toml:27` |

No calendar tables, routes, frontend components, or crates exist today (`rg -i calendar` matches only `calendar-days` icon references — the `meetings` app, the frontend icon registry, activity/icon tests — and these calendar design docs).

### Options analysis (required by the issue; full text in the ADR)

1. **First-party Embedded Application modeled on Mail** — chosen.
2. **Bundle an external CalDAV server** (Radicale/Baikal) as a Bridge app — rejected: ungoverned second ACL model, operational complexity, no Elembra-native CRUD.
3. **Implement a CalDAV server inside Elembra** — rejected for v1: huge protocol surface with no permission mapping; solves a different problem than Google/Outlook integration.
4. **Frontend-only calendar over dated notes** — rejected: satisfies neither import nor sync.

### Design decisions

| Decision | Choice | Rejected alternative | Why |
|---|---|---|---|
| Application model | First-party Embedded Application `io.elembra.calendar`, route `/apps/calendar`, renderer `calendar`, icon `calendar-days`, nav order 40 | Connector runtime (issue #213); bundled CalDAV server | Reuses proven manifest/enablement/credential/queue machinery; connector contract is spec-only and Calendar feeds requirements back into #213 instead of blocking on it |
| External sync direction | Read-only mirror (provider authoritative, Elembra caches, `read_only = true`) | Bidirectional sync | Explicit connector-contract warning; conflict/deletion/permission semantics undesigned; Google/Microsoft change models differ (syncToken vs delta) |
| Credential storage | `calendar_sources.refresh_token_enc` / `access_token_enc`, AES-256-GCM via `SecretEncryptionKey` (`mail_accounts.password_enc` pattern) | New secrets backend; plaintext; OS keyring | The encrypted-column path is already reviewed and deployed; server-side workers need server-side secrets |
| iCal parsing | New dependency `icalendar` crate (maintained RFC 5545 parse+build) | `ical` (stale, weaker timezone support); hand-rolled parser (folding/TZID/RRULE edge cases); `calcard` (Stalwart — powerful but pulls a JSCalendar stack far beyond v1 needs) | Maintained, minimal, parse+serialize covers both import and the stretch ICS export |
| Change detection | Provider cursors: Google `syncToken`, Microsoft `deltaToken`, stored in `calendar_sync_states`; bounded full resync on `410 GONE` | Webhooks/push channels; full scan every run | Connector contract's preferred order (delta API → cursor polling); webhooks need a public endpoint + verification channel — deferred |
| Events table name | `calendar_events` | `events` | Taken by the append-only domain event store |
| Recurrence | Store RRULE verbatim; expand server-side within the requested range, capped window (≤ 366 days) | Pre-materialize instances | Unbounded storage growth; expansion-at-read is exact and cheap for UI windows |
| Frontend calendar UI | Hand-rolled month/week/agenda grid in `CalendarApplicationView.svelte` | FullCalendar or similar component library | No heavy dependency; Svelte 5 runes + existing TanStack Query idiom suffice for v1 views; keeps bundle small |
| Visibility / sharing | Owner-only rows (`tenant_id` + `owner_id` on every query, 404 for foreign); read-only per-user ICS feed as stretch | Map calendar onto Files share links | No permission model exists for per-user mirrored external data; safety boundary requires design + review first (`0031-tenant-isolation-share-links-and-rls.md` tenant isolation file, sharing rules) |
| Deletion propagation | Mirror follows provider: remote deletion soft-deletes the Elembra row | Retain local copy after provider deletion | Mirror-mode semantics from the connector contract; the provider is authoritative |
| Import identity | Upsert key `(source_id, external_uid, COALESCE(recurrence_id,''))` | Row-per-import with dedupe heuristic | Re-importable by construction; retries never duplicate |

## File structure

**Task 1 (backend foundation):**

- Modify: `backend/crates/core/src/domain/application.rs` — `first_party_manifests()` entry + registry tests
- Create: `backend/crates/core/src/domain/calendar.rs` — domain types; modify `domain/mod.rs`
- Create: `backend/migrations/20261001090000_create_calendar_tables.sql`
- Create: `backend/migrations/20261001090100_create_calendar_import_jobs_table.sql`
- Modify: `backend/crates/storage/src/metadata.rs` — calendar queries
- Create: `backend/server/src/services/calendar_service.rs`; modify `services/mod.rs`
- Create: `backend/server/src/handlers/calendar.rs`; modify `handlers/mod.rs`
- Modify: `backend/server/src/routes.rs` (`calendar_routes()`), `backend/server/src/main.rs:122` (merge), `backend/server/src/state.rs` (`calendar_service`)
- Create: `backend/tests/calendar_api_test.rs`
- Regenerate: `.sqlx/` metadata

**Task 2 (iCal import):**

- Modify: `backend/server/Cargo.toml` (+ workspace `Cargo.toml`) — `icalendar` dependency
- Create: `backend/server/src/services/ical_import.rs` — parser/upsert
- Create: `backend/server/src/calendar_import_worker.rs`; modify `lib.rs`, `bootstrap.rs`, `config.rs`, `.env.example`
- Modify: `backend/server/src/handlers/calendar.rs`, `routes.rs` — import endpoints

**Task 3 (frontend):**

- Create: `frontend/src/lib/api/calendar.ts` + `calendar.test.ts`
- Create: `frontend/src/lib/components/apps/CalendarApplicationView.svelte` + `.test.ts`
- Modify: `frontend/src/routes/(app)/apps/[key]/ApplicationPageRenderer.svelte` — renderer map entry
- Create: `frontend/src/lib/settings/CalendarSettingsPanel.svelte`; modify `frontend/src/routes/(app)/settings/apps/[slug]/+page.svelte`

**Task 4 (Google):**

- Modify: `backend/server/src/config.rs`, `.env.example` — Google OAuth config
- Create: `backend/server/src/services/google_calendar.rs` — OAuth client + events.list paging
- Modify: `handlers/calendar.rs`, `routes.rs`, `calendar_service.rs`, `metadata.rs` — connect/callback/disconnect/resync
- Create: `backend/server/src/calendar_sync_worker.rs`; modify `lib.rs`, `bootstrap.rs`

**Task 5 (Microsoft/Outlook):**

- Modify: `backend/server/src/config.rs`, `.env.example` — Microsoft OAuth config
- Create: `backend/server/src/services/outlook_calendar.rs` — OAuth client + calendarView/delta paging
- Modify: `handlers/calendar.rs`, `calendar_sync_worker.rs` — `outlook` kind

**Task 6 (events, polish, docs):**

- Modify: `calendar_service.rs`, `ical_import.rs`, sync workers — outbox publishes
- Modify: `CalendarSettingsPanel.svelte` — source list, connect/disconnect, resync, job status
- Modify: `backend/server/src/services/application_service.rs` — `io.elembra.calendar` dashboard summary arm
- Modify: `CHANGELOG.md`, `README.md`/`docs/` pointers if applicable

---

## Task 1: Calendar Application registration, schema, domain types, internal events CRUD API

**Files:**
- Modify: `backend/crates/core/src/domain/application.rs` (array at line 489-506; `resources` match at 531; `integration_events` match at 568)
- Create: `backend/crates/core/src/domain/calendar.rs`
- Modify: `backend/crates/core/src/domain/mod.rs` (after line 22 `pub mod mail_account;`; follow the `mod x; pub use x::*;` pattern)
- Create: `backend/migrations/20261001090000_create_calendar_tables.sql`
- Create: `backend/migrations/20261001090100_create_calendar_import_jobs_table.sql`
- Modify: `backend/crates/storage/src/metadata.rs`
- Create: `backend/server/src/services/calendar_service.rs`
- Modify: `backend/server/src/services/mod.rs` (line ~12)
- Create: `backend/server/src/handlers/calendar.rs`
- Modify: `backend/server/src/handlers/mod.rs` (line 26)
- Modify: `backend/server/src/routes.rs` (after `mail_routes()`, line 265), `backend/server/src/main.rs` (after line 122)
- Modify: `backend/server/src/state.rs` (add the field to `ServiceState` near line 118 and `AppState` near line 218; clone it in `FromRef<AppState> for ServiceState` near line 298)
- Modify: `backend/server/src/bootstrap.rs` (`Services` struct field near line 72, `CalendarService::new` near line 357, `AppState` literal near line 977)
- Modify: `backend/server/Cargo.toml` (register a `[[test]]` target: `calendar_api_test` → `../tests/calendar_api_test.rs`; the crate sets `autotests = false`, so `backend/tests/*.rs` is never auto-discovered)
- Create: `backend/tests/calendar_api_test.rs`

- [ ] **Step 1: Write the failing manifest tests**

In `backend/crates/core/src/domain/application.rs` `mod tests` (mirroring `first_party_catalogue_includes_chat_as_a_bridge_application` at line 898), add:

```rust
    #[test]
    fn first_party_catalogue_includes_calendar_with_event_contracts() {
        let registry = ApplicationRegistry::first_party().unwrap();
        let calendar = ApplicationId::new("io.elembra.calendar");
        let manifest = registry.manifest(&calendar).unwrap();
        assert_eq!(manifest.runtime.kind, ApplicationRuntimeKind::Embedded);
        assert_eq!(
            manifest.contributions.navigation[0].route.as_deref(),
            Some("/apps/calendar")
        );
        assert_eq!(
            manifest.contributions.navigation[0].icon.as_deref(),
            Some("calendar-days")
        );
        assert_eq!(
            manifest.contributions.routes[0].renderer.as_deref(),
            Some("calendar")
        );
        assert!(registry.owns_event_type(&calendar, "io.elembra.calendar.event.created.v1"));
        assert!(registry.owns_event_type(&calendar, "io.elembra.calendar.event.imported.v1"));
        assert!(!registry.owns_event_type(&calendar, "io.elembra.files.file.created.v1"));
    }
```

Run `SQLX_OFFLINE=true cargo test -p rustshare-core --lib application` — expected: the new test FAILS (no calendar manifest).

- [ ] **Step 2: Register the manifest**

In `first_party_manifests()` add `("calendar", "Calendar", "calendar-days", "calendar", 40),` to the slug table (between `mail` order 30 and `chat` order 60). Extend the `resources` match with a `slug == "calendar"` arm:

```rust
        } else if slug == "calendar" {
            vec![
                ApplicationResource {
                    resource_type: "calendar.event".into(),
                    actions: vec![
                        ActionCapability::new("calendar.read"),
                        ActionCapability::new("calendar.write"),
                        ActionCapability::new("calendar.delete"),
                    ],
                },
                ApplicationResource {
                    resource_type: "calendar.source".into(),
                    actions: vec![
                        ActionCapability::new("calendar.read"),
                        ActionCapability::new("calendar.write"),
                    ],
                },
            ]
        } else {
```

and the `integration_events` match with:

```rust
            "calendar" => IntegrationEvents {
                publishes: vec![
                    "io.elembra.calendar.event.created.v1".into(),
                    "io.elembra.calendar.event.updated.v1".into(),
                    "io.elembra.calendar.event.deleted.v1".into(),
                    "io.elembra.calendar.event.imported.v1".into(),
                ],
                subscribes: Vec::new(),
            },
```

Run the manifest tests — expected: PASS. The validator (`valid_event_type`, `application.rs:807`) accepts the four event names; verify by the test, not by inspection.

- [ ] **Step 3: Migrations**

Create `backend/migrations/20261001090000_create_calendar_tables.sql` with `calendar_sources`, `calendar_events`, `calendar_sync_states`, and `backend/migrations/20261001090100_create_calendar_import_jobs_table.sql` with `calendar_import_jobs`, exactly as drafted in `docs/specs/calendar-application-v1alpha1.md` §Data model (all columns, CHECK constraints, partial unique index on `(source_id, external_uid, COALESCE(recurrence_id,'')) WHERE deleted_at IS NULL`, range index on `(owner_id, starts_at)`, partial recurring-masters index on `(owner_id) WHERE rrule IS NOT NULL AND deleted_at IS NULL` named `calendar_events_recurring_owner_idx`, and `calendar_sync_states` columns including `next_sync_at`, `locked_at`, `locked_by`). Follow the conventions of `20260708160002_create_mail_accounts_table.sql` / `20260708160003_create_mail_import_jobs_table.sql`.

Verify against a local Postgres:

```bash
docker compose up -d postgres
cd backend && cargo sqlx migrate run
```

Expected: both migrations apply with no errors; `\d calendar_events` shows the CHECK constraints and indexes.

- [ ] **Step 4: Domain types**

Create `backend/crates/core/src/domain/calendar.rs`: `CalendarSourceKind` (`Internal | IcalImport | Google | Outlook` with `as_str()`/`parse()` like `MailSourceMode`), `CalendarSourceStatus`, `CalendarEventStatus`, `CalendarSource`, `CalendarEvent`, `CalendarSyncState`, `CalendarImportJob` structs mirroring the table columns (chrono `DateTime<Utc>`, serde `Serialize`, `utoipa::ToSchema` where handlers need it). Export from `domain/mod.rs`.

```bash
SQLX_OFFLINE=true cargo check -p rustshare-core
```

Expected: compiles clean.

- [ ] **Step 5: Storage + service + handlers + routes**

In `backend/crates/storage/src/metadata.rs` add: `ensure_internal_calendar_source`, `list_calendar_sources`, `get_calendar_source`, `create_ical_import_source`, `update_calendar_source`, `soft_delete_calendar_source`, `create_calendar_event`, `get_calendar_event`, `update_calendar_event`, `soft_delete_calendar_event`, `list_calendar_events_in_range(owner, from, to, source_ids, include_cancelled)` — every query filters `tenant_id` AND `owner_id` and `deleted_at IS NULL`. Use `sqlx::query!` macros (offline metadata regenerated below).

Create `backend/server/src/services/calendar_service.rs` (`CalendarService::new(pool, secret_key)` like `MailService`), enforcing: writes rejected with a typed `ReadOnlyMirror` error when the event's source kind is not internal; lazy internal-source creation on first event create.

Create `backend/server/src/handlers/calendar.rs` with `require_calendar_enabled()` cloned from `require_mail_enabled()` (`handlers/mail.rs:25-43`, constant `CALENDAR_APPLICATION_ID: &str = "io.elembra.calendar"`, message "Calendar module is disabled") and the event CRUD + range-list + source list/create/patch/delete handlers per `docs/contracts/calendar-application-api.md`. Recurrence expansion for `rrule` events happens in the service within the requested window (reject windows > 366 days with 400).

Add `calendar_routes()` in `routes.rs` (next to `mail_routes()`), merge in `main.rs` after line 122, add `calendar_service: Arc<CalendarService>` to `ServiceState` and `AppState` in `state.rs` (fields near lines 118 and 218; clone in `FromRef<AppState> for ServiceState` near line 298), and construct/wire it in `bootstrap.rs` (`Services` field near line 72, construction near line 357, `AppState` literal near line 977).

- [ ] **Step 6: Regenerate SQLx metadata and verify**

```bash
cd backend && DATABASE_URL=postgres://... cargo sqlx prepare --workspace -- --all-targets
cargo sqlx prepare --workspace --check
SQLX_OFFLINE=true cargo check -p rustshare-core -p rustshare-storage -p rustshare-server
SQLX_OFFLINE=true cargo test -p rustshare-core --lib
SQLX_OFFLINE=true cargo clippy -p rustshare-core -p rustshare-storage -p rustshare-server --all-targets --all-features -- -D warnings
```

Expected: prepare check exits 0; all checks/tests/clippy green.

- [ ] **Step 7: API tests**

**Test harness conventions** (apply to every DB-backed `backend/tests/calendar_*_test.rs` suite in this plan): every DB-backed test carries `#[ignore]`; tests run with env loaded from the repo root and single-threaded, exactly as `backend/tests/chat_bootstrap_test.rs:44-52` documents:

```text
//! DB-backed and `#[ignore]`d; run against the dev database (migrations
//! applied) with `--test-threads=1`:
//!
//!   set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
//!     cargo test -p rustshare-server --test calendar_api_test -- \
//!       --ignored --test-threads=1
//!
//! Every test takes the shared `SERIAL` guard and cleans up exactly the rows
//! it created under fresh tenants.
```

Create `backend/tests/calendar_api_test.rs` modeled on `backend/tests/chat_bootstrap_test.rs`: (a) 403 when the Application is disabled, (b) create/list/update/delete event round-trip, (c) user B gets 404 for user A's event id, (d) range query rejects a 400-day window with 400, (e) PATCH on a google-source event returns 409. Register the target in `backend/server/Cargo.toml` before `[dev-dependencies]` (the crate sets `autotests = false`, so `backend/tests/*.rs` is never auto-discovered; Tasks 2, 4 and 5 add the analogous entries for `calendar_import_test`, `calendar_google_sync_test`, and `calendar_outlook_sync_test`; each follows the harness conventions above):

```toml
[[test]]
name = "calendar_api_test"
path = "../tests/calendar_api_test.rs"
```

```bash
set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
  cargo test -p rustshare-server --test calendar_api_test -- --ignored --test-threads=1
```

Expected: all tests pass (see the harness conventions above).

- [ ] **Step 8: Commit**

```bash
git add backend/crates/core/src/domain backend/crates/storage/src/metadata.rs backend/server/src backend/migrations backend/tests/calendar_api_test.rs .sqlx
git commit -s -m "feat(calendar): register io.elembra.calendar application with internal events CRUD

First-party Embedded Application per ADR-0037 (issue #315): manifest
entry, calendar_events/calendar_sources/calendar_sync_states/
calendar_import_jobs migrations, domain types, owner-scoped CRUD API
under /api/v1/calendar gated on tenant enablement."
```

## Task 2: RFC 5545 parsing + iCal import endpoint + import worker

**Files:**
- Modify: workspace `Cargo.toml`, `backend/server/Cargo.toml` — add `icalendar`
- Create: `backend/server/src/services/ical_import.rs`
- Create: `backend/server/src/calendar_import_worker.rs`
- Modify: `backend/server/src/lib.rs` (alphabetical list: insert after line 9, before `pub mod config;`), `backend/server/src/bootstrap.rs` (near line 768), `backend/server/src/config.rs` (after line 78), `.env.example`, `backend/.env.example`
- Modify: `backend/server/src/handlers/calendar.rs`, `backend/server/src/routes.rs`
- Modify: `backend/crates/storage/src/metadata.rs` — job claim/reset/heartbeat
- Modify: `backend/server/Cargo.toml` (register `calendar_import_test` → `../tests/calendar_import_test.rs`), `backend/tests/calendar_api_test.rs` or create `backend/tests/calendar_import_test.rs`

- [ ] **Step 1: Add the parser dependency**

Add `icalendar = "0.17"` (current published version is 0.17.14 per `cargo search icalendar`; the default `parser` feature is enabled) and `rrule = "0.13"` (recurrence expansion at read time; per `cargo search rrule` the 0.13 line is maintained — 0.14 is the newest published — pin `0.13` alongside `icalendar`) to the workspace dependencies and `backend/server/Cargo.toml`. Decision record (keep in the commit message): chosen over `ical` (stale maintenance, weaker TZID handling), `calcard` (Stalwart; pulls a JSCalendar stack beyond v1 needs), and hand-rolled parsing (RFC 5545 line folding/TZID/RRULE edge cases).

```bash
SQLX_OFFLINE=true cargo check -p rustshare-server
```

Expected: compiles; `Cargo.lock` updated.

- [ ] **Step 2: Parser/upsert service**

Create `backend/server/src/services/ical_import.rs`: `parse_and_upsert(pool, job, bytes) -> ImportOutcome`. Rules from the spec §Import semantics: VEVENT → `(source_id, UID, RECURRENCE-ID)` upsert; `TZID` converted to UTC (embedded VTIMEZONE or `chrono-tz`; floating = UTC); all-day DATE → UTC-midnight span with `all_day`/`original_date`; RRULE stored verbatim; VALARM discarded; VTODO/VJOURNAL/VFREEBUSY counted as skipped; per-component failure increments `failed_events` with a bounded `last_error` sample; structurally unreadable file fails the job. Unit tests in the same file with fixture strings inline (no binary fixtures): minimal VEVENT, TZID event, all-day event, RRULE master + RECURRENCE-ID override, malformed component, duplicate UID re-import = 0 new rows.

```bash
SQLX_OFFLINE=true cargo test -p rustshare-server --lib ical_import
```

Expected: all parser/upsert unit tests pass.

- [ ] **Step 3: Import endpoints**

In `handlers/calendar.rs`: `POST /api/v1/calendar/import` (multipart `file` field, spool via the existing `stream_multipart_field_to_temp_file` helper used at `handlers/mail.rs:482`, 10 MB cap, create-or-reuse the target `ical_import` source, enqueue job, `202 { job_id, source_id, status }`), `GET /api/v1/calendar/import-jobs`, `GET /api/v1/calendar/import-jobs/{id}`. Wire into `calendar_routes()`.

- [ ] **Step 4: Import worker**

Create `backend/server/src/calendar_import_worker.rs` cloned from `mail_import_worker.rs`: `CalendarImportWorkerConfig::from_config`, stale reset skipping in-flight ids, `claim_next_pending_calendar_import_job` (add to `metadata.rs` next to `claim_next_pending_mail_import_job`, line 1468), bounded `JoinSet` concurrency, panic containment, `updated_at` heartbeat per processed event batch. Config keys in `config.rs` after line 78: `RUSTSHARE_CALENDAR_IMPORT_WORKER_{ENABLED,POLL_SECS,MAX_CONCURRENT,STALE_SECS}` with the same defaults as mail. Spawn from `bootstrap.rs` next to line 768. Document the four vars in `.env.example` and `backend/.env.example`.

```bash
SQLX_OFFLINE=true cargo check -p rustshare-server -p rustshare-storage
cd backend && cargo sqlx prepare --workspace --check
```

Expected: green.

- [ ] **Step 5: Integration test**

`backend/tests/calendar_import_test.rs`: upload a small .ics via the multipart endpoint → poll the job to `completed` → `GET /calendar/events?from=…&to=…` returns the events → re-upload the identical file → `total_events` equal, zero additional rows (`SELECT count(*)` assertion), job `completed`.

```bash
set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
  cargo test -p rustshare-server --test calendar_import_test -- --ignored --test-threads=1
```

Expected: pass (harness conventions per Task 1 Step 7).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock backend .sqlx .env.example
git commit -s -m "feat(calendar): iCal .ics import with background job worker

RFC 5545 parsing via the icalendar crate (chosen over ical/calcard/
hand-rolled; see ADR-0037), multipart import endpoint, and a
claim/stale-reset import worker modeled on mail_import_worker.
Re-imports are idempotent on (source, UID, RECURRENCE-ID) (issue #315)."
```

## Task 3: Frontend calendar view, API client, renderer registration

**Files:**
- Create: `frontend/src/lib/api/calendar.ts`
- Create: `frontend/src/lib/api/calendar.test.ts`
- Create: `frontend/src/lib/components/apps/CalendarApplicationView.svelte`
- Create: `frontend/src/lib/components/apps/CalendarApplicationView.test.ts`
- Modify: `frontend/src/routes/(app)/apps/[key]/ApplicationPageRenderer.svelte` (imports lines 3-12, `rendererMap` lines 16-30)
- Create: `frontend/src/lib/settings/CalendarSettingsPanel.svelte` (placeholder level: internal-source info + link to import UI)
- Modify: `frontend/src/routes/(app)/settings/apps/[slug]/+page.svelte` (the `application.id === 'io.elembra.mail'` branch)

- [ ] **Step 1: API module with tests**

Create `frontend/src/lib/api/calendar.ts` following `frontend/src/lib/api/mail.ts` idiom: `apiClient`-based `calendarApi` with `listEvents({from,to,sourceIds,includeCancelled})`, `createEvent`, `updateEvent`, `deleteEvent`, `listSources`, `createIcsSource`, `updateSource`, `deleteSource`, `uploadIcs(file, sourceId?)`, `listImportJobs`, `getImportJob`. Types mirror the contract (`CalendarEvent`, `CalendarSource`, `CalendarImportJob`). Write `calendar.test.ts` first, mocking `apiClient` exactly like `mail.test.ts` does.

```bash
cd frontend && npx vitest run src/lib/api/calendar.test.ts
```

Expected: new tests fail before implementation (import error), pass after.

- [ ] **Step 2: CalendarApplicationView**

Hand-rolled grid, no component library (decision table row 8). Svelte 5 runes + TanStack Query via `$lib/query-compat` (see `MailApplicationView.svelte` for the idiom):

- view switcher: month / week / agenda (`$state`);
- month grid: 7×6 day cells computed from the visible month; week: 7 columns; agenda: grouped list;
- events query keyed `['calendar-events', from, to]` with `from`/`to` derived from the visible window; sources query `['calendar-sources']`;
- source filter chips (color per source kind: internal / ical_import / google / outlook) and per-source toggle using the `source_id` param;
- create/edit modal for internal events; read-only detail popover for `read_only` events showing source attribution ("from Google — user@example.com");
- empty state pointing to `/settings/apps/calendar` for import/connect.

- [ ] **Step 3: Component tests**

`CalendarApplicationView.test.ts` modeled on `MailApplicationView.test.ts` (mock `$lib/api/calendar`): renders events in the visible month; read-only events show no edit affordance; source toggle refetches with the filter; disabled-app path handled by the page wrapper (no test needed here).

```bash
cd frontend && npx vitest run src/lib/components/apps/CalendarApplicationView.test.ts
```

Expected: pass.

- [ ] **Step 4: Register renderer and settings panel**

In `ApplicationPageRenderer.svelte`: import `CalendarApplicationView` and add `calendar: CalendarApplicationView` to `rendererMap` (the manifest registers renderer `calendar`; no alias needed, unlike `mail-list`/`mail`). In `settings/apps/[slug]/+page.svelte` add an `{:else if application.id === 'io.elembra.calendar'}` branch rendering `CalendarSettingsPanel`. The panel (this task) lists sources from `calendarApi.listSources()` with status/`last_synced_at`, and the import upload control; OAuth connect buttons arrive in Tasks 4–6 and must render disabled with a tooltip until the endpoints exist.

```bash
cd frontend && npm run check && npm run lint && npm run test
```

Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -s -m "feat(calendar): calendar application view, API client, renderer registration

Hand-rolled month/week/agenda grid (no component library), per-source
filtering and attribution, internal event create/edit modal, settings
panel with source list and .ics import upload (issue #315)."
```

## Task 4: Google Calendar OAuth connect + sync worker

**Files:**
- Modify: `backend/server/src/config.rs` (after the calendar worker block added in Task 2), `.env.example`, `backend/.env.example`
- Create: `backend/server/src/services/google_calendar.rs`
- Modify: `backend/server/src/services/calendar_service.rs`
- Modify: `backend/server/src/handlers/calendar.rs` — connect/callback/disconnect/resync
- Modify: `backend/server/src/routes.rs`
- Modify: `backend/crates/storage/src/metadata.rs` — OAuth state rows, token update, cursor read/write
- Create: `backend/server/src/calendar_sync_worker.rs`
- Modify: `backend/server/src/lib.rs`, `backend/server/src/bootstrap.rs`
- Modify: `backend/server/Cargo.toml` (register `calendar_google_sync_test` → `../tests/calendar_google_sync_test.rs`)
- Create: `backend/tests/calendar_google_sync_test.rs`

- [ ] **Step 1: Config**

`RUSTSHARE_CALENDAR_GOOGLE_CLIENT_ID`, `RUSTSHARE_CALENDAR_GOOGLE_CLIENT_SECRET`, plus `RUSTSHARE_CALENDAR_SYNC_WORKER_{ENABLED,POLL_SECS,MAX_CONCURRENT,STALE_SECS}` (STALE_SECS drives the sync-lease stale reset) and `RUSTSHARE_CALENDAR_SYNC_{PAST_DAYS(90),FUTURE_DAYS(365)}`, same serde/env attribute style as lines 60-78. Absent client id/secret = provider unconfigured (503 on connect), not a startup error.

- [ ] **Step 2: OAuth flow**

`calendar_service.rs`: `begin_google_connect(user) -> authorize_url` — generates a 256-bit `state`, persists it (new `calendar_oauth_states` table added via a small migration `20261002090000_create_calendar_oauth_states.sql`: `state PK, tenant_id, owner_id, kind, expires_at`, single-use delete-on-consume) bound to the user, 10-minute expiry; `complete_google_connect(state, code)` — validates+consumes state, exchanges the code at `https://oauth2.googleapis.com/token` via `reqwest`, encrypts refresh/access tokens with `AppState.secret_key` into `calendar_sources` (kind `google`, `external_account` from the provider's userinfo/id_token email), enqueues initial sync. Callback handler redirects 302 to `/settings/apps/calendar?connected=google` / `?error=oauth_*` per the contract; it never renders token data.

Tests: state mismatch/expired/reuse rejected; token exchange against a `wiremock`-style local server (check `backend/tests/` for an existing HTTP-mock helper and reuse it; if none, gate the exchange behind a trait and fake it).

- [ ] **Step 3: Google sync**

`services/google_calendar.rs`: `sync_source(pool, secret_key, http, source) -> SyncOutcome` — only the sync lease holder may refresh the access token (see below) and a rotated refresh token is written unconditionally (newer token wins); incremental `events.list` (primary calendar only in v1 — no `calendarList` discovery) with `syncToken` from `calendar_sync_states`; page until `nextSyncToken`; upsert by `(source_id, event.id, recurrence-id)`; `status: cancelled` → `status = 'cancelled'` tombstone (row kept, queryable via `include_cancelled`); entries removed from the delta result set without a cancelled marker are soft-deleted; on HTTP 410 null the cursor and full-resync the configured window; 429/`Retry-After` → `rate_limited` + backoff (also backing off `next_sync_at`); invalid grant → `auth_required`. Update `last_synced_at`/`last_error` on `calendar_sources`.

`calendar_sync_worker.rs`: clone of the Task 2 worker claiming *due sources* — `SELECT ... FOR UPDATE SKIP LOCKED WHERE next_sync_at <= now() AND (locked_at IS NULL OR locked_at < now() - stale)` (`is_enabled AND kind IN ('google','outlook')`), acquiring the lease (`locked_by` = worker id, `locked_at` heartbeat refreshed during the run, released on completion/failure; stale threshold from `RUSTSHARE_CALENDAR_SYNC_WORKER_STALE_SECS`, same pattern as the mail stale-job reset). Every run sets the next `next_sync_at` on completion, with backoff on rate-limit. `POST /api/v1/calendar/sources/{id}/resync` nulls the cursor, forces due-now, and is rejected if the source is lease-locked; `POST .../disconnect` revokes best-effort and wipes token columns.

- [ ] **Step 4: Tests**

`backend/tests/calendar_google_sync_test.rs`: full-sync pages materialize events; delta applies updates+deletions; `status: cancelled` entries become `status = 'cancelled'` tombstones (visible with `include_cancelled`), entries absent from the delta result set are soft-deleted; 410 triggers exactly one full resync; revoked grant flips `auth_required` and further runs are no-ops; concurrent same-source runs are safe (only the lease holder refreshes tokens; a rotated refresh token is written unconditionally — newer token wins); token plaintext appears in no response/log/assertable surface.

```bash
SQLX_OFFLINE=true cargo test -p rustshare-server --lib
cd backend && cargo sqlx prepare --workspace --check
set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
  cargo test -p rustshare-server --test calendar_google_sync_test -- --ignored --test-threads=1
```

Expected: green (harness conventions per Task 1 Step 7).

- [ ] **Step 5: Commit**

```bash
git add backend .sqlx .env.example
git commit -s -m "feat(calendar): Google Calendar OAuth connect and read-only sync

Backend-driven authorization-code flow with single-use user-bound
state, AES-256-GCM token storage (mail_accounts pattern), syncToken
incremental sync with 410 full-resync fallback, and a bounded sync
worker (issue #315)."
```

## Task 5: Microsoft/Outlook OAuth connect + sync worker

**Files:**
- Modify: `backend/server/src/config.rs`, `.env.example`, `backend/.env.example`
- Create: `backend/server/src/services/outlook_calendar.rs`
- Modify: `backend/server/src/services/calendar_service.rs`, `handlers/calendar.rs`, `calendar_sync_worker.rs`
- Modify: `backend/server/Cargo.toml` (register `calendar_outlook_sync_test` → `../tests/calendar_outlook_sync_test.rs`)
- Create: `backend/tests/calendar_outlook_sync_test.rs`

- [ ] **Step 1: Config + OAuth**

`RUSTSHARE_CALENDAR_MICROSOFT_CLIENT_ID` / `RUSTSHARE_CALENDAR_MICROSOFT_CLIENT_SECRET`. Same flow as Task 4 against the Microsoft identity platform (`https://login.microsoftonline.com/common/oauth2/v2.0/authorize|token`), scope `offline_access Calendars.Read`; `kind = 'outlook'` sources.

- [ ] **Step 2: Delta sync**

`outlook_calendar.rs`: `calendarView/delta` (primary calendar only in v1) with `@odata.deltaLink` persisted as the `ms_delta_token` cursor; invalid/expired delta token → full resync of the window; map `seriesMaster`/`occurrence` to master + `recurrence_id` rows; `isCancelled` entries → `status = 'cancelled'` tombstones (row kept, queryable via `include_cancelled`); entries absent from the delta payload are soft-deleted. Register `outlook` in the sync worker dispatch.

- [ ] **Step 3: Tests + verify**

Same matrix as Task 4 Step 4 against a mocked Graph API. Then:

```bash
SQLX_OFFLINE=true cargo clippy -p rustshare-server -p rustshare-storage -p rustshare-core --all-targets --all-features -- -D warnings
set -a; . ./backend/.env; set +a; SQLX_OFFLINE=true \
  cargo test -p rustshare-server --test calendar_outlook_sync_test -- --ignored --test-threads=1
```

Expected: green (harness conventions per Task 1 Step 7).

- [ ] **Step 4: Commit**

```bash
git add backend .sqlx .env.example
git commit -s -m "feat(calendar): Microsoft/Outlook OAuth connect and delta sync

Mirrors the Google path against the Microsoft identity platform and
Graph calendarView/delta tokens (issue #315)."
```

## Task 6: Integration events, settings panel polish, docs + full validation

**Files:**
- Modify: `backend/server/src/services/calendar_service.rs` (internal CRUD publishes), `services/ical_import.rs` + sync services (imported publishes)
- Modify: `backend/tests/calendar_api_test.rs` — atomic outbox test
- Modify: `frontend/src/lib/settings/CalendarSettingsPanel.svelte` — enable OAuth buttons, connect/disconnect/resync, job polling
- Modify: `backend/server/src/services/application_service.rs` — `io.elembra.calendar` dashboard summary arm (next to the `"io.elembra.mail"` arm, line 1013)
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Outbox publishes**

In `calendar_service` create/update/delete: build an `IntegrationEvent` (envelope per `docs/specs/integration-event-v1alpha1.md`, source `elembra://io.elembra.calendar`, types declared in the manifest in Task 1) and call `OutboxStore::insert_in_tx` inside the same transaction as the mutation — the generic path already validates ownership via the registry (`outbox_store.rs:359-370`); do NOT touch the FileService `publish_in_tx` adapter. Import/sync runs publish one `io.elembra.calendar.event.imported.v1` per run with counts and the source ResourceRef — identifiers/counts only, never titles/descriptions (minimum-safe-data rule). Test: force a rollback after the mutation and assert no outbox row exists; assert `owns_event_type` rejects an undeclared type.

- [ ] **Step 2: Settings panel polish**

Wire the Task 3 placeholder buttons to `GET /sources/{kind}/connect` (navigate to `authorize_url`), show `?connected=`/`?error=oauth_*` toasts from the redirect, disconnect/resync actions with confirmation, import-job status with 3 s polling (pattern from `importJobsQuery` in `MailApplicationView.svelte`). 503 from connect → render "not configured on this deployment" disabled state.

- [ ] **Step 3: Dashboard summary arm**

Add `"io.elembra.calendar"` to the summary match in `application_service.rs` (line 1013): count of events in the next 7 days, summary key `calendar-summary`.

- [ ] **Step 4: CHANGELOG + full baselines**

`CHANGELOG.md` `[Unreleased]` → new `### Added` entry describing the Calendar application (internal events, .ics import, Google/Outlook read-only sync, issue #315). Then the full PR baselines from AGENTS.md:

```bash
cargo fmt --all --check
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --workspace --all-features --lib
cd backend && cargo sqlx prepare --workspace --check
cd frontend && npm install && npm run check && npm run lint && npm run test && npm run build
```

Expected: every command green. Then a manual smoke against the local stack (`docker compose up -d`): enable Calendar for the tenant, create an event, import an .ics, connect a test Google account (or document the sandbox-credential limitation if no test project is available).

- [ ] **Step 5: Commit**

```bash
git add backend frontend CHANGELOG.md
git commit -s -m "feat(calendar): integration events, settings panel, dashboard summary

Calendar publishes io.elembra.calendar.event.*.v1 through the
transactional outbox as its third publisher (after Files and Chat);
the settings panel gains OAuth connect/disconnect/resync and
import-job status (issue #315)."
```

## Security note (required — AGENTS.md safety boundaries)

This plan touches three listed safety boundaries; the PR must carry this note and get human review:

- **Secret handling / credentials:** per-user OAuth refresh+access tokens in `calendar_sources.*_enc` via the existing AES-256-GCM `SecretEncryptionKey`; OAuth client secrets in env only; tokens never in logs, responses, or event payloads (asserted by tests in Tasks 4–5); OAuth `state` single-use, 10-minute, user-bound.
- **Connectors / external imports:** untrusted .ics input is size-capped, temp-file spooled, defensively parsed, never rendered as HTML; provider fetches use fixed allowlisted base URLs (no SSRF surface); external deletion propagates to the mirror by design.
- **Permissions / workspace visibility:** calendar data is owner-only (`tenant_id` + `owner_id` on every query, 404 for foreign ids); mirrored provider events do not acquire Elembra workspace visibility; workspace sharing is explicitly out of scope pending a permission design.

## Out of scope (documented for follow-up)

- **Bidirectional sync** with Google/Microsoft (connector contract warning; needs conflict/deletion/permission semantics first).
- **Workspace-shared calendars, delegation, public share links** — needs a permission design for per-user mirrored data; only the owner-only read-only ICS feed is allowed as stretch.
- **CalDAV server or client.**
- **Reminders/notifications** (VALARM execution, email/push).
- **Provider webhooks/push channels** (polling cursors only).
- **Free/busy, attendee management, iTIP scheduling.**
- **Memory/Search indexing of events** (the manifest declares no memory policy — `memory: None` — until permission-aware indexing is designed for owner-only data).
