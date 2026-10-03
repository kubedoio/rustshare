# RustShare Pilot Readiness Report

## Conclusion: NOT READY

The repository-level Pilot Release gate is now green for the exact revision
below, including hosted deployment, the canonical journey,
restart/persistence, dependency failure and recovery, backup/restore, and a
previous-release upgrade. The FWS/Erasmus target environment has not yet been
clean-installed and accepted, so this evidence does not authorize real users.

The deferred FWS configuration and acceptance work is tracked in
[#335](https://github.com/kubedoio/rustshare/issues/335). The repository-level
gate remains useful and authoritative for the tested revision, but it cannot
substitute for validation on the eventual FWS host.

## Revision and run identity

- Assessment source SHA: `748b2fcb517c87331ba3c6491b2afffcfcd618cc`
- Tested candidate revision: `b4b711a4c07bfb66b71b1b3279731008a0055d41`
- Hosted workflow: [Pilot Release run 37148082023](https://github.com/kubedoio/rustshare/actions/runs/37148082023)
- Hosted artifact: `rustshare-pilot-evidence-b4b711a4c07bfb66b71b1b3279731008a0055d41`
- Hosted deployment identity: `github-actions-37148082023-1`
- Image identity: `pilot-b4b711a4c07b`, OCI revision
  `b4b711a4c07bfb66b71b1b3279731008a0055d41`
- Configuration identity: Compose SHA-256
  `22261f873bce35e00712c86167a51f096712fbb1c6f835b527f4e4a40ddaf25e`
- Target environment: isolated GitHub Actions Docker Compose runner; the
  FWS/Erasmus pilot environment remains unverified.

## Verification performed in this worktree

- SQLX_OFFLINE=true cargo test -p rustshare-server --test notes_test --no-run
  — passed.
- SQLX_OFFLINE=true cargo clippy -p rustshare-server --test notes_test
  --all-features -- -D warnings — passed.
- Frontend Notes/editor suites — 2 files and 24 tests passed.
- cargo fmt --all --check, bash -n for the changed smoke/restore scripts,
  git diff --check, YAML parsing and bash syntax checks for all workflow run
  blocks — passed.
- Pilot Compose merge with non-secret placeholder configuration — passed.
- Exact-candidate local deployment, canonical journey and UI index reachability
  — passed; evidence: `/tmp/rustshare-final-candidate-evidence/`.
- Clean production Compose profile startup with an empty Compose environment —
  liveness, readiness, UI reachability and secure-cookie configuration passed;
  evidence: `/tmp/rustshare-local-production-evidence-20261003T134045Z/`.
  Authenticated production-profile traffic was not claimed over plain HTTP;
  the profile correctly requires HTTPS termination.
- Exact-candidate application restart and persistence verification — passed;
  evidence: `/tmp/rustshare-final-candidate-evidence/`.
- Exact-candidate dependency failure/recovery and invalid-configuration
  checks — passed; evidence: `/tmp/rustshare-final-candidate-evidence/`.
- Exact-candidate real Postgres/RustFS backup and isolated restore drill —
  passed; evidence under `/tmp/rustshare-final-candidate-evidence/` and its
  backup bundle.
- Exact-candidate previous-release upgrade — passed from
  `v0.8.0-alpha.5` at
  `d28816cbedaa877026cdd8bcb57274c54694e40a` to the candidate, including
  representative migration checks and post-upgrade persistence verification;
  evidence: `/tmp/rustshare-final-candidate-upgrade-evidence/`.
- Hosted exact-candidate Pilot Release run 37148082023 — passed. The retained
  artifact contains `pilot-identity.env`, canonical and restart persistence
  results, health/readiness responses, dependency failure drills, backup
  verification, restore verification, previous-release and candidate upgrade
  results, migration status, security sanity status, redacted logs and the
  machine-generated workflow summary.
- Hosted evidence values: `WORKFLOW_RESULT=passed`,
  `BETA_SMOKE_STATUS=passed`, `DATABASE_FAILURE_STATUS=passed`,
  `STORAGE_FAILURE_STATUS=passed`, `BACKUP_STATUS=passed`,
  `UPGRADE_STATUS=passed`, `MIGRATION_STATUS=passed`,
  `SECRET_LOG_SCAN=passed` and
  `EVIDENCE_COLLECTION_STATUS=passed`.
- The hosted run exercised an isolated GitHub Actions environment, not the
  FWS/Erasmus target. The shared local Docker stack was left untouched.

## Gate status

| Gate | Status | Evidence / gap |
| --- | --- | --- |
| Deploy/configure | HOSTED EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | Run 37148082023 bound the image and Compose configuration to the tested SHA; clean FWS installation is not evidenced. |
| Authenticate/authorize | HOSTED EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | The smoke journey covers password login, protected resources, sharing and negative permission-aware search; target identity configuration remains unverified. |
| Canonical product journey | HOSTED EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | Files, Notes, search, authorization negative checks, sharing, audit, chat status and logout passed in the retained artifact. |
| Restart/persistence | HOSTED EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | Real Postgres/RustFS data survived application restart and re-authentication in the hosted run. |
| Backup/restore | HOSTED EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | The workflow created a real backup bundle, restored it in an isolated drill and verified the journey afterward. |
| Upgrade | HOSTED EXACT-CANDIDATE VERIFIED WITH SCOPE LIMIT; FWS NOT VERIFIED | `v0.8.0-alpha.5` data passed through candidate startup, migration checks and post-upgrade persistence verification. |
| Health/diagnostics | HOSTED EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | `/health` and `/health/ready` probes, dependency outages and recovery responses were retained in the artifact. |
| Security sanity | HOSTED EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | Invalid configuration failed closed, production secure-cookie configuration passed, and the artifact secret-log scan passed; target TLS/OIDC configuration remains unverified. |

## Unresolved risks

1. **High — FWS clean install and recovery unverified.** Tracked by
   [#335](https://github.com/kubedoio/rustshare/issues/335). Repository scripts are
   not evidence of the target host’s Docker, TLS, secret-store and volume
   behavior. Workaround: execute the runbook on an isolated FWS host before
   inviting users.
2. **Medium — upgrade compatibility scope.** The proof covers only
   `v0.8.0-alpha.5` to the current candidate, not every prior release.
   Workaround: use the pre-upgrade backup and previously verified image as the
   rollback path; require the workflow’s immutable candidate build before
   approval.
3. **Medium — identity-provider acceptance.** Password login is the baseline;
   OIDC acceptance depends on the chosen FWS provider and credentials. Do not
   claim OIDC readiness until that provider is exercised.

This report must be regenerated from workflow evidence before changing the
conclusion to READY WITH ACCEPTED LIMITATIONS or READY.
