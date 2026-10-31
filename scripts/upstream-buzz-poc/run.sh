#!/usr/bin/env bash
# Disposable, upstream-only Buzz relay interoperability run.
set -euo pipefail
umask 077

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SHA=eafd77e9f6e96052476bdd88be732c046fdb7ab3
TREE=04e3a40545bd3b23510ec32b4c8cccee2acef7c5
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/upstream-buzz-poc.XXXXXX")"
PROJECT="buzzpoc$(basename "$RUN_DIR" | tr -cd '[:alnum:]' | tr '[:upper:]' '[:lower:]')"
SOURCE="${BUZZ_SOURCE_DIR:-$RUN_DIR/source}"
IMAGE="local/buzz-upstream-poc:${SHA:0:12}"
STACKS=()
PROXIES=()
POC_GUI_NSEC_FILE="$RUN_DIR/alice.nsec"

compose() { docker compose --project-name "$PROJECT-$1" --env-file "$RUN_DIR/$1/.env" -f "$RUN_DIR/$1/compose.yml" "${@:2}"; }
cleanup() {
  local status=$? failed=0
  trap - EXIT
  for proxy in "${PROXIES[@]}"; do kill "$proxy" >/dev/null 2>&1 || true; wait "$proxy" 2>/dev/null || true; done
  for stack in "${STACKS[@]}"; do
    if ! compose "$stack" down --volumes --remove-orphans >/dev/null 2>&1; then
      echo "Failed to remove disposable Buzz stack $PROJECT-$stack; secrets/config retained in $RUN_DIR" >&2
      failed=1
    fi
  done
  if [[ "$failed" == 0 ]]; then rm -rf -- "$RUN_DIR"; fi
  if [[ "$failed" == 1 && "$status" == 0 ]]; then status=1; fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for command in git docker node npm openssl curl; do command -v "$command" >/dev/null || { echo "Missing $command" >&2; exit 1; }; done
docker info >/dev/null 2>&1 || { echo 'Docker daemon unavailable' >&2; exit 1; }

if [[ -n "${BUZZ_SOURCE_DIR:-}" ]]; then
  git -C "$SOURCE" rev-parse --git-dir >/dev/null 2>&1 || { echo 'BUZZ_SOURCE_DIR must be a git checkout' >&2; exit 1; }
else
  git clone --quiet https://github.com/block/buzz.git "$SOURCE"
  git -C "$SOURCE" checkout --quiet --detach "$SHA"
fi
git -C "$SOURCE" cat-file -e "$SHA^{commit}"
[[ "$(git -C "$SOURCE" rev-parse HEAD)" == "$SHA" ]] || { echo 'Upstream source SHA mismatch' >&2; exit 1; }
[[ "$(git -C "$SOURCE" rev-parse HEAD^{tree})" == "$TREE" ]] || { echo 'Upstream tree hash mismatch' >&2; exit 1; }
[[ -z "$(git -C "$SOURCE" status --porcelain)" ]] || { echo 'Upstream checkout is modified' >&2; exit 1; }
[[ -z "$(git -C "$SOURCE" clean -ndX)" ]] || { echo 'Upstream checkout contains ignored files' >&2; exit 1; }

if ! docker buildx build --load --quiet --metadata-file "$RUN_DIR/build-metadata.json" --build-arg "BUZZ_SOURCE_SHA=$SHA" -t "$IMAGE" "$SOURCE" >"$RUN_DIR/build.log" 2>&1; then
  echo 'Unmodified upstream image build failed; see final build lines:' >&2
  tail -n 25 "$RUN_DIR/build.log" >&2
  exit 1
fi
IMAGE_ID="$(docker image inspect --format '{{.Id}}' "$IMAGE")"
IMAGE_BUILD_DIGEST="$(node -e 'const m=require(process.argv[1]);console.log(m["containerimage.digest"]||"unavailable")' "$RUN_DIR/build-metadata.json")"
mkdir -p "$RUN_DIR/test-deps"
cp "$ROOT/frontend/package.json" "$ROOT/frontend/package-lock.json" "$RUN_DIR/test-deps/"
npm ci --prefix "$RUN_DIR/test-deps" --ignore-scripts --no-audit --no-fund >"$RUN_DIR/npm.log" 2>&1 || { echo 'Frontend protocol dependencies could not be installed' >&2; tail -n 20 "$RUN_DIR/npm.log" >&2; exit 1; }
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
  -keyout "$RUN_DIR/tls.key" -out "$RUN_DIR/tls.crt" -days 1 \
  -subj '/CN=localhost' -addext 'subjectAltName=IP:127.0.0.1,DNS:localhost' \
  >"$RUN_DIR/tls.log" 2>&1

new_key() { openssl rand -hex 32; }
pubkey() { node -e 'const e=require("node:crypto").createECDH("secp256k1"); e.setPrivateKey(Buffer.from(process.argv[1],"hex")); console.log(e.getPublicKey("hex","compressed").slice(2))' "$1"; }
free_port() { node -e 'const s=require("node:net").createServer(); s.listen(0,"127.0.0.1",()=>{console.log(s.address().port);s.close()})'; }

POC_OWNER_SK="$(new_key)" POC_ALICE_SK="$(new_key)" POC_BOB_SK="$(new_key)" POC_OUTSIDER_SK="$(new_key)"
POC_B_OWNER_SK="$(new_key)" POC_B_USER_SK="$(new_key)"
start_stack() {
  local stack="$1" owner="$2" relay_key="$3" port="$4" tls_port="$5"
  mkdir -p "$RUN_DIR/$stack"
  cp "$SOURCE/deploy/compose/compose.yml" "$RUN_DIR/$stack/compose.yml"
  cat >"$RUN_DIR/$stack/.env" <<EOF
BUZZ_IMAGE=$IMAGE
BUZZ_HTTP_PORT=127.0.0.1:$port
RELAY_URL=wss://127.0.0.1:$tls_port
BUZZ_REQUIRE_AUTH_TOKEN=true
BUZZ_REQUIRE_RELAY_MEMBERSHIP=true
BUZZ_AUTO_MIGRATE=true
BUZZ_GIT_CONFORMANCE_PROBE=true
BUZZ_PUSH_ENABLED=false
RELAY_OWNER_PUBKEY=$(pubkey "$owner")
BUZZ_RELAY_PRIVATE_KEY=$relay_key
BUZZ_GIT_HOOK_HMAC_SECRET=$(new_key)
POSTGRES_PASSWORD=$(new_key)
REDIS_PASSWORD=$(new_key)
BUZZ_S3_ACCESS_KEY=poc$(openssl rand -hex 8)
BUZZ_S3_SECRET_KEY=$(new_key)
BUZZ_S3_BUCKET=buzz-media
BUZZ_S3_ADDRESSING_STYLE=path
EOF
  compose "$stack" config --quiet
  STACKS+=("$stack")
  compose "$stack" up -d >"$RUN_DIR/$stack/start.log" 2>&1 || { echo "Stack $stack failed to start" >&2; tail -n 20 "$RUN_DIR/$stack/start.log" >&2; exit 1; }
  for _ in {1..90}; do
    if compose "$stack" exec -T relay bash -c 'exec 3<>/dev/tcp/127.0.0.1/8080; printf "GET /_readiness HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n" >&3; grep -q "200 OK" <&3' >/dev/null 2>&1; then
      node "$ROOT/scripts/upstream-buzz-poc/tls-proxy.mjs" "$RUN_DIR/tls.crt" "$RUN_DIR/tls.key" "$tls_port" "$port" >"$RUN_DIR/$stack/proxy.log" 2>&1 &
      PROXIES+=("$!")
      for _ in {1..20}; do
        if curl -fsS --cacert "$RUN_DIR/tls.crt" --max-time 3 -H 'Accept: application/nostr+json' "https://127.0.0.1:$tls_port/" >/dev/null 2>&1; then return; fi
        sleep 1
      done
      echo "TLS proxy $stack did not become ready" >&2
      exit 1
    fi
    sleep 2
  done
  echo "Stack $stack did not become ready" >&2
  compose "$stack" ps >&2
  exit 1
}

PORT_A="$(free_port)"
PORT_B="$(free_port)"
TLS_A="$(free_port)"
TLS_B="$(free_port)"
[[ "$(printf '%s\n' "$PORT_A" "$PORT_B" "$TLS_A" "$TLS_B" | sort -u | wc -l)" == 4 ]] || { echo 'Port collision; rerun' >&2; exit 1; }
start_stack a "$POC_OWNER_SK" "$(new_key)" "$PORT_A" "$TLS_A"
start_stack b "$POC_B_OWNER_SK" "$(new_key)" "$PORT_B" "$TLS_B"
POC_RELAY_URL="wss://127.0.0.1:$TLS_A"
POC_RELAY_B_URL="wss://127.0.0.1:$TLS_B"
POC_WRONG_HOST_URL="wss://localhost:$TLS_A"

echo "upstream_relay_commit=$SHA"
echo "upstream_relay_tree=$TREE"
echo "upstream_image_id=$IMAGE_ID"
echo "upstream_image_build_digest=$IMAGE_BUILD_DIGEST"
echo "docker_server=$(docker version --format '{{.Server.Version}}')"
echo "compose_version=$(docker compose version --short)"
echo "node_version=$(node --version)"
echo "relay_a=$POC_RELAY_URL relay_b=$POC_RELAY_B_URL"
echo "tls_ca_path=$RUN_DIR/tls.crt"

PROTOCOL_STATUS=0
POC_OWNER_SK="$POC_OWNER_SK" POC_ALICE_SK="$POC_ALICE_SK" POC_BOB_SK="$POC_BOB_SK" \
POC_OUTSIDER_SK="$POC_OUTSIDER_SK" POC_B_OWNER_SK="$POC_B_OWNER_SK" POC_B_USER_SK="$POC_B_USER_SK" \
POC_RELAY_URL="$POC_RELAY_URL" POC_RELAY_B_URL="$POC_RELAY_B_URL" \
POC_WRONG_HOST_URL="$POC_WRONG_HOST_URL" POC_GUI_NSEC_FILE="$POC_GUI_NSEC_FILE" \
POC_TEST_PACKAGE="$RUN_DIR/test-deps/package.json" \
NODE_EXTRA_CA_CERTS="$RUN_DIR/tls.crt" G_TLS_CA_FILE="$RUN_DIR/tls.crt" \
  node "$ROOT/scripts/upstream-buzz-poc/protocol.mjs" || PROTOCOL_STATUS=$?
relay_identity() {
  curl -fsS --cacert "$RUN_DIR/tls.crt" --max-time 5 -H 'Accept: application/nostr+json' "https://127.0.0.1:$TLS_A/" |
    node -e 'let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>console.log(JSON.parse(s).self||""))'
}
BEFORE="$(relay_identity)"
compose a restart relay >/dev/null
for _ in {1..30}; do
  AFTER="$(relay_identity 2>/dev/null || true)"
  [[ -n "$AFTER" ]] && break
  sleep 2
done
if [[ -n "$BEFORE" && "$BEFORE" == "$AFTER" ]]; then
  echo "relay_identity_restart=PASS pubkey=$AFTER"
else
  echo 'relay_identity_restart=FAIL'
  exit 1
fi
if [[ "${POC_HOLD:-0}" == 1 ]]; then
  echo "GUI nsec file: $POC_GUI_NSEC_FILE (removed when this run exits)"
  read -r -p 'GUI test complete; press Enter to destroy the disposable stacks: ' _
fi
exit "$PROTOCOL_STATUS"
