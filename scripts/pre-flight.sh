#!/usr/bin/env bash
#
# pre-flight.sh
#
# Deployment pre-flight checker. Verifies that all required secrets exist
# in the local .env file, auto-generates strong values for any that are
# missing or weak, appends them to .env, and exports them for docker-compose.
#
# Usage:
#   source ./scripts/pre-flight.sh
#   docker compose up -d
#
#   # Or non-interactive:
#   . ./scripts/pre-flight.sh && docker compose up -d
#

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

ENV_FILE="${REPO_ROOT}/.env"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

require_command() {
	local command_name="$1"
	if ! command -v "${command_name}" >/dev/null 2>&1; then
		echo "Missing required command: ${command_name}" >&2
		exit 1
	fi
}

info() {
	printf "\033[1;34mℹ\033[0m %s\n" "$1"
}

ok() {
	printf "\033[1;32m✓\033[0m %s\n" "$1"
}

warn() {
	printf "\033[1;33m⚠\033[0m %s\n" "$1" >&2
}

error() {
	printf "\033[1;31m✗\033[0m %s\n" "$1" >&2
}

# Read a variable from .env (ignores comments, handles simple KEY=VAL lines)
env_get() {
	local key="$1"
	local file="${2:-${ENV_FILE}}"
	if [[ ! -f "${file}" ]]; then
		return 1
	fi
	# Use grep to find the line, strip comments, extract value
	local line
	line="$(grep "^${key}=" "${file}" 2>/dev/null | tail -n 1 || true)"
	if [[ -z "${line}" ]]; then
		return 1
	fi
	# Strip everything before =
	printf '%s' "${line#*=}"
}

# Append a key=value pair to .env
env_set() {
	local key="$1"
	local value="$2"
	printf '%s=%s\n' "${key}" "${value}" >> "${ENV_FILE}"
}

# Update an existing key=value pair in .env, or append it if absent.
# Uses a temp file to avoid duplicate keys.
env_update_or_set() {
	local key="$1"
	local value="$2"
	local tmp_file
	tmp_file="$(mktemp)"
	if grep -q "^${key}=" "${ENV_FILE}" 2>/dev/null; then
		awk -v k="${key}=" -v v="${key}=${value}" 'index($0, k) == 1 {print v; next} {print}' "${ENV_FILE}" > "${tmp_file}"
		mv "${tmp_file}" "${ENV_FILE}"
	else
		printf '%s=%s\n' "${key}" "${value}" >> "${ENV_FILE}"
		rm -f "${tmp_file}"
	fi
}

# Generate a strong secret (machine-only)
generate_secret() {
	openssl rand -base64 32
}

# Generate a strong password (URL-safe hex — avoids / and + that break DATABASE_URL)
generate_password() {
	openssl rand -hex 32
}

# Generate an S3-compatible access key (alphanumeric, <=20 chars)
generate_access_key() {
	openssl rand -base64 15 | tr -d '/+=' | cut -c1-20
}

# ---------------------------------------------------------------------------
# Secret definitions
# ---------------------------------------------------------------------------

# Each entry: VAR_NAME|TYPE|MIN_LENGTH|WEAK_DEFAULTS
# TYPE: secret (machine key) or password (human credential)
# WEAK_DEFAULTS: pipe-separated list of known-bad values
SECRET_SPECS=(
	"JWT_SECRET|secret|32|change-this-secret-in-production|dev-secret-change-in-production|ci-pilot-secret"
	"RUSTSHARE_SECRET_ENCRYPTION_KEY|secret|32|AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
	"RUSTSHARE_CHAT_WEBHOOK_SECRET|secret|32|change-me-in-production"
	"POSTGRES_PASSWORD|password|16|changeme"
	"RUSTFS_ROOT_USER|access_key|4|rustfsadmin"
	"RUSTFS_ROOT_PASSWORD|password|16|rustfsadmin"
	"RUSTSHARE_DEMO_VIEWER_PASSWORD|password|12|"
)

