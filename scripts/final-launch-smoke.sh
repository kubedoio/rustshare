#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Read a single variable from .env without executing the file. Sourcing .env
# under set -euo pipefail would run arbitrary shell and break on unquoted
# values containing spaces. Consistent with env_get in scripts/pre-flight.sh.
env_file_get() {
	local key="$1"
	if [[ ! -f "${REPO_ROOT}/.env" ]]; then
		return 1
	fi
	local line
	line="$(grep "^${key}=" "${REPO_ROOT}/.env" 2>/dev/null | tail -n 1 || true)"
	if [[ -z "${line}" ]]; then
		return 1
	fi
	local value
	value="${line#*=}"
	value="$(printf '%s' "${value}" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
	if [[ "${value}" == \"*\" ]]; then
		value="${value#\"}"
		value="${value%\"}"
	elif [[ "${value}" == \'*\' ]]; then
		value="${value#\'}"
		value="${value%\'}"
	else
		value="$(printf '%s' "${value}" | sed 's/[[:space:]]#.*//' | sed 's/[[:space:]]*$//')"
	fi
	printf '%s' "${value}"
}

# Pull only the variables this script actually needs from .env. Explicit
# environment variables always take precedence.
RUSTSHARE_ADMIN_PASSWORD="${RUSTSHARE_ADMIN_PASSWORD:-$(env_file_get RUSTSHARE_ADMIN_PASSWORD || true)}"
RUSTSHARE_DEMO_VIEWER_PASSWORD="${RUSTSHARE_DEMO_VIEWER_PASSWORD:-$(env_file_get RUSTSHARE_DEMO_VIEWER_PASSWORD || true)}"
RUSTSHARE_BOOTSTRAP_PASSWORD_FILE="${RUSTSHARE_BOOTSTRAP_PASSWORD_FILE:-$(env_file_get RUSTSHARE_BOOTSTRAP_PASSWORD_FILE || true)}"

usage() {
	cat <<'EOF'
Usage: scripts/final-launch-smoke.sh

Executes a broader launch smoke test against the active Rustshare deployment.

DEV-ONLY: This script is intended for local development smoke testing. It
requires credentials that match the deployed backend. Do not hardcode
production credentials; pass them via environment variables or .env.

What it verifies:
- nginx health endpoint and proxied backend readiness endpoint
- admin password login and cookie session
- viewer password login
- root listing
- folder creation
- private file upload and streamed download
- internal share visibility for the recipient
- public file link download
- upload-only public folder upload
- public link revocation removes public access
- internal share revocation removes recipient access
- replication summary endpoint

Environment overrides:
- BASE_URL (default: http://localhost)
- API_BASE_URL (default: ${BASE_URL}/api/v1)
- ADMIN_EMAIL (default: admin@localhost)
- ADMIN_PASSWORD (falls back to RUSTSHARE_ADMIN_PASSWORD from .env, then backend bootstrap file)
- RUSTSHARE_BOOTSTRAP_PASSWORD_FILE (default: /tmp/rustshare-bootstrap-password.txt)
- VIEWER_EMAIL (default: viewer@localhost)
- VIEWER_PASSWORD (required; falls back to RUSTSHARE_DEMO_VIEWER_PASSWORD from .env)
- REPORT_DIR (default: ./launch-smoke-reports)
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
	usage
	exit 0
fi

require_command() {
	local command_name="$1"
	if ! command -v "${command_name}" >/dev/null 2>&1; then
		echo "Missing required command: ${command_name}" >&2
		exit 1
	fi
}

json_get() {
	local file_path="$1"
	local expression="$2"
	python3 - "$file_path" "$expression" <<'PY'
import json
import sys

file_path = sys.argv[1]
expression = sys.argv[2].split(".")

with open(file_path, "r", encoding="utf-8") as handle:
    value = json.load(handle)

for part in expression:
    if part == "":
        continue
    if isinstance(value, list):
        value = value[int(part)]
    else:
        value = value.get(part)

if value is None:
    sys.exit(1)

if isinstance(value, bool):
    print("true" if value else "false")
elif isinstance(value, (dict, list)):
    print(json.dumps(value))
else:
    print(value)
PY
}

run_json_request() {
	local method="$1"
	local url="$2"
	local body="${3:-}"
	local cookie_jar="${4:-}"
	local output_file="$5"
	local extra_header="${6:-}"
	local status

	local curl_args=(
		-sS
		-X "$method"
		-o "$output_file"
		-w "%{http_code}"
	)

	if [[ -n "$cookie_jar" ]]; then
		curl_args+=(-b "$cookie_jar" -c "$cookie_jar")
	fi

	if [[ -n "$extra_header" ]]; then
		curl_args+=(-H "$extra_header")
	fi

	if [[ -n "$body" ]]; then
		curl_args+=(-H "Content-Type: application/json" --data "$body")
	fi

	status="$(curl "${curl_args[@]}" "$url")"
	if [[ "$status" != 2* ]]; then
		echo "Request failed: ${method} ${url} -> ${status}" >&2
		if [[ -s "$output_file" ]]; then
			cat "$output_file" >&2
			echo >&2
		fi
		exit 1
	fi
}

login_with_password() {
	local email="$1"
	local password="$2"
	local cookie_jar="$3"
	local output_file="$4"
	local payload

	payload="$(python3 - "$email" "$password" <<'PY'
import json
import sys
print(json.dumps({"email": sys.argv[1], "password": sys.argv[2]}))
PY
)"

	run_json_request "POST" "${API_BASE_URL}/auth/login" "${payload}" "${cookie_jar}" "${output_file}"
}

try_login_with_password() {
	local email="$1"
	local password="$2"
	local cookie_jar="$3"
	local output_file="$4"
	local payload
	local status

	payload="$(python3 - "$email" "$password" <<'PY'
import json
import sys
print(json.dumps({"email": sys.argv[1], "password": sys.argv[2]}))
PY
)"

	status="$(
		curl -sS -X POST \
			-b "$cookie_jar" -c "$cookie_jar" \
			-o "$output_file" -w "%{http_code}" \
			-H "Content-Type: application/json" \
			--data "$payload" \
			"${API_BASE_URL}/auth/login"
	)"

	[[ "$status" == 2* ]]
}

read_bootstrap_admin_password() {
	local password_file="${RUSTSHARE_BOOTSTRAP_PASSWORD_FILE:-/tmp/rustshare-bootstrap-password.txt}"
	local password=""

	if ! command -v docker >/dev/null 2>&1; then
		return 1
	fi

	password="$(
		docker compose exec -T backend cat "${password_file}" 2>/dev/null || true
	)"
	password="$(printf '%s' "${password}" | tr -d '\r' | sed 's/[[:space:]]*$//')"

	if [[ -z "${password}" ]]; then
		return 1
	fi

	printf '%s' "${password}"
}

# Extract the double-submit CSRF token from a curl cookie jar. The backend
# requires X-Rustshare-Csrf to match the rustshare_csrf_token cookie, which is
# issued on login (and refreshed whenever a session lacks one).
csrf_token_from_jar() {
	local cookie_jar="$1"
	local token
	token="$(awk -F'\t' '$6 == "rustshare_csrf_token" {print $7}' "${cookie_jar}" | tail -n 1 | tr -d '\r')"
	if [[ -z "${token}" ]]; then
		echo "ERROR: rustshare_csrf_token cookie not found in ${cookie_jar}; did login succeed?" >&2
		exit 1
	fi
	printf '%s' "${token}"
}

csrf_json_request() {
	local method="$1"
	local url="$2"
	local body="${3:-}"
	local cookie_jar="$4"
	local output_file="$5"
	run_json_request "$method" "$url" "$body" "$cookie_jar" "$output_file" "X-Rustshare-Csrf: $(csrf_token_from_jar "${cookie_jar}")"
}

upload_file() {
	local cookie_jar="$1"
	local parent_folder_id="$2"
	local file_path="$3"
	local upload_name="$4"
	local output_file="$5"
	local status
	local csrf_token
	csrf_token="$(csrf_token_from_jar "${cookie_jar}")"

	status="$(
		curl -sS -o "$output_file" -w "%{http_code}" \
			-b "$cookie_jar" -c "$cookie_jar" \
			-H "X-Rustshare-Csrf: ${csrf_token}" \
			-F "file=@${file_path}" \
			-F "name=${upload_name}" \
			-F "parent_folder_id=${parent_folder_id}" \
			"${API_BASE_URL}/files/upload"
	)"

	if [[ "$status" != 2* ]]; then
		echo "Upload failed: ${status}" >&2
		cat "$output_file" >&2
		echo >&2
		exit 1
	fi
}

upload_public_file() {
	local token="$1"
	local session_token="$2"
	local parent_folder_id="$3"
	local file_path="$4"
	local upload_name="$5"
	local output_file="$6"
	local status

	status="$(
		curl -sS -o "$output_file" -w "%{http_code}" \
			-H "Authorization: Bearer ${session_token}" \
			-F "file=@${file_path}" \
			-F "name=${upload_name}" \
			-F "parent_folder_id=${parent_folder_id}" \
			-F "uploader_name=Phase 6 Smoke" \
			"${API_BASE_URL}/public/share/${token}/folder/upload"
	)"

	if [[ "$status" != 2* ]]; then
		echo "Public upload failed: ${status}" >&2
		cat "$output_file" >&2
		echo >&2
		exit 1
	fi
}

