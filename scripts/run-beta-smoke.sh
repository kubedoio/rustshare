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
# - PILOT_EXERCISE_USER_LIFECYCLE (default: 0; set to 1 only in an ephemeral
#   deployment to verify admin password reset, session revocation, disable and
#   cleanup)
# - REPORT_DIR (default: ./beta-smoke-reports)
# - PILOT_REPORT_PATH (optional exact report path)
# - PILOT_SOURCE_SHA, PILOT_BUILD_VERSION, PILOT_DEPLOYMENT_ID and
#   PILOT_CONFIG_ID (evidence identity fields)
# - PILOT_PRESERVE_DATA=1 (leave the representative data for restart/restore)
# - PILOT_VERIFY_STATE_FILE (verify data identified by a prior report)

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

state_get() {
	local key="$1"
	awk -F= -v key="${key}" '$1 == key {sub(/^[^=]*=/, ""); print; exit}' \
		"${PILOT_VERIFY_STATE_FILE}"
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
	local expect_2xx="${5:-1}"
	local payload
	payload="$(python3 - "$email" "$password" <<'PY'
import json
import sys
print(json.dumps({"email": sys.argv[1], "password": sys.argv[2]}))
PY
)"
	run_json_request "POST" "${API_BASE_URL}/auth/login" "${payload}" "${cookie_jar}" "${output_file}" "" "${expect_2xx}"
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
	local expect_2xx="${6:-1}"
	run_json_request "$method" "$url" "$body" "${cookie_jar}" "${output_file}" "X-Rustshare-Csrf: $(csrf_token_from_jar "${cookie_jar}")" "${expect_2xx}"
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
BETA_SMOKE_MODE=${PILOT_MODE}
BETA_SMOKE_STARTED_AT=${STARTED_AT}
BETA_SMOKE_FINISHED_AT=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
BETA_SMOKE_BASE_URL=${BASE_URL}
BETA_SMOKE_SOURCE_SHA=${PILOT_SOURCE_SHA}
BETA_SMOKE_BUILD_VERSION=${PILOT_BUILD_VERSION}
BETA_SMOKE_DEPLOYMENT_ID=${PILOT_DEPLOYMENT_ID}
BETA_SMOKE_CONFIG_ID=${PILOT_CONFIG_ID}
BETA_SMOKE_FAILURE_PHASE=${CURRENT_PHASE}
BETA_SMOKE_PRESERVE_DATA=${PILOT_PRESERVE_DATA}
BETA_SMOKE_USER_LIFECYCLE=${USER_LIFECYCLE_RESULT}
ADMIN_EMAIL=${ADMIN_EMAIL}
VIEWER_EMAIL=${VIEWER_EMAIL}
SMOKE_FOLDER_ID=${SMOKE_FOLDER_ID:-}
SMOKE_FILE_ID=${SMOKE_FILE_ID:-}
SMOKE_NOTE_ID=${SMOKE_NOTE_ID:-}
SMOKE_NOTE_TITLE=${SMOKE_NOTE_TITLE:-}
SEARCH_NEEDLE=${SEARCH_NEEDLE:-}
CHAT_STATUS_HTTP=${CHAT_STATUS_HTTP:-}
REPORT_DETAILS=${details}
EOF
}

require_command curl
require_command python3
require_command cmp

PILOT_MODE="${PILOT_MODE:-canonical}"
PILOT_SOURCE_SHA="${PILOT_SOURCE_SHA:-unknown}"
PILOT_BUILD_VERSION="${PILOT_BUILD_VERSION:-unknown}"
PILOT_DEPLOYMENT_ID="${PILOT_DEPLOYMENT_ID:-unknown}"
PILOT_CONFIG_ID="${PILOT_CONFIG_ID:-unknown}"
PILOT_PRESERVE_DATA="${PILOT_PRESERVE_DATA:-0}"
PILOT_VERIFY_STATE_FILE="${PILOT_VERIFY_STATE_FILE:-}"
PILOT_EXERCISE_USER_LIFECYCLE="${PILOT_EXERCISE_USER_LIFECYCLE:-0}"
USER_LIFECYCLE_RESULT="not_run"
CURRENT_PHASE="bootstrap"

