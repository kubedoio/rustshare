#!/usr/bin/env bash
# scripts/start-buzz-observer.sh
# Host-side start of the relay -> Elembra observation bridge for the Alpha
# stack. The bridge must connect to the relay through the SAME host the
# browsers use (community is resolved from the connection host, ADR-0034), so
# it runs on the host rather than inside the Compose network.
#
# Prerequisites: .env populated (scripts/pre-flight.sh + the BUZZ_* chat keys,
# see .env.example) and `npm install` run in frontend/ (for @noble/curves).
#
# Usage:
#   ./scripts/start-buzz-observer.sh
# Run in the foreground (Ctrl-C stops it); or background it with a supervisor
# for the dogfooding period:
#   nohup ./scripts/start-buzz-observer.sh >> /var/log/buzz-observer.log 2>&1 &
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

if [[ ! -f .env ]]; then
	echo "Missing .env. Copy .env.example, run scripts/pre-flight.sh, add the" >&2
	echo "BUZZ_* chat keys (see .env.example), and retry." >&2
	exit 1
fi

# .env is developer-controlled operator config and already the Compose source
# of truth. Never print its values.
set -a
# shellcheck disable=SC1091
. ./.env
set +a

: "${BUZZ_RELAY_WS:?BUZZ_RELAY_WS must be set in .env (default ws://localhost:7447)}"
: "${RUSTSHARE_CHAT_WEBHOOK_SECRET:?RUSTSHARE_CHAT_WEBHOOK_SECRET must be set in .env}"
: "${BUZZ_SERVICE_SK:?BUZZ_SERVICE_SK must be set in .env (run frontend/scripts/alpha-gen-buzz-keys.mjs)}"
: "${BUZZ_COMMUNITY_ID:?BUZZ_COMMUNITY_ID must be set in .env}"

if [[ ! -d frontend/node_modules/@noble ]]; then
	echo "frontend dependencies missing: run 'npm install' in frontend/ first." >&2
	exit 1
fi

cd frontend
# Supervise: the observation bridge must not stay down after a crash. Restart
# with a short delay. Stop cleanly on SIGINT/SIGTERM: bash defers traps while
# a foreground child runs, so run the child in the background + `wait` (which
# IS interruptible) and forward the signal to the child in the trap.
child_pid=""
status=0
trap 'echo "buzz-observer supervisor: stopping"; [[ -n "$child_pid" ]] && kill "$child_pid" 2>/dev/null; exit 0' INT TERM
while true; do
	node scripts/buzz-observer.mjs &
	child_pid=$!
	# `set -e` would abort the supervisor on a crashing child; `|| status=$?`
	# captures the exit status without letting it terminate the loop.
	wait "$child_pid" || status=$?
	child_pid=""
	echo "buzz-observer exited (status $status); restarting in 3s…" >&2
	sleep 3
done
