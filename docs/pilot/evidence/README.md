# Pilot evidence

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
