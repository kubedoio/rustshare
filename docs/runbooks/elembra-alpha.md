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
Browser ── ws://localhost:7447 ── buzz-relay ── buzz-postgres / buzz-redis / buzz-rustfs
                                      ▲
             chat-observer (managed Compose service)
             └─ signed community/registry discovery + state recovery
                → HMAC webhook → Elembra observation index
```

Trust boundaries and data flow: see the Alpha readiness doc §1–§2.

### Components

| Component | Source | Runs as |
|---|---|---|
| Elembra backend/frontend | this repo (`docker/backend.Dockerfile`) | `docker compose up -d` |
| Postgres / RustFS / nginx | `docker-compose.yml` | same |
| Buzz relay + backing services | pinned by `config/buzz-compatibility.env` (see §3) | managed Compose services |
| Observation bridge | `frontend/scripts/buzz-observer.mjs` in the pinned Node image | managed `chat-observer`, restart-on-failure |

The canonical Compose wrapper gives the observer and backend the same network
namespace, so Buzz's host-derived community remains identical for browser,
gateway, and observer without a host-side Node process.

---

## 2. Deployment (exact setup)

### 2.1 Prerequisites

- Docker Engine + Compose plugin (validated matrix: Ubuntu 22.04/24.04, Debian 12)
- The committed `config/buzz-compatibility.env` manifest, loaded automatically
  by `scripts/elembra.sh`, which selects the supported Buzz commit, v1alpha1
  contract, and immutable OCI image digest.

### 2.2 Bring up

```bash
# One supported path. It generates base secrets and Chat identities in a
# mode-0600 .elembra/chat.env; private keys are never printed.
./scripts/elembra.sh init --with-chat
./scripts/elembra.sh up

# Admin session — required for the enable call (§2.4), which in auto mode
#    (the alpha default) auto-provisions the deployment community
#    (discover → verify → insert, idempotent; ADR-0036). Mutating API calls
#    require CSRF double-submit (cookie + X-Rustshare-Csrf header, see §6);
#    <tenant_id> equals the admin tenant id:
curl -s -c /tmp/admin.jar -X POST http://localhost/api/v1/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"admin@localhost","password":"<admin-password>"}'
CSRF="$(awk '$6 == "rustshare_csrf_token" { print $7 }' /tmp/admin.jar)"
#    External Buzz deployments (RUSTSHARE_CHAT_PROVISIONING=manual):
#    use the admin page's "Connect existing Chat deployment" form, or the
#    existing admin API POST .../community with the CSRF header above. The
#    SSRF guard resolves the relay host, so placeholder hosts fail —
#    "wss://relay.example.com" is not a real address. Alternatively, use
#    scripts/run-alpha-dogfood.sh, which provisions everything and runs the
#    full dogfood matrix. In bundled mode the observer discovers channels from
#    Buzz's signed registry; operators never enter channel UUIDs.

# Verify
curl -s http://localhost/health/ready
./scripts/elembra.sh status   # includes chat-observer readiness
```

### 2.3 Bundled-relay SSRF trust

The bundled path sets `RUSTSHARE_CHAT_DEPLOYMENT_RELAY_URL` to exactly
`ws://localhost:7447`. The Chat URL validator and Buzz gateway allow a private
destination only when the complete configured URL matches that value. Redirects
remain disabled, DNS results are pinned for the request, and the signed relay
identity is still checked by the gateway and observer. Arbitrary loopback,
RFC1918, link-local, and metadata targets remain rejected. External Buzz mode
does not use this exception and must use a public/resolvable relay URL.

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

- **Relay network namespace**: the supported wrapper recreates the backend,
  relay, and observer together, preserving the host-derived community.
- **Relay wipe**: a fresh relay database may generate a new community id. The
  managed observer reports `community_changed` when this happens during its
  lifetime; after a restart Elembra rejects the newly discovered community and
  the observer becomes degraded. Re-enable or reprovision the Workspace↔Community
  mapping through the admin contract.
- **Health probes**: the backend exposes `/health` and `/health/ready`
  (not `/api/v1/health`); nginx maps `/api/v1` to the backend only.
- **Ask provider**: set `ELEMBRA_LLM_API_KEY`/`BASE_URL`/`MODEL` in `.env`
  (see §3); the backend reads them at startup — recreate the backend after
  changing them. Leave the key empty for the documented gated Ask (503).

