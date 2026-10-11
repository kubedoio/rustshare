#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
	cat <<'EOF'
Usage: scripts/run-restore-drill.sh <backup_dir>

Runs a non-destructive restore drill in an isolated Docker Compose project.

What it does:
1. verifies the backup bundle
2. restores it into a disposable Docker Compose project on alternate host ports
3. runs the post-restore smoke test against the isolated stack
4. optionally tears the drill stack down

Environment overrides:
- DRILL_PROJECT_NAME (default: rustshare-restore-drill)
- DRILL_COMPOSE_FILE (default: docker-compose.restore-drill.yml)
- DRILL_BASE_URL (default: http://localhost:18080)
- DRILL_API_BASE_URL (default: ${DRILL_BASE_URL}/api/v1)
- DRILL_POSTGRES_HOST_PORT (default: 15432)
- DRILL_RUSTFS_HOST_PORT (default: 19000)
- DRILL_RUSTFS_CONSOLE_HOST_PORT (default: 19001)
- DRILL_BACKEND_HOST_PORT (default: 18081)
- DRILL_NGINX_HOST_PORT (default: 18080)
- DRILL_KEEP_STACK (default: false)
- restore drills sharing a DRILL_PROJECT_NAME are serialized with an OS lock
- DRILL_REPORT_DIR (default: ./restore-drill-reports)
- ADMIN_EMAIL (default: admin@localhost)
- ADMIN_PASSWORD (default: )
- PUBLIC_SHARE_TOKEN (optional)
- PUBLIC_SHARE_PASSWORD (optional)
- ALLOW_SKIP_PUBLIC_SHARE (default: true)
- RESTORE_DRILL_STATE_FILE (optional beta-smoke report containing data
  identifiers to verify after restore)
- RESTORE_DRILL_PILOT_REPORT_PATH (optional exact persistence report path)
- RUSTSHARE_BACKEND_IMAGE (optional prebuilt backend image; when unset,
  the drill builds rustshare-backend:restore-drill from the current workspace)
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" || $# -lt 1 ]]; then
	usage
	exit $(( $# < 1 ))
fi

require_command() {
	local command_name="$1"
	if ! command -v "${command_name}" >/dev/null 2>&1; then
		echo "Missing required command: ${command_name}" >&2
		exit 1
	fi
}

timestamp() {
	date -u +"%Y-%m-%dT%H:%M:%SZ"
}

write_report() {
	local status="$1"
	local details="$2"
	mkdir -p "${DRILL_REPORT_DIR}"
	cat >"${REPORT_PATH}" <<EOF
RESTORE_DRILL_STATUS=${status}
RESTORE_DRILL_STARTED_AT=${DRILL_STARTED_AT}
RESTORE_DRILL_FINISHED_AT=$(timestamp)
RESTORE_DRILL_PROJECT=${DRILL_PROJECT_NAME}
RESTORE_DRILL_BASE_URL=${DRILL_BASE_URL}
RESTORE_DRILL_SOURCE_SHA=${PILOT_SOURCE_SHA:-unknown}
RESTORE_DRILL_BUILD_VERSION=${PILOT_BUILD_VERSION:-unknown}
RESTORE_DRILL_DEPLOYMENT_ID=${PILOT_DEPLOYMENT_ID:-unknown}
RESTORE_DRILL_CONFIG_ID=${PILOT_CONFIG_ID:-unknown}
RESTORE_DRILL_WORKFLOW_RUN_ID=${GITHUB_RUN_ID:-unknown}
RESTORE_DRILL_WORKFLOW_RUN_ATTEMPT=${GITHUB_RUN_ATTEMPT:-unknown}
RESTORE_DRILL_BACKEND_IMAGE=${DRILL_BACKEND_IMAGE}
RESTORE_DRILL_BACKEND_IMAGE_ID=${DRILL_BACKEND_IMAGE_ID}
RESTORE_DRILL_IMAGE_REVISION=${DRILL_BACKEND_IMAGE_REVISION}
BACKUP_DIR=${BACKUP_DIR}
REPORT_DETAILS=${details}
EOF
}

require_command docker
require_command bash
require_command flock

if [[ -f "${PROJECT_ROOT}/.env" && -z "${ADMIN_PASSWORD:-}" && -z "${RUSTSHARE_ADMIN_PASSWORD:-}" ]]; then
	# shellcheck disable=SC1091
	set -a
	. "${PROJECT_ROOT}/.env"
	set +a
fi

BACKUP_DIR="$(cd "$1" && pwd)"
DRILL_PROJECT_NAME="${DRILL_PROJECT_NAME:-rustshare-restore-drill}"
DRILL_COMPOSE_FILE="${DRILL_COMPOSE_FILE:-${PROJECT_ROOT}/docker-compose.restore-drill.yml}"
DRILL_POSTGRES_HOST_PORT="${DRILL_POSTGRES_HOST_PORT:-15432}"
DRILL_RUSTFS_HOST_PORT="${DRILL_RUSTFS_HOST_PORT:-19000}"
DRILL_RUSTFS_CONSOLE_HOST_PORT="${DRILL_RUSTFS_CONSOLE_HOST_PORT:-19001}"
DRILL_BACKEND_HOST_PORT="${DRILL_BACKEND_HOST_PORT:-18081}"
DRILL_NGINX_HOST_PORT="${DRILL_NGINX_HOST_PORT:-18080}"
DRILL_BASE_URL="${DRILL_BASE_URL:-http://localhost:${DRILL_NGINX_HOST_PORT}}"
DRILL_API_BASE_URL="${DRILL_API_BASE_URL:-${DRILL_BASE_URL%/}/api/v1}"
DRILL_KEEP_STACK="${DRILL_KEEP_STACK:-false}"
if [[ -n "${RUSTSHARE_BACKEND_IMAGE:-}" ]]; then
	DRILL_BACKEND_IMAGE="${RUSTSHARE_BACKEND_IMAGE}"
	DRILL_BACKEND_IMAGE_PREBUILT=true
else
	DRILL_BACKEND_IMAGE="rustshare-backend:restore-drill"
	DRILL_BACKEND_IMAGE_PREBUILT=false
fi
DRILL_REPORT_DIR="${DRILL_REPORT_DIR:-${PROJECT_ROOT}/restore-drill-reports}"
ALLOW_SKIP_PUBLIC_SHARE="${ALLOW_SKIP_PUBLIC_SHARE:-true}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-${RUSTSHARE_ADMIN_PASSWORD:-}}"
ADMIN_EMAIL="${ADMIN_EMAIL:-${RUSTSHARE_ADMIN_EMAIL:-admin@localhost}}"
VIEWER_EMAIL="${VIEWER_EMAIL:-${RUSTSHARE_DEMO_VIEWER_EMAIL:-viewer@localhost}}"
VIEWER_PASSWORD="${VIEWER_PASSWORD:-${RUSTSHARE_DEMO_VIEWER_PASSWORD:-}}"
RESTORE_DRILL_STATE_FILE="${RESTORE_DRILL_STATE_FILE:-}"
RESTORE_DRILL_PILOT_REPORT_PATH="${RESTORE_DRILL_PILOT_REPORT_PATH:-}"
export RUSTSHARE_ADMIN_PASSWORD="${ADMIN_PASSWORD}"
export RUSTSHARE_DEMO_VIEWER_PASSWORD="${VIEWER_PASSWORD}"
export RESTORE_DRILL_POSTGRES_HOST_PORT="${DRILL_POSTGRES_HOST_PORT}"
export RESTORE_DRILL_RUSTFS_HOST_PORT="${DRILL_RUSTFS_HOST_PORT}"
export RESTORE_DRILL_RUSTFS_CONSOLE_HOST_PORT="${DRILL_RUSTFS_CONSOLE_HOST_PORT}"
export RESTORE_DRILL_BACKEND_HOST_PORT="${DRILL_BACKEND_HOST_PORT}"
export RESTORE_DRILL_NGINX_HOST_PORT="${DRILL_NGINX_HOST_PORT}"

DRILL_STARTED_AT="$(timestamp)"
REPORT_BASENAME="$(date -u +%Y%m%dT%H%M%SZ)-restore-drill.env"
REPORT_PATH="${DRILL_REPORT_DIR%/}/${REPORT_BASENAME}"

export COMPOSE_PROJECT_NAME="${DRILL_PROJECT_NAME}"
export COMPOSE_FILE="${DRILL_COMPOSE_FILE}"
export RUSTSHARE_BACKEND_IMAGE="${DRILL_BACKEND_IMAGE}"
DRILL_STACK_STARTED=false
DRILL_BACKEND_IMAGE_ID="unknown"
DRILL_BACKEND_IMAGE_REVISION="unknown"

require_unused_drill_project() {
	local project_label="label=com.docker.compose.project=${DRILL_PROJECT_NAME}"
	local resources
	local resource_type
	local -a resources_cmd
	local found_resources=false

	for resource_type in containers volumes networks; do
		case "${resource_type}" in
			containers) resources_cmd=(docker ps -aq --filter "${project_label}") ;;
			volumes) resources_cmd=(docker volume ls -q --filter "${project_label}") ;;
			networks) resources_cmd=(docker network ls -q --filter "${project_label}") ;;
		esac

		if ! resources="$("${resources_cmd[@]}")"; then
			echo "Unable to check Docker ${resource_type} for project '${DRILL_PROJECT_NAME}'. Refusing to start restore drill." >&2
			return 1
		fi

		if [[ -n "${resources}" ]]; then
			echo "Found existing Docker ${resource_type} for restore drill project '${DRILL_PROJECT_NAME}':" >&2
			printf '%s\n' "${resources}" >&2
			found_resources=true
		fi
	done

	if [[ "${found_resources}" == "true" ]]; then
		echo "Inspect and explicitly remove resources labeled '${project_label}' after confirming they belong to this isolated drill project, then retry." >&2
		return 1
	fi
	return 0
}

