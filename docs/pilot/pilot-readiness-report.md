# RustShare Pilot Readiness Report

## Conclusion: NOT READY

## Latest exact-candidate workflow update (2026-10-06)

- Candidate source SHA: `82b47d04608b242b074e2a5a5f4d21590158d76a`.
- [Integration Tests run 37455378778](https://github.com/kubedoio/rustshare/actions/runs/37455378778):
  success. Its log explicitly records
  `user_disable_credential_revocation_test.rs` running the real-service
  `disabling_user_revokes_credentials_and_stale_cookie_cannot_authenticate`
  test, with 1 passed and 0 failed.
- [Pilot Release run 37455425225](https://github.com/kubedoio/rustshare/actions/runs/37455425225):
  completed successfully. Clean deployment, canonical journey,
  UI reachability, application restart and persistence, bounded dependency
  failure/recovery, invalid-configuration rejection, backup verification, and
  isolated restore/re-verification have passed. The previous-release-to-
  candidate upgrade passed (11:38:52–11:58:42 UTC). Representative migration
  verification and evidence collection passed; the run completed successfully.
  The artifact binds build `pilot-82b47d04608b`, image
  `sha256:d157b6a39faeedc434aab17301c55b7af28106d788db55d773d9af3758a459c6`,
  deployment `github-actions-37455425225-1`, and config fingerprint
  `88198f4e2ec00e6c3414dc354c995c9508ef62294f638ccc9e7d972e79af0bbe`.
  `pilot-workflow-summary.env` records `WORKFLOW_RESULT=passed`;
  `ui-playwright-results.json` has `expected=1`, `skipped=0`,
  `unexpected=0`, `flaky=0`; the representative migration passed; and the
  upgrade used
  `v0.8.0-alpha.5` (`d28816cbedaa877026cdd8bcb57274c54694e40a`).
- The artifact also records `SOURCE_STATUS=?? scripts/__pycache__/`, created by
  the Python redaction test before source identity capture. This is a
  checkout-cleanliness evidence defect: a green result must not claim a clean
  source tree in this state. The workflow now disables Python bytecode and
  fails closed on a dirty checkout; rerun on the corrected revision is
  required before accepting pristine-source evidence. Artifact:
  `rustshare-pilot-evidence-82b47d04608b242b074e2a5a5f4d21590158d76a`
  (29,786 bytes; run 37455425225).
- This is isolated GitHub Actions evidence only. SHA `82b47d…` has not been
  deployed to FWS, and the two-administrator/second-operator recovery
  rehearsal remains outstanding. Institutional authorization design remains
  unapproved and unimplemented. Therefore the conclusion remains **NOT READY**.

The gate table and risk register below retain the historical FWS and earlier
candidate evidence they cite. Read the exact-candidate update above for the
current in-progress CI run; do not reinterpret historical PASS entries as a
combined acceptance of this revision.

The previously deployed FWS candidate passed the repository Pilot Release
gate, public load-balanced journey, clean deployment, restart/persistence,
backup/restore, bounded dependency failure/recovery, and supported upgrade
checks recorded below. An earlier isolated CI candidate,
`c5a82d830e0d890b28d82fc43f374869b9fd296d`, passed its then-committed Pilot
Release workflow and produced account-lifecycle evidence, but was not deployed
to FWS. The current candidate's workflow state is recorded above and has not
finished its upgrade/evidence phases. The second-operator lifecycle and
recovery rehearsal also remains outstanding; the pilot therefore remains NOT
READY under the participant-start contract.

The previous **READY WITH ACCEPTED LIMITATIONS** conclusion applied to the
earlier bounded password-login baseline. It does not carry forward as approval
to start users under the expanded contract. Do not begin the cohort until the
updated workflow passes for an exact candidate and the operator verifies the
second-admin recovery procedure. This does not authorize Bund expansion or
imply OIDC-provider acceptance.

## Revision and evidence identity

- Source SHA: `c3648fdb918ac2b7ff59952bc91e26cce66d5e70`
- Candidate build: `rustshare-backend:pilot-c3648fdb918a`
- Candidate image: `sha256:02862e272beadd035471808a13c25a8b05cc4ba13a5f531ecf7d38acca2072e8`
- FWS deployment identity: `fws-app-kubedo-io`
- FWS Compose identity: `3ee7e5b3e790ac48bccd76910107a759c919578975a64781c8b638b4b49793eb`
- Public validation URL: `https://app.kubedo.io`
- Host evidence directory: `/var/backups/rustshare/fws-evidence-20261004`
- Clean-install evidence: `fws-clean-install.env`, config identity
  `4b39c2c0b76d965d0ce0631dbb060503f7ed4d699e32011df54ce5269ae48db3`
- Repository workflow evidence:
  - [Pilot Release run 37156360337](https://github.com/kubedoio/rustshare/actions/runs/37156360337)
  - [Integration Tests run 37154953625](https://github.com/kubedoio/rustshare/actions/runs/37154953625)
  - [final branch checks run 37158465897](https://github.com/kubedoio/rustshare/actions/runs/37158465897)
- Updated isolated CI candidate evidence:
  - Source SHA: `83bd70406f9fb7e2d040a2fe95d35e9cfd0f4de4`
  - Build/version: `pilot-83bd70406f9f`; deployment identity:
    `github-actions-37206208388-1`; configuration identity:
    `3dd95a1417e1ed14ac6347094d34e0b5255696d1b5ba4b5b443c93b0d7bb6cc7`
  - [Pilot Release workflow_dispatch run 37206208388](https://github.com/kubedoio/rustshare/actions/runs/37206208388):
    success, 2026-10-04 13:36:21–14:17:18 UTC.
  - Machine artifact:
    `rustshare-pilot-evidence-83bd70406f9fb7e2d040a2fe95d35e9cfd0f4de4`
    (140,603 bytes; GitHub expiry 2027-01-02 13:36:18 UTC). It records
    `BETA_SMOKE_USER_LIFECYCLE=passed`, successful restart/persistence,
    backup/restore, previous-release upgrade, migrations, bounded failure
    recovery, and security-sanity markers. The artifact's evidence-collection
    and secret-log-scan markers passed.
- Latest exact isolated CI candidate evidence:
  - Source SHA: `c5a82d830e0d890b28d82fc43f374869b9fd296d`
  - [Pilot Release workflow_dispatch run 37220445574](https://github.com/kubedoio/rustshare/actions/runs/37220445574):
    success, 2026-10-04 17:25:18–18:07:05 UTC.
  - Machine artifact:
    `rustshare-pilot-evidence-c5a82d830e0d890b28d82fc43f374869b9fd296d`
    (140,715 bytes; expires 2027-01-02 17:25:20 UTC). It records the exact
    run/source identity, `WORKFLOW_RESULT=passed`, canonical journey,
    account-lifecycle result, and restart/persistence success. It came from
    the committed workflow before current uncommitted evidence-gate changes:
    it has no `clean-install.env` or `tested-image.env`, and the
    `workflow_dispatch` run skipped the push-only image publication job. It is
    isolated CI evidence, not an FWS deployment or validation of the current
    worktree.

The machine-generated host identity is
`/var/backups/rustshare/fws-evidence-20261004/pilot-identity.env`. It contains
no passwords, tokens, cookies, private keys, or database credentials.

## Gate status

These PASS results are component evidence from different environments and
revisions, not a combined pass for one current candidate. The FWS operational
checks identify source SHA `c3648fdb918ac2b7ff59952bc91e26cce66d5e70`; the
newer isolated CI run identifies `c5a82d830e0d890b28d82fc43f374869b9fd296d`
and covers the committed workflow before the current local gate changes. No
single revision has yet passed the complete current gate and been deployed to
FWS.

| Gate | Result | Evidence |
| --- | --- | --- |
| Deploy/configure | PASS | Exact image/source/config identity is recorded; the public load balancer terminates TLS and forwards to the private host. |
| Clean install | PASS | Fresh host directory, `.env.example`, `scripts/pre-flight.sh`, durable admin password before first start, fresh Postgres/RustFS volumes, canonical smoke and cleanup. Evidence: `fws-clean-install.env`. |
| Authenticate/authorize | PASS | `fws-canonical-final.env`: admin and viewer login, protected file access, viewer denial, sharing and audit assertions. |
| Institutional organization/group/workspace authorization | NOT ACCEPTED | Phase 0 decisions remain open; ADR-0038, [`0030-elembra-application-model.md`](../adr/0030-elembra-application-model.md), [`0032-resource-refs-and-authorization.md`](../adr/0032-resource-refs-and-authorization.md), and [`0037-calendar-application-and-external-sync.md`](../adr/0037-calendar-application-and-external-sync.md) are Proposed. Existing group administration and membership are not proven tenant-safe; see the [#333 plan](../plans/2026-10-04-issue-333-institutional-readiness-plan.md). |
| Canonical product journey | PASS | `fws-canonical-final.env`: UI reachability, folders, Files upload/download, Notes create/read/update, search, sharing/revocation and logout. |
| Authenticated UI/browser hydration | NOT VERIFIED for current candidate | Historical FWS evidence checked UI reachability only. The current worktree adds an isolated Playwright sign-in/Files/Notes journey to the authoritative workflow. A fail-closed JSON verifier requires the exact canonical test in `tests/pilot.e2e.ts` to actually pass and rejects skipped, expected-failure, flaky, extra, missing, or malformed results; local regressions pass, but the browser journey has not run against a live deployment or authoritative CI revision. |
| Restart/persistence | PASS | `fws-restart-final.env` and `fws-clean-restart-persistence.env`: application stop/start followed by re-authentication and Note/File verification. |
| Backup | PASS | Bundle `/var/backups/rustshare/20261004T000917Z`; structural verification passed for PostgreSQL, RustFS, configuration, manifest and SHA-256 checksums. |
| Restore | PASS | `fws-restore-persistence.env` and restore-drill report: isolated Compose restore followed by representative data verification. |
| Upgrade | PASS for supported path | The authoritative Pilot Release workflow upgrades `v0.8.0-alpha.5` to the candidate. The FWS candidate also verified Note/File data after replacement and 102 migrations through `20261002100000`. |
| Account lifecycle | PARTIAL; operator rehearsal pending | Runs 37206208388 and 37220445574 record `BETA_SMOKE_USER_LIFECYCLE=passed`: create, admin reset, session revocation, old-password denial, self-service password change, disable/offboarding, denial, and cleanup. Source review then found that SCIM v1/v2 disable paths on the deployed candidate did not revoke existing web sessions or device tokens, so those credentials could revive after re-enable. A local migration/session-check correction passes a guarded PostgreSQL regression but is uncommitted and not deployed or hosted-workflow verified. This does not prove FWS lifecycle behavior or two-admin recovery rehearsal. |
| Health/diagnostics | PASS | `/health` remained 200 while PostgreSQL was stopped; `/health/ready` returned 503 with `database connectivity failed`, then recovered. RustFS failure returned 503 with `object storage check failed`, then recovered. |
| Observability | PASS | Startup/migration/dependency logs, liveness/readiness component diagnostics, smoke phase reports and redacted failure evidence are retained. |
| Security sanity | PASS for bounded pilot | Secure cookies, public HTTPS origin, protected routes and secret-redaction checks passed. OIDC provider acceptance is intentionally outside the password-login pilot. |

## Institutional acceptance by issue

| Issue | State | Evidence and remaining acceptance |
| --- | --- | --- |
| [#330](https://github.com/kubedoio/rustshare/issues/330) | BLOCKED | ADR-0038 remains Proposed; tenant-safe organization, group, workspace, role, and revocation semantics are neither approved nor implemented. |
| [#332](https://github.com/kubedoio/rustshare/issues/332) | PARTIAL | Local guarded PostgreSQL/RustFS route coverage proves anonymous, non-admin, disabled-admin bearer, and disabled-admin cookie-session callers cannot read the admin audit route while an active admin can; audit-detail allowlisting and template-path minimization are also tested. A separate self-service regression verifies revoked-session metadata redaction and owner-scoped share-log projection. This does not prove organization-scoped coverage, tenant-isolated query/pagination, retention, or safe export, and is not revision-bound hosted evidence. |
| [#327](https://github.com/kubedoio/rustshare/issues/327) | PARTIAL | Current evidence is per-user Calendar API/import-export work; the institutional calendar journey and visibility model remain undecided, and FWS browser evidence does not cover that journey. |
| [#328](https://github.com/kubedoio/rustshare/issues/328) | NOT STARTED | No verified event-to-agenda/Notes/document/decision/action workflow with source reauthorization, revocation, and stale-reference behavior. |
| [#329](https://github.com/kubedoio/rustshare/issues/329) | PARTIAL | Local import/export tests cover selected recurrence cases; non-UTC recurring masters and detached overrides remain unsupported, and client interoperability/feed revocation are not accepted. |
| [#331](https://github.com/kubedoio/rustshare/issues/331) | PARTIAL | Single-user lifecycle is exercised; bounded bulk onboarding preview/retry, group/workspace assignment, and organization-wide offboarding are not implemented. The deployed candidate also has an SCIM credential-revocation gap; a local correction is not yet revision-bound. |
| [#333](https://github.com/kubedoio/rustshare/issues/333) | PARTIAL | Operational proof spans different revisions; current local workflow changes lack an authoritative run, there is no staged 10–15/30–50/~100-user evidence, and independent clean-install/runbook rehearsal is outstanding. Local changes serialize restore drills sharing a Compose project and sanitize OIDC runtime-config/discovery/callback/token failures; the lock regression and focused PostgreSQL/RustFS OIDC sentinel, recovery, and corrupted-secret checks pass, but the restore drill has not been exercised against real deployment services and these changes have no revision-bound hosted run. |

## Evidence sequence

1. The candidate was deployed from the exact source SHA and served through
   the existing load balancer; TLS terminates at the load balancer and the
   private host serves HTTP on its bound interface.
2. The existing `scripts/run-beta-smoke.sh` passed against the public FWS
   URL.
3. A separate fresh-volume deployment was created from the documented
   bootstrap procedure and passed the same smoke plus restart persistence.
4. A real PostgreSQL/RustFS/configuration backup was created and structurally
   verified.
5. The backup was restored into an isolated Compose project and the
   representative pilot data was verified.
6. The supported previous-release upgrade was exercised by the authoritative
   workflow; the target candidate’s migration and post-upgrade persistence
   were also verified on the host.
7. PostgreSQL and RustFS outage/recovery probes demonstrated the distinction
   between liveness and dependency readiness.
8. An earlier updated Pilot Release run passed for candidate SHA
    `83bd70406f9fb7e2d040a2fe95d35e9cfd0f4de4`; its uploaded artifact records
    the account-lifecycle result and exact workflow/deployment/configuration
    identity. The later committed-workflow run 37220445574 tested candidate
    `c5a82d830e0d890b28d82fc43f374869b9fd296d` and is the latest isolated CI
    evidence listed above. Both CI results are separate from the currently
    deployed FWS SHA `c3648fdb918ac2b7ff59952bc91e26cce66d5e70`.
9. The current uncommitted workflow additionally asserts component-specific
   PostgreSQL/RustFS readiness diagnostics and readiness after each recovery,
   the exact weak-JWT configuration diagnostic, and visible startup failure for
   a real SQL migration privilege error. Static YAML/shell validation and
   synthetic dependency-diagnostic fixtures pass, but no authoritative run has
   exercised these local changes.

## Unresolved limitations and follow-up

1. **Medium — candidate deployment and operator lifecycle rehearsal pending.**
   Affected operations: account setup, password recovery/change, offboarding,
   and recovery from administrator lockout. Evidence: isolated candidate SHA
   `83bd70406f9fb7e2d040a2fe95d35e9cfd0f4de4` passed the lifecycle workflow
   phase in run 37206208388, but FWS still runs the earlier tested SHA
   `c3648fdb918ac2b7ff59952bc91e26cce66d5e70`; a second operator has not yet
   validated the documented clean-install/runbook procedure or rehearsed the
   lifecycle and two-admin recovery procedure.
   Workaround: do not start the cohort; retain two independent administrators
   and use supervised lifecycle procedures for rehearsal only. GitHub currently
   reports PR #337 as APPROVED with its required checks successful, but it
   remains open and unmerged. Follow-up: deploy the approved exact candidate using
   the documented operator procedure, have an independent operator validate
   clean install and the runbook, and verify two independent admin accounts
   plus create/reset/change/disable behavior before cohort start. The
   `fws-pilot` protected GitHub Environment is not configured; deployment
   remains an explicit operator action.
2. **High — SCIM offboarding can leave credentials usable after re-enable.**
   Affected operations: SCIM account deactivation, browser-session access, and
   device-token access after reactivation. Evidence: the FWS candidate predates
   the local correction; SCIM v1/v2 update `users.disabled_at` directly, while
   cookie resolution previously returned an unexpired session without checking
   account status. Local fix evidence is the guarded
   `user_disable_credential_revocation_test` (1/1) plus the existing admin
   lifecycle integration suite (6 passed), against dedicated loopback
   PostgreSQL/RustFS. The new migration and resolver change remain uncommitted;
   no hosted run or FWS deployment proves them. Workaround: use the existing
   administrator disable path, which transactionally revokes sessions and
   device tokens; do not use SCIM deactivation as the sole access-revocation
   control on the deployed candidate. Follow-up: merge/deploy a reviewed exact
   revision, apply and verify migration `20261003090000`, and rerun the
   revision-bound lifecycle and recovery workflow before cohort start.
3. **Medium — password-login pilot baseline.** Affected operation: OIDC
   authentication. Evidence: password login passed; no FWS OIDC provider was
   configured for this run. Workaround: use the documented password-login
   accounts for the bounded pilot. Follow-up: run OIDC acceptance when the
   provider, redirect URI and client credentials are provisioned.
4. **Medium — loss of all administrator credentials.** A retained database
   required a controlled password-hash rotation because its existing admin
   credential did not match the copied first-boot `.env`; changing the
   first-boot `.env` does not reset an existing account. For ordinary user
   recovery, Admin → Users can set a new password; the handler revokes the
   target user's sessions and device tokens and records an admin action
   (`backend/server/src/handlers/admin/users.rs`). Workaround: preserve
   durable credentials before first start and keep a separately secured
   second administrator. Follow-up: rehearse the admin reset/offboarding path
   with a disposable account and verify the two-admin recovery procedure
   before cohort start. If every administrator credential is lost, database
   recovery remains an exceptional, separately approved incident action.
5. **Low — unsupported old image behavior.** The retained host image
   `rustshare-backend:latest` stopped its full smoke during search, but it is
   not the supported previous release. The supported `v0.8.0-alpha.5` upgrade
   path passed in the authoritative workflow. Follow-up: remove or label the
   stale image so operators do not select it as an upgrade source.
6. **Medium — recurring iCalendar export is intentionally limited.** Affected
    operation: exporting recurring Calendar events to another calendar client.
    Evidence: the new owner-scoped single-event export returns `409` for
    non-UTC recurring masters and detached recurrence overrides until it can
    serialize the necessary timezone definition and recurrence identity. The
    Calendar API integration target passed 24/24 against a fresh disposable
    PostgreSQL/RustFS test environment, including the owner/enablement boundary;
    the import integration target passed 10/10 and exercises real import-to-
    export UID and recurrence behavior plus event-level reporting of listed
    unsupported fields and duplicate/ranged `RECURRENCE-ID` rejection. These
    tests use the current uncommitted worktree based
    on HEAD `c5a82d830e0d890b28d82fc43f374869b9fd296d`; they are not evidence of
    a deployed FWS revision or independent third-party client interoperability.
    Workaround: export standalone events or UTC recurring masters; for provider-
    mirrored events, download from the source provider. There is no supported
    export workaround for non-UTC internal recurrence or detached overrides.
    Follow-up: implement and test `VTIMEZONE` and `RECURRENCE-ID` semantics under
    issue #329 before claiming full recurring-event interoperability.
7. **High — institutional organization/group/workspace authorization is not
   approved or implemented.** Affected operations: institutional onboarding,
   group and workspace administration, inherited access explanation and
   revocation, shared-calendar visibility, and organization-wide offboarding.
   Evidence: Phase 0 in the [#333 plan](../plans/2026-10-04-issue-333-institutional-readiness-plan.md)
   remains open; ADR-0038,
   [`0030-elembra-application-model.md`](../adr/0030-elembra-application-model.md),
   [`0032-resource-refs-and-authorization.md`](../adr/0032-resource-refs-and-authorization.md),
   and [`0037-calendar-application-and-external-sync.md`](../adr/0037-calendar-application-and-external-sync.md)
   are Proposed. The current group implementation uses global-admin checks and
   group-ID lookups without proving tenant-safe membership; no production data
   inventory has been run. Workaround: there is no safe institutional-cohort
   workaround; restrict any separately approved evaluation to existing
   per-user behavior without claiming organization or group isolation.
   Follow-up: obtain product/security decisions, run the reviewed read-only
   inventory against the pilot data, then implement and verify tenant-scoped
   administration, inherited access, audit, and revocation before cohort use.
8. **Medium — direct File-share access and revocation have not yet been
   re-proven by the candidate workflow.** Affected operation: a pilot user
   downloading a File shared by another user, and losing access after the
   share is revoked. Evidence: the prior canonical smoke checked only that the
   recipient saw the share listing. The uncommitted workflow now checks
   pre-share denial, byte-identical download while shared, and denial after
   revocation; the pilot workflow/redaction regressions pass 31/31, UI-result
   verifier tests pass 8/8, and all 35 embedded Bash blocks pass syntax checks
   locally, but no authoritative Docker/workflow run has exercised
   these assertions. Workaround: do not rely on File sharing for pilot data
   until the updated workflow passes; use access that has been separately
   verified. Follow-up: run the authoritative Pilot Release workflow on the
   exact candidate and retain its artifact before enabling shared-File use.
9. **Medium — browser sign-in and authenticated Notes/Files UI behavior are not
   yet proven for the current candidate.** Affected operation: a pilot user
   signing in, opening a File, editing a Note's Markdown H1 and renaming the
   Note independently. Evidence: the historical UI check fetched the SPA HTML
   only. The uncommitted Pilot Release change now opens the canonical File and
   smoke-created Note in Playwright, checks H1/name independence through UI
    edits, reloads to verify both, and restores the Note fixture before later
    persistence checks. Test discovery lists 1 test. The Playwright JSON gate
    tests pass 8/8, including skips, expected failures, flaky/extra/wrong tests,
    and missing/malformed reports; the workflow regression verifies validation
    precedes success evidence. Existing pilot workflow/redaction regressions
    pass 31/31. Targeted Notes route tests pass 4/4, UI-result verifier tests
    pass 8/8, workflow YAML parses, and all 35 embedded shell blocks pass
    syntax checks. The full frontend suite passes 108
   files/1,189 tests, Svelte check has zero errors, frontend lint exits 0, and
   the production build passes; the browser flow has not run against the
   candidate. Workaround: none for claiming browser behavior; do not infer it
   from API-only smoke. Follow-up: run the authoritative workflow and retain
   the Playwright JSON report and UI identity evidence before cohort start.
10. **High — current diagnostic-artifact sanitization is not revision-verified.**
   Affected operation: collecting and retaining failure/upgrade diagnostics
   without exposing credentials. Evidence: the current uncommitted workflow
   keeps raw upgrade logs outside the evidence tree, redacts them before
   evidence placement, allows upload only after successful evidence collection,
   no detected secret, and a clean artifact scan, and now ensures a summary-
   validation failure is marked `WORKFLOW_RESULT=failed` before safe artifact
   upload; the finalizer rescans the completed evidence tree including that
    summary. The workflow/redaction regression suite passes 31/31 locally; the
   prior committed workflow wrote upgrade logs directly into the artifact
   directory, and no authoritative
   run has exercised the fix. Workaround: do not retain or distribute
   artifacts from runs using the prior workflow for diagnostic review.
    Follow-up: run the updated Pilot Release workflow on the exact committed
    candidate and verify its uploaded evidence artifact and scan result.
11. **Low — concurrent application root-path changes may leave extra folders.**
    Affected operation: changing an application's configured root path. Evidence:
    the admin update route provisions the requested folder before its config
    transaction; concurrent requests can each create a root folder even though
    only the last committed path remains configured. The configured path is
    provisioned by its own successful request; no missing-root or data-loss case
    was demonstrated. The enable route separately rejects a root-path change
    between provisioning and its locked update, and its guarded regression
    passed 1/1 locally. Workaround: serialize root-path changes. Do not remove
    old folders automatically because they may contain user data. Follow-up:
    define whether old roots are retained as user content and establish a
    reference-safe cleanup policy before automating removal.

## Final decision

**NOT READY** — the institutional child-issue acceptance remains incomplete,
organization/workspace authorization is unapproved and unimplemented, and the
latest exact isolated candidate SHA
`c5a82d830e0d890b28d82fc43f374869b9fd296d` passed the committed Pilot Release
workflow, but is not deployed to FWS. Its run predates local evidence-gate
changes, and the second-operator/two-admin recovery rehearsal has not passed.
Reassess after human review, an authoritative run of the updated gate, exact
candidate deployment, and operator rehearsal. Do not begin Bund-specific
expansion under this milestone.
