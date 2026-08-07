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

require_service_running() {
	local service="$1"
	if ! docker compose ps --services --status running | grep -qx "${service}"; then
		echo "Service '${service}' is not running. Start the stack before creating a backup." >&2
		exit 1
	fi
}

require_container_id() {
	local service="$1"
	local container_id
	container_id="$(docker compose ps -q "${service}")"
	if [[ -z "${container_id}" ]]; then
		echo "Could not determine container ID for service '${service}'." >&2
		exit 1
	fi

	echo "${container_id}"
}

require_named_volume_for_mount() {
	local container_id="$1"
	local mount_path="$2"
	local volume_name
	volume_name="$(docker inspect --format "{{range .Mounts}}{{if eq .Destination \"${mount_path}\"}}{{.Name}}{{end}}{{end}}" "${container_id}")"
	if [[ -z "${volume_name}" ]]; then
		echo "Could not determine named volume mounted at '${mount_path}'." >&2
		exit 1
	fi

	echo "${volume_name}"
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
docker compose exec -T "${POSTGRES_SERVICE}" \
	pg_dump -U "${POSTGRES_USER}" "${POSTGRES_DB}" | gzip -c >"${TARGET_DIR}/postgres.sql.gz"

echo "Creating RustFS volume snapshot..."
RUSTFS_CONTAINER_ID="$(require_container_id "${RUSTFS_SERVICE}")"
RUSTFS_VOLUME_NAME="$(require_named_volume_for_mount "${RUSTFS_CONTAINER_ID}" "/data")"

docker run --rm \
	-v "${RUSTFS_VOLUME_NAME}:/data:ro" \
	alpine:3.21 \
	sh -lc 'tar -czf - -C /data .' >"${TARGET_DIR}/rustfs-data.tar.gz"

echo "Creating configuration snapshot..."
tar -czf "${TARGET_DIR}/config.tar.gz" \
	docker-compose.yml \
	docker-compose.dev.yml \
	docker \
	scripts \
	README.md \
	PRODUCTION_READINESS.md \
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
