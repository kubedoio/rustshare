# Disposable pilot candidate — `5bf3585b`

This record binds protected-main revision
`5bf3585b516a2fdff0fc1e88f15c7207333319f8` to non-publishing Pilot Release
run [45901542778](https://github.com/kubedoio/rustshare/actions/runs/45901542778).
The run completed successfully on 2026-11-26 using synthetic data and
isolated disposable services. The image publication job was skipped.

## Immutable identities

| Item | Identity |
| --- | --- |
| Protected-main source | `5bf3585b516a2fdff0fc1e88f15c7207333319f8` |
| Workflow / attempt | `45901542778` / 1; completed 22:36:57 UTC |
| Evidence artifact | [15158225765](https://github.com/kubedoio/rustshare/actions/runs/45901542778/artifacts/15158225765), 35,231 bytes, downloaded ZIP SHA-256 `79bf51f62437c3b464916e81a059731f97fe413264dca34b2bffcc014e5ba3f8` |
| Tested-image transfer artifact | [15158017785](https://github.com/kubedoio/rustshare/actions/runs/45901542778/artifacts/15158017785), 40,480,477 bytes, downloaded ZIP SHA-256 `b4177f34094df94cf1fb074be114807c58774dcf1b55fbcfb9ab20eed171ca62` |
| Backend image ID / revision | `sha256:ef29a05aa497b4fdcc4fe495f49e472d60e4ddff94853c4ad674c50676ee492f` / `5bf3585b516a2fdff0fc1e88f15c7207333319f8` |
| Tested image archive SHA-256 | `b04de4e3dcb8f7d7edc3224c85079fe9341af2b2629e866dbe080350eb4f18e4` |
| Compose configuration SHA-256 | `84b1d827896f9ecfd328752170eadea3d00278a0b3795f3e1348e7d5e56ca288` |
| Image version | `pilot-5bf3585b516a` |

Both artifact ZIPs were downloaded, independently hashed and passed `unzip -t`.
The image TAR hash matches `tested-image.env` in the image artifact. These are
private GitHub Actions artifacts, not registry publication or release
artifacts.

## Qualification results

- Fresh PostgreSQL and RustFS volumes; all 17 migrations through
  `20261123190000`; clean install and canonical Files/Notes journey passed.
- Five Playwright journeys passed with zero skipped, unexpected, or flaky
  results: Calendar Settings iCalendar import/export/idempotency; the full
  yesterday-dated institutional Calendar/meeting/Notes/Decision/Action/Memory
  search/audit journey with group revocation and unavailable-content denial; a
  100-user synthetic organization with two administrators and ordinary-user
  denial; Files plus independent Note name/H1 behavior; and opening an exact
  historical Memory citation without changing Note identity.
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

## Other gates on exact tested revision

- Integration Tests [45901542697](https://github.com/kubedoio/rustshare/actions/runs/45901542697)
  passed all four jobs: integration-tests, Chat Product Acceptance, Buzz Live
  Conformance, and Buzz Identity Replacement E2E.
- CodeQL [45901542642](https://github.com/kubedoio/rustshare/actions/runs/45901542642),
  Frontend CI [45901542708](https://github.com/kubedoio/rustshare/actions/runs/45901542708),
  Vault Sync Compliance [45901542741](https://github.com/kubedoio/rustshare/actions/runs/45901542741),
  and DCO Check [45901542614](https://github.com/kubedoio/rustshare/actions/runs/45901542614)
  passed. Main CI [45901542583](https://github.com/kubedoio/rustshare/actions/runs/45901542583)
  was still queued when this record was prepared.
- This Pilot Release does not include a qualifying runtime-image scan, SBOM, or
  provenance. Older image CVE alerts do not establish the status of this image.
  Release qualification is therefore **NOT QUALIFIED**.

## Decision and boundary

**Disposable workflow: PASS for this exact revision and tested synthetic
services. Disposable pilot: GO for repeatable engineering qualification. FWS
pilot: NO-GO pending operator-owned gates.** Memory source-specific acceptance
remains partial under #119; connected Local Text editor ingestion remains
outstanding under #99.

This evidence does not establish FWS DNS, HTTPS, authentication, operator
secret custody, backup destination/restore access, monitoring/on-call, or
authorized-user acceptance. It does not authorize FWS deployment, institutional
data import, image publication, or real-user onboarding.
