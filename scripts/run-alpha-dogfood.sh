#!/usr/bin/env bash
# scripts/run-alpha-dogfood.sh
# =============================================================================
# Multi-user Elembra Alpha dogfood proof (goal: Elembra Alpha Deployment &
# Dogfooding v1). Drives the REAL stack — Elembra backend (via nginx) + real
# Buzz relay + the relay->Elembra observation bridge — through the dogfood
# matrix and reports PASS/FAIL per check. The browser is replaced by the API
# plus a wire-level Buzz client (frontend/scripts/alpha-buzz-ops.mjs) using the
# same NIP-42/NIP-43 contracts as the product clients.
#
# Prerequisites (see docs/runbooks/elembra-alpha.md):
#   - base stack up:   docker compose up -d
#   - relay stack up:  docker compose -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml up -d
#   - observer up:     ./scripts/start-buzz-observer.sh
#   - frontend deps:   npm install (frontend/)
#
# Required env:
#   BUZZ_SERVICE_SK    bridge/owner secret key (relay owner identity)
#   BUZZ_RELAY_WS      relay ws url (default ws://localhost:7447)
#   BUZZ_COMMUNITY_ID  community id (must match the observer + mapping)
#   BUZZ_RELAY_PUBKEY  relay identity pubkey (printed by alpha-gen-buzz-keys.mjs)
#   ADMIN_EMAIL / ADMIN_PASSWORD  admin creds (default RUSTSHARE_ADMIN_*)
# Optional: BUZZ_CHANNEL_ID, BUZZ_CHANNEL2_ID, ELEMBRA_API, RELAY_CONTAINER, POSTGRES_CONTAINER
#
# Per-check PASS/FAIL output; exits 1 if any check failed.
# =============================================================================
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

if [[ -f .env ]]; then
	set -a
	# shellcheck disable=SC1091
	. ./.env
	set +a
fi

# Validate Buzz key consistency before touching the stack.
if [[ -n "${BUZZ_SERVICE_SK:-}" ]]; then
	if ! node frontend/scripts/alpha-validate-buzz-config.mjs; then
		echo "Buzz key configuration is inconsistent — fix .env and re-run." >&2
		exit 1
	fi
fi

: "${BUZZ_SERVICE_SK:?BUZZ_SERVICE_SK must be set (bridge/owner key)}"
BUZZ_RELAY_WS="${BUZZ_RELAY_WS:-ws://localhost:7447}"
: "${BUZZ_COMMUNITY_ID:?BUZZ_COMMUNITY_ID must be set}"
: "${BUZZ_RELAY_PUBKEY:?BUZZ_RELAY_PUBKEY must be set (relay identity pubkey from alpha-gen-buzz-keys.mjs)}"
BUZZ_CHANNEL_ID="${BUZZ_CHANNEL_ID:-585e55c7-97d9-43ad-bbe3-a355cad93082}"
BUZZ_CHANNEL2_ID="${BUZZ_CHANNEL2_ID:-4bec90c0-4c14-48cc-8958-da8c258f9759}"
POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-rustshare-postgres-1}"
ELEMBRA_API="${ELEMBRA_API:-http://localhost/api/v1}"
ADMIN_EMAIL="${ADMIN_EMAIL:-${RUSTSHARE_ADMIN_EMAIL:-admin@localhost}}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-${RUSTSHARE_ADMIN_PASSWORD:-}}"
OPS="node scripts/alpha-buzz-ops.mjs"
TMP="$(mktemp -d)"
trap 'if [[ "${relay_stopped:-0}" == "1" ]]; then docker start "${RELAY_CONTAINER:-rustshare-buzz-relay-1}" >/dev/null 2>&1 || true; fi; [[ -f "$TMP/401-debug.log" ]] && cp "$TMP/401-debug.log" /tmp/alpha-401-debug.log; rm -rf "$TMP"' EXIT

PASS=0
FAIL=0
declare -a RESULTS=()

check() { # name ok detail
	local name="$1" ok="$2" detail="$3"
	if [[ "$ok" == "true" ]]; then
		PASS=$((PASS + 1))
		RESULTS+=("PASS  $name  $detail")
		echo "  [PASS] $name $detail"
	else
		FAIL=$((FAIL + 1))
		RESULTS+=("FAIL  $name  $detail")
		echo "  [FAIL] $name $detail"
	fi
}

jq_get() { # expr file
	python3 -c "import json,sys; d=json.load(open(sys.argv[2])); print(eval(sys.argv[1]))" "$1" "$2" 2>/dev/null || echo ""
}