if [[ "${PILOT_EXERCISE_USER_LIFECYCLE}" != "0" && "${PILOT_EXERCISE_USER_LIFECYCLE}" != "1" ]]; then
	echo "PILOT_EXERCISE_USER_LIFECYCLE must be 0 or 1" >&2
	exit 1
fi

BASE_URL="${BASE_URL:-http://localhost}"
API_BASE_URL="${API_BASE_URL:-${BASE_URL%/}/api/v1}"
ADMIN_EMAIL="${ADMIN_EMAIL:-${RUSTSHARE_ADMIN_EMAIL:-admin@localhost}}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-${RUSTSHARE_ADMIN_PASSWORD:-}}"
VIEWER_EMAIL="${VIEWER_EMAIL:-${RUSTSHARE_DEMO_VIEWER_EMAIL:-viewer@localhost}}"
VIEWER_PASSWORD="${VIEWER_PASSWORD:-${RUSTSHARE_DEMO_VIEWER_PASSWORD:-}}"
REPORT_DIR="${REPORT_DIR:-$(pwd)/beta-smoke-reports}"
STARTED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
REPORT_PATH="${REPORT_DIR%/}/$(date -u +%Y%m%dT%H%M%SZ)-$$-beta-smoke.env"
if [[ -n "${PILOT_REPORT_PATH:-}" ]]; then
	REPORT_PATH="${PILOT_REPORT_PATH}"
fi

TMP_DIR=""
cleanup() {
	if [[ -n "${TMP_DIR}" ]]; then
		rm -rf "${TMP_DIR}"
	fi
}
# Failure reporting keys off the exit code, not the ERR trap: bash does not
# fire ERR for explicit `exit 1` branches, which this script uses for every
# assertion failure. On success the passed report is written by the main
# flow before exit 0.
on_exit() {
	local code=$?
	if [[ "${code}" -ne 0 ]]; then
		write_report "failed" "Beta smoke failed during phase ${CURRENT_PHASE}."
	fi
	if ! cleanup; then
		code=1
		write_report "failed" "Beta smoke cleanup failed after phase ${CURRENT_PHASE}."
	fi
	return "${code}"
}
trap on_exit EXIT

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

TMP_DIR="$(mktemp -d)"
ADMIN_COOKIES="${TMP_DIR}/admin.cookies"
VIEWER_COOKIES="${TMP_DIR}/viewer.cookies"

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
RENAME_NOTE_RESPONSE="${TMP_DIR}/rename-note.json"
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
PRIVATE_FILE_NAME="beta-private-${SEARCH_NEEDLE}.txt"

echo "0. Verifying backend readiness..."
CURRENT_PHASE="readiness"
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
CURRENT_PHASE="authentication"
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

if [[ -n "${PILOT_VERIFY_STATE_FILE}" ]]; then
	CURRENT_PHASE="persistence"
	[[ -f "${PILOT_VERIFY_STATE_FILE}" ]] || {
		echo "Persistence state file not found: ${PILOT_VERIFY_STATE_FILE}" >&2
		exit 1
	}
	VERIFY_NOTE_ID="$(state_get SMOKE_NOTE_ID)"
	VERIFY_FILE_ID="$(state_get SMOKE_FILE_ID)"
	VERIFY_NOTE_TITLE="$(state_get SMOKE_NOTE_TITLE)"
	VERIFY_SEARCH_NEEDLE="$(state_get SEARCH_NEEDLE)"
	[[ -n "${VERIFY_NOTE_ID}" && -n "${VERIFY_FILE_ID}" && -n "${VERIFY_NOTE_TITLE}" && -n "${VERIFY_SEARCH_NEEDLE}" ]] || {
		echo "Persistence state file is missing smoke identifiers" >&2
		exit 1
	}

	echo "2p. Verifying the persisted Note after restart/restore..."
	run_json_request "GET" "${API_BASE_URL}/notes/${VERIFY_NOTE_ID}" "" "${ADMIN_COOKIES}" "${GET_NOTE_RESPONSE}"
	python3 - "${GET_NOTE_RESPONSE}" "${VERIFY_SEARCH_NEEDLE}" "${VERIFY_NOTE_TITLE}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