> **Security note:** `RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY` remains only as an
> explicit development/test escape hatch. The supported bundled path does not
> set it.

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

Memory projection and content indexing use the tenant-scoped, audited admin
Application configuration API. Without these flags, message bodies are not
stored and the Memory/Ask pipeline stays empty:

```bash
curl -s -b /tmp/admin.jar -X PATCH \
  http://localhost/api/v1/admin/applications/io.elembra.chat \
  -H "X-Rustshare-Csrf: ${CSRF}" -H 'content-type: application/json' \
  -d '{"memory_projection":true,"content_indexing":true}'
```

`scripts/run-alpha-dogfood.sh` performs this step through the same public
contract (P02d); direct production setup SQL is not supported.

### 2.4.1 Chat UI state machine

The Chat application view follows a deterministic state machine:

| State | Render | Description |
|---|---|---|
| No mapping | "Chat is being configured for this workspace." | Workspace not yet bound to a Buzz community |
| Mapping exists + no binding | `BindingPanel` | Key creation / import UI; user creates a Buzz keypair |
| Binding exists + no admission | "Admission pending" | Waiting for relay admission (automated via bridge) |
| Bound + admitted + key locked | `ChatIdentityUnlock` | Passphrase entry / key recovery |
| Bound + admitted + key unlocked | `ChannelList` + `ChannelHeader` + `MessageTimeline` + `MessageComposer` | Full Chat experience |

Channel display names from the Buzz registry are shown as the primary label
(e.g. `# ops`); the raw channel UUID is preserved as the stable identity for
all API calls but is never the default primary label. Message authors are
resolved to the Elembra user's display name when the author's Buzz pubkey has
an active or historical binding in the current tenant; unknown Buzz pubkeys
are shown as "Unknown Buzz user" with a shortened pubkey.

### 2.5 Teardown

```bash
./scripts/elembra.sh down        # stops services and preserves all volumes
./scripts/elembra.sh reset --yes # explicit destructive reset
```

`down -v` also drops the Elembra database — for a real dogfooding period keep
the base volumes (`docker compose down` without `-v`) and only reset the
`buzz-*` volumes when a relay reset is wanted.

---

## 3. Configuration

| Variable | Purpose | Default |
|---|---|---|
| `RUSTSHARE_CHAT_AUTHORITY` | `local` (coarse community gate) or `buzz` (upstream access/check) | `local` |
| `RUSTSHARE_CHAT_PROVISIONING` | chat community provisioning mode: `auto` (bundled zero-config bootstrap, ADR-0036) or `manual` (external Buzz) | `auto` bundled / `manual` external |
| `RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL` | relay URL (`ws://`/`wss://`) discovered for auto-provisioning; required when provisioning is `auto` | — |
| `RUSTSHARE_CHAT_WEBHOOK_SECRET` | HMAC shared with the observation bridge (required) | — |
| `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` | internally derived from the managed deployment service identity; external Buzz mode may provide it explicitly | empty |
| `RUSTSHARE_CHAT_DEPLOYMENT_RELAY_URL` | exact bundled relay URL allowed by the narrow private-target trust model | `ws://localhost:7447` (bundled) |
| `BUZZ_RELAY_IMAGE` | relay image loaded from `config/buzz-compatibility.env`; the supported value is immutable by OCI digest | manifest |
| `BUZZ_RELAY_OWNER_PUBKEY`, `BUZZ_RELAY_PRIVATE_KEY`, `BUZZ_SERVICE_SK` | generated and persisted in `.elembra/chat.env`; existing valid values are reused | internal |
| `BUZZ_RELAY_WS` | relay URL browsers + observer use | `ws://localhost:7447` |
| `BUZZ_COMMUNITY_ID`, `BUZZ_CHANNEL_ID`, `BUZZ_CHANNEL2_ID` | not part of the supported operator contract; the observer discovers community and channels from Buzz | none |
| `BUZZ_POSTGRES_PASSWORD`, `BUZZ_RUSTFS_ACCESS_KEY`, `BUZZ_RUSTFS_SECRET_KEY` | dedicated Buzz RustFS runtime and `buzz-media` bucket | `buzz_dev` / `buzz_dev` / `buzz_dev_secret` |
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
community-identity discovery endpoint (ADR-0035/0036). The supported image,
source commit, contract version, and OCI digest are recorded in
`config/buzz-compatibility.env`; the blocking gate never follows Buzz `main`.

