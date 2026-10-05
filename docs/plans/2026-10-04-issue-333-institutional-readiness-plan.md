# Issue #333: Institutional Readiness Implementation Plan

Status: Implementation in progress; Phase 0 decisions remain open.

ADR numbering clarification (2026-10-05): this repository contains duplicate
ADR numbers. In the Phase 0 status below, ADR-0030 means
`0030-elembra-application-model.md`, ADR-0032 means
`0032-resource-refs-and-authorization.md`, and ADR-0037 means
`0037-calendar-application-and-external-sync.md`; these, along with ADR-0038,
are Proposed. The separate
`0032-safe-content-addressed-blob-garbage-collection.md` is accepted for
implementation and is not the institutional-access ADR.

Progress (2026-10-04): source review corrected the group-scope and calendar-storage assumptions below. Proposed ADR-0038 records the Phase 0 baseline and is awaiting product/security approval; related ADR-0030, ADR-0032, and ADR-0037 are also Proposed and are design inputs, not accepted architecture decisions. No membership, authorization, or schema migration has started. A read-only group-scope inventory query counts group/member/share, resource/owner, and share-creator tenant mismatches; legacy-zero groups; expired/revoked ACL references; and malformed or missing file/folder targets. It was schema-validated against the isolated local test PostgreSQL database, which contained no groups. No pilot/production inventory has been run, and matching zero-default tenant IDs do not prove historical ownership. A safe #332-adjacent UI regression exposed and fixed stale audit type/date filter values; its focused Vitest suite passes (9/9). The PostgreSQL-backed Calendar API suite now has 24 ignored integration tests, including owner-visible persistence after a foreign-tenant delete and the event-export owner/enablement boundary; all pass against fresh schemas on the dedicated local test Postgres/RustFS services. This does not satisfy organization-scoped audit acceptance.

Phase 0 external-status recheck (2026-10-05): GitHub issue #333 remains open and has no comments recording product/security approval. PR #337 is open and `APPROVED` at head `c5a82d830e0d890b28d82fc43f374869b9fd296d`; its stated scope and changed files concern pilot account-lifecycle validation, runbook, and evidence. That approval does not approve ADR-0030, ADR-0032, ADR-0037, or ADR-0038, all of which remain `Proposed` in the current worktree. Therefore Phase 0 still gates organization/group/workspace authorization, organization-scoped audit semantics, and calendar-sharing changes. Do not interpret PR #337's approval as a waiver of those decisions.

Pilot inventory access check (2026-10-05): SSH and noninteractive sudo Docker access to the configured pilot host were verified; the running stack includes Nginx, the RustShare backend, RustFS, and PostgreSQL. No host configuration was changed. Review of `scripts/pilot-group-scope-inventory.sql` confirmed an explicit `BEGIN TRANSACTION READ ONLY`/`ROLLBACK` boundary and no active write-capable SQL. Its output is group-level aggregate metadata containing tenant/group UUIDs, has no row limit, and must remain confidential. Read-only catalog checks in the configured application database found zero candidate login roles by conventional name, and zero eligible non-superuser logins with SELECT on all six referenced tables (`users`, `user_groups`, `group_members`, `shares`, `files`, `folders`) while lacking INSERT/UPDATE/DELETE/TRUNCATE and role memberships. This grant check is conservative and does not validate credentials or connection authentication. Repository deployment/pilot docs do not document a dedicated inventory role or its provisioning. The production inventory has therefore **not** been run: first establish an approved least-privilege read-only login and verify its grants; do not use the application or superuser credential as a substitute. No inventory output has been produced or retained.

Pilot inventory RLS/security gate (2026-10-05): a read-only catalog inspection on the pilot PostgreSQL instance found RLS enabled on both `files` and `folders`; each has one `FOR ALL` owner-isolation policy using `owner_id = current_setting('app.current_user_id')::uuid`, with no unrestricted SELECT policy. The inventory joins both tables. Therefore, a normal non-owner login cannot be assumed to see complete rows: an unset setting may fail the query, and a set user identity scopes results to that owner's rows. Do not use a superuser, table owner, or `BYPASSRLS` as an unreviewed workaround. A broad SELECT grant does not bypass RLS and would not make the inventory complete; it would also expose columns the inventory does not need (including user/share credential material and file metadata). The production inventory remains unrun and no role/configuration was changed. Before running it, agree and review a least-privilege complete-inventory mechanism (for example, a narrowly scoped owner-executed database routine that exposes only the required aggregate result), test the exact access path against representative RLS-enabled fixtures, and define bounded execution, evidence handling/retention, and cleanup. The fixture regression now covers both owner-visible aggregate results and the real inventory query's fail-closed path as a non-owner, but still does not provide a successful least-privilege complete-inventory mechanism.

