#!/usr/bin/env bash
# scripts/run-chat-channel-acceptance.sh
# =============================================================================
# Local Chat channel-management product acceptance driver (PR #480).
#
# Runs the Playwright journey in frontend/tests/chat-channel-acceptance.e2e.ts
# against the disposable bundled stack (scripts/elembra.sh with
# docker-compose.chat-acceptance.yml). Collects build provenance (git SHA +
# image digests) up front and injects it into the evidence JSON the spec
# writes (docs/pilot/evidence/chat-channel-acceptance-<date>.json).
#
# Prerequisites:
#   COMPOSE_PROJECT_NAME + ELEMBRA_COMPOSE_EXTRA_FILE set (see .env), and
#   ./scripts/elembra.sh up
#
# Usage:
#   ./scripts/run-chat-channel-acceptance.sh
# =============================================================================
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

set -a
# shellcheck disable=SC1091
. ./.env
# shellcheck disable=SC1091
. ./config/buzz-compatibility.env
set +a

COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:?set COMPOSE_PROJECT_NAME in .env}"
ELEMBRA_COMPOSE_EXTRA_FILE="${ELEMBRA_COMPOSE_EXTRA_FILE:-docker-compose.chat-acceptance.yml}"
export COMPOSE_PROJECT_NAME ELEMBRA_COMPOSE_EXTRA_FILE

compose() {
	docker compose -f docker-compose.yml -f docker-compose.alpha.yml \
		-f docker-compose.dogfood.yml -f "${ELEMBRA_COMPOSE_EXTRA_FILE}" "$@"
}

BASE_URL="${ACCEPTANCE_BASE_URL:-http://127.0.0.1:18080}"
RELAY_URL="${BUZZ_RELAY_WS:-ws://localhost:17447}"
EVIDENCE_PATH="${ACCEPTANCE_EVIDENCE:-docs/pilot/evidence/chat-channel-acceptance-$(date +%Y%m%d).json}"
# ACCEPTANCE_GIT_SHA lets evidence runs record the exact CODE commit under
# test even when infra/evidence commits sit on top of it.
GIT_SHA="${ACCEPTANCE_GIT_SHA:-$(git rev-parse HEAD)}"

echo "== chat channel acceptance: waiting for the stack =="
backend_container="$(compose ps -q backend)"
ADMIN_PASSWORD="$(scripts/read-bootstrap-password.sh "${backend_container}")"

for i in $(seq 1 90); do
	code="$(curl -s -o /dev/null -w '%{http_code}' "${BASE_URL}/health/ready" || true)"
	[[ "${code}" == "200" ]] && break
	sleep 2
done
[[ "${code}" == "200" ]] || { echo "backend did not become ready at ${BASE_URL}" >&2; exit 1; }

for i in $(seq 1 150); do
	if curl -sf --max-time 2 "http://127.0.0.1:${RELAY_URL##*:}/health" >/dev/null 2>&1; then
		break
	fi
	sleep 2
	[[ "${i}" == "150" ]] && { echo "relay did not become healthy at ${RELAY_URL}" >&2; exit 1; }
done

observer_ready=""
for i in $(seq 1 60); do
	if compose exec -T chat-observer wget -q -O - http://127.0.0.1:8091/ready >/dev/null 2>&1; then
		observer_ready="yes"
		break
	fi
	sleep 2
done
[[ "${observer_ready}" == "yes" ]] || { echo "chat-observer did not become ready" >&2; exit 1; }

echo "== collecting image provenance =="
# Record the image each RUNNING container actually executes (Config.Image +
# resolved image id), not the compose-file reference: acceptance runs may
# override the relay image (e.g. the local candidate via
# docker-compose.chat-acceptance-candidate.yml), and the evidence must reflect
# what was really under test. Falls back to the compose reference image when a
# container is not running.
IMAGE_DIGESTS="$(
	python3 - <<'PY' "${COMPOSE_PROJECT_NAME}" "${ELEMBRA_COMPOSE_EXTRA_FILE}"
import json, os, subprocess, sys

project, extra_file = sys.argv[1], sys.argv[2]
compose = ["docker", "compose", "-f", "docker-compose.yml", "-f",
           "docker-compose.alpha.yml", "-f", "docker-compose.dogfood.yml",
           "-f", extra_file, "-p", project]

def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True)

out = {}
for name in ["backend", "chat-observer", "postgres", "rustfs", "nginx", "buzz-relay"]:
    container = run(compose + ["ps", "-q", name]).stdout.strip()
    if container:
        proc = run(["docker", "inspect", container,
                    "--format", "{{.Config.Image}}|{{.Image}}"])
        if proc.returncode == 0:
            image, image_id = (proc.stdout.strip().split("|") + [""])[:2]
            ref = run(["docker", "image", "inspect", image,
                       "--format", "{{index .RepoDigests 0}}"]).stdout.strip()
            out[name] = {"image": image, "repoDigest": ref or None, "imageId": image_id}
            continue
    out[name] = {"image": None, "repoDigest": None, "imageId": None}
print(json.dumps(out))
PY
)"

echo "== running Playwright acceptance journey =="
cd frontend
E2E_BASE_URL="${BASE_URL}" \
	ACCEPTANCE_BASE_URL="${BASE_URL}" \
	ACCEPTANCE_RELAY_URL="${RELAY_URL}" \
	ACCEPTANCE_ADMIN_EMAIL="${RUSTSHARE_ADMIN_EMAIL:-admin@localhost}" \
	ACCEPTANCE_ADMIN_PASSWORD="${ADMIN_PASSWORD}" \
	ACCEPTANCE_EVIDENCE="$(cd "${REPO_ROOT}" && pwd)/${EVIDENCE_PATH}" \
	ACCEPTANCE_GIT_SHA="${GIT_SHA}" \
	ACCEPTANCE_STACK="${COMPOSE_PROJECT_NAME} (source build at ${GIT_SHA}, relay ${RELAY_URL})" \
	ACCEPTANCE_IMAGE_DIGESTS="${IMAGE_DIGESTS}" \
	npx playwright test tests/chat-channel-acceptance.e2e.ts --workers=1
