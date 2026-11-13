#!/usr/bin/env bash

# Disposable full-stack restore and relay image-switch drill. This owns a
# uniquely named Compose project and child worktree; it never uses the caller's
# .env, .elembra state, Docker project, or volumes.
set -Eeuo pipefail
umask 077

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/chat-recovery-drill-safety.sh"
SOURCE_SHA="$(git -C "${ROOT}" rev-parse HEAD)"
OLD_IMAGE="${ELEMBRA_RECOVERY_OLD_RELAY_IMAGE:-ghcr.io/kubedoio/buzz-elembra@sha256:2b6c4b2a1800984283f1893a407997b2d5101b01fd0195669ee5deb84814906a}"
NEW_IMAGE="${ELEMBRA_RECOVERY_NEW_RELAY_IMAGE:-ghcr.io/kubedoio/buzz-elembra@sha256:38c362bb37d15ea879634b0f56a867401df4e4e0074a84382dff6b07bb578b4e}"
[[ "${OLD_IMAGE}" =~ @sha256:[0-9a-f]{64}$ && "${NEW_IMAGE}" =~ @sha256:[0-9a-f]{64}$ && "${OLD_IMAGE}" != "${NEW_IMAGE}" ]] || {
	echo "Both relay images must be distinct immutable OCI digest references." >&2
	exit 2
}
git -C "${ROOT}" diff --quiet && git -C "${ROOT}" diff --cached --quiet || {
	echo "Run this drill from a clean committed source checkout." >&2
	exit 2
}

require() { command -v "$1" >/dev/null || { echo "Missing command: $1" >&2; exit 2; }; }
for command in docker git curl npm node python3 sha256sum openssl; do require "${command}"; done

RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-${RANDOM}"
PROJECT="elembra-g0-recovery-${RUN_ID,,}"
PUBLISHED_EVIDENCE_DIR="$(create_recovery_evidence_dir "${ELEMBRA_RECOVERY_EVIDENCE_DIR:-}" "${TMPDIR:-/tmp}" "elembra-g0-recovery-evidence-${RUN_ID,,}")"
TEMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/${PROJECT}.XXXXXX")"
chmod 700 "${TEMP_DIR}"
TEST_ROOT="${TEMP_DIR}/project"
BACKUP_ROOT="${TEMP_DIR}/backups"
EVIDENCE_DIR="${TEMP_DIR}/evidence"
mkdir -m 700 "${BACKUP_ROOT}" "${EVIDENCE_DIR}"
WORKTREE_ADDED=false
STACK_CREATED=false
PHASE=setup

NGINX_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
RELAY_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
POSTGRES_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
RUSTFS_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
CONSOLE_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
HEALTH_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
METRICS_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
ADMIN_PASSWORD="$(openssl rand -hex 32)"

compose() {
	docker compose --project-name "${PROJECT}" \
		-f docker-compose.yml -f docker-compose.alpha.yml \
		-f docker-compose.dogfood.yml -f docker-compose.g0-recovery.yml "$@"
}

cleanup() {
	local status=$?
	local cleanup_failed=false
	trap - EXIT
	cd "${ROOT}"
	if [[ "${STACK_CREATED}" == true ]]; then
		if [[ ! -d "${TEST_ROOT}" ]]; then
			echo "Expected recovery project worktree is missing; cannot verify cleanup for ${PROJECT}." >&2
			cleanup_failed=true
		elif ! cleanup_recovery_project "${ROOT}" "${PROJECT}" "${TEST_ROOT}" "${OLD_IMAGE}"; then
			cleanup_failed=true
		fi
		if [[ "${cleanup_failed}" == false ]]; then WORKTREE_ADDED=false; fi
	fi
	if [[ "${STACK_CREATED}" != true && "${WORKTREE_ADDED}" == true ]]; then
		if git -C "${ROOT}" worktree remove --force "${TEST_ROOT}"; then
			WORKTREE_ADDED=false
		else
			echo "Could not remove isolated worktree ${TEST_ROOT}; preserving it for cleanup." >&2
			cleanup_failed=true
		fi
	fi
	if [[ "${cleanup_failed}" == true ]]; then
		if ! record_recovery_cleanup_failure "${EVIDENCE_DIR}" "${PROJECT}" "${TEST_ROOT}" "${TEMP_DIR}"; then
			rm -f -- "${EVIDENCE_DIR}/result.env"
			echo "Could not write cleanup-failure record; removed any private PASS marker." >&2
		fi
		echo "Cleanup did not verify; retained isolated project/worktree at ${TEST_ROOT} and private run data at ${TEMP_DIR}." >&2
		exit 1
	fi
	if (( status == 0 )); then
		if ! publish_recovery_evidence "${EVIDENCE_DIR}" "${PUBLISHED_EVIDENCE_DIR}" \
			result.env api-before.sha256 api-after-restore.sha256 chat-channel.json chat-conversation.json \
			history-before.json history-after-restore.json history-new.json history-old.json; then
			echo "Could not publish without overwriting; retained private evidence at ${TEMP_DIR}." >&2
			exit 1
		fi
	else
		cat >"${EVIDENCE_DIR}/RUN_FAILURE.txt" <<EOF
Run did not qualify: phase=${PHASE}, exit=${status}, source=${SOURCE_SHA}.
No PASS is asserted. Disposable project cleanup was verified before this report was published.
EOF
		if ! publish_recovery_evidence "${EVIDENCE_DIR}" "${PUBLISHED_EVIDENCE_DIR}" RUN_FAILURE.txt; then
			echo "Could not publish failure summary; retained private evidence at ${TEMP_DIR}." >&2
			exit 1
		fi
	fi
	rm -rf -- "${TEMP_DIR}"
	echo "Secret-free recovery evidence: ${PUBLISHED_EVIDENCE_DIR}"
	if (( status != 0 )); then
		echo "Recovery drill failed during ${PHASE} (exit ${status}); disposable project cleanup attempted." >&2
	fi
	exit "${status}"
}
trap cleanup EXIT