write_report() {
	local status="$1"
	local details="$2"
	mkdir -p "${REPORT_DIR}"
	cat >"${REPORT_PATH}" <<EOF
FINAL_LAUNCH_SMOKE_STATUS=${status}
FINAL_LAUNCH_SMOKE_STARTED_AT=${STARTED_AT}
FINAL_LAUNCH_SMOKE_FINISHED_AT=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
FINAL_LAUNCH_SMOKE_BASE_URL=${BASE_URL}
ADMIN_EMAIL=${ADMIN_EMAIL}
VIEWER_EMAIL=${VIEWER_EMAIL}
SMOKE_FOLDER_ID=${SMOKE_FOLDER_ID:-}
SMOKE_FILE_ID=${SMOKE_FILE_ID:-}
PUBLIC_FILE_SHARE_ID=${PUBLIC_FILE_SHARE_ID:-}
UPLOAD_ONLY_SHARE_ID=${UPLOAD_ONLY_SHARE_ID:-}
REPORT_DETAILS=${details}
EOF
}

require_command curl
require_command python3
require_command cmp

BASE_URL="${BASE_URL:-http://localhost}"
API_BASE_URL="${API_BASE_URL:-${BASE_URL%/}/api/v1}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@localhost}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-${RUSTSHARE_ADMIN_PASSWORD:-}}"
VIEWER_EMAIL="${VIEWER_EMAIL:-viewer@localhost}"
VIEWER_PASSWORD="${VIEWER_PASSWORD:-${RUSTSHARE_DEMO_VIEWER_PASSWORD:-}}"
REPORT_DIR="${REPORT_DIR:-$(pwd)/launch-smoke-reports}"

