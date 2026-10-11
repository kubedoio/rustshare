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
- This proves the isolated GitHub Actions deployment only. FWS deployment and
  the public canonical journey plus backend restart/persistence now have
  separate target-host evidence below. The independent two-admin recovery
  rehearsal and institutional authorization design approvals remain
  outstanding. Conclusion remains **NOT READY**.

## FWS target-host update (2026-10-06)

- PR #337 merged to `main` as `e30274ffe9b4962a85dcd7ada8503665b305ff20`.
  FWS runs the exact tested candidate archive from source SHA
  `1b4aeb18c9578f225e732ff44225ea6df54a874a`, version
  `pilot-1b4aeb18c957`, from Pilot Release run 37461268485. Its archive SHA-256
  is `1b06f12ee9bc1dae69759920d6ac379ef69dc6520d528b1e7c57d87c9aa3e6c5`.
- A fresh PostgreSQL/RustFS/configuration backup at
  `/var/backups/rustshare/20261006T163355Z` passed the repository verifier.
- The existing beta smoke passed against `https://app.kubedo.io`, including
  admin/viewer authentication, Notes, Files, search, File share access and
  revocation denial, audit activity, and logout. After restarting the FWS
  backend, its persistence mode verified the Note body/H1/title and File
  checksum. Raw host reports are retained outside Git under
  `/var/backups/rustshare/fws-evidence-20261006`; the redacted summary is
  [`FWS deployment evidence`](evidence/fws-deployment-2026-10-06.md).
- The current conclusion remains **NOT READY** pending a genuinely independent
  second-admin recovery rehearsal. The first post-restart outbox probe was
  unhealthy, then ten later probes spanning more than the 60-second freshness
  window were healthy; no consumer deliveries were present to verify. Chat is
  disabled and outbox-backed projections remain outside this pilot. Broader
  institutional #333 work remains gated on proposed ADR/product decisions.

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

The earlier FWS identity and pre-October-6 artifacts retained below are
historical context only. Current FWS deployment, workflow, persistence, and
backup evidence is summarized above and in the dated target-host evidence
record; old PASS entries must not be treated as acceptance of the current
revision.

The previous **READY WITH ACCEPTED LIMITATIONS** conclusion applied to the
earlier bounded password-login baseline. It does not carry forward as approval
to start users under the expanded contract. The exact tested candidate is
deployed to FWS, but do not begin the cohort until the independent operator
rehearsal and remaining mandatory acceptance are recorded. This does not
authorize Bund expansion or imply OIDC-provider acceptance.

## Historical FWS deployment identity (superseded 2026-10-06)

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

The evidence combines separate environments: authoritative Pilot Release run
37461268485 on source SHA
`1b4aeb18c9578f225e732ff44225ea6df54a874a` passed. Merge-triggered run
37496000263 for `e30274ffe9b4962a85dcd7ada8503665b305ff20` passed its
`pilot-compose-smoke` job, including the upgrade test, but failed its separate
image-publication job at Trivy; image publication was skipped. That failure is
not treated as a successful security scan. The exact tested candidate was
deployed to FWS; target-host evidence is recorded below and in
[`evidence/fws-deployment-2026-10-06.md`](evidence/fws-deployment-2026-10-06.md).

