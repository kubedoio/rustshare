#!/usr/bin/env bash
set -euo pipefail

# Beta product smoke: verifies the full beta web surface against a running
# Elembra deployment. Intended to run after every deploy to the hosted beta
# environment and to serve as the `files_notes_chat_memory_search_ask` and
# deploy evidence for docs/releases/beta-gate.yaml.
#
# Extends scripts/final-launch-smoke.sh (which covers login, folders, upload/
# download, internal + public sharing with revocation, replication, logout)
# with the beta-specific surface: notes CRUD, permission-aware search, chat
# status, and the admin audit log.
#
# Usage:
#   scripts/run-beta-smoke.sh
#
# Environment overrides:
# - BASE_URL (default: http://localhost)
# - API_BASE_URL (default: ${BASE_URL}/api/v1)
# - ADMIN_EMAIL (default: admin@localhost)
# - ADMIN_PASSWORD (falls back to RUSTSHARE_ADMIN_PASSWORD from .env)
# - VIEWER_EMAIL (default: viewer@localhost)
# - VIEWER_PASSWORD (falls back to RUSTSHARE_DEMO_VIEWER_PASSWORD from .env)
# - REQUIRE_CHAT (default: unset — chat status must respond but may be
#   unconfigured; set to 1 to fail when chat is not fully configured)
# - REPORT_DIR (default: ./beta-smoke-reports)

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Read a single variable from .env without executing the file. Consistent
# with env_get in scripts/pre-flight.sh.
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
		value="$(printf '%s' "${value}" | sed 's/^[[:space:]]*#.*//' | sed 's/[[:space:]]*$//')"
	fi
	printf '%s' "${value}"
}

RUSTSHARE_ADMIN_PASSWORD="${RUSTSHARE_ADMIN_PASSWORD:-$(env_file_get RUSTSHARE_ADMIN_PASSWORD || true)}"
RUSTSHARE_DEMO_VIEWER_PASSWORD="${RUSTSHARE_DEMO_VIEWER_PASSWORD:-$(env_file_get RUSTSHARE_DEMO_VIEWER_PASSWORD || true)}"
RUSTSHARE_BOOTSTRAP_PASSWORD_FILE="${RUSTSHARE_BOOTSTRAP_PASSWORD_FILE:-$(env_file_get RUSTSHARE_BOOTSTRAP_PASSWORD_FILE || true)}"

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
	local expect_2xx="${7:-1}"
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

	# `|| true` so a transient connection failure under `set -e` reaches the
	# friendly error path (or the search retry loop) instead of aborting.
	status="$(curl "${curl_args[@]}" "$url" || true)"
	if [[ -z "$status" ]]; then
		if [[ "$expect_2xx" == "1" ]]; then
			echo "Request failed (no response): ${method} ${url}" >&2
			exit 1
		fi
		return 0
	fi
	if [[ "$expect_2xx" == "1" && "$status" != 2* ]]; then
		echo "Request failed: ${method} ${url} -> ${status}" >&2
		if [[ -s "$output_file" ]]; then
			cat "$output_file" >&2
			echo >&2
		fi
		exit 1
	fi
	printf '%s' "$status"
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
	run_json_request "$method" "$url" "$body" "${cookie_jar}" "${output_file}" "X-Rustshare-Csrf: $(csrf_token_from_jar "${cookie_jar}")"
}

# Retrieve the one-time bootstrap admin password from the backend container,
# mirroring final-launch-smoke.sh. Only works when docker and the running
# compose project are reachable from this machine.
read_bootstrap_admin_password() {
	local password_file="${RUSTSHARE_BOOTSTRAP_PASSWORD_FILE:-/tmp/rustshare-bootstrap-password.txt}"
	local password=""

	if ! command -v docker >/dev/null 2>&1; then
		return 1
	fi

	local container_id
	container_id="$(docker compose ps -q backend 2>/dev/null || true)"
	[[ -n "${container_id}" ]] || return 1
	password="$("${REPO_ROOT}/scripts/read-bootstrap-password.sh" "${container_id}" "${password_file}" 2>/dev/null || true)"

	if [[ -z "${password}" ]]; then
		return 1
	fi

	printf '%s' "${password}"
}