if [[ -z "${ADMIN_PASSWORD}" ]]; then
	if ADMIN_PASSWORD="$(read_bootstrap_admin_password)"; then
		echo "Using admin password from backend bootstrap file."
	else
		echo "ERROR: ADMIN_PASSWORD or RUSTSHARE_ADMIN_PASSWORD must be set, or the backend bootstrap password file must be readable." >&2
		echo "Run scripts/pre-flight.sh and restart the stack, or set ADMIN_PASSWORD explicitly." >&2
		exit 1
	fi
fi

if [[ -z "${VIEWER_PASSWORD}" ]]; then
	echo "ERROR: VIEWER_PASSWORD or RUSTSHARE_DEMO_VIEWER_PASSWORD must be set." >&2
	echo "Run scripts/pre-flight.sh to populate .env, or set VIEWER_PASSWORD explicitly." >&2
	exit 1
fi

STARTED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
REPORT_PATH="${REPORT_DIR%/}/$(date -u +%Y%m%dT%H%M%SZ)-final-launch-smoke.env"

TMP_DIR="$(mktemp -d)"
ADMIN_COOKIES="${TMP_DIR}/admin.cookies"
VIEWER_COOKIES="${TMP_DIR}/viewer.cookies"

cleanup() {
	rm -rf "${TMP_DIR}"
}
trap 'write_report "failed" "Final launch smoke failed. Inspect the command output and server logs."; cleanup' ERR
trap cleanup EXIT