cleanup() {
	if [[ "${DRILL_KEEP_STACK}" == "true" || "${DRILL_STACK_STARTED}" != "true" ]]; then
		return 0
	fi

	docker compose down -v --remove-orphans >/dev/null
}

on_error() {
	local exit_code=$?
	local cleanup_details=""
	trap - ERR
	if ! cleanup; then
		cleanup_details=" Cleanup failed; resources for '${DRILL_PROJECT_NAME}' may remain."
		echo "Restore drill cleanup failed for project '${DRILL_PROJECT_NAME}'." >&2
	fi
	write_report "failed" "Restore drill failed. Inspect the isolated compose project logs.${cleanup_details}"
	exit "${exit_code}"
}

trap on_error ERR

cd "${PROJECT_ROOT}"

DRILL_LOCK_KEY="$(printf '%s' "${DRILL_PROJECT_NAME}" | sha256sum)"
DRILL_LOCK_KEY="${DRILL_LOCK_KEY%% *}"
DRILL_LOCK_FILE="/tmp/rustshare-restore-drill-${DRILL_LOCK_KEY}.lock"
exec {DRILL_LOCK_FD}>"${DRILL_LOCK_FILE}"
if ! flock -n "${DRILL_LOCK_FD}"; then
	echo "Another restore drill is already running for project '${DRILL_PROJECT_NAME}' (lock: ${DRILL_LOCK_FILE})." >&2
	exit 1
