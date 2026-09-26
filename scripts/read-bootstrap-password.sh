#!/usr/bin/env bash
set -euo pipefail

container_id="${1:?container id is required}"
password_file="${2:-/tmp/rustshare-bootstrap-password.txt}"
tmp_file="$(mktemp)"
trap 'rm -f "${tmp_file}"' EXIT

docker cp "${container_id}:${password_file}" "${tmp_file}" >/dev/null
tr -d '\r\n' <"${tmp_file}"