PHASE=source-checkout
git -C "${ROOT}" worktree add --detach "${TEST_ROOT}" "${SOURCE_SHA}" >/dev/null
WORKTREE_ADDED=true
# The drill's private umask protects generated credentials; container builds
# still need the committed source readable by their non-root runtime users.
chmod -R a+rX "${TEST_ROOT}"

cat >"${TEST_ROOT}/docker-compose.g0-recovery.yml" <<YAML
services:
  postgres:
    ports: !override ["127.0.0.1:${POSTGRES_PORT}:5432"]
  rustfs:
    ports: !override ["127.0.0.1:${RUSTFS_PORT}:9000", "127.0.0.1:${CONSOLE_PORT}:9001"]
  backend:
    ports: !override ["127.0.0.1:${RELAY_PORT}:${RELAY_PORT}", "127.0.0.1:${HEALTH_PORT}:8088", "127.0.0.1:${METRICS_PORT}:9102"]
  nginx:
    ports: !override ["127.0.0.1:${NGINX_PORT}:80"]
  buzz-relay:
    image: \${RECOVERY_RELAY_IMAGE:?set an immutable recovery relay image}
    environment:
      BUZZ_BIND_ADDR: 0.0.0.0:${RELAY_PORT}
YAML

cp "${TEST_ROOT}/.env.example" "${TEST_ROOT}/.env"
cat >>"${TEST_ROOT}/.env" <<EOF
COMPOSE_PROJECT_NAME=${PROJECT}
ELEMBRA_COMPOSE_EXTRA_FILE=docker-compose.g0-recovery.yml
RUSTSHARE_ADMIN_EMAIL=admin@localhost
RUSTSHARE_ADMIN_PASSWORD=${ADMIN_PASSWORD}
BUZZ_RELAY_WS=ws://localhost:${RELAY_PORT}
BUZZ_RELAY_URL=ws://localhost:${RELAY_PORT}
RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL=ws://localhost:${RELAY_PORT}
RUSTSHARE_CHAT_DEPLOYMENT_RELAY_URL=ws://localhost:${RELAY_PORT}
EOF
chmod 600 "${TEST_ROOT}/.env"

export RECOVERY_RELAY_IMAGE="${OLD_IMAGE}"
cd "${TEST_ROOT}"
set -a
. ./.env
. ./config/buzz-compatibility.env
set +a
compose --profile chat config --format json >"${TEMP_DIR}/compose-config.json"
python3 - "${TEMP_DIR}/compose-config.json" "${OLD_IMAGE}" "${RELAY_PORT}" <<'PY'
import json,sys
config=json.load(open(sys.argv[1])); image=sys.argv[2]; port=int(sys.argv[3])
services=config["services"]
assert services["buzz-relay"]["image"] == image, "Compose did not select the requested immutable relay image"
assert services["buzz-relay"]["environment"]["BUZZ_BIND_ADDR"] == f"0.0.0.0:{port}"
for name, service in services.items():
    for published in service.get("ports", []):
        assert published.get("host_ip") == "127.0.0.1", f"{name} publishes outside loopback"
relay_host_port=next(p for p in services["backend"]["ports"] if int(p["target"]) == port)
assert int(relay_host_port["published"]) == port
for name in ("backend", "chat-observer", "buzz-relay"):
    env=services[name].get("environment", {})
    for key in ("BUZZ_RELAY_WS", "BUZZ_RELAY_URL", "RELAY_URL", "RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL", "RUSTSHARE_CHAT_DEPLOYMENT_RELAY_URL"):
        value=env.get(key)
        if value is not None:
            assert value == f"ws://localhost:{port}", f"{name}.{key} would split the Host-derived community"