needle = "updated needle " + sys.argv[2]
if needle not in payload.get("content", ""):
    raise SystemExit("persisted note content marker was not found")
if "# Beta Smoke H1" not in payload.get("content", ""):
    raise SystemExit("persisted note H1 was not preserved")
if payload.get("metadata", {}).get("title") != sys.argv[3]:
    raise SystemExit("persisted note title was not preserved")
PY

	echo "3p. Verifying the persisted File after restart/restore..."
	PERSISTED_DOWNLOAD="${TMP_DIR}/persisted-download.bin"
	PERSISTED_DOWNLOAD_STATUS="$(
		curl -sS -X GET -o "${PERSISTED_DOWNLOAD}" -w "%{http_code}" \
			-b "${ADMIN_COOKIES}" -c "${ADMIN_COOKIES}" \
			"${API_BASE_URL}/files/${VERIFY_FILE_ID}/download"
	)"
	[[ "${PERSISTED_DOWNLOAD_STATUS}" == 2* && -s "${PERSISTED_DOWNLOAD}" ]] || {
		echo "Persisted file download failed with status ${PERSISTED_DOWNLOAD_STATUS}" >&2
		exit 1
	}

	SMOKE_NOTE_TITLE="${VERIFY_NOTE_TITLE}"
	CURRENT_PHASE="complete"
	write_report "passed" "Persistence verification completed successfully."
	echo "Persistence verification passed."
	echo "Report written to ${REPORT_PATH}"
	exit 0
fi

echo "2. Verifying root listing..."
CURRENT_PHASE="files"
run_json_request "GET" "${API_BASE_URL}/folders/root/contents" "" "${ADMIN_COOKIES}" "${ROOT_RESPONSE}"
json_get "${ROOT_RESPONSE}" "files" >/dev/null
json_get "${ROOT_RESPONSE}" "folders" >/dev/null

echo "3. Creating a smoke folder and uploading a private file..."
CURRENT_PHASE="files"
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
		-F "name=${PRIVATE_FILE_NAME}" \
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
CURRENT_PHASE="files"
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
CURRENT_PHASE="notes"
NOTE_TITLE="Beta Smoke ${SEARCH_NEEDLE}"
NOTE_PAYLOAD="$(python3 - "${NOTE_TITLE}" "${SEARCH_NEEDLE}" "${SMOKE_FOLDER_ID}" <<'PY'
import json
import sys
print(json.dumps({
    "title": sys.argv[1],
    "content": "needle " + sys.argv[2],
    "parent_folder_id": sys.argv[3],
}))
PY
)"
csrf_json_request "POST" "${API_BASE_URL}/notes" "${NOTE_PAYLOAD}" "${ADMIN_COOKIES}" "${CREATE_NOTE_RESPONSE}"
SMOKE_NOTE_ID="$(json_get "${CREATE_NOTE_RESPONSE}" "id")"
SMOKE_NOTE_TITLE="${NOTE_TITLE}"
run_json_request "GET" "${API_BASE_URL}/notes/${SMOKE_NOTE_ID}" "" "${ADMIN_COOKIES}" "${GET_NOTE_RESPONSE}"
SAVE_NOTE_PAYLOAD="$(python3 - "${SEARCH_NEEDLE}" <<'PY'
import json
import sys
# SaveNoteRequest has no title field: only content/color/attachments.
print(json.dumps({"content": "# Beta Smoke H1\n\nupdated needle " + sys.argv[1]}))
PY
)"
csrf_json_request "PUT" "${API_BASE_URL}/notes/${SMOKE_NOTE_ID}" "${SAVE_NOTE_PAYLOAD}" "${ADMIN_COOKIES}" "${SAVE_NOTE_RESPONSE}"
run_json_request "GET" "${API_BASE_URL}/notes/${SMOKE_NOTE_ID}" "" "${ADMIN_COOKIES}" "${GET_NOTE_RESPONSE}"
python3 - "${GET_NOTE_RESPONSE}" "${SMOKE_NOTE_TITLE}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

if payload.get("metadata", {}).get("title") != sys.argv[2]:
    raise SystemExit("editing the H1 unexpectedly renamed the note")
if "# Beta Smoke H1" not in payload.get("content", ""):
    raise SystemExit("edited note H1 was not preserved")