### 3.1 Object-storage boundary

Classification: **B — shared RustFS is technically safe but should be a later
isolated migration.** Buzz only needs the S3 data plane and currently uses a
dedicated `buzz-media` bucket. RustFS supports IAM/service-account
credentials, so a future deployment can provision a Buzz-only credential on
Elembra's existing RustFS without sharing the Elembra `rustshare-files`
namespace. This baseline keeps a separate pinned RustFS service and
credentials (RustFS 1.0.0 GA; pinned by OCI digest) to avoid changing storage
ownership, lifecycle, or migration semantics while repairing conformance.

`./scripts/elembra.sh init --with-chat` generates all deployment keys in the
managed bootstrap container. The relay owner key is the bridge identity: its public half is
`RELAY_OWNER_PUBKEY` on the relay, its secret half is
`RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` in Elembra and `BUZZ_SERVICE_SK` for the
observer/E2E.

In the bundled deployment there is exactly one canonical logical service
identity. `chat-bootstrap` derives all secondary environment values and fails
on an inconsistent existing key set. Validation is implicit in the bootstrap
command; no host-side Node/npm installation is part of the supported path.
The diagnostic checks cover:

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
   (`scripts/read-bootstrap-password.sh "$(docker compose ps -q backend)"`).
   It does not survive container recreation.

1. **Account**: admin creates the user (API: `POST /api/v1/admin/users`, or the
   admin UI). The user logs in.
2. **Chat key**: the browser generates a Buzz key on first Chat use and encrypts
   it with a passphrase (PBKDF2 600k). The encrypted key is stored in
   localStorage; the plaintext secret key is loaded into an **in-memory identity
   session** once per browser session:

   - **Locked state**: the user sees an unlock panel (`ChatIdentityUnlock`)
     with a passphrase input. A locked identity does not prevent reading
     messages, but the Send control and composer are not shown.
   - **Unlock**: on correct passphrase the session holds the decrypted secret
     key in module memory (never persisted, never visible to the backend).
   - **Wrong passphrase**: clear inline error; the session remains locked.
   - **No key / corrupt key**: the unlock panel shows the import/recovery UI.
   - **Export / Lock / Remove**: available from the `ChatIdentityMenu` in the
     composer action strip.
   - **Logout**: clears the in-memory session.
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
| Observer | `./scripts/elembra.sh status` | `chat-observer` is healthy and `/ready` reports `ready` |
| Ingestion | `docker logs rustshare-backend-1 | grep "buzz event rejected"` | none (or understood) |
| E2E matrix | `./scripts/run-alpha-dogfood.sh` after the supported install | all PASS |

---

## 6. Common failure diagnosis

