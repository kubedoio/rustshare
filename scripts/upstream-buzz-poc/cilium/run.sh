#!/usr/bin/env bash
# Disposable real-transport qualification against unmodified upstream Buzz.
set -Eeuo pipefail
umask 077

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HARNESS="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$HARNESS/../.." && pwd)"
source "$HERE/versions.env"
SOURCE_SHA=eafd77e9f6e96052476bdd88be732c046fdb7ab3
SOURCE_TREE=04e3a40545bd3b23510ec32b4c8cccee2acef7c5
NAMESPACE=elembra-buzz-test
RUN_ID="${BUZZ_CILIUM_RUN_ID:-$(date -u +%Y%m%d%H%M%S)-$$}"
[[ "$RUN_ID" =~ ^[a-z0-9][a-z0-9-]{0,31}$ ]] || { echo 'BUZZ_CILIUM_RUN_ID must be a short lowercase tag-safe value' >&2; exit 2; }
CLUSTER="ec-${RUN_ID}"
BUZZ_IMAGE="local/elembra-cilium-buzz:${RUN_ID}"
POC_IMAGE="local/elembra-cilium-tools:${RUN_ID}"
RUNNER_IMAGE="local/elembra-cilium-runner:${RUN_ID}"
EVIDENCE_DIR="${EVIDENCE_DIR:-${TMPDIR:-/tmp}/elembra-cilium-evidence-${RUN_ID}}"
[[ ! -e "$EVIDENCE_DIR" ]] || { echo "evidence path already exists: $EVIDENCE_DIR" >&2; exit 2; }
mkdir -m 0700 "$EVIDENCE_DIR"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/elembra-cilium.${RUN_ID}.XXXXXX")"
KUBECONFIG="$RUN_DIR/kubeconfig"
K=(kubectl --kubeconfig "$KUBECONFIG")
CLUSTER_CREATED=0
PF_PID=
TCP_PID=
UDP_PID=
EGRESS_PID=
TEST_PID=

