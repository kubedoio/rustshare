#!/usr/bin/env bash
#
# setup-ci-secrets.sh
#
# Idempotently ensures CI_JWT_SECRET and CI_ENCRYPTION_KEY exist as
# GitHub Actions repository secrets. Safe to run multiple times.
#
# Requires: gh CLI (https://cli.github.com/) authenticated against
# the target repository.
#
# Usage:
#   ./scripts/setup-ci-secrets.sh
#

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

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

generate_secret() {
	openssl rand -base64 32
}

secret_exists() {
	local name="$1"
	gh secret list --json name --jq ".[] | select(.name == \"${name}\") | .name" | grep -qx "${name}"
}

# ---------------------------------------------------------------------------
# Preflight checks
# ---------------------------------------------------------------------------

require_command gh
require_command openssl

if ! gh auth status >/dev/null 2>&1; then
	echo "You are not authenticated with the GitHub CLI." >&2
	echo "Run: gh auth login" >&2
	exit 1
fi

# Verify we are inside a GitHub repo
if ! gh repo view --json nameWithOwner >/dev/null 2>&1; then
	echo "Could not determine GitHub repository from the current directory." >&2
	exit 1
fi

REPO_NAME="$(gh repo view --json nameWithOwner --jq '.nameWithOwner')"
info "Working with repository: ${REPO_NAME}"

# ---------------------------------------------------------------------------
# Ensure secrets
# ---------------------------------------------------------------------------

SECRETS_CREATED=0
SECRETS_EXISTING=0

ensure_secret() {
	local name="$1"
	if secret_exists "${name}"; then
		ok "${name} already set"
		SECRETS_EXISTING=$((SECRETS_EXISTING + 1))
	else
		local value
		value="$(generate_secret)"
		# Pipe the value so it never hits the shell history or logs
		printf '%s' "${value}" | gh secret set "${name}" --repo "${REPO_NAME}"
		ok "${name} created"
		SECRETS_CREATED=$((SECRETS_CREATED + 1))
	fi
}

ensure_secret "CI_JWT_SECRET"
ensure_secret "CI_ENCRYPTION_KEY"

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

echo
echo "─────────────────────────────────────────"
echo "CI Secret Setup Summary"
echo "─────────────────────────────────────────"
echo "Repository: ${REPO_NAME}"
echo "Secrets already present: ${SECRETS_EXISTING}"
echo "Secrets created:         ${SECRETS_CREATED}"
echo
if [[ "${SECRETS_CREATED}" -gt 0 ]]; then
	info "The pilot-release.yml workflow can now use these secrets."
fi