PY

RENAMED_NOTE_TITLE="Beta Smoke Renamed ${SEARCH_NEEDLE}"
RENAME_NOTE_PAYLOAD="$(python3 - "${RENAMED_NOTE_TITLE}" <<'PY'
import json
import sys
print(json.dumps({"title": sys.argv[1]}))
PY
)"
csrf_json_request "POST" "${API_BASE_URL}/notes/${SMOKE_NOTE_ID}/rename" "${RENAME_NOTE_PAYLOAD}" "${ADMIN_COOKIES}" "${RENAME_NOTE_RESPONSE}"
SMOKE_NOTE_TITLE="${RENAMED_NOTE_TITLE}"
run_json_request "GET" "${API_BASE_URL}/notes/${SMOKE_NOTE_ID}" "" "${ADMIN_COOKIES}" "${GET_NOTE_RESPONSE}"
python3 - "${GET_NOTE_RESPONSE}" "${SMOKE_NOTE_TITLE}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

if payload.get("metadata", {}).get("title") != sys.argv[2]:
    raise SystemExit("explicit note rename was not persisted")
if "# Beta Smoke H1" not in payload.get("content", ""):
    raise SystemExit("explicit note rename changed the Markdown H1")
PY
run_json_request "GET" "${API_BASE_URL}/notes" "" "${ADMIN_COOKIES}" "${LIST_NOTES_RESPONSE}"

echo "6. Verifying permission-aware search finds the note..."
CURRENT_PHASE="search"
# Search runs with a bounded retry because a freshly created note may take a
# few seconds to appear in the name/path index. The unique marker is in the
# note title, while the body/H1 assertions above cover Markdown content.
SEARCH_PAYLOAD="$(python3 - "${SEARCH_NEEDLE}" <<'PY'
import json
import sys
print(json.dumps({"query": sys.argv[1], "limit": 10}))
PY
)"
SEARCH_OK=0
for _ in $(seq 1 10); do
	# Non-2xx is tolerated inside the bounded retry window while the new
	# metadata row becomes visible through the normal request path.
	SEARCH_HTTP="$(csrf_json_request "POST" "${API_BASE_URL}/search" "${SEARCH_PAYLOAD}" "${ADMIN_COOKIES}" "${SEARCH_RESPONSE}" "0")"
	if [[ "${SEARCH_HTTP}" != 2* ]]; then
		sleep 3
		continue
	fi
	if python3 - "${SEARCH_RESPONSE}" "${SEARCH_NEEDLE}" <<'PY'; then
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

results = payload.get("results") or payload.get("items") or []
if not isinstance(results, list):
    raise SystemExit(1)
needle = sys.argv[2]
if not any(needle in json.dumps(item) for item in results):
    raise SystemExit(1)
PY
		SEARCH_OK=1
		break
	fi
	sleep 3
done
[[ "${SEARCH_OK}" == "1" ]] || {
	cp "${SEARCH_RESPONSE}" "${REPORT_DIR%/}/search-failure-response.json" 2>/dev/null || true
	echo "Search did not surface the smoke note (query: ${SEARCH_NEEDLE}) within 30s" >&2
	exit 1
}

echo "6b. Verifying search is permission-aware (viewer must NOT find the note)..."
CURRENT_PHASE="authorization"
# The viewer holds no share on the smoke note; permission-aware search must
# not surface it. This is the negative half of the search evidence.
VIEWER_SEARCH_HTTP="$(csrf_json_request "POST" "${API_BASE_URL}/search" "${SEARCH_PAYLOAD}" "${VIEWER_COOKIES}" "${VIEWER_SEARCH_RESPONSE}" "0")"
if [[ "${VIEWER_SEARCH_HTTP}" != 2* ]]; then
	echo "Viewer search request failed with ${VIEWER_SEARCH_HTTP}" >&2
	exit 1
fi
if ! python3 - "${VIEWER_SEARCH_RESPONSE}" "${SEARCH_NEEDLE}" <<'PY'; then
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

results = payload.get("results") or payload.get("items") or []
if not isinstance(results, list):
    raise SystemExit(1)