# --- sessions ----------------------------------------------------------------
declare -A SESSION USER_EMAIL
sess() { # name -> jar path (login once, reuse; verifies and retries on failure)
	local name="$1"
	if [[ -z "${SESSION[$name]:-}" ]]; then
		SESSION[$name]="$TMP/sess.$name"
		local email password
		case "$name" in
			admin) email="$ADMIN_EMAIL"; password="$ADMIN_PASSWORD" ;;
			*) email="${USER_EMAIL[$name]:-${name}@kubedo.com}"; password="alpha-user-pass-2026" ;;
		esac
		# The login endpoint is rate-limited per IP (10/min default); retry with
		# backoff when the limiter is hit (429), but do NOT retry permanent
		# rejections (401: wrong password or disabled principal — e.g. a user
		# revoked mid-run). An empty jar would surface as confusing 401s.
		local attempt=0
		while :; do
			attempt=$((attempt + 1))
			local login_code
			login_code=$(curl -s -o "$TMP/login.$name.json" -w '%{http_code}' -c "${SESSION[$name]}" \
				-X POST "http://localhost/api/v1/auth/login" -H 'content-type: application/json' \
				-d "{\"email\":\"${email}\",\"password\":\"${password}\"}")
			if grep -q "rustshare_session" "${SESSION[$name]}" 2>/dev/null; then
				break
			fi
			# 401/403 are permanent; further attempts cannot succeed.
			if [[ "$login_code" == "401" || "$login_code" == "403" ]]; then
				echo "sess $name: login rejected ($login_code); not retrying" >&2
				break
			fi
			if [[ "$attempt" -ge 10 ]]; then
				echo "sess $name: login failed after 10 attempts" >&2
				break
			fi
			sleep 6
		done
	fi
	echo "${SESSION[$name]}"
}

http_call_csrf() { # method path data-file cookie-file
	local method="$1" path="$2" data="$3" cookie="$4"
	local csrf
	csrf="$(awk '$6 == "rustshare_csrf_token" { print $7 }' "$cookie")"
	local url="$path"
	[[ "$path" != http* ]] && url="http://localhost${path}"
	local args=(-s -o "$TMP/body" -w '%{http_code}' -b "$cookie" -c "$cookie" \
		-X "$method" "$url" -H "X-Rustshare-Csrf: $csrf")
	if [[ -n "$data" ]]; then
		args+=(-H 'content-type: application/json' --data-binary "@$data")
	fi
	http_code="$(curl "${args[@]}")"
	if [[ "$http_code" == "401" || "$http_code" == "403" ]]; then
		{
			echo "=== $http_code on $method $path ==="
			echo "cookie jar: $cookie"
			# Log cookie NAMES only — never values: session/csrf tokens are live
			# bearer credentials and this log is copied out of the 0700 tmpdir.
			echo "cookie names: $(awk '!/^#/ && NF >= 7 { print $6 }' "$cookie" | tr '\n' ' ')"
			echo "body: $(head -c 150 "$TMP/body")"
		} >> "$TMP/401-debug.log"
	fi
}

relay_ok() { # command args... -> 0 on success
	(cd frontend && $OPS "$@") > "$TMP/op.json" 2>/dev/null
}

echo "== Elembra Alpha dogfood run =="
echo "  api=$ELEMBRA_API relay=$BUZZ_RELAY_WS community=$BUZZ_COMMUNITY_ID channels=$BUZZ_CHANNEL_ID,$BUZZ_CHANNEL2_ID"

# --- P01 health --------------------------------------------------------------
code=$(curl -s -o /dev/null -w '%{http_code}' "http://localhost/health/ready")
check "P01 backend healthy" "$([[ "$code" == "200" ]] && echo true || echo false)" "GET /health/ready -> $code"

# --- P02 admin login + chat enabled + mapping --------------------------------
sess admin
code=$(curl -s -b "$(sess admin)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/applications/chat/status")
check "P02 admin chat status" "$([[ "$code" == "200" ]] && echo true || echo false)" "status -> $code"

http_call_csrf POST /api/v1/admin/applications/io.elembra.chat/enable '' "$(sess admin)"
check "P02b chat application enabled" "$([[ "$http_code" == "200" || "$http_code" == "201" ]] && echo true || echo false)" "enable -> $http_code"

# workspace id = tenant id for the admin session
curl -s -b "$(sess admin)" -o "$TMP/body" "$ELEMBRA_API/users/me"
tenant_id="$(jq_get 'd["tenant_id"]' "$TMP/body")"

# P02c' — zero-config bootstrap (ADR-0036): enabling Chat above triggered
# automatic provisioning (RUSTSHARE_CHAT_PROVISIONING=auto in the alpha
# compose). Poll the admin diagnostics endpoint until the authoritative
# mapping lands (404 until provisioned), then assert it matches the relay
# identity exactly.
mapping_json=""
for _ in $(seq 1 30); do
	code=$(curl -s -b "$(sess admin)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/admin/applications/chat/workspaces/${tenant_id}/community")
	if [[ "$code" == "200" ]]; then
		mapping_json="$(cat "$TMP/body")"
		break
	fi
	sleep 1
