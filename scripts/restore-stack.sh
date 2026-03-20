#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
	cat <<'EOF'
Usage: scripts/restore-stack.sh <backup_dir>

Restores a Rustshare backup bundle created by scripts/backup-stack.sh.

This command will:
1. stop backend and nginx
2. recreate the PostgreSQL database from `postgres.sql.gz`
3. replace the RustFS data volume contents from `rustfs-data.tar.gz`
4. restart rustfs, backend, and nginx

Environment overrides:
- POSTGRES_SERVICE (default: postgres)
- POSTGRES_DB (default: rustshare)
- POSTGRES_USER (default: rustshare)
- RUSTFS_SERVICE (default: rustfs)
- BACKEND_SERVICE (default: backend)
- EDGE_SERVICE (default: nginx)
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" || $# -lt 1 ]]; then
	usage
	exit $(( $# < 1 ))
fi

compose() {
	if docker compose version >/dev/null 2>&1; then
		docker compose "$@"
	else
		docker-compose "$@"
	fi
}

require_file() {
	local file="$1"
	if [[ ! -f "${file}" ]]; then
		echo "Required backup artifact missing: ${file}" >&2
		exit 1
	fi
}

BACKUP_DIR="$(cd "$1" && pwd)"
POSTGRES_SERVICE="${POSTGRES_SERVICE:-postgres}"
POSTGRES_DB="${POSTGRES_DB:-rustshare}"
POSTGRES_USER="${POSTGRES_USER:-rustshare}"
RUSTFS_SERVICE="${RUSTFS_SERVICE:-rustfs}"
BACKEND_SERVICE="${BACKEND_SERVICE:-backend}"
EDGE_SERVICE="${EDGE_SERVICE:-nginx}"

require_file "${BACKUP_DIR}/postgres.sql.gz"
require_file "${BACKUP_DIR}/rustfs-data.tar.gz"

cd "${PROJECT_ROOT}"

echo "Starting core services..."
compose up -d "${POSTGRES_SERVICE}" "${RUSTFS_SERVICE}"

echo "Stopping application traffic..."
compose stop "${BACKEND_SERVICE}" "${EDGE_SERVICE}" >/dev/null 2>&1 || true

echo "Restoring PostgreSQL database..."
compose exec -T "${POSTGRES_SERVICE}" \
	psql -U "${POSTGRES_USER}" -d postgres -v ON_ERROR_STOP=1 \
	-c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${POSTGRES_DB}' AND pid <> pg_backend_pid();" \
	-c "DROP DATABASE IF EXISTS \"${POSTGRES_DB}\";" \
	-c "CREATE DATABASE \"${POSTGRES_DB}\";"

gunzip -c "${BACKUP_DIR}/postgres.sql.gz" | compose exec -T "${POSTGRES_SERVICE}" \
	psql -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" -v ON_ERROR_STOP=1

echo "Restoring RustFS volume snapshot..."
	RUSTFS_CONTAINER_ID="$(compose ps -q "${RUSTFS_SERVICE}")"
if [[ -z "${RUSTFS_CONTAINER_ID}" ]]; then
	echo "Could not determine RustFS container ID." >&2
	exit 1
fi

RUSTFS_VOLUME_NAME="$(docker inspect --format '{{range .Mounts}}{{if eq .Destination "/data"}}{{.Name}}{{end}}{{end}}' "${RUSTFS_CONTAINER_ID}")"
if [[ -z "${RUSTFS_VOLUME_NAME}" ]]; then
	echo "Could not determine RustFS data volume name." >&2
	exit 1
fi

compose stop "${RUSTFS_SERVICE}"

docker run --rm -i \
	-v "${RUSTFS_VOLUME_NAME}:/data" \
	alpine:3.21 \
	sh -lc 'rm -rf /data/* /data/.[!.]* /data/..?* 2>/dev/null || true; tar -xzf - -C /data' \
	<"${BACKUP_DIR}/rustfs-data.tar.gz"

echo "Restarting services..."
compose up -d "${RUSTFS_SERVICE}" "${BACKEND_SERVICE}" "${EDGE_SERVICE}"

echo "Restore completed from ${BACKUP_DIR}"
