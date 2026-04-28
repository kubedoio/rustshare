#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: scripts/post-restore-smoke.sh

Non-destructive post-restore smoke test for the current Rustshare deployment.

What it verifies:
- password login works
- cookie-backed authenticated session works
- root folder listing works
- a public share flow works when a token is available

Environment overrides:
- BASE_URL (default: http://localhost)
- API_BASE_URL (default: ${BASE_URL}/api/v1)
- ADMIN_EMAIL (default: admin@localhost)
- ADMIN_PASSWORD (default: )
- PUBLIC_SHARE_TOKEN (optional)
- PUBLIC_SHARE_PASSWORD (optional, for password-protected public shares)
- ALLOW_SKIP_PUBLIC_SHARE (default: false)
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

assert_json_has_key() {
	local file_path="$1"
	local key="$2"
	python3 - "$file_path" "$key" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    payload = json.load(handle)

value = payload
for part in sys.argv[2].split("."):
    if isinstance(value, list):
        value = value[int(part)]
    else:
        if part not in value:
            sys.exit(1)
        value = value[part]
PY
}

run_json_request() {
	local method="$1"
	local url="$2"
	local body="${3:-}"
	local auth_header="${4:-}"
	local cookie_jar="${5:-}"
	local output_file="$6"
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

	if [[ -n "$auth_header" ]]; then
		curl_args+=(-H "$auth_header")
	fi

	if [[ -n "$body" ]]; then
		curl_args+=(-H "Content-Type: application/json" --data "$body")
	fi

	status="$(curl "${curl_args[@]}" "$url")"
	if [[ "$status" != 2* ]]; then
		echo "Request failed: ${method} ${url} -> ${status}" >&2
		if [[ -s "$output_file" ]]; then
			echo "Response body:" >&2
			cat "$output_file" >&2
			echo >&2
		fi
		exit 1
	fi
}

require_command curl
require_command python3

BASE_URL="${BASE_URL:-http://localhost}"
API_BASE_URL="${API_BASE_URL:-${BASE_URL%/}/api/v1}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@localhost}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-}"
PUBLIC_SHARE_TOKEN="${PUBLIC_SHARE_TOKEN:-}"
PUBLIC_SHARE_PASSWORD="${PUBLIC_SHARE_PASSWORD:-}"
ALLOW_SKIP_PUBLIC_SHARE="${ALLOW_SKIP_PUBLIC_SHARE:-false}"

TMP_DIR="$(mktemp -d)"
COOKIE_JAR="${TMP_DIR}/cookies.txt"
LOGIN_RESPONSE="${TMP_DIR}/login.json"
ME_RESPONSE="${TMP_DIR}/me.json"
ROOT_RESPONSE="${TMP_DIR}/root.json"
SHARES_RESPONSE="${TMP_DIR}/shares.json"
SHARE_INFO_RESPONSE="${TMP_DIR}/share-info.json"
SHARE_SESSION_RESPONSE="${TMP_DIR}/share-session.json"
PUBLIC_FOLDER_RESPONSE="${TMP_DIR}/public-folder.json"
PUBLIC_FILE_RESPONSE="${TMP_DIR}/public-file.bin"

cleanup() {
	rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

echo "1. Logging in as ${ADMIN_EMAIL}..."
LOGIN_PAYLOAD="$(python3 - "$ADMIN_EMAIL" "$ADMIN_PASSWORD" <<'PY'
import json
import sys
print(json.dumps({"email": sys.argv[1], "password": sys.argv[2]}))
PY
)"
run_json_request "POST" "${API_BASE_URL}/auth/login" "${LOGIN_PAYLOAD}" "" "${COOKIE_JAR}" "${LOGIN_RESPONSE}"

LOGIN_EMAIL="$(json_get "${LOGIN_RESPONSE}" "user.email")"
if [[ "${LOGIN_EMAIL}" != "${ADMIN_EMAIL}" ]]; then
	echo "Login response email mismatch: expected ${ADMIN_EMAIL}, got ${LOGIN_EMAIL}" >&2
	exit 1
fi

if ! grep -q 'rustshare_session' "${COOKIE_JAR}"; then
	echo "Login did not create a rustshare_session cookie" >&2
	exit 1
fi

echo "2. Verifying authenticated session..."
run_json_request "GET" "${API_BASE_URL}/me" "" "" "${COOKIE_JAR}" "${ME_RESPONSE}"
SESSION_EMAIL="$(json_get "${ME_RESPONSE}" "email")"
if [[ "${SESSION_EMAIL}" != "${ADMIN_EMAIL}" ]]; then
	echo "Authenticated profile email mismatch: expected ${ADMIN_EMAIL}, got ${SESSION_EMAIL}" >&2
	exit 1
fi

echo "3. Verifying root file listing..."
run_json_request "GET" "${API_BASE_URL}/folders/root/contents" "" "" "${COOKIE_JAR}" "${ROOT_RESPONSE}"
assert_json_has_key "${ROOT_RESPONSE}" "files"
assert_json_has_key "${ROOT_RESPONSE}" "folders"

SELECTED_SHARE_TOKEN="${PUBLIC_SHARE_TOKEN}"
SELECTED_SHARE_PASSWORD="${PUBLIC_SHARE_PASSWORD}"

if [[ -z "${SELECTED_SHARE_TOKEN}" ]]; then
	echo "4. Discovering a public share for smoke validation..."
	run_json_request "GET" "${API_BASE_URL}/shares" "" "" "${COOKIE_JAR}" "${SHARES_RESPONSE}"
	readarray -t AUTO_SHARE < <(python3 - "${SHARES_RESPONSE}" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    shares = json.load(handle)

selected = None
for share in shares:
    if not share.get("password_protected", False):
        selected = share
        break

if selected is None and shares:
    selected = shares[0]

if selected is not None:
    print(selected.get("share_token", ""))
    print("true" if selected.get("password_protected", False) else "false")
PY
)

	if [[ "${#AUTO_SHARE[@]}" -gt 0 && -n "${AUTO_SHARE[0]}" ]]; then
		SELECTED_SHARE_TOKEN="${AUTO_SHARE[0]}"
		if [[ "${AUTO_SHARE[1]:-false}" == "true" && -z "${SELECTED_SHARE_PASSWORD}" ]]; then
			if [[ "${ALLOW_SKIP_PUBLIC_SHARE}" == "true" ]]; then
				echo "4. No non-password public share available; skipping public-share smoke test."
				echo "Smoke test passed for authenticated flows."
				exit 0
			fi
			echo "A public share exists but it is password protected. Set PUBLIC_SHARE_PASSWORD or PUBLIC_SHARE_TOKEN." >&2
			exit 1
		fi
	else
		if [[ "${ALLOW_SKIP_PUBLIC_SHARE}" == "true" ]]; then
			echo "4. No public share available; skipping public-share smoke test."
			echo "Smoke test passed for authenticated flows."
			exit 0
		fi
		echo "No public share available for smoke testing. Set PUBLIC_SHARE_TOKEN or create a public share first." >&2
		exit 1
	fi
fi

echo "4. Verifying public share info..."
run_json_request "GET" "${API_BASE_URL}/public/share/${SELECTED_SHARE_TOKEN}/info" "" "" "" "${SHARE_INFO_RESPONSE}"
RESOURCE_TYPE="$(json_get "${SHARE_INFO_RESPONSE}" "resource_type")"
PASSWORD_PROTECTED="$(json_get "${SHARE_INFO_RESPONSE}" "password_protected" || echo false)"
UPLOAD_ONLY="$(json_get "${SHARE_INFO_RESPONSE}" "upload_only" || echo false)"

SESSION_PAYLOAD="{}"
if [[ "${PASSWORD_PROTECTED}" == "true" ]]; then
	if [[ -z "${SELECTED_SHARE_PASSWORD}" ]]; then
		echo "Public share requires a password but PUBLIC_SHARE_PASSWORD was not provided." >&2
		exit 1
	fi
	SESSION_PAYLOAD="$(python3 - "$SELECTED_SHARE_PASSWORD" <<'PY'
import json
import sys
print(json.dumps({"password": sys.argv[1]}))
PY
)"
fi

