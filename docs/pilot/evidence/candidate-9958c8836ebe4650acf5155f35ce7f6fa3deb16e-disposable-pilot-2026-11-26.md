# Disposable pilot candidate — `9958c883`

This record binds the protected-main revision
`9958c8836ebe4650acf5155f35ce7f6fa3deb16e` to non-publishing Pilot Release run
[45890507729](https://github.com/kubedoio/rustshare/actions/runs/45890507729).
The run completed successfully on 2026-11-26 using synthetic data and
isolated disposable services. The registry publication job was skipped.

## Immutable identities

| Item | Identity |
| --- | --- |
| Protected-main source | `9958c8836ebe4650acf5155f35ce7f6fa3deb16e` |
| Workflow / attempt | `45890507729` / 1; completed 21:02:26 UTC |
| Evidence artifact | [15153660588](https://github.com/kubedoio/rustshare/actions/runs/45890507729/artifacts/15153660588), 35,316 bytes, SHA-256 `f53b5bf5be61a8ad3f634f461293cfac33cdd505088c44b7b446dd2ecdb0e64a` |
| Tested-image transfer artifact | [15153815310](https://github.com/kubedoio/rustshare/actions/runs/45890507729/artifacts/15153815310), 40,423,861 bytes, SHA-256 `1016db8007debb9527a58414b77f191fd1f4cbfc6a9be0c6d84ab04c4fc7fd30` |
| Backend image ID / revision | `sha256:266d83b419395b9e25996becac2ae79730a9c94b2ed626ea12a060e3ea5b0d6e` / `9958c8836ebe4650acf5155f35ce7f6fa3deb16e` |
| Tested image archive SHA-256 | `8ca7d8420e1640669367d3c949365468cbbd74098f500e30abef1052beb7a88d` |
| Compose configuration SHA-256 | `dbc99c6678f47ab23e2ee54e473f8b47c9a1f0d85c62c3ca34ca9e378e2ca5c1` |
| Image version | `pilot-9958c8836ebe` |

The evidence artifact is retained for 90 days and the tested-image transfer
artifact for 3 days. They are private GitHub Actions artifacts, not a registry
publication or release artifact.

## Qualification results

- Fresh PostgreSQL and RustFS volumes; all 17 migrations through
  `20261123190000`; clean install and canonical Files/Notes journey passed.
- Five Playwright journeys passed with zero skipped, unexpected, or flaky
  results: Calendar Settings iCalendar import/export/idempotency; institutional
  Calendar linked context and unavailable-content denial; a 100-user synthetic
  organization with two administrators and ordinary-user denial; Files plus
  independent Note name/H1 behavior; and opening an exact historical Memory
  citation without changing Note identity.
- The separately provisioned `pilot_runtime` database role checks passed.
  Runtime migrations were disabled. This does not establish blanket database
  row-level security.
- Restart and persistence, database/storage/dependency failure recovery,
  invalid configuration diagnostics, and invalid credential rejection passed.
- Backup checksum and isolated restore drills passed. Upgrade from
  `v0.8.0-alpha.7` (previous migration `20261107150002`) passed. Rollback from
  the pre-upgrade backup into fresh volumes passed; rollback was not performed
  by downgrading the upgraded database in place.
- Pilot workflow aggregate result: `passed`. Publication job: `skipped`.

## Other exact-main gates

- Integration Tests run [45890507865](https://github.com/kubedoio/rustshare/actions/runs/45890507865)
  completed with all four jobs green at 20:54:59 UTC: integration-tests, Chat
  Product Acceptance, Buzz Live Conformance, and Buzz Identity Replacement E2E.
- CodeQL run [45890507700](https://github.com/kubedoio/rustshare/actions/runs/45890507700)
  completed successfully.
- Main CI run [45890508008](https://github.com/kubedoio/rustshare/actions/runs/45890508008)
  completed successfully at 21:11:30 UTC, including Build Release, tests, coverage,
  migration baseline, SQLx prepare, Clippy, and Cargo Deny. Its first Build Release
  attempt rejected an invalid downloaded Swagger UI archive (`Could not find EOCD`);
  the same-SHA retry passed. No build or security gate was weakened.
- This disposable Pilot Release does not provide release image scanning, SBOM, or
  provenance, so this candidate is not a qualified release artifact.

## Decision and boundary

**Disposable workflow: PASS for this exact revision and tested synthetic
services. Overall pilot: NO-GO pending target-environment acceptance.** Memory
source-specific acceptance remains partial under #119; connected Windows/macOS
Local Text editor ingestion remains outstanding under #99. The exact 100-user
journey is included in this candidate's five browser tests.

This record does not establish FWS DNS, HTTPS, authentication, operator secret
custody, backup destination/restore access, monitoring/on-call, or authorized
user acceptance. It does not authorize FWS deployment, institutional-data
import, image publication, or real-user onboarding. FWS remains NO-GO pending
operator-owned gates and owner approval.
