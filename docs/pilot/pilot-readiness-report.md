# RustShare Pilot Readiness Report

## Conclusion: NOT READY

## Latest exact-candidate workflow update (2026-10-06)

- Candidate source SHA: `1b4aeb18c9578f225e732ff44225ea6df54a874a`.
- [Pilot Release run 37461268485](https://github.com/kubedoio/rustshare/actions/runs/37461268485)
  completed successfully on this exact SHA (12:09:29–12:47:20 UTC). The
  Integration Tests and every executed PR check passed on this revision,
  including Chat Product Acceptance and CodeQL. Conditional Test, Build
  Release, SQLx Prepare, and Code Coverage jobs were skipped by their
  workflows.
- The clean-install identity records `SOURCE_STATUS=` (empty),
  `SOURCE_SHA=1b4aeb18c9578f225e732ff44225ea6df54a874a`, build
  `pilot-1b4aeb18c957`, deployment `github-actions-37461268485-1`, and config
  fingerprint
  `631d58a0f7584c4ccb7e7f3eba5bcf9b49dca7f0fd9dbd9a2a07893d8d15e19f`.
  The tested image ID is
  `sha256:55acfc99d78b3341c6f882a4b1d5145aaef7b2e51c276ec9ea64338eecbac14e`;
  its preserved archive SHA-256 is
  `1b06f12ee9bc1dae69759920d6ac379ef69dc6520d528b1e7c57d87c9aa3e6c5`.
- The workflow passed clean install and migrations, the canonical user/File/
  Note journey, authenticated browser UI and Notes rename/H1 regression,
  restart/persistence, migration-failure diagnostics, database and storage
  failure/recovery, invalid-configuration rejection, backup and isolated
  restore/re-verification, and previous-release upgrade plus representative
  migration checks. Persistence reports explicitly record
  `BETA_SMOKE_PERSISTENCE_STATE_VERIFIED=passed`; Playwright reports
  `expected=1`, `skipped=0`, `unexpected=0`, `flaky=0`. The upgrade source was
  `v0.8.0-alpha.5` (`d28816cbedaa877026cdd8bcb57274c54694e40a`). Cookie,
  secret-log, migration-diagnostic, and backup-output secret checks passed.
  The canonical smoke also records user lifecycle, shared-File access, and
  share cleanup as passed; the hosted Integration Tests log records
  `disabling_user_revokes_credentials_and_stale_cookie_cannot_authenticate ...
  ok` on this exact SHA.
- Evidence artifact
  `rustshare-pilot-evidence-1b4aeb18c9578f225e732ff44225ea6df54a874a`
  (29,415 bytes; artifact ID `11413759734`) and tested-image artifact
  `rustshare-pilot-tested-image-1b4aeb18c9578f225e732ff44225ea6df54a874a`
  (44,343,457 bytes; artifact ID `11414049393`) are retained on the workflow
  run. `WORKFLOW_RESULT=passed` and evidence collection passed.
- This proves the isolated GitHub Actions deployment only. It is not an FWS
  target-environment acceptance: candidate deployment to FWS, the two-admin/
  second-operator recovery rehearsal, and institutional authorization design
  approvals remain outstanding. Conclusion remains **NOT READY**.

## Prior exact-candidate workflow update (2026-10-06; superseded by SHA `1b4aeb1`)

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
latest completed CI run; do not reinterpret historical PASS entries as a
combined acceptance of this revision.

The previously deployed FWS candidate passed the repository Pilot Release
gate, public load-balanced journey, clean deployment, restart/persistence,
backup/restore, bounded dependency failure/recovery, and supported upgrade
checks recorded below. An earlier isolated CI candidate,
`c5a82d830e0d890b28d82fc43f374869b9fd296d`, passed its then-committed Pilot
Release workflow and produced account-lifecycle evidence, but was not deployed
to FWS. The current candidate's workflow completed its upgrade and evidence
phases successfully, but it has not been accepted in the FWS target
environment. The second-operator lifecycle and recovery rehearsal also
remain outstanding; the pilot therefore remains NOT READY under the
participant-start contract.

The previous **READY WITH ACCEPTED LIMITATIONS** conclusion applied to the
earlier bounded password-login baseline. It does not carry forward as approval
to start users under the expanded contract. The updated workflow has now
passed for an exact candidate; do not begin the cohort until FWS acceptance
and the operator's second-admin recovery procedure are verified. This does
not authorize Bund expansion or imply OIDC-provider acceptance.

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
- Earlier isolated CI candidate evidence (superseded by the run recorded above):
  - Source SHA: `c5a82d830e0d890b28d82fc43f374869b9fd296d`
  - [Pilot Release workflow_dispatch run 37220445574](https://github.com/kubedoio/rustshare/actions/runs/37220445574):
    success, 2026-10-04 17:25:18–18:07:05 UTC.
  - Machine artifact:
    `rustshare-pilot-evidence-c5a82d830e0d890b28d82fc43f374869b9fd296d`
    (140,715 bytes; expires 2027-01-02 17:25:20 UTC). It records the exact
    run/source identity, `WORKFLOW_RESULT=passed`, canonical journey,
    account-lifecycle result, and restart/persistence success. It came from
    the committed workflow before subsequent evidence-gate changes:
    it has no `clean-install.env` or `tested-image.env`, and the
    `workflow_dispatch` run skipped the push-only image publication job. It is
    isolated CI evidence, not an FWS deployment or validation of the current
    worktree.

The machine-generated host identity is
`/var/backups/rustshare/fws-evidence-20261004/pilot-identity.env`. It contains
no passwords, tokens, cookies, private keys, or database credentials.

## Gate status

These PASS results are component evidence from different environments and
revisions, not a combined FWS acceptance. FWS operational checks identify
source SHA `c3648fdb918ac2b7ff59952bc91e26cce66d5e70`; the latest isolated CI
run identifies candidate `1b4aeb18c9578f225e732ff44225ea6df54a874a` and passes
the current Pilot Release gates. That candidate has not been deployed to FWS.

| Gate | Result | Evidence |
| --- | --- | --- |
| Deploy/configure | PASS | Exact image/source/config identity is recorded; the public load balancer terminates TLS and forwards to the private host. |
| Clean install | PASS | Fresh host directory, `.env.example`, `scripts/pre-flight.sh`, durable admin password before first start, fresh Postgres/RustFS volumes, canonical smoke and cleanup. Evidence: `fws-clean-install.env`. |
| Authenticate/authorize | PASS | `fws-canonical-final.env`: admin and viewer login, protected file access, viewer denial, sharing and audit assertions. |
| Institutional organization/group/workspace authorization | NOT ACCEPTED | Phase 0 decisions remain open; ADR-0038, [`0030-elembra-application-model.md`](../adr/0030-elembra-application-model.md), [`0032-resource-refs-and-authorization.md`](../adr/0032-resource-refs-and-authorization.md), and [`0037-calendar-application-and-external-sync.md`](../adr/0037-calendar-application-and-external-sync.md) are Proposed. Existing group administration and membership are not proven tenant-safe; see the [#333 plan](../plans/2026-10-04-issue-333-institutional-readiness-plan.md). |
| Canonical product journey | PASS | `fws-canonical-final.env`: UI reachability, folders, Files upload/download, Notes create/read/update, search, sharing/revocation and logout. |
| Authenticated UI/browser hydration | PASS in isolated CI; FWS acceptance pending | Pilot Release run 37461268485 passed the authenticated Playwright sign-in, Files, and Notes journey on candidate SHA `1b4aeb18c9578f225e732ff44225ea6df54a874a`; the Notes name/H1 independence and reload checks passed. The result has `expected=1`, `skipped=0`, `unexpected=0`, `flaky=0`. The FWS target itself has not been browser-validated on this candidate. |
| Restart/persistence | PASS | `fws-restart-final.env` and `fws-clean-restart-persistence.env`: application stop/start followed by re-authentication and Note/File verification. |
| Backup | PASS | Bundle `/var/backups/rustshare/20261004T000917Z`; structural verification passed for PostgreSQL, RustFS, configuration, manifest and SHA-256 checksums. |
| Restore | PASS | `fws-restore-persistence.env` and restore-drill report: isolated Compose restore followed by representative data verification. |
| Upgrade | PASS for supported path | The authoritative Pilot Release workflow upgrades `v0.8.0-alpha.5` to the candidate. The FWS candidate also verified Note/File data after replacement and 102 migrations through `20261002100000`. |
| Account lifecycle | PARTIAL; FWS/operator rehearsal pending | The canonical smoke in run 37461268485 records `BETA_SMOKE_USER_LIFECYCLE=passed`; Integration Tests run 37461229546 on the same SHA ran `disabling_user_revokes_credentials_and_stale_cookie_cannot_authenticate` successfully. This revision contains the SCIM credential-revocation correction, but it is not deployed to FWS and no independent operator has rehearsed the lifecycle or two-admin recovery there. |
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
| [#333](https://github.com/kubedoio/rustshare/issues/333) | PARTIAL | Pilot Release run 37461268485 now provides exact-SHA CI evidence for clean install, canonical journey, UI, restart/persistence, failures/recovery, backup/restore, and upgrade; the matching image and evidence artifacts are retained. Still missing are FWS acceptance of this candidate, staged 10–15/30–50/~100-user evidence, independent clean-install/runbook and two-admin recovery rehearsal, and Phase 0 product/security decisions. |

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
8. Earlier isolated Pilot Release results on SHAs `83bd70406f9f…` and
   `c5a82d830e0d…` established account-lifecycle and operational evidence.
9. The latest exact-candidate Pilot Release run 37461268485 passed on SHA
   `1b4aeb18c9578f225e732ff44225ea6df54a874a`, including component-specific
   PostgreSQL/RustFS readiness diagnostics, readiness after recovery, invalid
   configuration, migration-failure diagnostics, redacted evidence collection,
   and the supported upgrade. Its artifacts bind the source, image, deployment,
   configuration, and workflow identities. These CI results remain separate
   from the currently deployed FWS SHA
   `c3648fdb918ac2b7ff59952bc91e26cce66d5e70`.

## Unresolved limitations and follow-up

1. **Medium — candidate deployment and operator lifecycle rehearsal pending.**
   Affected operations: account setup, password recovery/change, offboarding,
   and recovery from administrator lockout. Evidence: exact candidate SHA
   `1b4aeb18c9578f225e732ff44225ea6df54a874a` passed Pilot Release run
   37461268485, but FWS still runs the earlier tested SHA
   `c3648fdb918ac2b7ff59952bc91e26cce66d5e70`; a second operator has not yet
   validated the documented clean-install/runbook procedure or rehearsed the
   lifecycle and two-admin recovery procedure. PR #337 remains open and
   `REVIEW_REQUIRED`; issue #333 has no approval comments.
   Workaround: do not start the cohort; retain two independent administrators
   and use supervised lifecycle procedures for rehearsal only. Follow-up:
   obtain required human review and product/security decisions, deploy the
   reviewed exact candidate using the documented operator procedure, have an
   independent operator validate clean install and the runbook, and verify two
   independent admin accounts plus create/reset/change/disable behavior before
   cohort start. The `fws-pilot` protected GitHub Environment is not configured;
   deployment remains an explicit operator action.
2. **High — SCIM offboarding can leave credentials usable after re-enable.**
   Affected operations: SCIM account deactivation, browser-session access, and
   device-token access after reactivation. Evidence: the deployed FWS candidate
   predates the correction. The SCIM v1/v2 credential-revocation correction is
   now in candidate SHA `1b4aeb18c9578f225e732ff44225ea6df54a874a`; the exact
   SHA Integration Tests run 37461229546 passed
   `disabling_user_revokes_credentials_and_stale_cookie_cannot_authenticate`.
   It has not been deployed to FWS or independently reviewed/accepted.
   Workaround: use the existing administrator disable path, which transactionally
   revokes sessions and device tokens; do not use SCIM deactivation as the sole
   access-revocation control on deployed FWS. Follow-up: obtain human review,
   deploy the reviewed exact revision, verify migration `20261003090000`, and
   test session and device-token revocation on FWS before cohort start.
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
    tests used the then-current worktree based
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
8. **Medium — direct File-share behavior on FWS remains unverified.** Affected
   operation: a pilot user downloading another user's shared File and losing
   access when its share is revoked. Evidence: candidate SHA
   `1b4aeb18c9578f225e732ff44225ea6df54a874a` passed the canonical smoke's
   `BETA_SMOKE_FILE_SHARE_ACCESS` and `BETA_SMOKE_FILE_SHARE_CLEANUP` markers;
   the workflow checks pre-share denial, byte-identical download while shared,
   and denial after revocation. This proves the isolated CI deployment, not
   the currently deployed FWS revision. Workaround: do not rely on cross-user
   File sharing for FWS pilot data until the candidate is deployed and checked.
   Follow-up: after review, deploy the exact candidate and verify share access
   and revocation on FWS.
9. **Medium — authenticated Notes/Files UI behavior on FWS remains unverified.**
   Affected operation: signing in, opening a File, editing a Note's Markdown
   H1, renaming the Note independently, and reloading. Evidence: the exact
   candidate's Pilot Release run 37461268485 passed the authenticated Playwright
   flow with one expected test, zero skipped/unexpected/flaky tests, including
   the Notes name/H1 regression and reload. FWS has not been deployed to this
   revision or exercised in that browser flow. Workaround: none for claiming
   target-environment behavior. Follow-up: after review and deployment, run the
   authenticated browser journey against FWS and retain its report.
10. **Low — concurrent application root-path changes may leave extra folders.**
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
`1b4aeb18c9578f225e732ff44225ea6df54a874a` passed Pilot Release run
37461268485, but is not deployed to FWS. The required human/Phase 0 decisions,
FWS acceptance, staged scale evidence, and second-operator/two-admin recovery
rehearsal remain incomplete. Reassess after human review, those decisions, exact
candidate deployment, and operator rehearsal. Do not begin Bund-specific
expansion under this milestone.
