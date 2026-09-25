#!/usr/bin/env bash

# Collect secret-safe diagnostics for operator support. This script deliberately
# never archives .env, .elembra/, container inspection output, or Compose's
# rendered environment.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_ROOT="${1:-${ROOT}/support-bundles}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/elembra-support.XXXXXX")"
OUTPUT_DIR="${OUTPUT_ROOT%/}/elembra-support-${STAMP}"

cleanup() { rm -rf "${WORK_DIR}"; }
trap cleanup EXIT

mkdir -p "${OUTPUT_ROOT}"
cd "${ROOT}"

set +u
if [[ -f .env ]]; then
	# shellcheck disable=SC1091
	. ./.env
fi
if [[ -f config/buzz-compatibility.env ]]; then
	# shellcheck disable=SC1091
	set -a
	. ./config/buzz-compatibility.env
	set +a
fi
set -u

compose() {
	docker compose \
		-f docker-compose.yml \
		-f docker-compose.alpha.yml \
		-f docker-compose.dogfood.yml \
		--profile chat \
		"$@"
}

redact() {
	sed -E \
		-e 's/(^|[[:space:];,\"])([^=[:space:]]*(PASSWORD|SECRET|TOKEN|PRIVATE_KEY|CLIENT_SECRET|ACCESS_KEY|JWT|ENCRYPTION_KEY)[^=[:space:]]*)=([^[:space:];,\"]+)/\1\2=<redacted>/gI' \
		-e 's/(Bearer[[:space:]]+)[A-Za-z0-9._~+\/-]+/\1<redacted>/gI' \
		-e 's/([?&](token|secret|password|key)=)[^&[:space:]]+/\1<redacted>/gI' \
		-e 's/("?(password|secret|token|private_key|client_secret|access_key|jwt|encryption_key)"?[[:space:]]*:[[:space:]]*")([^\"]*)(")/\1<redacted>\4/gI'
}

mkdir -p "${WORK_DIR}/logs" "${WORK_DIR}/health"

{
	echo "created_at_utc=${STAMP}"
	echo "git_commit=$(git rev-parse HEAD 2>/dev/null || echo unknown)"
	echo "git_branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
	echo "buzz_upstream_tag=${BUZZ_UPSTREAM_TAG:-unknown}"
	echo "buzz_upstream_sha=${BUZZ_UPSTREAM_SHA:-unknown}"
	echo "buzz_fork_sha=${BUZZ_FORK_SHA:-unknown}"
	echo "buzz_contract=${BUZZ_CONTRACT_VERSION:-unknown}"
	echo "kernel=$(uname -sr)"
	echo "architecture=$(uname -m)"
	echo "docker=$(docker version --format '{{.Server.Version}}' 2>/dev/null || echo unavailable)"
	echo "compose=$(docker compose version --short 2>/dev/null || echo unavailable)"
} >"${WORK_DIR}/manifest.env"

compose ps >"${WORK_DIR}/compose-ps.txt" 2>&1 || true
compose ps --format json >"${WORK_DIR}/compose-ps.json" 2>&1 || true
docker system df >"${WORK_DIR}/docker-disk.txt" 2>&1 || true
df -h >"${WORK_DIR}/host-disk.txt" 2>&1 || true

curl --silent --show-error --max-time 10 http://127.0.0.1/health >"${WORK_DIR}/health/backend.txt" 2>&1 || true
curl --silent --show-error --max-time 10 http://127.0.0.1/health/ready >"${WORK_DIR}/health/backend-ready.txt" 2>&1 || true
compose exec -T chat-observer wget -q -O - http://127.0.0.1:8091/health >"${WORK_DIR}/health/observer.txt" 2>&1 || true
compose exec -T buzz-relay curl -fsS --max-time 10 http://127.0.0.1:8088/health >"${WORK_DIR}/health/buzz-relay.txt" 2>&1 || true

for service in backend nginx postgres rustfs buzz-relay buzz-postgres buzz-redis buzz-rustfs chat-observer; do
	compose logs --no-color --tail=200 "${service}" 2>&1 | redact >"${WORK_DIR}/logs/${service}.log" || true
done

cp config/buzz-compatibility.env "${WORK_DIR}/buzz-compatibility.env"
{
	echo "services:"
	compose config --services 2>/dev/null || true
} >"${WORK_DIR}/compose-services.txt"

mv "${WORK_DIR}" "${OUTPUT_DIR}"
WORK_DIR=""

tar -C "${OUTPUT_ROOT}" -czf "${OUTPUT_DIR}.tar.gz" "$(basename "${OUTPUT_DIR}")"
rm -rf "${OUTPUT_DIR}"

echo "Support bundle created: ${OUTPUT_DIR}.tar.gz"
