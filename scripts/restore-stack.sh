#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
	cat <<'EOF'
Usage: scripts/restore-stack.sh [--with-chat] <backup_dir>

Restores a Rustshare backup bundle created by scripts/backup-stack.sh.

This command will:
1. stop backend and nginx
2. recreate the PostgreSQL database from `postgres.sql.gz`
3. replace the RustFS data volume contents from `rustfs-data.tar.gz`
4. restart rustfs, backend, and nginx

Pass `--with-chat` to restore the complete bundled Alpha backup, including
Buzz PostgreSQL and dedicated Buzz RustFS state. Deployment secrets remain
external and must be restored separately; never regenerate them during a
restore.

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

WITH_CHAT=false
if [[ "${1:-}" == "--with-chat" ]]; then
	WITH_CHAT=true
	shift
fi

if [[ $# -lt 1 ]]; then
	usage
	exit 1
fi

require_file() {
	local file="$1"
	if [[ ! -f "${file}" ]]; then
		echo "Required backup artifact missing: ${file}" >&2
		exit 1
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
		container_id="$(compose ps -q "${service}")"
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

cd "${PROJECT_ROOT}"

if [[ "${WITH_CHAT}" == true ]]; then
	# shellcheck disable=SC1091
	. ./config/buzz-compatibility.env
	BUZZ_POSTGRES_SERVICE="${BUZZ_POSTGRES_SERVICE:-buzz-postgres}"
	BUZZ_RUSTFS_SERVICE="${BUZZ_RUSTFS_SERVICE:-buzz-rustfs}"
fi

compose() {
	if [[ "${WITH_CHAT}" == true ]]; then
		docker compose -f docker-compose.yml -f docker-compose.alpha.yml \
			-f docker-compose.dogfood.yml --profile chat "$@"
	else
		docker compose "$@"
	fi
}

require_file "${BACKUP_DIR}/postgres.sql.gz"
require_file "${BACKUP_DIR}/rustfs-data.tar.gz"

if [[ "${WITH_CHAT}" == true ]]; then
	require_file "${BACKUP_DIR}/buzz-postgres.sql.gz"
	require_file "${BACKUP_DIR}/buzz-rustfs-data.tar.gz"
fi

echo "Starting core services..."
if [[ "${WITH_CHAT}" == true ]]; then
	compose up -d "${POSTGRES_SERVICE}" "${RUSTFS_SERVICE}" \
		"${BUZZ_POSTGRES_SERVICE}" "${BUZZ_RUSTFS_SERVICE}" buzz-redis
else
	compose up -d "${POSTGRES_SERVICE}" "${RUSTFS_SERVICE}"
fi
wait_for_healthy "${POSTGRES_SERVICE}"
wait_for_healthy "${RUSTFS_SERVICE}"
if [[ "${WITH_CHAT}" == true ]]; then
	wait_for_healthy "${BUZZ_POSTGRES_SERVICE}"
	wait_for_healthy "${BUZZ_RUSTFS_SERVICE}"
	wait_for_healthy buzz-redis
fi

echo "Stopping application traffic..."
compose stop "${BACKEND_SERVICE}" "${EDGE_SERVICE}" >/dev/null 2>&1 || true
if [[ "${WITH_CHAT}" == true ]]; then
	# Stop every writer before restoring either Buzz store. In particular, the
	# relay shares the backend network namespace in the bundled topology.
	compose stop buzz-relay chat-observer >/dev/null 2>&1 || true
fi

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

if [[ "${WITH_CHAT}" == true ]]; then
	echo "Restoring Buzz PostgreSQL database..."
	compose exec -T "${BUZZ_POSTGRES_SERVICE}" \
		psql -U buzz -d postgres -v ON_ERROR_STOP=1 \
		-c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = 'buzz' AND pid <> pg_backend_pid();" \
		-c 'DROP DATABASE IF EXISTS buzz;' \
		-c 'CREATE DATABASE buzz;'
	gunzip -c "${BACKUP_DIR}/buzz-postgres.sql.gz" | compose exec -T "${BUZZ_POSTGRES_SERVICE}" \
		psql -U buzz -d buzz -v ON_ERROR_STOP=1

	echo "Restoring dedicated Buzz RustFS volume snapshot..."
	BUZZ_RUSTFS_CONTAINER_ID="$(compose ps -q "${BUZZ_RUSTFS_SERVICE}")"
	if [[ -z "${BUZZ_RUSTFS_CONTAINER_ID}" ]]; then
		echo "Could not determine Buzz RustFS container ID." >&2
		exit 1
	fi
	BUZZ_RUSTFS_VOLUME_NAME="$(docker inspect --format '{{range .Mounts}}{{if eq .Destination "/data"}}{{.Name}}{{end}}{{end}}' "${BUZZ_RUSTFS_CONTAINER_ID}")"
	if [[ -z "${BUZZ_RUSTFS_VOLUME_NAME}" ]]; then
		echo "Could not determine Buzz RustFS data volume name." >&2
		exit 1
	fi
	compose stop "${BUZZ_RUSTFS_SERVICE}"
	docker run --rm -i \
		-v "${BUZZ_RUSTFS_VOLUME_NAME}:/data" \
		alpine:3.21 \
		sh -lc 'rm -rf /data/* /data/.[!.]* /data/..?* 2>/dev/null || true; tar -xzf - -C /data' \
		<"${BACKUP_DIR}/buzz-rustfs-data.tar.gz"
fi

echo "Restarting services..."
if [[ "${WITH_CHAT}" == true ]]; then
	compose --profile chat up -d
else
	compose up -d "${RUSTFS_SERVICE}" "${BACKEND_SERVICE}" "${EDGE_SERVICE}"
fi
wait_for_healthy "${RUSTFS_SERVICE}"
wait_for_healthy "${BACKEND_SERVICE}"
wait_for_healthy "${EDGE_SERVICE}"
if [[ "${WITH_CHAT}" == true ]]; then
	wait_for_healthy buzz-relay
	wait_for_healthy chat-observer
fi

echo "Restore completed from ${BACKUP_DIR}"
