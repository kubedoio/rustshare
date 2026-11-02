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
RUN_ID="$(date -u +%Y%m%d%H%M%S)-$$"
CLUSTER="elembra-cilium-${RUN_ID}"
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

cleanup() {
	local status=$?
	trap - EXIT INT TERM
	for pid in "$TCP_PID" "$UDP_PID" "$PF_PID"; do
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

Runs two ignored Rust trusted-service tests in a new kind/Cilium cluster. It
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
kind version | grep -Fq "$KIND_VERSION" || { echo "kind must be $KIND_VERSION" >&2; exit 2; }
kubectl version --client -o yaml | grep -Fq "$KUBERNETES_VERSION" || { echo "kubectl must be $KUBERNETES_VERSION" >&2; exit 2; }
cilium version | grep -Fq "$CILIUM_CLI_VERSION" || { echo "Cilium CLI must be $CILIUM_CLI_VERSION" >&2; exit 2; }
cilium version | grep -Fq "$CILIUM_VERSION" || { echo "Cilium image must be $CILIUM_VERSION" >&2; exit 2; }
hubble version | grep -Fq "$HUBBLE_CLI_VERSION" || { echo "Hubble CLI must be $HUBBLE_CLI_VERSION" >&2; exit 2; }
rustc +1.97.1 --version | grep -Fq 1.97.1 || { echo 'Rust 1.97.1 is required' >&2; exit 2; }
node --version | grep -Eq '^v22\.' || { echo 'Node.js 22 is required' >&2; exit 2; }
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
{
	echo "buzz_source=$SOURCE_SHA"
	echo "buzz_tree=$SOURCE_TREE"
	echo "kind=$(kind version | head -1)"
	echo "kubectl=$(kubectl version --client -o json | python3 -c 'import json,sys; print(json.load(sys.stdin)["clientVersion"]["gitVersion"])')"
	echo "cilium=$(cilium version | head -1)"
	echo "hubble=$(hubble version | head -1)"
	echo "rustc=$(rustc +1.97.1 --version)"
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
for name in postgres-a postgres-b redis-a redis-b silo-a silo-b buzz-a buzz-b buzz-edge-a buzz-edge-b; do
	"${K[@]}" -n "$NAMESPACE" rollout status "deployment/$name" --timeout=4m >"$RUN_DIR/rollout-$name.log"
done
for tenant in a b; do "${K[@]}" -n "$NAMESPACE" wait --for=condition=complete "job/silo-init-$tenant" --timeout=3m >"$RUN_DIR/silo-init-$tenant.log"; done
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
	"${K[@]}" -n "$NAMESPACE" logs job/seed-private-dms >"$EVIDENCE_DIR/seed-failure.log" 2>&1 || true
	echo 'upstream NIP-42/NIP-29 fixture seeding failed' >&2
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
}
path = Path(sys.argv[3]); path.write_text("".join(f"{k}={v}\n" for k,v in values.items())); path.chmod(0o600)
public = {"tenant_a": {"community": a["community_id"], "channel": a["channel_id"], "event": a["event_id"], "author": a["author"]},
          "tenant_b": {"community": b["community_id"], "channel": b["channel_id"], "event": b["event_id"], "author": b["author"]}}
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
for _ in {1..30}; do
	if hubble observe --server 127.0.0.1:4245 --last 1 --output compact >"$RUN_DIR/hubble-ready.log" 2>&1; then break; fi
	sleep 1
done
kill -0 "$PF_PID" 2>/dev/null || { echo 'Hubble port-forward failed' >&2; exit 1; }

run_rust_test() {
	local label="$1" filter="$2" capture="$3" log="$EVIDENCE_DIR/$1.log" test_status
	if [[ "$capture" == tcp ]]; then
		timeout --signal=TERM 60s hubble observe --server 127.0.0.1:4245 --since 1s --follow --pod "$NAMESPACE/rust-connector" --protocol tcp --output compact >"$EVIDENCE_DIR/$1-hubble-tcp.log" 2>&1 &
		TCP_PID=$!
		timeout --signal=TERM 60s hubble observe --server 127.0.0.1:4245 --since 1s --follow --pod "$NAMESPACE/rust-connector" --protocol udp --output compact >"$EVIDENCE_DIR/$1-hubble-udp.log" 2>&1 &
		UDP_PID=$!
		sleep 1
	fi
	set +e
	"${K[@]}" -n "$NAMESPACE" exec rust-connector -c rust-test -- /usr/local/bin/test-runner --ignored --exact "$filter" --nocapture >"$log" 2>&1
	test_status=$?
	set -e
	echo "exit=$test_status" >>"$log"
	for name in TCP_PID UDP_PID; do
		local pid="${!name}"
		if [[ -n "$pid" ]]; then kill -INT "$pid" >/dev/null 2>&1 || true; wait "$pid" 2>/dev/null || true; printf -v "$name" ''; fi
	done
	if [[ "$test_status" != 0 ]] || ! grep -q '1 passed; 0 failed' "$log"; then
		echo "Rust test failed: $filter" >&2; tail -n 40 "$log" >&2; return 1
	fi
	if [[ "$capture" == tcp ]] && grep -q -- '->' "$EVIDENCE_DIR/$1-hubble-tcp.log"; then
		echo 'unexpected TCP flow in DNS-rejection window' >&2; cat "$EVIDENCE_DIR/$1-hubble-tcp.log" >&2; return 1
	fi
}

run_rust_test cross-tenant upstream_buzz_transport::disposable_service_tests::trusted_service_tenant_sessions_are_isolated_across_private_dm_relays none
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- dropdb -U postgres --if-exists rustshare_test >"$RUN_DIR/db-reset.log" 2>&1
"${K[@]}" -n "$NAMESPACE" exec rust-connector -c postgres -- createdb -U postgres rustshare_test >>"$RUN_DIR/db-reset.log" 2>&1
python3 "$HERE/render.py" --phase dns --namespace "$NAMESPACE" --edge-a-ip "$EDGE_A_IP" --edge-b-ip "$EDGE_B_IP" --poison-dns >"$RUN_DIR/dns-poison.json"
cp "$RUN_DIR/dns-poison.json" "$EVIDENCE_DIR/dns-poison.json"
"${K[@]}" apply -f "$RUN_DIR/dns-poison.json" >"$EVIDENCE_DIR/dns-poison-apply.txt"
"${K[@]}" -n "$NAMESPACE" rollout restart deployment/controlled-dns >"$RUN_DIR/dns-restart.log"
"${K[@]}" -n "$NAMESPACE" rollout status deployment/controlled-dns --timeout=90s >>"$RUN_DIR/dns-restart.log"
sleep 2
run_rust_test dns-poison upstream_buzz_transport::disposable_service_tests::trusted_service_rejects_an_unapproved_dns_answer_before_socket_dial tcp
cp "$RUN_DIR/dns-restart.log" "$EVIDENCE_DIR/dns-restart.txt"
"${K[@]}" get cnp -n "$NAMESPACE" -o yaml >"$EVIDENCE_DIR/cilium-policies.yaml"
"${K[@]}" get pods -n "$NAMESPACE" -o wide >"$EVIDENCE_DIR/pods-final.txt"
echo 'result=PASS: both ignored Rust trusted-service tests passed' >"$EVIDENCE_DIR/result.txt"
echo "cluster=$CLUSTER" >>"$EVIDENCE_DIR/result.txt"