cleanup() {
	local status=$?
	trap - EXIT INT TERM
	for pid in "$TCP_PID" "$UDP_PID" "$EGRESS_PID" "$PF_PID" "$TEST_PID"; do
		if [[ -n "$pid" ]]; then kill -INT "$pid" >/dev/null 2>&1 || true; wait "$pid" 2>/dev/null || true; fi
	done
	if [[ "$CLUSTER_CREATED" == 1 ]]; then kind delete cluster --name "$CLUSTER" >>"$EVIDENCE_DIR/cleanup.log" 2>&1 || true; fi
	for image in "$BUZZ_IMAGE" "$POC_IMAGE" "$RUNNER_IMAGE"; do docker image rm "$image" >>"$EVIDENCE_DIR/cleanup.log" 2>&1 || true; done
	python3 - "$RUN_DIR" <<'PY'
from pathlib import Path
import shutil, sys
p = Path(sys.argv[1])
if p.exists(): shutil.rmtree(p)
PY
	python3 - "$EVIDENCE_DIR" <<'PY'
from pathlib import Path
import hashlib, sys
p = Path(sys.argv[1])
manifest = p / "SHA256SUMS"
files = sorted(item for item in p.iterdir() if item.is_file() and item != manifest)
manifest.write_text("".join(f"{hashlib.sha256(item.read_bytes()).hexdigest()}  {item.name}\n" for item in files))
manifest.chmod(0o600)
PY
	echo "sanitized_evidence=$EVIDENCE_DIR"
	exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

usage() {
	cat <<'HELP'
Usage: scripts/upstream-buzz-poc/cilium/run.sh [--preflight-only]

Runs six ignored Rust trusted-service tests in a new kind/Cilium cluster. It
never connects to host Compose applications. The cluster, generated test keys,
and unique local image tags are removed on exit; sanitized evidence remains
under EVIDENCE_DIR or /tmp.
HELP
}
PREFLIGHT_ONLY=0
if [[ "${1:-}" == --help ]]; then usage; exit 0; fi
if [[ "${1:-}" == --preflight-only ]]; then PREFLIGHT_ONLY=1; shift; fi
[[ $# == 0 ]] || { usage >&2; exit 2; }

require() { command -v "$1" >/dev/null 2>&1 || { echo "missing required command: $1" >&2; exit 2; }; }
for command in git docker kind kubectl cilium hubble cargo rustc node npm openssl python3 sha256sum timeout; do require "$command"; done
[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] || { echo 'qualified platform is linux/amd64' >&2; exit 2; }
docker info >/dev/null 2>&1 || { echo 'Docker daemon unavailable' >&2; exit 2; }
docker buildx version >/dev/null 2>&1 || { echo 'Docker Buildx unavailable' >&2; exit 2; }
kind_version="$(kind version)"
[[ "$kind_version" == *"$KIND_VERSION"* ]] || { echo "kind must be $KIND_VERSION" >&2; exit 2; }
kubectl_version="$(kubectl version --client -o yaml)"
[[ "$kubectl_version" == *"$KUBERNETES_VERSION"* ]] || { echo "kubectl must be $KUBERNETES_VERSION" >&2; exit 2; }
cilium_version="$(cilium version)"
[[ "$cilium_version" == *"$CILIUM_CLI_VERSION"* ]] || { echo "Cilium CLI must be $CILIUM_CLI_VERSION" >&2; exit 2; }
[[ "$cilium_version" == *"$CILIUM_VERSION"* ]] || { echo "Cilium image must be $CILIUM_VERSION" >&2; exit 2; }
hubble_version="$(hubble version)"
[[ "$hubble_version" == *"$HUBBLE_CLI_VERSION"* ]] || { echo "Hubble CLI must be $HUBBLE_CLI_VERSION" >&2; exit 2; }
rustc_version="$(rustc +1.97.1 --version)"
[[ "$rustc_version" == *1.97.1* ]] || { echo 'Rust 1.97.1 is required' >&2; exit 2; }
[[ "$(node --version)" == "v$NODE_VERSION" ]] || { echo "Node.js $NODE_VERSION is required" >&2; exit 2; }
npm --version | awk -F. '$1 >= 10 { ok=1 } END { exit !ok }' || { echo 'npm 10 or newer is required' >&2; exit 2; }

default_source="${BUZZ_SOURCE_DIR:-$RUN_DIR/buzz-source}"
if [[ -n "${BUZZ_SOURCE_DIR:-}" ]]; then
	git -C "$default_source" rev-parse --git-dir >/dev/null 2>&1 || { echo 'BUZZ_SOURCE_DIR must be a git checkout' >&2; exit 2; }
else
	git clone --quiet https://github.com/block/buzz.git "$default_source"
	git -C "$default_source" checkout --quiet --detach "$SOURCE_SHA"
fi
[[ "$(git -C "$default_source" rev-parse HEAD)" == "$SOURCE_SHA" ]] || { echo 'Buzz source commit mismatch' >&2; exit 2; }
[[ "$(git -C "$default_source" rev-parse HEAD^{tree})" == "$SOURCE_TREE" ]] || { echo 'Buzz source tree mismatch' >&2; exit 2; }
[[ -z "$(git -C "$default_source" status --porcelain)" && -z "$(git -C "$default_source" clean -ndX)" ]] || { echo 'Buzz checkout must be clean, including ignored files' >&2; exit 2; }
[[ -z "$(git -C "$ROOT" status --porcelain)" ]] || { echo 'RustShare worktree must be clean to bind test evidence to a commit' >&2; exit 2; }
git -C "$ROOT" rev-parse HEAD >"$EVIDENCE_DIR/rustshare-source.txt"
cp "$HERE/versions.env" "$EVIDENCE_DIR/tool-image-pins.env"
{
	echo "buzz_source=$SOURCE_SHA"
	echo "buzz_tree=$SOURCE_TREE"
	echo "kind=$(kind version | head -1)"
	echo "kubectl=$(kubectl version --client -o json | python3 -c 'import json,sys; print(json.load(sys.stdin)["clientVersion"]["gitVersion"])')"
	echo "cilium=$(cilium version | head -1)"
	echo "hubble=$(hubble version | head -1)"
	echo "rust_version=$rustc_version"
	echo "node=$(node --version)"
	echo "npm=$(npm --version)"
} >"$EVIDENCE_DIR/tool-versions.txt"
if [[ "$PREFLIGHT_ONLY" == 1 ]]; then echo 'preflight=PASS; no cluster created'; exit 0; fi

if kind get clusters | grep -Fxq "$CLUSTER"; then echo "refusing to reuse kind cluster $CLUSTER" >&2; exit 2; fi
for image in "$BUZZ_IMAGE" "$POC_IMAGE" "$RUNNER_IMAGE"; do
	if docker image inspect "$image" >/dev/null 2>&1; then echo "refusing to reuse image tag $image" >&2; exit 2; fi
done

docker buildx build --platform linux/amd64 --load --quiet --metadata-file "$RUN_DIR/buzz-build.json" \
	--build-arg "BUZZ_SOURCE_SHA=$SOURCE_SHA" --tag "$BUZZ_IMAGE" "$default_source" >"$RUN_DIR/buzz-build.log" 2>&1 || {
	echo 'unmodified upstream Buzz image build failed' >&2; tail -n 30 "$RUN_DIR/buzz-build.log" >&2; exit 1;
}
docker buildx build --platform linux/amd64 --load --quiet --build-arg "NODE_BASE=$NODE_IMAGE" \
	--tag "$POC_IMAGE" -f "$HERE/Dockerfile.poc" "$HARNESS" >"$RUN_DIR/poc-build.log" 2>&1 || {
	echo 'disposable Node fixture image build failed' >&2; tail -n 30 "$RUN_DIR/poc-build.log" >&2; exit 1;
}

(
	cd "$ROOT"
	SQLX_OFFLINE=true cargo test -p rustshare-server --features upstream-buzz-runtime --lib --no-run --message-format=json
) >"$RUN_DIR/cargo-build.json" 2>"$RUN_DIR/cargo-build.stderr" || {
	echo 'feature-enabled Rust conformance test build failed' >&2; tail -n 40 "$RUN_DIR/cargo-build.stderr" >&2; exit 1;
}
python3 - "$RUN_DIR/cargo-build.json" "$RUN_DIR/test-runner" <<'PY'
import json, shutil, sys
artifacts = []
for line in open(sys.argv[1], encoding="utf-8"):
    try: item = json.loads(line)
    except json.JSONDecodeError: continue
    target = item.get("target", {})
    if item.get("reason") == "compiler-artifact" and target.get("name") == "rustshare_server" and target.get("test") and item.get("executable"):
        artifacts.append(item["executable"])
if len(artifacts) != 1: raise SystemExit(f"expected one server test binary, found {len(artifacts)}")
shutil.copyfile(artifacts[0], sys.argv[2])
PY
chmod 0555 "$RUN_DIR/test-runner"
sha256sum "$RUN_DIR/test-runner" >"$EVIDENCE_DIR/test-binary.sha256"
mkdir -m 0700 "$RUN_DIR/runner-context"
cp "$HERE/Dockerfile.runner" "$RUN_DIR/runner-context/Dockerfile"
cp "$RUN_DIR/test-runner" "$RUN_DIR/runner-context/test-runner"
docker buildx build --platform linux/amd64 --load --quiet --build-arg "RUNNER_BASE=$RUST_RUNNER_BASE" \
	--tag "$RUNNER_IMAGE" "$RUN_DIR/runner-context" >"$RUN_DIR/runner-build.log" 2>&1 || {
	echo 'Rust test runner image build failed' >&2; tail -n 30 "$RUN_DIR/runner-build.log" >&2; exit 1;
}
BUZZ_IMAGE_ID="$(docker image inspect --format '{{.Id}}' "$BUZZ_IMAGE")"
POC_IMAGE_ID="$(docker image inspect --format '{{.Id}}' "$POC_IMAGE")"
RUNNER_IMAGE_ID="$(docker image inspect --format '{{.Id}}' "$RUNNER_IMAGE")"
BUZZ_BUILD_DIGEST="$(python3 - "$RUN_DIR/buzz-build.json" <<'PY'
import json, sys
try: print(json.load(open(sys.argv[1], encoding="utf-8")).get("containerimage.digest", "unavailable"))
except Exception: print("unavailable")
PY
)"
{
	echo "buzz_image=$BUZZ_IMAGE_ID"
	echo "buzz_build_digest=$BUZZ_BUILD_DIGEST"
	echo "poc_image=$POC_IMAGE_ID"
	echo "runner_image=$RUNNER_IMAGE_ID"
	echo "buzz_source=$SOURCE_SHA"
	echo "buzz_tree=$SOURCE_TREE"
} >"$EVIDENCE_DIR/image-identities.txt"

CLUSTER_CREATED=1
kind create cluster --name "$CLUSTER" --image "$KIND_NODE_IMAGE" --config "$HERE/kind.yaml" \
	--kubeconfig "$KUBECONFIG" >"$EVIDENCE_DIR/kind-create.log" 2>&1
export KUBECONFIG
cilium install --version "$CILIUM_VERSION" --set kubeProxyReplacement=false --set ipam.mode=kubernetes \
	--set routingMode=tunnel --set hubble.enabled=true --set hubble.relay.enabled=true --set hubble.ui.enabled=false \
	>"$EVIDENCE_DIR/cilium-install.log" 2>&1
cilium status --wait >"$EVIDENCE_DIR/cilium-status.txt" 2>&1
kind load docker-image --name "$CLUSTER" "$BUZZ_IMAGE" "$POC_IMAGE" "$RUNNER_IMAGE" >"$EVIDENCE_DIR/image-load.log" 2>&1

mkdir -m 0700 "$RUN_DIR/test-deps"
cp "$HARNESS/package.json" "$HARNESS/package-lock.json" "$RUN_DIR/test-deps/"
npm ci --prefix "$RUN_DIR/test-deps" --ignore-scripts --no-audit --no-fund >"$RUN_DIR/npm.log" 2>&1 || {
	echo 'locked Node dependencies could not be installed' >&2; tail -n 20 "$RUN_DIR/npm.log" >&2; exit 1;
}
python3 - "$RUN_DIR/private" <<'PY'
from pathlib import Path
import json, secrets, sys
p = Path(sys.argv[1]); p.mkdir(mode=0o700)
keys = {f"{role}_{tenant}": secrets.token_hex(32) for tenant in ("a", "b") for role in ("owner", "operator", "relay", "member", "friend")}
f = p / "keys.json"; f.write_text(json.dumps(keys) + "\n"); f.chmod(0o600)
PY
node "$HERE/derive-keys.mjs" "$RUN_DIR/test-deps/package.json" "$RUN_DIR/private/keys.json" \
	"$RUN_DIR/private/public.json" "$RUN_DIR/private/connector-credentials.env"
python3 - "$RUN_DIR/private" "$RUN_DIR/secrets" "$NAMESPACE" <<'PY'
from pathlib import Path
import json, secrets, sys
p, out, ns = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
out.mkdir(mode=0o700)
keys = json.loads((p / "keys.json").read_text())
pub = json.loads((p / "public.json").read_text())
for tenant in ("a", "b"):
    values = {
        "POSTGRES_PASSWORD": secrets.token_hex(32), "REDIS_PASSWORD": secrets.token_hex(32),
        "BUZZ_S3_ACCESS_KEY": "poc" + secrets.token_hex(12), "BUZZ_S3_SECRET_KEY": secrets.token_hex(32),
        "BUZZ_GIT_HOOK_HMAC_SECRET": secrets.token_hex(32), "BUZZ_RELAY_PRIVATE_KEY": keys[f"relay_{tenant}"],
        "RELAY_OWNER_PUBKEY": pub[f"owner_{tenant}_pubkey"], "OPERATOR_PUBKEY": pub[f"operator_{tenant}_pubkey"],
    }
    f = out / f"buzz-{tenant}.env"; f.write_text("".join(f"{k}={v}\n" for k,v in values.items())); f.chmod(0o600)
seed = {f"{role.upper()}_{tenant.upper()}_SK": keys[f"{role}_{tenant}"] for tenant in ("a", "b") for role in ("owner", "operator", "member", "friend")}
f = out / "seed.env"; f.write_text("".join(f"{k}={v}\n" for k,v in seed.items())); f.chmod(0o600)
dbpass = secrets.token_hex(32)
creds = (p / "connector-credentials.env").read_text().rstrip().splitlines()
# Disposable relay-admin identity for the NIP-43 provisioning qualification.
creds += [f"BUZZ_TEST_RELAY_ADMIN_NSEC={keys['owner_a']}"]
creds += [f"POSTGRES_PASSWORD={dbpass}", f"DATABASE_URL=postgres://postgres:{dbpass}@127.0.0.1/rustshare_test"]
f = out / "connector.env"; f.write_text("\n".join(creds) + "\n"); f.chmod(0o600)
f = out / "seed-config.env"; f.write_text(f"RELAY_A_URL=wss://buzz-edge-a.{ns}.svc.cluster.local\nRELAY_B_URL=wss://buzz-edge-b.{ns}.svc.cluster.local\nROOT_A_PEM=/run/roots/a.pem\nROOT_B_PEM=/run/roots/b.pem\n"); f.chmod(0o600)
PY

make_tls() {
	local tenant="$1" host="$2" dir="$RUN_DIR/tls-$1"
	mkdir -m 0700 "$dir"
	openssl ecparam -name prime256v1 -genkey -noout -out "$dir/ca.key"
	openssl req -x509 -new -key "$dir/ca.key" -sha256 -days 1 -subj "/CN=elembra-cilium-$tenant-root" -out "$dir/ca.pem" >/dev/null 2>&1
	openssl ecparam -name prime256v1 -genkey -noout -out "$dir/leaf.key"
	openssl req -new -key "$dir/leaf.key" -subj "/CN=$host" -out "$dir/leaf.csr" >/dev/null 2>&1
	cat >"$dir/leaf.ext" <<EOF_CERT
[server_cert]
subjectAltName=DNS:$host
extendedKeyUsage=serverAuth
keyUsage=digitalSignature,keyEncipherment
EOF_CERT
	openssl x509 -req -in "$dir/leaf.csr" -CA "$dir/ca.pem" -CAkey "$dir/ca.key" \
		-CAcreateserial -out "$dir/leaf.crt" -days 1 -sha256 -extfile "$dir/leaf.ext" -extensions server_cert >/dev/null 2>&1
	openssl x509 -in "$dir/ca.pem" -outform DER -out "$dir/ca.der"
	openssl x509 -in "$dir/leaf.crt" -pubkey -noout | openssl pkey -pubin -outform DER \
		| openssl dgst -sha256 -hex | awk '{print "sha256:"$NF}' >"$dir/spki.txt"
}
make_tls a "buzz-edge-a.$NAMESPACE.svc.cluster.local"
make_tls b "buzz-edge-b.$NAMESPACE.svc.cluster.local"

kubectl create namespace "$NAMESPACE" >/dev/null
for tenant in a b; do
	kubectl -n "$NAMESPACE" create secret generic "buzz-$tenant-config" --from-env-file="$RUN_DIR/secrets/buzz-$tenant.env" >/dev/null
	kubectl -n "$NAMESPACE" create secret generic "buzz-edge-$tenant-tls" \
		--from-file=tls.crt="$RUN_DIR/tls-$tenant/leaf.crt" --from-file=tls.key="$RUN_DIR/tls-$tenant/leaf.key" >/dev/null
done
kubectl -n "$NAMESPACE" create secret generic buzz-seed-keys --from-env-file="$RUN_DIR/secrets/seed.env" >/dev/null
kubectl -n "$NAMESPACE" create secret generic buzz-root-pems \
	--from-file=a.pem="$RUN_DIR/tls-a/ca.pem" --from-file=b.pem="$RUN_DIR/tls-b/ca.pem" >/dev/null
kubectl -n "$NAMESPACE" create secret generic connector-test-credentials --from-env-file="$RUN_DIR/secrets/connector.env" >/dev/null
kubectl -n "$NAMESPACE" create secret generic buzz-root-ders \
	--from-file=a.der="$RUN_DIR/tls-a/ca.der" --from-file=b.der="$RUN_DIR/tls-b/ca.der" >/dev/null

python3 "$HERE/render.py" --phase base --namespace "$NAMESPACE" --buzz-image "$BUZZ_IMAGE" --poc-image "$POC_IMAGE" \
	--postgres-image "$POSTGRES_IMAGE" --redis-image "$REDIS_IMAGE" --silo-image "$SILO_IMAGE" --mc-image "$MC_IMAGE" --coredns-image "$COREDNS_IMAGE" \
	>"$RUN_DIR/base.json"
cp "$RUN_DIR/base.json" "$EVIDENCE_DIR/base-resources.json"
"${K[@]}" apply -f "$RUN_DIR/base.json" >"$EVIDENCE_DIR/base-apply.txt"
for name in postgres-a postgres-b redis-a redis-b silo-a silo-b buzz-edge-a buzz-edge-b; do
	"${K[@]}" -n "$NAMESPACE" rollout status "deployment/$name" --timeout=4m >"$RUN_DIR/rollout-$name.log"
done
for tenant in a b; do
	if ! "${K[@]}" -n "$NAMESPACE" wait --for=condition=complete "job/silo-init-$tenant" --timeout=3m >"$RUN_DIR/silo-init-$tenant.log"; then
		pod="$("${K[@]}" -n "$NAMESPACE" get pods -l "app=silo-init-$tenant" -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || true)"
		if [[ -n "$pod" ]]; then
			"${K[@]}" -n "$NAMESPACE" logs "$pod" -c mc >"$RUN_DIR/silo-init-$tenant-raw.log" 2>&1 || true
		else
			"${K[@]}" -n "$NAMESPACE" logs "job/silo-init-$tenant" >"$RUN_DIR/silo-init-$tenant-raw.log" 2>&1 || true
		fi
		python3 - "$RUN_DIR/silo-init-$tenant-raw.log" "$RUN_DIR/secrets/buzz-$tenant.env" "$EVIDENCE_DIR/silo-init-$tenant-failure.log" <<'PY'
from pathlib import Path
import sys
log = Path(sys.argv[1]).read_text(errors="replace")
secrets = [line.partition("=")[2].strip() for line in Path(sys.argv[2]).read_text().splitlines() if "=" in line]
for secret in sorted((value for value in secrets if value), key=len, reverse=True):
    log = log.replace(secret, "[redacted]")
out = Path(sys.argv[3])
out.write_text(log)
out.chmod(0o600)
PY
		echo "Silo bucket initialization failed for tenant $tenant; see sanitized evidence $EVIDENCE_DIR/silo-init-$tenant-failure.log" >&2
		exit 1
	fi
done
for name in buzz-a buzz-b; do
	"${K[@]}" -n "$NAMESPACE" rollout status "deployment/$name" --timeout=4m >"$RUN_DIR/rollout-$name.log"
done
EDGE_A_IP="$("${K[@]}" -n "$NAMESPACE" get svc buzz-edge-a -o jsonpath='{.spec.clusterIP}')"
EDGE_B_IP="$("${K[@]}" -n "$NAMESPACE" get svc buzz-edge-b -o jsonpath='{.spec.clusterIP}')"
DNS_IP="$("${K[@]}" -n "$NAMESPACE" get svc controlled-dns -o jsonpath='{.spec.clusterIP}')"
python3 "$HERE/render.py" --phase dns --namespace "$NAMESPACE" --edge-a-ip "$EDGE_A_IP" --edge-b-ip "$EDGE_B_IP" >"$RUN_DIR/dns-normal.json"
cp "$RUN_DIR/dns-normal.json" "$EVIDENCE_DIR/dns-normal.json"
"${K[@]}" apply -f "$RUN_DIR/dns-normal.json" >"$EVIDENCE_DIR/dns-normal-apply.txt"
"${K[@]}" -n "$NAMESPACE" rollout status deployment/controlled-dns --timeout=90s >"$RUN_DIR/dns-rollout.log"
kubectl -n "$NAMESPACE" create configmap seed-config --from-env-file="$RUN_DIR/secrets/seed-config.env" >/dev/null
python3 "$HERE/render.py" --phase seed --namespace "$NAMESPACE" --poc-image "$POC_IMAGE" --dns-ip "$DNS_IP" >"$RUN_DIR/seed-job.json"
"${K[@]}" apply -f "$RUN_DIR/seed-job.json" >"$EVIDENCE_DIR/seed-apply.txt"
if ! "${K[@]}" -n "$NAMESPACE" wait --for=condition=complete job/seed-private-dms --timeout=2m >"$RUN_DIR/seed-wait.log"; then
	"${K[@]}" -n "$NAMESPACE" logs job/seed-private-dms >"$RUN_DIR/seed-raw.log" 2>&1 || true
	python3 - "$RUN_DIR/seed-raw.log" "$RUN_DIR/secrets/seed.env" "$EVIDENCE_DIR/seed-failure.log" <<'PY'
from pathlib import Path
import sys
log = Path(sys.argv[1]).read_text(errors="replace")
secrets = [line.partition("=")[2].strip() for line in Path(sys.argv[2]).read_text().splitlines() if "=" in line]
for secret in sorted((value for value in secrets if value), key=len, reverse=True):
    log = log.replace(secret, "[redacted]")
out = Path(sys.argv[3])
out.write_text(log)
out.chmod(0o600)
PY
	echo "upstream NIP-42/NIP-29 fixture seeding failed; see sanitized evidence $EVIDENCE_DIR/seed-failure.log" >&2
	exit 1
fi
"${K[@]}" -n "$NAMESPACE" logs job/seed-private-dms >"$EVIDENCE_DIR/seed-metadata.json"

CLUSTER_UID="$("${K[@]}" get namespace kube-system -o jsonpath='{.metadata.uid}')"
SERVICE_A_UID="$("${K[@]}" -n "$NAMESPACE" get svc buzz-edge-a -o jsonpath='{.metadata.uid}')"
SERVICE_B_UID="$("${K[@]}" -n "$NAMESPACE" get svc buzz-edge-b -o jsonpath='{.metadata.uid}')"
python3 - "$EVIDENCE_DIR/seed-metadata.json" "$RUN_DIR/private/public.json" "$RUN_DIR/connector-public.env" \
	"$NAMESPACE" "$EDGE_A_IP" "$EDGE_B_IP" "$BUZZ_IMAGE_ID" "$CLUSTER_UID" "$SERVICE_A_UID" "$SERVICE_B_UID" \
	"$(cat "$RUN_DIR/tls-a/spki.txt")" "$(cat "$RUN_DIR/tls-b/spki.txt")" <<'PY'
from pathlib import Path
import json, sys
seed = json.loads(Path(sys.argv[1]).read_text())
keys = json.loads(Path(sys.argv[2]).read_text())
if seed.get("ok") is not True: raise SystemExit("seeder did not report success")
ns, ip_a, ip_b, digest, cluster_uid, svc_a_uid, svc_b_uid, spki_a, spki_b = sys.argv[4:13]
a, b = seed["tenants"]["a"], seed["tenants"]["b"]
host_a, host_b = f"buzz-edge-a.{ns}.svc.cluster.local", f"buzz-edge-b.{ns}.svc.cluster.local"
values = {
 "BUZZ_TEST_DISPOSABLE_CONFIRM":"DISPOSABLE_ONLY",
 "BUZZ_TEST_RELAY_URL":f"wss://{host_a}", "BUZZ_TEST_RELAY_HOST":host_a, "BUZZ_TEST_COMMUNITY_ID":a["community_id"],
 "BUZZ_TEST_RELAY_PUBKEY":a["relay_pubkey"], "BUZZ_TEST_DEPLOYMENT_DIGEST":digest,
 "BUZZ_TEST_SERVICE_IDENTITY":f"k8s://{cluster_uid}/{ns}/buzz-edge-a/{svc_a_uid}", "BUZZ_TEST_TLS_SPKI_PIN":spki_a,
 "BUZZ_TEST_APPROVED_IP":ip_a, "BUZZ_TEST_DM_CHANNEL_ID":a["channel_id"], "BUZZ_TEST_EXPECTED_DM_EVENT_ID":a["event_id"],
 "BUZZ_TEST_PROVISIONING_CHANNEL_ID":a["provisioning_channel_id"], "BUZZ_TEST_PROVISIONING_EVENT_ID":a["provisioning_event_id"],
 "BUZZ_TEST_EXPECTED_DM_AUTHOR":a["author"], "BUZZ_TEST_OTHER_MEMBER_PUBKEY":keys["member_b_pubkey"], "BUZZ_TEST_TLS_ROOT_DER":"/run/trust/a.der",
 "BUZZ_TEST_B_RELAY_URL":f"wss://{host_b}", "BUZZ_TEST_B_RELAY_HOST":host_b, "BUZZ_TEST_B_COMMUNITY_ID":b["community_id"],
 "BUZZ_TEST_B_RELAY_PUBKEY":b["relay_pubkey"], "BUZZ_TEST_B_DEPLOYMENT_DIGEST":digest,
 "BUZZ_TEST_B_SERVICE_IDENTITY":f"k8s://{cluster_uid}/{ns}/buzz-edge-b/{svc_b_uid}", "BUZZ_TEST_B_TLS_SPKI_PIN":spki_b,
 "BUZZ_TEST_B_APPROVED_IP":ip_b, "BUZZ_TEST_B_DM_CHANNEL_ID":b["channel_id"], "BUZZ_TEST_B_EXPECTED_DM_EVENT_ID":b["event_id"],
 "BUZZ_TEST_B_EXPECTED_DM_AUTHOR":b["author"], "BUZZ_TEST_B_OTHER_MEMBER_PUBKEY":keys["member_a_pubkey"], "BUZZ_TEST_B_TLS_ROOT_DER":"/run/trust/b.der",
 "BUZZ_TEST_DNS_REBIND_RELAY_URL":f"wss://{host_a}", "BUZZ_TEST_DNS_REBIND_HOST":host_a,
 "BUZZ_TEST_DNS_REBIND_APPROVED_IP":ip_a, "BUZZ_TEST_DNS_REBIND_UNAPPROVED_IP":ip_b,
 "BUZZ_TEST_DNS_REBIND_COMMUNITY_ID":a["community_id"], "BUZZ_TEST_DNS_REBIND_RELAY_PUBKEY":a["relay_pubkey"],
 "BUZZ_TEST_DNS_REBIND_DEPLOYMENT_DIGEST":digest,
 "BUZZ_TEST_DNS_REBIND_SERVICE_IDENTITY":f"k8s://{cluster_uid}/{ns}/buzz-edge-a/{svc_a_uid}",
 "BUZZ_TEST_DNS_REBIND_TLS_SPKI_PIN":spki_a, "BUZZ_TEST_DNS_REBIND_TLS_ROOT_DER":"/run/trust/a.der",
 "BUZZ_TEST_DB_OUTAGE_ACK_FILE":"/tmp/db-outage.ack", "BUZZ_TEST_DB_RESTORE_ACK_FILE":"/tmp/db-restore.ack",
 "BUZZ_TEST_RELAY_OUTAGE_ACK_FILE":"/tmp/relay-outage.ack", "BUZZ_TEST_RELAY_RESTORE_ACK_FILE":"/tmp/relay-restore.ack",
}
path = Path(sys.argv[3]); path.write_text("".join(f"{k}={v}\n" for k,v in values.items())); path.chmod(0o600)
public = {
    "tenant_a": {"community": a["community_id"], "channel": a["channel_id"], "event": a["event_id"], "author": a["author"],
                 "provisioning_channel": a["provisioning_channel_id"], "provisioning_event": a["provisioning_event_id"],
                 "provisioning_author": a["provisioning_author"]},
    "tenant_b": {"community": b["community_id"], "channel": b["channel_id"], "event": b["event_id"], "author": b["author"],
                 "provisioning_channel": b["provisioning_channel_id"], "provisioning_event": b["provisioning_event_id"],
                 "provisioning_author": b["provisioning_author"]},
}
Path(sys.argv[1]).write_text(json.dumps(public, indent=2) + "\n")
PY
cp "$RUN_DIR/connector-public.env" "$EVIDENCE_DIR/connector-public.env"
kubectl -n "$NAMESPACE" create configmap connector-test-config --from-env-file="$RUN_DIR/connector-public.env" >/dev/null
python3 "$HERE/render.py" --phase runner --namespace "$NAMESPACE" --runner-image "$RUNNER_IMAGE" \
	--pgvector-image "$PGVECTOR_IMAGE" --dns-ip "$DNS_IP" >"$RUN_DIR/runner-pod.json"
cp "$RUN_DIR/runner-pod.json" "$EVIDENCE_DIR/runner-pod.json"
"${K[@]}" apply -f "$RUN_DIR/runner-pod.json" >"$EVIDENCE_DIR/runner-apply.txt"
"${K[@]}" -n "$NAMESPACE" wait --for=condition=Ready pod/rust-connector --timeout=2m >"$RUN_DIR/runner-ready.log"

"${K[@]}" -n kube-system port-forward --address 127.0.0.1 svc/hubble-relay 4245:80 >"$EVIDENCE_DIR/hubble-port-forward.log" 2>&1 &
PF_PID=$!
hubble_ready=0
for _ in {1..30}; do
	if hubble observe --server 127.0.0.1:4245 --last 1 --output compact >"$RUN_DIR/hubble-ready.log" 2>&1; then
		hubble_ready=1
		break
	fi
	sleep 1
done
kill -0 "$PF_PID" 2>/dev/null || { echo 'Hubble port-forward failed' >&2; exit 1; }
[[ "$hubble_ready" == 1 ]] || { echo 'Hubble observer readiness failed' >&2; exit 1; }

timeout --signal=TERM 10s hubble observe --server 127.0.0.1:4245 --since 1s --follow \
	--from-pod "$NAMESPACE/rust-connector" --protocol tcp --to-ip 1.1.1.1 --verdict DROPPED --output compact \
	>"$EVIDENCE_DIR/public-egress-hubble.log" 2>&1 &
EGRESS_PID=$!
sleep 1
set +e
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- pg_isready -h 1.1.1.1 -p 443 -t 3 >"$EVIDENCE_DIR/public-egress-probe.log" 2>&1
egress_status=$?
set -e
sleep 1
kill -INT "$EGRESS_PID" >/dev/null 2>&1 || true
wait "$EGRESS_PID" 2>/dev/null || true
EGRESS_PID=
if [[ "$egress_status" == 0 ]] || ! grep -Eq '1\.1\.1\.1:443.*(DENIED|DROPPED)|(DENIED|DROPPED).*1\.1\.1\.1:443' "$EVIDENCE_DIR/public-egress-hubble.log"; then
	echo 'Cilium did not prove the forbidden public TCP destination was denied' >&2
	cat "$EVIDENCE_DIR/public-egress-hubble.log" "$EVIDENCE_DIR/public-egress-probe.log" >&2
	exit 1
fi
echo 'forbidden_public_tcp=denied_by_cilium' >"$EVIDENCE_DIR/public-egress-result.txt"

run_rust_test() {
	local label="$1" filter="$2" capture="$3" log="$EVIDENCE_DIR/$1.log" test_status capture_alive=1
	if [[ "$capture" == tcp || "$capture" == no-tcp ]]; then
		timeout --signal=TERM 60s hubble observe --server 127.0.0.1:4245 --since 1s --follow --pod "$NAMESPACE/rust-connector" --protocol tcp --output compact >"$EVIDENCE_DIR/$1-hubble-tcp.log" 2>&1 &
		TCP_PID=$!
		timeout --signal=TERM 60s hubble observe --server 127.0.0.1:4245 --since 1s --follow --pod "$NAMESPACE/rust-connector" --protocol udp --output compact >"$EVIDENCE_DIR/$1-hubble-udp.log" 2>&1 &
		UDP_PID=$!
		# Let both Hubble streams subscribe before the test's first network request.
		sleep 3
		for name in TCP_PID UDP_PID; do
			local pid="${!name}"
			if ! kill -0 "$pid" 2>/dev/null; then
				echo "Hubble $name observer failed before $label" >&2
				return 1
			fi
		done
		if [[ "$capture" == no-tcp ]]; then
			local dns_host="hubble-ready-$RUN_ID.$NAMESPACE.svc.cluster.local"
			"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- getent ahostsv4 "$dns_host" \
				>"$EVIDENCE_DIR/dns-observer-preflight.txt" 2>&1 || true
			for _ in {1..20}; do
				if grep -Eq 'rust-connector.*->.*controlled-dns.*:53' "$EVIDENCE_DIR/$1-hubble-udp.log"; then break; fi
				sleep 0.25
			done
			if ! grep -Eq 'rust-connector.*->.*controlled-dns.*:53' "$EVIDENCE_DIR/$1-hubble-udp.log"; then
				echo 'Hubble did not capture the controlled DNS preflight' >&2
				cat "$EVIDENCE_DIR/$1-hubble-udp.log" >&2
				return 1
			fi
		fi
	fi
	set +e
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- /usr/local/bin/test-runner --ignored --exact "$filter" --nocapture >"$log" 2>&1
	test_status=$?
	set -e
	echo "exit=$test_status" >>"$log"
	for name in TCP_PID UDP_PID; do
		local pid="${!name}"
		if [[ -n "$pid" ]]; then
			if ! kill -0 "$pid" 2>/dev/null; then
				echo "Hubble $name observer exited during $label" >&2
				capture_alive=0
			fi
			kill -INT "$pid" >/dev/null 2>&1 || true
			wait "$pid" 2>/dev/null || true
			printf -v "$name" ''
		fi
	done
	if [[ "$test_status" != 0 ]] || ! grep -q '1 passed; 0 failed' "$log"; then
		echo "Rust test failed: $filter" >&2; tail -n 40 "$log" >&2; return 1
	fi
	[[ "$capture_alive" == 1 ]] || return 1
	if [[ "$capture" == no-tcp ]]; then
		if ! grep -Eq 'rust-connector.*->.*controlled-dns.*:53' "$EVIDENCE_DIR/$1-hubble-udp.log"; then
			echo 'no positive DNS exchange in the rejection-window capture' >&2
			cat "$EVIDENCE_DIR/$1-hubble-udp.log" >&2
			return 1
		fi
		# Source ports and resolver flow counts vary across libc/runtime versions.
		# The Rust test validates the answers; this preflight validates DNS capture,
		# while the same window verifies the rejected target was never dialed.
		if ! grep -Eq 'rust-connector.*->.*controlled-dns.*:53' "$EVIDENCE_DIR/$1-hubble-udp.log"; then
			echo 'Hubble did not capture the controlled DNS preflight' >&2
			cat "$EVIDENCE_DIR/$1-hubble-udp.log" >&2
			return 1
		fi
		if grep -q -- '->' "$EVIDENCE_DIR/$1-hubble-tcp.log"; then
			echo 'unexpected TCP flow in DNS-rejection window' >&2; cat "$EVIDENCE_DIR/$1-hubble-tcp.log" >&2; return 1
		fi
	fi
}

wait_for_test_marker() {
	local log="$1" marker="$2"
	for _ in {1..120}; do
		if grep -Fq "$marker" "$log"; then return 0; fi
		if ! kill -0 "$TEST_PID" 2>/dev/null; then
			echo "Disposable relay test exited before $marker" >&2
			tail -n 30 "$log" >&2
			return 1
		fi
		sleep 1
	done
	echo "Timed out waiting for disposable relay test marker $marker" >&2
	tail -n 30 "$log" >&2
	return 1
}

run_member_transport_test() {
	local filter=upstream_buzz_transport::disposable_service_tests::trusted_service_transport_connects_and_authenticates_member
	local log="$EVIDENCE_DIR/member-transport.log" test_status db_stopped=0
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- /usr/local/bin/test-runner --ignored --exact "$filter" --nocapture >"$log" 2>&1 &
	TEST_PID=$!
	echo 'waiting for DB outage marker' >>"$EVIDENCE_DIR/member-transport-orchestration.log"
	wait_for_test_marker "$log" BUZZ_TEST_DB_OUTAGE_READY
	echo 'DB outage marker observed; stopping PostgreSQL' >>"$EVIDENCE_DIR/member-transport-orchestration.log"
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- gosu postgres pg_ctl -D /var/lib/postgresql/data/pgdata -m fast stop >"$RUN_DIR/postgres-stop.log" 2>&1
	cp "$RUN_DIR/postgres-stop.log" "$EVIDENCE_DIR/postgres-stop.log"
	echo 'pg_ctl stop returned' >>"$EVIDENCE_DIR/member-transport-orchestration.log"
	for _ in {1..30}; do
		if ! "${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- pg_isready -h 127.0.0.1 -p 5432 -U postgres -d rustshare_test >/dev/null 2>&1; then
			db_stopped=1
			break
		fi
		sleep 1
	done
	[[ "$db_stopped" == 1 ]] || { echo 'PostgreSQL did not become unavailable' >&2; return 1; }
	echo 'PostgreSQL outage verified' >>"$EVIDENCE_DIR/member-transport-orchestration.log"
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- touch /tmp/db-outage.ack
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- test -f /tmp/db-outage.ack
	echo 'outage acknowledgement visible in test container' >>"$EVIDENCE_DIR/member-transport-orchestration.log"
	wait_for_test_marker "$log" BUZZ_TEST_DB_RESTORE_READY
	echo 'restore-ready marker observed' >>"$EVIDENCE_DIR/member-transport-orchestration.log"
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- touch /run/db-control/restart
	echo 'postgres restart gate released' >>"$EVIDENCE_DIR/member-transport-orchestration.log"
	timeout --signal=TERM 90s "${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- sh -ec 'until pg_isready -h 127.0.0.1 -p 5432 -U postgres -d rustshare_test; do sleep 1; done'
	echo 'PostgreSQL restore verified' >>"$EVIDENCE_DIR/member-transport-orchestration.log"
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- touch /tmp/db-restore.ack
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- test -f /tmp/db-restore.ack
	set +e
	wait "$TEST_PID"
	test_status=$?
	set -e
	TEST_PID=
	echo "exit=$test_status" >>"$log"
	if [[ "$test_status" != 0 ]] || ! grep -q '1 passed; 0 failed' "$log"; then
		echo "Rust test failed: $filter" >&2
		tail -n 40 "$log" >&2
		return 1
	fi
}

run_history_reader_db_outage_test() {
	local filter=upstream_identity_provisioning::tests::live_provisioning_reaches_ready_after_nip42_and_member_only_read
	local log="$EVIDENCE_DIR/provisioning.log" test_status db_stopped=0
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- rm -f /tmp/history-db-outage.ack /tmp/history-db-restore.ack
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- test ! -e /tmp/history-db-outage.ack -a ! -e /tmp/history-db-restore.ack
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- env BUZZ_TEST_HISTORY_DB_OUTAGE_ACK_FILE=/tmp/history-db-outage.ack BUZZ_TEST_HISTORY_DB_RESTORE_ACK_FILE=/tmp/history-db-restore.ack /usr/local/bin/test-runner --ignored --exact "$filter" --nocapture >"$log" 2>&1 &
	TEST_PID=$!
	wait_for_test_marker "$log" BUZZ_TEST_HISTORY_DB_OUTAGE_READY
	echo 'history read received relay events; stopping PostgreSQL before authorization revalidation' >"$EVIDENCE_DIR/history-reader-db-outage-orchestration.log"
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- gosu postgres pg_ctl -D /var/lib/postgresql/data/pgdata -m fast stop >"$RUN_DIR/history-postgres-stop.log" 2>&1
	cp "$RUN_DIR/history-postgres-stop.log" "$EVIDENCE_DIR/history-postgres-stop.log"
	for _ in {1..30}; do
		if ! "${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- pg_isready -h 127.0.0.1 -p 5432 -U postgres -d rustshare_test >/dev/null 2>&1; then
			db_stopped=1
			break
		fi
		sleep 1
	done
	[[ "$db_stopped" == 1 ]] || { echo 'PostgreSQL did not become unavailable during managed-history read' >&2; return 1; }
	echo 'PostgreSQL outage verified' >>"$EVIDENCE_DIR/history-reader-db-outage-orchestration.log"
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- touch /tmp/history-db-outage.ack
	wait_for_test_marker "$log" BUZZ_TEST_HISTORY_DB_RESTORE_READY
	echo 'managed-history reader failed closed; restoring PostgreSQL' >>"$EVIDENCE_DIR/history-reader-db-outage-orchestration.log"
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- touch /run/db-control/restart
	timeout --signal=TERM 90s "${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- sh -ec 'until pg_isready -h 127.0.0.1 -p 5432 -U postgres -d rustshare_test; do sleep 1; done'
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- touch /tmp/history-db-restore.ack
	set +e
	wait "$TEST_PID"
	test_status=$?
	set -e
	TEST_PID=
	echo "exit=$test_status" >>"$log"
	if [[ "$test_status" != 0 ]] || ! grep -q '1 passed; 0 failed' "$log"; then
		echo "Rust test failed: $filter" >&2
		tail -n 40 "$log" >&2
		return 1
	fi
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- rm -f /tmp/history-db-outage.ack /tmp/history-db-restore.ack
	echo 'PostgreSQL restore verified' >>"$EVIDENCE_DIR/history-reader-db-outage-orchestration.log"
}

run_provisioning_relay_outage_test() {
	local filter=upstream_identity_provisioning::tests::live_provisioning_recovers_after_relay_outage
	local log="$EVIDENCE_DIR/provisioning-relay-outage.log" test_status outage_verified=0 restore_verified=0
	local endpoints pods service_ip_before service_ip_after
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- /usr/local/bin/test-runner --ignored --exact "$filter" --nocapture >"$log" 2>&1 &
	TEST_PID=$!
	wait_for_test_marker "$log" BUZZ_TEST_RELAY_OUTAGE_READY
	endpoints="$("${K[@]}" -n "$NAMESPACE" get endpoints buzz-edge-a -o jsonpath='{.subsets[*].addresses[*].ip}')"
	[[ -n "$endpoints" ]] || { echo 'relay service had no endpoint before outage' >&2; return 1; }
	service_ip_before="$("${K[@]}" -n "$NAMESPACE" get service buzz-edge-a -o jsonpath='{.spec.clusterIP}')"
	"${K[@]}" -n "$NAMESPACE" scale deployment/buzz-edge-a --replicas=0 >"$RUN_DIR/relay-outage-scale-down.log"
	for _ in {1..60}; do
		endpoints="$("${K[@]}" -n "$NAMESPACE" get endpoints buzz-edge-a -o jsonpath='{.subsets[*].addresses[*].ip}')"
		if [[ -z "$endpoints" ]]; then outage_verified=1; break; fi
		sleep 1
	done
	[[ "$outage_verified" == 1 ]] || { echo 'relay Service endpoint remained during scale-down' >&2; return 1; }
	cp "$RUN_DIR/relay-outage-scale-down.log" "$EVIDENCE_DIR/relay-outage-scale-down.log"
	for _ in {1..90}; do
		pods="$(${K[@]} -n "$NAMESPACE" get pods -l app=buzz-edge-a -o name)"
		[[ -z "$pods" ]] && break
		sleep 1
	done
	[[ -z "$pods" ]] || { echo 'relay pod remained after scale-down' >&2; return 1; }
	echo 'relay_pod_deleted=true' >"$EVIDENCE_DIR/relay-outage-pod-deletion.log"
	"${K[@]}" -n "$NAMESPACE" get deployment,service,endpoints buzz-edge-a -o wide >"$EVIDENCE_DIR/relay-outage-down-state.txt"
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- touch /tmp/relay-outage.ack
	wait_for_test_marker "$log" BUZZ_TEST_RELAY_RESTORE_READY
	"${K[@]}" -n "$NAMESPACE" scale deployment/buzz-edge-a --replicas=1 >"$RUN_DIR/relay-outage-scale-up.log"
	"${K[@]}" -n "$NAMESPACE" rollout status deployment/buzz-edge-a --timeout=120s >"$RUN_DIR/relay-outage-rollout.log"
	for _ in {1..60}; do
		endpoints="$("${K[@]}" -n "$NAMESPACE" get endpoints buzz-edge-a -o jsonpath='{.subsets[*].addresses[*].ip}')"
		if [[ -n "$endpoints" ]]; then restore_verified=1; break; fi
		sleep 1
	done
	[[ "$restore_verified" == 1 ]] || { echo 'relay Service endpoint did not return after scale-up' >&2; return 1; }
	service_ip_after="$("${K[@]}" -n "$NAMESPACE" get service buzz-edge-a -o jsonpath='{.spec.clusterIP}')"
	[[ "$service_ip_before" == "$service_ip_after" ]] || { echo 'relay Service identity changed across the outage' >&2; return 1; }
	cp "$RUN_DIR/relay-outage-scale-up.log" "$EVIDENCE_DIR/relay-outage-scale-up.log"
	cp "$RUN_DIR/relay-outage-rollout.log" "$EVIDENCE_DIR/relay-outage-rollout.log"
	"${K[@]}" -n "$NAMESPACE" get deployment,service,endpoints buzz-edge-a -o wide >"$EVIDENCE_DIR/relay-outage-up-state.txt"
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- touch /tmp/relay-restore.ack
	set +e
	wait "$TEST_PID"
	test_status=$?
	set -e
	TEST_PID=
	echo "exit=$test_status" >>"$log"
	if [[ "$test_status" != 0 ]] || ! grep -q '1 passed; 0 failed' "$log"; then
		echo "Rust test failed: $filter" >&2
		tail -n 40 "$log" >&2
		return 1
	fi
}

run_member_transport_test
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- dropdb -U postgres --if-exists rustshare_test >"$EVIDENCE_DIR/cross-tenant-db-reset.log" 2>&1
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- createdb -U postgres rustshare_test >>"$EVIDENCE_DIR/cross-tenant-db-reset.log" 2>&1
run_rust_test cross-tenant upstream_buzz_transport::disposable_service_tests::trusted_service_tenant_sessions_are_isolated_across_private_dm_relays none
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- dropdb -U postgres --if-exists rustshare_test >"$EVIDENCE_DIR/member-revocation-db-reset.log" 2>&1
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- createdb -U postgres rustshare_test >>"$EVIDENCE_DIR/member-revocation-db-reset.log" 2>&1
run_rust_test member-revocation upstream_buzz_transport::disposable_service_tests::trusted_service_rejects_revoked_member_on_reconnect none
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- dropdb -U postgres --if-exists rustshare_test >"$EVIDENCE_DIR/member-db-reset.log" 2>&1
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- createdb -U postgres rustshare_test >>"$EVIDENCE_DIR/member-db-reset.log" 2>&1
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- dropdb -U postgres --if-exists rustshare_test >"$RUN_DIR/db-reset.log" 2>&1
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- createdb -U postgres rustshare_test >>"$RUN_DIR/db-reset.log" 2>&1
run_history_reader_db_outage_test
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- dropdb -U postgres --if-exists rustshare_test >"$EVIDENCE_DIR/provisioning-relay-outage-db-reset.log" 2>&1
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- createdb -U postgres rustshare_test >>"$EVIDENCE_DIR/provisioning-relay-outage-db-reset.log" 2>&1
run_provisioning_relay_outage_test
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- dropdb -U postgres --if-exists rustshare_test >"$RUN_DIR/db-reset-provisioning.log" 2>&1
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- createdb -U postgres rustshare_test >>"$RUN_DIR/db-reset-provisioning.log" 2>&1
python3 "$HERE/render.py" --phase dns --namespace "$NAMESPACE" --edge-a-ip "$EDGE_A_IP" --edge-b-ip "$EDGE_B_IP" --poison-dns >"$RUN_DIR/dns-poison.json"
cp "$RUN_DIR/dns-poison.json" "$EVIDENCE_DIR/dns-poison.json"
"${K[@]}" apply -f "$RUN_DIR/dns-poison.json" >"$EVIDENCE_DIR/dns-poison-apply.txt"
"${K[@]}" -n "$NAMESPACE" rollout restart deployment/controlled-dns >"$RUN_DIR/dns-restart.log"
"${K[@]}" -n "$NAMESPACE" rollout status deployment/controlled-dns --timeout=90s >>"$RUN_DIR/dns-restart.log"
sleep 2
run_rust_test dns-poison upstream_buzz_transport::disposable_service_tests::trusted_service_rejects_an_unapproved_dns_answer_before_socket_dial no-tcp
cp "$RUN_DIR/dns-restart.log" "$EVIDENCE_DIR/dns-restart.txt"
"${K[@]}" get cnp -n "$NAMESPACE" -o yaml >"$EVIDENCE_DIR/cilium-policies.yaml"
"${K[@]}" get pods -n "$NAMESPACE" -o wide >"$EVIDENCE_DIR/pods-final.txt"
python3 - "$EVIDENCE_DIR" "$RUN_DIR/private/connector-credentials.env" "$RUN_DIR/secrets/connector.env" <<'PY'
from pathlib import Path
import sys

evidence = Path(sys.argv[1])
secrets = set()
for name in sys.argv[2:]:
    for line in Path(name).read_text().splitlines():
        key, separator, value = line.partition("=")
        if separator and "NSEC" in key and value:
            secrets.add(value.encode())

leaks = []
for path in evidence.iterdir():
    if not path.is_file():
        continue
    data = path.read_bytes()
    original = data
    for secret in secrets:
        if secret in data:
            data = data.replace(secret, b"[redacted-test-key]")
    if data != original:
        path.write_bytes(data)
        leaks.append(path.name)

if leaks:
    print("test private-key material found and redacted in evidence: " + ", ".join(leaks), file=sys.stderr)
    raise SystemExit(1)
PY
echo 'result=PASS: all six selected ignored Rust trusted-service and provisioning tests passed' >"$EVIDENCE_DIR/result.txt"
echo "cluster=$CLUSTER" >>"$EVIDENCE_DIR/result.txt"