fi

echo "Checking that isolated drill project '${DRILL_PROJECT_NAME}' is unused..."
require_unused_drill_project

echo "Verifying backup bundle..."
"${PROJECT_ROOT}/scripts/verify-backup-bundle.sh" "${BACKUP_DIR}"

if [[ "${DRILL_BACKEND_IMAGE_PREBUILT}" == "true" ]]; then
	echo "Using prebuilt backend image '${DRILL_BACKEND_IMAGE}'..."
else
	echo "Building isolated backend image from current workspace..."
	docker build -f docker/backend.Dockerfile \
		--build-arg VERSION="${PILOT_BUILD_VERSION:-restore-drill}" \
		--build-arg REVISION="${PILOT_SOURCE_SHA:-$(git rev-parse HEAD)}" \
		-t "${DRILL_BACKEND_IMAGE}" .
fi
DRILL_BACKEND_IMAGE_ID="$(docker image inspect "${DRILL_BACKEND_IMAGE}" --format '{{.Id}}')"
DRILL_BACKEND_IMAGE_REVISION="$(docker image inspect "${DRILL_BACKEND_IMAGE}" --format '{{index .Config.Labels "org.opencontainers.image.revision"}}')"
if [[ "${DRILL_BACKEND_IMAGE_REVISION}" == "<no value>" ]]; then
	DRILL_BACKEND_IMAGE_REVISION="unknown"
