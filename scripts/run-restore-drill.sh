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
- DRILL_KEEP_STACK (default: false)
- DRILL_REPORT_DIR (default: ./restore-drill-reports)
- ADMIN_EMAIL (default: admin@localhost)
- ADMIN_PASSWORD (default: )
- PUBLIC_SHARE_TOKEN (optional)
- PUBLIC_SHARE_PASSWORD (optional)
- ALLOW_SKIP_PUBLIC_SHARE (default: true)
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
BACKUP_DIR=${BACKUP_DIR}
REPORT_DETAILS=${details}
EOF
}

require_command docker
require_command bash

BACKUP_DIR="$(cd "$1" && pwd)"
DRILL_PROJECT_NAME="${DRILL_PROJECT_NAME:-rustshare-restore-drill}"
DRILL_COMPOSE_FILE="${DRILL_COMPOSE_FILE:-${PROJECT_ROOT}/docker-compose.restore-drill.yml}"
DRILL_BASE_URL="${DRILL_BASE_URL:-http://localhost:18080}"
DRILL_API_BASE_URL="${DRILL_API_BASE_URL:-${DRILL_BASE_URL%/}/api/v1}"
DRILL_KEEP_STACK="${DRILL_KEEP_STACK:-false}"
DRILL_REPORT_DIR="${DRILL_REPORT_DIR:-${PROJECT_ROOT}/restore-drill-reports}"
ALLOW_SKIP_PUBLIC_SHARE="${ALLOW_SKIP_PUBLIC_SHARE:-true}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@localhost}"
export RUSTSHARE_ADMIN_PASSWORD="${ADMIN_PASSWORD}"

DRILL_STARTED_AT="$(timestamp)"
REPORT_BASENAME="$(date -u +%Y%m%dT%H%M%SZ)-restore-drill.env"
REPORT_PATH="${DRILL_REPORT_DIR%/}/${REPORT_BASENAME}"

export COMPOSE_PROJECT_NAME="${DRILL_PROJECT_NAME}"
export COMPOSE_FILE="${DRILL_COMPOSE_FILE}"

cleanup() {
	if [[ "${DRILL_KEEP_STACK}" == "true" ]]; then
		return
	fi

	docker compose down -v --remove-orphans >/dev/null 2>&1 || true
}

trap 'write_report "failed" "Restore drill failed. Inspect the isolated compose project logs."; cleanup' ERR

cd "${PROJECT_ROOT}"

echo "Verifying backup bundle..."
"${PROJECT_ROOT}/scripts/verify-backup-bundle.sh" "${BACKUP_DIR}"

echo "Resetting isolated drill stack..."
docker compose down -v --remove-orphans >/dev/null 2>&1 || true

echo "Building isolated backend image from current workspace..."
docker compose build backend

echo "Restoring backup into isolated project '${DRILL_PROJECT_NAME}'..."
"${PROJECT_ROOT}/scripts/restore-stack.sh" "${BACKUP_DIR}"

echo "Checking isolated stack health..."
docker compose ps
curl -fsS "${DRILL_BASE_URL%/}/health" >/dev/null
curl -fsS "http://localhost:18081/health" >/dev/null

echo "Running post-restore smoke test..."
BASE_URL="${DRILL_BASE_URL}" \
API_BASE_URL="${DRILL_API_BASE_URL}" \
ALLOW_SKIP_PUBLIC_SHARE="${ALLOW_SKIP_PUBLIC_SHARE}" \
ADMIN_EMAIL="${ADMIN_EMAIL}" \
ADMIN_PASSWORD="${ADMIN_PASSWORD}" \
PUBLIC_SHARE_TOKEN="${PUBLIC_SHARE_TOKEN:-}" \
PUBLIC_SHARE_PASSWORD="${PUBLIC_SHARE_PASSWORD:-}" \
	"${PROJECT_ROOT}/scripts/post-restore-smoke.sh"

write_report "passed" "Restore drill completed successfully in isolated Docker Compose project."
cleanup
trap - ERR

echo "Restore drill passed."
echo "Report written to ${REPORT_PATH}"
