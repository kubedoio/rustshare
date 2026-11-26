Protected main is `99d289a18a1d8e9384528f7e1205d9eb22d20e03` after #570. As of 2026-11-26 18:51 UTC, non-publishing Pilot Release [45873817433](https://github.com/kubedoio/rustshare/actions/runs/45873817433) passed and its exact artifacts and qualification boundary are recorded in the [99d289a1 candidate record](candidate-99d289a18a1d8e9384528f7e1205d9eb22d20e03-disposable-pilot-2026-11-26.md). CodeQL [45873817208](https://github.com/kubedoio/rustshare/actions/runs/45873817208), DCO, Frontend CI, and Vault Sync Compliance passed. Main CI [45873817353](https://github.com/kubedoio/rustshare/actions/runs/45873817353) is queued and Integration Tests [45873817319](https://github.com/kubedoio/rustshare/actions/runs/45873817319) is in progress. The d6 candidate passed Integration Tests and non-publishing Pilot Release; its exact artifacts are [here](candidate-d6bcbc9d4e4e5aebc6449b2b47267b267c3cf2e2-disposable-pilot-2026-11-26.md), but d6 is historical for 99d. Refreshed #569 head `0275042cf4b95c942df7e17656549b9c26f666d6` has non-publishing Pilot Release [45876551829](https://github.com/kubedoio/rustshare/actions/runs/45876551829) queued with `publish_image=false`; its earlier successful run belongs to former head `911e936fb74c367143c613e43e65777c70ca773b`. Earlier b9/d0/d4 evidence remains historical: [b9](candidate-b9baaaefe59c9b0fd4f42abddd4763e576617066-disposable-pilot-2026-11-26.md), [d0](candidate-d0fc70c0-disposable-pilot-2026-11-26.md), [d4](candidate-d4aebe4c-disposable-pilot-2026-11-26.md).

Protected main is `99d289a18a1d8e9384528f7e1205d9eb22d20e03` after #570. As of 2026-11-26 18:32 UTC, Main CI [45873817353](https://github.com/kubedoio/rustshare/actions/runs/45873817353) and Integration Tests [45873817319](https://github.com/kubedoio/rustshare/actions/runs/45873817319) are queued; non-publishing Pilot Release [45873817433](https://github.com/kubedoio/rustshare/actions/runs/45873817433) is in progress; CodeQL [45873817208](https://github.com/kubedoio/rustshare/actions/runs/45873817208) passed; DCO, Frontend CI, and Vault Sync Compliance also passed. The d6 candidate passed Integration Tests and non-publishing Pilot Release, with exact artifacts recorded [here](candidate-d6bcbc9d4e4e5aebc6449b2b47267b267c3cf2e2-disposable-pilot-2026-11-26.md); d6 remains historical for 99d. Refreshed #569 head `0275042cf4b95c942df7e17656549b9c26f666d6` has non-publishing Pilot Release [45876551829](https://github.com/kubedoio/rustshare/actions/runs/45876551829) queued with `publish_image=false`; its earlier successful pilot run belongs to former head `911e936fb74c367143c613e43e65777c70ca773b`. Earlier b9/d0/d4 evidence remains historical: [b9](candidate-b9baaaefe59c9b0fd4f42abddd4763e576617066-disposable-pilot-2026-11-26.md), [d0](candidate-d0fc70c0-disposable-pilot-2026-11-26.md), [d4](candidate-d4aebe4c-disposable-pilot-2026-11-26.md).

Historical exact-main candidate [3bd949b6](candidate-3bd949b6-disposable-pilot-2026-11-26.md), tied to run 45752680910 on former main `3bd949b6e8f5bd9ea078c0d9e76ebb9194764e2f`, remains historical; the current protected-main status is at the top of this page. Its clean-install, browser, backup and restore results remain historical evidence.

An earlier exact-candidate disposable Pilot Release run is [candidate c613cc7b](candidate-c613cc7b-disposable-pilot-2026-11-26.md), tied to PR #569 head `c613cc7b368d0c61d9bef99c053ce3e3c7071f2f` and run 45746539135. It passed clean install, the four browser journeys including 100-user administration, backup, and isolated restore. Installed upgrade/rollback remains unqualified for that first-clean-install baseline; FWS/operator acceptance remains outstanding. Image publication was skipped.

PR #569's earlier manual disposable-pilot run on exact head `308017d476fded112f1b2bf8d7026a2c6d20a9b4` passed the 100-user browser journey and fresh-volume restore drill. Its tested image was retained as a run artifact, and image publication was skipped. This branch-specific evidence does not qualify current protected main or installed upgrade/rollback. The later exact-head run [45831217098](https://github.com/kubedoio/rustshare/actions/runs/45831217098) passed the five browser tests, backup/restore, upgrade, and rollback but failed workflow-summary validation on a duplicate `UI_CALENDAR_IMPORT=passed` marker. The duplicate was removed in #569 head `b5a751000f5b8b1d15996db2dff7a01ee70be55e`; non-publishing Pilot Release rerun [45839820086](https://github.com/kubedoio/rustshare/actions/runs/45839820086) passed. After #572 merged, #569 was refreshed onto d0-based head `920a25ca8f021108a0be0f6cbc72adb2b268549a`; Pilot Release run [45849369794](https://github.com/kubedoio/rustshare/actions/runs/45849369794) passed and its exact artifacts are recorded [here](candidate-920a25ca-pr569-disposable-pilot-2026-11-26.md). That does not qualify b9. The refreshed b9-based head `d3f6e442b46dc77b3669253b26cd5a765a096635` completed Pilot Release [45862470683](https://github.com/kubedoio/rustshare/actions/runs/45862470683) successfully; that result is historical for this head. See the [b5a75100 candidate record](candidate-b5a75100-pr569-disposable-pilot-2026-11-26.md) for the successful prior-head run and its artifact digests, and [the earlier candidate record](candidate-308017d4-pr569-disposable-pilot-2026-11-26.md) for prior-run evidence.

