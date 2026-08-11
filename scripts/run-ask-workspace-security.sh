#!/usr/bin/env bash
set -euo pipefail

# Run the DB-backed Ask Workspace / Unified Search security tests from the
# host using the same credentials as the local Compose stack.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

if [[ ! -f .env ]]; then
	echo "Missing .env. Copy .env.example, run scripts/pre-flight.sh, and retry." >&2
	exit 1
fi

# .env is a developer-controlled file and is already the Compose source of
# truth. Never print its values or create fallback credentials here.
set -a
# shellcheck disable=SC1091
. ./.env
set +a

: "${POSTGRES_PASSWORD:?POSTGRES_PASSWORD must be set in .env}"
: "${RUSTFS_ROOT_USER:?RUSTFS_ROOT_USER must be set in .env}"
: "${RUSTFS_ROOT_PASSWORD:?RUSTFS_ROOT_PASSWORD must be set in .env}"

if [[ -z "${DATABASE_URL:-}" ]]; then
	DATABASE_URL="postgres://rustshare:${POSTGRES_PASSWORD}@127.0.0.1:5432/rustshare"
else
	# Compose resolves `postgres`; host-run Cargo commands need loopback.
	DATABASE_URL="${DATABASE_URL/@postgres:/@127.0.0.1:}"
fi
export DATABASE_URL

# Force host-side endpoints and the credentials used by the Compose RustFS
# service; unrelated shell state must not silently select another environment.
export AWS_ACCESS_KEY_ID="${RUSTFS_ROOT_USER}"
export AWS_SECRET_ACCESS_KEY="${RUSTFS_ROOT_PASSWORD}"
export RUSTFS_ENDPOINT=http://127.0.0.1:9000
export RUSTFS_PUBLIC_ENDPOINT=http://127.0.0.1:9000
export RUSTFS_REGION=us-east-1
export RUSTFS_BUCKET=rustshare-files
export S3_ENDPOINT=http://127.0.0.1:9000
export S3_REGION=us-east-1
export S3_BUCKET=rustshare-files
export RUSTSHARE_CHAT_AUTHORITY=local

docker compose up -d postgres rustfs

wait_for() {
	local description="$1"
	shift
	for _ in {1..30}; do
		if "$@" >/dev/null 2>&1; then
			return 0
		fi
		sleep 1
	done
	echo "Timed out waiting for ${description}." >&2
	return 1
}

wait_for PostgreSQL docker compose exec -T postgres pg_isready -U rustshare -d rustshare
wait_for RustFS docker compose exec -T rustfs sh -c 'nc -z localhost 9000 && nc -z localhost 9001'

cargo sqlx migrate run --source backend/migrations >/dev/null
cargo sqlx prepare --workspace --check

echo "Running Ask Workspace security matrix (run 1/2)"
SQLX_OFFLINE=true cargo test -p rustshare-server \
	--test unified_search_test \
	--features test-recording-provider \
	-- --ignored --test-threads=1

echo "Running Ask Workspace security matrix (run 2/2)"
SQLX_OFFLINE=true cargo test -p rustshare-server \
	--test unified_search_test \
	--features test-recording-provider \
	-- --ignored --test-threads=1

echo "Ask Workspace security matrix passed twice."
