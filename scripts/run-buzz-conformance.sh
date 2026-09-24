#!/usr/bin/env bash
# scripts/run-buzz-conformance.sh
#
# Runs the blocking Elembra <-> Buzz live conformance gate from a clean
# checkout. The script owns the Elembra and Buzz dependency stacks, uses the
# pinned Buzz compatibility manifest, runs migrations, and removes everything
# it started unless explicitly asked to keep it for debugging.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

COMPATIBILITY_FILE="${REPO_ROOT}/config/buzz-compatibility.env"
COMPOSE_PROJECT="rustshare-buzz-conformance"
COMPOSE_ARGS=(--env-file /dev/null -p "${COMPOSE_PROJECT}" -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.conformance.yml)
DIAGNOSTICS_FILE="${TMPDIR:-/tmp}/buzz-conformance-diagnostics.log"
umask 077
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/buzz-conformance.XXXXXX")"
rm -f -- "${DIAGNOSTICS_FILE}"
COMPOSE_OUTPUT="${RUN_DIR}/compose.log"
MIGRATION_OUTPUT="${RUN_DIR}/migrations.log"
SUITE_OUTPUT="${RUN_DIR}/suite.log"
PHASE="preflight"
FAILURE_CLASS="unknown"
KEEP_STACK="${RUSTSHARE_BUZZ_CONFORMANCE_KEEP:-0}"
POSTGRES_HOST_PORT="${RUSTSHARE_BUZZ_CONFORMANCE_POSTGRES_PORT:-15432}"
RUSTFS_HOST_PORT="${RUSTSHARE_BUZZ_CONFORMANCE_RUSTFS_PORT:-19000}"
RUSTFS_CONSOLE_HOST_PORT="${RUSTSHARE_BUZZ_CONFORMANCE_RUSTFS_CONSOLE_PORT:-19001}"
RELAY_HOST_PORT="${RUSTSHARE_BUZZ_CONFORMANCE_RELAY_PORT:-17447}"
HEALTH_HOST_PORT="${RUSTSHARE_BUZZ_CONFORMANCE_HEALTH_PORT:-18088}"
METRICS_HOST_PORT="${RUSTSHARE_BUZZ_CONFORMANCE_METRICS_PORT:-19102}"
for variable in ${!RUSTSHARE_@}; do
	unset "${variable}"
done
export RUSTSHARE_POSTGRES_HOST_PORT="${POSTGRES_HOST_PORT}"
export RUSTSHARE_RUSTFS_HOST_PORT="${RUSTFS_HOST_PORT}"
export RUSTSHARE_RUSTFS_CONSOLE_HOST_PORT="${RUSTFS_CONSOLE_HOST_PORT}"
export BUZZ_RELAY_HOST_PORT="${RELAY_HOST_PORT}"
export BUZZ_HEALTH_HOST_PORT="${HEALTH_HOST_PORT}"
export BUZZ_METRICS_HOST_PORT="${METRICS_HOST_PORT}"

compose() {
	docker compose "${COMPOSE_ARGS[@]}" "$@"
}

redact() {
	sed -E \
		-e 's/(PASSWORD|SECRET|TOKEN|PRIVATE_KEY|ACCESS_KEY|AWS_ACCESS_KEY_ID|AWS_SECRET_ACCESS_KEY)=([^[:space:]]+)/\1=<redacted>/g' \
		-e 's/("(password|secret|token|private_key|access_key)"[[:space:]]*:[[:space:]]*)"[^"]*"/\1"<redacted>"/Ig' \
		-e 's/((PASSWORD|SECRET|TOKEN|PRIVATE_KEY|ACCESS_KEY)[[:space:]]*:[[:space:]]*)[^,[:space:]}]+/\1<redacted>/Ig' \
		-e 's#((postgres(ql)?://[^:/[:space:]]+):)[^@[:space:]]+@#\1<redacted>@#Ig' \
		-e 's/[0-9a-fA-F]{64}/<redacted-hex>/g'
}

