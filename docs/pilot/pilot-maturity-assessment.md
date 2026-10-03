# RustShare Pilot Maturity Assessment

**Assessment basis:** repository revision `748b2fcb517c87331ba3c6491b2afffcfcd618cc`,
captured before the pilot-gate implementation changes in this worktree. The
workflow and script gaps below are the pre-change baseline; subsequent code
changes do not become evidence until the updated workflow runs successfully.
This is a repository assessment, not evidence that the FWS environment has
been exercised.

## PRESENT AND VERIFIED IN THE REPOSITORY

| Area | Evidence | What is present |
| --- | --- | --- |
| Deployment topology | [`docs/DEPLOYMENT.md`](../DEPLOYMENT.md), [`docker-compose.yml`](../../docker-compose.yml) | Nginx, backend, PostgreSQL and RustFS are defined as a repeatable Compose stack. |
| Configuration bootstrap | [`scripts/pre-flight.sh`](../../scripts/pre-flight.sh), [`.env.example`](../../.env.example) | Required secrets are generated/validated and weak defaults are rejected by the server. |
| Liveness/readiness | [`backend/server/src/lib.rs`](../../backend/server/src/lib.rs), [`backend/server/src/handlers/health.rs`](../../backend/server/src/handlers/health.rs), [`docker/nginx.conf`](../../docker/nginx.conf) | `/health` is a lightweight liveness surface; `/health/ready` checks database, object storage, event delivery and auth/session health and returns 503 when required components are unhealthy. |
| Authentication | [`backend/server/src/routes.rs`](../../backend/server/src/routes.rs), [`backend/server/src/handlers/notes.rs`](../../backend/server/src/handlers/notes.rs) | Password login, logout, session cookies and optional OIDC routes exist. |
| Authorization | [`backend/tests/notes_test.rs`](../../backend/tests/notes_test.rs), [`backend/tests/contracts/tenant_isolation_contract.rs`](../../backend/tests/contracts/tenant_isolation_contract.rs), [`scripts/run-beta-smoke.sh`](../../scripts/run-beta-smoke.sh) | Tenant/user authorization and a negative permission-aware search check are covered in code and smoke validation. |
| Files and Notes | [`backend/server/src/handlers/files.rs`](../../backend/server/src/handlers/files.rs), [`backend/server/src/handlers/notes.rs`](../../backend/server/src/handlers/notes.rs), [`scripts/run-beta-smoke.sh`](../../scripts/run-beta-smoke.sh) | Upload/download, Notes create/read/save/rename, sharing and search paths are implemented. |
| Persistence | [`docker-compose.yml`](../../docker-compose.yml), [`backend/server/src/bootstrap.rs`](../../backend/server/src/bootstrap.rs) | PostgreSQL and RustFS use named volumes; migrations run during backend bootstrap. |
| Backup | [`scripts/backup-stack.sh`](../../scripts/backup-stack.sh), [`scripts/verify-backup-bundle.sh`](../../scripts/verify-backup-bundle.sh) | PostgreSQL, RustFS data, configuration and a manifest/checksum bundle can be created and structurally verified. |
| Restore | [`scripts/restore-stack.sh`](../../scripts/restore-stack.sh), [`scripts/run-restore-drill.sh`](../../scripts/run-restore-drill.sh) | Restore and isolated restore-drill procedures exist for the core stack. |
| Upgrade guidance | [`docs/upgrading.md`](../upgrading.md), [`scripts/test-application-migration.sh`](../../scripts/test-application-migration.sh) | Forward-only migration behavior, backup-before-upgrade guidance and representative migration checks exist. |
| Observability | [`backend/server/src/metrics.rs`](../../backend/server/src/metrics.rs), [`docker-compose.monitoring.yml`](../../docker-compose.monitoring.yml) | Structured tracing, request/error metrics, optional protected Prometheus metrics and dependency readiness diagnostics exist. |
| Pilot product smoke | [`scripts/run-beta-smoke.sh`](../../scripts/run-beta-smoke.sh), [`scripts/final-launch-smoke.sh`](../../scripts/final-launch-smoke.sh) | Existing smoke paths exercise login, Files, Notes, search, sharing, audit and logout against a real deployment. |

## PRESENT BUT NOT VERIFIED FOR THE FWS PILOT

| Requirement | Evidence gap |
| --- | --- |
| Clean-install acceptance | [`docs/DEPLOYMENT.md`](../DEPLOYMENT.md) is detailed, but no retained run on a clean FWS host is present in this repository. |
| Exact-revision binding | [`.github/workflows/pilot-release.yml`](../../.github/workflows/pilot-release.yml) builds from the checkout, but the current job does not retain a complete revision/build/config evidence bundle. |
| Full pilot journey | The pre-change workflow waited for readiness and tore the stack down; it did not invoke [`scripts/run-beta-smoke.sh`](../../scripts/run-beta-smoke.sh). |
| Restart/persistence | Compose volumes and application restart behavior exist, but the pre-change pilot workflow had no post-restart data assertion. |
| Backup/restore proof | Restore tooling exists, but no pilot workflow run proves backup, restore and the same pilot data after restoration. |
| Upgrade proof | Migration tests exist, but a previous compatible released image upgraded to the candidate has not been exercised by the pilot workflow. |
| Dependency failure drills | Readiness code has component checks, but the pilot workflow does not intentionally stop database or object storage and verify visible failure/recovery. |
| FWS identity/OIDC | The repository supports password login and optional OIDC, but the target FWS identity configuration and its live login evidence are external. |

## PARTIAL

- The existing pilot workflow has secret generation, Compose validation,
  readiness polling and failure logs, but its green result currently means
  “the stack became ready”, not “the canonical pilot journey and recovery
  gates passed”. Evidence: [`.github/workflows/pilot-release.yml`](../../.github/workflows/pilot-release.yml).
- Backup bundles deliberately exclude deployment secrets. This is correct, but
  the operator must preserve the external secret set and browser-only Chat keys
  separately. Evidence: [`scripts/backup-stack.sh`](../../scripts/backup-stack.sh),
  [`docs/runbooks/customer-alpha.md`](../runbooks/customer-alpha.md).
- Health is intentionally split between Nginx’s static liveness response and
  the backend readiness response. Operators must use `/health/ready` for
  dependency readiness. Evidence: [`docker/nginx.conf`](../../docker/nginx.conf),
  [`backend/server/src/handlers/health.rs`](../../backend/server/src/handlers/health.rs).
- The Notes implementation preserves note identity while saving an edited H1,
  and explicit rename updates the note name/frontmatter. The rename-preserves-H1
  and reload regression is required as a release gate. Evidence:
  [`backend/server/src/services/note_service.rs`](../../backend/server/src/services/note_service.rs),
  [`docs/adr/0029-filename-heading-separation.md`](../adr/0029-filename-heading-separation.md).

## MISSING FROM THE PRE-CHANGE PILOT ACCEPTANCE PATH

- A revision-bound evidence directory containing source SHA, image labels,
  deployment/config identity, phase result and diagnostics.
- A mandatory invocation of the existing product smoke as part of the pilot
  workflow.
- A restart/persistence assertion, backup/restore verification and bounded
  dependency-failure probes in that same workflow.
- A runbook that a second operator can follow from clean install through
  recovery.

These gaps require an actual successful workflow run and a target-environment
exercise before the pilot can be called ready.

## OUT OF SCOPE

- Bund feature expansion.
- High availability, multi-region deployment and zero-downtime upgrades.
- A new test harness or observability platform.
- A complete security audit or formal OIDC-provider certification.
- Unsupported downgrade semantics.
