# Customer Alpha Operations Runbook

This is the supported operational contract for a controlled, single-host
Elembra web Alpha. It supplements the technical [Deployment Guide](../DEPLOYMENT.md)
and [Backup/Restore Runbook](backup-restore.md); operators should not need
repository history or developer tooling.

## Support boundary

Supported:

- one Linux host with Docker Engine and Docker Compose;
- Elembra PostgreSQL and RustFS;
- bundled Buzz PostgreSQL, Redis, dedicated RustFS, relay, and managed observer;
- externally terminated HTTPS/WSS;
- one configured OIDC provider or password login where explicitly accepted.

Not supported by this Alpha claim: Kubernetes, HA, multi-region, zero-downtime
upgrades, mobile/desktop production use, shared Elembra/Buzz RustFS, or an
enterprise security certification.

## Minimum host requirements

Use a clean Linux host with:

- 4 vCPU;
- 8 GiB RAM;
- 50 GiB free SSD space before importing customer data;
- Docker Engine 27+ and Compose v2.30+;
- a filesystem supporting Docker volumes and at least 10 GiB free space for
  temporary backup/restore work.

These are an Alpha baseline, not a capacity guarantee. Measure the workload
and increase CPU, memory, and storage before approaching the disk threshold.

## Installation and first start

Install the published deployment bundle at its recorded release SHA. Do not
build Rust, install npm packages, compile Buzz, or use floating image tags.
The release bundle must provide an immutable `RUSTSHARE_BACKEND_IMAGE` digest
and the Buzz digest from `config/buzz-compatibility.env`.

```bash
RUSTSHARE_BACKEND_IMAGE=ghcr.io/kubedoio/rustshare-backend@sha256:<published-digest> \
  ./scripts/elembra.sh init --with-chat --release
./scripts/elembra.sh up
./scripts/elembra.sh status
```

Configure DNS and the documented reverse proxy before inviting users. Use
HTTPS for the web/API origin and WSS for Chat. Keep backend and relay listener
ports bound to the host/proxy boundary; do not expose PostgreSQL, Redis, or
RustFS publicly.

## OIDC

Configure the provider's issuer, client ID/secret, redirect URI, and required
claims in `.env` through the secrets manager. The redirect URI must exactly
match the public HTTPS origin. Validate first login, logout, expiry, disabled
users, wrong issuer/audience, and IdP outage before accepting users. OIDC
authenticates the Elembra Principal; it never replaces the user's encrypted
Buzz identity or the relay's signing authority.

## Administration and offboarding

Use the admin UI/API to invite users, enable Chat, configure the Application,
and disable a Principal. Never use `psql`, direct Buzz API calls, or SQL edits
for normal administration. Disabling a Principal must revoke Elembra access
and enqueue Buzz membership revocation; verify that reads and publishes fail
closed immediately.

## Health, logs, and support

```bash
./scripts/elembra.sh status
./scripts/elembra.sh support-bundle /secure/support-output
```

The support bundle contains sanitized Compose state, health responses, pinned
compatibility metadata, disk information, and bounded logs. It does not include
`.env`, `.elembra/chat.env`, container environment inspection, credentials, or
private keys. Transfer the resulting archive through the approved support
channel only.

Monitor `/health`, `/health/ready`, `/metrics`, Buzz relay readiness, observer
readiness/reconnects, database/object-store availability, HTTP 5xx rate,
authorization failures, and observation/outbox lag. Page on backend/Buzz
unavailability, a growing observer lag, database/object-store failure, or a
full disk; warn on elevated authentication failures and retryable backlog.

## Backup and restore

Back up Elembra PostgreSQL/RustFS, Buzz PostgreSQL/RustFS, deployment
configuration, and persistent deployment identities. `.env` and
`.elembra/chat.env` belong in the encrypted secrets backup, not the normal
diagnostic bundle. Never regenerate deployment identities during restore.

For the Alpha safety model, enable maintenance mode or stop writers, then:

```bash
./scripts/backup-stack.sh /secure/backups/elembra
./scripts/verify-backup-bundle.sh /secure/backups/elembra/<timestamp>
./scripts/run-restore-drill.sh /secure/backups/elembra/<timestamp>
```

Keep encrypted off-host copies and test restoration on an isolated host. The
backup procedure does not claim atomic distributed snapshots across Elembra
and Buzz; quiescing writers avoids cross-store skew.

## Upgrade and rollback

1. Read the release notes and record the exact image digests.
2. Take and verify a complete backup.
3. Test the upgrade against a copy in the restore-drill environment.
4. Stop writers, update only the documented release artifact, and start.
5. Run health, product smoke, Chat conformance, and restore checks.

Buzz relay binary rollback for the current relay-v0.2.1 schema is classified
as safe without a database restore. This does not automatically make the full
Elembra release rollback safe: if an application migration is irreversible,
restore the pre-upgrade snapshot instead.

## Known Alpha limitations

This milestone is a controlled pilot gate, not a general production claim.
Obsidian/vault sync remains excluded while issue #236 is open. Device
management is limited to encrypted identity export/import; a full device
administration portal is not part of Alpha.
