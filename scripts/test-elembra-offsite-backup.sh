#!/usr/bin/env bash

set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUSTFS_IMAGE="rustfs/rustfs@sha256:60f4f2f41ce95216f8cac676e69f9d90c0bfec458a3bc7fd7fb9b7c2452ac57a"
POSTGRES_IMAGE="postgres@sha256:b0f9560a2de083e2cc7382e75f808c7381a32852a7ec49117deedb300e552b24"
ARCHIVE_IMAGE="alpine@sha256:ce64758a109eb420d874a118f87920e625e12d3634e03b4a5573fd9f6e5d3507"
AWS_CLI_IMAGE="amazon/aws-cli@sha256:3b018ce74732c98acf6f1de59b3a89587cb7f9eb6ea0d1447d1779091b2bf057"
TEST_ID="$(date -u +%Y%m%d%H%M%S)-$$"
PROJECT="elembra-backup-test-${TEST_ID}"
S3_CONTAINER="${PROJECT}-s3"
S3_VOLUME="${PROJECT}-s3-data"
TEST_ROOT="$(mktemp -d)"
S3_PORT=""
HEARTBEAT_PID=""

cleanup() {
	local status=$?
	trap - EXIT
	if [[ -f "${TEST_ROOT}/compose.yml" ]]; then
		docker compose --project-name "${PROJECT}" \
			--file "${TEST_ROOT}/compose.yml" \
			--file "${TEST_ROOT}/project/docker-compose.alpha.yml" \
			--file "${TEST_ROOT}/project/docker-compose.dogfood.yml" \
			down --volumes --remove-orphans >/dev/null 2>&1 || true
	fi
	if [[ -n "${S3_CONTAINER}" ]]; then docker rm --force "${S3_CONTAINER}" >/dev/null 2>&1 || true; fi
	if [[ -n "${S3_VOLUME}" ]]; then docker volume rm "${S3_VOLUME}" >/dev/null 2>&1 || true; fi
	if [[ -n "${HEARTBEAT_PID}" ]]; then kill "${HEARTBEAT_PID}" >/dev/null 2>&1 || true; fi
	rm -rf -- "${TEST_ROOT}"
	exit "${status}"
}
trap cleanup EXIT

for command in docker age age-keygen curl python3 git; do
	command -v "${command}" >/dev/null || { printf 'Required test command not found: %s\n' "${command}" >&2; exit 2; }
done
docker image inspect "${RUSTFS_IMAGE}" >/dev/null
docker image inspect "${POSTGRES_IMAGE}" >/dev/null
docker image inspect "${ARCHIVE_IMAGE}" >/dev/null
docker image inspect "${AWS_CLI_IMAGE}" >/dev/null

test_project="${TEST_ROOT}/project"
mkdir -p "${test_project}"
git -C "${ROOT}" archive HEAD | tar -x -C "${test_project}"
cp "${ROOT}/scripts/elembra-offsite-backup.sh" "${test_project}/scripts/"
mkdir -p "${test_project}/.elembra"
cat >"${test_project}/.env" <<EOF
COMPOSE_PROJECT_NAME=${PROJECT}
POSTGRES_USER=rustshare
POSTGRES_PASSWORD=disposable-test-db-password
POSTGRES_DB=rustshare
EOF
chmod 600 "${test_project}/.env"
cat >"${test_project}/.elembra/chat.env" <<'EOF'
BUZZ_TEST_RELAY_IDENTITY=disposable-only
EOF
chmod 600 "${test_project}/.elembra/chat.env"
cat >"${test_project}/.elembra/rustfs-kms.override.yml" <<'EOF'
services: {}
EOF
chmod 600 "${test_project}/.elembra/rustfs-kms.override.yml"

cat >"${TEST_ROOT}/compose.yml" <<EOF
services:
  postgres:
    image: ${POSTGRES_IMAGE}
    environment:
      POSTGRES_USER: rustshare
      POSTGRES_PASSWORD: disposable-test-db-password
      POSTGRES_DB: rustshare
    volumes: ["postgres-data:/var/lib/postgresql/data"]
  buzz-postgres:
    image: ${POSTGRES_IMAGE}
    environment:
      POSTGRES_USER: buzz
      POSTGRES_PASSWORD: disposable-test-buzz-password
      POSTGRES_DB: buzz
    volumes: ["buzz-postgres-data:/var/lib/postgresql/data"]
  rustfs:
    image: ${RUSTFS_IMAGE}
    environment:
      RUSTFS_ROOT_USER: disposable
      RUSTFS_ROOT_PASSWORD: disposable-rustfs-password
      RUSTFS_DATA_DIR: /data
      RUSTFS_LOGS_DIR: /logs
    volumes: ["rustfs-data:/data"]
  buzz-rustfs:
    image: ${RUSTFS_IMAGE}
    environment:
      RUSTFS_ROOT_USER: disposable
      RUSTFS_ROOT_PASSWORD: disposable-rustfs-password
      RUSTFS_DATA_DIR: /data
      RUSTFS_LOGS_DIR: /logs
    volumes: ["buzz-rustfs-data:/data"]
