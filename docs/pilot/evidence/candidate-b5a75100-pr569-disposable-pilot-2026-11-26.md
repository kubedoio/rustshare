# PR #569 100-user disposable pilot evidence

Date: 2026-11-26

## Candidate identity

- PR head: `b5a751000f5b8b1d15996db2dff7a01ee70be55e`
- Pilot Release run: [45839820086](https://github.com/kubedoio/rustshare/actions/runs/45839820086), attempt 1 — **passed**.
- Workflow result: `passed`; readiness: `NOT_READY_UNTIL_TARGET_ENVIRONMENT_ACCEPTANCE`.
- Tested backend image ID: `sha256:44cdd954de30ed5348b88119be983c0cec8a48b597be31d80dba0ae5af8eb61a`.
- Compose config ID: `8df66555c5f95505fd79ad0d0f442cae4f325378a12f96d140c063d9f66baaa2`.
- Evidence artifact: [15130473739](https://github.com/kubedoio/rustshare/actions/runs/45839820086/artifacts/15130473739), `sha256:1056d78deabf51ea4b0db8ab6ffdea3344670bf38a5c68c4221f8e2acf39b4c9`.
- Tested-image transfer artifact: [15129934300](https://github.com/kubedoio/rustshare/actions/runs/45839820086/artifacts/15129934300), `sha256:2c64009a1414bf1abd16b9c8b653d1164408a06663b92f36bf2f77407e315017`; contained image archive SHA-256 `6cd0c63cd25dcc21ebe665922548dda9c9498edc3cdd77f8147a30163281de84`.
- Image publication was skipped. There is no registry digest for this candidate.

## Qualification results

- Clean PostgreSQL and RustFS volumes; all 17 migrations applied, latest `20261123190000`; first canonical journey passed.
- All 5 Playwright tests passed with 0 skipped, unexpected, or flaky results:
  - 100-user synthetic organization administration.
  - Files and independent Note-name/H1 editing.
  - Exact historical Memory citation without changing Note identity.
  - Calendar import/export and idempotent re-import.
  - Institutional Calendar context and denial for unavailable linked content.
- Runtime database posture passed as `pilot_runtime`; negative checks rejected privileged attributes, ownership/role assumptions, and schema creation.
- Restart/persistence, dependency failure/recovery, invalid-configuration diagnostics, backup secret scan, and evidence collection passed.
- Backup restored into fresh volumes; installed upgrade from the immutable `v0.8.0-alpha.7` image and fresh-volume rollback both passed.

The preceding run [45831217098](https://github.com/kubedoio/rustshare/actions/runs/45831217098) completed the journeys and recovery checks but failed the workflow summary because the calendar-import marker was written twice. The duplicate writer was removed; focused verifier and redaction tests passed before this successful rerun.

## Qualification boundary

This is synthetic, disposable **PR-head** evidence, not a protected-main release qualification or FWS deployment. Protected main is now `d0fc70c0594f709223e3ff83423cc38340f03dc6`; its CI, CodeQL, integration, security-compliance, and non-publishing Pilot Release workflows are queued. PR #569's remaining exact-head checks and independent review are also pending. FWS remains **NO-GO** until operator-owned DNS, HTTPS, authentication, secret custody, backup destination/restore, monitoring, and authorized-user gates are completed. No institutional data, real users, publication, or deployment was involved.