write_report() {
	local status="$1"
	local details="$2"
	mkdir -p "${REPORT_DIR}"
	cat >"${REPORT_PATH}" <<EOF
BETA_SMOKE_STATUS=${status}
BETA_SMOKE_STARTED_AT=${STARTED_AT}
BETA_SMOKE_FINISHED_AT=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
BETA_SMOKE_BASE_URL=${BASE_URL}
ADMIN_EMAIL=${ADMIN_EMAIL}
VIEWER_EMAIL=${VIEWER_EMAIL}
SMOKE_FOLDER_ID=${SMOKE_FOLDER_ID:-}
SMOKE_FILE_ID=${SMOKE_FILE_ID:-}
SMOKE_NOTE_ID=${SMOKE_NOTE_ID:-}
SEARCH_NEEDLE=${SEARCH_NEEDLE:-}
CHAT_STATUS_HTTP=${CHAT_STATUS_HTTP:-}
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
REPORT_DIR="${REPORT_DIR:-$(pwd)/beta-smoke-reports}"

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
	exit 1
fi

STARTED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
REPORT_PATH="${REPORT_DIR%/}/$(date -u +%Y%m%dT%H%M%SZ)-$$-beta-smoke.env"

TMP_DIR="$(mktemp -d)"
ADMIN_COOKIES="${TMP_DIR}/admin.cookies"
VIEWER_COOKIES="${TMP_DIR}/viewer.cookies"
cleanup() {
	rm -rf "${TMP_DIR}"
}
# Failure reporting keys off the exit code, not the ERR trap: bash does not
# fire ERR for explicit `exit 1` branches, which this script uses for every
# assertion failure. On success the passed report is written by the main
# flow before exit 0.
on_exit() {
	local code=$?
	if [[ "${code}" -ne 0 ]]; then
		write_report "failed" "Beta smoke failed with exit code ${code}. Inspect the command output and server logs."
	fi
	cleanup
}
trap on_exit EXIT

LOGIN_ADMIN="${TMP_DIR}/login-admin.json"
LOGIN_VIEWER="${TMP_DIR}/login-viewer.json"
HEALTH_READY_RESPONSE="${TMP_DIR}/health-ready.json"
ROOT_RESPONSE="${TMP_DIR}/root.json"
CREATE_FOLDER_RESPONSE="${TMP_DIR}/create-folder.json"
UPLOAD_RESPONSE="${TMP_DIR}/upload.json"
DOWNLOAD_RESPONSE="${TMP_DIR}/download.bin"
CREATE_NOTE_RESPONSE="${TMP_DIR}/create-note.json"
GET_NOTE_RESPONSE="${TMP_DIR}/get-note.json"
SAVE_NOTE_RESPONSE="${TMP_DIR}/save-note.json"
LIST_NOTES_RESPONSE="${TMP_DIR}/list-notes.json"
SEARCH_RESPONSE="${TMP_DIR}/search.json"
VIEWER_SEARCH_RESPONSE="${TMP_DIR}/viewer-search.json"
CHAT_STATUS_RESPONSE="${TMP_DIR}/chat-status.json"
AUDIT_RESPONSE="${TMP_DIR}/audit.json"
INTERNAL_SHARE_RESPONSE="${TMP_DIR}/internal-share.json"
VIEWER_RECEIVED_RESPONSE="${TMP_DIR}/viewer-received.json"
REVOKE_INTERNAL_SHARE_RESPONSE="${TMP_DIR}/revoke-internal-share.txt"
DELETE_FOLDER_RESPONSE="${TMP_DIR}/delete-folder.txt"
DELETE_NOTE_RESPONSE="${TMP_DIR}/delete-note.txt"
LOGOUT_RESPONSE="${TMP_DIR}/logout.json"
POST_LOGOUT_ME_RESPONSE="${TMP_DIR}/post-logout-me.json"

PRIVATE_FILE_PATH="${TMP_DIR}/beta-private.txt"
printf 'beta-private-%s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" >"${PRIVATE_FILE_PATH}"
SEARCH_NEEDLE="betasearch-$(date -u +%s)"

echo "0. Verifying backend readiness..."
READY_ATTEMPTS=0
until curl -fsS "${BASE_URL}/health/ready" >"${HEALTH_READY_RESPONSE}" 2>/dev/null; do
	READY_ATTEMPTS=$((READY_ATTEMPTS + 1))
	if [[ "${READY_ATTEMPTS}" -ge 30 ]]; then
		echo "Backend readiness check failed: ${BASE_URL}/health/ready did not return 2xx within 60s" >&2
		exit 1
	fi
	sleep 2
done

echo "1. Logging in as admin and viewer..."
login_with_password "${ADMIN_EMAIL}" "${ADMIN_PASSWORD}" "${ADMIN_COOKIES}" "${LOGIN_ADMIN}"
[[ "$(json_get "${LOGIN_ADMIN}" "user.email")" == "${ADMIN_EMAIL}" ]] || {
	echo "Admin login returned the wrong user" >&2
	exit 1
}
login_with_password "${VIEWER_EMAIL}" "${VIEWER_PASSWORD}" "${VIEWER_COOKIES}" "${LOGIN_VIEWER}"
[[ "$(json_get "${LOGIN_VIEWER}" "user.email")" == "${VIEWER_EMAIL}" ]] || {
	echo "Viewer login returned the wrong user" >&2
	exit 1
}

echo "2. Verifying root listing..."
run_json_request "GET" "${API_BASE_URL}/folders/root/contents" "" "${ADMIN_COOKIES}" "${ROOT_RESPONSE}"
json_get "${ROOT_RESPONSE}" "files" >/dev/null
json_get "${ROOT_RESPONSE}" "folders" >/dev/null

echo "3. Creating a smoke folder and uploading a private file..."
FOLDER_PAYLOAD="$(python3 - <<'PY'
import json
print(json.dumps({"name": "Beta Smoke", "parent_folder_id": None}))
PY
)"
csrf_json_request "POST" "${API_BASE_URL}/folders" "${FOLDER_PAYLOAD}" "${ADMIN_COOKIES}" "${CREATE_FOLDER_RESPONSE}"
SMOKE_FOLDER_ID="$(json_get "${CREATE_FOLDER_RESPONSE}" "id")"

