# PR #569 disposable pilot evidence — 2026-11-26

This is a candidate-specific `workflow_dispatch` run on PR #569 head `308017d476fded112f1b2bf8d7026a2c6d20a9b4`, whose second parent is protected-main snapshot `b3f64684a10ef42aac76e22d2d19218c84ada28`. Current protected main is `a4d45723f6d63b4b478ce08b449e03354cb26e49`; this result is not evidence for an image built from `a4`.

## Run and artifact identity

- [Pilot Release run 45741839680](https://github.com/kubedoio/rustshare/actions/runs/45741839680), attempt 1, `workflow_dispatch`, completed **success** at `2026-11-26T02:55:25Z`.
- Candidate build version: `pilot-308017d476fd`.
- Backend image ID: `sha256:770cc0800008227e2974e622ea194c04e987724a416db2a5046c3f5915256f5f`.
- Tested-image artifact: `rustshare-pilot-tested-image-308017d476fded112f1b2bf8d7026a2c6d20a9b4`, artifact ID `15082917078`, artifact digest `sha256:e45f30f900a455a7dae3315b4afee17f68d6da4123d41440717d64abd531dcd8`; image archive SHA-256 `2a52148fcea69e5f61b3ca0567c7fca74f62b8973805d2a5683e4adc489bafd7`.
- Evidence artifact: `rustshare-pilot-evidence-308017d476fded112f1b2bf8d7026a2c6d20a9b4`, artifact ID `15083286614`, artifact digest `sha256:8e5834ad92ee10e74807216a1c66d74c7167a2e665a2159fdaf4fb2bf4a22485`.
- The `publish-pilot-image` job was **skipped**. The tested image was retained as a workflow artifact; it was not published or deployed.

## Executed acceptance

- Clean install used fresh PostgreSQL and RustFS volumes, applied 17 migrations, and passed the first canonical journey.
- Runtime database role and negative checks passed; the recorded runtime role was `pilot_runtime`. This does not establish PostgreSQL RLS tenant isolation.
- Restart/persistence, bounded dependency failure and recovery, invalid-configuration rejection, backup verification, and restore to a separate isolated Compose project with fresh volumes passed.
- Playwright reported 4 expected tests, 0 skipped, 0 unexpected, 0 flaky, and 0 errors. Each test had a passed result with no retry:
  - institutional Calendar journey preserves linked context and denies unavailable content;
  - organization admins manage a 100-user synthetic organization safely; the journey creates 100 synthetic users, grants organization-admin capability to two users, confirms an ordinary user receives HTTP 403, verifies 50+50 pagination, runs search and offboarding preview/apply, and checks audit attribution and cleanup/last-admin safeguards;
  - pilot administrator uses Files and edits a Note name independently from its H1;
  - Notes opens an exact historical Memory citation without changing Note identity.
- Representative migration validation passed. The evidence records `UPGRADE_STATUS=not_applicable` because this was the first clean-install baseline. It does **not** qualify an installed old-image upgrade or rollback; PR #567 is the separate upgrade/rollback qualification.

## Readiness limits

The workflow summary records `READINESS_DECISION=NOT_READY_UNTIL_TARGET_ENVIRONMENT_ACCEPTANCE`. This disposable run does not prove cross-tenant denial, independent use/recovery by both administrators, group-revocation-to-Memory search/citation behavior, complete audit attribution, target-platform Local Text ingestion, or FWS host acceptance. The scale test creates two synthetic organization-admin grants, but it exercises the admin UI as the second admin only; this is not a two-admin recovery rehearsal or group-membership revocation test. It used synthetic CI data and invited no real users. FWS DNS/HTTPS, authentication and group-claim contract, secret custody, backup destination, monitoring, and authorized-user gates remain operator-dependent.

Keep this run distinct from the exact-protected-main pilot candidate in [candidate b3f64684](candidate-b3f64684-disposable-pilot-2026-11-26.md), and from the installed-upgrade/rollback run under PR #567.
