# RustShare FWS Pilot Runbook

This runbook is for the pilot operator. It assumes a single-host Docker
Compose deployment and the password-login baseline. Use the exact Git commit
or immutable backend image recorded in the evidence bundle.

## Prerequisites

- Linux host with Docker Engine and Docker Compose plugin 2.24.4 or later
  (`!override` is used by the FWS candidate override);
- repository checkout at the exact tested revision, or access to the exact
  immutable backend image;
- for GitHub-hosted acceptance, authenticated GitHub CLI access with permission
  to dispatch the workflow and read its run/artifacts;
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

### FWS load-balancer host

The FWS load balancer terminates TLS. Keep the application host's nginx bound
to its private interface on port 80; do not publish PostgreSQL or RustFS. Load
the exact tested image artifact (not a local rebuild), verify its archive
checksum from `tested-image.env`, then verify its embedded source revision:

~~~bash
set -euo pipefail
archive="rustshare-backend-pilot.tar"
expected_archive_sha="$(awk -F= '$1 == "ARCHIVE_SHA256" {print $2}' tested-image.env)"
printf '%s  %s\n' "${expected_archive_sha}" "${archive}" | sha256sum --check -
sudo -n docker load --input "${archive}"
candidate_sha="$(awk -F= '$1 == "SOURCE_SHA" {print $2}' tested-image.env)"
candidate_tag="rustshare-backend:pilot-${candidate_sha:0:12}"
sudo -n docker tag rustshare-backend:pilot "${candidate_tag}"
actual_sha="$(sudo -n docker image inspect --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' "${candidate_tag}")"
test "${actual_sha}" = "${candidate_sha}"

export RUSTSHARE_BACKEND_IMAGE="${candidate_tag}"
export FWS_PRIVATE_BIND_ADDRESS="REPLACE_WITH_HOST_PRIVATE_IP"
export RUSTSHARE_BACKEND_PULL_POLICY=never
compose=(sudo -n --preserve-env=RUSTSHARE_BACKEND_IMAGE,FWS_PRIVATE_BIND_ADDRESS,RUSTSHARE_BACKEND_PULL_POLICY docker compose -p rustshare \
  -f docker-compose.yml \
  -f docker-compose.prod.yml \
  -f docker-compose.pilot.yml \
  -f docker-compose.fws-candidate.yml)
"${compose[@]}" config --quiet
"${compose[@]}" up -d backend nginx
~~~

Keep the previous backend image tag and a verified pre-upgrade backup. Check
the host's container image ID and revision label after deployment. Record the
archive checksum and runtime ID separately: the FWS Docker 29.6.1 import kept
the candidate's source/version labels and all 23 layer DiffIDs but reported a
host-local image ID different from the CI `IMAGE_ID` field.

The FWS host cannot route outbound HTTPS to its own public hostname. Run the
canonical smoke from an operator runner with HTTPS access to the public origin,
or use a separately documented private-route test; do not interpret a failure
of host-to-public DNS/hairpin routing as an application health failure.

On a shared host, choose unused loopback ports for the disposable clean-install
exercise before starting it (`RUSTSHARE_POSTGRES_HOST_PORT`,
`RUSTSHARE_RUSTFS_HOST_PORT` and `RUSTSHARE_RUSTFS_CONSOLE_HOST_PORT`). Do not
reuse the production Compose project name or volumes. The operator must set a
durable `RUSTSHARE_ADMIN_PASSWORD` before the first start; do not depend on a
container-local bootstrap password for a pilot deployment.

For an immutable candidate image, set the digest recorded by the release
evidence and add the pilot image override:

~~~bash
candidate_digest="sha256:REPLACE_WITH_CANDIDATE_DIGEST"
export RUSTSHARE_BACKEND_IMAGE="ghcr.io/kubedoio/rustshare-backend@${candidate_digest}"
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

### Mandatory independent two-admin rehearsal

Complete this with a second operator who did not deploy the candidate, using
their own secondary administrator account. Do this before inviting pilot users.
Use a dedicated disposable non-admin test account with no user data; never use
a real pilot account.

1. The second operator reviews this runbook, signs in with their own admin
   credentials, and records the candidate SHA, deployment identity, and UTC
   start time.
2. In a separate browser profile, sign in as the disposable user and open a
   protected RustShare page. Keep this session open for the stale-session test.