dump_diagnostics() {
	{
		echo "Buzz conformance diagnostics"
		echo "phase=${PHASE}"
		echo "failure_class=${FAILURE_CLASS}"
		echo "project=${COMPOSE_PROJECT}"
		echo
		echo "== compose ps =="
		compose ps || true
		echo
		echo "== dependency and relay logs (last 200 lines) =="
		compose logs --no-color --tail=200 postgres rustfs buzz-postgres buzz-redis buzz-rustfs buzz-rustfs-init buzz-relay || true
		echo
		echo "== readiness probes =="
		for url in \
			"http://127.0.0.1:${RUSTSHARE_RUSTFS_HOST_PORT}" \
			"http://127.0.0.1:${RUSTSHARE_RUSTFS_CONSOLE_HOST_PORT}" \
			"http://127.0.0.1:${BUZZ_RELAY_HOST_PORT}/health" \
			"http://127.0.0.1:${BUZZ_HEALTH_HOST_PORT}/health" \
			"http://127.0.0.1:${BUZZ_METRICS_HOST_PORT}/metrics"
		do
			if curl -fsS --max-time 3 "$url" >/dev/null 2>&1; then
				echo "ready ${url}"
			else
			echo "not-ready ${url}"
			fi
		done
		for log_file in "${COMPOSE_OUTPUT}" "${MIGRATION_OUTPUT}" "${SUITE_OUTPUT}"; do
			if [[ -f "${log_file}" ]]; then
				echo
				echo "== ${log_file} (last 120 lines) =="
				tail -n 120 "${log_file}"
			fi
		done
	} 2>&1 | redact >"${DIAGNOSTICS_FILE}" || true
	echo "diagnostics: ${DIAGNOSTICS_FILE}" >&2
}

cleanup() {
	if [[ "${KEEP_STACK}" == "1" ]]; then
		echo "== keeping conformance stack (RUSTSHARE_BUZZ_CONFORMANCE_KEEP=1) =="
		return
	fi
	echo "== cleaning up conformance stack =="
	compose down --volumes --remove-orphans >/dev/null 2>&1 || true
}

on_exit() {
	local status=$?
	if (( status != 0 )); then
		dump_diagnostics
	fi
	cleanup
	rm -rf -- "${RUN_DIR}"
	if (( status != 0 )); then
		echo "FAIL: Buzz conformance ${FAILURE_CLASS} during ${PHASE}" >&2
	fi
	exit "${status}"
}
trap on_exit EXIT

require_command() {
	command -v "$1" >/dev/null 2>&1 || {
		echo "FAIL: missing required command: $1" >&2
		FAILURE_CLASS="configuration-failure"
		exit 1
	}
}

wait_for_tcp() {
	local name="$1" host="$2" port="$3"
	for attempt in $(seq 1 60); do
		if (echo >/dev/tcp/${host}/${port}) >/dev/null 2>&1; then
			echo "  ${name} ready"
			return 0
		fi
		echo "  waiting for ${name} (${attempt}/60)…"
		sleep 2
	done
	if [[ "${name}" == "Buzz relay" ]]; then
		FAILURE_CLASS="Buzz readiness failure"
	else
		FAILURE_CLASS="dependency-readiness-failure"
	fi
	echo "FAIL: ${name} did not become ready at ${host}:${port}" >&2
	exit 1
}

extract_key() {
	local name="$1" output="$2"
	printf '%s\n' "${output}" | sed -n "s/^${name}=//p" | awk '{print $1}'
}

PHASE="preflight"
require_command docker
require_command cargo
require_command curl
require_command openssl
require_command node
require_command npm

if [[ ! -r "${COMPATIBILITY_FILE}" ]]; then
	FAILURE_CLASS="configuration-failure"
	echo "FAIL: missing ${COMPATIBILITY_FILE}" >&2
	exit 1
fi

set -a
# The blocking gate deliberately ignores .env: local shell code and stale
# endpoint overrides must not change the implementation under test.
# shellcheck disable=SC1090
. "${COMPATIBILITY_FILE}"
set +a

: "${BUZZ_SUPPORTED_COMMIT:?BUZZ_SUPPORTED_COMMIT is required in ${COMPATIBILITY_FILE}}"
: "${BUZZ_SUPPORTED_IMAGE:?BUZZ_SUPPORTED_IMAGE is required in ${COMPATIBILITY_FILE}}"
: "${BUZZ_SUPPORTED_IMAGE_TAG:?BUZZ_SUPPORTED_IMAGE_TAG is required in ${COMPATIBILITY_FILE}}"
: "${BUZZ_SUPPORTED_IMAGE_DIGEST:?BUZZ_SUPPORTED_IMAGE_DIGEST is required in ${COMPATIBILITY_FILE}}"
: "${BUZZ_CONTRACT_VERSION:?BUZZ_CONTRACT_VERSION is required in ${COMPATIBILITY_FILE}}"
: "${BUZZ_RELAY_IMAGE:?BUZZ_RELAY_IMAGE is required in ${COMPATIBILITY_FILE}}"
if [[ ! "${BUZZ_SUPPORTED_COMMIT}" =~ ^[0-9a-f]{40}$ \
	|| ! "${BUZZ_SUPPORTED_IMAGE_DIGEST}" =~ ^sha256:[0-9a-f]{64}$ \
	|| "${BUZZ_SUPPORTED_IMAGE_TAG}" != "sha-${BUZZ_SUPPORTED_COMMIT:0:7}" \
	|| "${BUZZ_RELAY_IMAGE}" != "${BUZZ_SUPPORTED_IMAGE}@${BUZZ_SUPPORTED_IMAGE_DIGEST}" ]]; then
	FAILURE_CLASS="configuration-failure"
	echo "FAIL: invalid Buzz compatibility manifest" >&2
	exit 1
