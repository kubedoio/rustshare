# Disposable 100-user pilot evidence — PR #569 head `7d1926ae`

## Run and artifact identity

- Source revision: `7d1926aee99f142bdb2f6c85fc994b8b6603e8c9` (PR #569, based on protected main `3bd949b6`).
- [Pilot Release run 45753851378](https://github.com/kubedoio/rustshare/actions/runs/45753851378) completed **success**; the image publication job was skipped.
- Tested image ID: `sha256:0d07962a26772371a1d40f60de33551c2a6a0712a701ea94a092a05b204be005`.
- Tested image archive SHA-256: `529fe3442904d3012e70598a3ac2833b2d8d8df7f1af0fafdcde3e2c69c86a72`.
- Tested-image artifact [15088883648](https://github.com/kubedoio/rustshare/actions/runs/45753851378/artifacts/15088883648), digest `sha256:a1cd1f7c06dfb24d32b5f33c6f203f166960344a9734209afb61deb26c131657`.
- Evidence artifact [15088688876](https://github.com/kubedoio/rustshare/actions/runs/45753851378/artifacts/15088688876), digest `sha256:e6f3ab95953168841b367774bafaf5e233f6f0bcaadb776f44b23f44c5b83960`.

## Results and limits

- All four expected browser tests passed, with zero skipped, unexpected, or flaky results. The exact test “organization admins manage a 100-user synthetic organization safely” passed, including separate administrator actions and audit attribution.
- Fresh-volume installation and migrations, runtime database-role checks, restart/persistence, failure recovery, fail-closed configuration, backup, restore to fresh volumes, and representative migration checks passed.
- `UPGRADE_STATUS=not_applicable`; this run does not qualify installed upgrade or rollback.
- This is PR-head evidence, not protected-main evidence. PR #569 must still complete all required exact-head checks and independent review before merge.
- Image publication was skipped. This does not qualify FWS or authorize deployment, institutional data, or real users. Target-environment acceptance remains outstanding.
