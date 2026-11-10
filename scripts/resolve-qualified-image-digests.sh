#!/usr/bin/env bash
set -euo pipefail

if (($# != 2)); then
	echo "Usage: $0 <backend-qualified-ref> <observer-qualified-ref>" >&2
	exit 2
fi

resolve_digest() {
	local ref="$1" inspect_output digest
	if ! inspect_output="$(docker buildx imagetools inspect "${ref}" 2>&1)"; then
		echo "::error::Unable to inspect qualified image ${ref}" >&2
		printf '%s\n' "${inspect_output}" >&2
		return 1
	fi
	digest="$(awk '$1 == "Digest:" { print $2; found++ } END { if (found != 1) exit 1 }' <<<"${inspect_output}")" || {
		echo "::error::Expected exactly one manifest digest for ${ref}" >&2
		return 1
	}
	if [[ ! "${digest}" =~ ^sha256:[0-9a-f]{64}$ ]]; then
		echo "::error::Invalid manifest digest for ${ref}" >&2
		return 1
	fi
	printf '%s' "${digest}"
}

backend_digest="$(resolve_digest "$1")"
observer_digest="$(resolve_digest "$2")"
printf 'backend_digest=%s\nobserver_digest=%s\n' "${backend_digest}" "${observer_digest}"