| Gate | Result | Evidence |
| --- | --- | --- |
| Deploy/configure | PASS | Exact image/source/config identity is recorded; the public load balancer terminates TLS and forwards to the private host. |
| Clean install | PASS in isolated CI; FWS fresh install not run | Pilot Release run 37461268485 passed the clean-install workflow. FWS is a retained deployment, not a fresh-install proof. |
| Authenticate/authorize | PASS | The 2026-10-06 FWS canonical smoke: admin and viewer login, protected File denial, sharing/revocation and audit assertions; see the dated evidence record. |
| Institutional organization/group/workspace authorization | NOT ACCEPTED | Phase 0 decisions remain open; ADR-0038, [`0030-elembra-application-model.md`](../adr/0030-elembra-application-model.md), [`0032-resource-refs-and-authorization.md`](../adr/0032-resource-refs-and-authorization.md), and [`0037-calendar-application-and-external-sync.md`](../adr/0037-calendar-application-and-external-sync.md) are Proposed. Existing group administration and membership are not proven tenant-safe; see the [#333 plan](../plans/2026-10-04-issue-333-institutional-readiness-plan.md). |
| Canonical product journey | PASS | The 2026-10-06 FWS canonical smoke: UI reachability, Files upload/download, Notes create/read/update, search, sharing/revocation and logout. |
| Authenticated UI/browser hydration | PASS in isolated CI and on FWS | Pilot Release run 37461268485 passed the authenticated Playwright sign-in, Files, and Notes journey on candidate SHA `1b4aeb18c9578f225e732ff44225ea6df54a874a`. The target-host browser run on 2026-10-06 also passed one expected test with zero skipped/unexpected/flaky tests; it verified the deployed candidate's Files view and Note name/H1 independent edits, reload persistence, and restoration of the fixture. See [`fws-browser-2026-10-06.md`](evidence/fws-browser-2026-10-06.md) and its Playwright JSON report. |
| Restart/persistence | PASS in CI and on FWS | The canonical FWS smoke persisted representative Note/File data, backend restart completed, and report-driven smoke reauthenticated and verified both records; machine reports are retained in the FWS evidence bundle. |
| Backup | PASS | Fresh FWS bundle `/var/backups/rustshare/20261006T163355Z`; structural verification passed for PostgreSQL, RustFS, configuration, manifest and SHA-256 checksums. |
| Restore | PASS in isolated CI; FWS restore not run | Pilot Release run 37461268485 passed isolated Compose restore and verification. An in-place FWS restore was not run. |
| Upgrade | PASS in isolated CI and merge-run smoke | Pilot Release run 37461268485 and the `pilot-compose-smoke` job in merge-triggered run 37496000263 validated upgrade from `v0.8.0-alpha.5`. No FWS upgrade was performed after deploying this candidate. |
| Account lifecycle | PARTIAL; independent FWS rehearsal pending | The canonical smoke in run 37461268485 records `BETA_SMOKE_USER_LIFECYCLE=passed`; Integration Tests run 37461229546 on the same SHA passed `disabling_user_revokes_credentials_and_stale_cookie_cannot_authenticate`. The candidate containing this correction is deployed to FWS, but no independent operator has rehearsed the lifecycle or two-admin recovery there. |
| Health/diagnostics | PASS | `/health` remained 200 while PostgreSQL was stopped; `/health/ready` returned 503 with `database connectivity failed`, then recovered. RustFS failure returned 503 with `object storage check failed`, then recovered. |
| Observability | PASS | Startup/migration/dependency logs, liveness/readiness component diagnostics, smoke phase reports and redacted failure evidence are retained. |
| Security sanity | PARTIAL | Secure cookies, public HTTPS origin, protected routes and secret-redaction checks passed in the exact candidate workflow. The merge-image Trivy failure below remains unresolved; OIDC provider acceptance is intentionally outside the password-login pilot. |
| Image vulnerability scan/publication | NOT ACCEPTED | Merge run 37496000263 failed `Scan image with Trivy`; the GHCR push was skipped. Its uploaded Trivy analysis contains 44 findings (2 high, 32 medium, 10 low, no critical reported), while an independent Trivy 0.70.0 scan of that run's exact image archive with `--severity CRITICAL` returned zero findings. The mismatch is unexplained; neither result is accepted as a passing revision-bound gate until reproduced and reconciled. See limitation 11. |

## Institutional acceptance by issue

| Issue | State | Evidence and remaining acceptance |
| --- | --- | --- |
| [#330](https://github.com/kubedoio/rustshare/issues/330) | BLOCKED | ADR-0038 remains Proposed; tenant-safe organization, group, workspace, role, and revocation semantics are neither approved nor implemented. |
| [#332](https://github.com/kubedoio/rustshare/issues/332) | PARTIAL | Local guarded PostgreSQL/RustFS route coverage proves anonymous, non-admin, disabled-admin bearer, and disabled-admin cookie-session callers cannot read the admin audit route while an active admin can; audit-detail allowlisting and template-path minimization are also tested. A separate self-service regression verifies revoked-session metadata redaction and owner-scoped share-log projection. This does not prove organization-scoped coverage, tenant-isolated query/pagination, retention, or safe export, and is not revision-bound hosted evidence. |
| [#327](https://github.com/kubedoio/rustshare/issues/327) | PARTIAL | Current evidence is per-user Calendar API/import-export work; the institutional calendar journey and visibility model remain undecided, and FWS browser evidence does not cover that journey. |
| [#328](https://github.com/kubedoio/rustshare/issues/328) | NOT STARTED | No verified event-to-agenda/Notes/document/decision/action workflow with source reauthorization, revocation, and stale-reference behavior. |
| [#329](https://github.com/kubedoio/rustshare/issues/329) | PARTIAL | Local import/export tests cover selected recurrence cases; non-UTC recurring masters and detached overrides remain unsupported, and client interoperability/feed revocation are not accepted. |
| [#331](https://github.com/kubedoio/rustshare/issues/331) | PARTIAL | Single-user lifecycle is exercised; bounded bulk onboarding preview/retry, group/workspace assignment, and organization-wide offboarding are not implemented. The candidate contains the SCIM credential-revocation correction and its targeted integration test passed, but no disposable SCIM-managed user has been tested on FWS. |
| [#333](https://github.com/kubedoio/rustshare/issues/333) | PARTIAL | Pilot Release run 37461268485 provides exact-SHA CI evidence for clean install, canonical journey, UI, restart/persistence, failures/recovery, backup/restore, and upgrade; the exact tested image is now deployed to FWS and the public canonical journey plus restart persistence passed there. Still missing are the independent two-admin recovery/runbook rehearsal, staged 10–15/30–50/~100-user evidence, and Phase 0 product/security decisions. |

## Evidence sequence

1. Exact candidate source SHA `1b4aeb18c9578f225e732ff44225ea6df54a874a`
   was deployed to FWS from the preserved tested image archive; TLS continues
   to terminate at the existing load balancer and nginx binds only the private
   host interface.
2. The existing `scripts/run-beta-smoke.sh` passed against the public FWS URL,
   exercising password login, Notes, Files, sharing/revocation and logout.
3. The backend was restarted and the same report-driven smoke path verified
   persisted Note title/H1/body and File content hash. This is target-host
   persistence evidence, separate from CI.
4. A fresh PostgreSQL/RustFS/configuration backup was created and passed the
   repository structural/checksum verifier. CI separately passed isolated
   restore and journey re-verification; an in-place FWS restore was not run.
5. The authoritative Pilot Release run 37461268485 passed on the exact source
   SHA, including fresh install, browser UI, restart/persistence, bounded
   failure/recovery, invalid configuration, backup/restore, supported upgrade,
   migration checks, and evidence collection. Its evidence binds source,
   image, deployment, configuration and workflow identities.
6. The merge-triggered run 37496000263 passed its functional pilot-compose
   smoke, including supported upgrade, but its separate Trivy image-publication
   job failed and did not push an image. The run's archive is SHA-256
   `74443eaf54075bd6b20b8c71ba77bbd7fddee0f84a6f629f27c96d456820d7c2`.
   GitHub's Trivy analysis reports 44 findings but none at critical severity;
   a local Trivy 0.70.0 scan of that exact archive using the critical-only
   threshold also found zero. The exit-1 mismatch is unresolved, so this is a
   failed/unaccepted gate, not a green scan.
7. FWS `/health` and `/health/ready` returned 200; database, object storage,
   auth/session and event delivery were healthy. The optional outbox component
   remains unhealthy after restart and the Chat bridge is disabled; see the
   target-host evidence for the operational boundary.

## Unresolved limitations and follow-up

1. **Medium — independent two-admin recovery rehearsal pending.** Affected
   operations: admin account recovery, password reset/session revocation,
   disable/offboarding and lockout recovery. Evidence: FWS runs the exact
   candidate, the public canonical journey passed, and the database has two
   enabled admin accounts; no independent human has yet performed the second
   admin sign-in/recovery steps. Workaround: do not start the cohort until the
   rehearsal is recorded; keep both admin credentials separately controlled.
   Follow-up: have the named independent operator verify their own admin login,
   reset a disposable user's password, confirm stale-session rejection, disable
   the disposable user and confirm access revocation, and review the runbook.
   The protected `fws-pilot` GitHub Environment remains unconfigured, so deploys
   are explicit operator actions.
 2. **Medium — FWS SCIM lifecycle behavior has not been exercised on this
    revision.**
   Affected operations: SCIM account deactivation, browser-session access, and
    device-token access after reactivation. Evidence: deployed candidate SHA
    `1b4aeb18c9578f225e732ff44225ea6df54a874a` includes the v1/v2
    credential-revocation correction; exact-SHA Integration Tests run
    37461229546 passed
    `disabling_user_revokes_credentials_and_stale_cookie_cannot_authenticate`.
    This SCIM flow has not been exercised against a disposable FWS-managed user.
   Workaround: use the existing administrator disable path, which transactionally
   revokes sessions and device tokens; do not use SCIM deactivation as the sole
    access-revocation control on FWS. Follow-up: only if SCIM is in pilot scope,
    test session and device-token revocation on a disposable managed account
    before enabling SCIM offboarding for pilot users.
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
 8. **Medium — dispatcher completed a late initial tick; event delivery remains
    unverified.** Affected operations: delayed integration projections and
    outbox-backed Chat flows. Evidence: the first post-restart probe reported
    `outbox dispatcher has not completed a tick`; later `/health/ready` probes
    were healthy ten times from `17:37:15Z` through `17:39:08Z`, spanning the
     configured 60-second freshness window. A public probe at `18:25:00Z` again
     returned overall `ready` while the `outbox` component was `unhealthy`.
     The point-in-time database check showed 33 outbox rows but zero integration
     delivery rows, so no consumer delivery was exercised; logs did not explain
     the initial delay. The Chat bridge is still disabled by an invalid service
     key. Workaround: the tested Notes/Files/share journey passed; keep Chat and
     delayed projections out of scope. Follow-up: diagnose the unstable health
     signal and verify a real subscribed event delivery before expanding scope.
  9. **Low — concurrent application root-path changes may leave extra folders.**
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
  10. **High — candidate image security gate is not yet revalidated.** Affected
     operation: accepting and publishing the exact candidate image for pilot
     use. Evidence: merge run 37496000263 failed publication. Its SARIF analysis
     records 44 findings (2 high, 32 medium, 10 low, none critical), and the
     exact image archive passes a direct Trivy 0.70.0 CRITICAL-only scan. The
     cause is identified: Trivy Action v0.36.0 removes `TRIVY_SEVERITY` when
     producing all-severity SARIF unless `limit-severities-for-sarif` is enabled
     ([upstream entrypoint](https://github.com/aquasecurity/trivy-action/blob/v0.36.0/entrypoint.sh#L508-L531)).
     Consequently the action's `exit-code: 1` was applied to all findings, not
     just CRITICAL ones. The workflow now separates all-severity SARIF reporting
     from a CRITICAL-only blocking scan, but that correction has not yet passed
     the full GitHub workflow. Workaround: retain the exact tested FWS image and
     do not claim the merge image passed publication/security acceptance.
     Follow-up: pass the corrected full workflow and preserve the resulting
     image/security evidence before replacing or publishing the candidate.
 11. **Medium — failed-job-only publication retry cannot reuse the tested artifact.**
    Affected operation: recovery of a publication-only Pilot Release failure.
    Evidence: run 37496000263 attempt 2 downloaded the immutable image artifact
    from attempt 1, then `Verify and load tested pilot image` failed because it
    required the artifact's `WORKFLOW_RUN_ATTEMPT` to equal the current attempt.
    Trivy and push were skipped; the subsequent SARIF upload correctly failed
    because no report existed. This is fail-closed, not a false green. Workaround:
    preserve the original failure and rerun the complete workflow if another
    revision-bound artifact is required. Follow-up: retain this strict producer
    attempt provenance and document/test the supported full-rerun path; do not
    relax the check merely to make a partial retry pass.

## Final decision

**NOT READY** — the exact tested candidate is deployed to FWS and its public
canonical journey, authenticated FWS browser journey, backup verification,
backend restart and persistence check passed. However, the independent
second-admin recovery rehearsal has not yet been completed, the merge image's
Trivy publication gate failed with an unresolved scan/report mismatch, its
failed-job-only retry did not reach scanning, the initial outbox delay remains
unexplained and event delivery is unverified, staged scale evidence is missing,
and institutional organization/workspace authorization remains unapproved and
unimplemented.
Keep the bounded cohort stopped until the independent rehearsal and explicit
acceptance of the operational limitations are recorded. Keep broader #333
institutional work gated on ADR/product decisions. Do not begin Bund-specific
expansion under this milestone.
