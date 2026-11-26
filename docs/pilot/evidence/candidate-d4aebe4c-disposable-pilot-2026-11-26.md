# Disposable pilot candidate — `d4aebe4c`

Protected main `d4aebe4cf146372bc7d58cb193677921516178e2` passed Pilot Release
run [45830910523](https://github.com/kubedoio/rustshare/actions/runs/45830910523)
on 2026-11-26 using synthetic data and isolated disposable services. The
workflow uploaded the evidence bundle; it skipped image publication.

## Candidate identities

| Item | Identity |
| --- | --- |
| Source commit | `d4aebe4cf146372bc7d58cb193677921516178e2` |
| Workflow / attempt | `45830910523` / `1` |
| Local backend image ID | `sha256:f0ddb6d89e686250eac53f698c725ccca1d9ad7412dfe2c89d7f3d3453618a2c` |
| Pilot build version | `pilot-d4aebe4cf146` |
| Compose configuration SHA-256 | `522587e98e9442d90d4480aad896a13c0df76a599b2636d05fafb0b873d25d1e` |
| Evidence artifact | `rustshare-pilot-evidence-d4aebe4cf146372bc7d58cb193677921516178e2`, ID `15127175256`, 34,215 bytes |
| Evidence artifact digest | `sha256:1028c561e0df729956c972574d9e07c0128eff4cf2259dadf0cc546304795f95` |
| Tested-image transfer artifact | ID `15126125595`, 40,550,844 bytes |
| Tested-image artifact digest | `sha256:86abc1292b588a470604a449dfc05a03441d4212d131d14fc65f0caf6df61ff5` |

The local image ID identifies the disposable image used by the workflow. The
image was not published to GHCR and has no registry digest. The workflow
artifacts are downloadable from the linked run; the evidence artifact is
retained for 90 days.

## Acceptance results

- Clean install used fresh PostgreSQL and RustFS volumes, applied all 17
  migrations through `20261123190000`, reached readiness, and passed the
  canonical Files/Notes journey.
- Four Playwright journeys passed with zero skipped, unexpected, or flaky
  tests: Calendar import/export/idempotency; linked Calendar context and
  unavailable-content denial; administrator Files and Note rename; and opening
  an exact historical Memory citation without changing Note identity.
- Runtime database-role posture and negative privilege checks passed. This
  does not demonstrate blanket PostgreSQL RLS tenant isolation.
- Restart and persistence, database and storage failure/recovery, invalid
  configuration rejection, and secret-safe diagnostic scans passed.
- Backup verification passed. Restore ran on isolated fresh volumes and
  verified persisted state.
- Upgrade from `v0.8.0-alpha.7` source
  `fd6cfba71f1cd3c07ec4729b6a9521bd821a4c67` and immutable image
  `sha256:42ba8f0c8efad65aabe655ae29e296999ac93c54ae7e7823b641b333b97f2059`
  passed. Rollback restored the pre-upgrade backup to fresh volumes and
  verified state. Representative upgrade migrations passed.
- The Pilot Release job passed all 39 successful steps; one conditional step
  was skipped. Publication was skipped.
- On d4, CodeQL [45830910559](https://github.com/kubedoio/rustshare/actions/runs/45830910559),
  Frontend CI [45830910536](https://github.com/kubedoio/rustshare/actions/runs/45830910536),
  DCO [45830910553](https://github.com/kubedoio/rustshare/actions/runs/45830910553),
  and Vault Sync Compliance
  [45830910687](https://github.com/kubedoio/rustshare/actions/runs/45830910687)
  passed. Main CI [45830910826](https://github.com/kubedoio/rustshare/actions/runs/45830910826)
  was cancelled after main advanced. Integration Tests
  [45830910599](https://github.com/kubedoio/rustshare/actions/runs/45830910599)
  passed all four jobs.

## Decision and limits

**Core disposable Pilot Release workflow: PASS** for this exact revision and
synthetic environment. The d4 Integration Tests passed all four jobs; d4 Main CI
was cancelled after protected main advanced. The 100-user journey later passed
on separate PR #569 head `d3f6e442`, not on the exact d4 candidate, so do not
combine those heads into a single candidate GO. FWS remains **NO-GO** pending
operator-owned DNS, HTTPS/certificate, authentication, secret custody, backup
destination and restore access, monitoring/alert routing, and authorized
pilot-user selection.

This is not release qualification. No release image scan, SBOM, or provenance
was produced for d4; image publication was skipped. It does not authorize an
FWS deployment, institutional-data import, or real-user onboarding. Memory
remains partial under #119, connected Windows/macOS Local Text acceptance
remains outstanding under #99, and the pilot database-role checks do not prove
blanket RLS tenant isolation.
