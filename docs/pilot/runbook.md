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