LOGIN_ADMIN="${TMP_DIR}/login-admin.json"
LOGIN_VIEWER="${TMP_DIR}/login-viewer.json"
HEALTH_RESPONSE="${TMP_DIR}/health.txt"
HEALTH_READY_RESPONSE="${TMP_DIR}/health-ready.json"
ROOT_RESPONSE="${TMP_DIR}/root.json"
CREATE_FOLDER_RESPONSE="${TMP_DIR}/create-folder.json"
UPLOAD_RESPONSE="${TMP_DIR}/upload.json"
PRIVATE_DOWNLOAD_RESPONSE="${TMP_DIR}/private-download.bin"
SHARE_FILE_RESPONSE="${TMP_DIR}/share-file.json"
SHARE_SESSION_RESPONSE="${TMP_DIR}/share-session.json"
SHARED_FILE_RESPONSE="${TMP_DIR}/shared-file.bin"
INTERNAL_SHARE_RESPONSE="${TMP_DIR}/internal-share.json"
VIEWER_RECEIVED_RESPONSE="${TMP_DIR}/viewer-received.json"
REVOKE_PUBLIC_SHARE_RESPONSE="${TMP_DIR}/revoke-public-share.txt"
REVOKED_SESSION_RESPONSE="${TMP_DIR}/revoked-session.json"
REVOKE_INTERNAL_SHARE_RESPONSE="${TMP_DIR}/revoke-internal-share.txt"
VIEWER_RECEIVED_AFTER_REVOKE_RESPONSE="${TMP_DIR}/viewer-received-after-revoke.json"
UPLOAD_ONLY_SHARE_RESPONSE="${TMP_DIR}/upload-only-share.json"
UPLOAD_ONLY_SESSION_RESPONSE="${TMP_DIR}/upload-only-session.json"
PUBLIC_UPLOAD_RESPONSE="${TMP_DIR}/public-upload.json"
FOLDER_CONTENTS_RESPONSE="${TMP_DIR}/folder-contents.json"
REPLICATION_SUMMARY_RESPONSE="${TMP_DIR}/replication-summary.json"
DELETE_FOLDER_RESPONSE="${TMP_DIR}/delete-folder.txt"
LOGOUT_RESPONSE="${TMP_DIR}/logout.json"
POST_LOGOUT_ME_RESPONSE="${TMP_DIR}/post-logout-me.json"

PRIVATE_FILE_PATH="${TMP_DIR}/phase6-private.txt"
PUBLIC_UPLOAD_FILE_PATH="${TMP_DIR}/phase6-public-upload.txt"
printf 'phase-6-private-%s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" >"${PRIVATE_FILE_PATH}"
printf 'phase-6-public-upload-%s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" >"${PUBLIC_UPLOAD_FILE_PATH}"

echo "0. Verifying nginx health and proxied backend readiness..."
# /health is answered by nginx itself; /health/ready is proxied to the backend
# and reports 503 until the database and object storage are reachable.
curl -fsS "${BASE_URL}/health" >"${HEALTH_RESPONSE}"
READY_ATTEMPTS=0
until curl -fsS "${BASE_URL}/health/ready" >"${HEALTH_READY_RESPONSE}" 2>/dev/null; do
	READY_ATTEMPTS=$((READY_ATTEMPTS + 1))
	if [[ "${READY_ATTEMPTS}" -ge 30 ]]; then
		echo "Backend readiness check failed: ${BASE_URL}/health/ready did not return 2xx within 60s" >&2
		if [[ -s "${HEALTH_READY_RESPONSE}" ]]; then
			cat "${HEALTH_READY_RESPONSE}" >&2
			echo >&2
		fi
		exit 1
	fi
	sleep 2
done