fi
# The blocking gate always uses the manifest image, even if a local .env has
# an old BUZZ_RELAY_IMAGE override.
export BUZZ_RELAY_IMAGE
echo "Buzz compatibility: commit=${BUZZ_SUPPORTED_COMMIT} contract=${BUZZ_CONTRACT_VERSION} image=${BUZZ_RELAY_IMAGE}"

random_hex() {
	openssl rand -hex "$1"
}

# The conformance stack is intentionally fresh and host-local. Do not inherit
# a developer's external DB or object-store endpoint into the live proof.
unset BUZZ_SERVICE_SK BUZZ_RELAY_PRIVATE_KEY BUZZ_RELAY_OWNER_PUBKEY BUZZ_RELAY_PUBKEY
unset POSTGRES_PASSWORD DATABASE_URL AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY
unset RUSTFS_ROOT_USER RUSTFS_ROOT_PASSWORD JWT_SECRET RUSTSHARE_SECRET_ENCRYPTION_KEY
unset RUSTSHARE_CHAT_WEBHOOK_SECRET RUSTSHARE_ADMIN_PASSWORD RUSTSHARE_DEMO_VIEWER_PASSWORD
unset BUZZ_POSTGRES_PASSWORD BUZZ_RUSTFS_ACCESS_KEY BUZZ_RUSTFS_SECRET_KEY
unset ELEMBRA_LLM_API_KEY ELEMBRA_LLM_BASE_URL ELEMBRA_LLM_MODEL ELEMBRA_LLM_TEMPERATURE ELEMBRA_LLM_TIMEOUT_SECS
unset RUSTSHARE_CHAT_AUTHORITY RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY RUSTSHARE_CHAT_PROVISIONING
unset OIDC_ISSUER_URL OIDC_CLIENT_ID OIDC_CLIENT_SECRET OIDC_REDIRECT_URL OIDC_LOGIN_LABEL OIDC_SCOPES
unset OIDC_MOBILE_CLIENT_ID OIDC_MOBILE_CLIENT_SECRET OIDC_MOBILE_REDIRECT_URIS PASSWORD_LOGIN_ENABLED

PHASE="dependency installation"
FAILURE_CLASS="configuration-failure"
echo "== installing frontend key-generation dependencies =="
npm ci --prefix frontend --no-audit --no-fund
SQLX_CLI_VERSION=0.8.6
if ! command -v sqlx >/dev/null 2>&1 || ! sqlx --version 2>/dev/null | grep -q "^sqlx-cli ${SQLX_CLI_VERSION}($|[[:space:]])"; then
	echo "== installing sqlx-cli =="
	cargo install sqlx-cli --version "${SQLX_CLI_VERSION}" --features postgres --locked --force
fi

