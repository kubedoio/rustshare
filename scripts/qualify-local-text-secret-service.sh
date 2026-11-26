#!/usr/bin/env bash
set -euo pipefail

worker="${1:-target/release/elembra-local-text-worker}"
test -x "$worker"

state="$(mktemp -d "${RUNNER_TEMP:-/tmp}/local-text-secret-service.XXXXXX")"
trap 'rm -rf "$state"' EXIT
chmod 700 "$state"
export HOME="$state/home"
export XDG_DATA_HOME="$HOME/data"
export XDG_CONFIG_HOME="$HOME/config"
export XDG_RUNTIME_DIR="$HOME/runtime"
export GNOME_KEYRING_CONTROL="$XDG_RUNTIME_DIR/keyring"
export WORKER_BIN="$(realpath "$worker")"
export INSTANCE_ID="00000000-0000-4000-8000-000000000099"
export ROOT="$state/root"
export QUEUE="$state/queue.sqlite"
export LEGACY="$state/credential"

mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_RUNTIME_DIR" "$ROOT"
chmod 700 "$HOME" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_RUNTIME_DIR"
printf 'synthetic local text fixture\n' > "$ROOT/note.txt"
printf 'ecw1_synthetic-local-text-token' > "$LEGACY"
chmod 600 "$LEGACY"

export SYNTHETIC_CREDENTIAL="$(cat "$LEGACY")"

dbus-run-session -- bash --noprofile --norc -euo pipefail -c '
  printf "synthetic-keyring-pass" | gnome-keyring-daemon --unlock --components=secrets --control-directory="$GNOME_KEYRING_CONTROL" >/dev/null

  run_worker() {
    local status
    set +e
    timeout --signal=TERM --kill-after=1 2 "$WORKER_BIN" \
      --instance-id "$INSTANCE_ID" \
      --server http://127.0.0.1:59999 \
      "$@" \
      --root "$ROOT" --queue "$QUEUE" --reconcile-seconds 1 > "$HOME/worker.log" 2>&1
    status=$?
    set -e
    if [[ $status -ne 124 ]]; then
      cat "$HOME/worker.log"
      echo "worker did not remain running (exit $status)" >&2
      return 1
    fi
    if ! grep -Eq "^Scanned [0-9]+ eligible observations, uploaded [0-9]+, excluded [0-9]+$" "$HOME/worker.log"; then
      cat "$HOME/worker.log"
      echo "worker did not complete a scan after reading its credential" >&2
      return 1
    fi
  }

  run_worker --migrate-credential-file "$LEGACY"
  [[ ! -e "$LEGACY" ]]
  [[ "$(secret-tool lookup service io.elembra.local-text-worker username "$INSTANCE_ID")" == "$SYNTHETIC_CREDENTIAL" ]]

  run_worker --credential-keyring

  secret-tool clear service io.elembra.local-text-worker username "$INSTANCE_ID"
  printf "%s" ecw1_synthetic-local-text-rotated | secret-tool store --label="Elembra Local Text CI" service io.elembra.local-text-worker username "$INSTANCE_ID" >/dev/null
  run_worker --credential-keyring

  printf "%s" "$SYNTHETIC_CREDENTIAL" > "$LEGACY"
  chmod 600 "$LEGACY"
  set +e
  "$WORKER_BIN" --instance-id "$INSTANCE_ID" \
    --server http://127.0.0.1:59999 \
    --migrate-credential-file "$LEGACY" \
    --root "$ROOT" --queue "$QUEUE" --reconcile-seconds 1 > "$HOME/migration.log" 2>&1
  status=$?
  set -e
  if [[ $status -eq 0 ]] || ! grep -q "refusing to replace it during migration" "$HOME/migration.log"; then
    cat "$HOME/migration.log"
    echo "migration unexpectedly replaced a rotated credential" >&2
    exit 1
  fi
  [[ -e "$LEGACY" ]]
  [[ "$(secret-tool lookup service io.elembra.local-text-worker username "$INSTANCE_ID")" == ecw1_synthetic-local-text-rotated ]]
  echo "Local Text Secret Service migration, lookup, restart and rotation checks passed"
'
