# Protected-main candidate af8f695b — disposable pilot evidence

**Result:** Pilot Release passed on exact protected main `af8f695b66b14526be1eff0b027b353f807dce38`. Publication was skipped. The workflow recorded `NOT_READY_UNTIL_TARGET_ENVIRONMENT_ACCEPTANCE`; this is disposable candidate evidence, not release qualification or FWS acceptance.

## Pilot Release

- Workflow: [run 45762571913](https://github.com/kubedoio/rustshare/actions/runs/45762571913), branch `main`, exact `head_sha` `af8f695b66b14526be1eff0b027b353f807dce38`.
- `pilot-compose-smoke`: success; `publish-pilot-image`: skipped.
- Clean install and all 17 migrations passed; canonical pilot journey passed; browser result was 3 expected and 0 unexpected.
- Backup and restore passed. Upgrade from immutable `v0.8.0-alpha.7` passed; rollback restored the pre-upgrade backup to fresh volumes and persistence verification passed.
- Candidate image ID: `sha256:bdbc79c7ce6c0d1b38a2d073293abe0566a0136beeaa02af952aaa32957b00ad`.
- Tested image archive SHA-256: `de234e2c8620de438c8883cda52f21c6709b30b47cc52a3585cfe3b663c95772`.
- Tested-image workflow artifact ID `15093195126`, digest `sha256:d8f43962b79107bef0b29e7b8738d8c5635e0013bddea2fcf697221abd2affcc` (workflow artifact expires 2026-11-29).
- Evidence workflow artifact ID `15091919766`, digest `sha256:dd16fbffdb671df5f2d3d750dedfb33d3025d6ae20437433bda9044c8bf0d6e2` (workflow artifact expires 2027-02-24).

## Exact-main Integration Tests

- Workflow: [run 45762572054](https://github.com/kubedoio/rustshare/actions/runs/45762572054), exact `head_sha` `af8f695b66b14526be1eff0b027b353f807dce38`; all four jobs passed: `Buzz Identity Replacement E2E`, `Buzz Live Conformance`, `integration-tests`, and `Chat Product Acceptance`.
- Sanitized Buzz identity-replacement evidence artifact ID `15091262635`, digest `sha256:7e642fca6daf6108b148859b24d3ffa4b4a78f0d4b9a4cb4e14f0194b9a87b77`.

These results qualify the tested application candidate and its exact-main integration suites. They do not establish connected desktop/native-keychain acceptance (#442), independent security acceptance of the managed-history run (#458), upstream publisher remediation/acceptance (#460), production release, or FWS target-environment readiness. No institutional data, real user, secret, or production mapping was used or changed.
