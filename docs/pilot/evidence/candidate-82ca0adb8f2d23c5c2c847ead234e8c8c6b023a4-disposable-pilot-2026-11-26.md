# Disposable pilot candidate — `82ca0adb`

This record binds protected-main revision
`82ca0adb8f2d23c5c2c847ead234e8c8c6b023a4` (after #575) to non-publishing
Pilot Release run [45906682546](https://github.com/kubedoio/rustshare/actions/runs/45906682546).
The workflow passed on 2026-11-26 using synthetic data and isolated disposable
services. The image publication job was skipped.

## Immutable identities

| Item | Identity |
| --- | --- |
| Protected-main source | `82ca0adb8f2d23c5c2c847ead234e8c8c6b023a4` |
| Workflow / attempt | `45906682546` / 1; completed 23:19:05 UTC |
| Evidence artifact | [15160115101](https://github.com/kubedoio/rustshare/actions/runs/45906682546/artifacts/15160115101), 34,428 bytes, downloaded ZIP SHA-256 `24db43761cdd664837b29d1cab6a8851ea67c1f355f00533354fd5f4d0e82811` |
| Tested-image transfer artifact | [15159846820](https://github.com/kubedoio/rustshare/actions/runs/45906682546/artifacts/15159846820), 40,478,975 bytes, downloaded ZIP SHA-256 `40f728720deacbe34cfffed9fa44bc45030540f17c2c6bb9f8ec6f7cc03b171a` |
| Backend image ID / revision | `sha256:30cc8a9872f912cdcf39abcc57ab65b2a4b01cfef4f2485a5e7c715e994d3cb7` / `82ca0adb8f2d23c5c2c847ead234e8c8c6b023a4` |
| Tested image archive SHA-256 | `a637ba045365ccf72f1f2b2f20956fe6ce01b797ab76ea68b89b628377e40ca1` |
| Compose configuration SHA-256 | `2542463b6b2a00bd97d420bfeb1c44c55772f7a2a532acd779547a3c99651f84` |
| Image version | `pilot-82ca0adb8f2d` |

Both artifact ZIPs were downloaded and independently hashed; both passed
`unzip -t`. The image TAR digest matches `tested-image.env`. These are private
GitHub Actions artifacts, not registry publication or release artifacts.

## Qualification results

- Fresh PostgreSQL and RustFS volumes; all 17 migrations through
  `20261123190000`; clean install and canonical Files/Notes journey passed.
- Five Playwright journeys passed with zero skipped, unexpected, or flaky
  results: Calendar Settings iCalendar import/export/idempotency; institutional
  Calendar/meeting linked-context, group revocation and audit assertions, and
  unavailable-content denial; a synthetic
  100-user organization with two provisioned organization administrators and
  ordinary-user denial; Files plus independent Note name/H1 behavior; and
  opening an exact historical Memory citation without changing Note identity.
- The separately provisioned `pilot_runtime` database-role checks passed.
  Runtime migrations were disabled. This does not establish blanket database
  row-level security.
- Restart and persistence, database/storage/dependency failure recovery,
  invalid configuration diagnostics, and invalid credential rejection passed.
- Backup checksum and isolated restore drills passed. Upgrade from immutable
  `v0.8.0-alpha.7` image revision
  `fd6cfba71f1cd3c07ec4729b6a9521bd821a4c67` passed. Rollback from the
  pre-upgrade backup into fresh volumes passed; no in-place schema downgrade
  was attempted.
- Pilot workflow aggregate result: `passed`. Image publication job: `skipped`.

## Other gates on the exact tested revision

- Integration Tests [45906682696](https://github.com/kubedoio/rustshare/actions/runs/45906682696)
  passed all four jobs: integration-tests, Chat Product Acceptance, Buzz Live
  Conformance, and Buzz Identity Replacement E2E. The integration job passed
  Local Text connector worker E2E, Memory source-access audit tests, and the
  repeated Ask Workspace security matrix.
- CodeQL [45906682633](https://github.com/kubedoio/rustshare/actions/runs/45906682633),
  Frontend CI [45906682721](https://github.com/kubedoio/rustshare/actions/runs/45906682721),
  Vault Sync Compliance [45906682654](https://github.com/kubedoio/rustshare/actions/runs/45906682654),
  and DCO Check [45906682624](https://github.com/kubedoio/rustshare/actions/runs/45906682624)
  passed.
- Main CI [45906682585](https://github.com/kubedoio/rustshare/actions/runs/45906682585)
  concluded failure in Monitoring Compose Contract. The failure remains
  unresolved and is not treated as a passing gate.
- This Pilot Release does not include a qualifying runtime-image scan, SBOM,
  or provenance. Release qualification is therefore **NOT QUALIFIED**.

## Decision and boundary

**Disposable workflow: PASS for this exact revision and tested synthetic
services. Disposable pilot: GO for repeatable engineering qualification. FWS
pilot: NO-GO pending operator-owned gates.** Institutional acceptance remains
partial under [#333](https://github.com/kubedoio/rustshare/issues/333); operator
environment acceptance remains open under [#335](https://github.com/kubedoio/rustshare/issues/335).

This evidence does not establish FWS DNS, HTTPS, authentication, operator
secret custody, backup destination/restore access, monitoring/on-call, or
authorized-user acceptance. It does not authorize FWS deployment,
institutional data import, image publication, or real-user onboarding.
