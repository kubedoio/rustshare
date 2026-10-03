# RustShare Pilot Readiness Report

## Conclusion: NOT READY

This report is intentionally conservative. Local isolated runtime validation
covers the canonical journey, restart/persistence, dependency failure and
recovery, backup/restore, and a previous-release upgrade using an image built
from the candidate revision. The FWS target environment has not yet produced a
complete authoritative evidence bundle.

## Revision and run identity

- Assessment source SHA: 748b2fcb517c87331ba3c6491b2afffcfcd618cc
- Candidate revision: the DCO-signed commit containing this report; resolve
  its exact SHA with `git rev-parse HEAD`
- Candidate status: committed locally with DCO sign-off; not pushed or run in
  the authoritative hosted workflow
- Target environment: local isolated Docker Compose validation for the
  candidate; FWS/Erasmus pilot — not exercised by this repository run
- Historical workflow run: [37072211603](https://github.com/kubedoio/rustshare/actions/runs/37072211603)
  succeeded for this SHA, but it ran the older build/Compose/readiness path
  and produced no pilot evidence artifact.
- Deployment/configuration identity: local image OCI revision, Compose config
  hash and workflow identity are emitted by the pilot workflow; the hosted
  candidate identity is pending.

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
- The historical GitHub run for this SHA passed only its older Compose
  startup/readiness and image-publication jobs; it did not run the expanded
  pilot gates and had no uploaded evidence bundle.
- The authoritative GitHub Actions Pilot Release workflow and clean FWS
  installation were not executed in this worktree. The running shared Docker
  stack was left untouched.

## Gate status

| Gate | Status | Evidence / gap |
| --- | --- | --- |
| Deploy/configure | LOCAL EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | The candidate image and Compose profile start cleanly with documented secret/configuration inputs locally; clean FWS installation is not evidenced. |
| Authenticate/authorize | LOCAL EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | The smoke journey covers password login, protected resources, sharing and negative permission-aware search locally. |
| Canonical product journey | LOCAL EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | Files, Notes, search, authorization negative checks, sharing, audit, chat status and logout passed locally. No authoritative hosted artifact exists yet. |
| Restart/persistence | LOCAL EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | Real Postgres/RustFS data survived backend restart and re-authentication locally. |
| Backup/restore | LOCAL EXACT-CANDIDATE VERIFIED; FWS NOT VERIFIED | `scripts/backup-stack.sh` plus `scripts/run-restore-drill.sh` passed with real database/object state locally. |
| Upgrade | LOCAL EXACT-CANDIDATE VERIFIED WITH SCOPE LIMIT; FWS NOT VERIFIED | `v0.8.0-alpha.5` data passed through candidate startup, migration checks and persistence verification locally; hosted immutable-image evidence is pending. |
| Health/diagnostics | PRESENT IN CODE; NOT VERIFIED | /health and /health/ready distinguish liveness/readiness; failure probes and retained responses are required. |
| Security sanity | LOCAL PARTIAL; FWS NOT VERIFIED | Invalid configuration fails closed and the production profile enables secure cookies locally; target TLS termination and OIDC configuration remain unverified. |

## Unresolved risks

1. **High — hosted exact release evidence missing.** A pilot operator cannot
   yet point to a retained workflow artifact proving the exact source/image/
   config combination passed all mandatory gates. Local evidence is useful but
   is not a GitHub workflow artifact. Workaround: run the updated Pilot Release
   workflow and retain its artifact; until then the pilot is not approved.
2. **High — FWS clean install and recovery unverified.** Repository scripts are
   not evidence of the target host’s Docker, TLS, secret-store and volume
   behavior. Workaround: execute the runbook on an isolated FWS host before
   inviting users.
3. **Medium — upgrade compatibility scope.** The proof covers only
   `v0.8.0-alpha.5` to the current candidate, not every prior release.
   Workaround: use the pre-upgrade backup and previously verified image as the
   rollback path; require the workflow’s immutable candidate build before
   approval.
4. **Medium — identity-provider acceptance.** Password login is the baseline;
   OIDC acceptance depends on the chosen FWS provider and credentials. Do not
   claim OIDC readiness until that provider is exercised.

This report must be regenerated from workflow evidence before changing the
conclusion to READY WITH ACCEPTED LIMITATIONS or READY.
