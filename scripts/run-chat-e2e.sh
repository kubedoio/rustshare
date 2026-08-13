#!/usr/bin/env bash
# scripts/run-chat-e2e.sh
# Real Buzz relay probe for the Chat v1 publish path: signs a kind-1 event and
# publishes it over NIP-42 against an operator-supplied disposable relay
# (ADR-0034 recipe), plus a MANUAL revocation checklist — the 9030/9031 relay
# orchestration needs the relay's own admin CLI, so revocation is not
# automated here (the Elembra-side read-gate half is automated in
# backend/tests/chat_app_read_test.rs).
# Required env:
#   BUZZ_RELAY_IMAGE   docker image of the Buzz relay (e.g. ghcr.io/.../buzz-relay:main)
#   BUZZ_RELAY_WS      wss:// or ws:// URL of the started relay
#   BUZZ_SERVICE_SK    hex service/bridge key with relay admin authority
# Optional:
#   BUZZ_RELAY_PORT    host port mapped to the relay's 7447 (default 7447)
#   ELEMBRA_API        Elembra API base (default http://localhost:8080/api/v1)
#   ADMIN_EMAIL / ADMIN_PASSWORD   for the Elembra-side binding steps
set -euo pipefail

# Run from anywhere: the probe lives under frontend/ relative to the repo root.
cd "$(dirname "$0")/.."

: "${BUZZ_RELAY_IMAGE:?set BUZZ_RELAY_IMAGE to the Buzz relay docker image}"
: "${BUZZ_RELAY_WS:?set BUZZ_RELAY_WS to the relay websocket URL}"
: "${BUZZ_SERVICE_SK:?set BUZZ_SERVICE_SK to a relay admin service secret key}"
ELEMBRA_API="${ELEMBRA_API:-http://localhost:8080/api/v1}"

echo "== 1. start disposable relay =="
# Clear a leftover container from a previous interrupted run (SIGKILL before
# the EXIT trap) so `docker run --name` does not fail.
docker rm -f rustshare-buzz-proof >/dev/null 2>&1 || true
docker run -d --rm --name rustshare-buzz-proof \
  -e BUZZ_REQUIRE_RELAY_MEMBERSHIP=true \
  -p "${BUZZ_RELAY_PORT:-7447}:7447" \
  "$BUZZ_RELAY_IMAGE" >/dev/null
trap 'docker stop rustshare-buzz-proof >/dev/null 2>&1 || true' EXIT

echo "== 2. publish a signed kind-1 event =="
# Retry while the relay is still booting: the probe exits 1 on any failure,
# and step 2 expects success.
for attempt in $(seq 1 15); do
  if node frontend/scripts/chat-relay-probe.mjs "$BUZZ_RELAY_WS" "$BUZZ_SERVICE_SK" "chat-e2e hello $(date +%s)"; then
    break
  fi
  echo "relay not ready (attempt $attempt/15), retrying in 2s…"
  sleep 2
  if [ "$attempt" = 15 ]; then
    echo "probe failed after 15 attempts; is the relay up at $BUZZ_RELAY_WS?" >&2
    exit 1
  fi
done

echo "== 3. revocation denial (relay-side) — MANUAL =="
echo "Publish accepted above; after the relay revokes the member (kind 9031),"
echo "re-run the probe with the same key — it must exit 1. Orchestrate the"
echo "9030/9031 admission/revocation with the relay's own admin CLI; Elembra's"
echo "bridge already emits these commands (backend/crates/server/src/buzz_bridge.rs)."
echo "This step is a manual checklist, not an automated assertion."

echo "== 4. Elembra-side read gate after revocation =="
echo "Covered by backend/tests/chat_app_read_test.rs (revoked_binding_denies_reads);"
echo "run it with DATABASE_URL set. See docs/superpowers/specs/2026-08-12-elembra-chat-app-v1-design.md §8."

echo "publish probe complete (revocation steps 3-4 are manual)"
