# Disposable pilot candidate — `b3f64684`

## Identity and decision

- Protected main source: `b3f64684a10ef42aac76e22b0ac54371a67c5f9c`.
- Pilot Release run: [45729254164](https://github.com/kubedoio/rustshare/actions/runs/45729254164), attempt 1, `2026-11-26T00:30:43Z`–`00:58:40Z`.
- Workflow result: passed. Workflow readiness decision: `NOT_READY_UNTIL_TARGET_ENVIRONMENT_ACCEPTANCE`.
- Tested backend image: `pilot-b3f64684a10e`, image ID `sha256:460d2e6ff99c54715c7e7d5347a0185d18d907d56400325131d7166a0f98f4e6`.
- Tested image archive SHA-256: `2ac7e144c76bfc4c15b4e31902c381cf14510f9aaa309e8c8c3e6b9b62e0cf23`.
- Evidence artifact: [15076473787](https://github.com/kubedoio/rustshare/actions/runs/45729254164/artifacts/15076473787), 28,262 bytes; uploaded artifact SHA-256 `489816c3011ea480d997b0394a351d70f1f4d76238b809f42d8204cf46c96c89`.
- Tested-image artifact: [15077156964](https://github.com/kubedoio/rustshare/actions/runs/45729254164/artifacts/15077156964), 40,384,905 bytes. `publish-pilot-image` was skipped; this artifact was not published as a release image.

## Executed qualification

- Clean installation used fresh PostgreSQL and RustFS volumes, applied 17 migrations through `20261123190000`, verified image identity and first boot, and passed the canonical Files/Notes/user journey.
- Migration failure aborted startup with diagnostics. Representative upgrade migrations passed; installed product upgrade was explicitly `not_applicable` because this is the first clean-install baseline. No installed upgrade/rollback drill was run.
- Runtime database role checks passed, including negative privilege checks; migrations were not run under the runtime role.
- Restart/persistence, bounded database and storage failure/recovery, authentication failure/recovery, invalid configuration rejection, and invalid credential rejection passed.
- Backup creation and secret scan passed. The restore drill used isolated volumes and passed persistence verification.
- Browser suite ran three expected tests with 0 skipped, 0 unexpected and 0 flaky: institutional Calendar linked-context/denial; Files and Note identity/H1 behavior; and opening an exact historical Memory citation without changing Note identity.
- UI checks passed. Chat status returned HTTP 200 in the canonical browser journey.

## Limits

- This is disposable synthetic-data evidence only. It is not FWS host acceptance and does not establish FWS DNS/TLS, OIDC, secret custody, backups, monitoring or authorized-user gates.
- The journey did not establish multiple independent administrators, group membership revocation, cross-tenant denial in the institutional browser flow, or the 100-user scale target. PRs #576 and #569 remain open for additional evidence.
- No installed upgrade/rollback was performed. Representative migration validation is narrower than upgrading an installed candidate.
- The tested image was retained as a run artifact. This run did not publish an image or produce release-qualifying image scan, SBOM or provenance evidence.
- Readiness remains **NO-GO** for an institutional disposable pilot and **NO-GO** for FWS until the missing workflow and target-environment acceptance gates pass.
