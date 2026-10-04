# RustShare FWS Pilot Runbook

This runbook is for the pilot operator. It assumes a single-host Docker
Compose deployment and the password-login baseline. Use the exact Git commit
or immutable backend image recorded in the evidence bundle.

## Prerequisites

- Linux host with Docker Engine and the Compose plugin;
- repository checkout at the exact tested revision, or access to the exact
  immutable backend image;
- DNS/HTTPS termination for the pilot hostname;
- an operator-managed secret store for .env and any OIDC secrets;
- durable backup storage outside the application host.

Do not expose PostgreSQL or RustFS administrative ports publicly. Production
traffic must use HTTPS, with the edge proxy forwarding
X-Forwarded-Proto: https; the production profile enables secure session
cookies.

## Clean deployment

~~~bash
cp .env.example .env
./scripts/pre-flight.sh
# Set RUSTSHARE_PUBLIC_URL, RUSTSHARE_ADMIN_EMAIL and a durable
# RUSTSHARE_ADMIN_PASSWORD before the first start.
docker compose -f docker-compose.yml -f docker-compose.prod.yml config
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d
~~~

Record the Git SHA, image digest, host/environment name and a redacted config
hash. Never paste .env into evidence or support tickets.

On a shared host, choose unused loopback ports for the disposable clean-install
exercise before starting it (`RUSTSHARE_POSTGRES_HOST_PORT`,
`RUSTSHARE_RUSTFS_HOST_PORT` and `RUSTSHARE_RUSTFS_CONSOLE_HOST_PORT`). Do not
reuse the production Compose project name or volumes. The operator must set a
durable `RUSTSHARE_ADMIN_PASSWORD` before the first start; do not depend on a
container-local bootstrap password for a pilot deployment.

For an immutable candidate image, set the digest recorded by the release
evidence and add the pilot image override:

~~~bash
export RUSTSHARE_BACKEND_IMAGE=ghcr.io/kubedoio/rustshare-backend@sha256:<digest>
export RUSTSHARE_BACKEND_PULL_POLICY=always
docker compose -f docker-compose.yml -f docker-compose.prod.yml \
  -f docker-compose.pilot.yml config
docker compose -f docker-compose.yml -f docker-compose.prod.yml \
  -f docker-compose.pilot.yml up -d
~~~

## Start and health

~~~bash
docker compose -f docker-compose.yml -f docker-compose.prod.yml ps
curl -fsS https://pilot.example/health
curl -fsS https://pilot.example/health/ready
./scripts/run-beta-smoke.sh
~~~

/health means the HTTP process/reverse proxy is alive. /health/ready must
return 200 before pilot traffic is allowed; its component response identifies
database, object storage, event delivery and auth/session failures.

The `outbox` component is informational and does not change the overall
readiness decision. It can briefly report unhealthy while a dispatch tick is
running. Recheck after the tick; if the component remains unhealthy or an
included pilot operation depends on delayed projections, inspect backend logs
and verify the affected event-driven behavior before declaring recovery.

## Stop and restart

~~~bash
docker compose -f docker-compose.yml -f docker-compose.prod.yml stop backend nginx
docker compose -f docker-compose.yml -f docker-compose.prod.yml start backend nginx
curl -fsS https://pilot.example/health/ready
~~~

For a planned host restart, stop the stack only after a verified backup. Do
not use docker compose down -v on the pilot host except as part of an approved
restore procedure: it deletes named volumes.

## Pilot validation

The authoritative acceptance path is the repository Pilot Release workflow
(.github/workflows/pilot-release.yml). For a running deployment, the same
product journey is available as:

~~~bash
BASE_URL=https://pilot.example \
ADMIN_EMAIL=... ADMIN_PASSWORD=... \
VIEWER_EMAIL=... VIEWER_PASSWORD=... \
REPORT_DIR=/secure/evidence \
./scripts/run-beta-smoke.sh
~~~

Keep the generated report and, on failure, the Compose status/log bundle.

## User accounts, recovery, and offboarding

Before the cohort starts, verify two independent administrator accounts. Keep
the secondary account's strong credential in the approved operator secret
store, separate from the primary administrator's normal sign-in. This avoids
depending on direct database edits if one administrator is locked out.

An administrator creates pilot accounts from **Admin → Users**. Set the
least-privileged role and required workspace access; do not grant administrator
privileges to pilot users. Deliver each initial password through an approved
secure channel separate from this repository, issue tracker, and ordinary
email. Ask the user to change it immediately in **Settings → Password**. The
application does not currently enforce a first-login password change.

To recover a user's forgotten password, a second administrator opens that
user in **Admin → Users**, sets a new password, and securely relays it to the
user. The admin password-update operation revokes the user's active sessions
and device tokens and records an admin action. The user signs in with the new
password and changes it in Settings. Do not put the password in support logs,
tickets, or this runbook.

For offboarding, use **Admin → Users → Disable**. Disabling a user revokes
active sessions and device tokens. Prefer disable over delete unless the data
owner has approved deletion and its file/object cleanup implications.

There is no supported self-service or operator reset if every administrator
credential is lost. Prevent that condition with two independently controlled
administrator accounts and the documented secret-store procedure; any
emergency database-level recovery is an exceptional, separately approved
incident action, not a normal pilot step.

For a committed candidate revision, trigger the authoritative workflow and
retain its artifact:

~~~bash
gh workflow run pilot-release.yml --ref <candidate-commit-or-branch>
gh run list --workflow pilot-release.yml --limit 1
gh run watch <run-id> --exit-status
gh run download <run-id> --name rustshare-pilot-evidence-<candidate-sha>
~~~

