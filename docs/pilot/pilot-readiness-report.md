# RustShare Pilot Readiness Report

## Conclusion: READY WITH ACCEPTED LIMITATIONS

The exact candidate has now passed the repository Pilot Release gate, the
public FWS load-balanced journey, a fresh-volume clean deployment, restart
and persistence checks, real backup/restore, bounded dependency failure and
recovery checks, and the supported previous-release upgrade path.

RustShare is ready for the bounded FWS password-login pilot under the
limitations below. This decision does not authorize Bund expansion or imply
OIDC-provider acceptance.

## Revision and evidence identity

- Source SHA: `c3648fdb918ac2b7ff59952bc91e26cce66d5e70`
- Candidate build: `rustshare-backend:pilot-c3648fdb918a`
- Candidate image: `sha256:02862e272beadd035471808a13c25a8b05cc4ba13a5f531ecf7d38acca2072e8`
- FWS deployment identity: `fws-app-kubedo-io`
- FWS Compose identity: `3ee7e5b3e790ac48bccd76910107a759c919578975a64781c8b638b4b49793eb`
- Public validation URL: `https://app.kubedo.io`
- Host evidence directory: `/var/backups/rustshare/fws-evidence-20261004`
- Clean-install evidence: `fws-clean-install.env`, config identity
  `4b39c2c0b76d965d0ce0631dbb060503f7ed4d699e32011df54ce5269ae48db3`
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
| Deploy/configure | PASS | Exact image/source/config identity is recorded; the public load balancer terminates TLS and forwards to the private host. |
| Clean install | PASS | Fresh host directory, `.env.example`, `scripts/pre-flight.sh`, durable admin password before first start, fresh Postgres/RustFS volumes, canonical smoke and cleanup. Evidence: `fws-clean-install.env`. |
| Authenticate/authorize | PASS | `fws-canonical-final.env`: admin and viewer login, protected file access, viewer denial, sharing and audit assertions. |
| Canonical product journey | PASS | `fws-canonical-final.env`: UI reachability, folders, Files upload/download, Notes create/read/update, search, sharing/revocation and logout. |
| Restart/persistence | PASS | `fws-restart-final.env` and `fws-clean-restart-persistence.env`: application stop/start followed by re-authentication and Note/File verification. |
| Backup | PASS | Bundle `/var/backups/rustshare/20261004T000917Z`; structural verification passed for PostgreSQL, RustFS, configuration, manifest and SHA-256 checksums. |
| Restore | PASS | `fws-restore-persistence.env` and restore-drill report: isolated Compose restore followed by representative data verification. |
| Upgrade | PASS for supported path | The authoritative Pilot Release workflow upgrades `v0.8.0-alpha.5` to the candidate. The FWS candidate also verified Note/File data after replacement and 102 migrations through `20261002100000`. |
| Health/diagnostics | PASS | `/health` remained 200 while PostgreSQL was stopped; `/health/ready` returned 503 with `database connectivity failed`, then recovered. RustFS failure returned 503 with `object storage check failed`, then recovered. |
| Observability | PASS | Startup/migration/dependency logs, liveness/readiness component diagnostics, smoke phase reports and redacted failure evidence are retained. |
| Security sanity | PASS for bounded pilot | Secure cookies, public HTTPS origin, protected routes and secret-redaction checks passed. OIDC provider acceptance is intentionally outside the password-login pilot. |

## Evidence sequence

1. The candidate was deployed from the exact source SHA and served through
   the existing load balancer; TLS terminates at the load balancer and the
   private host serves HTTP on its bound interface.
2. The existing `scripts/run-beta-smoke.sh` passed against the public FWS
   URL.
3. A separate fresh-volume deployment was created from the documented
   bootstrap procedure and passed the same smoke plus restart persistence.
4. A real PostgreSQL/RustFS/configuration backup was created and structurally
   verified.
5. The backup was restored into an isolated Compose project and the
   representative pilot data was verified.
6. The supported previous-release upgrade was exercised by the authoritative
   workflow; the target candidate’s migration and post-upgrade persistence
   were also verified on the host.
7. PostgreSQL and RustFS outage/recovery probes demonstrated the distinction
   between liveness and dependency readiness.

## Accepted limitations and follow-up

1. **Medium — password-login pilot baseline.** Affected operation: OIDC
   authentication. Evidence: password login passed; no FWS OIDC provider was
   configured for this run. Workaround: use the documented password-login
   accounts for the bounded pilot. Follow-up: run OIDC acceptance when the
   provider, redirect URI and client credentials are provisioned.
2. **Medium — existing-database credential recovery.** A retained database
   required a controlled password-hash rotation because its existing admin
   credential did not match the copied first-boot `.env`. Workaround: set and
   preserve durable credentials before first start, as demonstrated by the
   clean-install evidence. Follow-up: provide a supported application-level
   password-reset procedure instead of direct DB intervention.
3. **Low — unsupported old image behavior.** The retained host image
   `rustshare-backend:latest` stopped its full smoke during search, but it is
   not the supported previous release. The supported `v0.8.0-alpha.5` upgrade
   path passed in the authoritative workflow. Follow-up: remove or label the
   stale image so operators do not select it as an upgrade source.

## Final decision

**READY WITH ACCEPTED LIMITATIONS** — the bounded FWS password-login pilot may
proceed using the exact candidate, the runbook, the existing pilot workflow,
the verified backup, and the documented recovery path. Do not begin
Bund-specific expansion under this milestone.
