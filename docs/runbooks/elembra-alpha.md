# Elembra Alpha Deployment & Dogfooding Runbook

> **Audience:** Operators running the Elembra Alpha dogfooding deployment
> **Scope:** Base stack + Buzz relay runtime + observation bridge (this goal)
> **Contract:** `docs/architecture/elembra-chat-alpha-readiness.md` (Alpha contract A1–A20)

This runbook documents a reproducible Elembra Alpha environment: a clean
operator brings up the complete stack — Elembra (backend + frontend), Postgres,
RustFS, the Buzz relay runtime, and the relay→Elembra observation bridge — then
onboards real users who can use Files + Chat + Memory + Ask together.

---

## 1. Architecture (what is running)

```
Browser ── nginx :80 ── backend :8080 ── postgres :5432
                              │            rustfs :9000
                              │
Browser ── ws://localhost:7447 ── buzz-relay ── buzz-postgres / buzz-redis / buzz-minio
                              ▲
        buzz-observer (host) ─┘   (NIP-42 AUTH + REQ, forwards the observed
                                  kinds — stream messages 9/40002 and legacy
                                  kind-1 — to POST /api/v1/integrations/buzz/events)
```

Trust boundaries and data flow: see the Alpha readiness doc §1–§2.

### Components

| Component | Source | Runs as |
|---|---|---|
| Elembra backend/frontend | this repo (`docker/backend.Dockerfile`) | `docker compose up -d` |
| Postgres / RustFS / nginx | `docker-compose.yml` | same |
| Buzz relay + backing services | `ghcr.io/kubedoio/buzz` (main-built; pin a `sha-<7>` tag, see §3) | `docker compose -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml up -d` |
| Observation bridge | `frontend/scripts/buzz-observer.mjs` | host process via `scripts/start-buzz-observer.sh` |

**Why the observer runs on the host:** Buzz resolves the community from the
connection *host* at row zero (ADR-0034). Browsers connect to
`ws://localhost:7447`, so the observer must use the same host to observe the
same community; an internal container network cannot present that Host value.

---

## 2. Deployment (exact setup)

### 2.1 Prerequisites

- Docker Engine + Compose plugin (validated matrix: Ubuntu 22.04/24.04, Debian 12)
- Node.js 22+ (for the observer and E2E driver)
- An image of the Buzz relay: `ghcr.io/kubedoio/buzz`, built from merged
  `kubedoio/buzz` main and published by the fork's `docker.yml` workflow with
  `:main` + immutable `:sha-<7>` tags and provenance attestation. Pin
  `BUZZ_RELAY_IMAGE` to the `sha-<7>` tag of the merged-main build (see §3);
  never use a floating upstream `block/buzz` image — its API contract is stale
  or absent.

### 2.2 Bring up

