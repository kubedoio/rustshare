#!/usr/bin/env bash

set -Eeuo pipefail
umask 077

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
WORK_DIR=""
HEARTBEAT_URL="${ELEMBRA_BACKUP_HEARTBEAT_URL:-}"

ping_monitor() {
	local url="$1"
	[[ "${url}" != *$'\n'* && "${url}" != *$'\r'* && "${url}" != *'"'* && "${url}" != *'\'* ]] || return 2
	printf 'url = "%s"\n' "${url}" | curl --disable --config - --proto =https,http --fail --silent --max-time 10 >/dev/null 2>&1
}

on_error() {
	local status="$1" line="$2"
	trap - ERR
	if [[ -n "${HEARTBEAT_URL}" ]]; then
		ping_monitor "${HEARTBEAT_URL%/}/fail" || true
	fi
	printf 'Elembra off-host backup failed at line %s (exit %s).\n' "${line}" "${status}" >&2
	exit "${status}"
}

cleanup() {
	if [[ -n "${WORK_DIR}" ]]; then
		rm -rf -- "${WORK_DIR}"
	fi
}
trap 'on_error $? $LINENO' ERR
trap cleanup EXIT

required=(ELEMBRA_BACKUP_S3_ENDPOINT ELEMBRA_BACKUP_S3_BUCKET ELEMBRA_BACKUP_AGE_RECIPIENT AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY ELEMBRA_BACKUP_HEARTBEAT_URL)
for name in "${required[@]}"; do
	if [[ -z "${!name:-}" ]]; then
		printf 'Required backup setting is missing: %s\n' "${name}" >&2
		exit 2
	fi
done

for command in age aws curl sha256sum stat docker tar gzip awk find; do
	command -v "${command}" >/dev/null || { printf 'Required command not found: %s\n' "${command}" >&2; exit 2; }
done

endpoint="${ELEMBRA_BACKUP_S3_ENDPOINT}"
case "${endpoint}" in
	https://*) ;;
	http://localhost:*|http://127.0.0.1:*)
		[[ "${ELEMBRA_BACKUP_TEST_ALLOW_HTTP_LOOPBACK:-}" == true ]] || { echo 'HTTP is allowed only for loopback integration tests.' >&2; exit 2; }
		;;
	*) echo 'Backup endpoint must use HTTPS.' >&2; exit 2 ;;
esac
[[ "${ELEMBRA_BACKUP_S3_BUCKET}" =~ ^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$ ]] || { echo 'Invalid S3 bucket name.' >&2; exit 2; }
case "${HEARTBEAT_URL}" in
	https://*) ;;
	http://localhost:*|http://127.0.0.1:*)
		[[ "${ELEMBRA_BACKUP_TEST_ALLOW_HTTP_LOOPBACK:-}" == true ]] || { echo 'HTTP heartbeat is allowed only for loopback integration tests.' >&2; exit 2; }
		;;
	*) echo 'Backup heartbeat URL must use HTTPS.' >&2; exit 2 ;;
esac

for file in .env config/buzz-compatibility.env .elembra/chat.env; do
	[[ -f "${PROJECT_ROOT}/${file}" ]] || { printf 'Required deployment file is missing: %s\n' "${file}" >&2; exit 2; }
done

region="${AWS_DEFAULT_REGION:-us-east-1}"
export AWS_DEFAULT_REGION="${region}" AWS_EC2_METADATA_DISABLED=true AWS_PAGER=""
staging="${ELEMBRA_BACKUP_STAGING_DIR:-/var/tmp}"
mkdir -p -- "${staging}"
WORK_DIR="$(mktemp -d "${staging%/}/elembra-offsite.XXXXXXXX")"
chmod 700 "${WORK_DIR}"

bundle_root="${WORK_DIR}/bundle"
mkdir -m 700 "${bundle_root}"
"${PROJECT_ROOT}/scripts/backup-stack.sh" --with-chat "${bundle_root}"
bundle_dir="$(find "${bundle_root}" -mindepth 1 -maxdepth 1 -type d -print -quit)"
[[ -n "${bundle_dir}" ]]
"${PROJECT_ROOT}/scripts/verify-backup-bundle.sh" "${bundle_dir}"

timestamp="$(basename "${bundle_dir}")"
[[ "${timestamp}" =~ ^[0-9]{8}T[0-9]{6}Z$ ]]
encrypted="${WORK_DIR}/${timestamp}.tar.gz.age"
secret_files=(.env config/buzz-compatibility.env .elembra/chat.env)
if [[ -f "${PROJECT_ROOT}/.elembra/rustfs-kms.override.yml" ]]; then
	secret_files+=(.elembra/rustfs-kms.override.yml)
fi
tar -czf - \
	-C "${bundle_root}" "${timestamp}" \
	-C "${PROJECT_ROOT}" "${secret_files[@]}" \
	| age --recipient "${ELEMBRA_BACKUP_AGE_RECIPIENT}" >"${encrypted}"

checksum="$(sha256sum "${encrypted}" | awk '{print $1}')"
size="$(stat -c '%s' "${encrypted}")"
key="elembra/${timestamp}.tar.gz.age"
aws --endpoint-url "${endpoint}" --region "${region}" s3 cp \
	"${encrypted}" "s3://${ELEMBRA_BACKUP_S3_BUCKET}/${key}" \
	--checksum-algorithm SHA256 --metadata "sha256=${checksum}" --only-show-errors

printf '%s  %s\n' "${checksum}" "${timestamp}.tar.gz.age" >"${WORK_DIR}/${timestamp}.sha256"
aws --endpoint-url "${endpoint}" --region "${region}" s3 cp \
	"${WORK_DIR}/${timestamp}.sha256" "s3://${ELEMBRA_BACKUP_S3_BUCKET}/${key}.sha256" \
	--checksum-algorithm SHA256 --only-show-errors

printf 'Encrypted Elembra/Buzz backup uploaded: s3://%s/%s (%s bytes, sha256 %s)\n' \
	"${ELEMBRA_BACKUP_S3_BUCKET}" "${key}" "${size}" "${checksum}"
ping_monitor "${HEARTBEAT_URL%/}"
