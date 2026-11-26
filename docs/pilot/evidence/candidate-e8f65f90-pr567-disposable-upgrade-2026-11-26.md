# Disposable upgrade and rollback evidence — PR #567 head `e8f65f90`

## Run and artifact identity

- Tested source revision: `e8f65f909cf78ecd2509c6e23f4bcf72bef41342`, the exact PR #567 head.
- [Pilot Release run 45757287608](https://github.com/kubedoio/rustshare/actions/runs/45757287608) completed **success** on `workflow_dispatch`; its image-publication job was skipped.
- The tested source tree SHA `39dc246bfb04355bcc55f517f95bfbe2eeeb29ed` matches protected-main merge commit `af8f695b66b14526be1eff0b027b353f807dce38`. Image metadata remains tied to source commit `e8f65f90`; this is not a release image or an artifact built with the main commit identity.
- Tested image ID: `sha256:061610d818d8639f24dd99bf86c1363665026d67a1599464def957b3b48bf994`.
- Image archive SHA-256: `f08e8323276bc9362623db48b94d36a809d3b33f49c890e9f46ca0e0a332d7d0`.
- Tested-image workflow artifact [15091062827](https://github.com/kubedoio/rustshare/actions/runs/45757287608/artifacts/15091062827), digest `sha256:c917d183ead757d5d582cabbde8e6c16884b53c74d8b84333d19125f252422e4`.
- Evidence workflow artifact [15090893155](https://github.com/kubedoio/rustshare/actions/runs/45757287608/artifacts/15090893155), digest `sha256:d76b7a48a750a552b509f5e90b983b39d39bfbcf72efa0d5f17f18de854ee8c9`.

## Executed results

- Fresh PostgreSQL and RustFS volumes, first boot, all 17 migrations, and the canonical synthetic journey passed.
- Browser acceptance passed 3/3 expected tests with zero skipped, unexpected, or flaky results. The 100-user administration and Calendar import/export journeys are not included in this run.
- Backup creation, diagnostic secret scan, checksum verification, and restore to separate fresh volumes passed; persistence checks passed after restore.
- Installed upgrade from the immutable `v0.8.0-alpha.7` image (`fd6cfba7`, digest `sha256:42ba8f0c8efad65aabe655ae29e296999ac93c54ae7e7823b641b333b97f2059`) to the tested candidate passed. Representative upgrade migrations passed.
- Rollback restored the pre-upgrade backup onto fresh volumes and reinstated the previous immutable image; the restored source SHA and image digest matched the recorded previous release. Persistence verification passed.

## Limits

- This run is bound to PR head `e8f65f90`, whose source tree matches protected main `af8f695b`; it is not a release-qualification run and does not establish an immutable registry artifact for main.
- GitHub image publication was skipped. No FWS deployment, institutional data, or real users were involved.
- The workflow readiness decision is `NOT_READY_UNTIL_TARGET_ENVIRONMENT_ACCEPTANCE`. Keep the FWS pilot **NO-GO** until the operator-owned DNS, HTTPS, authentication, secret custody, backup destination, monitoring, and authorized-user gates pass.