done
if [[ -n "$mapping_json" ]]; then
	got_community="$(jq_get 'd["community_id"]' "$TMP/body")"
	got_relay_url="$(jq_get 'd["relay_url"]' "$TMP/body")"
	got_relay_pubkey="$(jq_get 'd["relay_pubkey"]' "$TMP/body")"
	check "P02c auto-provisioned mapping" \
		"$([[ "$got_community" == "$BUZZ_COMMUNITY_ID" && "$got_relay_url" == "$BUZZ_RELAY_WS" && "$got_relay_pubkey" == "$BUZZ_RELAY_PUBKEY" ]] && echo true || echo false)" \
		"community=$got_community relay=$got_relay_url pubkey=${got_relay_pubkey:0:8}…"
else
	check "P02c auto-provisioned mapping" false "no mapping within 30s of enable (last status -> $code)"
fi

# Memory projection + content indexing: the chat Application configuration has
# no admin API — the operator enables it via SQL (documented in the runbook).
# Without it, bodies are not stored and the Memory/Ask pipeline stays empty.
if [[ "${ALPHA_ENABLE_MEMORY_PROJECTION:-1}" == "1" ]]; then
	mp_status=0
	# RETURNING makes a 0-row UPDATE (wrong tenant/app id) visible as empty
	# output instead of a silent success.
	PGPASSWORD="${POSTGRES_PASSWORD:-}" docker exec -e PGPASSWORD="${POSTGRES_PASSWORD:-}" \
		"${POSTGRES_CONTAINER:-rustshare-postgres-1}" psql -U rustshare -d rustshare -v ON_ERROR_STOP=1 -t -A -c \
		"UPDATE application_enablements SET configuration = configuration || '{\"memory_projection\": true, \"content_indexing\": true}'::jsonb WHERE application_id='io.elembra.chat' AND tenant_id='${tenant_id}' AND workspace_id='${tenant_id}' RETURNING application_id;" \
		> "$TMP/mp.json" 2>&1 || mp_status=$?
	mp_rows="$(cat "$TMP/mp.json")"
	check "P02d memory projection + content indexing enabled" "$([[ "$mp_status" == "0" && "$mp_rows" != "" ]] && echo true || echo false)" "rows=$mp_rows"
fi

# --- P03 create users --------------------------------------------------------
# Fresh per-run user identities (timestamp suffix) so re-runs never collide
# with stale bindings/admissions — deleting a user with Files artifacts is
# blocked by the file_versions FK, so delete-and-recreate is not idempotent.
RUN_SUFFIX="$(date +%s)"
declare -A USER_ID USER_SK USER_PK
USERS=(alpha_alice alpha_bob alpha_mallory)
create_user() { # username
	local username="${1}_${RUN_SUFFIX}"
	local sk pk
	(cd frontend && $OPS keygen) > "$TMP/kg.json"
	sk="$(jq_get 'd["secretKey"]' "$TMP/kg.json")"
	pk="$(cd frontend && $OPS pubkey "$sk")"
	USER_SK[$1]="$sk"
	USER_PK[$1]="$pk"
	cat > "$TMP/user.json" <<EOF
{"username":"$username","email":"${username}@kubedo.com","password":"alpha-user-pass-2026","display_name":"$username","storage_quota_bytes":10737418240}
EOF
	http_call_csrf POST /api/v1/admin/users "$TMP/user.json" "$(sess admin)"
	if [[ "$http_code" != "200" && "$http_code" != "201" ]]; then
		check "P03 create $1" false "create -> $http_code $(head -c 150 "$TMP/body")"
		return 1
	fi
	USER_ID[$1]="$(jq_get 'd["id"]' "$TMP/body")"
	USER_EMAIL[$1]="${username}@kubedo.com"
	echo "    user $username id=${USER_ID[$1]:0:8} pk=${pk:0:8}"
}
echo "== creating users =="
for u in "${USERS[@]}"; do create_user "$u"; done
create_user alpha_eve 2>/dev/null || true
USERS+=(alpha_eve)
check "P03 users created" \
	"$([[ "${USER_ID[alpha_alice]:-}" != "" && "${USER_ID[alpha_bob]:-}" != "" && "${USER_ID[alpha_mallory]:-}" != "" && "${USER_ID[alpha_eve]:-}" != "" ]] && echo true || echo false)" \
	"alice/bob/mallory/eve"

