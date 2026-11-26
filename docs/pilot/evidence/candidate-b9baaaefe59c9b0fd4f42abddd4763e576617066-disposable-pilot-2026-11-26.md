# Protected-main b9baaaefe59c9b0fd4f42abddd4763e576617066 disposable pilot evidence

Date: 2026-11-26

## Candidate identity

- Protected-main SHA: `b9baaaefe59c9b0fd4f42abddd4763e576617066`.
- Non-publishing Pilot Release run: [45857489674](https://github.com/kubedoio/rustshare/actions/runs/45857489674), attempt 1 — **passed**; image publication job skipped.
- Integration Tests [45857490452](https://github.com/kubedoio/rustshare/actions/runs/45857490452) passed all four jobs. CodeQL [45857490103](https://github.com/kubedoio/rustshare/actions/runs/45857490103), Frontend CI [45857489912](https://github.com/kubedoio/rustshare/actions/runs/45857489912), and Vault Sync Compliance [45857490249](https://github.com/kubedoio/rustshare/actions/runs/45857490249) passed. Main CI [45857490449](https://github.com/kubedoio/rustshare/actions/runs/45857490449) was cancelled when protected main advanced after #588.
- Tested backend image ID: `sha256:888ec1fe347a03c226edf80f19f59bdc63c0c569fbf56f807b64922a181dd5c6`.
- Compose config ID: `7da0b9cd6cbbf39d7fb51c58b2b33699cbeafcea20aaa5c9c8c2f2898cf0e17f`.
- Evidence artifact [15139335912](https://github.com/kubedoio/rustshare/actions/runs/45857489674/artifacts/15139335912), digest `sha256:b4b3eb4e96ebfeff8c1f3ecf94c60ec59706fda85a8ed24c0f274836d39fb7d3`.
- Tested-image transfer artifact [15139540699](https://github.com/kubedoio/rustshare/actions/runs/45857489674/artifacts/15139540699), digest `sha256:a0382a040584c9364145306c421b29d1fcfb4225aa3c4557a6dfc777ffad1182`; image archive SHA-256 `5737b8ef79a4dacce6514e3837281db72b2b52700e4b30d7a4ade8861b313d2b`.
- Image publication was skipped; no registry digest was created.

## Qualification results

- Fresh PostgreSQL and RustFS volumes; all 17 migrations through `20261123190000`; clean install and first canonical journey passed.
- All four browser journeys passed with 0 skipped, unexpected, or flaky results: Calendar import/export/idempotency; denial for unavailable linked Calendar content; Files plus independent Note name/H1 editing; exact historical Memory citation.
- Runtime database checks, restart/persistence, dependency failure/recovery, invalid-configuration diagnostics, backup/secret scan, and isolated restore to fresh volumes passed.
- Installed upgrade from `v0.8.0-alpha.7`, representative migration validation, and fresh-volume rollback passed.

## Qualification boundary

This is synthetic disposable evidence for exact protected-main SHA `b9baaa…`; it is not release-image qualification. The b9 workflow produced neither immutable release-image scans nor SBOM/provenance, and it did not include the 100-user browser journey. PR #569 separately passed the 100-user journey and upgrade/rollback on d0-based head `920a25ca8f021108a0be0f6cbc72adb2b268549a`; that record is [here](candidate-920a25ca-pr569-disposable-pilot-2026-11-26.md). Protected main advanced to `d6bcbc9d4e4e5aebc6449b2b47267b267c3cf2e2` after #588. As of 2026-11-26 17:57 UTC, its push CI is queued, Integration Tests are in progress, non-publishing Pilot Release passed, and CodeQL passed. The d6 pilot evidence is recorded [here](candidate-d6bcbc9d4e4e5aebc6449b2b47267b267c3cf2e2-disposable-pilot-2026-11-26.md). Refreshed PR #569 head `911e936fb74c367143c613e43e65777c70ca773b` passed the separate non-publishing Pilot Release [45865650938](https://github.com/kubedoio/rustshare/actions/runs/45865650938). This is branch-specific 100-user evidence for #569, not part of this b9 protected-main candidate. FWS remains **NO-GO** pending operator-owned DNS, HTTPS, authentication, secret custody, backup destination/restore, monitoring and authorized users. No institutional data, real users, publication, or deployment was involved.