PY

for resource in containers volumes networks; do
	case "${resource}" in
		containers) existing="$(docker ps -aq --filter "label=com.docker.compose.project=${PROJECT}")" ;;
		volumes) existing="$(docker volume ls -q --filter "label=com.docker.compose.project=${PROJECT}")" ;;
		networks) existing="$(docker network ls -q --filter "label=com.docker.compose.project=${PROJECT}")" ;;
	esac
	[[ -z "${existing}" ]] || { echo "Refusing to reuse existing ${resource} for ${PROJECT}." >&2; exit 2; }
done

cd "${TEST_ROOT}"
PHASE=initialization
STACK_CREATED=true
RECOVERY_RELAY_IMAGE="${OLD_IMAGE}" ./scripts/elembra.sh init --with-chat >"${EVIDENCE_DIR}/init.log" 2>&1
PHASE=source-build-startup
RECOVERY_RELAY_IMAGE="${OLD_IMAGE}" ./scripts/elembra.sh up >"${EVIDENCE_DIR}/up.log" 2>&1

container_id() { docker compose --project-name "${PROJECT}" -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml -f docker-compose.g0-recovery.yml --profile chat ps -q "$1"; }
BACKEND_IMAGE_ID="$(docker inspect --format '{{.Image}}' "$(container_id backend)")"
OBSERVER_IMAGE_ID="$(docker inspect --format '{{.Image}}' "$(container_id chat-observer)")"
RELAY_IMAGE_ID="$(docker inspect --format '{{.Image}}' "$(container_id buzz-relay)")"
ACCEPTANCE_IMAGE_DIGESTS="$(python3 - "${BACKEND_IMAGE_ID}" "${OBSERVER_IMAGE_ID}" "${OLD_IMAGE}" <<'PY'
import json,sys
print(json.dumps({"backendImageId":sys.argv[1],"observerImageId":sys.argv[2],"buzzRelayImage":sys.argv[3]},separators=(",",":")))
PY
)"
export ACCEPTANCE_GIT_SHA="${SOURCE_SHA}" ACCEPTANCE_STACK="${PROJECT}" ACCEPTANCE_IMAGE_DIGESTS

BASE_URL="http://127.0.0.1:${NGINX_PORT}"
RELAY_URL="ws://localhost:${RELAY_PORT}"
ADMIN_PASSWORD="${ADMIN_PASSWORD}"
BUZZ_CHANNEL_ID="$(python3 -c 'import uuid; print(uuid.uuid4())')"
BUZZ_CHANNEL2_ID="$(python3 -c 'import uuid; print(uuid.uuid4())')"
export BASE_URL RELAY_URL ADMIN_PASSWORD
export BUZZ_CHANNEL_ID BUZZ_CHANNEL2_ID

PHASE=alpha-dogfood
ELEMBRA_API="${BASE_URL}/api/v1" ELEMBRA_ORIGIN="${BASE_URL}" \
	BUZZ_RELAY_WS="${RELAY_URL}" BUZZ_RELAY_HEALTH_URL="${RELAY_URL/ws:/http:}" \
	./scripts/run-alpha-dogfood.sh >"${EVIDENCE_DIR}/alpha-dogfood.log" 2>&1

PHASE=browser-channel-acceptance
npm ci --prefix frontend --no-audit --no-fund >"${EVIDENCE_DIR}/npm-ci.log" 2>&1
npx --prefix frontend playwright install chromium >"${EVIDENCE_DIR}/playwright-install.log" 2>&1
CHANNEL_EVIDENCE_RELATIVE="$(python3 - "${TEST_ROOT}" "${EVIDENCE_DIR}/chat-channel.json" <<'PY'
import os,sys
print(os.path.relpath(sys.argv[2], sys.argv[1]))
PY
)"
ACCEPTANCE_BASE_URL="${BASE_URL}" ACCEPTANCE_RELAY_URL="${RELAY_URL}" \
	ACCEPTANCE_ADMIN_PASSWORD="${ADMIN_PASSWORD}" \
	ACCEPTANCE_EVIDENCE="${CHANNEL_EVIDENCE_RELATIVE}" \
	./scripts/run-chat-channel-acceptance.sh >"${EVIDENCE_DIR}/channel.log" 2>&1
CHANNEL_CASES="$(python3 - "${EVIDENCE_DIR}/chat-channel.json" 13 <<'PY'
import json,sys
evidence=json.load(open(sys.argv[1])); matrix=evidence.get("matrix",[])
if len(matrix) != int(sys.argv[2]) or any(row.get("status") != "PASS" for row in matrix):
    raise SystemExit(f"channel browser matrix incomplete or failed: {len(matrix)}/{sys.argv[2]}")
print(len(matrix))
PY
)"