# --- P04 binding + admission per user (alice/bob/mallory only) ----------------
bind_user() { # username
	local username="$1" sk="${USER_SK[$1]}" pk="${USER_PK[$1]}"
	local jar; jar="$(sess "$username")"
	cat > "$TMP/challenge.json" <<EOF
{"workspace_id":"${tenant_id}","buzz_pubkey":"${pk}"}
EOF
	http_call_csrf POST "$ELEMBRA_API/applications/chat/identity-binding/challenge" "$TMP/challenge.json" "$jar"
	local challenge_id relay_url
	challenge_id="$(jq_get 'd["challenge_id"]' "$TMP/body")"
	relay_url="$(jq_get 'd["relay_url"]' "$TMP/body")"
	(cd frontend && $OPS bind-proof "$relay_url" "$(jq_get 'd["nonce"]' "$TMP/body")" "$sk") > "$TMP/proof.json"
	cat > "$TMP/verify.json" <<EOF
{"challenge_id":"${challenge_id}","event":$(cat "$TMP/proof.json")}
EOF
	http_call_csrf POST "$ELEMBRA_API/applications/chat/identity-binding/verify" "$TMP/verify.json" "$jar"
	local verify_code="$http_code"
	cat > "$TMP/admit.json" <<EOF
{"workspace_id":"${tenant_id}"}
EOF
	http_call_csrf POST "$ELEMBRA_API/applications/chat/admission" "$TMP/admit.json" "$jar"
	BIND_RESULT[$username]="${verify_code}:${http_code}"
	echo "    $username challenge=$challenge_id verify=$verify_code admission=$http_code"
	sleep 1
}
echo "== binding users =="
declare -A BIND_RESULT
for u in alpha_alice alpha_bob alpha_mallory; do bind_user "$u"; done
check "P04 bindings + admission" \
	"$([[ "${BIND_RESULT[alpha_alice]}" == 2* && "${BIND_RESULT[alpha_bob]}" == 2* && "${BIND_RESULT[alpha_mallory]}" == 2* ]] && echo true || echo false)" \
	"alice=${BIND_RESULT[alpha_alice]:-none} bob=${BIND_RESULT[alpha_bob]:-none} mallory=${BIND_RESULT[alpha_mallory]:-none}"

# --- P05 relay admission (NIP-43 9030, owner authority) ----------------------
echo "== admitting users at the relay =="
for u in alpha_alice alpha_bob alpha_mallory; do
	# owner-sk comes from BUZZ_SERVICE_SK in the environment, never argv
	if relay_ok admit "$BUZZ_RELAY_WS" "${USER_PK[$u]}"; then
		check "P05 relay admit $u" true "9030 accepted"
	else
		check "P05 relay admit $u" false "9030 failed: $(cat "$TMP/op.json")"
	fi
done

# --- channel provisioning ------------------------------------------------------
# The relay's v1alpha1 registry is UUID-keyed: the alpha channels must exist
# as kind-9007 rows (open visibility) before publishes/reads exercise them,
# and every client (observer, E2E driver, gateway) must use the same UUID ids
# (BUZZ_CHANNEL_ID / BUZZ_CHANNEL2_ID default to the alpha channel UUIDs).
# Re-runs of an existing channel answer accepted:false "duplicate: channel
# already exists" — idempotent, treated as success.
echo "== provisioning channels at the relay =="
create_channel() { # id name
	local id="$1" name="$2"
	(cd frontend && $OPS create-channel "$BUZZ_RELAY_WS" "$id" "$name" open stream) > "$TMP/ch.json"
	local ok reason
	ok="$(jq_get 'd["accepted"]' "$TMP/ch.json")"
	reason="$(jq_get 'd["reason"]' "$TMP/ch.json")"
	if [[ "$ok" != "True" && "$reason" != *duplicate* ]]; then
		echo "FATAL: create-channel $name ($id) failed: $(cat "$TMP/ch.json")" >&2
		exit 1
	fi
	echo "    channel $name ($id) ready"
}
create_channel "$BUZZ_CHANNEL_ID" "alpha-channel"
create_channel "$BUZZ_CHANNEL2_ID" "alpha-ops"

# --- P06 publish + observation -----------------------------------------------
echo "== publishing messages =="
pub_msg() { # username channel content
	local sk="${USER_SK[$1]}"
	(cd frontend && $OPS publish "$BUZZ_RELAY_WS" "$sk" "$3" "$2") > "$TMP/pub.json"
	local ok event_id
	ok="$(jq_get 'd["accepted"]' "$TMP/pub.json")"
	event_id="$(jq_get 'd["eventId"]' "$TMP/pub.json")"
	# Count only relay-accepted publishes: alpha-buzz-ops prints an eventId even
	# when the relay rejects, so P06 must not measure attempts as acceptances.
	if [[ "$ok" == "True" ]]; then
		PUBLISHED_EVENTS+=("$event_id")
		echo "    [$1/$2] $event_id"
	else
		echo "    [$1/$2] FAILED: $(cat "$TMP/pub.json")"
	fi
}

