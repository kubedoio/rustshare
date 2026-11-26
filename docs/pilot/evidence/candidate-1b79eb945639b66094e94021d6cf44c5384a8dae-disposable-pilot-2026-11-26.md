# Disposable pilot candidate — `1b79eb94`

This record identifies protected-main revision
`1b79eb945639b66094e94021d6cf44c5384a8dae`, exercised by non-publishing
Pilot Release run [45881776865](https://github.com/kubedoio/rustshare/actions/runs/45881776865).
The workflow completed successfully on 2026-11-26 using synthetic data and
disposable services. The registry publication job was skipped.

## Immutable identities

| Item | Identity |
| --- | --- |
| Protected-main source | `1b79eb945639b66094e94021d6cf44c5384a8dae` |
| Workflow / attempt | `45881776865` / 1; completed 19:45:34 UTC |
| Evidence artifact | [15149237006](https://github.com/kubedoio/rustshare/actions/runs/45881776865/artifacts/15149237006), 34,963 bytes, ZIP SHA-256 `81660c9775d376f5fbdedcffbebb5d85dde805346f27d752c2dd33489e5bb137` |
| Tested-image transfer artifact | [15148309760](https://github.com/kubedoio/rustshare/actions/runs/45881776865/artifacts/15148309760), 40,423,777 bytes, ZIP SHA-256 `a866dca59de1ea2f525611573842f5b676e25339787dd0cec60203c5770598ce` |
| Backend image ID / revision | `sha256:27a884cb15785e0e8b3ba831d2c5ce7a88007994507db48a8d4783050997f642` / `1b79eb945639b66094e94021d6cf44c5384a8dae` |
| Tested image archive SHA-256 | `09de3783635d3ceeafa0d4e3500b118a329026406fb05c3f9481f0f3cd0af08f` |
| Compose configuration SHA-256 | `894345ad058cf018505e8cb0894cc4461ffc5c5896424ab9eb1747fd2afdbb51` |
| Image version | `pilot-1b79eb945639` |

The workflow retains the evidence artifact for 90 days and the tested-image
transfer artifact for 3 days. These are GitHub Actions artifacts, not a registry
publication or release artifact.

## Qualification results

- Fresh PostgreSQL and RustFS volumes; all 17 migrations through
  `20261123190000`; clean install and canonical Files/Notes journey passed.
- Four Playwright journeys passed with zero skipped, unexpected, or flaky
  results: Calendar Settings import/export/idempotency; institutional Calendar
  context and unavailable-content denial; Files plus independent Note name/H1;
  and opening an exact historical Memory citation without changing Note identity.
- The separate `pilot_runtime` role checks passed, including negative privilege,
  routine, type, schema, and ownership checks. Runtime migrations were disabled.
  This is not evidence of blanket database row-level security.
- Restart/persistence, database and storage failure/recovery, invalid
  configuration diagnostics, credential rejection, and security-sanity checks
  passed.
- Backup checksum and restore drills passed. Upgrade from `v0.8.0-alpha.7`
  (source `fd6cfba71f1cd3c07ec4729b6a9521bd821a4c67`, prior migration
  `20261107150002`) passed. Rollback restored the pre-upgrade backup into fresh
  volumes using the prior image digest
  `sha256:42ba8f0c8efad65aabe655ae29e296999ac93c54ae7e7823b641b333b97f2059`.
- Workflow readiness was
  `NOT_READY_UNTIL_TARGET_ENVIRONMENT_ACCEPTANCE`.

## Decision and boundary

**Disposable workflow: PASS for this exact revision and tested synthetic
environment. Overall pilot: NO-GO pending remaining acceptance.** The 100-user/two-administrator run for current-main-based PR #569 passed at
head `453ca591aea23e0190f09f932f6d9d451a007321` in Pilot Release run
[45882009152](https://github.com/kubedoio/rustshare/actions/runs/45882009152); its
exact candidate evidence is [here](candidate-453ca591aea23e0190f09f932f6d9d451a007321-pr569-disposable-pilot-2026-11-26.md).
Memory's source-specific acceptance remains partial under #119, and connected
Windows/macOS Local Text editor ingestion remains outstanding under #99.

At the 2026-11-26 19:50 UTC status check, current-main CI [45881776570](https://github.com/kubedoio/rustshare/actions/runs/45881776570)
was still running queued jobs; Integration Tests [45881776637](https://github.com/kubedoio/rustshare/actions/runs/45881776637)
was in progress; CodeQL [45881776713](https://github.com/kubedoio/rustshare/actions/runs/45881776713)
had passed. These results do not qualify a release. No release image scan,
SBOM, or provenance was produced for this candidate.

This record does not establish FWS DNS, HTTPS, authentication, operator secret
custody, backup destination, monitoring, or authorized-user acceptance. It does
not authorize FWS deployment, institutional-data import, image publication, or
real-user onboarding. FWS remains NO-GO pending operator-owned gates and owner
approval.