echo "5. Creating public share session..."
run_json_request "POST" "${API_BASE_URL}/public/share/${SELECTED_SHARE_TOKEN}/session" "${SESSION_PAYLOAD}" "" "" "${SHARE_SESSION_RESPONSE}"
SESSION_TOKEN="$(json_get "${SHARE_SESSION_RESPONSE}" "session_token")"

if [[ "${RESOURCE_TYPE}" == "file" ]]; then
	echo "6. Verifying public file download..."
	FILE_STATUS="$(curl -sS -H "Authorization: Bearer ${SESSION_TOKEN}" -o "${PUBLIC_FILE_RESPONSE}" -w "%{http_code}" "${API_BASE_URL}/public/share/${SELECTED_SHARE_TOKEN}/file")"
	if [[ "${FILE_STATUS}" != 2* ]]; then
		echo "Public file download failed with status ${FILE_STATUS}" >&2
		exit 1
	fi
elif [[ "${RESOURCE_TYPE}" == "folder" && "${UPLOAD_ONLY}" != "true" ]]; then
	echo "6. Verifying public folder listing..."
	run_json_request "GET" "${API_BASE_URL}/public/share/${SELECTED_SHARE_TOKEN}/folder/contents" "" "Authorization: Bearer ${SESSION_TOKEN}" "" "${PUBLIC_FOLDER_RESPONSE}"
	assert_json_has_key "${PUBLIC_FOLDER_RESPONSE}" "files"
	assert_json_has_key "${PUBLIC_FOLDER_RESPONSE}" "folders"
else
	echo "6. Upload-only public folder detected; verified info and session creation without mutating data."
fi

echo "Post-restore smoke test passed."
