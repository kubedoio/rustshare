# FWS candidate deployment evidence — 2026-10-06

Status: candidate deployed; canonical journey and post-restart persistence
passed. The independent two-admin rehearsal and bounded pilot start are still
pending.

## Revision and image

- Tested product source: `1b4aeb18c9578f225e732ff44225ea6df54a874a`
- Merge into `main`: PR [#337](https://github.com/kubedoio/rustshare/pull/337),
  merge commit `e30274ffe9b4962a85dcd7ada8503665b305ff20`
- Passing exact-source Pilot Release: run
  [37461268485](https://github.com/kubedoio/rustshare/actions/runs/37461268485)
- Tested-image artifact: `11414049393`
- Build/version: `pilot-1b4aeb18c957`
- Tested archive SHA-256:
  `1b06f12ee9bc1dae69759920d6ac379ef69dc6520d528b1e7c57d87c9aa3e6c5`
- Tested-image metadata recorded CI image/config ID
  `sha256:55acfc99d78b3341c6f882a4b1d5145aaef7b2e51c276ec9ea64338eecbac14e`.
  Docker Engine 29.6.1 on FWS assigned loaded image ID
  `sha256:eb32694d0e470c40fb4433fa43b0d16cc5c942c94a0f943fe0b51e41b4997b18`;
  all 23 rootfs DiffIDs and the source/version labels matched the archive.
  The archive and its checksum are retained in the external FWS evidence
  bundle; do not substitute a rebuild.

## Post-merge image publication/security gate

- Merge-triggered Pilot Release run
  [37496000263](https://github.com/kubedoio/rustshare/actions/runs/37496000263)
  passed `pilot-compose-smoke`, including the supported upgrade test, but the
  separate `publish-pilot-image` job failed at `Scan image with Trivy`; GHCR
  publication was skipped. The FWS deployment above was not replaced.
- The failed run's tested-image artifact is for merge SHA
  `e30274ffe9b4962a85dcd7ada8503665b305ff20`, archive SHA-256
  `74443eaf54075bd6b20b8c71ba77bbd7fddee0f84a6f629f27c96d456820d7c2`.
  GitHub's uploaded Trivy analysis for that run reports 44 findings: 2 high,
  32 medium and 10 low, with no critical-severity findings in the analysis.
- An independent Trivy 0.70.0 scan of that exact archive with the workflow's
  `CRITICAL` severity threshold exited successfully with zero findings. This
  conflicts with the CI Trivy step's exit code 1. The raw CI SARIF is not
  included in the published run artifacts, and the exit-status/analysis
  mismatch remains unexplained. This does not prove the separately deployed
  pre-merge candidate archive passed image scanning.
- A failed-jobs-only rerun created attempt 2 but did not repeat the scan:
  `Verify and load tested pilot image` rejected the immutable attempt-1 image
  artifact because it requires `WORKFLOW_RUN_ATTEMPT` to equal the current
  attempt. The scan and push were skipped, and SARIF upload then had no file.
  This fail-closed behavior preserves provenance but means a selective retry
  cannot recover a publication-only failure; rerun the full workflow if a new
  tested artifact is needed. Do not loosen the provenance check to make a
  partial retry appear successful.
- Treat image publication/security acceptance as unresolved. Do not replace the
  FWS image with a rebuild or the failed merge-run image until the scanner
  configuration/result mismatch is reproduced and reconciled without
  weakening the gate, and the workflow succeeds with revision-bound evidence.

## FWS deployment and verification

- Environment: FWS host `10.5.199.85`, deployment `fws-app-20261006-rustshare`,
  public origin `https://app.kubedo.io`; TLS terminates at the load balancer.
- Backend runs the source/version above and was healthy after deployment and
  after restart. PostgreSQL recorded 103 successful migrations, latest
  `20261003090000`.
- Pre-change backup: `/var/backups/rustshare/20261006T163355Z`. The repository
  `verify-backup-bundle.sh` passed for PostgreSQL, RustFS, configuration,
  manifest and SHA-256 checksums. Deployment secrets remain in the operator
  secret store, not the backup evidence or this repository.
- `/health` and `/health/ready` returned 200. Database, object storage,
  authentication/session and event-delivery components reported healthy.
  The optional outbox component remains unhealthy with
  `outbox dispatcher has not completed a tick`; overall readiness remains
  ready by contract. Treat delayed outbox projections/Chat as unverified and
  do not depend on them during this pilot. FWS logs also report that the Chat
  bridge is disabled because its configured service key is invalid; Chat is
  outside the Notes/Files pilot journey.
- Canonical smoke ran from an external HTTPS-capable operator runner because
  the FWS host cannot connect outbound to its own public hostname. The first
  host-local attempt is retained as a failed readiness-phase diagnostic; it
  created no pilot data. The external public HTTPS run passed on
  `2026-10-06T16:48:29Z`–`16:48:34Z` using the candidate's existing
  `scripts/run-beta-smoke.sh` (SHA-256
  `aa2474cef9becb77dabf6624eb77d54d3ac3a6707f430e0daabe5a19fcf0a7ee`).
- The canonical run authenticated the configured admin and permitted viewer,
  exercised Files upload/download, Notes create/read/update, permission-aware
  search, internal share access and revocation denial, audit activity and
  logout. Representative data was deliberately preserved.
- Backend restart: `2026-10-06T16:49:49Z`. The same existing smoke script then
  re-authenticated both accounts and verified the persisted Note body, H1 and
  independent note title, plus the downloaded File SHA-256. The machine report
  recorded `BETA_SMOKE_PERSISTENCE_STATE_VERIFIED=passed`.
- Machine reports and the verified backup reference are retained outside Git
  under `/var/backups/rustshare/fws-evidence-20261006` and
  `/var/backups/rustshare/20261006T163355Z`. They contain environment-specific
  test identifiers and must not be copied wholesale into public documentation.

## Remaining gate

- FWS has two enabled administrator accounts, but no independent person has
  yet confirmed and completed the second-admin recovery/session-revocation
  rehearsal. An agent-controlled second login is not independent evidence.
- The bounded FWS cohort has not started. Do not announce the revision as
  READY or admit pilot users until that rehearsal is recorded, the outbox
  limitation is explicitly accepted or resolved for the operations the cohort
  will use, and the merge-image security-gate mismatch is resolved.
- Broader #333 institutional work remains gated by the proposed ADR/product
  decisions; this deployment makes no organization, tenant, calendar or
  institutional authorization claim.
