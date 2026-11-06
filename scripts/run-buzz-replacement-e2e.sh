#!/usr/bin/env bash
# Debug-only acceptance, with no deployment .env or production endpoints.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
umask 077
[[ -z "${DOCKER_HOST:-}" || "$DOCKER_HOST" == unix://* ]] || { echo 'A local Docker socket is required' >&2; exit 1; }
[[ "$(docker context inspect --format '{{.Endpoints.docker.Host}}')" == unix://* ]] || { echo 'Remote Docker contexts are forbidden' >&2; exit 1; }
mkdir -p .agents/evidence
export REPLACEMENT_E2E_ARTIFACTS="${REPLACEMENT_E2E_ARTIFACTS:-$(mktemp -d "$REPO_ROOT/.agents/evidence/replacement.XXXXXX")}"
mkdir -p "$REPLACEMENT_E2E_ARTIFACTS"
export RUSTSHARE_TEST_BUZZ_SOURCE_DIR="${RUSTSHARE_TEST_BUZZ_SOURCE_DIR:-$REPO_ROOT/.agents/buzz-source}"
export RUSTSHARE_TEST_BUZZ_TARGET_DIR="${RUSTSHARE_TEST_BUZZ_TARGET_DIR:-$REPO_ROOT/.agents/buzz-target}"
if [[ ! -d "$RUSTSHARE_TEST_BUZZ_SOURCE_DIR" ]]; then
  git clone --no-checkout https://github.com/kubedoio/buzz.git "$RUSTSHARE_TEST_BUZZ_SOURCE_DIR" >"$REPLACEMENT_E2E_ARTIFACTS/checkout.log" 2>&1
  git -C "$RUSTSHARE_TEST_BUZZ_SOURCE_DIR" checkout --detach dc09fc2602a7cc64ea15caa865bd773d8be61eed >>"$REPLACEMENT_E2E_ARTIFACTS/checkout.log" 2>&1
fi
[[ "$(git -C "$RUSTSHARE_TEST_BUZZ_SOURCE_DIR" rev-parse HEAD)" == dc09fc2602a7cc64ea15caa865bd773d8be61eed ]]
[[ -z "$(git -C "$RUSTSHARE_TEST_BUZZ_SOURCE_DIR" status --porcelain --untracked-files=all)" ]]
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 SQLX_OFFLINE=true
(
  cd "$RUSTSHARE_TEST_BUZZ_SOURCE_DIR"
  unset RUSTUP_TOOLCHAIN
  CARGO_TARGET_DIR="$RUSTSHARE_TEST_BUZZ_TARGET_DIR" cargo build --locked -p buzz-relay
) >"$REPLACEMENT_E2E_ARTIFACTS/buzz-build.log" 2>&1
export RUSTSHARE_TEST_BUZZ_BINARY="$RUSTSHARE_TEST_BUZZ_TARGET_DIR/debug/buzz-relay"
export RUSTSHARE_TEST_BUZZ_BINARY_SHA256="$(sha256sum "$RUSTSHARE_TEST_BUZZ_BINARY" | cut -d ' ' -f 1)"
cargo build --locked -p rustshare-server --features test-buzz-replacement-qualification >"$REPLACEMENT_E2E_ARTIFACTS/elembra-build.log" 2>&1
export REPLACEMENT_E2E_BACKEND="${CARGO_TARGET_DIR:-$REPO_ROOT/target}/debug/rustshare-server"
npm --prefix frontend ci --ignore-scripts >"$REPLACEMENT_E2E_ARTIFACTS/npm.log" 2>&1
node frontend/scripts/buzz-replacement-e2e.mjs
