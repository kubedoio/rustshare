#!/usr/bin/env bash
# scripts/run-buzz-conformance.sh
# =============================================================================
# LIVE Buzz conformance run (proofs 1-10 of the Elembra production-authority
# goal): builds the relay image from the v1alpha1 worktree, brings up the
# relay stack with the Elembra service key trusted for the v1alpha1
# authorization API, and runs backend/tests/buzz_live_conformance_test.rs
# against the REAL relay (in-process Elembra, dev DB).
#
# The suite seeds the relay itself over its public HTTP surface
# (POST /events) and ingests the same signed events through the in-process
# observation bridge; this script only orchestrates the stack + keys.
#
# Prerequisites:
#   - docker (relay image build + stack)
#   - the dev Elembra DB up (backend/.env DATABASE_URL, e.g. docker compose
#     up -d postgres)
#   - frontend node deps for keygen (npm install in frontend/) — only when
#     BUZZ_SERVICE_SK / BUZZ_RELAY_PRIVATE_KEY are not already set in .env
#
# Required env (or generated):
#   BUZZ_SERVICE_SK        Elembra service / relay-owner secret key (64 hex)
#   BUZZ_RELAY_PRIVATE_KEY the relay's own identity secret key (64 hex)
# Optional:
#   BUZZ_RELAY_URL         relay ws url (default ws://127.0.0.1:7447)
#   BUZZ_RELAY_IMAGE       relay docker image (default buzz-relay:conformance;
#                          built from .worktrees/buzz when missing)
#   BUZZ_RELAY_WORKTREE    relay worktree path (default .worktrees/buzz)
#   RUSTSHARE_BUZZ_CONFORMANCE_KEEP=1  leave the relay stack running on exit
#
# Exit code: 0 when every proof passes; 1 otherwise.
# =============================================================================
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

if [[ -f .env ]]; then
	set -a
	# shellcheck disable=SC1091
	. ./.env
	set +a
fi

BUZZ_RELAY_URL="${BUZZ_RELAY_URL:-ws://127.0.0.1:7447}"
BUZZ_RELAY_IMAGE="${BUZZ_RELAY_IMAGE:-buzz-relay:conformance}"
BUZZ_RELAY_WORKTREE="${BUZZ_RELAY_WORKTREE:-${REPO_ROOT}/.worktrees/buzz}"
COMPOSE_FILES="-f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.conformance.yml"
PASS=0
FAIL=0
declare -a RESULTS=()

# --- keys -------------------------------------------------------------------
if [[ -z "${BUZZ_SERVICE_SK:-}" || -z "${BUZZ_RELAY_PRIVATE_KEY:-}" ]]; then
	echo "== generating Buzz keys (alpha-gen-buzz-keys.mjs) =="
	if [[ ! -d frontend/node_modules ]]; then
		echo "FAIL: frontend/node_modules missing (npm install) — or set BUZZ_SERVICE_SK and BUZZ_RELAY_PRIVATE_KEY in .env" >&2
		exit 1
	fi
	KEYS_OUT="$(cd frontend && node scripts/alpha-gen-buzz-keys.mjs)"
	BUZZ_SERVICE_SK="$(printf '%s\n' "$KEYS_OUT" | sed -n 's/^BUZZ_SERVICE_SK=//p')"
	BUZZ_RELAY_PRIVATE_KEY="$(printf '%s\n' "$KEYS_OUT" | sed -n 's/^BUZZ_RELAY_PRIVATE_KEY=//p')"
fi
: "${BUZZ_SERVICE_SK:?BUZZ_SERVICE_SK must be set}"
: "${BUZZ_RELAY_PRIVATE_KEY:?BUZZ_RELAY_PRIVATE_KEY must be set}"
BUZZ_SERVICE_PK="$(cd frontend && node scripts/alpha-buzz-ops.mjs pubkey "$BUZZ_SERVICE_SK")"
BUZZ_RELAY_PUBKEY="$(cd frontend && node scripts/alpha-buzz-ops.mjs pubkey "$BUZZ_RELAY_PRIVATE_KEY")"
echo "  service pubkey: ${BUZZ_SERVICE_PK:0:16}…  relay pubkey: ${BUZZ_RELAY_PUBKEY:0:16}…"

# Export the compose interpolation vars up front: the alpha compose uses
# `:?`-required vars (BUZZ_RELAY_OWNER_PUBKEY / BUZZ_RELAY_PRIVATE_KEY), so
# EVERY compose invocation (ps/up/stop) needs them or config validation fails.
export BUZZ_RELAY_IMAGE BUZZ_RELAY_URL
export RELAY_TRUSTED_SERVICE_PUBKEYS="$BUZZ_SERVICE_PK"
export BUZZ_RELAY_OWNER_PUBKEY="$BUZZ_SERVICE_PK"
export BUZZ_RELAY_PRIVATE_KEY