3. As the second administrator, reset the disposable user's password in
   **Admin → Users**. Confirm the old browser session is rejected on its next
   protected request, then authenticate in a fresh session with the new
   password and confirm permitted access works.
4. Disable the disposable user in **Admin → Users**. Confirm a fresh login is
   rejected and the previously authenticated session can no longer access the
   protected page. Leave the account disabled unless cleanup is explicitly
   required and approved.
5. Record pass/fail for each check and retain only redacted diagnostics. If any
   check fails, stop: do not start the bounded cohort; preserve the account
   state and escalate through the project owner.

Evidence must identify the candidate SHA, deployment, UTC time, approved
independent-operator and secondary-admin account references, disposable account
reference, each result, and diagnostic artifact paths. Do not record passwords,
cookies, tokens, session IDs, or personal contact details in the repository.

There is no supported self-service or operator reset if every administrator
credential is lost. Prevent that condition with two independently controlled
administrator accounts and the documented secret-store procedure; any
emergency database-level recovery is an exceptional, separately approved
incident action, not a normal pilot step.

For a committed candidate revision, trigger the authoritative workflow and
retain its artifact. Dispatch from a branch or tag that resolves to the
reviewed commit; `workflow_dispatch --ref` accepts a branch or tag, not an
arbitrary commit SHA. Filter the resulting run by the exact commit and verify
its event, creation time, and head SHA before watching or downloading it:

~~~bash
candidate_ref="REPLACE_WITH_CANDIDATE_BRANCH_OR_TAG"
candidate_sha="$(git rev-parse "${candidate_ref}^{commit}")"
dispatch_started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
gh workflow run pilot-release.yml --ref "${candidate_ref}"
gh run list --workflow pilot-release.yml --event workflow_dispatch \
  --commit "${candidate_sha}" --limit 10 \
  --json databaseId,headSha,createdAt,status,url
run_id="REPLACE_WITH_RUN_ID_CREATED_AFTER_DISPATCH"
gh run view "${run_id}" --json headSha,event,status,conclusion,url
gh run watch "${run_id}" --exit-status
gh run download "${run_id}" \
  --name "rustshare-pilot-evidence-${candidate_sha}" \
  --dir "./pilot-evidence-${candidate_sha}"
grep -Fx "SOURCE_SHA=${candidate_sha}" \
  "./pilot-evidence-${candidate_sha}/pilot-workflow-summary.env"
grep -Fx 'WORKFLOW_RESULT=passed' \
  "./pilot-evidence-${candidate_sha}/pilot-workflow-summary.env"
~~~

Use the `createdAt` output and `dispatch_started_at` to select the run created
by this dispatch; confirm `event=workflow_dispatch` and the exact `headSha` in
`gh run view` before proceeding. If no unique matching run appears, stop and
resolve it rather than downloading the newest unrelated run. A workflow
dispatch validates the repository workflow on a disposable GitHub runner; it
does not prove the FWS deployment. Its push-only image publication job is
skipped. Do not call a revision pilot-ready until the downloaded artifact
contains the mandatory evidence files, matches the candidate SHA, and records
`WORKFLOW_RESULT=passed`; then validate the deployed FWS instance separately.

## Backup

Use the same Compose model that started the pilot. For a source-built
production deployment:

~~~bash
export COMPOSE_FILE=docker-compose.yml:docker-compose.prod.yml
~~~

For an FWS candidate image, include both `docker-compose.pilot.yml` and
`docker-compose.fws-candidate.yml` in that value. Ensure
`RUSTSHARE_BACKEND_IMAGE`, `FWS_PRIVATE_BIND_ADDRESS`, and
`RUSTSHARE_BACKEND_PULL_POLICY=never` are set in the operator environment. The
backup and restore scripts honor this explicit Compose file set.

~~~bash
backup_root="/secure/backups/rustshare"
sudo -n --preserve-env=COMPOSE_FILE,RUSTSHARE_BACKEND_IMAGE,FWS_PRIVATE_BIND_ADDRESS,RUSTSHARE_BACKEND_PULL_POLICY \
  ./scripts/backup-stack.sh "${backup_root}"
# Set backup_path to the exact path printed by “Backup created at …”.
backup_path="${backup_root}/REPLACE_WITH_TIMESTAMP"
sudo -n ./scripts/verify-backup-bundle.sh "${backup_path}"
~~~