volumes:
  postgres-data:
  buzz-postgres-data:
  rustfs-data:
  buzz-rustfs-data:
EOF

docker volume create "${S3_VOLUME}" >/dev/null
docker run --detach --name "${S3_CONTAINER}" --publish 127.0.0.1::9000 \
	--env RUSTFS_ROOT_USER=disposableWriter \
	--env RUSTFS_ROOT_PASSWORD=disposable-s3-password \
	--env RUSTFS_DATA_DIR=/data --env RUSTFS_LOGS_DIR=/logs \
	--mount "type=volume,source=${S3_VOLUME},target=/data" "${RUSTFS_IMAGE}" >/dev/null
S3_PORT="$(docker inspect --format '{{(index (index .NetworkSettings.Ports "9000/tcp") 0).HostPort}}' "${S3_CONTAINER}")"

for _ in $(seq 1 60); do
	status="$(curl --silent --output /dev/null --write-out '%{http_code}' "http://127.0.0.1:${S3_PORT}/" || true)"
	[[ "${status}" == 403 || "${status}" == 200 ]] && break
	sleep 1
done
[[ "${status}" == 403 || "${status}" == 200 ]]

cat >"${TEST_ROOT}/heartbeat.py" <<'PY'
from http.server import BaseHTTPRequestHandler, HTTPServer
import sys

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        with open(sys.argv[2], "a", encoding="utf-8") as log:
            log.write(self.path + "\n")
        self.send_response(204)
        self.end_headers()
    def log_message(self, *_args):
        pass

server = HTTPServer(("127.0.0.1", 0), Handler)
with open(sys.argv[1], "w", encoding="utf-8") as port_file:
    port_file.write(str(server.server_port))
server.serve_forever()
PY
python3 "${TEST_ROOT}/heartbeat.py" "${TEST_ROOT}/heartbeat.port" "${TEST_ROOT}/heartbeat.log" &
HEARTBEAT_PID=$!
for _ in $(seq 1 50); do [[ -s "${TEST_ROOT}/heartbeat.port" ]] && break; sleep 0.1; done
HEARTBEAT_PORT="$(cat "${TEST_ROOT}/heartbeat.port")"

cat >"${TEST_ROOT}/bin-aws" <<EOF
#!/usr/bin/env bash
if [[ "\${ELEMBRA_TEST_AWS_FAIL:-}" == true && " \$* " == *' s3 cp '* ]]; then exit 42; fi
exec docker run --rm --network host \\
  --env AWS_ACCESS_KEY_ID --env AWS_SECRET_ACCESS_KEY --env AWS_DEFAULT_REGION --env AWS_EC2_METADATA_DISABLED \\
  --volume "${TEST_ROOT}:${TEST_ROOT}" --volume "${ROOT}:${ROOT}:ro" \\
  ${AWS_CLI_IMAGE} "\$@"
EOF
chmod 700 "${TEST_ROOT}/bin-aws"
mkdir "${TEST_ROOT}/bin"
mv "${TEST_ROOT}/bin-aws" "${TEST_ROOT}/bin/aws"
export PATH="${TEST_ROOT}/bin:${PATH}"
export AWS_ACCESS_KEY_ID=disposableWriter AWS_SECRET_ACCESS_KEY=disposable-s3-password AWS_DEFAULT_REGION=us-east-1
export ELEMBRA_BACKUP_S3_ENDPOINT="http://127.0.0.1:${S3_PORT}"
export ELEMBRA_BACKUP_S3_BUCKET="elembra-test-${TEST_ID//[^a-z0-9]/}"
export ELEMBRA_BACKUP_HEARTBEAT_URL="http://127.0.0.1:${HEARTBEAT_PORT}"
export ELEMBRA_BACKUP_TEST_ALLOW_HTTP_LOOPBACK=true
export ELEMBRA_BACKUP_STAGING_DIR="${TEST_ROOT}/staging"
export COMPOSE_FILE="${TEST_ROOT}/compose.yml"
export COMPOSE_PROJECT_NAME="${PROJECT}"
export POSTGRES_USER=rustshare POSTGRES_DB=rustshare
export BUZZ_POSTGRES_SERVICE=buzz-postgres BUZZ_RUSTFS_SERVICE=buzz-rustfs