# --- relay image ------------------------------------------------------------
if ! docker image inspect "$BUZZ_RELAY_IMAGE" >/dev/null 2>&1; then
	echo "== building relay image $BUZZ_RELAY_IMAGE from $BUZZ_RELAY_WORKTREE =="
	docker build -t "$BUZZ_RELAY_IMAGE" "$BUZZ_RELAY_WORKTREE"
else
	echo "== relay image $BUZZ_RELAY_IMAGE present (no rebuild) =="
fi

# --- stack ------------------------------------------------------------------
STARTED_RELAY=0
# Only a RUNNING relay counts as "already up" (plain `docker ps`: compose's
# ps -q lists stopped containers too and depends on env interpolation).
if ! docker ps --format '{{.Names}}' | grep -qx 'rustshare-buzz-relay-1'; then
	STARTED_RELAY=1
fi
echo "== bringing up the relay stack =="
# Fail fast with a clear message when a non-compose container holds the
# relay ports (e.g. a leftover scratch relay) — the port bind would fail
# mid-start otherwise.
for port in 7447 8088 9102; do
	if docker ps --format '{{.Names}} {{.Ports}}' | grep -q ":$port->" 2>/dev/null \
		&& ! docker ps --format '{{.Names}}' | grep -qx 'rustshare-buzz-relay-1'; then
		echo "FAIL: port $port is held by another container (docker ps | grep ':$port->'); stop it or set RUSTSHARE_BUZZ_CONFORMANCE_KEEP=1 after a manual cleanup" >&2
		exit 1
	fi
done
BUZZ_RELAY_IMAGE="$BUZZ_RELAY_IMAGE" \
	docker compose $COMPOSE_FILES up -d buzz-relay

teardown() {
	if [[ "${RUSTSHARE_BUZZ_CONFORMANCE_KEEP:-0}" == "1" ]]; then
		echo "== keeping the relay stack (RUSTSHARE_BUZZ_CONFORMANCE_KEEP=1) =="
	elif [[ "$STARTED_RELAY" == "1" ]]; then
		echo "== stopping the relay stack =="
		docker stop rustshare-buzz-relay-1 >/dev/null 2>&1 || true
	fi
}
trap teardown EXIT

echo "== waiting for relay health =="
# The app router serves `/health` on the main relay port (7447); the 8088
# health listener uses a separate probe surface.
for attempt in $(seq 1 30); do
	if curl -sf "http://127.0.0.1:7447/health" >/dev/null 2>&1; then
		break
	fi
	echo "  relay not healthy (attempt $attempt/30)…"
	sleep 2
	if [[ "$attempt" == "30" ]]; then
		echo "FAIL: relay did not become healthy at http://127.0.0.1:7447/health" >&2
		exit 1
	fi
done
echo "  relay healthy"

# --- suite ------------------------------------------------------------------
echo "== running the live conformance suite =="
export RUSTSHARE_BUZZ_LIVE_RELAY_URL="$BUZZ_RELAY_URL"
export RUSTSHARE_BUZZ_LIVE_SERVICE_SK="$BUZZ_SERVICE_SK"
export RUSTSHARE_BUZZ_LIVE_RELAY_PUBKEY="$BUZZ_RELAY_PUBKEY"
export RUSTSHARE_BUZZ_LIVE_METRICS_URL="${RUSTSHARE_BUZZ_LIVE_METRICS_URL:-http://127.0.0.1:9102}"
# The dev DB env (DATABASE_URL, S3/RustFS endpoints); optional — the suite's
# pool() falls back to localhost defaults, and CI provides the env directly.
if [[ -f backend/.env ]]; then
	set -a
	# shellcheck disable=SC1091
	. ./backend/.env
	set +a
fi
set +e
SQLX_OFFLINE=true cargo test -p rustshare-server --test buzz_live_conformance_test -- \
	--ignored --test-threads=1 ${RUSTSHARE_BUZZ_CONFORMANCE_TEST_FILTER:-} 2>&1 | tee /tmp/buzz-conformance.out
SUITE_EXIT="${PIPESTATUS[0]}"
set -e

echo "== summary =="
if [[ "$SUITE_EXIT" == "0" ]]; then
	PASS=1
	RESULTS+=("PASS  live conformance suite  all proofs green")
	echo "  [PASS] live conformance suite: all proofs green"
else
	FAIL=1
	RESULTS+=("FAIL  live conformance suite  exit $SUITE_EXIT (see /tmp/buzz-conformance.out)")
	echo "  [FAIL] live conformance suite: exit $SUITE_EXIT (see /tmp/buzz-conformance.out)"
fi

for result in "${RESULTS[@]}"; do
	printf '%s\n' "$result"
done
echo "PASS=$PASS FAIL=$FAIL"
[[ "$FAIL" == "0" ]]