CSRF_TOKEN="$(csrf_token_from_jar "${ADMIN_COOKIES}")"
UPLOAD_STATUS="$(
	curl -sS -o "${UPLOAD_RESPONSE}" -w "%{http_code}" \
		-b "${ADMIN_COOKIES}" -c "${ADMIN_COOKIES}" \
		-H "X-Rustshare-Csrf: ${CSRF_TOKEN}" \
		-F "file=@${PRIVATE_FILE_PATH}" \
		-F "name=beta-private.txt" \
		-F "parent_folder_id=${SMOKE_FOLDER_ID}" \
		"${API_BASE_URL}/files/upload"
)"
[[ "${UPLOAD_STATUS}" == 2* ]] || {
	echo "Upload failed: ${UPLOAD_STATUS}" >&2
	cat "${UPLOAD_RESPONSE}" >&2 || true
	exit 1
}
SMOKE_FILE_ID="$(json_get "${UPLOAD_RESPONSE}" "id")"

echo "4. Validating private streamed download..."
DOWNLOAD_STATUS="$(
	curl -sS -X GET -o "${DOWNLOAD_RESPONSE}" -w "%{http_code}" \
		-b "${ADMIN_COOKIES}" -c "${ADMIN_COOKIES}" \
		"${API_BASE_URL}/files/${SMOKE_FILE_ID}/download"
)"
[[ "${DOWNLOAD_STATUS}" == 2* ]] || {
	echo "Private download failed with status ${DOWNLOAD_STATUS}" >&2
	exit 1
}
cmp -s "${PRIVATE_FILE_PATH}" "${DOWNLOAD_RESPONSE}" || {
	echo "Private download content did not match uploaded file" >&2
	exit 1
}

