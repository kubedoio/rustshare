# Protected-main d0fc70c0 disposable pilot evidence

Date: 2026-11-26

## Candidate identity

- Protected-main SHA: `d0fc70c0594f709223e3ff83423cc38340f03dc6` (includes merged PR #572).
- Pilot Release run: [45842979547](https://github.com/kubedoio/rustshare/actions/runs/45842979547), attempt 1 — **passed**.
- Main CodeQL run [45842979479](https://github.com/kubedoio/rustshare/actions/runs/45842979479) — passed. Integration Tests run [45842979511](https://github.com/kubedoio/rustshare/actions/runs/45842979511) — all four jobs passed. Main CI run [45842979462](https://github.com/kubedoio/rustshare/actions/runs/45842979462) was cancelled after protected main advanced.
- Tested backend image ID: `sha256:32348a3c04396fbd7edb7b45042929e4818be5ee331ac388c56f45c37acba585`.
- Compose config ID: `c6da48b87c3806e654afe469fb29ab1df37c839ef8c21e99bee394467db8e160`.
- Evidence artifact: [15133236620](https://github.com/kubedoio/rustshare/actions/runs/45842979547/artifacts/15133236620), `sha256:224bfc4df8e11023f5de66f2fd030e3cc5ed5935522e6181fc65faf905f088bd`.
- Tested-image transfer artifact: [15133965783](https://github.com/kubedoio/rustshare/actions/runs/45842979547/artifacts/15133965783), `sha256:a77303b5b6187e534f842d2280cb3ebe2110dc3947093e995dc75b8daeaeeeea`; contained image archive SHA-256 `a59df05f196910a7f3bd9aa0cf755479e03dfb5bcd760bab2b466f65e3ff4b6d`.
- Image publication was skipped; there is no registry digest.

## Qualification results

- Fresh PostgreSQL and RustFS volumes; all 17 migrations applied through `20261123190000`; first canonical journey passed.
- All four Playwright journeys passed with 0 skipped, unexpected, or flaky results: Calendar settings import/export and idempotency; denial for unavailable linked Calendar content; Files plus independent Note-name/H1 editing; exact historical Memory citation without changing Note identity.
- Runtime database posture passed as `pilot_runtime`; backup secret scan passed.
- Restart/persistence, dependency failure/recovery, invalid-configuration diagnostics, backup, restore to fresh volumes, installed upgrade from the immutable `v0.8.0-alpha.7` image, and fresh-volume rollback passed.

## Qualification boundary

This is a synthetic, disposable protected-main candidate. The Pilot Release image was not published. This run does not include the 100-user administration journey; PR #569 separately passed that journey and recovery drill on exact head `b5a751000f5b8b1d15996db2dff7a01ee70be55e` in run [45839820086](https://github.com/kubedoio/rustshare/actions/runs/45839820086). After #572 merged, #569 refreshed to d0-based head `920a25ca8f021108a0be0f6cbc72adb2b268549a`; exact-head Pilot Release run [45849369794](https://github.com/kubedoio/rustshare/actions/runs/45849369794) passed (candidate evidence: [920a25ca](candidate-920a25ca-pr569-disposable-pilot-2026-11-26.md)). Protected main later advanced to `b9baaaefe59c9b0fd4f42abddd4763e576617066`; b9 Integration Tests passed all four jobs in run [45857490452](https://github.com/kubedoio/rustshare/actions/runs/45857490452); b9 Main CI [45857490449](https://github.com/kubedoio/rustshare/actions/runs/45857490449) remains queued. PR #569 is now refreshed to b9-based head `d3f6e442b46dc77b3669253b26cd5a765a096635`, with non-publishing Pilot Release run [45862470683](https://github.com/kubedoio/rustshare/actions/runs/45862470683) queued.

This is not release qualification: immutable release-image scanning, SBOM, and provenance have not been produced for d0. FWS remains **NO-GO** pending operator-owned DNS, HTTPS, authentication, secret custody, backup destination/restore, monitoring, and authorized-user gates. No institutional data, real users, publication, or deployment was involved.