aws --endpoint-url "${ELEMBRA_BACKUP_S3_ENDPOINT}" s3api create-bucket --bucket "${ELEMBRA_BACKUP_S3_BUCKET}" >/dev/null
aws --endpoint-url "${ELEMBRA_BACKUP_S3_ENDPOINT}" s3api put-bucket-lifecycle-configuration \
	--bucket "${ELEMBRA_BACKUP_S3_BUCKET}" \
	--lifecycle-configuration "file://${ROOT}/ops/s3/elembra-lifecycle.json" >/dev/null
aws --endpoint-url "${ELEMBRA_BACKUP_S3_ENDPOINT}" s3api get-bucket-lifecycle-configuration \
	--bucket "${ELEMBRA_BACKUP_S3_BUCKET}" --query 'Rules[0].Expiration.Days' --output text | grep -qx 90

docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" up --detach >/dev/null
for _ in $(seq 1 60); do
	if docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" exec -T postgres pg_isready -U rustshare >/dev/null 2>&1 && \
		docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" exec -T buzz-postgres pg_isready -U buzz >/dev/null 2>&1; then break; fi
	sleep 1
done
docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" exec -T postgres \
	psql -U rustshare -d rustshare -c 'CREATE TABLE backup_fixture (id integer PRIMARY KEY); INSERT INTO backup_fixture VALUES (471);' >/dev/null
docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" exec -T buzz-postgres \
	psql -U buzz -d buzz -c 'CREATE TABLE backup_fixture (id integer PRIMARY KEY); INSERT INTO backup_fixture VALUES (472);' >/dev/null
docker run --rm --volume "${PROJECT}_rustfs-data:/data" "${ARCHIVE_IMAGE}" \
	sh -c 'printf "elembra fixture\n" >/data/backup-fixture.txt'
docker run --rm --volume "${PROJECT}_buzz-rustfs-data:/data" "${ARCHIVE_IMAGE}" \
	sh -c 'printf "buzz fixture\n" >/data/backup-fixture.txt'

age-keygen -o "${TEST_ROOT}/age-identity.txt" 2>/dev/null
export ELEMBRA_BACKUP_AGE_RECIPIENT="$(age-keygen -y "${TEST_ROOT}/age-identity.txt")"
(cd "${test_project}" && ./scripts/elembra-offsite-backup.sh)

object="$(aws --endpoint-url "${ELEMBRA_BACKUP_S3_ENDPOINT}" s3api list-objects-v2 \
	--bucket "${ELEMBRA_BACKUP_S3_BUCKET}" --prefix elembra/ --query 'Contents[?ends_with(Key, `.age`)].Key | [0]' --output text)"