echo "1. Logging in as admin..."
if ! try_login_with_password "${ADMIN_EMAIL}" "${ADMIN_PASSWORD}" "${ADMIN_COOKIES}" "${LOGIN_ADMIN}" >/dev/null 2>&1; then
	if BOOTSTRAP_ADMIN_PASSWORD="$(read_bootstrap_admin_password)" && [[ "${BOOTSTRAP_ADMIN_PASSWORD}" != "${ADMIN_PASSWORD}" ]]; then
		echo "Configured admin password failed; retrying with backend bootstrap password."
		ADMIN_PASSWORD="${BOOTSTRAP_ADMIN_PASSWORD}"
		login_with_password "${ADMIN_EMAIL}" "${ADMIN_PASSWORD}" "${ADMIN_COOKIES}" "${LOGIN_ADMIN}"
	else
		cat "${LOGIN_ADMIN}" >&2
		echo >&2
		echo "Admin login failed. Set ADMIN_PASSWORD to the active admin password or recreate the stack after scripts/pre-flight.sh." >&2
		exit 1
	fi
fi
if [[ "$(json_get "${LOGIN_ADMIN}" "user.email")" != "${ADMIN_EMAIL}" ]]; then
	echo "Admin login returned the wrong user" >&2
	exit 1
fi

echo "2. Logging in as viewer..."
if ! try_login_with_password "${VIEWER_EMAIL}" "${VIEWER_PASSWORD}" "${VIEWER_COOKIES}" "${LOGIN_VIEWER}" >/dev/null 2>&1; then
	# Fallback: some local-dev seeds reuse the admin password for the demo
	# viewer account. Try that before giving up.
	login_with_password "${VIEWER_EMAIL}" "${ADMIN_PASSWORD}" "${VIEWER_COOKIES}" "${LOGIN_VIEWER}"
fi
if [[ "$(json_get "${LOGIN_VIEWER}" "user.email")" != "${VIEWER_EMAIL}" ]]; then
	echo "Viewer login returned the wrong user" >&2
	exit 1
fi

echo "3. Verifying root listing..."
run_json_request "GET" "${API_BASE_URL}/folders/root/contents" "" "${ADMIN_COOKIES}" "${ROOT_RESPONSE}"
json_get "${ROOT_RESPONSE}" "files" >/dev/null
json_get "${ROOT_RESPONSE}" "folders" >/dev/null

echo "4. Creating a smoke folder..."
FOLDER_PAYLOAD="$(python3 - <<'PY'
import json
print(json.dumps({"name": "Phase 6 Smoke", "parent_folder_id": None}))
PY
)"
csrf_json_request "POST" "${API_BASE_URL}/folders" "${FOLDER_PAYLOAD}" "${ADMIN_COOKIES}" "${CREATE_FOLDER_RESPONSE}"
SMOKE_FOLDER_ID="$(json_get "${CREATE_FOLDER_RESPONSE}" "id")"

echo "5. Uploading a private file..."
upload_file "${ADMIN_COOKIES}" "${SMOKE_FOLDER_ID}" "${PRIVATE_FILE_PATH}" "phase6-private.txt" "${UPLOAD_RESPONSE}"
SMOKE_FILE_ID="$(json_get "${UPLOAD_RESPONSE}" "id")"

echo "6. Validating private streamed download..."
DOWNLOAD_STATUS="$(
	curl -sS -X GET \
		-b "${ADMIN_COOKIES}" -c "${ADMIN_COOKIES}" \
		-o "${PRIVATE_DOWNLOAD_RESPONSE}" -w "%{http_code}" \
		"${API_BASE_URL}/files/${SMOKE_FILE_ID}/download"
)"
if [[ "${DOWNLOAD_STATUS}" != 2* ]]; then
	echo "Private download failed with status ${DOWNLOAD_STATUS}" >&2
	if [[ -s "${PRIVATE_DOWNLOAD_RESPONSE}" ]]; then
		cat "${PRIVATE_DOWNLOAD_RESPONSE}" >&2
		echo >&2
	fi
	exit 1
fi
if ! cmp -s "${PRIVATE_FILE_PATH}" "${PRIVATE_DOWNLOAD_RESPONSE}"; then
	echo "Private download content did not match uploaded file" >&2
	exit 1