PHASE=browser-conversation-acceptance
(cd frontend && ACCEPTANCE_BASE_URL="${BASE_URL}" ACCEPTANCE_RELAY_URL="${RELAY_URL}" \
	ACCEPTANCE_ADMIN_PASSWORD="${ADMIN_PASSWORD}" \
	ACCEPTANCE_EVIDENCE="${EVIDENCE_DIR}/chat-conversation.json" \
	npx playwright test tests/chat-conversation-acceptance.e2e.ts --workers=1) \
	>"${EVIDENCE_DIR}/conversation.log" 2>&1
CONVERSATION_CASES="$(python3 - "${EVIDENCE_DIR}/chat-conversation.json" 9 <<'PY'
import json,sys
evidence=json.load(open(sys.argv[1])); matrix=evidence.get("matrix",[])
if len(matrix) != int(sys.argv[2]) or any(row.get("status") != "PASS" for row in matrix):
    raise SystemExit(f"conversation browser matrix incomplete or failed: {len(matrix)}/{sys.argv[2]}")
print(len(matrix))
PY
)"

COMPOSE_PROJECT_NAME="${PROJECT}"
export COMPOSE_PROJECT_NAME
set -a
. ./.env
. ./config/buzz-compatibility.env
. ./.elembra/chat.env
set +a
export ELEMBRA_COMPOSE_EXTRA_FILE=docker-compose.g0-recovery.yml
export RECOVERY_RELAY_IMAGE="${OLD_IMAGE}"

BUZZ_RECOVERY_CHANNEL_ID="${BUZZ_CHANNEL_ID}"
export BUZZ_RECOVERY_CHANNEL_ID
node frontend/scripts/chat-relay-history-probe.mjs >"${EVIDENCE_DIR}/history-before.json"
BASELINE_HISTORY_HASH="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["eventSetSha256"])' "${EVIDENCE_DIR}/history-before.json")"

FILE_FIXTURE="$(docker compose --project-name "${PROJECT}" -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml -f docker-compose.g0-recovery.yml --profile chat exec -T postgres psql -U rustshare -d rustshare -Atc "SELECT f.id || '|' || u.email || '|' || f.content_hash FROM files f JOIN users u ON u.id=f.owner_id WHERE f.name='alpha-e2e.md' ORDER BY f.created_at DESC LIMIT 1")"
IFS='|' read -r FILE_ID FILE_OWNER_EMAIL FILE_DB_HASH <<<"${FILE_FIXTURE}"
[[ "${FILE_ID}" =~ ^[0-9a-f-]{36}$ && -n "${FILE_OWNER_EMAIL}" && "${FILE_DB_HASH}" =~ ^[0-9a-f]{64}$ ]] || {
	echo 'Could not resolve the disposable Files attachment fixture.' >&2; exit 1;
}
EXPECTED_FILE_HASH="$(sha256sum AGENTS.md | awk '{print $1}')"
[[ "${FILE_DB_HASH}" == "${EXPECTED_FILE_HASH}" ]] || { echo 'Files metadata hash does not match AGENTS.md.' >&2; exit 1; }
EVE_EMAIL="$(docker compose --project-name "${PROJECT}" -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml -f docker-compose.g0-recovery.yml --profile chat exec -T postgres psql -U rustshare -d rustshare -Atc "SELECT email FROM users WHERE username LIKE 'alpha_eve_%' ORDER BY created_at DESC LIMIT 1")"
[[ -n "${EVE_EMAIL}" ]] || { echo 'Could not resolve disposable unauthorized user.' >&2; exit 1; }

capture_api_projection() {
	local email="$1" result_path="$2" cookie="${TEMP_DIR}/api-cookie-${email}" login_body="${TEMP_DIR}/api-login-${email}.json" status
	python3 - "${email}" <<'PY' >"${login_body}"
import json,sys
print(json.dumps({"email": sys.argv[1], "password": "alpha-user-pass-2026"}))
PY
	status="$(curl -sS -o /dev/null -w '%{http_code}' -c "${cookie}" -H 'content-type: application/json' --data-binary "@${login_body}" "${BASE_URL}/api/v1/auth/login")"
	[[ "${status}" == 2* ]] || { echo "API projection login failed (${status})." >&2; return 1; }
	curl -fsS -b "${cookie}" "${BASE_URL}/api/v1/applications/chat/channels" >"${TEMP_DIR}/api-channels.json"
	curl -fsS -b "${cookie}" "${BASE_URL}/api/v1/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}&limit=64" >"${TEMP_DIR}/api-messages.json"
	python3 - "${TEMP_DIR}/api-channels.json" "${TEMP_DIR}/api-messages.json" "${BUZZ_CHANNEL_ID}" "${EVIDENCE_DIR}/history-before.json" >"${result_path}" <<'PY'
import hashlib,json,sys
channels=json.load(open(sys.argv[1])); response=json.load(open(sys.argv[2])); channel_id=sys.argv[3]
history=json.load(open(sys.argv[4])); messages=response.get("messages", [])
channel=next((c for c in channels if c.get("channel_id")==channel_id),None)
if channel is None: raise SystemExit("Chat API did not authorize the test channel")
signed={event_id:pubkey for event_id,pubkey in history["eventAuthors"]}
projection=[]
for message in messages:
    if message.get("channel_id") != channel_id: continue
    event_id=message.get("event_id"); author=message.get("author_pubkey")
    if not event_id or not author or signed.get(event_id) != author:
        raise SystemExit("Chat API author/event does not match the signed relay event")
    projection.append([message.get("message_id"),event_id,author,message.get("event_created_at"),message.get("thread_root_id"),message.get("attachments")])
if not projection: raise SystemExit("Chat API returned no test-channel history")
projection.sort(key=lambda row:(row[1],row[0] or ""))
channel_projection=[channel.get(k) for k in ("channel_id","name","channel_kind","channel_type","visibility","member")]
print(hashlib.sha256(json.dumps([channel_projection,projection],separators=(",",":")).encode()).hexdigest())
PY
}

