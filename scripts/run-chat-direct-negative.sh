#!/usr/bin/env bash
# scripts/run-chat-direct-negative.sh
# =============================================================================
# Direct-relay negative acceptance driver (mission E): guest-role write denial
# and two-tenant isolation against the LOCALLY QUALIFIED candidate relay image
# (buzz-elembra-candidate:8ce0b7bd) on a disposable two-community harness
# (docker-compose.chat-direct-negative.yml).
#
# The harness is created, seeded with two Host-keyed communities, tested via
# frontend/scripts/chat-direct-negative.mjs, and ALWAYS torn down (down -v)
# via trap. Evidence lands at
# docs/pilot/evidence/chat-direct-negative-acceptance-<date>.json.
#
# Usage:
#   ./scripts/run-chat-direct-negative.sh
# =============================================================================
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

PROJECT="${CHAT_DIRECT_NEG_PROJECT:-chatnegharden-$(date +%s)-$$}"
FILE="docker-compose.chat-direct-negative.yml"
# The relay serves GET /health on its MAIN (WS) listener; the dedicated
# health port only answers /_liveness|/_readiness|/_status|/_mesh.
HEALTH_URL="http://127.0.0.1:17547/health"
IMAGE="buzz-elembra-candidate:8ce0b7bd"
EVIDENCE_PATH="${CHAT_DIRECT_NEG_EVIDENCE:-docs/pilot/evidence/chat-direct-negative-acceptance-$(date +%Y%m%d).json}"
# ACCEPTANCE_GIT_SHA lets evidence runs record the exact CODE commit under
# test even when infra/evidence commits sit on top of it.
GIT_SHA="${ACCEPTANCE_GIT_SHA:-$(git rev-parse HEAD)}"

compose() {
	docker compose -p "${PROJECT}" -f "${FILE}" "$@"
}

down() {
	compose down -v --remove-orphans || true
}

# `down -v` is destructive for every resource in a Compose project, so never
# attach the harness to a project that already owns containers, volumes, or networks.
for resources in \
	"$(docker ps -aq --filter "label=com.docker.compose.project=${PROJECT}")" \
	"$(docker volume ls -q --filter "label=com.docker.compose.project=${PROJECT}")" \
	"$(docker network ls -q --filter "label=com.docker.compose.project=${PROJECT}")"; do
	if [[ -n "${resources}" ]]; then
		echo "refusing to use non-empty Docker Compose project ${PROJECT}" >&2
		exit 1
	fi
done

trap down EXIT

echo "== direct-negative harness: starting ${PROJECT} =="
compose up -d

echo "== waiting for relay health =="
for i in $(seq 1 90); do
	code="$(curl -s -o /dev/null -w '%{http_code}' "${HEALTH_URL}" || true)"
	[[ "${code}" == "200" ]] && break
	sleep 2
done
[[ "${code}" == "200" ]] || { echo "relay did not become healthy at ${HEALTH_URL}" >&2; exit 1; }

echo "== seeding the second Host-keyed community =="
# The relay auto-ensures its RELAY_URL host community at startup
# ("Deployment community ensured" — a.localhost:17547); seed only tenant B
# and read back the auto-created tenant A id for the evidence topology.
compose exec -T pg psql -U buzz -d buzz -c "
INSERT INTO communities (id, host) VALUES
  ('22222222-2222-4222-8222-222222222222', 'b.localhost:17547')
ON CONFLICT (lower(host)) DO NOTHING;
"
COMMUNITY_A_ID="$(
	compose exec -T pg psql -U buzz -d buzz -tAc \
		"SELECT id FROM communities WHERE lower(host) = lower('a.localhost:17547')"
)"

echo "== collecting image provenance =="
IMAGE_ID="$(docker image inspect "${IMAGE}" --format '{{.Id}}' 2>/dev/null || true)"

echo "== running direct-relay negative matrix =="
CHAT_DIRECT_NEG_RELAY_PORT=17547 \
	CHAT_DIRECT_NEG_IMAGE="${IMAGE}" \
	CHAT_DIRECT_NEG_IMAGE_ID="${IMAGE_ID}" \
	CHAT_DIRECT_NEG_COMMUNITY_A_ID="${COMMUNITY_A_ID}" \
	CHAT_DIRECT_NEG_EVIDENCE="${REPO_ROOT}/${EVIDENCE_PATH}" \
	ACCEPTANCE_GIT_SHA="${GIT_SHA}" \
	ACCEPTANCE_STACK="${PROJECT} (direct-negative harness, relay 127.0.0.1:17547, image ${IMAGE})" \
	node frontend/scripts/chat-direct-negative.mjs

echo "== direct-negative matrix complete; harness teardown via trap =="