Do not call a revision pilot-ready until the downloaded artifact contains the
mandatory evidence files and the workflow result is successful.

## Backup

Use the same Compose model that started the pilot. For a source-built
production deployment:

~~~bash
export COMPOSE_FILE=docker-compose.yml:docker-compose.prod.yml
~~~

For the immutable image variant, include `docker-compose.pilot.yml` in that
value as well. The backup and restore scripts honor this explicit Compose
file set.

~~~bash
./scripts/backup-stack.sh /secure/backups/rustshare
./scripts/verify-backup-bundle.sh /secure/backups/rustshare/<timestamp>
~~~

The bundle contains the PostgreSQL dump, RustFS data snapshot, deployment
configuration and manifest/checksums. It intentionally excludes secrets.
Back up the exact secret set separately in the operator secret store,
including any OIDC credentials. Preserve user/browser Chat key backups using
the Chat procedure; they are not server-side durable state.

## Restore

Prefer a non-destructive drill first:

~~~bash
./scripts/run-restore-drill.sh /secure/backups/rustshare/<timestamp>
~~~

For an approved in-place recovery, stop traffic, restore the external secrets,
then run:

~~~bash
./scripts/restore-stack.sh /secure/backups/rustshare/<timestamp>
curl -fsS https://pilot.example/health/ready
./scripts/run-beta-smoke.sh
~~~

The post-restore journey must verify a known pilot record, not only that the
containers are running.

## Upgrade

1. Take and verify a backup.
2. Validate the candidate image/configuration in an isolated restore-drill
   environment.
3. Deploy the candidate immutable image during the maintenance window; the
   backend applies forward SQLx migrations at startup.
4. Check logs for migration success, then check /health/ready and run the
   pilot smoke.
5. If the candidate fails, stop traffic and restore the pre-upgrade backup and
   previously verified image. Do not attempt an unsupported downgrade against
   a migrated database.

The repository migration regression path is:

~~~bash
scripts/test-application-migration.sh
~~~

## Logs and diagnostics

~~~bash
docker compose -f docker-compose.yml -f docker-compose.prod.yml logs \
  --tail=200 backend nginx postgres rustfs
docker compose -f docker-compose.yml -f docker-compose.prod.yml ps
curl -i https://pilot.example/health
curl -i https://pilot.example/health/ready
~~~

Interpretation:

- /health fails: edge/process problem;
- readiness reports database unhealthy: PostgreSQL/network/migration issue;
- readiness reports object_storage unhealthy: RustFS/S3 endpoint, bucket or
  credentials issue;
- login returns 401/5xx while readiness is healthy: authentication/config or
  identity-provider issue;
- upload/download fails while database is healthy: object storage or upload
  limit/configuration issue.

Do not collect or share cookies, authorization headers, request bodies with
passwords, .env, private keys or raw secret-bearing configuration.

## FWS load-balancer deployment

For the FWS pilot host, the load balancer terminates TLS and forwards the
public hostname to the host’s private HTTP listener. The backend therefore
uses the public HTTPS origin for generated URLs and secure cookies, while the
host-side Compose edge binds only to the private interface.

Required settings include:

~~~dotenv
RUSTSHARE_PUBLIC_URL=https://app.kubedo.io
VITE_API_URL=https://app.kubedo.io/api
VITE_WS_URL=wss://app.kubedo.io/api
SERVER_HOST=0.0.0.0
SESSION_COOKIE_SECURE=true
RUSTSHARE_SESSION_COOKIE_SECURE=true
~~~

The load balancer must forward the original HTTPS scheme (or the equivalent
trusted forwarded-proto configuration) and WebSocket upgrades. Do not add a
second public TLS terminator on the private host without changing this
topology review.

The validated FWS deployment used the repository base and production Compose
files plus a host-local candidate override for the immutable backend image and
private edge bind. Keep that override outside Git if it contains site-specific
addresses; record its SHA-256 in the evidence identity. Validate the public
surface, not only host-local HTTP:

For the current FWS operator account, Docker commands require noninteractive
`sudo` (direct Docker socket access is not granted). From the deployment
directory, include the host-local candidate override when inspecting or
operating the live stack:

~~~bash
cd /opt/rustshare
sudo -n docker compose -f docker-compose.yml \
  -f docker-compose.prod.yml -f docker-compose.fws-candidate.yml ps
sudo -n docker compose -f docker-compose.yml \
  -f docker-compose.prod.yml -f docker-compose.fws-candidate.yml logs --tail=200 backend
~~~

Do not add the operator to the Docker group solely to avoid `sudo`; Docker
socket access is effectively root-equivalent. Apply the same privilege
requirement to all Compose commands above and to backup/restore helper scripts
that invoke Docker. The public hostname's load balancer terminates TLS; do not
expose the host's private address in public evidence.

~~~bash
curl -fsS https://app.kubedo.io/health
curl -fsS https://app.kubedo.io/health/ready
BASE_URL=https://app.kubedo.io \
  ADMIN_EMAIL=... ADMIN_PASSWORD=... \
  VIEWER_EMAIL=... VIEWER_PASSWORD=... \
  PILOT_SOURCE_SHA=... PILOT_BUILD_VERSION=... \
  PILOT_DEPLOYMENT_ID=... PILOT_CONFIG_ID=... \
  scripts/run-beta-smoke.sh
~~~

The accepted target-host evidence for this run is retained at
`/var/backups/rustshare/fws-evidence-20261004`. It includes the identity,
canonical journey, restart/persistence, backup/restore, upgrade and bounded
dependency-failure reports. Never copy the host `.env` into that evidence
directory.