```bash
# 1. Secrets
cp .env.example .env
./scripts/pre-flight.sh

# 2. Frontend deps (required for the key generator, observer, and E2E driver)
npm install --prefix frontend

# 3. Generate the Buzz identity keys (prints values; paste into .env)
node frontend/scripts/alpha-gen-buzz-keys.mjs
#    -> BUZZ_RELAY_OWNER_PUBKEY, BUZZ_RELAY_PRIVATE_KEY,
#       BUZZ_RELAY_PUBKEY (labeled informational by the keygen, but REQUIRED
#       for buzz mode: the workspace mapping pins it),
#       RUSTSHARE_CHAT_BRIDGE_SECRET_KEY (== BUZZ_SERVICE_SK)

# 4. Configure chat in .env (see §3)
#    RUSTSHARE_CHAT_PROVISIONING=auto   (alpha default; zero-config bootstrap)
#    RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL=ws://localhost:7447   (required for auto)
#    RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY=true   (only when the relay runs on this host)
#    RUSTSHARE_ADMIN_PASSWORD=<strong-password>  (set BEFORE first start, see §4)

# 5. Base stack
docker compose up -d

# 6. Relay runtime
docker compose -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml up -d
#    (the dogfood override makes the relay reachable from the containerized
#    gateway by sharing the backend container's network namespace: the relay
#    listens on the backend's own loopback, so the gateway's pinned
#    127.0.0.1 relay address works while the Host-derived community stays
#    localhost:7447 for gateway, observer, and browsers; the relay's
#    host-published ports remain loopback-only)

# 7. Observation bridge (foreground; supervise for dogfooding)
BUZZ_SERVICE_SK=<bridge sk> BUZZ_COMMUNITY_ID=alpha-community \
  nohup ./scripts/start-buzz-observer.sh >> buzz-observer.log 2>&1 &

# 8. Admin session — required for the enable call (§2.4), which in auto mode
#    (the alpha default) auto-provisions the deployment community
#    (discover → verify → insert, idempotent; ADR-0036). Mutating API calls
#    require CSRF double-submit (cookie + X-Rustshare-Csrf header, see §6);
#    <tenant_id> equals the admin tenant id:
curl -s -c /tmp/admin.jar -X POST http://localhost/api/v1/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"admin@localhost","password":"<admin-password>"}'
CSRF="$(awk '$6 == "rustshare_csrf_token" { print $7 }' /tmp/admin.jar)"
#    Manual deployments (RUSTSHARE_CHAT_PROVISIONING=manual, the default):
#    use the admin page's "Connect existing Chat deployment" form, or the
#    existing admin API POST .../community with the CSRF header above. The
#    SSRF guard resolves the relay host, so placeholder hosts fail —
#    "wss://relay.example.com" is not a real address. Alternatively, use
#    scripts/run-alpha-dogfood.sh, which provisions everything and runs the
#    full dogfood matrix. The relay's channel registry is UUID-keyed: the
#    script creates the alpha channels there (kind-9007, open visibility) and
#    the observer/E2E driver default to those UUID channel ids
#    (BUZZ_CHANNEL_ID / BUZZ_CHANNEL2_ID in .env).

# 9. Verify
curl -s http://localhost/health/ready
tail -f buzz-observer.log     # expect "authenticated ... EOSE"
```

### 2.3 Local-relay note (dev/dogfooding on one host)

The admin mapping API, the binding challenge, AND the Buzz gateway all
validate the relay URL against the same SSRF guard
(`resolve_chat_relay_socket_addrs`). A relay on `localhost`/private
addresses is rejected **unless** `RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY=true` is
set (mirrors `RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS`; off by default). **The
flag is required for any localhost relay** — the binding challenge
re-validates the stored URL (so no row can bypass the guard, however it was
created), and the gateway needs the flag to reach a same-host relay in buzz
mode. With the flag set, `ws://localhost:7447` is accepted and the mapping/
bootstrap path works end to end.

No direct SQL is needed to create the mapping anymore: in `auto` mode
enabling Chat provisions it (§2.2 step 8), and in `manual` mode the admin
page ("Connect existing Chat deployment") or the admin API does it.

### 2.4 Operational notes (learned from the clean-install proof)

**Chat readiness is not the same as "Chat enabled".** Enabling the application
(§2.4 step 1) only flips the workspace-level toggle. A user can only send/read
messages after five independent states line up:

1. **Application enabled** — `POST /admin/applications/io.elembra.chat/enable`
   succeeded.
2. **Community mapped** — the workspace has an active `community_id` + relay
   identity (auto-provisioned in `auto` mode, or manually connected).
3. **Relay trusted-service authentication healthy** — the relay accepts the
   bridge identity used by Elembra (`RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` /
   `BUZZ_SERVICE_SK`) against its `RELAY_TRUSTED_SERVICE_PUBKEYS` allowlist.
4. **User identity bound** — the user completed the NIP-42 binding challenge.
5. **Admission active** — the relay has admitted the user (9030 delivered and
   accepted).

The admin Chat page and `applications/chat/status` distinguish these states.
"Chat enabled" with no mapping shows the provisioning UI; a mapping with no
binding shows the binding UI; only when all five states are true is Chat ready
for that user.

- **Relay network namespace**: with the dogfood override, the relay shares
  the backend container's network namespace (`network_mode: service:backend`).
  Recreating the backend (e.g. `docker compose up -d backend` after changing
  .env) orphans the relay; re-attach with
  `docker compose -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml up -d buzz-relay`.
- **`BUZZ_COMMUNITY_ID` after a relay wipe**: a fresh relay database generates
  a NEW community id (auto-provisioning discovers it). After wiping the relay
  volumes, re-read the discovered community id (backend log / chat status) and
  update `BUZZ_COMMUNITY_ID` in `.env`, then restart the observer — otherwise
  the observer forwards events that the bridge rejects with "Unknown
  community" (403).