The bundle contains the PostgreSQL dump, RustFS data snapshot, deployment
configuration and manifest/checksums. It intentionally excludes secrets.
Back up the exact secret set separately in the operator secret store,
including any OIDC credentials. Preserve user/browser Chat key backups using
the Chat procedure; they are not server-side durable state.

## Restore

Prefer a non-destructive drill first:

The operator host needs the `flock` command (provided by `util-linux`; it is
available by default on supported Ubuntu hosts).

~~~bash
backup_path="/secure/backups/rustshare/REPLACE_WITH_TIMESTAMP"
sudo -n ./scripts/run-restore-drill.sh "${backup_path}"
~~~

The drill takes an operating-system lock for its `DRILL_PROJECT_NAME` and
fails before Docker access if another drill is already using that project.
Wait for the active process to exit before retrying; do not remove its lock
file or use another project name with the same published host ports while it
is running. Different projects require distinct host-port overrides to run
without port conflicts. The lock is released automatically when its process
exits; an old, unlocked lock file does not prevent a later run.

For an approved in-place recovery, stop traffic, restore the external secrets,
then run:

~~~bash
set -euo pipefail
# FWS candidate stack: keep these identical to the active deployment.
export COMPOSE_FILE=docker-compose.yml:docker-compose.prod.yml:docker-compose.pilot.yml:docker-compose.fws-candidate.yml
export RUSTSHARE_BACKEND_IMAGE="rustshare-backend:pilot-REPLACE_WITH_SOURCE_SHA_PREFIX"
export FWS_PRIVATE_BIND_ADDRESS="REPLACE_WITH_HOST_PRIVATE_IP"
export RUSTSHARE_BACKEND_PULL_POLICY=never
backup_path="/secure/backups/rustshare/REPLACE_WITH_TIMESTAMP"
sudo -n --preserve-env=COMPOSE_FILE,RUSTSHARE_BACKEND_IMAGE,FWS_PRIVATE_BIND_ADDRESS,RUSTSHARE_BACKEND_PULL_POLICY \
  ./scripts/restore-stack.sh "${backup_path}"
curl -fsS https://app.kubedo.io/health/ready
BASE_URL=https://app.kubedo.io \
  ADMIN_EMAIL=... ADMIN_PASSWORD=... \
  VIEWER_EMAIL=... VIEWER_PASSWORD=... \
  PILOT_SOURCE_SHA=... PILOT_BUILD_VERSION=... \
  PILOT_DEPLOYMENT_ID=... PILOT_CONFIG_ID=... \
  ./scripts/run-beta-smoke.sh
~~~

The post-restore journey must verify a known pilot record, not only that the
containers are running.

For non-FWS deployments, set `COMPOSE_FILE` to the files used by that stack and
run the restore helper with that deployment's Docker privileges.

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
- OIDC authentication fails while dependencies are ready: inspect the backend
  `failure_stage` log field. `runtime_config_database_load` and
  `client_secret_decryption` indicate server configuration/secret-key
  problems; `provider_discovery`, `web_token_exchange`, or
  `mobile_token_exchange` indicate identity-provider/network failures; login
  state and user stages indicate persistence/provisioning failures. Public
  responses stay generic, and logs intentionally omit raw provider errors,
  issuer URLs, token bodies, and error descriptions.
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
`sudo` (direct Docker socket access is not granted). Use `sudo -n` for the
Docker-backed backup/restore helpers above; preserve the exported Compose
settings when invoking them. The generic diagnostics block assumes an operator
with Docker access; for FWS, use the candidate-aware commands below. On other
hosts, use the site's configured Docker access. From the FWS deployment
directory, include the candidate override when inspecting or operating the live
stack:

~~~bash
cd /opt/rustshare
sudo -n docker compose -f docker-compose.yml \
  -f docker-compose.prod.yml -f docker-compose.fws-candidate.yml ps
sudo -n docker compose -f docker-compose.yml \
  -f docker-compose.prod.yml -f docker-compose.fws-candidate.yml logs --tail=200 backend
~~~

Do not add the operator to the Docker group solely to avoid `sudo`; Docker
socket access is effectively root-equivalent. The public hostname's load
balancer terminates TLS; do not expose the host's private address in public
evidence.

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

The accepted target-host evidence for the 2026-10-06 candidate is retained at
`/var/backups/rustshare/fws-evidence-20261006`. It includes the identity,
canonical journey, restart/persistence, backup/restore, upgrade and bounded
dependency-failure reports. Never copy the host `.env` into that evidence
directory.