needle = sys.argv[2]
leaked = [item for item in results if needle in json.dumps(item)]
if leaked:
    raise SystemExit(1)
PY
	echo "PERMISSION FAILURE: viewer search surfaced the admin-only smoke note; treat as Sev-1" >&2
	exit 1
fi

echo "7. Checking chat application status..."
CURRENT_PHASE="collaboration"
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
CURRENT_PHASE="sharing"
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

# ponytail: exercise mutable account lifecycle only in the disposable CI stack.
if [[ "${PILOT_EXERCISE_USER_LIFECYCLE}" == "1" ]]; then
	echo "8b. Verifying admin password recovery and user offboarding..."
	CURRENT_PHASE="account_lifecycle"
	USER_LIFECYCLE_RESULT="running"
	LIFECYCLE_SUFFIX="$(date -u +%s)-${RANDOM}"
	LIFECYCLE_EMAIL="pilot-lifecycle-${LIFECYCLE_SUFFIX}@example.invalid"
	LIFECYCLE_USERNAME="pilot-lifecycle-${LIFECYCLE_SUFFIX}"
	LIFECYCLE_INITIAL_PASSWORD="$(python3 -c 'import secrets; print(secrets.token_urlsafe(24))')"
	LIFECYCLE_RESET_PASSWORD="$(python3 -c 'import secrets; print(secrets.token_urlsafe(24))')"
	LIFECYCLE_CHANGED_PASSWORD="$(python3 -c 'import secrets; print(secrets.token_urlsafe(24))')"
	LIFECYCLE_CREATE_RESPONSE="${TMP_DIR}/lifecycle-create.json"
	LIFECYCLE_LOGIN_RESPONSE="${TMP_DIR}/lifecycle-login.json"
	LIFECYCLE_COOKIE_JAR="${TMP_DIR}/lifecycle.cookies"
	LIFECYCLE_ME_RESPONSE="${TMP_DIR}/lifecycle-me.json"
	LIFECYCLE_MUTATION_RESPONSE="${TMP_DIR}/lifecycle-mutation.json"
	LIFECYCLE_PAYLOAD="$(python3 - "${LIFECYCLE_USERNAME}" "${LIFECYCLE_EMAIL}" "${LIFECYCLE_INITIAL_PASSWORD}" <<'PY'
import json
import sys
print(json.dumps({"username": sys.argv[1], "email": sys.argv[2], "password": sys.argv[3], "display_name": "Pilot lifecycle check", "is_admin": False}))
PY
)"
	csrf_json_request "POST" "${API_BASE_URL}/admin/users" "${LIFECYCLE_PAYLOAD}" "${ADMIN_COOKIES}" "${LIFECYCLE_CREATE_RESPONSE}"
	LIFECYCLE_USER_ID="$(json_get "${LIFECYCLE_CREATE_RESPONSE}" "id")"
	login_with_password "${LIFECYCLE_EMAIL}" "${LIFECYCLE_INITIAL_PASSWORD}" "${LIFECYCLE_COOKIE_JAR}" "${LIFECYCLE_LOGIN_RESPONSE}"
	LIFECYCLE_RESET_PAYLOAD="$(python3 - "${LIFECYCLE_RESET_PASSWORD}" <<'PY'
import json
import sys
print(json.dumps({"password": sys.argv[1]}))
PY
)"
	csrf_json_request "PATCH" "${API_BASE_URL}/admin/users/${LIFECYCLE_USER_ID}" "${LIFECYCLE_RESET_PAYLOAD}" "${ADMIN_COOKIES}" "${LIFECYCLE_MUTATION_RESPONSE}"
	LIFECYCLE_SESSION_STATUS="$(run_json_request "GET" "${API_BASE_URL}/me" "" "${LIFECYCLE_COOKIE_JAR}" "${LIFECYCLE_ME_RESPONSE}" "" "0")"
	[[ "${LIFECYCLE_SESSION_STATUS}" == "401" ]] || {
		echo "Admin password reset did not revoke the user's active session" >&2
		exit 1
	}
	LIFECYCLE_LOGIN_STATUS="$(login_with_password "${LIFECYCLE_EMAIL}" "${LIFECYCLE_INITIAL_PASSWORD}" "" "${LIFECYCLE_LOGIN_RESPONSE}" "0")"
	[[ "${LIFECYCLE_LOGIN_STATUS}" == "401" ]] || {
		echo "The previous password still authenticates after admin reset" >&2
		exit 1
	}
	login_with_password "${LIFECYCLE_EMAIL}" "${LIFECYCLE_RESET_PASSWORD}" "${LIFECYCLE_COOKIE_JAR}" "${LIFECYCLE_LOGIN_RESPONSE}"
	LIFECYCLE_CHANGE_PAYLOAD="$(python3 - "${LIFECYCLE_RESET_PASSWORD}" "${LIFECYCLE_CHANGED_PASSWORD}" <<'PY'