- **Health probes**: the backend exposes `/health` and `/health/ready`
  (not `/api/v1/health`); nginx maps `/api/v1` to the backend only.
- **Ask provider**: set `ELEMBRA_LLM_API_KEY`/`BASE_URL`/`MODEL` in `.env`
  (see §3); the backend reads them at startup — recreate the backend after
  changing them. Leave the key empty for the documented gated Ask (503).

> **Security note:** `RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY` relaxes the SSRF guard
> for Chat relay URLs only. Production deployments with public relays must
> keep it unset (default fail-closed).

### 2.4 Memory projection + content indexing (required)

The Chat application must be **enabled** first (admin API, with the CSRF
header from §2.2 step 8):

```bash
curl -s -b /tmp/admin.jar -X POST \
  http://localhost/api/v1/admin/applications/io.elembra.chat/enable \
  -H "X-Rustshare-Csrf: ${CSRF}"
```

In `auto` mode (the alpha default) this enable call also auto-provisions the
workspace mapping from `RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL` (§2.2 step 8);
a provisioning failure is logged and Chat stays unconfigured but enabled —
retry from the Chat admin page.

Verify the mapping after the enable call (admin-only; returns `community_id`,
`relay_url`, `relay_pubkey`, `active`) — or check the Chat admin page at
`/admin/applications/chat`:

```bash
curl -s -b /tmp/admin.jar \
  http://localhost/api/v1/admin/applications/chat/workspaces/<tenant_id>/community
```

If auto-provisioning failed (enable succeeded but the mapping is absent),
retry with the admin page's "Set up automatically" button or:

```bash
curl -s -b /tmp/admin.jar -X POST \
  http://localhost/api/v1/admin/applications/chat/workspaces/<tenant_id>/provision \
  -H "X-Rustshare-Csrf: ${CSRF}"
```

There is **no admin API** for the chat Application *configuration*. Without
`memory_projection` and `content_indexing`, message bodies are not stored and
the Memory/Ask pipeline stays empty (the company-memory loop is dead). Enable
them once per tenant via SQL — the UPDATE must match a row; if it matches 0
rows the Chat application is not enabled for this tenant (run the enable call
above first):

```sql
UPDATE application_enablements
SET configuration = configuration || '{"memory_projection": true, "content_indexing": true}'::jsonb
WHERE application_id = 'io.elembra.chat'
  AND tenant_id = '<tenant_id>' AND workspace_id = '<tenant_id>';
```

`scripts/run-alpha-dogfood.sh` performs this step automatically (P02d).

### 2.5 Teardown

```bash
docker compose -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml down -v   # relay volumes removed
docker compose down -v                                                     # base stack + volumes
pkill -f start-buzz-observer.sh                                             # supervisor INT/TERM trap stops the node child
```

`down -v` also drops the Elembra database — for a real dogfooding period keep
the base volumes (`docker compose down` without `-v`) and only reset the
`buzz-*` volumes when a relay reset is wanted.

---

## 3. Configuration

