# Pilot evidence

Protected main `af8f695b66b14526be1eff0b027b353f807dce38` completed exact-head Pilot Release [run 45762571913](https://github.com/kubedoio/rustshare/actions/runs/45762571913) and Integration Tests [run 45762572054](https://github.com/kubedoio/rustshare/actions/runs/45762572054), both successfully. The Pilot Release passed 17 migrations, the canonical journey, 3 expected/0 unexpected browser results, backup/restore, installed upgrade and fresh-volume rollback; Integration Tests passed all four jobs including Buzz Live Conformance and Chat Product Acceptance. See [candidate af8f695b exact-main evidence](candidate-af8f695b-disposable-pilot-2026-11-26.md). Publication was skipped; this is not release artifact or FWS acceptance.

Historical exact-main candidate [3bd949b6](candidate-3bd949b6-disposable-pilot-2026-11-26.md), tied to run 45752680910 on former main `3bd949b6e8f5bd9ea078c0d9e76ebb9194764e2f`, is superseded for current-main claims by the af8f695b candidate above. Its clean-install, browser, backup and restore results remain historical evidence.

The latest exact-candidate disposable Pilot Release run is [candidate c613cc7b](candidate-c613cc7b-disposable-pilot-2026-11-26.md), tied to PR #569 head `c613cc7b368d0c61d9bef99c053ce3e3c7071f2f` and run 45746539135. It passed clean install, the four browser journeys including 100-user administration, backup, and isolated restore. Installed upgrade/rollback remains unqualified for this first-clean-install baseline; FWS/operator acceptance remains outstanding. Image publication was skipped.

PR #569's manual disposable-pilot run on exact head `308017d476fded112f1b2bf8d7026a2c6d20a9b4` passed the 100-user browser journey and fresh-volume restore drill. Its tested image was retained as a run artifact, and image publication was skipped. This branch-specific evidence does not qualify current protected main or installed upgrade/rollback. See [PR #569 candidate evidence](candidate-308017d4-pr569-disposable-pilot-2026-11-26.md) for exact artifact digests and limits.

Historical exact-main candidate [b3f64684](candidate-b3f64684-disposable-pilot-2026-11-26.md), tied to Pilot Release run 45729254164, passed clean install, three browser tests, failure, backup and fresh-volume restore checks. It is superseded for current-main claims by candidate af8f695b above. Image publication was skipped.

[Candidate 592c9188](candidate-592c9188-disposable-pilot-2026-11-26.md) is the previous exact-main snapshot, tied to run 45717748617.

[Candidate 219d1d18](candidate-219d1d18-disposable-pilot-2026-11-25.md) and [candidate c2157181](candidate-c2157181-disposable-recovery-2026-11-25.md) are historical source snapshots and must not be treated as the current candidate.

See [candidate c2157181 disposable recovery and authorization evidence](candidate-c2157181-disposable-recovery-2026-11-25.md) for the 2026-11-25 synthetic recovery, browser, restore/rollback, and protected-main organization authorization results. It is separate from PR #560 head CI and does not qualify the FWS target or a release.

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