compose_args=(--project-name "${PROJECT}" -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml -f docker-compose.g0-recovery.yml --profile chat)
compose_call() { docker compose "${compose_args[@]}" "$@"; }

query_hash() {
	local service="$1" user="$2" database="$3" query="$4"
	hash_recovery_command_output compose_call exec -T "${service}" psql -U "${user}" -d "${database}" -Atc "${query}"
}

volume_hash() {
	local service="$1" container volume
	container="$(compose_call ps -q "${service}")" || return 1
	[[ -n "${container}" ]] || { echo "No container for ${service}." >&2; return 1; }
	volume="$(docker inspect --format '{{range .Mounts}}{{if eq .Destination "/data"}}{{.Name}}{{end}}{{end}}' "${container}")" || return 1
	[[ -n "${volume}" ]] || { echo "No named data volume on ${service}." >&2; return 1; }
	hash_recovery_command_output docker run --rm -v "${volume}:/data:ro" alpine@sha256:ce64758a109eb420d874a118f87920e625e12d3634e03b4a5573fd9f6e5d3507 \
		sh -c 'cd /data && find . -type f -print0 | sort -z | xargs -0 -r sha256sum'
}

snapshot() {
	local name="$1"
	local buzz_events buzz_channels buzz_members identity_bindings workspace_communities users observed_events files
	local buzz_events_hash buzz_channels_hash buzz_members_hash identity_bindings_hash workspace_communities_hash
	local users_hash observed_events_hash files_hash rustfs_objects_hash buzz_rustfs_objects_hash
	buzz_events="SELECT COALESCE(jsonb_agg(jsonb_build_array(community_id,encode(id,'hex'),encode(pubkey,'hex'),created_at,kind,tags,content,encode(sig,'hex'),channel_id,deleted_at) ORDER BY community_id,created_at,encode(id,'hex')),'[]'::jsonb)::text FROM events"
	buzz_channels="SELECT COALESCE(jsonb_agg(jsonb_build_array(community_id,id,name,channel_type::text,visibility::text,encode(created_by,'hex'),archived_at,deleted_at) ORDER BY community_id,id),'[]'::jsonb)::text FROM channels"
	buzz_members="SELECT COALESCE(jsonb_agg(jsonb_build_array(community_id,channel_id,encode(pubkey,'hex'),role::text,joined_at,removed_at) ORDER BY community_id,channel_id,pubkey),'[]'::jsonb)::text FROM channel_members"
	identity_bindings="SELECT COALESCE(jsonb_agg(jsonb_build_array(binding_id,tenant_id,principal_id,buzz_pubkey,status,verified_at,revoked_at) ORDER BY tenant_id,principal_id,binding_id),'[]'::jsonb)::text FROM chat_identity_bindings"
	workspace_communities="SELECT COALESCE(jsonb_agg(jsonb_build_array(mapping_id,tenant_id,workspace_id,community_id,relay_url,active,relay_pubkey,membership_protocol_version) ORDER BY tenant_id,workspace_id,mapping_id),'[]'::jsonb)::text FROM chat_workspace_communities"
	users="SELECT COALESCE(jsonb_agg(jsonb_build_array(id,username,email,password_hash,display_name,is_admin,tenant_id,disabled_at,credentials_revoked_at,credential_version) ORDER BY tenant_id,id),'[]'::jsonb)::text FROM users"
	observed_events="SELECT COALESCE(jsonb_agg(jsonb_build_array(tenant_id,workspace_id,event_id,message_id,event_type,supersedes_event_id,community_id,channel_id,channel_kind,thread_root_id,author_pubkey,author_principal_id,event_created_at,checksum,signature,signature_verified,body,active,attachment_refs) ORDER BY tenant_id,event_id),'[]'::jsonb)::text FROM chat_observed_events"
	files="SELECT COALESCE(jsonb_agg(jsonb_build_array(id,name,size,content_hash,storage_key,owner_id,tenant_id,deleted_at) ORDER BY tenant_id,id),'[]'::jsonb)::text FROM files"
	buzz_events_hash="$(query_hash buzz-postgres buzz buzz "${buzz_events}")" || return 1
	buzz_channels_hash="$(query_hash buzz-postgres buzz buzz "${buzz_channels}")" || return 1
	buzz_members_hash="$(query_hash buzz-postgres buzz buzz "${buzz_members}")" || return 1
	identity_bindings_hash="$(query_hash postgres rustshare rustshare "${identity_bindings}")" || return 1
	workspace_communities_hash="$(query_hash postgres rustshare rustshare "${workspace_communities}")" || return 1
	users_hash="$(query_hash postgres rustshare rustshare "${users}")" || return 1
	observed_events_hash="$(query_hash postgres rustshare rustshare "${observed_events}")" || return 1
	files_hash="$(query_hash postgres rustshare rustshare "${files}")" || return 1
	rustfs_objects_hash="$(volume_hash rustfs)" || return 1
	buzz_rustfs_objects_hash="$(volume_hash buzz-rustfs)" || return 1
	{
		printf 'buzz_events=%s\n' "${buzz_events_hash}"
		printf 'buzz_channels=%s\n' "${buzz_channels_hash}"
		printf 'buzz_members=%s\n' "${buzz_members_hash}"
		printf 'elembra_identity_bindings=%s\n' "${identity_bindings_hash}"
		printf 'elembra_workspace_communities=%s\n' "${workspace_communities_hash}"
		printf 'elembra_users_and_credentials=%s\n' "${users_hash}"
		printf 'elembra_observed_events=%s\n' "${observed_events_hash}"
		printf 'elembra_files=%s\n' "${files_hash}"
		printf 'rustfs_objects=%s\n' "${rustfs_objects_hash}"
		printf 'buzz_rustfs_objects=%s\n' "${buzz_rustfs_objects_hash}"
	} >"${EVIDENCE_DIR}/${name}.env"
}