Fail-closed inventory guard (2026-10-05): `scripts/pilot-group-scope-inventory.sql` now runs `SET LOCAL row_security = off` inside its existing read-only transaction and invokes `psql -X` to ignore operator startup configuration. PostgreSQL documents this setting as raising an error rather than applying a policy that would filter results; it does not bypass RLS and has no effect for superusers or roles with `BYPASSRLS` ([PostgreSQL `row_security` setting](https://www.postgresql.org/docs/current/runtime-config-client.html#GUC-ROW-SECURITY)). The existing fixture-backed PostgreSQL integration test was extended to add resources owned by a second user, execute the inventory query under a transaction-scoped non-owner role subject to the migrated `files`/`folders` policies, and require SQLSTATE `42501` plus the row-security error. It passed (1/1) after all repository migrations on the CI-pinned PostgreSQL image; both the role and fixture rows rolled back. Formatting and targeted all-features Clippy passed. This proves the query fails closed for the restricted-role case; it does not prove that an owner/bypass execution exercised the guard, does not establish a production inventory result, and does not solve the least-privilege complete-access gate.

Fixture-backed inventory regression (2026-10-05): added `backend/tests/pilot_group_scope_inventory_test.rs` as an ignored PostgreSQL integration test registered on the existing `rustshare-server` test target. The existing `integration-tests` workflow's migration-plus-ignored-test command discovers it; no parallel workflow was added. Locally, all repository migrations were applied to a disposable PostgreSQL container using the same pinned pgvector image as CI. The test seeded a legacy-zero group, cross-tenant member, active file/folder shares, an expired share, and a revoked share with valid but mismatched tenant metadata, all inside a transaction. It passed (1/1), asserting the legacy flag, membership and share lifecycle counts, group/resource/file-owner/folder-owner/share-creator tenant mismatch counts, and share-tenant count. The transaction and temporary database container were removed. `cargo fmt --all --check` and targeted all-features Clippy with warnings denied also passed. This does not validate every inventory counter, production data, or the authoritative hosted workflow. Validation was performed on the uncommitted worktree based on source SHA `c5a82d830e0d890b28d82fc43f374869b9fd296d`.

Phase 7 follow-up (2026-10-04): the authoritative Pilot Release workflow already exercises the current Files/Notes journey, restart, dependency failure/recovery, backup/restore, and previous-release upgrade. Review identified remaining product/evaluation gaps: staged scale evidence and institutional calendar/group/workspace acceptance. Local workflow changes now require fresh-volume install/migration/first-journey evidence and transfer the exact tested image to the push-only publish job, retaining image ID, archive checksum, and registry manifest digest. YAML parsing, all 31 embedded Bash syntax checks, the fresh-install migration query against isolated test PostgreSQL, and an independent diff review passed/no critical issue found; these changes remain uncommitted and need an authoritative run. Workflow run [37220445574](https://github.com/kubedoio/rustshare/actions/runs/37220445574) completed successfully against exact remote PR SHA `c5a82d830e0d890b28d82fc43f374869b9fd296d` (2026-10-04 17:25:18–18:07:05 UTC). Its uploaded 140,715-byte artifact expires 2027-01-02 and proves the committed workflow’s canonical journey, account lifecycle, and restart persistence. It predates local workflow changes, contains neither `clean-install.env` nor `tested-image.env`, and this `workflow_dispatch` skipped the push-only publish job; it is isolated CI, not evidence for the uncommitted worktree changes or an FWS deployment. A delegated review of the local success-summary gate found that generic smoke success alone did not prove state-file persistence verification ran. The local gate now requires the explicit `BETA_SMOKE_PERSISTENCE_STATE_VERIFIED=passed` marker in restart, restore, and upgrade-candidate reports; the smoke script writes it only after both persisted Note and File checks pass. A second review found the File check only required a nonempty response; the smoke now records the original fixture SHA-256 and verifies the persisted download matches before setting the marker. Synthetic exact-match, wrong-bytes, malformed-checksum, and marker cases passed. The actual summary step accepted a consistent synthetic evidence bundle and rejected the same smoke evidence without its persistence marker. Workflow YAML parsing, all 31 embedded Bash syntax checks, both smoke-script syntax checks, and `git diff --check` pass; the complete workflow has not yet been run against these uncommitted changes. Local work also adds workflow-run identity to smoke and restore-drill reports; all local workflow changes still require an authoritative run.

Failure-diagnostic evidence follow-up (2026-10-05): the existing dependency drill now requires HTTP 503 plus the expected non-sensitive unhealthy component and diagnostic in `/health/ready` JSON; it also asserts the unrelated dependency remains healthy. PostgreSQL readiness is awaited before testing RustFS, and full readiness is verified after each dependency recovery. New database/storage diagnostic and dependency-recovery markers are mandatory in the existing final workflow gate. YAML parsing, all 31 embedded Bash syntax checks, and synthetic expected/mismatched JSON assertions pass. The authoritative workflow has not been rerun for these uncommitted changes.

Configuration-failure evidence follow-up (2026-10-05): the previous negative-startup probe supplied both weak secrets and an invalid database URL, then treated any nonzero exit as proof of configuration validation. It now requires RustShare's startup-validation banner and the exact safe `JWT_SECRET must be at least 32 characters.` diagnostic, with no invalid database setting to act as an alternate failure cause. A dedicated diagnostic marker, invalid-configuration result, and production-cookie check are required by the existing final evidence gate. A new `AppConfig::from_env` regression checks that exact diagnostic; the focused test and server Clippy pass. Only an authoritative runner can prove the image emits it as expected.

Migration-failure evidence follow-up (2026-10-05): the existing upgrade checks proved successful migrations but did not intentionally show that a migration error aborts startup. The workflow now creates a run-scoped database and a non-privileged login, confirms it lacks `public` schema-create privilege, then starts the exact candidate image against that database. The gate requires the database connection log, PostgreSQL's expected permission diagnostic, nonzero startup, and absence of the successful-migrations log; its evidence binds source SHA, image ID/revision/version, deployment/config identity, and workflow run. Role/database cleanup is scoped to those generated names. YAML parsing and all 32 embedded Bash syntax checks pass; the new probe has not run in GitHub Actions yet.

Migration-probe review follow-up (2026-10-05): independent workflow review found the probe did not receive the required runtime configuration, so it could fail before reaching PostgreSQL; failure-path logs were also discarded and the success gate did not require the diagnostic log. The probe now receives the same generated/CI runtime settings as the main deployment while retaining its isolated `DATABASE_URL`; an EXIT handler preserves only an explicit allowlist of safe diagnostic messages, removes the raw log, and the green gate requires the exact connection and privilege diagnostics. YAML parsing, all 32 embedded Bash syntax checks, and `git diff --check` pass. No authoritative run has exercised these changes.

Evidence-safety follow-up (2026-10-05): a delegated review found that the
workflow's `sed` redaction interpolated secret values as syntax, failure
diagnostics printed raw Compose logs, and the evidence directory retained the
full backup bundle. The workflow now uses a byte-oriented literal redactor for
configured runtime secrets plus bearer/cookie/JWT-shaped values, scans the
complete artifact tree, and uploads only after the scan succeeds. Raw
Compose/backup/migration logs stay outside the artifact and are removed; backup
data is kept outside the evidence directory and removed after restore. The
redactor regression suite passes 12/12, including metacharacters, overlapping
secrets, non-UTF8 logs, and workflow fail-closed guards. Critical Trivy findings
now block image publication. Workflow YAML parsing and all 34 embedded Bash
syntax checks pass. These workflow changes have not had an authoritative run.

File-share authorization follow-up (2026-10-05): the existing canonical beta
journey previously verified recipient-share listing but never fetched the File
as the recipient. It now requires an authenticated non-owner to receive 403
before sharing, download byte-identical content under an active View share,
and receive 403 after revocation. The smoke EXIT handler best-effort revokes a
share if a later assertion fails and records cleanup status. The workflow
requires this marker only for full journeys (canonical and the previous
upgrade revision); persistence-only smoke modes (restart, restore, and
candidate verification) exit before that journey and retain their separate
persistence marker gate. Fifteen focused redaction/evidence
tests pass, including these assertions; smoke shell syntax, YAML parsing, all
34 embedded Bash syntax checks, and `git diff --check` pass. No Docker journey
or authoritative workflow run exercised this uncommitted change.

Browser UI follow-up (2026-10-05): review confirmed the existing UI step only
fetched `/` and searched for an HTML tag. The repository already pins
Playwright and configures `E2E_BASE_URL`; the broader existing admin E2E suite
mutates users, groups, and OIDC, so the pilot job now runs one isolated,
non-mutating browser test instead. It signs in with the seeded admin through
the login form, confirms navigation to `/files`, opens the canonical
`Beta Smoke` folder, and waits for the exact uploaded File name recorded by
the smoke report. The job installs locked npm dependencies and
Chromium, writes a JSON Playwright report plus a mandatory browser-status
marker, and the existing success gate requires that marker. Playwright lists
the targeted test (1/1), Prettier passes, the targeted Notes route test passes
(4/4), all 16 evidence/redaction tests pass, YAML and all 34 embedded Bash
syntax checks pass, and `git diff --check` passes. The summary gate requires
the exact filename tested by Playwright plus SHA/build/deployment/config/run
identity and timestamps. No live browser journey or authoritative runner has
yet exercised this uncommitted change; the shared local Docker service was not
used.

Notes title-independence follow-up (2026-10-05): audit found the product path
already keeps the note metadata/bundle name separate from the Markdown H1:
the route passes saved metadata title and extracted H1 separately, `save_note`
preserves the title, and `rename_note` updates the name/frontmatter while
preserving its body. The existing real API pilot journey edits the H1 and
asserts the prior title, renames the note and asserts the H1, then the
restart/restore/candidate verification checks both values after reload. Added
a route regression test for separate title/H1 props and a static guard that
the authoritative smoke retains the edit/rename/persistence assertions.
Targeted route Vitest passes 4/4 and all 16 pilot workflow regression tests
pass. The actual service-backed journey still needs the authoritative workflow
run; no product behavior change was necessary.

Browser Notes regression follow-up (2026-10-05): the existing pilot Playwright
journey now opens the actual smoke-created Note after confirming the canonical
File. It edits the Markdown H1 through the Notes editor and checks the note
name is unchanged, renames the note through the title control and checks the
H1 is unchanged, reloads to verify both values, then restores and verifies the
canonical smoke title/H1 in a `finally` cleanup. The workflow passes the
canonical smoke Note ID/title and requires those identities plus the H1 in its
UI evidence gate. This avoids contaminating the following restart/restore
checks; failed restoration fails the browser step and therefore the gate.
Python pilot workflow/redaction regressions pass 21/21, Playwright discovers
the one targeted test, Prettier and targeted ESLint pass, the Notes route suite
passes 4/4, and the full frontend suite passes 108 files/1,189 tests. Svelte
check reports zero errors and 75 warnings; full frontend lint exits 0 with
zero errors and 159 warnings; the production build passes. All 34 embedded
Bash commands parse. The browser mutation/recovery sequence has not run
against the deployment or authoritative GitHub runner.

Browser skip fail-closed follow-up (2026-10-05): independent review found the
Playwright command could exit successfully with a skipped test, after which the
workflow would write green UI evidence. The existing Pilot Release step now
validates its JSON report and requires exactly one expected pass with zero
skipped, unexpected, or flaky tests; these counts are recorded in `ui.env` and
required by the existing final evidence gate. Review of the pinned Playwright
1.63 JSON schema found `stats.expected` can also describe `test.fail()` whose
actual result failed, so the verifier additionally requires one test record
with `expectedStatus: passed`, outcome `expected`, and its sole result status
`passed`. The report is also bound to the canonical test title and
`tests/pilot.e2e.ts`, preventing a different passing test from substituting for
the journey. Missing/malformed reports and the verifier-before-success-marker
ordering are covered too. The standard-library verifier regressions pass 8/8,
including expected-failure, skipped, unexpected, flaky, extra-test, wrong-test,
and missing/malformed-report cases. Existing pilot workflow/redaction regressions
pass 21/21; workflow YAML parses and all 34 embedded Bash steps pass the
`bash -n` syntax check. This closes the local fail-open paths but does not substitute for a
browser run on the candidate revision; the hosted workflow remains unrun for
the dirty worktree.

Workflow-result artifact follow-up (2026-10-05): an independent audit found
that safe diagnostic evidence could be uploaded after the final summary gate
failed, leaving phase-level pass markers but no authoritative overall result.
The existing workflow now runs an unconditional summary finalizer before upload;
it verifies the summary's source/run identity and exactly one result, otherwise
rewrites it as `WORKFLOW_RESULT=failed` with the summary-validation phase. Upload
is gated on finalizer success, and the finalizer re-scans the completed evidence
tree (including the summary) with all configured secrets before upload.
Regression tests execute the actual extracted finalizer against a missing
summary, preserve a valid failed summary, and prove a configured secret in a
valid passing summary causes scan failure. Valid passed and failed summaries
are both preserved. The pilot workflow/redaction suite passes 25/25; workflow
YAML parses and all 35 embedded Bash blocks pass syntax
checks. No authoritative run has yet exercised these local changes.

Pilot regression rerun (2026-10-05): `scripts/test_redact_pilot_logs.py`
passes 31/31 and `scripts/test_pilot_ui_results.py` passes 8/8 against the
current worktree. The restore-drill preflight regression also passes all four
collision/query-failure cases using its mocked Docker command. These checks do
not execute the deployment workflow or validate an authoritative revision;
the worktree remains dirty at source SHA
`c5a82d830e0d890b28d82fc43f374869b9fd296d`.

Operator dispatch precision follow-up (2026-10-05): the runbook now requires
dispatching the reviewed branch/tag, filtering the run by exact candidate SHA
and `workflow_dispatch`, checking `headSha` and creation time, then verifying
the downloaded artifact's source SHA and `WORKFLOW_RESULT`. It explicitly
states that dispatch evidence is isolated GitHub-runner evidence, not FWS
deployment validation, and the push-only image publication job does not run on
dispatch. Image-digest and backup/restore path examples now use valid shell
placeholders instead of angle-bracket tokens; all 14 runbook Bash snippets pass
`bash -n`. Local pilot UI-result and workflow/redaction tests pass 8/8 and
25/25, and YAML-aware validation confirms the workflow parses and all 35
embedded Bash blocks pass `bash -n`. No candidate run was started.

Local replay safety preflight (2026-10-04): the Docker daemon is shared with an active RustShare stack and other containers; port 80 is already published. The current pilot job fixes the primary Nginx mapping at port 80 and its restore/upgrade drills use fixed Compose project names with `down -v` cleanup. A local replay was not started because it could conflict with active services or remove their named volumes; a clean GitHub runner remains the authoritative environment for the workflow. The local workflow changes are uncommitted and must not be represented as a tested exact revision.

Restore-drill safety follow-up (2026-10-04): `scripts/run-restore-drill.sh` no longer clears a fixed Compose project before starting. It fails closed if Docker cannot inspect the exact project or if containers, volumes, or networks already carry that project label; automatic teardown is disabled until this invocation enters restore-stack setup. Added collision and Docker-query-failure regression cases and documented the operator precondition in the backup/restore runbook. `bash -n`, the focused safety test, and `git diff --check` pass. No Docker restore drill was run; this is safety validation, not restore evidence.

Image evidence follow-up (2026-10-05): independent review found the restore drill built a new image from the worktree while reporting the candidate SHA, so the successful restore journey did not establish which revision ran. The restore Compose service now uses a supplied prebuilt image without a build fallback; the script checks image ID and OCI revision against the tested candidate, verifies the started restore backend uses that exact image ID, and records image identity in its report. A second review found the clean-install journey only checked the image tag; the workflow now checks the actual clean-install backend container image ID before running the canonical journey, records it, and requires it to equal the tested image ID. Restore evidence is similarly required to match that exact ID and candidate revision. Error cleanup failures are emitted and preserved in the failure report. YAML and all 31 embedded Bash steps parse, Compose config parsing, restore-drill safety regression, Rust formatting, and diff checks pass. No restore Docker run was attempted because the local daemon is shared and has conflicting published ports; the changed workflow remains uncommitted and unverified in an authoritative runner.

Calendar interoperability follow-up (2026-10-04): added owner-scoped `GET /api/v1/calendar/events/{id}/export` using the existing Calendar-enabled and owner-filtered lookup. Independent review found and prompted fixes for missing `DTSTAMP`, iCalendar line injection through recurrence data, bare-CR text handling, and west-of-UTC all-day end dates; a second static review found no remaining findings. Because this version does not generate `VTIMEZONE` or represent detached overrides, it explicitly returns `409` for non-UTC recurring masters and recurrence overrides; this limitation is documented in the API contract/spec and changelog, not treated as full recurrence interoperability. Nine serializer/parser unit tests pass; the round-trip parser is from the same `icalendar` crate used to emit the ICS, so these tests cover parseability and selected field semantics but are not independent third-party client-interoperability evidence. Server package Clippy and Rust formatting pass. The DB-backed Calendar API suite passed 24/24 on a fresh migrated disposable schema, including the owner/disabled-calendar export boundary; this establishes only current per-user owner/enablement scoping, not institutional group/workspace authorization.

Issue: [#333 — Institutional readiness for a ~100-user organization](https://github.com/KubedoIO/RustShare/issues/333)

Phase 5 follow-up (2026-10-05): the single-event `.ics` exporter has parser
round-trip tests for supported UTC recurrence and all-day dates using the
already pinned `icalendar` parser (9 focused export tests pass). Those parser
tests establish parseability and selected field semantics, not independent
third-party client interoperability. A new ignored DB integration test imports
a supported recurring UTC event through the real multipart endpoint/worker,
then exports it through the authenticated API and verifies source UID, UTC
start/end, and recurrence semantics. The API response is now parsed with the
independent `ical` parser and checked for one calendar/event plus UTC times and
RRULE; the raw UID line assertion remains to preserve the trailing-space case
that this parser trims. This ignored API/worker/database/object-store journey
passed 1/1 against a fresh migrated disposable schema and loopback RustFS.
Its first run exposed that export replaced
an imported UID with the RustShare row UUID; export now preserves non-empty
source UIDs with RFC 5545 TEXT escaping (including line-break injection
protection), while internal events continue to use the RustShare UUID. A
review found the importer trimmed UID text, collapsing distinct identifiers;
it now preserves leading/trailing spaces while rejecting whitespace-only UIDs.
Unit and end-to-end fixtures cover whitespace, comma/semicolon/backslash
escaping, whitespace-only rejection, and line-break injection protection. ICS
events with unsupported organizer/attendee, recurrence-exception (including
duplicate `RECURRENCE-ID` properties and `RECURRENCE-ID;RANGE`), privacy,
attachment, URL, related-resource, or alarm semantics now fail at event level
instead of being partially imported. Existing job counters and `last_error`
show property names/parameters only, not private values. Real-DB regressions
prove unsupported events are not persisted, attendee addresses are absent from
diagnostics, and a ranged recurrence override is rejected while its supported
master remains imported. The focused unsupported-event regression passed 1/1
against a fresh migrated schema and isolated test RustFS service. The full
ignored `calendar_import_test`
suite passes 10/10 on a fresh schema and covers UID-based re-import
idempotency, TZID/timezone handling, recurrence expansion, all-day spans,
malformed ICS, unsupported-field and duplicate/ranged-recurrence reporting,
bounded/error paths, and the
import→export round trip. Test databases were dropped; post-run queries
confirmed only the pre-existing
`postgres` and `rustshare` databases remain. These local results use the current
uncommitted worktree based on HEAD `c5a82d830e0d890b28d82fc43f374869b9fd296d`,
not a committed revision or deployed pilot. The Calendar API DB integration
target passed 24/24 on a fresh schema, including
`single_event_export_requires_enabled_calendar_and_event_ownership`; this
proves current per-user owner/enablement scoping, not institutional
group/workspace authorization.
Repository-wide inspection found that the API contract/spec had described a
per-user token feed that has no route, token table, or UI implementation. A
proposed ADR-0037 assessment withdraws that availability claim and recommends
deferral until secret paths are redacted by application tracing, bundled
Nginx, and the external TLS load balancer. It also recommends deferring full
CalDAV pending a named pilot-client requirement. Neither deferral is approved
yet; the ADR remains Proposed and the issue acceptance stays open pending
review. The contract/spec now state the actual implemented surface.

Independent calendar-export check (2026-10-05): repository inspection found
no independent iCalendar parser in the existing dependencies. Added the
parser-only `ical` crate as a server dev-dependency and two unit checks for the
supported UTC-recurring and all-day outputs. Both parse with that independent
implementation and assert UID, dates/times, recurrence, and all-day value type.
An independent review then identified that none of those checks forced folded
text. Added a long UTF-8 summary/description regression that requires folded
content lines within 75 octets and verifies the independent parser reconstructs
both values. The focused export module passes 12/12; server all-target Clippy
passes. This is independent syntax/selected-field evidence, not full RFC 5545
conformance or third-party calendar-client acceptance. No production behavior
changed.

Validation follow-up (2026-10-05): the real import→authenticated-export
integration test now parses the API response with the independent `ical`
parser and passed 1/1 against a fresh migrated disposable PostgreSQL database
and loopback RustFS. The full server library suite passed 548/548 with 9
ignored on a fresh migrated disposable database; its first attempt could not
resolve the `.env` Docker hostname from the host and no tests reached their
assertions, so it was rerun using the local PostgreSQL port mapping. Server
all-target Clippy and formatting pass. The full frontend baseline also passes:
Svelte check has zero errors and 75 warnings across 34 files, lint passes, all
1,188 tests across 108 files pass, and the production build succeeds. No
warnings reference the modified AuditTable component. These are local results
for the dirty worktree at HEAD
`c5a82d830e0d890b28d82fc43f374869b9fd296d`, not revision-bound pilot evidence.

Final Rust workspace follow-up (2026-10-05):
`cargo test --workspace --all-features --lib --locked` passed with 1,250
tests passed and 58 already-ignored tests. The run used the dedicated local
test PostgreSQL service and disposable database
`rustshare_test_issue333_workspace_lib_20261005`; the database was dropped and
its absence verified afterward. The first attempt exposed that the SMTP
localhost rejection test depended on the caller's
`RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS` environment. The test now serializes
with existing env-mutating tests, clears that flag during its assertion, and
restores the original OS value (including non-Unicode values); the focused
regression and all 418 core library tests pass. Workspace Clippy and core
Clippy pass, and final formatting passes. These remain local dirty-worktree
checks, not evidence for an immutable candidate revision.
The standalone `scripts/test-restore-drill-safety.sh` also passed its four
fail-closed preflight cases (container, volume, network collisions, and Docker
query failure); it uses a mocked Docker command and did not mutate the shared
Docker environment.

This plan sequences the work needed to take RustShare from its current individual-user and workspace capabilities to a supportable institutional evaluation. The target is not simply 100 accounts: an organization should be able to onboard people, run a calendar-led collaboration workflow, retain and find its organizational context, and administer access without routine developer or database intervention.

The existing pilot/deployment validation workflow remains authoritative. Add coverage to it as needed; do not create a parallel validation system.

## Current repository constraints

- Calendar is already a PostgreSQL-backed application with per-user ownership. The implementation and current scope are described in [ADR-0037](../adr/0037-calendar-application-and-external-sync.md), the [Calendar v1alpha1 spec](../specs/calendar-application-v1alpha1.md), and [issue #315 follow-up plan](2026-10-02-issue-315-calendar-followups.md). Workspace-shared calendars and delegation are not part of the current spec.
- Proposed [ADR-0030](../adr/0030-elembra-application-model.md) and [ADR-0032](../adr/0032-resource-refs-and-authorization.md) assign application data and authorization to the owning application and require owner-side checks for cross-application resources. Confirm their decision status and applicability during Phase 0 review.
- User groups exist. `user_groups.tenant_id` was added later with an all-zero default (`20260329170001_add_tenant_id_to_tables.sql`), but group names remain globally unique and `group_members` has no tenant key (`20260322000002_create_user_groups.sql`, `20260322000003_create_user_group_members.sql`). The permission resolver scopes membership lookup by the user's tenant but does not verify the group's tenant (`backend/crates/infrastructure/src/repositories/permission_resolver.rs`); group admin operations also need tenant-boundary review (`backend/server/src/handlers/admin/groups.rs`). Do not assume this is a safe organization-group model without compatibility analysis and a migration/recovery plan.
- Before any group migration or cleanup, run [`scripts/pilot-group-scope-inventory.sql`](../../scripts/pilot-group-scope-inventory.sql) using read-only credentials against the intended database. Review and securely retain its output as migration evidence; group deletion can cascade to group-targeted share rows (`20260401000001_add_group_sharing.sql`). The local schema check is not a substitute for inventorying pilot data.
- Admin actions and a combined admin audit endpoint already exist (`20260322000007_create_admin_actions.sql`, `backend/server/src/handlers/admin/audit.rs`). Extend these if they can meet tenant isolation, privacy, and retention needs; avoid a second audit system.
- No distinct organization/workspace membership model was found in the current migrations. Define how organization, tenant, workspace, and current resource ownership relate before implementing membership migrations.

Issue #327 asks for calendar-related information in workspace folders using date-structured JSON, but does not say whether that means canonical event storage or meeting artifacts/export. The current event model is PostgreSQL-backed and owner-scoped; ADR-0037 defers workspace sharing pending a permission design. Treat the storage wording as unresolved, not as a confirmed demand to replace PostgreSQL. Preserve the current model unless review establishes a concrete requirement and safe migration path for changing it.

## Intended outcome

At completion, a cleanly deployed RustShare revision can support the staged evaluation below using documented procedures and the repository's authoritative pilot workflow. Organization membership and resource permissions are understandable and tenant-bounded; administrators can perform routine lifecycle tasks in-product; the calendar-to-meeting workflow is usable; important administration is auditable; and recovery has been exercised.

Do not claim readiness solely because the features exist or the test suite is green. Each stage needs evidence from the deployed configuration and real persistence path.

## Phased implementation

| Phase | Scope | Main dependency / exit condition |
|---|---|---|
| 0. Resolve domain and architecture | Define tenant/organization/workspace mapping, roles, group compatibility, calendar sharing, and split #327 into bounded deliverables | Approved decisions and migration/compatibility strategy before schema changes |
| 1. Organization and workspace access | #330: tenant-scoped groups, bounded organization roles, workspace membership and explainable effective access | Cross-tenant and revocation tests pass; resource ACLs remain authoritative |
| 2. Administrative audit foundation | #332: trustworthy, tenant-scoped audit coverage for important administrative mutations | Mutations in phases 1 and later produce safe, queryable audit records |
| 3. Calendar-led workflow | #327: finish needed calendar usability work and define event-to-meeting relationships | Calendar sharing/storage decision is settled; normal workflow works for pilot roles |
| 4. Durable organizational memory | #328: connect meetings, notes, documents, decisions, and actions | Every linked resource is reauthorized by its owning application |
| 5. Calendar interoperability | #329: bounded iCalendar import/export and permission-aware subscriptions | Access, revocation, recurrence, and feed security behavior are specified and tested |
| 6. User lifecycle | #331: bulk onboarding and safe offboarding | Group/workspace model and audit foundation are in place |
| 7. Operational evidence and staged expansion | Complete deployed validation, recovery, support, and scale evidence | 10–15, 30–50, and ~100-user gates pass in order |

Some work can proceed in parallel after phase 0: calendar UI defects that do not alter sharing/storage semantics, audit event taxonomy, and test-fixture preparation. Do not parallelize dependent schema or authorization changes before the domain decisions are made.

### Phase 0 — Domain and architecture decisions

Current evidence confirms that a tenant column exists on groups but does not
establish tenant-safe group administration or membership: the column's default
is shared zero-value state, name uniqueness is global, membership rows have no
tenant key, and the resolver does not compare the group's tenant with the
principal's tenant. Group administration uses the global `AdminUser` guard
and group-ID lookups. These are implementation facts to preserve in the design
review, not a basis for an automatic backfill.

Write down and review these decisions, updating the relevant ADR/specs before implementation:

1. Whether the existing tenant is the institutional organization, and how workspace membership maps to it. Define behavior for current users/resources and any single-tenant or personal data already present.
2. Whether existing global groups can be safely migrated or must remain a separate legacy/ACL concept. Preserve existing access semantics; specify migration, rollback/recovery, and uniqueness behavior.
3. The minimum role set: organization administrator versus member, and existing workspace roles where they suffice. Organization administration must not implicitly grant content access.
4. Effective authorization when a user has both direct and group-derived access; explain that path to administrators and define the immediate effect of membership removal.
5. Calendar visibility and sharing for the pilot: personal calendars, shared/workspace calendars, delegation, and external provider identities. Keep application-owned authorization authoritative.
6. Whether any specific pilot requirement justifies changing ADR-0037's PostgreSQL storage design. If not, close the #327 JSON-storage request as superseded with rationale.
7. For #327, separate user-visible bugs and calendar workflow completion from provider integrations and any email functionality. Maintain and validate the already-designed per-user, read-only Google/Microsoft sync where needed; defer expanding providers or changing sharing semantics until justified.
8. Clarify whether “calendar items should move on the webUI in responsive way” means responsive presentation, drag-to-reschedule, or both. The current view has month/work-week grids but no drag interaction; do not claim drag behavior from the issue's ambiguous wording.

Exit criteria: decisions are recorded in accepted or explicitly approved ADR/spec changes; tenant-boundary and migration test cases are agreed; no unresolved design choice can silently broaden access or lose existing data.

### Phase 1 — Organization, groups, and workspace access (#330)

- Add tenant/organization scoping to group and membership operations, reusing the existing implementation only if the phase 0 compatibility review supports it.
- Provide bounded administrator operations to view membership, add/remove people, create/manage groups, and assign workspace roles using the supported UI/API.
- Define and expose why a user can access a resource (direct grant, group grant, or workspace role) without making the explanation itself an authorization bypass.
- Preserve existing resource-level checks. Organization admin status alone does not authorize reading private notes, files, or meeting content.
- Make group and workspace membership removal revoke derived access promptly. Define treatment of sessions, tokens, shares, and authored content without deleting user data implicitly.

Required verification: PostgreSQL-backed tests with at least two tenants; cross-tenant list/read/write denials; direct and group grant combinations; removal/revocation; administrator boundaries; migration compatibility against existing groups and ACLs. Authorization evidence must not rely only on mocks.

### Phase 2 — Administrative audit foundation (#332)

- Extend the existing admin action/audit path where practical. Avoid parallel event stores and keep event writes transactional with the mutation where possible.
- Record actor, tenant, action, target, time, and outcome for invitations, cancellations, disable/enable, role changes, group membership, workspace membership, sharing/access changes, and calendar administration.
- Keep metadata bounded and safe: never include passwords, tokens, invitation secrets, note/document bodies, or provider credentials. Specify retention and tenant-scoped pagination/filtering; provide export only if needed for the pilot and protect it as sensitive administrative data.
- Make partial/failed administrative operations distinguishable from successful ones. Define how records remain attributable if an account is later disabled or removed.

Phase 2 bounded evidence (2026-10-05): user disable, enable, and update
mutations now write their applicable audit records in the same PostgreSQL
transaction. Password updates include session deletion and device-token
revocation; quota and password events retain their existing names, while a
change to the existing global `is_admin` flag records `user.admin_status_changed`
with old/new boolean values. No organization or workspace role semantics are
implied. The guarded real-service regression injects device-token revocation
failure and audit-insert failures for disable, enable, and global-admin-status
changes; sequence markers prove each audit trigger fired, rollback assertions
verify state remains unchanged, and retries verify exactly one success event.
For the combined password/global-admin update, the injected admin-status
audit failure occurs after the password update, session deletion, device-token
revocation, and password-event insert have been attempted; rollback preserves
the original password, session, token, and audit state. A successful retry
verifies one password event and one admin-status event, and that the audit
details contain no password value.
Group member add/remove handlers now likewise commit the membership mutation
and its existing `group.member_added` / `group.member_removed` event in one
transaction. The guarded handler-level PostgreSQL regression forces each
audit insert to fail, proves the trigger fired via a non-transactional sequence
marker, verifies the membership and audit state rolled back, then retries and
checks one success event with the expected actor and member. This is same-tenant
transaction/audit evidence only; it does not establish tenant-scoped group
authorization.
Group create/update/delete now also commit their mutation and corresponding
audit event together. The handler-level regression injects each audit failure,
verifies create leaves no group, update preserves the old name, and delete
preserves the group; after removing the failure trigger, all three retries
succeed with exactly one matching event. This changes no group tenant or
authorization semantics, which remain Phase 0 decisions.
Admin-user create and permanent-delete operations now likewise commit the
user row mutation and applicable audit event in one transaction. The guarded
PostgreSQL/RustFS handler regression injects both audit failures, verifies the
failed create leaves no user and the failed delete preserves the user, then
retries successfully and checks exactly one event per operation. It also
verifies the create event contains the username but not the password. This
does not change preference seeding or post-commit object cleanup semantics.
Admin webhook create, update, and delete now commit their PostgreSQL mutation
and corresponding admin audit event in one transaction. Webhook audit details
now record the name only, not the configured URL (which may contain embedded
credentials or sensitive query parameters) or the signing secret. The ignored,
guarded handler-level regression injects create, update, and delete audit
failures, checks sequence markers, proves each mutation rolls back, retries
each route and verifies exactly one event, and verifies audit metadata omits
URL tokens and signing secrets. It passed against the dedicated loopback
PostgreSQL/RustFS test services. A post-test database query found zero fixture
users, webhook rows, audit rows, sequences, functions, or triggers. Formatting,
server all-target/all-feature Clippy, and transaction-test target compilation
also pass. This remains local evidence from the dirty worktree, not a
revision-bound workflow result.
Template create/update audit events now use an allow-listed projection of the
template key and application ID instead of serializing the full request, which
can contain default-file bodies and application/UI configuration. The focused
server unit regression passes. The template service still owns its mutation
outside the handler's audit transaction, so these events remain best-effort
until the service boundary can support an atomic write; this change only
removes unnecessary content from audit metadata.

The separate user-facing `POST /api/v1/applications/from-template` path has
the same audit limitation and a wider partial-success window: it persists the
template-created object before reloading the template, may then initialize
Kanban state, and only afterward attempts the audit insert through a helper
that logs and suppresses insert errors. Object files are uploaded to RustFS
before their metadata transactions, and folder/file/Kanban work spans multiple
commits, so a handler-only PostgreSQL transaction cannot make this operation
atomic or safely undo its effects. No route-level failure-injection regression
currently proves this path. Treat user-facing template instantiation as
best-effort audited, not as satisfying mandatory audit durability; changing
that contract requires an explicit partial-success/idempotency decision and a
coordinated storage boundary, not a local audit wrapper.

Admin workflow update, enable, and disable handlers now commit their database
mutation and applicable audit event together. A guarded real-service handler
regression injected `workflow.updated` and `workflow.disabled` audit failures,
verified rollback with sequence markers, retried both routes successfully,
and confirmed exactly one empty-detail event per operation. The test passed
against the isolated PostgreSQL/RustFS services; post-run checks found no
fixture users, workflows, audit rows, sequences, functions, or triggers.
Formatting, server all-target/all-feature Clippy, and test-target compilation
pass. The ignored integration regression now also exercises workflow enable:
it uses a tenant-scoped `invite_email` fixture and temporarily satisfies the
SMTP configuration check, injects an audit failure, verifies the workflow
remains draft with no event, then retries and verifies active state and exactly
one success event. The test catches assertion unwinds so DB fixtures and the
SMTP singleton are restored before rethrow; the SMTP snapshot matched exactly
before and after the local run, and post-run queries found no fixture rows or
trigger/function/sequence objects. It passed 1/1 against the isolated local
PostgreSQL/RustFS test services. This only satisfies the handler's SMTP
precondition; it does not test email delivery. This remains uncommitted local
evidence, not a revision-bound workflow result.
The follow-up serial run of all six ignored PostgreSQL/RustFS audit handler
regressions passed 6/6 in 4.91 seconds. The normal target run passed all 7
guard tests and correctly left the six service tests ignored by default.
The user lifecycle regression also verifies a re-enabled user does not regain
the revoked session and authored file bytes survive. That PostgreSQL/RustFS
integration test passed 1/1 on the dedicated loopback test stack; the ordinary
target passes 7/7 guard tests (six real-service tests are ignored by default),
and server all-target/all-feature Clippy passes. Group add/remove integration
passed 1/1 against the isolated
`rustshare-test` PostgreSQL/RustFS services after provisioning and migrating
the dedicated `rustshare_test` database. Cleanup attempts every fixture
teardown, surfaces the first cleanup error, and a post-run query found zero
generated group users, groups, membership audit events, sequences, functions,
or triggers. Group create/update/delete integration also passed 1/1 against
those services; a post-run query again found zero generated users, groups,
group audit events, sequences, functions, or triggers. Admin-user
create/delete integration passed 1/1 against the same services, with a
post-run query finding zero fixture users, audit rows, sequences, functions,
or triggers. The membership test's
first guarded attempt stopped during harness initialization
because the bucket resolved to the ordinary `rustshare` name. Explicit
`S3_ENDPOINT` and `S3_BUCKET` settings fixed the precondition; the failed
attempt ran before fixture insertion. This remains uncommitted local evidence,
not an authoritative revision. Invitation, workspace membership,
share-event unification, and calendar-specific audit semantics remain gated on
Phase 0. At this point in the investigation, application, template, and
remaining admin-configuration mutations still used the shared best-effort
helper. Subsequent bounded cases below close transactional auditing for admin
application/template CRUD and OIDC, SMTP, and security configuration. A current
source search finds the helper is now called only by user-facing
application-from-template instantiation, which remains best-effort as documented
above. This work does not make the overall audit system reliable or
tenant-scoped.

OIDC provider configuration is now another bounded transactional case: the
handler locks the singleton config row, persists the update and empty-detail
`config.oidc_updated` audit event in one transaction, then invalidates the
runtime cache after commit. The guarded route-level regression injects an audit
insert failure and proves configuration rollback, then retries successfully
and verifies the client secret is encrypted at rest and the audit detail has
no secret. The regular OIDC integration target passed (9 passed, 1 ignored),
the ignored rollback test passed (1/1) against the disposable loopback
PostgreSQL/RustFS stack, and `cargo fmt --all -- --check` passed. A test-harness
guard regression that modified process-global environment variables was
removed after review found it could race parallel tests; the underlying URL
classification and explicit opt-out remain covered by non-mutating helper
tests. These are local working-tree results, not evidence from a
revision-bound workflow.

SMTP configuration is now another bounded transactional case: the handler
locks the singleton config row and commits the update with an empty-detail
`config.smtp_updated` event. The ignored real-route PostgreSQL/RustFS regression
injects an audit failure, proves rollback, retries successfully, checks the
masked response and encrypted-at-rest/decryptable SMTP password, and verifies
the audit detail is empty. The guarded route test passed 1/1; the ordinary SMTP
target passed 8 tests with one guarded test ignored; server all-target,
all-feature Clippy and formatting passed. The post-run query found no fixture
users/tenants/audit rows or injected DB objects, and confirmed the SMTP
singleton reset. These remain local working-tree results, not revision-bound
evidence.

Application configuration update is now a bounded transactional case: the
handler locks and updates the tenant's existing enablement row, then inserts
the existing `application.updated` event in the same transaction. The ignored
real-route regression injects an audit insert failure and verifies rollback,
then retries and checks the persisted normalized configuration and exactly
one correctly-targeted audit event. The test also removes the folder events
created while seeding default applications, so its durable fixture state is
cleaned even though the event table has no user foreign key. The guarded test
passed 1/1 against the disposable loopback PostgreSQL/RustFS services; the
ordinary target passed 4 tests with the guarded test correctly ignored.
`cargo fmt --all -- --check`, server all-target/all-feature Clippy, and
`git diff --check` passed. A post-run query found no fixture tenant/user,
orphaned application audit row, or application enablement without a tenant.
This is local working-tree evidence, not evidence from a revision-bound
workflow.

Template create, update, delete, and duplicate now commit with their existing
audit events. Update and delete lock their tenant/key row before validation and
mutation; create and duplicate use the same transaction for insert and audit.
The real-route PostgreSQL regression injects an audit failure for each action,
proves rollback, retries each action, and verifies the established action,
target, and detail contract—including duplicate's original-template target.
The disposable fully migrated test database confirms the `application_id`
column and `(template_key, tenant_id)` unique constraint. The guarded test
passed 1/1; the ordinary target passed 4 tests with the guarded test correctly
ignored; workspace formatting, server all-target/all-feature Clippy, and
`git diff --check` passed. A post-run query found no fixture user/template,
audit rows, trigger, function, or sequence. These are local results, not
revision-bound workflow evidence.

The global security-configuration update now writes the singleton setting and
its existing `config.security_updated` event in one transaction. The real-route
test injects audit failure, checks rollback of all fields and `updated_at`,
then retries and verifies the exact event and response. Its cleanup restores
the prior singleton values/timestamp only if the row still matches the
timestamp correlated to this test's actor-attributed audit event and the exact
values, avoiding overwrite of a concurrent operator change. The guarded route test passed 1/1; the ordinary target passed
4 tests with the guarded test correctly ignored; workspace formatting and
storage/server all-target Clippy passed. The fixture cleanup query found no
test user, orphan audit event, trigger, function, or sequence. These remain
local results, not revision-bound workflow evidence.

Application enable/disable now lock and update the tenant-scoped enablement row
and insert the existing audit event in one database transaction. Enable first
ensures root folders using the existing idempotent service path; those folder
rows/events intentionally remain if the later status/audit transaction fails,
so retry can safely proceed. Optional remote Chat provisioning remains
post-commit and best-effort, as before. The real-route regression covers audit
failure rollback, retry, tenant isolation, and these side-effect boundaries.
Its ordinary target passes 4 tests with the guarded test correctly ignored;
workspace formatting, server test-target Clippy, and `git diff --check` pass.
The guarded real-route test passed 1/1 against the existing isolated local
Compose project `rustshare-test`: loopback PostgreSQL on port 15432, migrated
`rustshare_test` database, and RustFS on port 19000 using the `rustshare-test`
bucket. It proves the existing nested-path-to-final-segment mapping, rejects a
new nested root-path update, rejects a root-path change between folder
preparation and the locked enablement update, verifies enable/disable audit
failure rollback and retry, prepares a new single-segment root path for an
enabled app before config commit, and checks tenant isolation and folder
preservation on disable. Its ordinary test target passes
4 tests with the guarded test ignored. Post-run fixture inspection found 0
tenants, users, admin actions, and application enablements; the 8 pre-existing
events remained, and no test trigger/function/sequence remained. Workspace
formatting, server all-target/all-feature Clippy, and `git diff --check` pass.
New root-path updates now reject nested, empty, and `.` components because the
current storage mapping supports one folder directly under `/Workspace` only.
Existing nested root configurations retain their legacy final-segment mapping
so startup does not silently relocate data; an existing nested setting may
still differ from its actual provisioned folder. For such pilot tenants, set
the configured root to that existing single-segment folder only after
confirming the data location. Supporting true nested roots requires an explicit
data-move/compatibility policy. This is local working-tree evidence, not
evidence bound to a clean revision or authoritative CI run. Unified admin-audit
detail redaction now uses typed, action-specific allowlists; free-form security
descriptions and share-access network metadata are omitted. Group lifecycle
events retain only a bounded, non-control display name. Eight sanitizer unit
tests and a guarded real-route regression passed on 2026-10-05 against the
isolated local PostgreSQL/RustFS stack; cleanup verification found no matching
tenant, user, admin action, security event, share, or file rows. This closes
detail redaction for that endpoint only and is not revision-bound or
authoritative workflow evidence.

Audit path minimization follow-up (2026-10-05): user-facing template creation
no longer stores the created object's path in its `admin_actions.detail`; the
template key and object ID remain. The guarded real-route redaction regression
now seeds a legacy `object.created.from_template` event containing a private
path and asserts the exact projected detail and complete API body omit it. The
test passed 1/1 against the isolated local PostgreSQL/RustFS stack, and the
audit projection unit suite passed 8/8. This does not remove paths from
historical database rows; the endpoint projection continues to suppress them.
These local results are not revision-bound or authoritative workflow evidence.

Application root-path concurrency review (2026-10-05): the enable flow
provisions a folder before its transaction, then locks and compares the current
configuration; a root-path change in between is rejected and the app stays
disabled. The guarded route test passed 1/1 against the isolated local test
services. The configuration-update route also pre-provisions a requested root
before its transaction. Concurrent updates can therefore leave an additional
`Workspace` folder after the other request's configuration wins; each
successful request provisions its own requested root, so review found no
committed configuration pointing to a missing folder. Root folders may contain
user data and are intentionally not deleted on conflict or update. Operational
workaround: serialize root-path changes. Before adding cleanup or changing
concurrent-update semantics, define whether old roots are retained as user
content and how emptiness/reference safety is established. This is an
unresolved UX/cleanup risk, not evidence of data loss.

Audit-read-path gap (2026-10-05): `list_audit_log` merges admin, security, and
share event sources without tenant predicates. The guarded PostgreSQL/RustFS
route regression now verifies anonymous access is rejected (401), an
authenticated non-admin is rejected (403), disabled-admin bearer access is
rejected (401), a persisted disabled-admin session cookie is rejected during
authentication (401), and an active admin can read the redacted result (200).
The cookie result changed with the shared session resolver hardening recorded
below. This establishes the existing global `is_admin` boundary only. Tenant
scope cannot be added safely
until Phase 0 defines tenant-level administration; cross-tenant semantics and
scope tests remain required before this is an operator-safe cross-tenant audit
surface. The separate share access-log and self-service security-event routes
were not redacted or covered by this work. Evidence: `backend/server/src/handlers/admin/audit.rs`,
`backend/server/src/handlers/extractors.rs`, and
`backend/tests/admin_audit_redaction_test.rs`; focused guarded route passed
1/1 on 2026-10-05 against the dedicated local PostgreSQL/RustFS services. This
is local evidence, not a hosted revision-bound result or approved authorization
change.

Required verification: expected events are present for success and failure paths; tenant isolation and pagination/filter tests pass; sensitive values/content are absent; a pilot administrator can answer who changed membership/access and when without database access.

### Phase 3 — Calendar-led collaboration workflow (#327)

- Start from the current calendar implementation and issue #315 follow-up plan. Fix only the calendar UI/API defects needed for the agreed pilot journey; verify the existing per-user, read-only Google/Microsoft integrations where configured rather than treating their existing contract as deferred.
- Specify the event lifecycle and visibility for the roles chosen in phase 0. Do not introduce workspace-shared calendars as an incidental side effect of UI work.
- Provide the agreed path from an event to its agenda and meeting materials, keeping Calendar as the entry point if that is the evaluated workflow.
- Resolve time zone, recurrence, attendee, update, and deletion semantics needed for the selected scenarios. Defer external-provider synchronization until its specific need and ownership model are clear.

Required verification: representative create/edit/view flows, concurrent or stale update behavior as applicable, same-tenant foreign-owner and cross-tenant access denials, time-zone edge cases, responsive viewport coverage for the agreed interpretation of the UI request, and one browser journey using the real deployment.

### Phase 4 — Durable organizational memory (#328)

- Link an event/meeting with its agenda, documents, Notes, decisions, and actions using the repository's ResourceRef and owner-application patterns where applicable; do not duplicate private content into a central meeting record merely for convenience.
- Ensure each read/search/navigation of a linked object is authorized by the source application at access time. A stored reference, cached projection, or search result is never proof of permission.
- Define lifecycle behavior for deleted/moved/unavailable source resources and for a user whose membership is revoked. Retain organizational records according to an explicit policy without exposing them to former or unrelated members.
- Use the existing durable outbox pattern for cross-application side effects that require reliable delivery; consumers must tolerate duplicate delivery and not silently lose events.

Required verification: end-to-end create/edit/reopen journey; permission changes after linking; cross-tenant and revoked-user denials; deletion/stale-reference behavior; duplicate/retry behavior for any asynchronous projection.

### Phase 5 — iCalendar interoperability (#329)

- Implement the smallest interoperability surface proven necessary: import/export and/or read-only subscriptions, not full CalDAV by default.
- Specify whether subscriptions are private per user or shared, how feed credentials are generated/revoked, and how current permissions are enforced on every fetch. Avoid long-lived URLs that continue exposing data after access removal.
- Define stable event identifiers, update/deletion behavior, time zones, recurrence, and duplicate handling across repeated imports.
- Validate untrusted ICS input and bound resource use; do not log feed secrets or imported private content.

Required verification: standards-compatible fixtures, malformed/hostile input, repeated import idempotency, recurrence/time zones, permission changes and revocation, and no cross-tenant disclosure.

### Phase 6 — Bulk onboarding and offboarding (#331)

- Support a documented CSV or similarly simple structured import. Preview and validate before mutation; report duplicates, invalid fields, unresolved groups/workspaces, and per-row outcomes clearly.
- Use the existing invitation/authentication mechanism. Support resend/cancel as appropriate, but do not expose invitation tokens in logs or audit details.
- Make bounded batches idempotent and report partial failure so an administrator can safely retry without duplicate users or grants.
- Define disable/offboard semantics for sessions/tokens, group/workspace access, shares/capabilities, and user-authored content. Revoke access safely while preserving content according to explicit policy; do not silently hard-delete it.
- Current single-user lifecycle gap closed locally: the existing disable endpoint wraps `users.disabled_at`, web-session deletion, and device-token revocation in one PostgreSQL transaction. The real PostgreSQL/RustFS regression injects a token-revocation failure and proves rollback, continued cookie authentication, and no success audit action; after restoring the dependency, retry proves the user is disabled, the session is deleted, the device token is revoked, the cookie is rejected, and exactly one success audit action exists. It also verifies both the authored Markdown metadata row and the exact object bytes remain in S3-compatible storage. The test is registered in `backend/server/Cargo.toml`; its guards require an explicit disposable local database name and `RUSTSHARE_TEST_DISPOSABLE_DB=1`, plus an explicit `RUSTSHARE_TEST_DISPOSABLE_OBJECT_STORE=1`, loopback-only S3 endpoint, and `rustshare-test*` bucket before the harness can create/use storage. On 2026-10-05 it passed (1/1) against a fresh migrated schema on isolated local PostgreSQL/RustFS services; the test-created database was dropped and verified absent, and the test removed its object key from the local test bucket. The non-DB target passed 7 tests with the real-service test ignored; formatting, target compilation, and server all-target Clippy pass. This remains an uncommitted working-tree result, not evidence for a committed revision. Group/workspace access, shares/capabilities, bulk lifecycle operations, and broader semantics remain gated on Phase 0 authorization decisions.
- Record each significant operation in the audit trail and scope every import to one organization.

Required verification: dry-run/preview, valid and invalid rows, duplicates, retry after partial failure, large-but-bounded input, unauthorized cross-tenant assignment, session/token revocation, preservation of content, audit coverage, and UI/API operation by an organization admin without DB access.

### Phase 7 — Operational proof and expansion

- Extend the existing authoritative pilot workflow to bind all results and artifacts to the exact source SHA, build/version, deployment/config identity, start/end time, and failing phase.
- Use deployed PostgreSQL and object storage for persistence and recovery tests. Exercise backup, destructive replacement of the test state, restore, restart, and the canonical verification journey. Do not treat a backup command succeeding as recovery evidence.
- Prove a compatible prior revision can be upgraded to the candidate and migrations fail visibly. Do not claim downgrade support unless it is designed and tested.
- Document clean deployment, configuration, health checks, logs, backup/restore, upgrade/recovery, and support/incident escalation. Keep secrets out of diagnostic artifacts.
- Record basic content-free usage and error/support signals needed to understand pilot burden. Do not introduce content analytics or a new observability platform.
- Establish load and latency targets from a baseline before asserting “100-user scale”; test the expected concurrency and data shape (about 100 users, groups, and workspaces) on the intended deployment class.

## Staged acceptance gates

### Gate A — 10–15 users

- An organization administrator can configure users/groups/workspaces and perform routine membership tasks without developer or database access.
- Users can complete the agreed calendar-led meeting workflow, including agenda/materials and Notes, with access checks intact.
- No unresolved critical authentication/authorization issue; cross-tenant negative tests pass.
- Backup and restore are documented and exercised against real durable state; restart preserves pilot data.
- Admin mutations are auditable, and an operator can diagnose application, database, authentication, and storage failures relevant to the deployed configuration.

### Gate B — 30–50 users

- Normal calendar/meeting workflows are repeatable with representative groups and workspaces.
- Organization administrators can understand and adjust effective membership; major admin mutations are visible in the audit trail.
- Support burden and recurring failure modes are recorded; no open P0/P1 data-loss or authorization defect remains.
- Backup/restore and upgrade paths have current evidence for the candidate revision.

### Gate C — approximately 100 users

- Bulk onboarding and offboarding are proven with representative input and safely recoverable partial failure.
- Calendar workflow remains stable at agreed pilot concurrency; the access-negative suite remains green.
- Recovery and upgrade are repeatable, and routine administration can be performed by customer administrators without vendor developer intervention.
- Operational limits, known risks, support ownership, and accepted deferrals are explicit.

Passing a later gate does not waive an earlier gate. Any exception is documented with an owner, workaround, expiry/review point, and affected pilot operation.

## Cross-cutting test and evidence requirements

- Tenant isolation: organization, group, workspace, calendar, meeting, resource-reference, search, and audit access across two or more tenants.
- Authorization lifecycle: direct/group-derived access, role changes, removal, session/capability revocation, and reauthorization of linked resources.
- Persistence: create representative organization data, stop/restart the application, authenticate again, and verify it; then repeat after backup/restore and candidate upgrade.
- Failure visibility: unavailable database/storage/identity dependency and invalid configuration must fail readiness or the relevant operation clearly, emit useful redacted diagnostics, and recover after restoration.
- Integrity: migration failures cannot be ignored; imports and asynchronous work have visible outcomes; retries do not mask failures or duplicate effects.
- Evidence artifact: exact SHA/version, environment/deployment identity, configuration fingerprint without secrets, workflow/run identity, phase results, failure diagnostics, and known limitations.

Use the existing pilot validation workflow and CI conventions. Add focused tests to the owning application and extend the workflow at the point that needs deployed evidence. Do not make a green result possible through skipped phases, swallowed setup/cleanup failures, or retries that hide the first failure.

## Risks and decisions to track

| Risk / open decision | Why it matters | Required treatment |
|---|---|---|
| Tenant, organization, and workspace do not yet have a clearly verified membership mapping | A wrong mapping can expose content across institutions or make admin actions ambiguous | Resolve in phase 0; no membership migration before this is reviewed |
| Existing groups have a `tenant_id` column but retain global name uniqueness; membership rows are unscoped and resolver/admin checks do not establish group-tenant integrity | Scoping or renaming them can change existing access or permit cross-tenant administration | Inventory consumers and data, then approve migration/backfill and recovery with compatibility and cross-tenant membership tests; preserve old ACL semantics |
| Calendar is currently owner-scoped while the institutional workflow needs shared access | Sharing affects authorization and schema, not just presentation | Decide and update ADR/spec first; do not bypass owner-app authorization |
| #327's date-structured workspace JSON request is ambiguous against the PostgreSQL event model in ADR-0037 | Treating artifacts/exports as canonical event storage (or vice versa) risks data loss and duplicated sources of truth | Clarify whether JSON means meeting artifacts or canonical events; preserve PostgreSQL unless a reviewed ADR and migration plan justify otherwise |
| Existing admin audit records are generic and not visibly tenant-scoped | An audit view may leak records or be insufficient for accountability | Scope and test the existing audit path before relying on it for the pilot |
| High: hard-delete user API is not a safe substitute for pilot offboarding | `delete_admin_user` deletes the user and cascading metadata, then starts unawaited direct object-store deletes from `file_versions.storage_key`; process exit or storage failure can leave orphaned objects, and direct deletion bypasses shared-reference protection. The existing object-GC queue checks references, but the content-addressed worker is opt-in (default false), the existing enqueue API is not transaction-bound, and legacy keys follow a separate retention path (`backend/server/src/handlers/admin/users.rs`, `backend/server/src/object_gc.rs`, `backend/server/src/retention.rs`, `backend/crates/storage/src/metadata.rs`). | For pilot offboarding use the existing disable operation, which preserves authored content. Keep hard delete outside the #331 offboarding flow until a reviewed policy and a transactional, reference-safe cleanup path cover all user-owned durable object keys; verify shared-reference safety and retry/recovery against real PostgreSQL and object storage. |
| Google/Outlook/email integration can expand without a bounded evaluation need | Provider sync introduces identity, credential, conflict, retry, and support complexity | Choose only a provider/surface required by the pilot; defer the rest explicitly |
| Data retention, support owner, SLOs, and concurrency targets are not specified here | These depend on pilot agreements and deployment capacity | Agree with pilot owner/operator before Gate A; do not invent values in implementation |

## Explicit non-goals

- Full CalDAV unless evaluation evidence proves the bounded iCalendar surface inadequate.
- SCIM/HRIS integration, billing, or GDPR hard deletion as part of #331.
- A bespoke school/organization role taxonomy or customer-specific domain model.
- A new audit, event, deployment, or observability platform where an existing repository mechanism can be safely extended.
- Broad email functionality, speculative Bund features, or unrelated UI redesign.
- Declaring 100-user readiness from account count, unit tests alone, or a single successful browser run.

## Completion checklist for #333

Phase 5 follow-up (2026-10-05): Calendar imports now enforce a cumulative
10,000-VEVENT ceiling during the existing unfolded-input scan, before
component parsing or database writes. Unit coverage checks the parser rejects
10,001 events; the ignored real-DB integration test passed against the isolated
local test PostgreSQL/RustFS services and verified the failed job leaves zero
imported rows. The API contract, product spec, and changelog state the limit.
The 10,000-event threshold bounds work but has not been derived from capacity
benchmarking.

Capacity baseline (2026-10-05): two successful runs of the 10,000-event ignored
integration test completed through the real multipart endpoint/import worker
with isolated PostgreSQL 16.15 and RustFS containers. Each persisted all 10,000
events without failures from an 888,932-byte ICS file. Upload-to-terminal-job
times were 65.593 and 67.929 seconds; direct `/usr/bin/time` runs measured
56,448 and 56,784 KiB peak RSS for the test process (the API and worker share
that process; PostgreSQL and RustFS RSS are excluded). The full ignored
Calendar import PostgreSQL/RustFS integration suite subsequently passed 12/12
in 70.18 seconds, including the max-size case. The runs were serial on the
Linux x86_64, 16-vCPU/16-GiB development host, with no CPU or memory limits
configured on either test container. This is a local baseline, not a pilot-host
capacity guarantee or an agreed latency target; the candidate worktree was
dirty and the results are not bound to a committed SHA. The import is
asynchronous, and the current 30-second test poll default is not used for this
maximum-size case (which uses a 300-second test safety timeout).

Import request-bound follow-up (2026-10-05): the import route now caps the
complete multipart request at 11 MiB in addition to the existing 10 MiB file
field and 10,000-VEVENT limits. Oversized multipart errors return 413 rather
than being misreported as internal failures. A real DB-backed regression
verified that both an over-limit file and an oversized ignored multipart field
are rejected without creating import jobs or source rows. The full isolated
PostgreSQL/RustFS `calendar_import_test` suite passed 13/13 in 67.56 seconds;
formatting, server all-target Clippy, and `git diff --check` pass. This is local
evidence from the dirty worktree based on HEAD
`c5a82d830e0d890b28d82fc43f374869b9fd296d`, not revision-bound pilot evidence.

Pilot-host capacity inventory (2026-10-05, read-only; private address omitted)
reports 4 vCPUs, 8,126,548 KiB RAM, and a 124 GiB root filesystem (20 GiB
used). The current backend container is `rustshare-backend:pilot-c3648fdb918a`
(image `sha256:02862e272beadd035471808a13c25a8b05cc4ba13a5f531ecf7d38acca2072e8`),
limited to 1 CPU and 512 MiB; PostgreSQL is limited to 1 CPU and 1 GiB. RustFS
and Nginx have no container CPU/memory limits. These facts identify the target
deployment class but are not load-test evidence; no stress test was run against
the live pilot. The current deployed image is not the uncommitted candidate,
and the 16-vCPU/16-GiB local import measurements must not be projected onto it.
The public `/health` endpoint returned HTTP 200, and `/health/ready` returned
HTTP 200 with `ready` and healthy database, object-storage, event-delivery, and
auth-session components in five consecutive reads. One earlier read briefly
reported the optional outbox component as unhealthy; five follow-up reads were
healthy. No logs were collected for that transient sample, so it is recorded as
unclassified rather than treated as a confirmed outage or ignored as proof of
recovery. These reads are operational snapshots, not candidate-revision evidence.

Diagnostic-artifact sanitization follow-up (2026-10-05): an independent
workflow review found that the previous-release upgrade drill wrote raw
backend logs into the uploadable evidence directory. The workflow now writes
that raw log under `/tmp`, runs it through the existing byte-oriented redactor,
and places only the redacted output into the evidence directory. The final
artifact gate also requires successful collection, no detected configured or
pattern secret in logs, and a clean evidence-tree scan; this closes the case
where an earlier collection error could still leave `safe_to_upload=true`.
Redaction now fails closed on tree-walk/read errors without echoing exception
details, and literal configured secrets are tested with metacharacters,
backslashes, and newlines. The existing Trivy `CRITICAL` scan has
`exit-code: '1'` before image push; tests now guard its order and error policy.
The focused redaction/workflow regression suite passed 21/21, YAML parsing,
shell syntax checks, Python compilation, and `git diff --check` passed. These
are local checks on a dirty worktree, not a revision-bound workflow run; do
not treat diagnostic-artifact safety as proven for a candidate until the
updated authoritative workflow passes and its artifact is inspected.

Inventory counter semantics follow-up (2026-10-05): mismatch counters now
require the joined user/resource to exist before comparing recorded tenant
IDs; unresolved member/file/folder/owner/creator references have separate
counters. This prevents a missing join from being reported as a tenant
mismatch. The migrated PostgreSQL regression asserts these missing-reference
counters are zero for valid fixtures and continues to verify the known tenant
mismatches. The current schema enforces foreign keys/cascades for these
references, so this fixture does not disable constraints to synthesize orphan
rows; detection of constraint-bypassed corruption remains defensive and is
not directly exercised.

Full inventory script smoke (2026-10-05): after applying every repository
migration to a fresh disposable PostgreSQL container using the CI-pinned
pgvector image, the complete SQL file ran successfully through `psql -X` with
`ON_ERROR_STOP=1` against the empty migrated database. The container was
stopped and removed. This validates SQL/script execution only; it is not a
production inventory or populated-data result.

Pilot workflow local validation recheck (2026-10-05): the current evidence
redaction regression passed 21/21 tests; restore-drill collision/query-failure
preflight checks passed; all 34 embedded Bash blocks parsed with `bash -n`,
the workflow YAML parsed, and the edited pilot scripts passed shell syntax
checks. The restore-drill preflight suite uses a fake Docker executable and
proves refusal behavior only. These are local static/safety regressions, not a
workflow execution, clean deployment, or revision-bound pilot result.

Pilot aggregate-result gate regression (2026-10-05): extended the existing
workflow/redaction test module to execute the workflow's extracted summary gate
against synthetic evidence. It proves missing reports, malformed timestamps,
duplicate phase results, failed database-dependency evidence, and a mismatched
candidate SHA cannot produce `WORKFLOW_RESULT=passed`; a failed job is recorded
as failed. `python3 scripts/test_redact_pilot_logs.py` passes 28/28. The
workflow itself was unchanged, and no hosted or deployment workflow was run;
this remains local regression evidence only.

Authentication dependency failure follow-up (2026-10-05): the previous
`AUTHENTICATION_FAILURE_STATUS` marker represented only invalid-password
rejection after PostgreSQL recovery, not an authentication dependency outage.
The existing failure drill now submits the seeded viewer's password login
while PostgreSQL is stopped and requires HTTP 500 (distinct from invalid
credentials), then restores PostgreSQL and requires that same login to succeed
with HTTP 200. It separately retains the invalid-credential 401/403 check and
records that this clean pilot workflow has password login enabled and OIDC
disabled. Credentials are passed to `jq` through its environment input rather
than command arguments; the response token remains only in a shell variable
and is not written to evidence. The phase report now includes and the aggregate
gate verifies source SHA, build version, deployment/config identity, workflow
run ID/attempt, and exactly one valid start/end timestamp. GNU `date` round-trip
validation rejects impossible calendar values; epoch comparison rejects a
finish before start. Regression cases cover malformed, impossible, duplicate,
and reversed timestamps. The extracted-workflow and aggregate-gate regression
suite passes 31/31; workflow YAML and both edited Bash steps parse, and
`git diff --check` passes. This does not exercise an external OIDC provider,
which is not configured by Pilot Release, and no deployed failure drill or
hosted candidate run has yet verified this sequence.

Local integration-harness safety follow-up (2026-10-05): review found the
shared calendar/OIDC test harness accepted the Docker alias `postgres`,
defaulted to database `rustshare`, and could auto-create a bucket against an
unguarded S3 endpoint. It now accepts only loopback PostgreSQL hosts and
`rustshare_test*` database names by default; the explicit remote override still
cannot bypass the database-name check, and the no-env fallback now names
`rustshare_test`. Object storage is restricted to loopback on test ports 9000
or 19000, credential-free endpoint URLs, and `rustshare-test*` buckets before
auto-create. The Docker aliases were removed; tests cover database-name and
alias rejection plus S3 remote endpoint, invalid scheme, unsafe bucket,
userinfo, IPv6, and unexpected-port cases. All seven focused guard tests pass.
A second independent review found no remaining object-store guard issue and
confirmed compatibility with the integration workflow's loopback endpoint and
test bucket. No RustFS/database integration run was needed for these pure guard
tests.

An ignored real-route OIDC discovery outage/recovery regression now uses a
loopback mock issuer, requires explicit disposable-database/object-store
opt-ins, checks no redirect/cookie/login-state on outage, verifies discovery
and JWKS requests occur after recovery, and restores the exact OIDC snapshot plus
test-scoped login state on completion or panic. The new target compiles with
`cargo test --no-run`, passes all-target/all-feature Clippy, and is rustfmt
clean. Its ignored real-service run passed 1/1 against the loopback services
in Compose project `rustshare-test` (`rustshare_test` database on port 15432,
RustFS on port 19000, `rustshare-test` bucket), using both explicit disposable
opt-ins. Two initial runs correctly cleaned up after exposing test-fixture
defects (the mock lacked a JWKS endpoint); the final run asserted one
well-known discovery request on each attempt and exactly one JWKS fetch after
recovery. Before/after checks confirmed the OIDC singleton
row's exact JSON hash was unchanged, zero login-state rows remained, and there
were no other active database sessions. The test must run alone (not
concurrently with `admin_config_oidc_test`) because the singleton lock is
process-local. This is local integration evidence, not evidence from a
revision-bound hosted/pilot run.

Hosted candidate prerequisite recheck (2026-10-05): the checked-out branch is
`codex/pre-pilot-lifecycle-gate`, exactly PR #337's approved head
`c5a82d830e0d890b28d82fc43f374869b9fd296d`; the worktree has 41 dirty paths.
`workflow_dispatch` runs against a remote commit, not these local edits (and
skips the push-only publish job). A revision-bound run therefore requires a
reviewable pushed candidate. No commit or push was made to this already-
approved PR branch; preserving its review state and the other dirty work is
required before choosing how to publish a candidate.

- [ ] Phases 0–6 completed, or each remaining child issue has a bounded and explicitly accepted deferral.
- [ ] Gates A, B, and C have evidence appropriate to the evaluation stage; no unresolved critical authorization/data-loss blocker is waived implicitly.
- [ ] The authoritative pilot workflow proves deploy → authenticate → canonical journey → restart/persistence → backup/restore → upgrade → re-verification on one exact candidate revision.
- [ ] Clean-install and operator runbook have been validated by someone other than the primary developer, using only documented prerequisites and steps.
- [ ] Final report includes exact revision, environment, workflow identity, results, known limitations, owners/workarounds for accepted risks, and one of READY, READY WITH ACCEPTED LIMITATIONS, or NOT READY.

Offboarding failure-injection regression (2026-10-05): an independent safety
review confirmed the ignored `admin_user_disable_transaction_test` writes only
UUID-scoped fixtures, a UUID-named trigger/function, and one generated object
key; cleanup does not drop a database or bucket. To avoid the developer/app
database, a new `rustshare_test` database was created in the Docker Compose
project labeled `rustshare-test` (separate named PostgreSQL/RustFS volumes and
loopback ports 15432/19000). Credentials were checked for equality with those
containers without printing them. All repository migrations applied to that
database, then the guarded ignored test ran against that database and the
dedicated `rustshare-test-admin-disable` bucket. The real PostgreSQL/RustFS-
backed test passed (1/1); follow-up database checks found zero generated
tenant, trigger, or function fixtures. The non-ignored test binary also passed
7 disposable-target/harness guard tests (1 live test remains intentionally
ignored by default). `cargo fmt --all --check`, focused all-feature Clippy with
warnings denied, and `git diff --check` passed; Clippy reported only the
existing future-incompatibility notice for `proc-macro-error2`. The test's
storage delete returned success, but an independent bucket listing was
unavailable because the AWS CLI is not installed. The isolated database and
test bucket are intentionally left in place; bucket contents were not
independently enumerated. No production/pilot host was touched. This ran
against the dirty worktree based on source SHA
`c5a82d830e0d890b28d82fc43f374869b9fd296d`, not an immutable pushed revision,
and is local regression evidence only—not pilot workflow or institutional
authorization acceptance.

Rust workspace validation (2026-10-05): `cargo fmt --all --check` and
`SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features --
-D warnings` passed (Clippy reports the existing `proc-macro-error2`
future-incompatibility notice). The first workspace library-test invocation
omitted `DATABASE_URL`; seven database-backed tests therefore failed to resolve
the default `postgres` hostname, while 541 tests passed. This was an invocation
configuration failure, not a green result. After setting `DATABASE_URL` to the
migrated `rustshare_test` database in the dedicated local test PostgreSQL
service, `SQLX_OFFLINE=true cargo test --workspace --all-features --lib`
passed: 1,257 passed, 0 failed, 58 ignored across the workspace's library
targets. This verifies local unit/library coverage only; ignored integration
tests and the authoritative hosted workflow remain separate gates.

`cargo sqlx prepare --workspace --check` also exited 0 against the migrated
disposable database. SQLx emitted a non-failing warning that potentially
unused query metadata exists in `.sqlx`; this check did not rewrite metadata.
`SQLX_OFFLINE=true cargo build --workspace --release --all-features` then
completed successfully; the release profile enables LTO, so the initial build
took over twenty minutes. An incremental confirmation exited 0 in 0.53s. The
only build notice was the existing future-incompatibility warning for
`proc-macro-error2`.

Existing integration workflow local verification (2026-10-05): applied all
repository migrations to a fresh `rustshare_test_integration` database in the
dedicated local test PostgreSQL service and ran the workflow's existing
serialized command,
`cargo test --workspace --all-features -j 1 -- --ignored --test-threads=1`,
with RustFS pointed at the loopback test service and a new
`rustshare-test-integration` bucket. The command exited 0. The context RPC
timed out at five minutes while Cargo was still compiling/running; process
inspection confirmed the same Cargo process continued through the ignored
calendar, contract, mail, and storage targets and exited successfully. It was
not restarted. The workflow's explicit Ask Workspace recording-provider
authorization matrix also passed twice (18/18 on each run). This is local
workflow-command evidence on the dirty worktree; it is not a GitHub Actions
run and does not bind evidence to an immutable revision.

Frontend baseline (2026-10-05): `npm run check` passed with 0 errors and 75
warnings; `npm run lint` passed with 0 errors and 159 warnings;
`npm run test` passed 108 files / 1,189 tests; `npm run build` completed
successfully. Test output includes expected error-path logging, but no test
failed. The Playwright `npm run test:e2e` command was not run against the
already-running local deployment; the full-stack browser journey remains a
separate runtime validation requirement.

Existing workflow disposable-target alignment (2026-10-05): review after the
local ignored-suite pass found that the authoritative integration job's old
environment (`DATABASE_URL` database `rustshare`, bucket `rustshare-files`, no
disposable-target acknowledgements) would correctly refuse the new offboarding
test. The existing job—not a parallel workflow—now uses its already-ephemeral
`rustshare_test` database and matching readiness probe, a
`rustshare-test-integration` bucket via both supported bucket variables, and
the two explicit disposable-service acknowledgements. The local serialized
workflow sweep passed with the same guard/bucket semantics and a prefixed
disposable database. YAML parsed, all six embedded job shell blocks passed
`bash -n`, environment assertions and `git diff --check` passed. `actionlint`
is not installed. A hosted workflow run is still required to verify this
changed workflow on a pushed candidate revision. A reviewer confirmed the
workflow values and test guards align; the two acknowledgement flags are
scoped only to the existing ignored-integration-test step.

CI-aligned offboarding retest (2026-10-05): after moving the disposable
database/object-store acknowledgements from job-wide environment into only the
existing ignored-integration-test step, YAML/environment assertions confirmed
the DB name, health probe, database URL, and both bucket variables align.
Running the ignored offboarding test against the exact `rustshare_test` DB
name and `rustshare-test-integration` bucket passed (1/1); its non-ignored
guards passed again (7/7, with the integration test intentionally ignored in
the default invocation). This verifies the stricter CI preflight values
locally, but not the hosted workflow execution.

Independent safety review of this workflow change (2026-10-05): conditional
GO for the configured CI targets. The offboarding test accepts the explicit
`rustshare_test` database, loopback S3 endpoint, and
`rustshare-test-integration` bucket, and requires both disposable
acknowledgements. The reviewer confirmed it does not drop the database or
bucket; cleanup is limited to UUID-scoped rows, its UUID-named
trigger/function, and one generated object key. The review also noted that
the test guard validates the database name and local hostname, not the
PostgreSQL port, server identity,
or Compose project label. CI safety therefore relies on the workflow's
explicit GitHub-hosted PostgreSQL service mapping and isolated RustFS process;
the local Compose target was separately checked before the destructive
integration run. This is not proof of the hosted workflow, whose changed
revision still requires execution.

OIDC outage-test CI inclusion review (2026-10-05): an independent review
confirmed `backend/tests/oidc_provider_failure_test.rs` is declared as a
workspace integration target and is included automatically by the existing
`cargo test --workspace --all-features -j 1 -- --ignored --test-threads=1`
step. The test therefore runs sequentially with the other singleton-mutating
OIDC integration tests; its disposable opt-ins, loopback PostgreSQL database,
loopback RustFS endpoint, and `rustshare-test-integration` bucket match the
workflow configuration. No parallel validation workflow or duplicate test
step is needed. The focused OIDC test passed locally, but the full serialized
sweep has not been rerun since this test was added, and there is no hosted
run for the current dirty worktree at the time of this review. The ordinary
`rustshare` Compose services were not used; a later isolated local sweep is
recorded below.

OIDC test safety follow-up (2026-10-05): independent review found that the
test's local RustFS preflight did not restrict the port, unlike the shared
test harness. It now accepts only ports 9000 and 19000; pure tests cover both
approved ports and reject unapproved/implicit ports. The focused guard tests
pass (2/2), and the test target passes all-target/all-feature Clippy with
warnings denied. This does not alter application behavior.

Local serialized integration workflow sweep (2026-10-05): the first attempt
failed during the first database-backed test's object-store setup. The
test's `dotenvy::dotenv()` loaded ignored `backend/.env`, whose
`S3_ENDPOINT=http://localhost:9000` took precedence over the intended isolated
`RUSTFS_ENDPOINT`; this was the separate local Buzz RustFS service, not the
`rustshare-test` service. A signed read-only S3 probe verified the isolated
RustFS endpoint and bucket were healthy. The failing route-level test
reproduced with only `RUSTFS_*` endpoint variables and passed 1/1 after
explicitly setting `S3_ENDPOINT` and `S3_REGION` to the isolated test service.
The existing workspace sweep was then rerun, unchanged, with both S3/RustFS
endpoint aliases pointed at `127.0.0.1:19000`, the dedicated
`rustshare-test` PostgreSQL service and `rustshare_test` database (102
migrations), `rustshare-test-integration` bucket, and the required disposable
opt-ins. The exact command
`cargo test --workspace --all-features -j 1 -- --ignored --test-threads=1`
exited 0; the new OIDC outage/recovery test passed 1/1 inside that sweep. The
post-run database check confirmed zero other sessions and zero OIDC login-state
rows. CI needs no separate test step: GitHub's fresh checkout does not contain
the ignored local `backend/.env`, and the job's existing `RUSTFS_ENDPOINT`
therefore remains authoritative. This is local evidence at HEAD
`c5a82d830e0d890b28d82fc43f374869b9fd296d` with 67 dirty paths, not a hosted
run or evidence bound to an immutable candidate revision.

Workflow environment parity follow-up (2026-10-05): the existing
`integration-tests` job now also sets `S3_ENDPOINT` and `S3_REGION` to the
same loopback service/region as its `RUSTFS_*` aliases. This makes the test
harness's preferred `S3_*` variables explicit. The
workflow parses as YAML, endpoint/region parity checks and `git diff --check`
pass; a hosted run on the changed workflow revision is still required. The
agent testing guide now documents the serialized CI command, disposable
database/bucket requirements, and `backend/.env` S3 alias precedence.

Audit pagination follow-up (2026-10-05): `GET /api/v1/admin/audit` now checks
offset multiplication before database access, returns a documented 400 for
overflow, and skips its row-fetch SELECT when the safe offset is beyond the
counted result set. Pagination helper tests and formatting pass; the guarded
real PostgreSQL/RustFS route regression passes 1/1, covering the largest
representable page-aligned offset (empty page) and the next overflowing page
(400). Formatting and server all-target/all-feature Clippy with warnings
denied pass. An independent source review confirmed the boundary arithmetic and
identified that the route assertion alone does not prove the SELECT was
skipped (the response would also be empty without that optimization); treat
the skip as source-reviewed, not independently instrumented test evidence.
The review also caught the missing OpenAPI 400 response; follow-up review
broadened its description to cover all invalid query parameters, including
filter combinations. These are local results from the dirty worktree at base HEAD
`c5a82d830e0d890b28d82fc43f374869b9fd296d`, not a hosted workflow run or
revision-bound pilot evidence.

Admin audit authorization follow-up (2026-10-05): the existing guarded
real-route redaction/pagination regression now also checks the authorization
boundary: anonymous requests receive 401, an active non-admin receives 403,
disabled-admin bearer access receives 401, a persisted disabled-admin cookie
session now receives 401 during authentication, and an active admin still
receives 200. The cookie contract now matches bearer authentication: disabled
credentials are rejected before the admin-role check. The combined
PostgreSQL/RustFS regression passed 1/1; the default target passed 7/7 guard
tests (the real-service test remains ignored by default), server all-target
Clippy passed with warnings denied, and formatting/diff checks passed. A
container-side post-test query confirmed zero `audit_boundary_` users and
associated sessions remain.
An earlier full serialized workspace sweep used the existing workflow command
against the same isolated services, but the Context Mode RPC timed out at 300
seconds before returning Cargo's exit code. The same Cargo process was
confirmed live after timeout and later terminated; captured target summaries
included passing audit and calendar tests, but the final exit code was not
captured, so that attempt is not counted as a green suite. The later tracked
rerun and final exit are recorded below. No hosted run binds these local edits
to a candidate revision.

SCIM credential-revocation follow-up (2026-10-05): source review found that
SCIM v1/v2 directly update `users.disabled_at`, unlike the transactional
administrator disable handler, leaving existing sessions and device tokens
able to revive after re-enable. `resolve_user_session` now checks that the
account exists and is enabled before touching or returning a persisted cookie
session; disabled/missing-user sessions are deleted and database errors fail
closed. Forward-only migration
`backend/migrations/20261003090000_revoke_credentials_on_user_disable.sql`
revokes all browser sessions and active device tokens atomically on every
enabled-to-disabled database transition and cleans up credentials for already
disabled users at migration time. The ignored real-PostgreSQL regression
`user_disable_credential_revocation_test` exercises `/api/v1/me`, the
disable/re-enable transition, credential revocation, stale-cookie rejection,
and cleanup against the dedicated loopback Postgres/RustFS test services. The
migration was applied only to `rustshare_test`; the regression passed 1/1, the
existing admin lifecycle regression passed (6 passed, 0 failed), the server
library suite passed (560 passed, 9 ignored), and server all-target/all-feature
Clippy passed with warnings denied. The subsequent full serialized workspace
rerun also completed with exit code 0 (841 passed, 0 failed); it additionally
exercised the audit status contract and a corrected Kanban test fixture that
had caused a rerun collision. Final `cargo fmt --all --check` and
`git diff --check` pass. These are local uncommitted results only: the current
candidate has not been rebuilt, deployed, or run through the authoritative
hosted workflow, and the overall readiness conclusion remains NOT READY.

Disposable-target guard follow-up (2026-10-05): independent review found the
admin-audit regression's database and bucket guards accepted any name sharing
the test prefix, including production-like `rustshare_test_prod` and
`rustshare-test-prod`. This test now accepts only the exact database and
bucket names configured by the authoritative integration workflow
(`rustshare_test` and `rustshare-test-integration`); loopback endpoint checks
and explicit disposable-service opt-ins remain required. Boundary regression,
the complete test target (8 passed, 1 ignored), target Clippy with warnings
denied, formatting, and `git diff --check` pass. No external service was
accessed by these checks.

Kanban integration-fixture cleanup follow-up (2026-10-05): independent
review found that the slug lookup integration test left the board's
`events.jsonl` object behind and skipped database cleanup on failed
assertions. The test now captures the board journey result, then cleans up
its uniquely named user before asserting the journey result. Cleanup collects
file and version object keys, removes only the user's metadata, and deletes an
object only after reference checks under the shared blob lock; it verifies
removed objects are absent. Review confirmed the Kanban event file is
represented by the user's file metadata and included in this cleanup path.
Formatting, diff checks, and
`SQLX_OFFLINE=true cargo test -p rustshare-server --test kanban_test --no-run`
pass. The ignored PostgreSQL/RustFS integration test was compiled but not run;
this is not runtime object-store cleanup evidence.

Restore-drill concurrency follow-up (2026-10-05): independent safety review
found a TOCTOU race: simultaneous drills could both pass the Docker resource
preflight, then reuse or remove the same Compose volumes during restore and
cleanup. `run-restore-drill.sh` now acquires a non-blocking OS lock keyed by
`DRILL_PROJECT_NAME` before preflight and holds it through report/cleanup;
lock release is automatic when the process exits, so an old lock file does not
block retries. The existing resource-collision preflight remains active. The
shell safety regression proves contention fails before any Docker call and a
subsequent invocation can acquire the released lock. `bash -n`, the regression,
and `git diff --check` pass. No real Docker services were run, so restore
recovery evidence remains outstanding.

Self-service security-event and share-log privacy follow-up (2026-10-05): the
existing authenticated user's security-event endpoint included arbitrary
user-agent text in session-revocation descriptions and returned stored user
agents. Share-access logs also returned user-agent and share-session identity
and subject fields that the UI does not use. Session revocation now records a
fixed description and the revoked session's ID (not the caller's session); the
security-event response normalizes historical revocation descriptions and
omits user-agent values. Share-log responses keep the actor label and IP used
by the UI while returning null for user-agent and share-session identity and
subject. A guarded real-route regression covers an old and newly created
revocation event, cross-user isolation, owner/non-owner share-log access, and
absence of sentinel metadata. It passed against the dedicated local
PostgreSQL/RustFS services (1/1 ignored integration test); the regular target
passed 8 tests with 1 ignored, server test-target Clippy, formatting, and diff
checks passed. This is uncommitted local evidence, not a hosted revision-bound
result; organization-scoped audit acceptance remains open.

OIDC response/diagnostic redaction follow-up (2026-10-05): review found public
OIDC routes reflected raw provider/configuration errors and callback
`error_description` values. Runtime-config database/missing-row/decryption,
issuer validation, HTTP-client initialization, provider discovery, callback,
token exchange, ID-token validation, user provisioning, session persistence,
JWT generation, and cookie serialization now use generic public errors with
stage-only server diagnostics. Provider rejection bodies/descriptions are not
reflected or logged. The ignored loopback-provider test checks upstream
discovery, callback-description, and token-response sentinels, successful
recovery, and corrupted stored ciphertext; failures set no cookies or
redirects. The focused ignored PostgreSQL/RustFS route test passed 1/1; OIDC
runtime library tests passed 2/2; the regular integration target passed 9
tests with 1 ignored; server all-target/all-feature Clippy, formatting, and
diff checks passed. Independent source review found no raw response/log
disclosure in the reviewed OIDC paths. Mobile exchange and injected SQLx/JWT/
cookie failure paths were source-reviewed but are not dynamically exercised;
logs were not captured dynamically. A fresh full serialized workspace sweep was attempted afterward,
but its Context Mode RPC timed out after 300 seconds; process inspection
confirmed Cargo continued through ignored integration targets and then exited,
but no exit code or captured final summary was available. That sweep is not
counted as passing evidence. These are local results from the dirty worktree,
not a hosted revision-bound run.