export POSTGRES_PASSWORD="$(random_hex 32)"
export DATABASE_URL="postgres://rustshare:${POSTGRES_PASSWORD}@127.0.0.1:${RUSTSHARE_POSTGRES_HOST_PORT}/rustshare"
export RUSTFS_ROOT_USER="rs$(random_hex 10)"
export RUSTFS_ROOT_PASSWORD="$(random_hex 32)"
export AWS_ACCESS_KEY_ID="${RUSTFS_ROOT_USER}"
export AWS_SECRET_ACCESS_KEY="${RUSTFS_ROOT_PASSWORD}"
export RUSTFS_ENDPOINT="http://127.0.0.1:${RUSTSHARE_RUSTFS_HOST_PORT}"
export RUSTFS_PUBLIC_ENDPOINT="http://127.0.0.1:${RUSTSHARE_RUSTFS_HOST_PORT}"
export RUSTFS_REGION="us-east-1"
export RUSTFS_BUCKET="rustshare-files"
export RUSTSHARE_OBJECT_STORE_AUTO_CREATE_BUCKET="true"
export JWT_SECRET="$(random_hex 32)"
export RUSTSHARE_SECRET_ENCRYPTION_KEY="$(random_hex 32)"
export RUSTSHARE_CHAT_WEBHOOK_SECRET="$(random_hex 32)"
export RUSTSHARE_ADMIN_PASSWORD="$(random_hex 32)"
export RUSTSHARE_DEMO_VIEWER_PASSWORD="$(random_hex 32)"
export BUZZ_POSTGRES_PASSWORD="$(random_hex 32)"
export BUZZ_RUSTFS_ACCESS_KEY="buzz$(random_hex 10)"
export BUZZ_RUSTFS_SECRET_KEY="$(random_hex 32)"
export BUZZ_RELAY_URL="ws://127.0.0.1:${BUZZ_RELAY_HOST_PORT}"
export BUZZ_RELAY_WS="ws://127.0.0.1:${BUZZ_RELAY_HOST_PORT}"
export RUSTSHARE_CHAT_AUTHORITY="buzz"
export RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY="true"
export RUSTSHARE_CHAT_PROVISIONING="auto"
export ELEMBRA_LLM_API_KEY=""
export ELEMBRA_LLM_BASE_URL=""
export ELEMBRA_LLM_MODEL=""
export ELEMBRA_LLM_TEMPERATURE=""
export ELEMBRA_LLM_TIMEOUT_SECS=""

PHASE="key preparation"
KEYS_OUT="$(node frontend/scripts/alpha-gen-buzz-keys.mjs)"
BUZZ_SERVICE_SK="$(extract_key BUZZ_SERVICE_SK "${KEYS_OUT}")"
BUZZ_RELAY_PRIVATE_KEY="$(extract_key BUZZ_RELAY_PRIVATE_KEY "${KEYS_OUT}")"

if [[ ! "${BUZZ_SERVICE_SK:-}" =~ ^[0-9a-f]{64}$ || ! "${BUZZ_RELAY_PRIVATE_KEY:-}" =~ ^[0-9a-f]{64}$ ]]; then
	FAILURE_CLASS="configuration-failure"
	echo "FAIL: BUZZ_SERVICE_SK and BUZZ_RELAY_PRIVATE_KEY must be 64 lowercase hex characters" >&2
	exit 1
fi

BUZZ_SERVICE_PK="$(cd frontend && node scripts/alpha-buzz-ops.mjs pubkey "${BUZZ_SERVICE_SK}")"
BUZZ_RELAY_DERIVED_PUBKEY="$(cd frontend && node scripts/alpha-buzz-ops.mjs pubkey "${BUZZ_RELAY_PRIVATE_KEY}")"
if [[ -n "${BUZZ_RELAY_OWNER_PUBKEY:-}" && "${BUZZ_RELAY_OWNER_PUBKEY}" != "${BUZZ_SERVICE_PK}" ]]; then
	FAILURE_CLASS="configuration-failure"
	echo "FAIL: BUZZ_RELAY_OWNER_PUBKEY does not match BUZZ_SERVICE_SK" >&2
	exit 1
fi
if [[ -n "${BUZZ_RELAY_PUBKEY:-}" && "${BUZZ_RELAY_PUBKEY}" != "${BUZZ_RELAY_DERIVED_PUBKEY}" ]]; then
	FAILURE_CLASS="configuration-failure"
	echo "FAIL: BUZZ_RELAY_PUBKEY does not match BUZZ_RELAY_PRIVATE_KEY" >&2
	exit 1
fi
BUZZ_RELAY_PUBKEY="${BUZZ_RELAY_DERIVED_PUBKEY}"
export BUZZ_SERVICE_SK BUZZ_RELAY_PRIVATE_KEY BUZZ_RELAY_PUBKEY
export BUZZ_RELAY_OWNER_PUBKEY="${BUZZ_SERVICE_PK}"
export RELAY_TRUSTED_SERVICE_PUBKEYS="${BUZZ_SERVICE_PK}"
echo "  service pubkey: ${BUZZ_SERVICE_PK:0:16}…  relay pubkey: ${BUZZ_RELAY_PUBKEY:0:16}…"

PHASE="Compose validation"
compose config --quiet

# Remove only the named project from an earlier interrupted run. Other
# developer containers and volumes are outside this script's ownership.
compose down --volumes --remove-orphans >/dev/null 2>&1 || true