PHASE=pre-backup-fingerprint
BASELINE="${EVIDENCE_DIR}/before.env"
snapshot before
capture_api_projection "${FILE_OWNER_EMAIL}" "${EVIDENCE_DIR}/api-before.sha256"

PHASE=backup
pin_recovery_compose_environment "${PROJECT}"
./scripts/backup-stack.sh --with-chat "${BACKUP_ROOT}" >"${EVIDENCE_DIR}/backup.log" 2>&1
BACKUP_DIR="$(sed -n 's/^Backup created at //p' "${EVIDENCE_DIR}/backup.log" | tail -n 1)"
[[ -n "${BACKUP_DIR}" && -d "${BACKUP_DIR}" ]] || { echo 'Backup command did not produce a bundle directory.' >&2; exit 1; }
./scripts/verify-backup-bundle.sh "${BACKUP_DIR}" >"${EVIDENCE_DIR}/backup-verify.log" 2>&1

PHASE=simulated-state-loss
compose_call down --volumes --remove-orphans >/dev/null
PHASE=restore
STACK_CREATED=true
pin_recovery_compose_environment "${PROJECT}"
RECOVERY_RELAY_IMAGE="${OLD_IMAGE}" ./scripts/restore-stack.sh --with-chat "${BACKUP_DIR}" >"${EVIDENCE_DIR}/restore.log" 2>&1

PHASE=post-restore-fingerprint
snapshot after-restore
cmp -s "${EVIDENCE_DIR}/before.env" "${EVIDENCE_DIR}/after-restore.env" || {
	diff -u "${EVIDENCE_DIR}/before.env" "${EVIDENCE_DIR}/after-restore.env" >&2
	echo "Restored application/relay state fingerprint differs from pre-backup state." >&2
	exit 1
}