[[ "${object}" == elembra/*.tar.gz.age ]]
filename="$(basename "${object}")"
aws --endpoint-url "${ELEMBRA_BACKUP_S3_ENDPOINT}" s3 cp \
	"s3://${ELEMBRA_BACKUP_S3_BUCKET}/${object}" "${TEST_ROOT}/${filename}" --only-show-errors
aws --endpoint-url "${ELEMBRA_BACKUP_S3_ENDPOINT}" s3 cp \
	"s3://${ELEMBRA_BACKUP_S3_BUCKET}/${object}.sha256" "${TEST_ROOT}/${filename}.sha256" --only-show-errors
(cd "${TEST_ROOT}" && sha256sum --check "${filename}.sha256")
age --decrypt --identity "${TEST_ROOT}/age-identity.txt" --output "${TEST_ROOT}/restored.tar.gz" "${TEST_ROOT}/${filename}"
tar -tzf "${TEST_ROOT}/restored.tar.gz" >"${TEST_ROOT}/archive-files.txt"
grep -q '/postgres.sql.gz$' "${TEST_ROOT}/archive-files.txt"
grep -q '/buzz-postgres.sql.gz$' "${TEST_ROOT}/archive-files.txt"
grep -qx '.env' "${TEST_ROOT}/archive-files.txt"
grep -qx '.elembra/chat.env' "${TEST_ROOT}/archive-files.txt"
grep -qx '.elembra/rustfs-kms.override.yml' "${TEST_ROOT}/archive-files.txt"
postgres_dump="$(grep '/postgres.sql.gz$' "${TEST_ROOT}/archive-files.txt" | head -n 1)"
buzz_dump="$(grep '/buzz-postgres.sql.gz$' "${TEST_ROOT}/archive-files.txt" | head -n 1)"
tar -xOzf "${TEST_ROOT}/restored.tar.gz" "${postgres_dump}" | gzip -dc | grep -q 'backup_fixture'
tar -xOzf "${TEST_ROOT}/restored.tar.gz" "${buzz_dump}" | gzip -dc | grep -q 'backup_fixture'
grep -q '471' <(tar -xOzf "${TEST_ROOT}/restored.tar.gz" "${postgres_dump}" | gzip -dc)
grep -q '472' <(tar -xOzf "${TEST_ROOT}/restored.tar.gz" "${buzz_dump}" | gzip -dc)
timestamp="$(basename "${object}" .tar.gz.age)"
tar -xOzf "${TEST_ROOT}/restored.tar.gz" "${timestamp}/rustfs-data.tar.gz" \
	| tar -tzf - | grep -q 'backup-fixture.txt'
tar -xOzf "${TEST_ROOT}/restored.tar.gz" "${timestamp}/buzz-rustfs-data.tar.gz" \
	| tar -tzf - | grep -q 'backup-fixture.txt'

mkdir -m 700 "${TEST_ROOT}/restored"
tar -xzf "${TEST_ROOT}/restored.tar.gz" -C "${TEST_ROOT}/restored"
restored_bundle="${TEST_ROOT}/restored/${timestamp}"
docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" exec -T postgres \
	psql -v ON_ERROR_STOP=1 -U rustshare -d rustshare -c 'UPDATE backup_fixture SET id = 999' >/dev/null
docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" exec -T buzz-postgres \
	psql -v ON_ERROR_STOP=1 -U buzz -d buzz -c 'UPDATE backup_fixture SET id = 999' >/dev/null
docker run --rm --volume "${PROJECT}_rustfs-data:/data" "${ARCHIVE_IMAGE}" \
	sh -c 'printf "mutated\n" >/data/backup-fixture.txt'
docker run --rm --volume "${PROJECT}_buzz-rustfs-data:/data" "${ARCHIVE_IMAGE}" \
	sh -c 'printf "mutated\n" >/data/backup-fixture.txt'
docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" stop rustfs buzz-rustfs >/dev/null
for db in postgres buzz-postgres; do
	if [[ "${db}" == postgres ]]; then user=rustshare; database=rustshare; else user=buzz; database=buzz; fi
	docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" exec -T "${db}" \
		psql -v ON_ERROR_STOP=1 -U "${user}" -d postgres -c "DROP DATABASE ${database}" -c "CREATE DATABASE ${database}" >/dev/null
done
gunzip -c "${restored_bundle}/postgres.sql.gz" | docker compose --project-name "${PROJECT}" \
	--file "${TEST_ROOT}/compose.yml" exec -T postgres psql -v ON_ERROR_STOP=1 -U rustshare -d rustshare
gunzip -c "${restored_bundle}/buzz-postgres.sql.gz" | docker compose --project-name "${PROJECT}" \
	--file "${TEST_ROOT}/compose.yml" exec -T buzz-postgres psql -v ON_ERROR_STOP=1 -U buzz -d buzz
for service in rustfs buzz-rustfs; do
	if [[ "${service}" == rustfs ]]; then file=rustfs-data.tar.gz; volume="${PROJECT}_rustfs-data"; else file=buzz-rustfs-data.tar.gz; volume="${PROJECT}_buzz-rustfs-data"; fi
	docker run --rm --interactive --volume "${volume}:/data" "${ARCHIVE_IMAGE}" \
		sh -lc 'rm -rf /data/* /data/.[!.]* /data/..?* 2>/dev/null || true; tar -xzf - -C /data' \
		<"${restored_bundle}/${file}"
done
docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" start rustfs buzz-rustfs >/dev/null
[[ "$(docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" exec -T postgres psql -At -U rustshare -d rustshare -c 'SELECT id FROM backup_fixture')" == 471 ]]
[[ "$(docker compose --project-name "${PROJECT}" --file "${TEST_ROOT}/compose.yml" exec -T buzz-postgres psql -At -U buzz -d buzz -c 'SELECT id FROM backup_fixture')" == 472 ]]
[[ "$(docker run --rm --volume "${PROJECT}_rustfs-data:/data:ro" "${ARCHIVE_IMAGE}" cat /data/backup-fixture.txt)" == 'elembra fixture' ]]
[[ "$(docker run --rm --volume "${PROJECT}_buzz-rustfs-data:/data:ro" "${ARCHIVE_IMAGE}" cat /data/backup-fixture.txt)" == 'buzz fixture' ]]

sleep 1
if (cd "${test_project}" && ELEMBRA_TEST_AWS_FAIL=true ./scripts/elembra-offsite-backup.sh) >/dev/null 2>&1; then
	echo 'Expected failed S3 upload to fail the backup.' >&2
	exit 1
fi
grep -qx '/' "${TEST_ROOT}/heartbeat.log"
grep -qx '/fail' "${TEST_ROOT}/heartbeat.log"
echo 'Disposable S3 encryption, checksum, retention, failure alert, and full Elembra/Buzz restore passed; this is not a production recovery drill.'
