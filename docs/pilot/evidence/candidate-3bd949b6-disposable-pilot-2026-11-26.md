# Disposable pilot evidence — protected main `3bd949b6`

## Run and artifact identity

- Protected-main source and tested revision: `3bd949b6e8f5bd9ea078c0d9e76ebb9194764e2f`.
- [Pilot Release run 45752680910](https://github.com/kubedoio/rustshare/actions/runs/45752680910), event `push`, completed **success**.
- The `publish-pilot-image` job was **skipped**. This is disposable-run evidence; no release, FWS deployment, or institutional data use occurred.
- Tested image version: `pilot-3bd949b6e8f5`.
- Tested image ID: `sha256:6b976537ab0d0b69d5e7ffb8b90629654ffe53430ee47732f978ca7311285131`.
- Image archive SHA-256: `616ef34556652bbcc5a1cd2c42708dd20ed15accf291f2df91dadddf8974b211`.
- Tested-image workflow artifact: [15088697032](https://github.com/kubedoio/rustshare/actions/runs/45752680910/artifacts/15088697032), artifact digest `sha256:4b86cf1ec86a3d38b635ef088b999246507e753dd64b5a4799fec98dc64f8744`.
- Evidence workflow artifact: [15088342984](https://github.com/kubedoio/rustshare/actions/runs/45752680910/artifacts/15088342984), artifact digest `sha256:2617667843c2eaaaa23b65a858566fdc16d114f9a14d741e2cc0b53dbd3fef24`.

## Executed results

- Clean install used fresh PostgreSQL and RustFS volumes and passed all 17 migrations.
- The canonical synthetic user and Files journey passed, including runtime-role checks, restart/persistence, bounded dependency recovery, invalid-configuration fail-closed behavior, and the synthetic user lifecycle.
- Browser acceptance passed **3/3** expected tests, with zero skipped, unexpected, or flaky results; the institutional Calendar/Notes/Decisions/Actions/Memory journey passed.
- Backup creation and verification passed; restore ran in a separate Compose project with fresh volumes and re-ran the journey successfully.
- The current-main Integration Tests run [45752680692](https://github.com/kubedoio/rustshare/actions/runs/45752680692) also completed **success** on the same SHA. Its integration job passed, including the Local Text PostgreSQL/RustFS worker E2E, Memory authorization/source-audit tests, Buzz conformance, identity replacement, and Chat Product Acceptance.

## Limits

- `UPGRADE_STATUS=not_applicable` because this run establishes the first clean-install baseline. Installed upgrade and rollback are not qualified by this run; PR #567's exact-head Pilot Release dispatch is recorded separately and still needs to complete.
- This run did not execute the separate 100-user/two-independent-admin journey. PR #569's refreshed two-admin run is separate and still in progress.
- This run does not qualify the #580 Calendar Settings import/export gate, connected Windows/macOS Local Text acceptance, or any FWS operator gate.
- The workflow readiness decision is `NOT_READY_UNTIL_TARGET_ENVIRONMENT_ACCEPTANCE`. Keep the FWS pilot **NO-GO** until the operator-owned acceptance is recorded.