verify_file_authorization_and_bytes() {
	local email="$1" output="$2" expected_status="$3" cookie="${TEMP_DIR}/cookies-$1" login_json="${TEMP_DIR}/login-$1.json" csrf status
	python3 - "${email}" <<'PY' >"${login_json}"
import json,sys
print(json.dumps({"email": sys.argv[1], "password": "alpha-user-pass-2026"}))
PY
	status="$(curl -sS -o /dev/null -w '%{http_code}' -c "${cookie}" -H 'content-type: application/json' --data-binary "@${login_json}" "${BASE_URL}/api/v1/auth/login")"
	[[ "${status}" == 2* ]] || { echo "Test user login failed (${status})." >&2; return 1; }
	csrf="$(awk '$6 == "rustshare_csrf_token" { print $7 }' "${cookie}")"
	status="$(curl -sS -o "${output}" -w '%{http_code}' -b "${cookie}" -H "X-Rustshare-Csrf: ${csrf}" -H 'content-type: application/json' \
		--data "{\"resource\":{\"application\":\"io.elembra.files\",\"resourceType\":\"file\",\"resourceId\":\"${FILE_ID}\"}}" \
		"${BASE_URL}/api/v1/applications/chat/attachments/open")"
	[[ "${status}" == "${expected_status}" ]] || { echo "Files authorization for ${email} returned ${status}, expected ${expected_status}." >&2; return 1; }
	if [[ "${expected_status}" == "200" ]]; then
		[[ "$(sha256sum "${output}" | awk '{print $1}')" == "${EXPECTED_FILE_HASH}" ]] || {
			echo 'Restored Files attachment bytes do not match the uploaded fixture.' >&2; return 1;
		}
	fi
}

PHASE=post-restore-files-api
verify_file_authorization_and_bytes "${FILE_OWNER_EMAIL}" "${TEMP_DIR}/owner-attachment" 200
verify_file_authorization_and_bytes "${EVE_EMAIL}" "${TEMP_DIR}/denied-attachment" 404
node frontend/scripts/chat-relay-history-probe.mjs >"${EVIDENCE_DIR}/history-after-restore.json"
RESTORED_HISTORY_HASH="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["eventSetSha256"])' "${EVIDENCE_DIR}/history-after-restore.json")"
[[ "${RESTORED_HISTORY_HASH}" == "${BASELINE_HISTORY_HASH}" ]] || { echo 'NIP-42 history differs after restore.' >&2; exit 1; }
capture_api_projection "${FILE_OWNER_EMAIL}" "${EVIDENCE_DIR}/api-after-restore.sha256"
cmp -s "${EVIDENCE_DIR}/api-before.sha256" "${EVIDENCE_DIR}/api-after-restore.sha256" || {
	echo 'Chat API channel/message/author projection differs after restore.' >&2; exit 1;
}

wait_relay_and_observer() {
	local expected_image="$1" container status
	container="$(compose_call ps -q buzz-relay)"
	[[ -n "${container}" ]] || { echo 'Buzz relay container is missing.' >&2; return 1; }
	[[ "$(docker inspect --format '{{.Config.Image}}' "${container}")" == "${expected_image}" ]] || {
		echo "Running relay image does not match requested immutable digest." >&2; return 1;
	}
	for _ in $(seq 1 90); do
		status="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "${container}")"
		[[ "${status}" == healthy || "${status}" == running ]] && break
		sleep 2
	done
	[[ "${status}" == healthy || "${status}" == running ]] || { echo 'Buzz relay did not become healthy.' >&2; return 1; }
	curl -fsS -H 'Accept: application/nostr+json' "http://127.0.0.1:${RELAY_PORT}/" >"${TEMP_DIR}/nip11.json"
	python3 - "${TEMP_DIR}/nip11.json" <<'PY'
import json,sys
metadata=json.load(open(sys.argv[1]))
if 42 not in metadata.get("supported_nips",[]): raise SystemExit("relay NIP-11 does not advertise NIP-42")
PY
	curl -fsS "${BASE_URL}/health/ready" >/dev/null
	for _ in $(seq 1 60); do
		if compose_call exec -T chat-observer wget -q -O - http://127.0.0.1:8091/ready >/dev/null 2>&1; then return 0; fi
		sleep 2
	done
	echo 'Chat observer did not become ready.' >&2
	return 1
}

PHASE=relay-image-switch-and-rollback
for image in "${NEW_IMAGE}" "${OLD_IMAGE}"; do
	RECOVERY_RELAY_IMAGE="${image}" compose_call up -d --no-deps --force-recreate buzz-relay >"${EVIDENCE_DIR}/image-switch.log" 2>&1
	wait_relay_and_observer "${image}"
	switched_relay_id="$(docker inspect --format '{{.Image}}' "$(compose_call ps -q buzz-relay)")"
	if [[ "${image}" == "${NEW_IMAGE}" ]]; then NEW_RELAY_IMAGE_ID="${switched_relay_id}"; else ROLLBACK_RELAY_IMAGE_ID="${switched_relay_id}"; fi
	snapshot "image-$( [[ "${image}" == "${OLD_IMAGE}" ]] && echo old || echo new )"
	node frontend/scripts/chat-relay-history-probe.mjs >"${EVIDENCE_DIR}/history-$( [[ "${image}" == "${OLD_IMAGE}" ]] && echo old || echo new ).json"
	image_history_hash="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["eventSetSha256"])' "${EVIDENCE_DIR}/history-$( [[ "${image}" == "${OLD_IMAGE}" ]] && echo old || echo new ).json")"
	[[ "${image_history_hash}" == "${BASELINE_HISTORY_HASH}" ]] || { echo "NIP-42 event history changed while using ${image}." >&2; exit 1; }
	if ! cmp -s "${EVIDENCE_DIR}/before.env" "${EVIDENCE_DIR}/image-$( [[ "${image}" == "${OLD_IMAGE}" ]] && echo old || echo new ).env"; then
		diff -u "${EVIDENCE_DIR}/before.env" "${EVIDENCE_DIR}/image-$( [[ "${image}" == "${OLD_IMAGE}" ]] && echo old || echo new ).env" >&2
		echo "State fingerprint changed while switching to ${image}." >&2
		exit 1
	fi
