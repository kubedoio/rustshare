#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
	cat <<'EOF'
Usage: scripts/restore-stack.sh <backup_dir>

Restores a Rustshare backup bundle created by scripts/backup-stack.sh.

This command will:
1. verify the bundle checksums (SHA256SUMS, when present) before touching any state
2. stop backend and nginx
3. recreate the PostgreSQL database from `postgres.sql.gz`
4. replace the RustFS data volume contents from `rustfs-data.tar.gz`
5. restart rustfs, backend, and nginx

Note: `config.tar.gz` (the deployment/config snapshot) is kept as a reference
artifact only; this script does not restore it.

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

require_file() {
	local file="$1"
	if [[ ! -f "${file}" ]]; then
		echo "Required backup artifact missing: ${file}" >&2
		exit 1
	fi
}

# Verify the bundle's SHA256SUMS (written by scripts/backup-stack.sh) before
# any state is touched. Old bundles without a checksum file are accepted with
# a warning; a present but mismatching checksum aborts the restore.
verify_bundle_checksums() {
	local sums_file="${BACKUP_DIR}/SHA256SUMS"
	if [[ ! -f "${sums_file}" ]]; then
		echo "No SHA256SUMS found in ${BACKUP_DIR}; skipping checksum verification." >&2
		return 0
	fi

	local checksum_cmd=()
	if command -v sha256sum >/dev/null 2>&1; then
		checksum_cmd=(sha256sum -c "${sums_file}")
	elif command -v shasum >/dev/null 2>&1; then
		checksum_cmd=(shasum -a 256 -c "${sums_file}")
	else
		echo "Neither sha256sum nor shasum is available; cannot verify SHA256SUMS." >&2
		return 1
	fi

	if ( cd "${BACKUP_DIR}" && "${checksum_cmd[@]}" ); then
		echo "Checksums verified: ${sums_file}"
	else
		echo "Checksum verification FAILED; aborting before touching any state." >&2
		return 1
	fi
}

wait_for_healthy() {
	local service="$1"
	local timeout_seconds="${2:-60}"
	local container_id
	local started_at
	local health_status

	started_at="$(date +%s)"
	while true; do
		container_id="$(docker compose ps -q "${service}")"
		if [[ -z "${container_id}" ]]; then
			if (( $(date +%s) - started_at >= timeout_seconds )); then
				echo "Could not determine container ID for service '${service}'." >&2
				exit 1
			fi

			sleep 2
			continue
		fi

		health_status="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "${container_id}")"
		if [[ "${health_status}" == "healthy" || "${health_status}" == "running" ]]; then
			return
		fi

		if (( $(date +%s) - started_at >= timeout_seconds )); then
			echo "Service '${service}' did not become healthy within ${timeout_seconds}s." >&2
			exit 1
		fi

		sleep 2
	done
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

verify_bundle_checksums

cd "${PROJECT_ROOT}"

echo "Starting core services..."
docker compose up -d "${POSTGRES_SERVICE}" "${RUSTFS_SERVICE}"
wait_for_healthy "${POSTGRES_SERVICE}"
wait_for_healthy "${RUSTFS_SERVICE}"

echo "Stopping application traffic..."
docker compose stop "${BACKEND_SERVICE}" "${EDGE_SERVICE}" >/dev/null 2>&1 || true

echo "Restoring PostgreSQL database..."
docker compose exec -T "${POSTGRES_SERVICE}" \
	psql -U "${POSTGRES_USER}" -d postgres -v ON_ERROR_STOP=1 \
	-c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '${POSTGRES_DB}' AND pid <> pg_backend_pid();" \
	-c "DROP DATABASE IF EXISTS \"${POSTGRES_DB}\";" \
	-c "CREATE DATABASE \"${POSTGRES_DB}\";"

gunzip -c "${BACKUP_DIR}/postgres.sql.gz" | docker compose exec -T "${POSTGRES_SERVICE}" \
	psql -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" -v ON_ERROR_STOP=1

echo "Restoring RustFS volume snapshot..."
	RUSTFS_CONTAINER_ID="$(docker compose ps -q "${RUSTFS_SERVICE}")"
if [[ -z "${RUSTFS_CONTAINER_ID}" ]]; then
	echo "Could not determine RustFS container ID." >&2
	exit 1
fi

RUSTFS_VOLUME_NAME="$(docker inspect --format '{{range .Mounts}}{{if eq .Destination "/data"}}{{.Name}}{{end}}{{end}}' "${RUSTFS_CONTAINER_ID}")"
if [[ -z "${RUSTFS_VOLUME_NAME}" ]]; then
	echo "Could not determine RustFS data volume name." >&2
	exit 1
fi

docker compose stop "${RUSTFS_SERVICE}"

docker run --rm -i \
	-v "${RUSTFS_VOLUME_NAME}:/data" \
	alpine:3.21 \
	sh -lc 'rm -rf /data/* /data/.[!.]* /data/..?* 2>/dev/null || true; tar -xzf - -C /data' \
	<"${BACKUP_DIR}/rustfs-data.tar.gz"

echo "Restarting services..."
docker compose up -d "${RUSTFS_SERVICE}" "${BACKEND_SERVICE}" "${EDGE_SERVICE}"
wait_for_healthy "${RUSTFS_SERVICE}"
wait_for_healthy "${BACKEND_SERVICE}"
wait_for_healthy "${EDGE_SERVICE}"

echo "Restore completed from ${BACKUP_DIR}"