| Symptom | Likely cause | Action |
|---|---|---|
| Message never appears in Elembra | webhook secret drift; observer down; mapping/binding missing | run `./scripts/elembra.sh status`; inspect `docker compose logs chat-observer` for the safe rejection category (`Unknown community` / `Unbound author`) |
| Observer reconnect loop | relay down; `BUZZ_SERVICE_SK` wrong | restart relay; verify key |
| Publish "relay unreachable" | relay down; wrong `BUZZ_RELAY_WS` | relay health; browser console |
| Publish "relay rejected: …" | not admitted at the relay; revoked | check relay membership (9030 delivered); run the E2E admit step |
| Ask 503 | LLM provider not configured | configure provider; status surface `ask_available` (issue #244) |
| Channel list frozen | observer unhealthy or WS exhaustion | `docker compose logs chat-observer`; Compose restarts it automatically |
| Binding challenge rejects bundled localhost | deployment URL was changed without changing the explicit trust anchor | restore the exact configured URL or perform an explicit operator configuration change; do not enable the generic local-relay flag |
| Buzz 401 / "service identity rejected" | generated service identity does not match the relay allowlist | run `./scripts/elembra.sh init --with-chat`; inconsistent existing keys fail closed |
| 403 on mutating calls | missing CSRF header (browser clients get it automatically) | API tooling: send `X-Rustshare-Csrf` matching the cookie |

---

## 7. Backup considerations

- Elembra data: `scripts/backup-stack.sh` (postgres dump + RustFS + config).
- Relay state: `buzz_postgres_data` / `buzz_rustfs_data` volumes — back these up
  for message-history continuity. A relay reset loses the **relay's** event
  history; Elembra's observation index (its own Postgres) survives, and on
  observer reconnect the relay replays whatever events it still holds (deduped
  by event id). Events the relay no longer holds are not re-projected.
- Keys: the bridge keys live in `.elembra/chat.env` with mode 0600 (not in the
  backup bundle — store them in a secrets manager). User Buzz keys never leave the browser; backup is the user's
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
# Stop the supported deployment and keep data:
./scripts/elembra.sh down

# Full reset (explicitly destructive):
./scripts/elembra.sh reset --yes
# then ./scripts/elembra.sh init --with-chat && ./scripts/elembra.sh up
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
  ADR-0035 relay capability is implemented and merged, and the supported
  runtime source is pinned in `config/buzz-compatibility.env`. The live gate
  (`scripts/run-buzz-conformance.sh`) executes the real relay and its 13 live
  proofs, including `live_p10` one-batch-round-trip, `live_p11` latency budget,
  `live_p12` tombstone reconciliation, `live_p13` bootstrap identity
  discovery, and `live_p14` NIP-98 rejection checks. Issue #245's four
  acceptance criteria: relay endpoints
  implemented ✅; live-relay conformance replaces the fake ✅; buzz-mode
  authorization enabled in production ✅ (kubedoio/rustshare PR #249 merged;
  enabled by default in the Alpha/dogfood stack); large-timeline latency
  regression test passes within budget ✅ (`live_p11`, observed median
  192 ms against the 500 ms budget, re-certified against the pinned source).
  Elembra does not emulate either upstream dependency.

## 12. Live Buzz conformance suite (production-authority proofs)

The live conformance suite (`backend/tests/buzz_live_conformance_test.rs`)
proves Elembra uses Buzz as the REAL production authority, fail-closed: an
in-process Elembra (AppState with the Buzz gateway authority + `buzz_gateway`
wired, same as the fake-relay suites) runs against the REAL relay selected by
`config/buzz-compatibility.env`, with a fresh Elembra DB as the store. The
suite seeds the relay itself over its public
HTTP surface (`POST /events`) and ingests the same signed events through the
real in-process observation bridge.

Live proofs covered (13 `#[tokio::test]` cases):

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
8. Memory/Search/Ask cannot bypass Buzz (message indexed + searchable, but
   RAG materialization returns nothing after relay revocation);
9. a 64-message page authorizes in exactly ONE relay batch round-trip
    (counted via the relay's own metrics endpoint; the latency budget itself
    is tracked separately);
10. timeline authorization latency stays within the 500 ms budget
    (`live_p11`, observed median 192 ms in the pinned baseline);
11. relay deletion is applied by reconciliation (`live_p12`) and the deleted
    message remains existence-hidden;
12. bootstrap identity discovery (`live_p13`, ADR-0036): the
    community-identity endpoint returns the deployment community and relay
    pubkey, the pubkey matches the harness pin, the response signature
    verifies, and authorization still works with the discovered identity;
13. NIP-98 service authentication is required (`live_p14`): missing and
    malformed authorization headers, plus a valid event signed by an untrusted
    key, receive `401` before community state is disclosed.

The no-Elembra-ACL and no-direct-Buzz-DB check is a separate structural guard,
not one of the live proof cases: `scripts/guard-buzz-no-acl.sh`.

Run it:

```bash
./scripts/run-buzz-conformance.sh
```

The script loads the compatibility manifest, generates keys and ephemeral
credentials when unset, starts fresh Elembra PostgreSQL/RustFS plus the
separate Buzz PostgreSQL/Redis/RustFS stack, runs migrations, waits for
dependency and relay readiness, runs the suite, emits secret-safe diagnostics
on failure, and removes its Compose project and volumes. Set
`RUSTSHARE_BUZZ_CONFORMANCE_KEEP=1` only when debugging a failed run. Its
isolated host ports default to 15432, 19000, 19001, 17447, 18088, and 19102,
so an existing Alpha stack on 5432/9000/9001/7447 is not reused or modified.