Historical exact-main candidate [b3f64684](candidate-b3f64684-disposable-pilot-2026-11-26.md), tied to Pilot Release run 45729254164, passed clean install, three browser tests, failure, backup and fresh-volume restore checks. It is historical; see the top of this page for current protected-main status. Image publication was skipped.

[Candidate 592c9188](candidate-592c9188-disposable-pilot-2026-11-26.md) is the previous exact-main snapshot, tied to run 45717748617.

[Candidate 219d1d18](candidate-219d1d18-disposable-pilot-2026-11-25.md) and [candidate c2157181](candidate-c2157181-disposable-recovery-2026-11-25.md) are historical source snapshots and must not be treated as the current candidate.

See [candidate c2157181 disposable recovery and authorization evidence](candidate-c2157181-disposable-recovery-2026-11-25.md) for the 2026-11-25 synthetic recovery, browser, restore/rollback, and protected-main organization authorization results. It is separate from PR #560 head CI and does not qualify the FWS target or a release.

Product-scope decisions for Mail, Calendar, organization administration, rebranding, and Chat are summarized in [H3 product-scope decisions](h3-product-scope-decisions-2026-11-26.md).

The Pilot Release workflow writes a machine-generated evidence bundle for each
tested revision. The bundle is uploaded as the rustshare-pilot-evidence
workflow artifact and should be retained with the release record.

Expected files include:

- pilot-identity.env and tested-image.env — source SHA, image revision/version
  and config-image ID, tested image archive checksum, workflow/run, deployment
  and configuration identities. The short-retention image transfer artifact
  can expire without losing its recorded digest metadata in the 90-day bundle;
- canonical-smoke.env — product journey result, timestamps, phase and test
  identifiers;
- ui.env, ui-index.html and ui-playwright-results.json — static UI response,
  authenticated browser sign-in/navigation, exact canonical File visibility,
  source/config/run identity and browser test result;
- failure-drills.env and health/authentication failure responses — bounded
  database, storage and invalid-login failure/recovery results;
- persistence.env — post-restart data verification result;
- backup.env — backup creation and structural verification result;
- `*-restore-drill.env` — backup/restore drill result;
- upgrade.env, upgrade-previous.env and upgrade-candidate.env — previous
  release data creation, candidate migration and post-upgrade verification;
- migration.env — representative migration regression result;
- health-*.json — liveness/readiness responses;
- compose-ps.txt, compose-config.sha256 and bounded logs on failure;
- security-sanity.env — automated configuration/security assertions;
- evidence-collection-status.env — proof that evidence collection itself did
  not fail.
- pilot-workflow-summary.env — machine-generated workflow result and explicit
  target-environment readiness boundary.
- clean-install.env — mandatory fresh-PostgreSQL/RustFS-volume bootstrap,
  migration, and first canonical-journey evidence from the Pilot Release
  workflow;
- clean-restart-persistence.env — fresh-volume target-host restart evidence
  when that external clean-install exercise is required.

Smoke reports include the source SHA, build version, deployment/configuration
identity, and GitHub workflow run/attempt when executed by Actions. Restore-drill
reports include the same available identities alongside their isolated Compose
project identity. The canonical smoke report records `SMOKE_FILE_SHA256` for
its uploaded fixture; restart, restore and upgrade verification reports include
`BETA_SMOKE_PERSISTENCE_STATE_VERIFIED=passed` only after the Note checks and
downloaded File checksum match the original fixture. The workflow summary
requires this marker for those three persistence phases. The workflow summary
remains the aggregate result; report identity fields are not a substitute for
checking that every mandatory phase passed. The canonical Notes journey edits
the Markdown H1 independently, renames the note, verifies the H1 remains
unchanged, and the restart/restore/candidate checks assert the renamed note
title and H1 after reloading.

Artifacts must not contain .env contents, passwords, tokens, cookies, private
keys or database credentials. A missing mandatory evidence file makes the
workflow non-pilot-ready even if an earlier test step passed.
The protected-main disposable pilot results for `20b3d75e` and `25e68c60` are
summarized in [`candidate-20b3d75e-disposable-pilot-2026-11-26.md`](candidate-20b3d75e-disposable-pilot-2026-11-26.md)
and [`candidate-25e68c60-disposable-pilot-2026-11-26.md`](candidate-25e68c60-disposable-pilot-2026-11-26.md).

For the FWS target-host exercise, the external evidence bundle is retained at
`/var/backups/rustshare/fws-evidence-20261004` on the deployment host. Its
`pilot-identity.env` binds the reports to the source SHA, candidate image,
deployment identity and Compose configuration hash. The bundle is external to
Git because it contains environment-specific operational records; only
redacted summaries belong in repository documentation.

The 2026-10-06 candidate deployment, FWS backup reference, public canonical
smoke and post-restart persistence reports are retained at
`/var/backups/rustshare/fws-evidence-20261006` on the deployment host. See
[`fws-deployment-2026-10-06.md`](fws-deployment-2026-10-06.md) for the redacted
summary. That target-host exercise is not complete until an independent
second administrator finishes the two-admin recovery rehearsal.