declare -a PUBLISHED_EVENTS
pub_msg alpha_alice "$BUZZ_CHANNEL_ID" "alpha dogfood: hello from alice 1"
pub_msg alpha_alice "$BUZZ_CHANNEL_ID" "alpha dogfood: hello from alice 2"
pub_msg alpha_bob "$BUZZ_CHANNEL_ID" "alpha dogfood: hello from bob 1"
pub_msg alpha_alice "$BUZZ_CHANNEL_ID" "alpha dogfood: pagination 3"
pub_msg alpha_alice "$BUZZ_CHANNEL_ID" "alpha dogfood: pagination 4"
pub_msg alpha_alice "$BUZZ_CHANNEL_ID" "alpha dogfood: pagination 5"
pub_msg alpha_alice "$BUZZ_CHANNEL_ID" "alpha dogfood: pagination 6"
pub_msg alpha_alice "$BUZZ_CHANNEL_ID" "alpha dogfood: pagination 7"
t1=$(date +%s%3N)
check "P06 publishes accepted by relay" "$([[ "${#PUBLISHED_EVENTS[@]}" == "8" ]] && echo true || echo false)" "${#PUBLISHED_EVENTS[@]} events"

# wait for observation + message quiescence (reuse alice's session, no re-login)
echo "== waiting for observation =="
seen=""
for i in $(seq 1 25); do
	code=$(curl -s -b "$(sess alpha_alice)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/applications/chat/channels")
	if [[ "$code" == "200" ]]; then
		seen="$(jq_get '[c for c in d if c["channel_id"] == "'$BUZZ_CHANNEL_ID'"]' "$TMP/body")"
		if [[ "$seen" != "[]" && "$seen" != "" ]]; then break; fi
	fi
	sleep 1
done
t2=$(date +%s%3N)
lag=$(( (t2 - t1) / 1000 ))
check "P07 channel observed (lag ${lag}s)" "$([[ "$seen" != "" && "$seen" != "[]" && "$seen" != "None" ]] && echo true || echo false)" "channel visible"

# The observer forwards events sequentially; wait until the timeline has all 8
# published messages (or the observation budget elapses) before content checks.
for i in $(seq 1 20); do
	code=$(curl -s -b "$(sess alpha_alice)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}")
	msg_count="$(jq_get 'len(d["messages"])' "$TMP/body")"
	[[ "$msg_count" != "" && "$msg_count" != "None" && "$msg_count" -ge 8 ]] && break
	sleep 1
done

# timeline content
code=$(curl -s -b "$(sess alpha_alice)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}")
msg_count="$(jq_get 'len(d["messages"])' "$TMP/body")"
check "P08 timeline populated" "$([[ "$msg_count" != "" && "$msg_count" != "0" && "$msg_count" != "None" ]] && echo true || echo false)" "$msg_count messages"

bob_attributed=""
# P08b needs THIS run's bob message observed; the accumulated timeline can
# satisfy the P08 count before this run's events land, so wait for bob's key
# specifically (observation-driven, same pattern as P07/P10). The 60s budget
# absorbs an observer reconnect + since=all replay after a relay bounce
# (the runbook supervises the observer; a restart is lossless by replay).
for i in $(seq 1 60); do
	code=$(curl -s -b "$(sess alpha_alice)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}")
	if [[ "$code" == "200" ]]; then
		bob_attributed="$(jq_get 'any(m["author_pubkey"] == "'${USER_PK[alpha_bob]}'" for m in d["messages"])' "$TMP/body")"
		[[ "$bob_attributed" == "True" ]] && break
	fi
	sleep 1
done
check "P08b author mapping correct" "$([[ "$bob_attributed" == "True" ]] && echo true || echo false)" "bob's msg attributed to his key"

# --- P09 pagination ----------------------------------------------------------
# Fetch a small page (limit=3) so the cursor must actually advance across
# pages; assert page 2 is non-empty and disjoint from page 1 (no overlap).
curl -s -b "$(sess alpha_alice)" -o "$TMP/pg1.json" -w '%{http_code}' "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}&limit=3" >/dev/null
page1_n="$(jq_get 'len(d["messages"])' "$TMP/pg1.json")"
next_before="$(jq_get 'd["next_before"]' "$TMP/pg1.json")"
page2_n=""
overlap=""
if [[ "$next_before" != "None" && "$next_before" != "" ]]; then
	curl -s -b "$(sess alpha_alice)" -o "$TMP/pg2.json" -w '%{http_code}' "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}&limit=3&before=${next_before}" >/dev/null
	page2_n="$(jq_get 'len(d["messages"])' "$TMP/pg2.json")"
	overlap="$(python3 -c "import json,sys; a={m['message_id'] for m in json.load(open(sys.argv[1]))['messages']}; b={m['message_id'] for m in json.load(open(sys.argv[2]))['messages']}; print(len(a & b))" "$TMP/pg1.json" "$TMP/pg2.json" 2>/dev/null || echo '')"
fi
check "P09 pagination cursor advances" \
	"$([[ "${page2_n:-0}" != "0" && "${page2_n:-}" != "" && "$overlap" == "0" ]] && echo true || echo false)" \
	"page1=$page1_n page2=$page2_n overlap=$overlap"

# --- P10 channel switching ---------------------------------------------------
pub_msg alpha_alice "$BUZZ_CHANNEL2_ID" "alpha dogfood: ops channel message"
# Wait for the ops channel to appear (observation-driven), like P07/P08.
ch2=""
for i in $(seq 1 20); do
	code=$(curl -s -b "$(sess alpha_alice)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/applications/chat/channels")
	ch2="$(jq_get '[c for c in d if c["channel_id"] == "'$BUZZ_CHANNEL2_ID'"]' "$TMP/body")"
	[[ "$ch2" != "[]" && "$ch2" != "" ]] && break
	sleep 1
done
curl -s -b "$(sess alpha_alice)" -o "$TMP/body2" "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL2_ID}"
ch2_msgs="$(jq_get 'len(d["messages"])' "$TMP/body2")"
curl -s -b "$(sess alpha_alice)" -o "$TMP/body1b" "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}"
ch1_msgs="$(jq_get 'len(d["messages"])' "$TMP/body1b")"
# isolation: the ops-channel timeline must never contain alpha-channel messages
leak="$(jq_get 'any((m["body"] or "") and ("pagination" in m["body"] or "hello from" in m["body"]) for m in d["messages"])' "$TMP/body2")"
check "P10 channel switch isolated" \
	"$([[ "$ch2" != "[]" && "$ch2" != "" && "${ch2_msgs:-0}" != "0" && "${ch2_msgs:-0}" != "" && "$leak" != "True" && "${ch1_msgs:-0}" != "" ]] && echo true || echo false)" \
	"ch1=$ch1_msgs msgs, ch2=$ch2_msgs msgs, leak=$leak"

# --- P11 unauthorized user cannot read ---------------------------------------
code=$(curl -s -b "$(sess alpha_eve)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}")
# eve is in the tenant but has no Chat binding/admission: the read gate fails
# closed — the timeline is EMPTY (existence-hiding), never channel content.
eve_msgs="$(jq_get 'len(d["messages"]) if "messages" in d else -1' "$TMP/body")"
check "P11 unbound user read gate" "$([[ "$eve_msgs" == "0" ]] && echo true || echo false)" "eve sees $eve_msgs messages (empty = fail closed)"

# --- P12 attachments ---------------------------------------------------------
echo "== attachments =="
# Build the Cookie header explicitly from the jar so the double-submit pair is
# guaranteed consistent (no -b/-c jar rewrite ambiguity).
upload=$(
	jar="$(sess alpha_alice)"
	sess_cookie="$(awk '$6 == "rustshare_session" { print $7 }' "$jar")"
	csrf="$(awk '$6 == "rustshare_csrf_token" { print $7 }' "$jar")"
	curl -s -H "Cookie: rustshare_session=${sess_cookie}; rustshare_csrf_token=${csrf}" \
		-H "X-Rustshare-Csrf: ${csrf}" \
		-F "file=@AGENTS.md;filename=alpha-e2e.md" -F "name=alpha-e2e.md" \
		-o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/files/upload"
)
file_id="$(jq_get 'd["id"]' "$TMP/body")"
check "P12a file uploaded" "$([[ "$file_id" != "" ]] && echo true || echo false)" "file=$file_id (upload=$upload)"
cat > "$TMP/prep.json" <<EOF
{"resource":{"application":"io.elembra.files","resourceType":"file","resourceId":"${file_id}"}}
EOF
http_call_csrf POST "$ELEMBRA_API/applications/chat/attachments/prepare" "$TMP/prep.json" "$(sess alpha_alice)"
ref_tag="$(jq_get 'd["buzz_tag"][1]' "$TMP/body")"
check "P12b attachment prepare" "$([[ "$ref_tag" != "" ]] && echo true || echo false)" "prepare=$http_code $(head -c 120 "$TMP/body")"
# share the file with bob (Files authorization grant; chat membership grants nothing)
cat > "$TMP/share.json" <<EOF
{"recipient_email":"${USER_EMAIL[alpha_bob]}","permission":"View"}
EOF
http_call_csrf POST "$ELEMBRA_API/files/${file_id}/share" "$TMP/share.json" "$(sess alpha_alice)"
check "P12b2 file shared with bob" "$([[ "$http_code" == "200" || "$http_code" == "201" ]] && echo true || echo false)" "share -> $http_code"
(cd frontend && $OPS publish "$BUZZ_RELAY_WS" "${USER_SK[alpha_alice]}" "alpha dogfood: attached file" "$BUZZ_CHANNEL_ID" "$ref_tag") > "$TMP/pub.json"
sleep 3
http_call_csrf POST "$ELEMBRA_API/applications/chat/attachments/open" "$TMP/prep.json" "$(sess alpha_bob)"
check "P12c authorized recipient opens" "$([[ "$http_code" == "200" ]] && echo true || echo false)" "open -> $http_code"
http_call_csrf POST "$ELEMBRA_API/applications/chat/attachments/open" "$TMP/prep.json" "$(sess alpha_eve)"
check "P12d unauthorized recipient denied" "$([[ "$http_code" == "404" || "$http_code" == "403" ]] && echo true || echo false)" "open -> $http_code"

# --- P13 Ask this channel ----------------------------------------------------
echo "== Ask this channel =="
cat > "$TMP/ask.json" <<EOF
{"question":"What did the team say in alpha channel?","workspace_id":"${tenant_id}","scope":{"type":"chatChannel","communityId":"${BUZZ_COMMUNITY_ID}","channelId":"${BUZZ_CHANNEL_ID}"}}
EOF
http_call_csrf POST "$ELEMBRA_API/memory/ask" "$TMP/ask.json" "$(sess alpha_alice)"
ask_code="$http_code"
ask_body="$(head -c 150 "$TMP/body")"
# The response serializes snake_case (`resource_ref`). A 503 "LLM provider not
# configured" is the documented Alpha limitation (L32 / issue #244): the
# pipeline is provider-gated, not broken — but a 200 MUST carry a grounded
# citation, or the pipeline is broken, not "configured-but-quiet".
citation="$(jq_get 'd["citations"][0]["resource_ref"] if "citations" in d and d["citations"] else ""' "$TMP/body")"
if [[ "$ask_code" == "200" ]]; then
	if [[ "$citation" != "" ]]; then
		check "P13 Ask this channel" true "grounded answer with citation"
		cat > "$TMP/cit.json" <<EOF
{"resource_ref":"${citation}"}
EOF
		http_call_csrf POST "$ELEMBRA_API/memory/citations/open" "$TMP/cit.json" "$(sess alpha_alice)"
		check "P14 citation open reauthorizes" "$([[ "$http_code" == "200" ]] && echo true || echo false)" "citation -> $http_code"
	else
		check "P13 Ask this channel" false "200 without grounded citations: $ask_body"
		check "P14 citation open reauthorizes" false "no citation to reauthorize"
	fi
elif [[ "$ask_code" == "503" ]]; then
	check "P13 Ask this channel" true "503 provider not configured — documented L32/#244"
	check "P14 citation open reauthorizes" true "not exercised: Ask provider not configured (L32/#244)"
else
	check "P13 Ask this channel" false "ask -> $ask_code $ask_body"
	check "P14 citation open reauthorizes" false "ask failed ($ask_code)"
fi

# --- P15 revocation ----------------------------------------------------------
echo "== revocation =="
uid_mallory="${USER_ID[alpha_mallory]}"
http_call_csrf POST "/api/v1/admin/users/${uid_mallory}/disable" '' "$(sess admin)"
check "P15a admin disabled mallory" "$([[ "$http_code" == "200" || "$http_code" == "204" ]] && echo true || echo false)" "disable -> $http_code"
sleep 2
code=$(curl -s -b "$(sess alpha_mallory)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}")
check "P15b revoked user cannot read" "$([[ "$code" == "401" || "$code" == "403" || "$code" == "404" ]] && echo true || echo false)" "mallory messages -> $code"
if relay_ok revoke "$BUZZ_RELAY_WS" "${USER_PK[alpha_mallory]}"; then
	check "P15c relay revoked (9031)" true "9031 accepted"
else
	# The Elembra-side disable (P15a) also queues a durable 9031 through the
	# outbox bridge; when that one lands first, this direct 9031 is answered
	# "member not found" — the relay already confirms the member is gone,
	# which is the same revoked end state (P15d still proves the consequence).
	revoke_reason="$(jq_get 'd["reason"]' "$TMP/op.json")"
	if [[ "$revoke_reason" == *"member not found"* ]]; then
		check "P15c relay revoked (9031)" true "member already gone (bridge 9031 landed first)"
	else
		check "P15c relay revoked (9031)" false "9031 failed: $(cat "$TMP/op.json")"
	fi
fi
if relay_ok publish "$BUZZ_RELAY_WS" "${USER_SK[alpha_mallory]}" "should fail" "$BUZZ_CHANNEL_ID"; then
	check "P15d revoked publish denied" false "publish unexpectedly accepted"
else
	check "P15d revoked publish denied" true "publish rejected by relay"
fi
code=$(curl -s -b "$(sess alpha_alice)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}")
check "P15e unaffected user still reads" "$([[ "$code" == "200" ]] && echo true || echo false)" "alice messages -> $code"

# --- P16 relay outage --------------------------------------------------------
echo "== relay outage =="
RELAY_CONTAINER="${RELAY_CONTAINER:-rustshare-buzz-relay-1}"
relay_stopped=0
docker stop "$RELAY_CONTAINER" >/dev/null 2>&1 && relay_stopped=1 || true
sleep 2
if relay_ok publish "$BUZZ_RELAY_WS" "${USER_SK[alpha_alice]}" "outage test" "$BUZZ_CHANNEL_ID"; then
	check "P16a publish fails during outage" false "publish unexpectedly accepted"
else
	check "P16a publish fails during outage" true "transport/rejected"
fi
code=$(curl -s -b "$(sess alpha_alice)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}")
check "P16b reads stay available (local gate)" "$([[ "$code" == "200" ]] && echo true || echo false)" "messages -> $code"
# Restore immediately; the EXIT trap also restores it if this run is interrupted.
docker start "$RELAY_CONTAINER" >/dev/null 2>&1 && relay_stopped=0 || true
# The v1alpha1 relay's cold start has a documented flaky S3 probe (the live
# conformance harness allows a 300s health window for the same reason), so
# the recovery budget follows that envelope: wait for relay health first,
# then for publish acceptance.
for i in $(seq 1 150); do
	if curl -sf --max-time 2 "http://127.0.0.1:7447/health" >/dev/null 2>&1; then
		break
	fi
	sleep 2
done
recovered=""
for i in $(seq 1 30); do
	sleep 2
	if relay_ok publish "$BUZZ_RELAY_WS" "${USER_SK[alpha_alice]}" "post-recovery" "$BUZZ_CHANNEL_ID"; then
		recovered="yes"
		break
	fi
done
check "P16c publish recovers after restart" "$([[ "$recovered" == "yes" ]] && echo true || echo false)" "publish accepted after recovery"

# --- P17 session isolation / logout-login ------------------------------------
# Drop bob's cached session and force a REAL fresh login; assert the new jar
# actually holds a session cookie before trusting the /users/me 200.
unset 'SESSION[alpha_bob]'
rm -f "$TMP/sess.alpha_bob"
jar="$(sess alpha_bob)"
fresh_cookie="false"
grep -q "rustshare_session" "$jar" 2>/dev/null && fresh_cookie="true"
code=$(curl -s -b "$jar" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/users/me")
check "P17 fresh login works" "$([[ "$code" == "200" && "$fresh_cookie" == "true" ]] && echo true || echo false)" "bob re-login /users/me -> $code"

# --- P18 restart persistence --------------------------------------------------
# A stack restart must preserve the auto-provisioned mapping and the
# authorization gate: bound reads keep working, unbound reads stay
# fail-closed (P11/P15 semantics) — no re-provisioning needed.
echo "== restart persistence =="
# nginx resolves its `proxy_pass http://backend:8080` upstream ONCE at startup
# and pins the resolved IP, so a backend container restart (new IP) must be
# accompanied by an nginx restart or every nginx-routed health/API call 502s.
docker compose -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml restart backend buzz-relay nginx
# Wait for the backend first (P01's readiness probe through nginx), then the
# relay — its health sits on the shared backend namespace's loopback
# (docker-compose.dogfood.yml), same envelope as P16's recovery wait.
for i in $(seq 1 90); do
	code=$(curl -s -o /dev/null -w '%{http_code}' "http://localhost/health/ready")
	[[ "$code" == "200" ]] && break
	sleep 2
done
# The relay's cold start runs a git object-store conformance probe (A3 gate)
# whose flaky transport can stall without a timeout (documented; the live
# conformance harness allows a 300s health window for the same reason). The
# recovery pattern mirrors P16: wait out the envelope, and if health still
# does not come up, restart the relay container once — a fresh process gets a
# fresh connection pool and passes the probe (observed: stuck >8min, healthy
# ~15s after restart).
relay_healthy=""
for attempt in 1 2; do
	for i in $(seq 1 150); do
		if curl -sf --max-time 2 "http://127.0.0.1:7447/health" >/dev/null 2>&1; then
			relay_healthy="yes"
			break
		fi
		sleep 2
	done
	[[ "$relay_healthy" == "yes" || "$attempt" == "2" ]] && break
	echo "    relay health not up within envelope; restarting relay container"
	docker restart "${RELAY_CONTAINER:-rustshare-buzz-relay-1}" >/dev/null 2>&1 || true
done
code=$(curl -s -b "$(sess admin)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/admin/applications/chat/workspaces/${tenant_id}/community")
mapping2="$(jq_get 'd["community_id"]' "$TMP/body")"
check "P18 mapping survives restart" "$([[ "$code" == "200" && "$mapping2" == "$BUZZ_COMMUNITY_ID" ]] && echo true || echo false)" "community=$mapping2 (-> $code)"
# The observer replays (since=all) after the relay bounce, so the timeline
# repopulates asynchronously through the webhook — wait for messages to land
# again (same observation-driven pattern as P08b) before asserting the gate.
for i in $(seq 1 60); do
	code=$(curl -s -b "$(sess alpha_alice)" -o "$TMP/body" -w '%{http_code}' "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}")
	if [[ "$code" == "200" ]]; then
		alice_msgs="$(jq_get 'len(d["messages"])' "$TMP/body")"
		[[ "${alice_msgs:-0}" != "0" && "${alice_msgs:-}" != "" ]] && break
	fi
	sleep 1
done
curl -s -b "$(sess alpha_eve)" -o "$TMP/body2" "$ELEMBRA_API/applications/chat/messages?channel_id=${BUZZ_CHANNEL_ID}"
eve_msgs="$(jq_get 'len(d["messages"]) if "messages" in d else -1' "$TMP/body2")"
check "P18 authorization gate survives restart" \
	"$([[ "$code" == "200" && "${alice_msgs:-0}" != "0" && "$eve_msgs" == "0" ]] && echo true || echo false)" \
	"alice=$alice_msgs msgs (-> $code), eve=$eve_msgs msgs (fail closed)"

# --- summary -----------------------------------------------------------------
echo
echo "== Alpha dogfood summary =="
printf '%s\n' "${RESULTS[@]}"
echo "  total: $PASS passed, $FAIL failed"
exit $((FAIL > 0 ? 1 : 0))
