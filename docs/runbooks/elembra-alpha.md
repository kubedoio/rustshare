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
        buzz-observer (host) ─┘   (NIP-42 AUTH + REQ, forwards kind-1 to
                                  POST /api/v1/integrations/buzz/events)
```

Trust boundaries and data flow: see the Alpha readiness doc §1–§2.

### Components

| Component | Source | Runs as |
|---|---|---|
| Elembra backend/frontend | this repo (`docker/backend.Dockerfile`) | `docker compose up -d` |
| Postgres / RustFS / nginx | `docker-compose.yml` | same |
| Buzz relay + backing services | `ghcr.io/block/buzz:main` (kubedoio/buzz) | `docker compose -f docker-compose.yml -f docker-compose.alpha.yml up -d` |
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
- An image of the Buzz relay: `ghcr.io/block/buzz:main` (pin a `relay-v*` tag
  for stability). Published by kubedoio/buzz (`docker.yml` workflow).

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
#       RUSTSHARE_CHAT_BRIDGE_SECRET_KEY (== BUZZ_SERVICE_SK)

# 4. Configure chat in .env (see §3)
#    RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY=true   (only when the relay runs on this host)
#    RUSTSHARE_ADMIN_PASSWORD=<strong-password>  (set BEFORE first start, see §4)

# 5. Base stack
docker compose up -d

# 6. Relay runtime
docker compose -f docker-compose.yml -f docker-compose.alpha.yml up -d

# 7. Observation bridge (foreground; supervise for dogfooding)
BUZZ_SERVICE_SK=<bridge sk> BUZZ_COMMUNITY_ID=alpha-community \
  nohup ./scripts/start-buzz-observer.sh >> buzz-observer.log 2>&1 &

# 8. Provision the workspace mapping. Mutating API calls require CSRF
#    double-submit (cookie + X-Rustshare-Csrf header, see §6); <workspace_id>
#    equals the admin tenant id. For a relay on this host (flag from step 4):
curl -s -c /tmp/admin.jar -X POST http://localhost/api/v1/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"admin@localhost","password":"<admin-password>"}'
CSRF="$(awk '$6 == "rustshare_csrf_token" { print $7 }' /tmp/admin.jar)"
curl -s -b /tmp/admin.jar -X POST \
  http://localhost/api/v1/admin/applications/chat/workspaces/<tenant_id>/community \
  -H 'content-type: application/json' -H "X-Rustshare-Csrf: ${CSRF}" \
  -d '{"community_id":"alpha-community","relay_url":"ws://localhost:7447"}'
#    (a PUBLIC relay: same command with its wss:// URL. The SSRF guard
#    resolves the host, so placeholder hosts fail — "wss://relay.example.com"
#    in older copies of this runbook is not a real address.)
#    Or use scripts/run-alpha-dogfood.sh, which provisions everything and
#    runs the full dogfood matrix.

# 9. Verify
curl -s http://localhost/health/ready
tail -f buzz-observer.log     # expect "authenticated ... EOSE"
```

### 2.3 Local-relay note (dev/dogfooding on one host)

The admin mapping API and the binding challenge both validate the relay URL
against the SSRF guard (`resolve_public_socket_addrs`). A relay on
`localhost`/private addresses is rejected **unless**
`RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY=true` is set (mirrors
`RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS`; off by default). **The flag is
required for any localhost relay** — the binding challenge re-validates the
stored URL, so an SQL row alone cannot bypass it. With the flag set,
`ws://localhost:7447` is accepted and the API path works end to end.

Operators who prefer to skip the admin-API/CSRF dance may insert the mapping
row directly (it only replaces the API call; the flag is still required):

```sql
INSERT INTO chat_workspace_communities
  (mapping_id, tenant_id, workspace_id, community_id, relay_url, active, created_at)
VALUES (gen_random_uuid(), '<tenant_id>', '<tenant_id>', 'alpha-community',
        'ws://localhost:7447', true, now());
```

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
docker compose -f docker-compose.yml -f docker-compose.alpha.yml down -v   # relay volumes removed
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
| `RUSTSHARE_CHAT_WEBHOOK_SECRET` | HMAC shared with the observation bridge (required) | — |
| `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` | bridge service key for NIP-43 9030/9031 (== `BUZZ_SERVICE_SK`) | empty |
| `RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY` | allow loopback/private relay URLs (dev only) | `false` |
| `BUZZ_RELAY_IMAGE` | relay image | `ghcr.io/block/buzz:main` |
| `BUZZ_RELAY_OWNER_PUBKEY` | relay owner / bridge public key | — |
| `BUZZ_RELAY_PRIVATE_KEY` | relay identity private key | — |
| `BUZZ_SERVICE_SK` | bridge secret key (observer AUTH + E2E driver) | — |
| `BUZZ_RELAY_WS` | relay URL browsers + observer use | `ws://localhost:7447` |
| `BUZZ_COMMUNITY_ID` | community id forwarded by the observer; must equal the mapping | — |
| `BUZZ_CHANNEL_ID` / `BUZZ_CHANNEL2_ID` | observer default channel / E2E second channel | `alpha-channel` / `alpha-ops` |
| `BUZZ_POSTGRES_PASSWORD`, `BUZZ_MINIO_USER`, `BUZZ_MINIO_PASSWORD` | relay backing services | `buzz_dev` / `buzz_dev` / `buzz_dev_secret` |

Generate all keys once: `node frontend/scripts/alpha-gen-buzz-keys.mjs`. The
relay owner key is the bridge identity: its public half is
`RELAY_OWNER_PUBKEY` on the relay, its secret half is
`RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` in Elembra and `BUZZ_SERVICE_SK` for the
observer/E2E.

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

- Channel list = observed events only (L1, upstream).
- Reference-first bodies render placeholder (L2, by design).
- `local` gate = coarse community authorization; per-channel membership is
  upstream (L7/L9/L12).
- Sender-side attachment tags only (L5).
- Observation relay→Elembra push is the host-side bridge (this deployment);
  upstream relay has no webhook delivery yet — hardening is issue #239.
- Reload/logout clears nothing client-side; keys are per-browser vault.

---

## 9. Rollback / reset

```bash
# Stop the dogfood additions, keep Elembra:
docker compose -f docker-compose.yml -f docker-compose.alpha.yml stop buzz-relay buzz-postgres buzz-redis buzz-minio
pkill -f start-buzz-observer.sh

# Full reset (nuclear):
docker compose -f docker-compose.yml -f docker-compose.alpha.yml down -v
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
- Observation lag: the E2E driver measures publish→channels-visible lag.
- Relay outage: publish fails with a distinct transport error; reads stay
  available under the `local` gate; recovery is automatic (observer reconnect).

Missing visibility (tracked): ingestion metrics/lag gauges (#239), publish
telemetry aggregation, bridge delivery state (9030/9031 acks), authorization
denial counters.
