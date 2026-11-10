#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT
mkdir -p "${TMP_DIR}/bin"
cat >"${TMP_DIR}/bin/docker" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1 $2 $3" == 'buildx imagetools inspect' ]] || {
	printf 'unexpected docker command: %s\n' "$*" >>"${DOCKER_CALLS}"
	exit 91
}
printf '%s\n' "$4" >>"${DOCKER_CALLS}"
case "$4" in
	ghcr.io/kubedoio/rustshare-backend:qualified-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef)
		digest=sha256:1111111111111111111111111111111111111111111111111111111111111111 ;;
	ghcr.io/kubedoio/rustshare-chat-observer:qualified-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef)
		digest=sha256:2222222222222222222222222222222222222222222222222222222222222222 ;;
	*) printf 'registry lookup failed for %s\n' "$4" >&2; exit 92 ;;
esac
printf 'Name: %s\nMediaType: application/vnd.oci.image.index.v1+json\nDigest: %s\n' "$4" "$digest"
# Ensure the resolver consumes the complete command output under pipefail.
for ((i = 0; i < 20000; i++)); do printf 'Manifest detail %d\n' "$i"; done
MOCK
chmod +x "${TMP_DIR}/bin/docker"
export PATH="${TMP_DIR}/bin:${PATH}"
export DOCKER_CALLS="${TMP_DIR}/docker-calls"
commit=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
output="$("${ROOT}/scripts/resolve-qualified-image-digests.sh" \
	"ghcr.io/kubedoio/rustshare-backend:qualified-${commit}" \
	"ghcr.io/kubedoio/rustshare-chat-observer:qualified-${commit}")"
[[ "${output}" == $'backend_digest=sha256:1111111111111111111111111111111111111111111111111111111111111111\nobserver_digest=sha256:2222222222222222222222222222222222222222222222222222222222222222' ]]
[[ "$(cat "${DOCKER_CALLS}")" == $'ghcr.io/kubedoio/rustshare-backend:qualified-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nghcr.io/kubedoio/rustshare-chat-observer:qualified-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef' ]]
if "${ROOT}/scripts/resolve-qualified-image-digests.sh" \
	"ghcr.io/kubedoio/rustshare-backend:qualified-${commit}" \
	"ghcr.io/kubedoio/rustshare-chat-observer:qualified-wrong-commit" \
	>"${TMP_DIR}/unexpected-output" 2>&1; then
	echo 'Resolver accepted a failed observer lookup.' >&2
	exit 1
fi
echo 'Qualified digest resolution contract passed.'
