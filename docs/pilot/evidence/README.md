# Pilot evidence

The Pilot Release workflow writes a machine-generated evidence bundle for each
tested revision. The bundle is uploaded as the rustshare-pilot-evidence
workflow artifact and should be retained with the release record.

Expected files include:

- pilot-identity.env — source SHA, image revision/version, workflow/run,
  deployment and configuration identities;
- canonical-smoke.env — product journey result, timestamps, phase and test
  identifiers;
- ui.env and ui-index.html — frontend reachability result and the served UI
  entry document;
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

Artifacts must not contain .env contents, passwords, tokens, cookies, private
keys or database credentials. A missing mandatory evidence file makes the
workflow non-pilot-ready even if an earlier test step passed.

For the FWS target-host exercise, the external evidence bundle is retained at
`/var/backups/rustshare/fws-evidence-20261004` on the deployment host. Its
`pilot-identity.env` binds the reports to the source SHA, candidate image,
deployment identity and Compose configuration hash. The bundle is external to
Git because it contains environment-specific operational records; only
redacted summaries belong in repository documentation.
