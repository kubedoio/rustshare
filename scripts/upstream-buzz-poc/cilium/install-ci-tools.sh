#!/usr/bin/env bash
set -Eeuo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/versions.env"
DEST="${1:-/usr/local/bin}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] || {
	echo 'Cilium CI tools require Linux amd64' >&2
	exit 2
}

download() {
	local url="$1" path="$2" expected="$3"
	curl --fail --silent --show-error --location --retry 3 --retry-all-errors \
		--connect-timeout 20 --output "$path" "$url"
	printf '%s  %s\n' "$expected" "$path" | sha256sum --check --status || {
		echo "download checksum mismatch: $(basename "$path")" >&2
		exit 1
	}
}

download "https://kind.sigs.k8s.io/dl/$KIND_VERSION/kind-linux-amd64" \
	"$TMP/kind" "$KIND_SHA256"
download "https://dl.k8s.io/release/$KUBERNETES_VERSION/bin/linux/amd64/kubectl" \
	"$TMP/kubectl" "$KUBECTL_SHA256"
download "https://github.com/cilium/cilium-cli/releases/download/$CILIUM_CLI_VERSION/cilium-linux-amd64.tar.gz" \
	"$TMP/cilium.tar.gz" "$CILIUM_CLI_SHA256"
download "https://github.com/cilium/hubble/releases/download/v$HUBBLE_CLI_VERSION/hubble-linux-amd64.tar.gz" \
	"$TMP/hubble.tar.gz" "$HUBBLE_CLI_SHA256"

tar -xzf "$TMP/cilium.tar.gz" -C "$TMP" cilium
tar -xzf "$TMP/hubble.tar.gz" -C "$TMP" hubble
install -d "$DEST"
install -m 0755 "$TMP/kind" "$TMP/kubectl" "$TMP/cilium" "$TMP/hubble" "$DEST/"
export PATH="$DEST:$PATH"

kind version | grep -F "$KIND_VERSION" >/dev/null
kubectl version --client -o yaml | grep -F "$KUBERNETES_VERSION" >/dev/null
cilium version | grep -F "$CILIUM_CLI_VERSION" >/dev/null
hubble version | grep -F "$HUBBLE_CLI_VERSION" >/dev/null
echo 'pinned kind, kubectl, Cilium CLI, and Hubble CLI installed and verified'