# Variables that docker-compose requires but are not secrets (we just check presence)
REQUIRED_NON_SECRETS=(
	"DATABASE_URL"
	"STORAGE_ENDPOINT"
	"STORAGE_BUCKET"
	"STORAGE_REGION"
	"ORIGIN"
	"VITE_API_URL"
	"VITE_WS_URL"
)

# ---------------------------------------------------------------------------
# Preflight checks
# ---------------------------------------------------------------------------

require_command openssl

# ---------------------------------------------------------------------------
# .env existence
# ---------------------------------------------------------------------------

if [[ ! -f "${ENV_FILE}" ]]; then
	if [[ -f "${REPO_ROOT}/.env.example" ]]; then
		warn ".env not found — copying from .env.example"
		cp "${REPO_ROOT}/.env.example" "${ENV_FILE}"
	else
		error ".env not found and .env.example is missing. Cannot continue."
		exit 1
	fi
fi

# ---------------------------------------------------------------------------
# Backup .env before any modifications
# ---------------------------------------------------------------------------

BACKUP_FILE="${ENV_FILE}.backup.$(date +%Y%m%d%H%M%S)"
cp "${ENV_FILE}" "${BACKUP_FILE}"
info ".env backed up to ${BACKUP_FILE}"

# ---------------------------------------------------------------------------
# Check / generate secrets
# ---------------------------------------------------------------------------

GENERATED_COUNT=0
WEAK_COUNT=0
OK_COUNT=0

for spec in "${SECRET_SPECS[@]}"; do
	IFS='|' read -r var_name var_type min_len weak_defaults <<< "${spec}"

	current_value=""
	if env_get "${var_name}" >/dev/null 2>&1; then
		current_value="$(env_get "${var_name}")"
	fi

	# Check if missing
	is_missing=0
	is_weak=0

	if [[ -z "${current_value}" ]]; then
		is_missing=1
	elif [[ "${#current_value}" -lt "${min_len}" ]]; then
		is_weak=1
	else
		# Check against known weak defaults
		IFS='|' read -r -a weak_array <<< "${weak_defaults}"
		for weak in "${weak_array[@]}"; do
			if [[ -n "${weak}" && "${current_value}" == "${weak}" ]]; then
				is_weak=1
				break
			fi
		done
	fi

	if [[ "${is_missing}" -eq 1 ]]; then
		new_value=""
		if [[ "${var_type}" == "secret" ]]; then
			new_value="$(generate_secret)"
		elif [[ "${var_type}" == "access_key" ]]; then
			new_value="$(generate_access_key)"
		else
			new_value="$(generate_password)"
		fi
		env_set "${var_name}" "${new_value}"
		warn "${var_name} was missing — generated and appended to .env"
		GENERATED_COUNT=$((GENERATED_COUNT + 1))
	elif [[ "${is_weak}" -eq 1 ]]; then
		new_value=""
		if [[ "${var_type}" == "secret" ]]; then
			new_value="$(generate_secret)"
		elif [[ "${var_type}" == "access_key" ]]; then
			new_value="$(generate_access_key)"
		else
			new_value="$(generate_password)"
		fi
		env_set "${var_name}" "${new_value}"
		warn "${var_name} was weak (too short or known default) — generated and appended to .env"
		WEAK_COUNT=$((WEAK_COUNT + 1))
	else
		ok "${var_name}"
		OK_COUNT=$((OK_COUNT + 1))
	fi

	# Export for docker-compose use
	export "${var_name}=$(env_get "${var_name}")"
done

# ---------------------------------------------------------------------------
# Derive S3/RustFS credentials from RustFS root credentials
# ---------------------------------------------------------------------------
# STORAGE_ACCESS_KEY / STORAGE_SECRET_KEY and AWS_ACCESS_KEY_ID /
# AWS_SECRET_ACCESS_KEY must match RustFS root credentials.
RUSTFS_ROOT_USER_VALUE="$(env_get "RUSTFS_ROOT_USER")"
RUSTFS_ROOT_PASSWORD_VALUE="$(env_get "RUSTFS_ROOT_PASSWORD")"