PHASE="dependency startup"
echo "== starting Elembra and Buzz dependencies =="
set +e
compose up -d postgres rustfs buzz-relay 2>&1 | redact | tee "${COMPOSE_OUTPUT}"
COMPOSE_EXIT="${PIPESTATUS[0]}"
set -e
if [[ "${COMPOSE_EXIT}" != "0" ]]; then
	if grep -Eqi 'port is already allocated|failed to set up container networking' "${COMPOSE_OUTPUT}"; then
		FAILURE_CLASS="dependency startup failure"
	elif grep -Eqi 'pull access denied|manifest unknown|unauthorized|not found' "${COMPOSE_OUTPUT}"; then
		if grep -Eqi 'ghcr.io/kubedoio/buzz|buzz-relay' "${COMPOSE_OUTPUT}"; then
			FAILURE_CLASS="Buzz image failure"
		else
			FAILURE_CLASS="dependency image failure"
		fi
	elif grep -Eqi 'ghcr.io/kubedoio/buzz|buzz-relay' "${COMPOSE_OUTPUT}"; then
		FAILURE_CLASS="Buzz startup failure"
	else
		FAILURE_CLASS="dependency startup failure"
	fi
	exit "${COMPOSE_EXIT}"
fi

PHASE="dependency readiness"
wait_for_tcp "Elembra PostgreSQL" 127.0.0.1 "${RUSTSHARE_POSTGRES_HOST_PORT}"
wait_for_tcp "Elembra RustFS S3" 127.0.0.1 "${RUSTSHARE_RUSTFS_HOST_PORT}"
wait_for_tcp "Elembra RustFS console" 127.0.0.1 "${RUSTSHARE_RUSTFS_CONSOLE_HOST_PORT}"
wait_for_tcp "Buzz relay" 127.0.0.1 "${BUZZ_RELAY_HOST_PORT}"

PHASE="Elembra migration/setup"
set +e
sqlx migrate run --source backend/migrations 2>&1 | redact | tee "${MIGRATION_OUTPUT}"
MIGRATION_EXIT="${PIPESTATUS[0]}"
set -e
if [[ "${MIGRATION_EXIT}" != "0" ]]; then
	FAILURE_CLASS="Elembra migration/setup failure"
	exit "${MIGRATION_EXIT}"
fi

PHASE="Buzz readiness"
echo "== waiting for Buzz readiness =="
for attempt in $(seq 1 150); do
	if curl -fsS "http://127.0.0.1:${BUZZ_RELAY_HOST_PORT}/health" >/dev/null 2>&1; then
		echo "  Buzz ready"
		break
	fi
	echo "  waiting for Buzz (${attempt}/150)…"
	sleep 2
	if [[ "${attempt}" == "150" ]]; then
		FAILURE_CLASS="Buzz readiness failure"
		echo "FAIL: Buzz did not become healthy at http://127.0.0.1:${BUZZ_RELAY_HOST_PORT}/health" >&2
		exit 1
	fi
done

PHASE="live conformance assertions"
echo "== running live Buzz conformance suite (${BUZZ_CONTRACT_VERSION}) =="
export RUSTSHARE_BUZZ_LIVE_RELAY_URL="${BUZZ_RELAY_URL}"
export RUSTSHARE_BUZZ_LIVE_SERVICE_SK="${BUZZ_SERVICE_SK}"
export RUSTSHARE_BUZZ_LIVE_RELAY_PUBKEY="${BUZZ_RELAY_PUBKEY}"
export RUSTSHARE_BUZZ_LIVE_METRICS_URL="http://127.0.0.1:${BUZZ_METRICS_HOST_PORT}"
TEST_ARGS=(--ignored --test-threads=1)
set +e
SQLX_OFFLINE=true cargo test -p rustshare-server --test buzz_live_conformance_test -- "${TEST_ARGS[@]}" 2>&1 | redact | tee "${SUITE_OUTPUT}"
SUITE_EXIT="${PIPESTATUS[0]}"
set -e
if [[ "${SUITE_EXIT}" != "0" ]]; then
	FAILURE_CLASS="conformance assertion failure"
	echo "FAIL: live Buzz conformance suite exited ${SUITE_EXIT}; see ${SUITE_OUTPUT}" >&2
	exit "${SUITE_EXIT}"
fi

echo "PASS: live Buzz conformance suite; all proofs executed"
echo "PASS: clean Buzz runtime baseline"
