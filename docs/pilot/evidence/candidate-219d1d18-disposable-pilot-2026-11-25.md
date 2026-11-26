# Candidate 219d1d18: disposable pilot workflow evidence

Date: 2026-11-25. Run entirely in GitHub Actions disposable Compose projects
with synthetic data. No FWS host, institutional data, real users, or production
identity was used.

## Candidate and artifact identity

- Protected-main source SHA: `219d1d1858c037c79716d9791dcb4096c7980c42`.
- Workflow: [Pilot Release run 45645790259](https://github.com/kubedoio/rustshare/actions/runs/45645790259), completed successfully.
- Tested backend image ID: `sha256:b9f9638640ed7a14bf7c73e21201845094cf36aba481997997b6b31f1a2ef4b6`.
- Image archive SHA-256: `c97ef6726bc9be7782df7dbbe012e18c691b0bcaea37ad78d3b2a11012089c8a`.
- Evidence Actions artifact: `rustshare-pilot-evidence-219d1d1858c037c79716d9791dcb4096c7980c42`, SHA-256 `b9bb7d9814eac6eb6e7880f74ab235beb448ee22d3cce48e397a0d292759cdbe`, 90-day retention.
- Tested-image transfer artifact SHA-256: `620a8ccdcdf0da0d1e252e12f4529f188c0dcbc836d5f52db1e662a7b9322393`, 3-day retention. This is a workflow artifact only; the gated GHCR publication job did not run.

## Results

- Fresh Compose install, 17 migrations, readiness, and canonical smoke: PASS.
- Browser UI: 3 expected Playwright tests, 0 skipped, 0 unexpected, 0 flaky; PASS.
- Files/Notes share journey, search, user lifecycle, and persistence after restart: PASS.
- Runtime database posture and negative privilege checks: PASS for the separately provisioned `pilot_runtime` role. This is not evidence that every deployment role enforces RLS.
- Dependency/database/storage failure and recovery drills, invalid configuration, and production cookie checks: PASS.
- Backup bundle verification and restore in a unique disposable project, followed by canonical journey verification: PASS.
- Representative application migration check and migration-failure diagnostic: PASS.
- Final workflow result: PASS; readiness remains `NOT_READY_UNTIL_TARGET_ENVIRONMENT_ACCEPTANCE`.

## Limits

`upgrade.env` explicitly records `UPGRADE_STATUS=not_applicable` and
`UPGRADE_REASON=first_clean_install_baseline`. The migration check passed, but
this run does not qualify a complete installed-version upgrade or rollback from
a pre-upgrade backup. Roughly 100-user administration, connected native
credential stores, off-host recovery, and FWS operator acceptance were not
tested. The workflow run and its evidence artifact do not qualify a release or
authorize deployment/publication.