fi

echo "7. Creating an internal share and validating recipient visibility..."
INTERNAL_SHARE_PAYLOAD="$(python3 - "$VIEWER_EMAIL" <<'PY'
import json
import sys
print(json.dumps({"recipient_email": sys.argv[1], "permission": "View"}))
PY
)"
csrf_json_request "POST" "${API_BASE_URL}/files/${SMOKE_FILE_ID}/share" "${INTERNAL_SHARE_PAYLOAD}" "${ADMIN_COOKIES}" "${INTERNAL_SHARE_RESPONSE}"
INTERNAL_SHARE_ID="$(json_get "${INTERNAL_SHARE_RESPONSE}" "share_id")"
run_json_request "GET" "${API_BASE_URL}/shares/received" "" "${VIEWER_COOKIES}" "${VIEWER_RECEIVED_RESPONSE}"
python3 - "${VIEWER_RECEIVED_RESPONSE}" "${SMOKE_FILE_ID}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    shares = json.load(handle)

target = sys.argv[2]
if not any(item.get("resource_id") == target for item in shares):
    raise SystemExit(1)
PY

echo "8. Creating a public file link and downloading through the public flow..."
PUBLIC_FILE_SHARE_PAYLOAD="$(python3 - <<'PY'
import json
print(json.dumps({"permissions": "View"}))
PY
)"
csrf_json_request "POST" "${API_BASE_URL}/files/${SMOKE_FILE_ID}/shares" "${PUBLIC_FILE_SHARE_PAYLOAD}" "${ADMIN_COOKIES}" "${SHARE_FILE_RESPONSE}"
PUBLIC_FILE_SHARE_ID="$(json_get "${SHARE_FILE_RESPONSE}" "id")"
PUBLIC_FILE_SHARE_TOKEN="$(json_get "${SHARE_FILE_RESPONSE}" "share_token")"
run_json_request "POST" "${API_BASE_URL}/public/share/${PUBLIC_FILE_SHARE_TOKEN}/session" "{}" "" "${SHARE_SESSION_RESPONSE}"
PUBLIC_FILE_SESSION_TOKEN="$(json_get "${SHARE_SESSION_RESPONSE}" "session_token")"
	curl -fsS -H "Authorization: Bearer ${PUBLIC_FILE_SESSION_TOKEN}" \
		"${API_BASE_URL}/public/share/${PUBLIC_FILE_SHARE_TOKEN}/file" >"${SHARED_FILE_RESPONSE}"
	test -s "${SHARED_FILE_RESPONSE}"

echo "9. Creating an upload-only public folder link and uploading through it..."
UPLOAD_ONLY_SHARE_PAYLOAD="$(python3 - <<'PY'
import json
print(json.dumps({"permissions": "Edit", "upload_only": True}))
PY
)"
csrf_json_request "POST" "${API_BASE_URL}/folders/${SMOKE_FOLDER_ID}/shares" "${UPLOAD_ONLY_SHARE_PAYLOAD}" "${ADMIN_COOKIES}" "${UPLOAD_ONLY_SHARE_RESPONSE}"
UPLOAD_ONLY_SHARE_ID="$(json_get "${UPLOAD_ONLY_SHARE_RESPONSE}" "id")"
UPLOAD_ONLY_SHARE_TOKEN="$(json_get "${UPLOAD_ONLY_SHARE_RESPONSE}" "share_token")"
run_json_request "POST" "${API_BASE_URL}/public/share/${UPLOAD_ONLY_SHARE_TOKEN}/session" "{}" "" "${UPLOAD_ONLY_SESSION_RESPONSE}"
UPLOAD_ONLY_SESSION_TOKEN="$(json_get "${UPLOAD_ONLY_SESSION_RESPONSE}" "session_token")"
upload_public_file \
	"${UPLOAD_ONLY_SHARE_TOKEN}" \
	"${UPLOAD_ONLY_SESSION_TOKEN}" \
	"${SMOKE_FOLDER_ID}" \
	"${PUBLIC_UPLOAD_FILE_PATH}" \
	"phase6-public-upload.txt" \
	"${PUBLIC_UPLOAD_RESPONSE}"
