# Disposable pilot candidate — `20b3d75e`

This record identifies the protected-main revision exercised by Pilot Release
run [45807933634](https://github.com/kubedoio/rustshare/actions/runs/45807933634).
The workflow completed successfully on 2026-11-26. It exercised isolated,
synthetic data and disposable services. The image publication job was skipped.

## Immutable identities

| Item | Identity |
| --- | --- |
| Protected-main source | `20b3d75e9b56287c3dd02f075e5fb8a8f824ec1c` |
| Workflow / attempt | `45807933634` / `1` |
| Backend image ID used by the workflow | `sha256:11d7dbe2e285a9b679fb6f106129a4f3eae6f9b8398557569a0e8f22d8459f32` |
| Tested image archive SHA-256 | `da14f22ccc6e510f4715b7be399001d1dbdec0e77ba6f6166383f3830ae8d21f` |
| Evidence artifact digest | `sha256:96e9a9cfc9ce6cdec1fcf0dc64b519ebf5202893ec1d1daad57f10bf6679fb76` |
| Short-retention tested-image artifact digest | `sha256:75e11d468fea945167148cc8a3189cba0d8e0532361c445e5b527651d8119d4a` |
| Compose configuration SHA-256 | `e2c508177cf7ba6cd77fee952a23a181934cd73c1b16df07264a1cb59a9c6ca2` |

GitHub Actions retained the machine-generated bundle
`rustshare-pilot-evidence-20b3d75e9b56287c3dd02f075e5fb8a8f824ec1c` (34,165
bytes). The tested-image transfer artifact is 40,590,433 bytes. Their recorded
digests bind these records to the downloaded artifacts; the image was not
published to a registry.

## Qualification results

- Clean installation used fresh PostgreSQL and RustFS volumes, applied all 17
  migrations, reached readiness, and completed the canonical Files/Notes
  journey.
- The canonical synthetic journey passed user lifecycle, File sharing and
  cleanup, Notes H1/name preservation, and keyword search without AI.
- Four Playwright journeys passed with zero skipped, unexpected or flaky tests:
  Calendar settings import/export/idempotency; institutional Calendar context
  and unavailable-content denial; administrator Files and Note rename; and
  opening an exact historical Memory citation without changing Note identity.
- Integration Tests run
  [45807933666](https://github.com/kubedoio/rustshare/actions/runs/45807933666)
  passed on the same SHA. It includes Notes keyword search without AI, Memory
  reconciliation lease/restart recovery, Local Text connector worker E2E,
  source-access audit tests, repeated Ask Workspace authorization checks, and
  Buzz/Chat acceptance jobs.
- A separately provisioned PostgreSQL runtime role passed the posture check;
  negative checks confirmed that privileged attributes, schema creation and
  ownership/role-assumption paths were denied.
- Restart/persistence, database and storage failure/recovery, invalid
  configuration diagnostics, and readiness checks passed.
- Backup components and checksum manifests verified. Restore was tested on
  isolated fresh volumes and the canonical persisted File/Note state was
  rechecked.
- Upgrade from `v0.8.0-alpha.7` (`fd6cfba71f1cd3c07ec4729b6a9521bd821a4c67`)
  to this candidate passed. Rollback restored the pre-upgrade backup into fresh
  volumes and verified the restored prior image and persisted state.
- A representative migration regression check passed.
- Main CI [45807933580](https://github.com/kubedoio/rustshare/actions/runs/45807933580),
  CodeQL [45807933738](https://github.com/kubedoio/rustshare/actions/runs/45807933738),
  Frontend CI [45807933654](https://github.com/kubedoio/rustshare/actions/runs/45807933654),
  DCO [45807933789](https://github.com/kubedoio/rustshare/actions/runs/45807933789),
  and Vault Sync Compliance
  [45807933619](https://github.com/kubedoio/rustshare/actions/runs/45807933619)
  passed on the same SHA.

## Decision and boundary

**Disposable pilot: GO** for the tested revision and synthetic disposable
environment. This evidence does not establish FWS DNS, HTTPS, authentication,
operator secret custody, backup destination, monitoring or authorized-user
acceptance. Those target-environment gates remain with the FWS operator.

This is not release qualification: the workflow did not publish an image and
did not produce a release-image vulnerability scan, SBOM or provenance
attestation for this candidate. It is not authorization to deploy to FWS,
import institutional data, or invite real users. Memory remains partial under
issue #119 because the complete source-specific browser/API acceptance matrix
has not been executed; Local Text remains partial under #99 because connected
Windows/macOS platform acceptance is outstanding.