echo "5. Creating a note, reading and updating it..."
NOTE_PAYLOAD="$(python3 - <<PY
import json
print(json.dumps({"title": "Beta Smoke Note", "content": "needle ${SEARCH_NEEDLE}", "parent_folder_id": "${SMOKE_FOLDER_ID}"}))
PY
)"
csrf_json_request "POST" "${API_BASE_URL}/notes" "${NOTE_PAYLOAD}" "${ADMIN_COOKIES}" "${CREATE_NOTE_RESPONSE}"
SMOKE_NOTE_ID="$(json_get "${CREATE_NOTE_RESPONSE}" "id")"
run_json_request "GET" "${API_BASE_URL}/notes/${SMOKE_NOTE_ID}" "" "${ADMIN_COOKIES}" "${GET_NOTE_RESPONSE}"
SAVE_NOTE_PAYLOAD="$(python3 - "${SEARCH_NEEDLE}" <<'PY'
import json
import sys
# SaveNoteRequest has no title field: only content/color/attachments.
print(json.dumps({"content": "updated needle " + sys.argv[1]}))
PY
)"
csrf_json_request "PUT" "${API_BASE_URL}/notes/${SMOKE_NOTE_ID}" "${SAVE_NOTE_PAYLOAD}" "${ADMIN_COOKIES}" "${SAVE_NOTE_RESPONSE}"
run_json_request "GET" "${API_BASE_URL}/notes" "" "${ADMIN_COOKIES}" "${LIST_NOTES_RESPONSE}"

echo "6. Verifying permission-aware search finds the note..."
# Search runs with a bounded retry: the notes/chat projection is fed through
# the durable outbox, so a freshly created note may take a few seconds to
# become searchable.
SEARCH_PAYLOAD="$(python3 - "${SEARCH_NEEDLE}" <<'PY'
import json
import sys
print(json.dumps({"query": sys.argv[1], "limit": 10}))
PY
)"
SEARCH_OK=0
for _ in $(seq 1 10); do
	# Non-2xx is tolerated inside the retry window; the projection is fed
	# through the durable outbox and may briefly lag.
	SEARCH_HTTP="$(run_json_request "POST" "${API_BASE_URL}/search" "${SEARCH_PAYLOAD}" "${ADMIN_COOKIES}" "${SEARCH_RESPONSE}" "" "0")"
	if [[ "${SEARCH_HTTP}" != 2* ]]; then
		sleep 3
		continue
	fi
	if python3 - "${SEARCH_RESPONSE}" <<'PY'; then
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

results = payload.get("results") or payload.get("items") or []
if not isinstance(results, list):
    raise SystemExit(1)
needle = "needle betasearch-"
if not any(needle in json.dumps(item) for item in results):
    raise SystemExit(1)
PY
		SEARCH_OK=1
		break
	fi
	sleep 3
done
[[ "${SEARCH_OK}" == "1" ]] || {
	echo "Search did not surface the smoke note (query: ${SEARCH_NEEDLE}) within 30s" >&2
	exit 1
}

echo "6b. Verifying search is permission-aware (viewer must NOT find the note)..."
# The viewer holds no share on the smoke note; permission-aware search must
# not surface it. This is the negative half of the search evidence.
VIEWER_SEARCH_HTTP="$(run_json_request "POST" "${API_BASE_URL}/search" "${SEARCH_PAYLOAD}" "${VIEWER_COOKIES}" "${VIEWER_SEARCH_RESPONSE}" "" "0")"
if [[ "${VIEWER_SEARCH_HTTP}" != 2* ]]; then
	echo "Viewer search request failed with ${VIEWER_SEARCH_HTTP}" >&2
	exit 1
fi
if ! python3 - "${VIEWER_SEARCH_RESPONSE}" <<'PY'; then
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

results = payload.get("results") or payload.get("items") or []
if not isinstance(results, list):
    raise SystemExit(1)
needle = "betasearch-"
leaked = [item for item in results if needle in json.dumps(item)]
if leaked:
    raise SystemExit(1)
PY
	echo "PERMISSION FAILURE: viewer search surfaced the admin-only smoke note; treat as Sev-1" >&2
	exit 1
fi

echo "7. Checking chat application status..."
CHAT_STATUS_HTTP="$(run_json_request "GET" "${API_BASE_URL}/applications/chat/status" "" "${ADMIN_COOKIES}" "${CHAT_STATUS_RESPONSE}" "" "0")"
if [[ "${CHAT_STATUS_HTTP}" != 2* ]]; then
	echo "Chat status endpoint returned ${CHAT_STATUS_HTTP}" >&2
	cat "${CHAT_STATUS_RESPONSE}" >&2 || true
	exit 1
fi
if [[ "${REQUIRE_CHAT:-0}" == "1" ]]; then
	if ! python3 - "${CHAT_STATUS_RESPONSE}" <<'PY'; then
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

