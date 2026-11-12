#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TEMP_DIR}"' EXIT

mkdir -p "${TEMP_DIR}/bin" "${TEMP_DIR}/backup"
for file in postgres.sql.gz rustfs-data.tar.gz buzz-postgres.sql.gz buzz-rustfs-data.tar.gz; do
	: >"${TEMP_DIR}/backup/${file}"
done
gzip -c </dev/null >"${TEMP_DIR}/backup/postgres.sql.gz"
gzip -c </dev/null >"${TEMP_DIR}/backup/rustfs-data.tar.gz"
gzip -c </dev/null >"${TEMP_DIR}/backup/buzz-postgres.sql.gz"
gzip -c </dev/null >"${TEMP_DIR}/backup/buzz-rustfs-data.tar.gz"

cat >"${TEMP_DIR}/bin/docker" <<'DOCKER'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${DOCKER_CALLS_FILE}"
case "$*" in
	*" ps -q "*) echo fake-container ;;
	"inspect --format "*) echo healthy ;;
	esac
DOCKER
chmod +x "${TEMP_DIR}/bin/docker"

cat >"${TEMP_DIR}/base.yml" <<'YAML'
services:
  postgres: {image: postgres:16}
  rustfs: {image: rustfs/rustfs:latest}
  backend: {image: rustshare-backend:test}
  nginx: {image: nginx:alpine}
  buzz-postgres: {image: postgres:16}
  buzz-rustfs: {image: rustfs/rustfs:latest}
  buzz-redis: {image: redis:7}
  buzz-relay: {image: buzz-relay:test}
  chat-observer: {image: rustshare-observer:test}
YAML
cat >"${TEMP_DIR}/extra.yml" <<'YAML'
services:
  buzz-relay:
    image: ghcr.io/kubedoio/buzz-elembra@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
YAML
chmod 600 "${TEMP_DIR}/extra.yml"

export PATH="${TEMP_DIR}/bin:${PATH}"
export DOCKER_CALLS_FILE="${TEMP_DIR}/docker-calls"
export COMPOSE_FILE="${TEMP_DIR}/base.yml"
export ELEMBRA_COMPOSE_EXTRA_FILE="${TEMP_DIR}/extra.yml"
export DRILL_PROJECT_NAME="elembra-restore-compose-test"
export BUZZ_POSTGRES_SERVICE=buzz-postgres BUZZ_RUSTFS_SERVICE=buzz-rustfs

"${ROOT}/scripts/restore-stack.sh" --with-chat "${TEMP_DIR}/backup"

compose_calls="$(grep '^compose ' "${DOCKER_CALLS_FILE}")"
[[ -n "${compose_calls}" ]] || { echo "Restore did not call Docker Compose." >&2; exit 1; }
while IFS= read -r invocation; do
	if [[ "${invocation}" != *"-f ${TEMP_DIR}/extra.yml"* ]]; then
		echo "A restore Compose invocation omitted ELEMBRA_COMPOSE_EXTRA_FILE: ${invocation}" >&2
		exit 1
	fi
	if [[ "${invocation}" != *"-f docker-compose.dogfood.yml"* ]]; then
		echo "A restore Compose invocation omitted the dogfood network config: ${invocation}" >&2
		exit 1
	fi
done <<<"${compose_calls}"
echo "All restore Compose calls preserved dogfood networking and the explicit extra file."