fi
if [[ -n "${PILOT_IMAGE_ID:-}" && "${PILOT_IMAGE_ID}" != "${DRILL_BACKEND_IMAGE_ID}" ]]; then
	echo "Restore drill image ID ${DRILL_BACKEND_IMAGE_ID} does not match tested pilot image ${PILOT_IMAGE_ID}." >&2
	exit 1
fi
if [[ -n "${PILOT_SOURCE_SHA:-}" && "${PILOT_SOURCE_SHA}" != "unknown" && "${DRILL_BACKEND_IMAGE_REVISION}" != "${PILOT_SOURCE_SHA}" ]]; then
	echo "Restore drill image revision ${DRILL_BACKEND_IMAGE_REVISION} does not match tested pilot SHA ${PILOT_SOURCE_SHA}." >&2
	exit 1
fi

echo "Restoring backup into isolated project '${DRILL_PROJECT_NAME}'..."
DRILL_STACK_STARTED=true
"${PROJECT_ROOT}/scripts/restore-stack.sh" "${BACKUP_DIR}"

backend_container="$(docker compose ps -q backend)"
if [[ -z "${backend_container}" ]]; then
	echo "Restore drill backend container is missing." >&2
	exit 1
fi
running_backend_image_id="$(docker inspect --format '{{.Image}}' "${backend_container}")"
if [[ "${running_backend_image_id}" != "${DRILL_BACKEND_IMAGE_ID}" ]]; then
	echo "Restore drill is running image ${running_backend_image_id}, expected ${DRILL_BACKEND_IMAGE_ID}." >&2
	exit 1
fi

echo "Checking isolated stack health..."
docker compose ps
curl -fsS "${DRILL_BASE_URL%/}/health" >/dev/null
curl -fsS "http://localhost:${DRILL_BACKEND_HOST_PORT}/health" >/dev/null

echo "Running post-restore smoke test..."
BASE_URL="${DRILL_BASE_URL}" \
API_BASE_URL="${DRILL_API_BASE_URL}" \
ALLOW_SKIP_PUBLIC_SHARE="${ALLOW_SKIP_PUBLIC_SHARE}" \
ADMIN_EMAIL="${ADMIN_EMAIL}" \
ADMIN_PASSWORD="${ADMIN_PASSWORD}" \
PUBLIC_SHARE_TOKEN="${PUBLIC_SHARE_TOKEN:-}" \
PUBLIC_SHARE_PASSWORD="${PUBLIC_SHARE_PASSWORD:-}" \
	"${PROJECT_ROOT}/scripts/post-restore-smoke.sh"

if [[ -n "${RESTORE_DRILL_STATE_FILE}" ]]; then
	echo "Verifying representative pilot data after restore..."
	BASE_URL="${DRILL_BASE_URL}" \
	API_BASE_URL="${DRILL_API_BASE_URL}" \
	PILOT_MODE="restore-verify" \
	PILOT_VERIFY_STATE_FILE="${RESTORE_DRILL_STATE_FILE}" \
	PILOT_REPORT_PATH="${RESTORE_DRILL_PILOT_REPORT_PATH}" \
	PILOT_SOURCE_SHA="${PILOT_SOURCE_SHA:-unknown}" \
	PILOT_BUILD_VERSION="${PILOT_BUILD_VERSION:-unknown}" \
	PILOT_DEPLOYMENT_ID="${DRILL_PROJECT_NAME}" \
	PILOT_CONFIG_ID="${PILOT_CONFIG_ID:-unknown}" \
	ADMIN_EMAIL="${ADMIN_EMAIL}" \
	ADMIN_PASSWORD="${ADMIN_PASSWORD}" \
	VIEWER_EMAIL="${VIEWER_EMAIL}" \
	VIEWER_PASSWORD="${VIEWER_PASSWORD}" \
		"${PROJECT_ROOT}/scripts/run-beta-smoke.sh"
fi

if ! cleanup; then
	write_report "failed" "Restore drill completed its checks but cleanup failed. Inspect the isolated compose project."
	exit 1
fi
write_report "passed" "Restore drill completed successfully in isolated Docker Compose project."
trap - ERR

echo "Restore drill passed."
echo "Report written to ${REPORT_PATH}"
