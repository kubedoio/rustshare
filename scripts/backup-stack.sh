#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
	cat <<'EOF'
Usage: scripts/backup-stack.sh [backup_root]

Creates a Docker-native Rustshare backup bundle containing:
- PostgreSQL logical dump (`postgres.sql.gz`)
- RustFS data volume snapshot (`rustfs-data.tar.gz`)
- Deployment/config snapshot (`config.tar.gz`)
- Backup manifest (`manifest.env`)

Environment overrides:
- POSTGRES_SERVICE (default: postgres)
- POSTGRES_DB (default: rustshare)
- POSTGRES_USER (default: rustshare)
- RUSTFS_SERVICE (default: rustfs)

The backup root defaults to ./backups and a timestamped subdirectory is created.
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
	usage
	exit 0
fi

compose() {
	if docker compose version >/dev/null 2>&1; then
		docker compose "$@"
	else
		docker-compose "$@"
	fi
}

require_service_running() {
	local service="$1"
	if ! compose ps --services --status running | grep -qx "${service}"; then
		echo "Service '${service}' is not running. Start the stack before creating a backup." >&2
		exit 1
	fi
}

POSTGRES_SERVICE="${POSTGRES_SERVICE:-postgres}"
POSTGRES_DB="${POSTGRES_DB:-rustshare}"
POSTGRES_USER="${POSTGRES_USER:-rustshare}"
RUSTFS_SERVICE="${RUSTFS_SERVICE:-rustfs}"
BACKUP_ROOT="${1:-${PROJECT_ROOT}/backups}"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
TARGET_DIR="${BACKUP_ROOT%/}/${TIMESTAMP}"

mkdir -p "${TARGET_DIR}"

cd "${PROJECT_ROOT}"

require_service_running "${POSTGRES_SERVICE}"
require_service_running "${RUSTFS_SERVICE}"

echo "Creating PostgreSQL backup..."
compose exec -T "${POSTGRES_SERVICE}" \
	pg_dump -U "${POSTGRES_USER}" "${POSTGRES_DB}" | gzip -c >"${TARGET_DIR}/postgres.sql.gz"

echo "Creating RustFS volume snapshot..."
compose exec -T "${RUSTFS_SERVICE}" \
	sh -lc 'tar -czf - -C /data .' >"${TARGET_DIR}/rustfs-data.tar.gz"

echo "Creating configuration snapshot..."
tar -czf "${TARGET_DIR}/config.tar.gz" \
	docker-compose.yml \
	docker-compose.dev.yml \
	docker \
	scripts \
	README.md \
	PRODUCTION_READINESS.md \
	docs/2026-03-20-rate-limit-hardening.md \
	docs/2026-03-20-backup-restore-runbook.md \
	>/dev/null 2>&1

cat >"${TARGET_DIR}/manifest.env" <<EOF
BACKUP_TIMESTAMP=${TIMESTAMP}
BACKUP_CREATED_AT_UTC=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
POSTGRES_SERVICE=${POSTGRES_SERVICE}
POSTGRES_DB=${POSTGRES_DB}
POSTGRES_USER=${POSTGRES_USER}
RUSTFS_SERVICE=${RUSTFS_SERVICE}
GIT_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
GIT_COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
EOF

if command -v shasum >/dev/null 2>&1; then
	(
		cd "${TARGET_DIR}"
		shasum -a 256 postgres.sql.gz rustfs-data.tar.gz config.tar.gz manifest.env >SHA256SUMS
	)
fi

echo "Backup created at ${TARGET_DIR}"
