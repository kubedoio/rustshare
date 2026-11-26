# Protected-main d6bcbc9d4e4e5aebc6449b2b47267b267c3cf2e2 disposable pilot evidence

Date: 2026-11-26

## Candidate identity

- Protected-main SHA: `d6bcbc9d4e4e5aebc6449b2b47267b267c3cf2e2`.
- Non-publishing Pilot Release run: [45865255455](https://github.com/kubedoio/rustshare/actions/runs/45865255455), attempt 1 — **passed**; `publish-pilot-image` was skipped.
- Tested backend image ID: `sha256:6302e6ba087d2e09b012a54b9396bcf7c03c8d4ecbbc80f78d2e34812e959c07`.
- Compose configuration ID: `dc88f875fa794d8c6f8c0188b3ce018c69205f3e45cb7c7761134e539dedc153`.
- Evidence artifact [15141639742](https://github.com/kubedoio/rustshare/actions/runs/45865255455/artifacts/15141639742), digest `sha256:bee5c6528d68b41f85726e70115e4ff0bb4a849c048473b2b7d09a30c5d68f17`.
- Tested-image artifact [15141504777](https://github.com/kubedoio/rustshare/actions/runs/45865255455/artifacts/15141504777), digest `sha256:6dddfba9e89ce0ebe8484b90b6aa73115465776f3336131ef8504540737912a3`; image archive SHA-256 `7594248b04427a5ba488d129c7a321af841c60f22c6149bc82fae824016c47d0`.
- No registry digest was created; this run did not publish the image.

## Qualification results

- Clean install used fresh PostgreSQL and RustFS volumes, applied all 17 migrations through `20261123190000`, and passed the first canonical Files/Notes/user journey.
- Four Playwright journeys passed: expected 4, skipped 0, unexpected 0, flaky 0. Coverage included Calendar import/export/idempotency and unavailable-linked-content denial, Files and independent Note title/H1 editing, and exact historical Memory citation opening.
- Runtime role posture, restart/persistence, dependency failure/recovery, invalid-configuration diagnostics, backup and secret scan, and restore into fresh volumes passed. The runtime database role was `pilot_runtime`; this is not evidence of database RLS.
- Installed upgrade from `v0.8.0-alpha.7`, representative migration validation, and rollback from its pre-upgrade backup to a fresh volume passed.

## Qualification boundary

- This is synthetic disposable evidence for the exact d6 commit, not release-image qualification: no immutable release-image scan, SBOM, provenance, or publication was produced.
- The separate d6 Main Integration Tests run [45865255462](https://github.com/kubedoio/rustshare/actions/runs/45865255462) has passed the integration-tests, Buzz Live Conformance, and Buzz Identity Replacement E2E jobs; Chat Product Acceptance is still in progress as of 2026-11-26 17:57 UTC.
- The 100-user/two-admin journey is not included in this run. PR #569 exact d6-based head `911e936fb74c367143c613e43e65777c70ca773b` has a separate non-publishing Pilot Release [45865650938](https://github.com/kubedoio/rustshare/actions/runs/45865650938) in progress; its results are not yet evidence for this protected-main candidate.
- FWS remains **NO-GO** pending operator-owned DNS, HTTPS, authentication, secret custody, backup destination/restore, monitoring, and authorized-user gates. No institutional data, real users, publication, or deployment was involved.