| Variable | Purpose | Default |
|---|---|---|
| `RUSTSHARE_CHAT_AUTHORITY` | `local` (coarse community gate) or `buzz` (upstream access/check) | `local` |
| `RUSTSHARE_CHAT_PROVISIONING` | chat community provisioning mode: `auto` (zero-config bootstrap, ADR-0036) or `manual` | `manual` |
| `RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL` | relay URL (`ws://`/`wss://`) discovered for auto-provisioning; required when provisioning is `auto` | — |
| `RUSTSHARE_CHAT_WEBHOOK_SECRET` | HMAC shared with the observation bridge (required) | — |
| `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` | bridge service key: NIP-43 9030/9031 AND the gateway's NIP-98 service key (== `BUZZ_SERVICE_SK`); its public half is `BUZZ_RELAY_OWNER_PUBKEY`, which the relay also trusts via `RELAY_TRUSTED_SERVICE_PUBKEYS` | empty |
| `RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY` | allow loopback/private relay URLs (dev only) | `false` |
| `BUZZ_RELAY_IMAGE` | relay image; supported: `ghcr.io/kubedoio/buzz` built from merged `kubedoio/buzz` main — pin the immutable `sha-<7>` tag of that build; never a floating upstream `block/buzz` image | `ghcr.io/kubedoio/buzz:sha-8ce4dac` |
| `BUZZ_RELAY_OWNER_PUBKEY` | relay owner / bridge public key (relay env `RELAY_OWNER_PUBKEY` + `RELAY_TRUSTED_SERVICE_PUBKEYS`) | — |
| `BUZZ_RELAY_PRIVATE_KEY` | relay identity private key | — |
| `BUZZ_SERVICE_SK` | bridge secret key (observer AUTH + E2E driver) | — |
| `BUZZ_RELAY_WS` | relay URL browsers + observer use | `ws://localhost:7447` |
| `BUZZ_COMMUNITY_ID` | community id forwarded by the observer; must equal the mapping | — |
| `BUZZ_CHANNEL_ID` / `BUZZ_CHANNEL2_ID` | relay UUID-keyed channel ids (kind-9007 registry rows, created by `run-alpha-dogfood.sh`, open visibility) | `585e55c7-97d9-43ad-bbe3-a355cad93082` / `4bec90c0-4c14-48cc-8958-da8c258f9759` |
| `BUZZ_POSTGRES_PASSWORD`, `BUZZ_MINIO_USER`, `BUZZ_MINIO_PASSWORD` | relay backing services | `buzz_dev` / `buzz_dev` / `buzz_dev_secret` |
| `ELEMBRA_LLM_API_KEY` | OpenAI-compatible Ask provider key (DeepSeek, OpenAI, …); leave unset to keep Ask gated (`ask_available=false`, #244) | empty |
| `ELEMBRA_LLM_BASE_URL` | provider base URL, e.g. `https://api.deepseek.com/v1` | empty |
| `ELEMBRA_LLM_MODEL` | provider model id, e.g. `deepseek-chat` | `gpt-4o-mini` (app fallback) |
| `ELEMBRA_LLM_TIMEOUT_SECS` | provider request timeout | `30` |

The Ask provider is optional: with `ELEMBRA_LLM_API_KEY` unset the Chat
status surface reports `ask_available=false` and Ask returns 503 — clean,
documented degradation (never a fallback to another provider or to local
mode). Set the four variables in `.env` (never commit credentials), then
`docker compose up -d backend` to inject them. The `docker-compose.alpha.yml`
backend service passes them through from the environment.

The relay image must contain the v1alpha1 authorization API and the
community-identity discovery endpoint (ADR-0035/0036). The supported image is
`ghcr.io/kubedoio/buzz`, built from merged `kubedoio/buzz` main (`8ce4dac`, PR
#2 merged) and published by the fork's CI with `:main` + `:sha-<7>` tags and
provenance attestation. The compose default pins `BUZZ_RELAY_IMAGE` to the
immutable `sha-8ce4dac` tag of the merged build; verify provenance with
`gh attestation verify oci://ghcr.io/kubedoio/buzz:sha-8ce4dac --owner kubedoio`.

Generate all keys once: `node frontend/scripts/alpha-gen-buzz-keys.mjs`. The
relay owner key is the bridge identity: its public half is
`RELAY_OWNER_PUBKEY` on the relay, its secret half is
`RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` in Elembra and `BUZZ_SERVICE_SK` for the
observer/E2E.

In the alpha/dogfood deployment there is exactly one canonical source for the
bridge service secret: `BUZZ_SERVICE_SK`. `docker-compose.alpha.yml` sets
`RUSTSHARE_CHAT_BRIDGE_SECRET_KEY: ${BUZZ_SERVICE_SK:?...}` directly, so a
stale explicit `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` cannot silently win. Before
starting the alpha stack, validate key consistency with:

```bash
node frontend/scripts/alpha-validate-buzz-config.mjs
```

`scripts/pre-flight.sh` runs this automatically when `BUZZ_SERVICE_SK` is set;
`scripts/run-alpha-dogfood.sh` runs it as a pre-check. The validation script
derives public keys from the configured secrets and checks that:

- `BUZZ_SERVICE_SK` derives `BUZZ_RELAY_OWNER_PUBKEY`;
- `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` equals `BUZZ_SERVICE_SK` when both are set;
- `BUZZ_RELAY_PUBKEY` matches the key derived from `BUZZ_RELAY_PRIVATE_KEY`.

It never prints private secrets.

---

## 4. User onboarding

0. **Admin password**: `pre-flight.sh` warns that the admin password is NOT
   stored in `.env`. Set `RUSTSHARE_ADMIN_PASSWORD` in `.env` **before the
   first start** for a durable password; otherwise the backend generates a
   random one-time password at first boot and writes it to
   `/tmp/rustshare-bootstrap-password.txt` inside the backend container
   (`docker compose exec backend cat /tmp/rustshare-bootstrap-password.txt`).
   It does not survive container recreation.

1. **Account**: admin creates the user (API: `POST /api/v1/admin/users`, or the
   admin UI). The user logs in.
2. **Chat key**: the browser generates a Buzz key on first Chat use and encrypts
   it with a passphrase (PBKDF2 600k). Export/import:
   - Export: composer "Export key" affordance → encrypted JSON envelope.
   - Import on another device: composer import UI → envelope + passphrase.
   - Loss without export: unrecoverable by design (a new key requires a new
     binding; document to users).
3. **Binding**: challenge → NIP-42 proof → verify (all client-driven).
4. **Admission**: `POST /api/v1/applications/chat/admission` queues the durable
   9030; the Buzz bridge consumer delivers it to the relay when
   `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` is configured. Until then, admission at
   the relay must be issued with the relay admin tooling (the E2E driver does
   this via `frontend/scripts/alpha-buzz-ops.mjs admit`).

---

## 5. Health checks

| Check | Command | Healthy |
|---|---|---|
| Backend | `curl -s http://localhost/health/ready` | `"status":"ready"` |
| Relay TCP | `nc -z localhost 7447` (or `bash -c 'exec 3<>/dev/tcp/localhost/7447'` if netcat is missing) | exit 0 |
| Observer | `tail -n 5 buzz-observer.log` | recent `EOSE` and no reconnect spam |
| Ingestion | `docker logs rustshare-backend-1 | grep "buzz event rejected"` | none (or understood) |
| E2E matrix | `ADMIN_EMAIL=... ADMIN_PASSWORD=... BUZZ_SERVICE_SK=... BUZZ_COMMUNITY_ID=alpha-community ./scripts/run-alpha-dogfood.sh` | all PASS |

---

## 6. Common failure diagnosis

| Symptom | Likely cause | Action |
|---|---|---|
| Message never appears in Elembra | webhook secret drift; observer down; mapping/binding missing | check `buzz-observer.log` for `forward failed (permanent 403)`; check backend log for the rejection category (`Unknown community` / `Unbound author`) |
| Observer reconnect loop | relay down; `BUZZ_SERVICE_SK` wrong | restart relay; verify key |
| Publish "relay unreachable" | relay down; wrong `BUZZ_RELAY_WS` | relay health; browser console |
| Publish "relay rejected: …" | not admitted at the relay; revoked | check relay membership (9030 delivered); run the E2E admit step |
| Ask 503 | LLM provider not configured | configure provider; status surface `ask_available` (issue #244) |
| Channel list frozen | observer dead or WS exhaustion (fixed: 15s poll covers channels) | restart observer |
| Binding challenge 400 "relay_url must resolve to an allowed address" | local relay without `RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY=true` | set flag + restart backend |
| Buzz 401 / "service identity rejected" | bridge secret (`RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` / `BUZZ_SERVICE_SK`) does not match the relay's `RELAY_TRUSTED_SERVICE_PUBKEYS` allowlist | regenerate keys with `alpha-gen-buzz-keys.mjs`, update `.env`, run `alpha-validate-buzz-config.mjs`, then recreate backend + relay containers |
| 403 on mutating calls | missing CSRF header (browser clients get it automatically) | API tooling: send `X-Rustshare-Csrf` matching the cookie |

---

## 7. Backup considerations

- Elembra data: `scripts/backup-stack.sh` (postgres dump + RustFS + config).
- Relay state: `buzz_postgres_data` / `buzz_minio_data` volumes — back these up
  for message-history continuity. A relay reset loses the **relay's** event
  history; Elembra's observation index (its own Postgres) survives, and on
  observer reconnect the relay replays whatever events it still holds (deduped
  by event id). Events the relay no longer holds are not re-projected.
- Keys: the bridge keys live in `.env` (not in the backup bundle — store in a
  secrets manager). User Buzz keys never leave the browser; backup is the user's
  encrypted envelope.

---

## 8. Known limitations (dogfooding posture)

Full classification: Alpha readiness doc §8. Relevant here:

- Channel list = the relay's authoritative registry in buzz mode (L1
  resolved); observation-derived only under the `local` fallback.
- Reference-first bodies render placeholder (L2, by design).
- The stack runs buzz mode: per-channel membership decisions are upstream
  (L7/L9 resolved); the `local` gate remains the explicit dev fallback only.
- Recipient-side attachment tags are shipped (L5 resolved): the observation
  index retains each event's identifier-only refs (migration
  `20260810000007`), the timeline DTO surfaces them as an openable
  affordance, and opening reauthorizes through Files at read time
  (existence-hiding, forced-download headers) — see
  `docs/implementation/elembra-chat-app-v1.md` §1–§2.
- Observation relay→Elembra push is the host-side bridge (this deployment);
  upstream relay has no webhook delivery yet.
- Reload/logout clears nothing client-side; keys are per-browser vault.

---

## 9. Rollback / reset

```bash
# Stop the dogfood additions, keep Elembra:
docker compose -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml stop buzz-relay buzz-postgres buzz-redis buzz-minio
pkill -f start-buzz-observer.sh

# Full reset (nuclear):
docker compose -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml down -v
docker compose down -v
# then §2.2 again
```

Relay identity keys: regenerating `BUZZ_RELAY_PRIVATE_KEY` changes the relay's
identity; rotate `RELAY_OWNER_PUBKEY`/bridge keys together (they are the same
keypair).

---

## 10. Operational proof surface

The following are operator-visible today (proven during this goal):

- Every webhook rejection: backend `WARN buzz event rejected: <category>` +
  observer `forward failed ... (permanent <status>)`.
- Relay-side auth/membership decisions: relay logs (`NIP-42 auth successful`,
  `auth failed`, `restricted: not a relay member`).
- Observation lag: `chat_observation_lag_seconds` (no labels — the gauge
  tracks the latest observed event age across the deployment; per-community
  series would be unbounded) measures the latest observed event age; alert
  when it exceeds 120 seconds for 5 minutes.
- Webhook outcomes: `chat_webhook_outcomes_total{outcome}` counts observed,
  duplicate, and category-safe rejection outcomes; alert on a rejection rate
  above 10% for 10 minutes.
- Authorization denials: `chat_authorization_denials_total` (no labels —
  per-tenant series would be unbounded; the counter contains no user,
  message, or tenant data).
- Bridge delivery: `chat_bridge_delivery_state{kind,state}` reports 9030/9031
  acked, retry-queued, or DLQ state; alert on any non-zero DLQ count.
- Relay outage: publish fails with a distinct transport error; reads fail
  closed in buzz mode (the gateway denies — no silent fallback to local);
  recovery is automatic (observer reconnect).

## 11. Alpha blocker disposition

- #240 is complete: the tenant-scoped admin revoke action calls the existing
  atomic `revoke_principal` path and queues kind-9031.
- #239 is complete: webhook outcomes, latest observation lag, authorization
  denials, and 9030/9031 delivery state are exposed through the existing
  Prometheus surface. Metrics contain bounded labels only; bodies, signatures,
  HMACs, keys, and PII are excluded.
- #241 is complete: relay acceptance shows “Sent — waiting for Elembra sync”;
  the existing 15-second observation poll resolves it, otherwise a warning is
  shown without claiming success.
- #244 is complete: Chat status exposes only `ask_available`; unavailable Ask
  is not advertised as an active control.
- #242 is complete: recipient-side tags shipped — the observation index
  retains each event's identifier-only refs (migration `20260810000007`), the
  timeline DTO surfaces them as an openable affordance, and opening
  reauthorizes through Files at read time (existence-hiding,
  forced-download headers). See
  `docs/implementation/elembra-chat-app-v1.md` §1–§2.
- #243 is resolved at the WIRE-FORMAT level: Buzz confirms the canonical
  thread/root contract — NIP-29 `["h", <channel-uuid>]` channel scoping plus
  NIP-10 `["e", <64-hex>, <relay-url?>, "root"|"reply"]` thread tags with
  server-validated ancestry (documented in the buzz repo's NOSTR.md and the
  Elembra spec's "Canonical publish tags and kinds"). The remaining #243 work
  is the reply/thread COMPOSER feature (reply UI in the message composer),
  which stays open as a separate follow-up.
- #245 is resolved at the RELAY-CAPABILITY and CONFORMANCE level: the Buzz
  ADR-0035 relay capability is implemented and MERGED (kubedoio/buzz PR #1,
  now on `kubedoio/buzz` main) and the live conformance suite is green
  (`scripts/run-buzz-conformance.sh`, 12 live proofs incl. `live_p10`
  one-batch-round-trip, `live_p11` latency budget, `live_p12` tombstone
  reconciliation, `live_p13` bootstrap identity discovery). Issue #245's four acceptance criteria: relay endpoints
  implemented ✅; live-relay conformance replaces the fake ✅; buzz-mode
  authorization enabled in production ✅ (kubedoio/rustshare PR #249 merged;
  enabled by default in the Alpha/dogfood stack); large-timeline latency
  regression test passes within budget ✅ (`live_p11`, observed median
  192 ms against the 500 ms budget, re-certified against merged Buzz main).
  Elembra does not emulate either upstream dependency.

## 12. Live Buzz conformance suite (production-authority proofs)

The live conformance suite (`backend/tests/buzz_live_conformance_test.rs`)
proves Elembra uses Buzz as the REAL production authority, fail-closed: an
in-process Elembra (AppState with the Buzz gateway authority + `buzz_gateway`
wired, same as the fake-relay suites) runs against the REAL relay built from
the merged Buzz main worktree (`.worktrees/buzz`), with the
dev Elembra DB as the store. The suite seeds the relay itself over its public
HTTP surface (`POST /events`) and ingests the same signed events through the
real in-process observation bridge.

Proofs covered (each a `#[tokio::test]`):

1. allowed channel read succeeds (member + available message → Allow + fetch
   returns the message bytes);
2. denied/private channel fails (non-member → Deny; fetch is existence-hiding
   404);
3. cross-community access fails (same relay, different host → unmapped host →
   Deny, while the primary tenant still works);
4. revoked user denied immediately (relay-side kind-9001/9031 → the very next
   authorize denies, no caching);
5. relay unavailable fails closed (dead port → Deny, never Allow/error);
6. batch decisions equal single decisions (mixed allow/deny against the live
   relay, per-message parity);
7. channel listing is authoritative (registry lists channels with ZERO
   observations; relay revocation reflected on the next call);
8. no Elembra ACL / no direct Buzz DB access — structural guard
   (`scripts/guard-buzz-no-acl.sh`);
9. Memory/Search/Ask cannot bypass Buzz (message indexed + searchable, but
   RAG materialization returns nothing after relay revocation);
10. a 64-message page authorizes in exactly ONE relay batch round-trip
    (counted via the relay's own metrics endpoint; the latency budget itself
    is tracked separately);
11. bootstrap identity discovery (`live_p13`, ADR-0036): the community-identity
    endpoint returns the deployment community and the relay pubkey, the
    pubkey matches the harness pin, the response signature verifies, and
    authorization still works with the discovered identity.

Run it:

```bash
./scripts/run-buzz-conformance.sh
```

The script builds the relay image from the worktree (skips when present),
brings up the relay stack (`docker compose -f docker-compose.yml -f
docker-compose.alpha.yml -f docker-compose.conformance.yml up -d buzz-relay`)
with `RELAY_URL=ws://127.0.0.1:7447` (so the suite's Host header binds the
seeded community) and `RELAY_TRUSTED_SERVICE_PUBKEYS=<Elembra service pk>`
(the v1alpha1 authorization API's trusted-service gate), generates the
service/relay keys when unset, waits for relay health, runs the suite with
the live env vars, and reports PASS/FAIL. Set `RUSTSHARE_BUZZ_CONFORMANCE_KEEP=1`
to leave the stack running. The suite requires the dev Elembra DB
(`backend/.env` DATABASE_URL) and fails with a clear message when a leftover
container holds the relay ports (7447/8088/9102).
