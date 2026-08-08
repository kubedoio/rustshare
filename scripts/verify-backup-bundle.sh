#!/usr/bin/env bash

set -euo pipefail

usage() {
	cat <<'EOF'
Usage: scripts/verify-backup-bundle.sh <backup_dir>

Validates the structure and integrity of a Rustshare backup bundle created by
scripts/backup-stack.sh.

Checks performed:
- required artifacts exist
- gzip/tar archives are readable
- manifest.env exists and contains key metadata
- SHA256SUMS matches when present

This command does not restore data. It is safe to run on any backup directory.
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" || $# -lt 1 ]]; then
	usage
	exit $(( $# < 1 ))
fi

BACKUP_DIR="$(cd "$1" && pwd)"

require_file() {
	local file="$1"
	if [[ ! -f "${file}" ]]; then
		echo "Missing required file: ${file}" >&2
		exit 1
	fi
}

require_file "${BACKUP_DIR}/postgres.sql.gz"
require_file "${BACKUP_DIR}/rustfs-data.tar.gz"
require_file "${BACKUP_DIR}/config.tar.gz"
require_file "${BACKUP_DIR}/manifest.env"

echo "Checking PostgreSQL dump..."
	gzip -t "${BACKUP_DIR}/postgres.sql.gz"

echo "Checking RustFS archive..."
	tar -tzf "${BACKUP_DIR}/rustfs-data.tar.gz" >/dev/null

echo "Checking configuration archive..."
	tar -tzf "${BACKUP_DIR}/config.tar.gz" >/dev/null

echo "Checking manifest..."
if ! grep -q '^BACKUP_TIMESTAMP=' "${BACKUP_DIR}/manifest.env"; then
	echo "manifest.env is missing BACKUP_TIMESTAMP" >&2
	exit 1
fi

if ! grep -q '^GIT_COMMIT=' "${BACKUP_DIR}/manifest.env"; then
	echo "manifest.env is missing GIT_COMMIT" >&2
	exit 1
fi

if [[ -f "${BACKUP_DIR}/SHA256SUMS" ]]; then
	echo "Verifying SHA256 checksums..."
	if command -v sha256sum >/dev/null 2>&1; then
		checksum_cmd=(sha256sum -c)
	elif command -v shasum >/dev/null 2>&1; then
		checksum_cmd=(shasum -a 256 -c)
	else
		echo "No checksum tool available; cannot verify SHA256SUMS" >&2
		exit 1
	fi
	(
		cd "${BACKUP_DIR}"
		"${checksum_cmd[@]}" SHA256SUMS
	) || {
		echo "Checksum verification FAILED" >&2
		exit 1
	}
fi

echo "Backup bundle is structurally valid: ${BACKUP_DIR}"