# ChatStatusResponse: chat_enabled plus an active workspace/community
# mapping (binding may still be absent for a caller without a personal
# identity — that state renders the BindingPanel, which is fine).
if not payload.get("chat_enabled"):
    raise SystemExit(1)
if payload.get("mapping") is None:
    raise SystemExit(1)
PY
		echo "REQUIRE_CHAT=1 but chat is not enabled or has no active workspace/community mapping; see ${CHAT_STATUS_RESPONSE}" >&2
		exit 1
	fi
fi

echo "8. Creating and revoking an internal share..."
INTERNAL_SHARE_PAYLOAD="$(python3 - "$VIEWER_EMAIL" <<'PY'
import json
import sys
print(json.dumps({"recipient_email": sys.argv[1], "permission": "View"}))
PY
)"
csrf_json_request "POST" "${API_BASE_URL}/files/${SMOKE_FILE_ID}/share" "${INTERNAL_SHARE_PAYLOAD}" "${ADMIN_COOKIES}" "${INTERNAL_SHARE_RESPONSE}"
INTERNAL_SHARE_ID="$(json_get "${INTERNAL_SHARE_RESPONSE}" "share_id")"
run_json_request "GET" "${API_BASE_URL}/shares/received" "" "${VIEWER_COOKIES}" "${VIEWER_RECEIVED_RESPONSE}"
if ! python3 - "${VIEWER_RECEIVED_RESPONSE}" "${SMOKE_FILE_ID}" <<'PY'; then
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    shares = json.load(handle)

target = sys.argv[2]
if not any(item.get("resource_id") == target for item in shares):
    raise SystemExit(1)
PY
	echo "Internal share not visible to the recipient" >&2
	exit 1
fi
csrf_json_request "DELETE" "${API_BASE_URL}/shares/${INTERNAL_SHARE_ID}/recipient" "" "${ADMIN_COOKIES}" "${REVOKE_INTERNAL_SHARE_RESPONSE}"

echo "9. Verifying the admin audit log records smoke activity..."
# AuditLogQuery paginates with page/per_page (not limit).
AUDIT_HTTP="$(run_json_request "GET" "${API_BASE_URL}/admin/audit?per_page=20" "" "${ADMIN_COOKIES}" "${AUDIT_RESPONSE}")"
[[ "${AUDIT_HTTP}" == 2* ]] || {
	echo "Admin audit endpoint returned ${AUDIT_HTTP}" >&2
	exit 1
}
if ! python3 - "${AUDIT_RESPONSE}" <<'PY'; then
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

# PaginatedAuditLog: entries/total/page/per_page. The smoke created shares
# and revoked them; the log must not be empty.
entries = payload.get("entries")
if not isinstance(entries, list) or not entries:
    raise SystemExit(1)
PY
	echo "Admin audit log returned no entries; smoke activity was not recorded" >&2
	exit 1
fi

echo "10. Cleaning up smoke artifacts..."
csrf_json_request "DELETE" "${API_BASE_URL}/notes/${SMOKE_NOTE_ID}" "" "${ADMIN_COOKIES}" "${DELETE_NOTE_RESPONSE}"
csrf_json_request "DELETE" "${API_BASE_URL}/folders/${SMOKE_FOLDER_ID}" "" "${ADMIN_COOKIES}" "${DELETE_FOLDER_RESPONSE}"

echo "11. Verifying logout..."
csrf_json_request "POST" "${API_BASE_URL}/auth/logout" "{}" "${ADMIN_COOKIES}" "${LOGOUT_RESPONSE}"
POST_LOGOUT_STATUS="$(
	curl -sS -o "${POST_LOGOUT_ME_RESPONSE}" -w "%{http_code}" \
		-b "${ADMIN_COOKIES}" -c "${ADMIN_COOKIES}" \
		"${API_BASE_URL}/me"
)"
[[ "${POST_LOGOUT_STATUS}" == "401" ]] || {
	echo "Expected /me to return 401 after logout, got ${POST_LOGOUT_STATUS}" >&2
	exit 1
}

write_report "passed" "Beta smoke completed successfully."

echo "Beta smoke passed."
echo "Report written to ${REPORT_PATH}"