run_json_request "GET" "${API_BASE_URL}/folders/${SMOKE_FOLDER_ID}/contents" "" "${ADMIN_COOKIES}" "${FOLDER_CONTENTS_RESPONSE}"
python3 - "${FOLDER_CONTENTS_RESPONSE}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

if not any(item.get("name") == "phase6-public-upload.txt" for item in payload.get("files", [])):
    raise SystemExit(1)
PY

echo "10. Revoking the public file link and verifying access is gone..."
csrf_json_request "DELETE" "${API_BASE_URL}/shares/${PUBLIC_FILE_SHARE_ID}" "" "${ADMIN_COOKIES}" "${REVOKE_PUBLIC_SHARE_RESPONSE}"
REVOKED_SESSION_STATUS="$(
	curl -sS -o "${REVOKED_SESSION_RESPONSE}" -w "%{http_code}" \
		-X POST -H "Content-Type: application/json" --data "{}" \
		"${API_BASE_URL}/public/share/${PUBLIC_FILE_SHARE_TOKEN}/session"
)"
if [[ "${REVOKED_SESSION_STATUS}" != "404" && "${REVOKED_SESSION_STATUS}" != "410" ]]; then
	echo "Expected 404 or 410 for a revoked public share, got ${REVOKED_SESSION_STATUS}" >&2
	if [[ -s "${REVOKED_SESSION_RESPONSE}" ]]; then
		cat "${REVOKED_SESSION_RESPONSE}" >&2
		echo >&2
	fi
	exit 1
fi

echo "11. Revoking the internal share and verifying recipient access is gone..."
csrf_json_request "DELETE" "${API_BASE_URL}/shares/${INTERNAL_SHARE_ID}/recipient" "" "${ADMIN_COOKIES}" "${REVOKE_INTERNAL_SHARE_RESPONSE}"
run_json_request "GET" "${API_BASE_URL}/shares/received" "" "${VIEWER_COOKIES}" "${VIEWER_RECEIVED_AFTER_REVOKE_RESPONSE}"
python3 - "${VIEWER_RECEIVED_AFTER_REVOKE_RESPONSE}" "${SMOKE_FILE_ID}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    shares = json.load(handle)

target = sys.argv[2]
if any(item.get("resource_id") == target for item in shares):
    raise SystemExit(1)
PY

echo "12. Checking replication visibility..."
run_json_request "GET" "${API_BASE_URL}/admin/replication/summary" "" "${ADMIN_COOKIES}" "${REPLICATION_SUMMARY_RESPONSE}"
json_get "${REPLICATION_SUMMARY_RESPONSE}" "job_states" >/dev/null
json_get "${REPLICATION_SUMMARY_RESPONSE}" "version_states" >/dev/null
json_get "${REPLICATION_SUMMARY_RESPONSE}" "target_states" >/dev/null

echo "13. Cleaning up the smoke folder..."
csrf_json_request "DELETE" "${API_BASE_URL}/folders/${SMOKE_FOLDER_ID}" "" "${ADMIN_COOKIES}" "${DELETE_FOLDER_RESPONSE}"

echo "14. Verifying logout..."
csrf_json_request "POST" "${API_BASE_URL}/auth/logout" "{}" "${ADMIN_COOKIES}" "${LOGOUT_RESPONSE}"
POST_LOGOUT_STATUS="$(
	curl -sS -o "${POST_LOGOUT_ME_RESPONSE}" -w "%{http_code}" \
		-b "${ADMIN_COOKIES}" -c "${ADMIN_COOKIES}" \
		"${API_BASE_URL}/me"
)"
if [[ "${POST_LOGOUT_STATUS}" != "401" ]]; then
	echo "Expected /me to return 401 after logout, got ${POST_LOGOUT_STATUS}" >&2
	cat "${POST_LOGOUT_ME_RESPONSE}" >&2
	exit 1
fi

write_report "passed" "Final launch smoke completed successfully."
trap - ERR

echo "Final launch smoke passed."
echo "Report written to ${REPORT_PATH}"
