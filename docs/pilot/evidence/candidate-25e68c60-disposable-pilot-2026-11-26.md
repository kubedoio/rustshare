# Disposable pilot candidate — `25e68c60`

This record identifies protected-main revision
`25e68c60d613c197bac1da7623368a8d0f71c273`, exercised by Pilot Release run
[45816480908](https://github.com/kubedoio/rustshare/actions/runs/45816480908).
The workflow passed on 2026-11-26 using isolated synthetic data and disposable
services. The image publication job was skipped.

## Immutable identities

| Item | Identity |
| --- | --- |
| Source commit | `25e68c60d613c197bac1da7623368a8d0f71c273` |
| Workflow / attempt | `45816480908` / `1` |
| Backend image ID | `sha256:ec333781d21fea27cf8e1c194135a3cd29dac69134666976d780974f1d41e7f7` |
| Tested image archive SHA-256 | `481b592acaea4f5298ede0f869e175da21cdb9f4f84cfb750d6316847ac5448e` |
| Evidence artifact digest | `sha256:90206b0844f824cd5f441be82aa1c4947a470528896dc343f091ea142fc30d8d` |
| Tested-image transfer artifact digest | `sha256:0be1e8de052bad88b466536701fc8494ff555131bdb467389e0ee456ec71beba` |
| Compose configuration SHA-256 | `0a8d53396a92c93580b15243ffb4d8cb22a12809b7f63b217a36b17f6b333d29` |

The evidence artifact
`rustshare-pilot-evidence-25e68c60d613c197bac1da7623368a8d0f71c273` is
34,390 bytes. The tested-image transfer artifact is 40,550,381 bytes. GitHub
retains the artifacts with the digests above; the image was not published to a
registry.

## Qualification results

- A clean install used fresh PostgreSQL and RustFS volumes, applied all 17
  migrations, reached readiness and completed the canonical Files/Notes journey.
- Four Playwright journeys passed with zero skipped, unexpected or flaky tests:
  Calendar import/export/idempotency; institutional Calendar context and
  unavailable-content denial; administrator Files and Note rename; and opening
  an exact historical Memory citation without changing Note identity.
- Separately provisioned PostgreSQL runtime-role checks passed, including
  negative checks for privileged attributes, schema creation, ownership and
  role assumption.
- Restart/persistence, database and storage failure/recovery, invalid
  configuration diagnostics and readiness checks passed.
- Backup contents and checksums verified. Restore was tested on isolated fresh
  volumes and persisted File/Note state was checked.
- Upgrade from `v0.8.0-alpha.7`
  (`fd6cfba71f1cd3c07ec4729b6a9521bd821a4c67`) passed. Rollback restored the
  pre-upgrade backup into fresh volumes and verified the prior image and
  persisted state. Representative migration regression validation passed.
- Frontend CI [45816480809](https://github.com/kubedoio/rustshare/actions/runs/45816480809),
  DCO [45816480542](https://github.com/kubedoio/rustshare/actions/runs/45816480542)
  and Vault Sync Compliance
  [45816480581](https://github.com/kubedoio/rustshare/actions/runs/45816480581)
  passed. Main CI [45816480817](https://github.com/kubedoio/rustshare/actions/runs/45816480817)
  was queued, Integration Tests
  [45816480555](https://github.com/kubedoio/rustshare/actions/runs/45816480555)
  and CodeQL [45816480926](https://github.com/kubedoio/rustshare/actions/runs/45816480926)
  passed. Main CI [45816480817](https://github.com/kubedoio/rustshare/actions/runs/45816480817)
  remains queued; Integration Tests
  [45816480555](https://github.com/kubedoio/rustshare/actions/runs/45816480555)
  remains in progress. Prior exact-main Integration
  Tests passed on `20b3d75e` in run
  [45807933666](https://github.com/kubedoio/rustshare/actions/runs/45807933666).

## Decision and boundary

**Disposable pilot: GO** for the tested revision and synthetic disposable
environment. Target-host acceptance remains outstanding: FWS DNS, HTTPS and
certificate, authentication, secret custody, backup destination/restore access,
monitoring and alert routing, and the authorized pilot-user list.

This is not release qualification: image publication was skipped and no
release-image scan, SBOM or provenance attestation was produced for this SHA.
It does not authorize an FWS deployment, institutional-data import or real-user
onboarding. Memory remains partial under #119 and connected Windows/macOS Local
Text acceptance remains outstanding under #99.
