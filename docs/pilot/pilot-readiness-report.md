# RustShare Pilot Readiness Report

## Conclusion: NOT READY

The exact candidate passes the repository Pilot Release gate and the live
FWS load-balanced deployment passes the canonical user journey,
restart/persistence, backup/restore, health failure/recovery, and candidate
upgrade data-verification checks. The milestone remains **NOT READY** because
the production FWS volumes were preserved during deployment rather than
reconstructed from a documented clean-install procedure, and the retained
previous-image upgrade run exposed an older-image search incompatibility.

No real-user pilot should start until those two limitations are accepted or
closed and the authoritative workflow is rerun against the final deployment.

## Revision and evidence identity

- Source SHA: `c3648fdb918ac2b7ff59952bc91e26cce66d5e70`
- Candidate build: `rustshare-backend:pilot-c3648fdb918a`
- Candidate image: `sha256:02862e272beadd035471808a13c25a8b05cc4ba13a5f531ecf7d38acca2072e8`
- FWS deployment identity: `fws-app-kubedo-io`
- FWS Compose identity: `3ee7e5b3e790ac48bccd76910107a759c919578975a64781c8b638b4b49793eb`
- Public validation URL: `https://app.kubedo.io`
- Host evidence directory: `/var/backups/rustshare/fws-evidence-20261004`
- Repository workflow evidence:
  - [Pilot Release run 37156360337](https://github.com/kubedoio/rustshare/actions/runs/37156360337)
  - [Integration Tests run 37154953625](https://github.com/kubedoio/rustshare/actions/runs/37154953625)
  - [final branch checks run 37158465897](https://github.com/kubedoio/rustshare/actions/runs/37158465897)

The machine-generated host identity is
`/var/backups/rustshare/fws-evidence-20261004/pilot-identity.env`. It contains
no passwords, tokens, cookies, private keys, or database credentials.

## Gate status

| Gate | Result | Evidence |
| --- | --- | --- |
| Deploy/configure | PASS with limitation | Candidate image and exact source/config identity are recorded in `pilot-identity.env`; deployment used the existing persistent volumes, not a clean production install. |
| Authenticate/authorize | PASS | `fws-canonical-final.env`: admin and viewer login, protected file access, viewer denial, sharing and audit assertions. |
| Canonical product journey | PASS | `fws-canonical-final.env`: UI reachability, folders, Files upload/download, Notes create/read/update, search, sharing/revocation and logout. |
| Restart/persistence | PASS | `fws-restart-final.env`: application stop/start followed by re-authentication and Note/File verification. |
| Backup | PASS | Bundle `/var/backups/rustshare/20261004T000917Z`; structural verification passed for PostgreSQL, RustFS, configuration, manifest and SHA-256 checksums. |
| Restore | PASS | `fws-restore-persistence.env` and restore-drill report: isolated Compose restore followed by representative data verification. |
| Upgrade | PARTIAL | The old retained image populated data; the candidate then verified the Note/File after replacement and 102 migrations through `20261002100000`. The old image’s full smoke stopped at its search assertion, so this is not a clean full previous-release journey. |
| Health/diagnostics | PASS | `/health` remained 200 while PostgreSQL was stopped; `/health/ready` returned 503 with `database connectivity failed`, then recovered. RustFS failure returned 503 with `object storage check failed`, then recovered. |
| Security sanity | PASS with follow-up | Secure cookies, public HTTPS origin, redacted logs, protected routes and readiness behavior were reviewed; OIDC provider acceptance remains outside this run. |
| Clean install | NOT VERIFIED | The live FWS deployment was an in-place candidate switch protected by a pre-switch backup. The isolated restore/upgrade projects were disposable tests, not a clean install from the operator runbook. |

## Evidence sequence

1. Candidate deployment was built from the exact source SHA and served through
   the existing load balancer; TLS terminates at the load balancer and the
   private host serves HTTP on its bound port.
2. The canonical public journey passed using the existing
   `scripts/run-beta-smoke.sh` workflow.
3. The application was stopped and started without removing Postgres or
   RustFS volumes; the same Note and File were verified afterward.
4. A real PostgreSQL/RustFS/configuration backup was created and structurally
   verified.
5. The backup was restored into an isolated Compose project and the
   representative pilot data was verified.
6. A retained older backend image was exercised in an isolated project. Its
   data survived replacement by the exact candidate image and candidate
   migrations, but the old image’s search assertion failed before its full
   smoke completed.
7. PostgreSQL and RustFS outage/recovery probes demonstrated the distinction
   between liveness and dependency readiness.

## Unresolved risks and follow-up

1. **High — clean-install acceptance.** Affected operation: future rebuild or
   replacement of the FWS pilot. Evidence: `CLEAN_INSTALL_RESULT` is
   `not-executed-on-production-volumes` in `pilot-identity.env`. Workaround:
   retain the verified backup and follow the runbook on a disposable host.
   Follow-up: execute the documented clean-install procedure on replacement
   FWS infrastructure and attach its evidence to issue [#335](https://github.com/kubedoio/rustshare/issues/335).
2. **High — previous-image smoke compatibility.** Affected operation:
   upgrades from the retained `rustshare-backend:latest` image. Evidence:
   `20261004T002129Z-1282058-beta-smoke.env` records failure in the search
   phase, while `fws-upgrade-candidate-verification.env` proves candidate
   persistence for the data created before that failure. Workaround: use the
   exact candidate workflow and pre-upgrade backup; do not treat the old image
   as a fully accepted pilot baseline. Follow-up: identify and test a formally
   supported previous release image, or document the compatibility boundary.
3. **Medium — credential recovery procedure.** A retained database had an
   admin email/password mismatch, requiring a controlled password-hash
   rotation before the pilot journey could run. Evidence: initial public smoke
   authentication failures and the later passing reports. Workaround: rotate
   credentials before pilot use and preserve the external secret set. Follow-up:
   provide a supported operator password-reset procedure rather than direct DB
   intervention.
4. **Medium — identity-provider scope.** Password authentication is verified;
   the FWS OIDC provider, redirect URI and client credentials were not
   exercised. Workaround: use password login for the bounded pilot only.
   Follow-up: run the OIDC acceptance path when the provider is provisioned.

## Final decision

**NOT READY** — the exact candidate is operationally promising and has strong
repository plus target-host evidence, but the mandatory clean-install gate and
the full previous-image upgrade journey are still open.