done

PHASE=observer-replay
projection_count="$(compose_call exec -T postgres psql -U rustshare -d rustshare -Atc 'SELECT count(*) FROM chat_observed_events')"
(( projection_count > 0 )) || { echo 'No observed Chat events exist for replay.' >&2; exit 1; }
projection_hash="$(query_hash postgres rustshare rustshare "SELECT COALESCE(jsonb_agg(jsonb_build_array(tenant_id,workspace_id,event_id,message_id,event_type,supersedes_event_id,community_id,channel_id,channel_kind,thread_root_id,author_pubkey,author_principal_id,event_created_at,checksum,signature,signature_verified,body,active,attachment_refs) ORDER BY tenant_id,event_id),'[]'::jsonb)::text FROM chat_observed_events")"
compose_call exec -T postgres psql -U rustshare -d rustshare -v ON_ERROR_STOP=1 -c 'DELETE FROM chat_observed_events' >/dev/null
compose_call restart chat-observer >/dev/null
for _ in $(seq 1 90); do
	current_count="$(compose_call exec -T postgres psql -U rustshare -d rustshare -Atc 'SELECT count(*) FROM chat_observed_events')"
	if (( current_count >= projection_count )); then break; fi
	sleep 2
done
(( current_count >= projection_count )) || { echo "Observer replay stopped at ${current_count}/${projection_count} rows." >&2; exit 1; }
replayed_projection_hash="$(query_hash postgres rustshare rustshare "SELECT COALESCE(jsonb_agg(jsonb_build_array(tenant_id,workspace_id,event_id,message_id,event_type,supersedes_event_id,community_id,channel_id,channel_kind,thread_root_id,author_pubkey,author_principal_id,event_created_at,checksum,signature,signature_verified,body,active,attachment_refs) ORDER BY tenant_id,event_id),'[]'::jsonb)::text FROM chat_observed_events")"
[[ "${replayed_projection_hash}" == "${projection_hash}" ]] || { echo 'Observer replayed row set differs from the pre-loss projection.' >&2; exit 1; }

cat >"${EVIDENCE_DIR}/result.env" <<EOF
RECOVERY_DRILL_STATUS=PASS
SOURCE_SHA=${SOURCE_SHA}
COMPOSE_PROJECT=${PROJECT}
OLD_RELAY_IMAGE=${OLD_IMAGE}
NEW_RELAY_IMAGE=${NEW_IMAGE}
INITIAL_BACKEND_IMAGE_ID=${BACKEND_IMAGE_ID}
INITIAL_OBSERVER_IMAGE_ID=${OBSERVER_IMAGE_ID}
INITIAL_RELAY_IMAGE_ID=${RELAY_IMAGE_ID}
NEW_RELAY_IMAGE_ID=${NEW_RELAY_IMAGE_ID}
ROLLBACK_RELAY_IMAGE_ID=${ROLLBACK_RELAY_IMAGE_ID}
RESTORED_FILES_API_BYTES=PASS_OWNER_HASH_MATCH_AND_UNAUTHORIZED_404
RESTORED_CHAT_API_PROJECTION=PASS_EXACT_CHANNEL_EVENT_AUTHOR_THREAD_ATTACHMENT_HASH
RESTORED_EVENT_PROJECTION_ROWS=${projection_count}
OBSERVER_REPLAY_ROWS=${current_count}
CHANNEL_BROWSER_CASES=${CHANNEL_CASES}/13_PASS
CONVERSATION_BROWSER_CASES=${CONVERSATION_CASES}/9_PASS
BUZZ_EVENT_HISTORY_SHA256=${BASELINE_HISTORY_HASH}
FILES_ATTACHMENT_SHA256=${EXPECTED_FILE_HASH}
OBSERVER_PROJECTION_SHA256=${replayed_projection_hash}
OFF_HOST_RECOVERY=NOT_TESTED_LOCAL_DISPOSABLE_BACKUP_ONLY
BROWSER_HELD_KEY_RECOVERY=NOT_TESTED_ACCEPTANCE_KEYS_EPHEMERAL_AND_NOT_EXPORTED
EXTERNAL_RECOVERY_CREDENTIAL_CUSTODY=NOT_TESTED
EOF
cat "${EVIDENCE_DIR}/result.env"
