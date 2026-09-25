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

Pass `--with-chat` for the complete Alpha bundle. It additionally captures
the dedicated Buzz PostgreSQL and RustFS state. Deployment secrets and
`.elembra/chat.env` are intentionally never copied into the bundle; keep them
in the encrypted secrets backup described by the Alpha runbook.

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
	if ! compose ps --services --status running | grep -qx "${service}"; then
		echo "Service '${service}' is not running. Start the stack before creating a backup." >&2
		exit 1
	fi
}

require_container_id() {
	local service="$1"
	local container_id
	container_id="$(compose ps -q "${service}")"
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

snapshot_volume() {
	local service="$1"
	local mount_path="$2"
	local output_file="$3"
	local container_id volume_name
	container_id="$(require_container_id "${service}")"
	volume_name="$(require_named_volume_for_mount "${container_id}" "${mount_path}")"
	docker run --rm \
		-v "${volume_name}:${mount_path}:ro" \
		alpine:3.21 \
		sh -lc "tar -czf - -C '${mount_path}' ." >"${output_file}"
}

WITH_CHAT=false
if [[ "${1:-}" == "--with-chat" ]]; then
	WITH_CHAT=true
	shift
fi

POSTGRES_SERVICE="${POSTGRES_SERVICE:-postgres}"
POSTGRES_DB="${POSTGRES_DB:-rustshare}"
POSTGRES_USER="${POSTGRES_USER:-rustshare}"
RUSTFS_SERVICE="${RUSTFS_SERVICE:-rustfs}"
BACKUP_ROOT="${1:-${PROJECT_ROOT}/backups}"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
TARGET_DIR="${BACKUP_ROOT%/}/${TIMESTAMP}"

mkdir -p "${TARGET_DIR}"

cd "${PROJECT_ROOT}"

if [[ "${WITH_CHAT}" == true && -f config/buzz-compatibility.env ]]; then
	# shellcheck disable=SC1091
	. ./config/buzz-compatibility.env
fi

compose() {
	if [[ "${WITH_CHAT}" == true ]]; then
		docker compose -f docker-compose.yml -f docker-compose.alpha.yml \
			-f docker-compose.dogfood.yml --profile chat "$@"
	else
		docker compose "$@"
	fi
}

require_service_running "${POSTGRES_SERVICE}"
require_service_running "${RUSTFS_SERVICE}"

echo "Creating PostgreSQL backup..."
compose exec -T "${POSTGRES_SERVICE}" \
	pg_dump -U "${POSTGRES_USER}" "${POSTGRES_DB}" | gzip -c >"${TARGET_DIR}/postgres.sql.gz"

echo "Creating RustFS volume snapshot..."
snapshot_volume "${RUSTFS_SERVICE}" "/data" "${TARGET_DIR}/rustfs-data.tar.gz"

if [[ "${WITH_CHAT}" == true ]]; then
	BUZZ_POSTGRES_SERVICE="${BUZZ_POSTGRES_SERVICE:-buzz-postgres}"
	BUZZ_RUSTFS_SERVICE="${BUZZ_RUSTFS_SERVICE:-buzz-rustfs}"
	require_service_running "${BUZZ_POSTGRES_SERVICE}"
	require_service_running "${BUZZ_RUSTFS_SERVICE}"
	echo "Creating Buzz PostgreSQL backup..."
	compose exec -T "${BUZZ_POSTGRES_SERVICE}" \
		pg_dump -U buzz buzz | gzip -c >"${TARGET_DIR}/buzz-postgres.sql.gz"
	echo "Creating dedicated Buzz RustFS volume snapshot..."
	snapshot_volume "${BUZZ_RUSTFS_SERVICE}" "/data" "${TARGET_DIR}/buzz-rustfs-data.tar.gz"
fi

echo "Creating configuration snapshot..."
tar -czf "${TARGET_DIR}/config.tar.gz" \
	docker-compose.yml \
	docker-compose.alpha.yml \
	docker-compose.dogfood.yml \
	docker-compose.pilot.yml \
	docker-compose.dev.yml \
	docker \
	scripts \
	config/buzz-compatibility.env \
	README.md \
	docs/PRODUCTION_READINESS.md \
	>/dev/null 2>&1

cat >"${TARGET_DIR}/manifest.env" <<EOF
BACKUP_TIMESTAMP=${TIMESTAMP}
BACKUP_CREATED_AT_UTC=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
POSTGRES_SERVICE=${POSTGRES_SERVICE}
POSTGRES_DB=${POSTGRES_DB}
POSTGRES_USER=${POSTGRES_USER}
RUSTFS_SERVICE=${RUSTFS_SERVICE}
CHAT_BACKUP=${WITH_CHAT}
BUZZ_POSTGRES_SERVICE=${BUZZ_POSTGRES_SERVICE:-}
BUZZ_RUSTFS_SERVICE=${BUZZ_RUSTFS_SERVICE:-}
CHAT_SECRETS_EXTERNAL=.env,.elembra/chat.env
GIT_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)
GIT_COMMIT=$(git rev-parse HEAD 2>/dev/null || echo unknown)
EOF

if command -v shasum >/dev/null 2>&1; then
	(
		cd "${TARGET_DIR}"
		files=(postgres.sql.gz rustfs-data.tar.gz config.tar.gz manifest.env)
		[[ -f buzz-postgres.sql.gz ]] && files+=(buzz-postgres.sql.gz)
		[[ -f buzz-rustfs-data.tar.gz ]] && files+=(buzz-rustfs-data.tar.gz)
		shasum -a 256 "${files[@]}" >SHA256SUMS
	)
fi

echo "Backup created at ${TARGET_DIR}"