import json
import sys
print(json.dumps({"current_password": sys.argv[1], "new_password": sys.argv[2], "confirm_password": sys.argv[2]}))
PY
)"
	csrf_json_request "PUT" "${API_BASE_URL}/me/password" "${LIFECYCLE_CHANGE_PAYLOAD}" "${LIFECYCLE_COOKIE_JAR}" "${LIFECYCLE_MUTATION_RESPONSE}"
	LIFECYCLE_LOGIN_STATUS="$(login_with_password "${LIFECYCLE_EMAIL}" "${LIFECYCLE_RESET_PASSWORD}" "" "${LIFECYCLE_LOGIN_RESPONSE}" "0")"
	[[ "${LIFECYCLE_LOGIN_STATUS}" == "401" ]] || {
		echo "The previous password still authenticates after self-service password change" >&2
		exit 1
	}
	login_with_password "${LIFECYCLE_EMAIL}" "${LIFECYCLE_CHANGED_PASSWORD}" "${LIFECYCLE_COOKIE_JAR}" "${LIFECYCLE_LOGIN_RESPONSE}"
	csrf_json_request "POST" "${API_BASE_URL}/admin/users/${LIFECYCLE_USER_ID}/disable" "" "${ADMIN_COOKIES}" "${LIFECYCLE_MUTATION_RESPONSE}"
	LIFECYCLE_SESSION_STATUS="$(run_json_request "GET" "${API_BASE_URL}/me" "" "${LIFECYCLE_COOKIE_JAR}" "${LIFECYCLE_ME_RESPONSE}" "" "0")"
	[[ "${LIFECYCLE_SESSION_STATUS}" == "401" ]] || {
		echo "Disabling the user did not revoke the user's active session" >&2
		exit 1
	}
	LIFECYCLE_LOGIN_STATUS="$(login_with_password "${LIFECYCLE_EMAIL}" "${LIFECYCLE_RESET_PASSWORD}" "" "${LIFECYCLE_LOGIN_RESPONSE}" "0")"
	[[ "${LIFECYCLE_LOGIN_STATUS}" == "401" ]] || {
		echo "A disabled user can still authenticate" >&2
		exit 1
	}
	csrf_json_request "DELETE" "${API_BASE_URL}/admin/users/${LIFECYCLE_USER_ID}" "" "${ADMIN_COOKIES}" "${LIFECYCLE_MUTATION_RESPONSE}"
	USER_LIFECYCLE_RESULT="passed"
	echo "Account lifecycle passed: reset revoked sessions, password change worked, and disabled account denied login."
fi

echo "9. Verifying the admin audit log records smoke activity..."
CURRENT_PHASE="observability"
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
CURRENT_PHASE="cleanup"
if [[ "${PILOT_PRESERVE_DATA}" == "1" ]]; then
	echo "Preserving representative pilot data for the restart/restore checks."
else
	csrf_json_request "DELETE" "${API_BASE_URL}/notes/${SMOKE_NOTE_ID}" "" "${ADMIN_COOKIES}" "${DELETE_NOTE_RESPONSE}"
	csrf_json_request "DELETE" "${API_BASE_URL}/folders/${SMOKE_FOLDER_ID}" "" "${ADMIN_COOKIES}" "${DELETE_FOLDER_RESPONSE}"
fi

echo "11. Verifying logout..."
CURRENT_PHASE="logout"
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

CURRENT_PHASE="complete"
write_report "passed" "Beta smoke completed successfully."

echo "Beta smoke passed."
echo "Report written to ${REPORT_PATH}"
