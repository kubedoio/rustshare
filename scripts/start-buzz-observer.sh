#!/usr/bin/env bash
# Compatibility shim for the former host-supervised observer.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
echo "start-buzz-observer.sh is retired; use ./scripts/elembra.sh up." >&2
exec "${ROOT}/scripts/elembra.sh" up "$@"