if [[ -n "${RUSTFS_ROOT_USER_VALUE}" && -n "${RUSTFS_ROOT_PASSWORD_VALUE}" ]]; then
	for derived_var in STORAGE_ACCESS_KEY AWS_ACCESS_KEY_ID; do
		current_derived=""
		if env_get "${derived_var}" >/dev/null 2>&1; then
			current_derived="$(env_get "${derived_var}")"
		fi
		if [[ "${current_derived}" != "${RUSTFS_ROOT_USER_VALUE}" ]]; then
			env_update_or_set "${derived_var}" "${RUSTFS_ROOT_USER_VALUE}"
			warn "${derived_var} did not match RUSTFS_ROOT_USER — updated in .env"
			WEAK_COUNT=$((WEAK_COUNT + 1))
		fi
		export "${derived_var}=${RUSTFS_ROOT_USER_VALUE}"
		ok "${derived_var}"
	done

	for derived_var in STORAGE_SECRET_KEY AWS_SECRET_ACCESS_KEY; do
		current_derived=""
		if env_get "${derived_var}" >/dev/null 2>&1; then
			current_derived="$(env_get "${derived_var}")"
		fi
		if [[ "${current_derived}" != "${RUSTFS_ROOT_PASSWORD_VALUE}" ]]; then
			env_update_or_set "${derived_var}" "${RUSTFS_ROOT_PASSWORD_VALUE}"
			warn "${derived_var} did not match RUSTFS_ROOT_PASSWORD — updated in .env"
			WEAK_COUNT=$((WEAK_COUNT + 1))
		fi
		export "${derived_var}=${RUSTFS_ROOT_PASSWORD_VALUE}"
		ok "${derived_var}"
	done
fi

# ---------------------------------------------------------------------------
# Auto-construct DATABASE_URL if empty
# ---------------------------------------------------------------------------

db_url=""
if env_get "DATABASE_URL" >/dev/null 2>&1; then
	db_url="$(env_get "DATABASE_URL")"
fi
if [[ -z "${db_url}" ]]; then
	env_set "DATABASE_URL" "postgres://rustshare:${POSTGRES_PASSWORD}@postgres:5432/rustshare"
	warn "DATABASE_URL was empty — auto-constructed from POSTGRES_PASSWORD"
fi

# ---------------------------------------------------------------------------
# Check required non-secrets
# ---------------------------------------------------------------------------

for var_name in "${REQUIRED_NON_SECRETS[@]}"; do
	if env_get "${var_name}" >/dev/null 2>&1; then
		value="$(env_get "${var_name}")"
		if [[ -n "${value}" ]]; then
			ok "${var_name}"
			export "${var_name}=${value}"
		else
			warn "${var_name} is present but empty"
		fi
	else
		warn "${var_name} is missing from .env"
	fi
done

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

TOTAL_ISSUES=$((GENERATED_COUNT + WEAK_COUNT))

echo
echo "─────────────────────────────────────────"
echo "Pre-flight Summary"
echo "─────────────────────────────────────────"
echo "Secrets OK:              ${OK_COUNT}"
echo "Secrets generated:       ${GENERATED_COUNT}"
echo "Secrets regenerated:     ${WEAK_COUNT}"
echo "Total issues:            ${TOTAL_ISSUES}"
echo

# ---------------------------------------------------------------------------
# Admin bootstrap warning
# ---------------------------------------------------------------------------

echo
warn "Admin password is NOT stored in .env"
echo "  On first boot, the backend will generate a random admin password"
echo "  and print it to the container logs. Retrieve it with:"
echo "    docker logs rustshare-backend-1 | grep 'Bootstrap admin password'"
echo "  Log in, then change the password immediately."
echo

if [[ "${TOTAL_ISSUES}" -gt 0 ]]; then
	info "New values have been appended to ${ENV_FILE}"
	info "A backup was saved to ${BACKUP_FILE}"
	info "Review the generated values, then run:"
	echo "    docker compose up -d"
else
	info "All required secrets are present and strong."
	info "You can now run:"
	echo "    docker compose up -d"
fi
