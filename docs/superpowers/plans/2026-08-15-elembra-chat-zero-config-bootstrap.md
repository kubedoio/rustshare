# Elembra Chat Zero-Config Bootstrap v1 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A supported Elembra deployment can enable Chat and obtain a correct, authoritative Workspace↔Buzz Community mapping automatically — without weakening Buzz's Host→Community isolation model, without direct Buzz DB access, and without manual SQL/curl mapping steps.

**Architecture:** Buzz gets one narrow, read-only discovery endpoint (`GET /api/v1/relay/community`) on the existing v1alpha1 surface (NIP-98 + trusted-service gate, relay-signed kind-19030 response) that returns the community bound to the request Host plus the relay's stable pubkey. Elembra gets a config-gated provisioning mode (`auto|manual`): in auto mode, enabling Chat discovers → verifies → idempotently inserts the Workspace↔Community mapping (with pinned relay pubkey); manual mode keeps the existing explicit admin API. The alpha compose (the supported deployment) defaults to auto and to a main-built, SHA-pinned `ghcr.io/kubedoio/buzz` image.

**Tech Stack:** Rust (buzz-relay: Axum/Nostr; rustshare backend: Axum/SQLx/Postgres), SvelteKit/TypeScript frontend, Docker Compose, GHCR, GitHub Actions.

---

## 0. Grounded facts (verified against code on 2026-08-15)

Repos: `kubedoio/buzz` worktree at `/srv/data02/projects/rustshare/.worktrees/buzz` (main `325b4949`, clean); `kubedoio/rustshare` at `/srv/data02/projects/rustshare` (main `0d316a33`, clean).

- **Buzz community model** (`crates/buzz-db/src/lib.rs`, `migrations/0001_initial_schema.sql`): `communities(id UUID PK DEFAULT gen_random_uuid(), host VARCHAR(255) NOT NULL, ...)` — no name column; UNIQUE on `lower(host)`. The deployment community is auto-seeded at every startup from `RELAY_URL` via `Db::ensure_configured_community` (lib.rs:1369-1396, idempotent `INSERT ... ON CONFLICT DO UPDATE ... RETURNING id, host, (xmax=0)`).
- **Host→Community binding** (`crates/buzz-relay/src/tenant.rs`): `bind_community(db, host)` normalizes host → `communities` row → `TenantContext` with accessors `community()` and `host()` (tests at tenant.rs:184-233). Unmapped host → generic 404, no fallback community.
- **Relay pubkey**: derived from `BUZZ_RELAY_PRIVATE_KEY` (`config.rs:754`, `Option<String>`); `AppState.relay_keypair` (state.rs:620); `has_stable_key = config.relay_private_key.is_some()` (nip11.rs:308). NOT an env var; not in `/health`.
- **v1alpha1 authz surface** (`crates/buzz-relay/src/api/relay_access.rs`, routes `router.rs:116-131`): `POST access/check`, `POST access/check-batch`, `GET channels`, `GET state/events`. Pipeline per endpoint: Host→community bind (404) → NIP-98 mandatory (`verify_bridge_auth`/`verify_bridge_auth_with_options`, GET has no payload tag) → trusted-service gate on `RELAY_TRUSTED_SERVICE_PUBKEYS` (empty allowlist ⇒ all 401) → `enforce_http_admission` + `check_nip98_replay` → signed kind-19030 response via `sign_response` (relay_access.rs:283-296, `KIND_RELAY_AUTHZ_RESPONSE`). `api_error`/`internal_error` helpers exist.
- **Operator API exists** (`api/operator.rs`): `POST /operator/communities` (create_only/convergence), `GET /operator/communities`, archive/transfer — NIP-98 + `RELAY_OPERATOR_PUBKEYS` + `RELAY_OPERATOR_API_ORIGIN` gated. **Rejected for auto-bootstrap**: it grants archive/transfer powers to the bridge identity; the narrow trusted-service endpoint is the smallest honest contract (documented in ADR-0036).
- **Elembra mapping** (`backend/migrations/20260810000002/05/06`): `chat_workspace_communities(mapping_id, tenant_id, workspace_id, community_id, relay_url, active, relay_pubkey CHECK (~'^[0-9a-f]{64}$'))` with `UNIQUE(tenant_id, workspace_id)`, `UNIQUE(tenant_id, community_id)`, partial unique index `chat_workspace_communities_active_community ON (community_id) WHERE active`.
- **Single existing write path**: `POST /api/v1/admin/applications/chat/workspaces/{workspace_id}/community` → `handlers/chat_identity.rs:111-154 configure_mapping` → `storage/chat_identity.rs:155 insert_mapping` (plain INSERT; caller maps violations to 409). Storage read: `mapping(tenant, workspace)` (storage/chat_identity.rs:38). No auto-provisioning code exists anywhere.
- **UX string**: `frontend/src/lib/components/chat/ChatApplicationView.svelte:181-184` renders "No Buzz community is mapped for this workspace yet. An administrator can configure it." when `status.mapping` is null; assertion `frontend/src/lib/components/chat/ChatApplicationView.test.ts:126-131`. Status endpoint (`handlers/chat_app.rs:96-141`) returns `mapping: None` until a binding exists (deliberate existence-hiding; **pre-existing quirk — BindingPanel is unreachable before a binding exists; out of scope, documented as limitation**).
- **Gateway client patterns** (`backend/server/src/buzz_gateway.rs`): `nip98_header(method, url, body)` (:386), `verify_19030(raw, relay_pubkey)` (:424), `validated_http(relay_url)` (:312, SSRF pin), `read_response_json`, `log_relay_error`, `MAX_EVALUATED_AT_AGE_SECS = 60` (:50), freshness check text at :477, `page_state` (:254) is the GET-template to copy. `BuzzGatewayClient::new_for_test` exists under `cfg(any(test, debug_assertions))`.
- **Image state**: `buzz/.github/workflows/docker.yml` defaults `IMAGE_NAME` to `ghcr.io/block/buzz` unless the `GHCR_IMAGE` repo variable is set; `gh api users/kubedoio/packages/container/buzz` → 404 and no `GHCR_IMAGE` variable ⇒ the fork publishes nothing today (previous task's "stale ghcr.io/block/buzz:main" blocker). Main pushes get `:main` + `:sha-<7>` tags via metadata-action. Push-gateway jobs hardcode `ghcr.io/block/buzz-push-gateway` and would fail on the fork.
- **Config patterns** (`backend/server/src/config.rs`): `RUSTSHARE_CHAT_AUTHORITY` (default `local`, validated fail-closed at startup, `validate_chat_authority` :285-311), `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` (required for buzz). `RUSTSHARE_CHAT_ALLOW_LOCAL_RELAY` is read raw by `resolve_chat_relay_socket_addrs` (SSRF guard used per-request).
- **Enable flow**: `POST /api/v1/admin/applications/{key}/enable` → `handlers/admin/applications.rs::enable_application` → `ApplicationService::enable_application` (application_service.rs:480-514; note `workspace_id = $1` binds the tenant id — workspace == tenant in this model). `crate::authz::chat_owner::CHAT_APPLICATION_ID` is the Chat app id.
- **Wiring**: `bootstrap.rs:850-895` — buzz mode builds `BuzzGatewayClient` + `BuzzGatewayAuthority`; local mode builds `LocalFallbackAuthority`.
- **E2E**: `scripts/run-alpha-dogfood.sh` — P02b enables chat, P02c manually maps (POST/PATCH with `ALPHA_LOCAL_RELAY=1` fallback), P05 binds, rest is read/publish/revoke/Ask proofs. Env `:?`-required: `BUZZ_SERVICE_SK`, `BUZZ_RELAY_WS`, `BUZZ_COMMUNITY_ID`, `BUZZ_RELAY_PUBKEY`.
- **Conformance**: `backend/tests/buzz_live_conformance_test.rs` (11 `#[ignore]` tests P1-P12) + `scripts/run-buzz-conformance.sh` (builds relay image from `.worktrees/buzz`, exports `RUSTSHARE_BUZZ_LIVE_RELAY_URL/SERVICE_SK/RELAY_PUBKEY/METRICS_URL`). `scripts/guard-buzz-no-acl.sh` = structural proof 8. CI: `integration-tests.yml` (jobs `integration-tests` + `buzz-conformance`, checks out `kubedoio/buzz@main`).
- **Docs**: runbook `docs/runbooks/elembra-alpha.md` (§2.2 keys, mapping via CSRF curl or SQL at :91-136, config table §3, proofs §12); ADR-0034 (boundary, "names and URLs are not inferred" :381, mapping existence-hidden); ADR-0035 (gateway, pinning); `docs/architecture/elembra-chat-alpha-readiness.md` (Alpha contract A1-A20, latency budget §14); design spec `docs/superpowers/specs/2026-08-12-elembra-chat-app-v1-design.md` (admin UI was "out of scope", :284).

## 1. Design decisions (locked — do not reopen)

- **D1 — Provisioning mode config.** New env `RUSTSHARE_CHAT_PROVISIONING` = `auto|manual`, default `manual` (backward compatible). `auto` requires `RUSTSHARE_CHAT_AUTHORITY=buzz` and `RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL` (ws/wss); any violation is a **startup error** (fail closed). `docker-compose.alpha.yml` sets `auto` + the bootstrap relay URL (the supported deployment).
- **D2 — Discovery contract.** New Buzz endpoint `GET /api/v1/relay/community`, same auth pipeline as the authz surface, read-only, never mutates state. Returns relay-signed kind-19030 whose content is `{"community_id","host","relay_pubkey","evaluated_at"}`. The Elembra client verifies the signature against the `relay_pubkey` **claimed in the content** (no prior pin exists at bootstrap; this proves key possession) plus `evaluated_at` freshness (≤60 s) and structural validity (UUID community, 64-hex pubkey, non-empty host). Fail closed on any deviation.
- **D3 — Bootstrap semantics.** `provision(tenant, workspace)` = 1) discover (read-only), 2) if a mapping already exists: same community → idempotent no-op; different → 409 `CommunityMismatch` (never overwritten silently), 3) else conflict-safe insert: workspace-unique race → return the winner after verifying community; community-unique / active-partial violation → 409 `CommunityInUse`. No mapping is ever created from a GET/status request.
- **D4 — Trigger.** In auto mode, `POST /api/v1/admin/applications/io.elembra.chat/enable` attempts provisioning inline after the enable succeeds. Failure is logged and leaves Chat safely unconfigured (enable still succeeds). Admin retry: `POST /api/v1/admin/applications/chat/workspaces/{workspace_id}/provision` ("Set up automatically").
- **D5 — No persisted provisioning state.** Failures live in logs and in the retry response. Admin diagnostics = `GET /api/v1/admin/applications/chat/workspaces/{workspace_id}/community` (admin-only, returns mapping incl. pubkey pin).
- **D6 — Multi-workspace.** NOT auto-provisioned in this task. Documented topology: one host = one community; the deployment community is the startup-seeded row; additional workspaces need per-workspace communities on distinct hosts (operator API `POST /operator/communities` `create_only:true` documented as the provisioning path, not used by auto). Schema already fails closed on sharing (unique per community).
- **D7 — Image strategy.** Buzz `docker.yml` default image name → `ghcr.io/kubedoio/buzz` (fork publishes to its own namespace; upstream keeps `ghcr.io/block/buzz`); push-gateway jobs run only in upstream `block/buzz`. After the Buzz PR merges, the main push publishes `:main` + `:sha-<7>`; RustShare pins `BUZZ_RELAY_IMAGE` default to that immutable `sha-` tag. Until the Buzz PR merges, local E2E builds the relay image from the buzz worktree (`buzz-relay:dogfood`) — no dependence on floating tags, no stale images.
- **D8 — UX scope.** Replace the "No Buzz community is mapped…" string with "Chat is being configured for this workspace." (normal users). Admin surface: new page `/admin/applications/chat` with "Set up automatically" and "Connect existing Chat deployment". No redesign of identity/binding flow; BindingPanel reachability quirk is pre-existing and documented as a limitation.

## 2. File map

**Buzz** (worktree `.worktrees/buzz`, branch `feat/community-identity`):
- Modify `crates/buzz-relay/src/api/relay_access.rs` — `community_identity` handler + doc comment + tests.
- Modify `crates/buzz-relay/src/router.rs` — route + comment.
- Modify `.github/workflows/docker.yml` — image default, push-gateway skip, attestation owner.

**RustShare** (branch `feat/chat-zero-config-bootstrap`):
- Modify `backend/server/src/config.rs` — `ChatProvisioningMode`, two env keys, startup validation + unit tests.
- Modify `backend/server/src/buzz_gateway.rs` — `BuzzCommunityIdentity`, `community_identity()`, `identity_from_19030()`, `validate_community_identity()` + unit tests.
- Modify `backend/crates/storage/src/chat_identity.rs` — `ProvisionMappingOutcome`, `ProvisionMappingError`, `provision_mapping()`.
- Create `backend/server/src/services/chat_bootstrap.rs` — `ChatBootstrapService`, `ProvisionOutcome`, `ChatBootstrapError`.
- Modify `backend/server/src/handlers/chat_identity.rs` — `provision_community_mapping`, `get_community_mapping`, response/error mapping.
- Modify `backend/server/src/routes.rs` — 2 routes.
- Modify `backend/server/src/handlers/admin/applications.rs` — enable hook.
- Modify `backend/server/src/bootstrap.rs` — service wiring (buzz mode only).
- Modify `frontend/src/lib/components/chat/ChatApplicationView.svelte` + `ChatApplicationView.test.ts`.
- Modify `frontend/src/lib/api/chat.ts` — admin client functions.
- Create `frontend/src/routes/admin/applications/chat/+page.svelte` + test.
- Modify `docker-compose.alpha.yml`, `.env.example`, `backend/.env.example`.
- Modify `scripts/run-alpha-dogfood.sh`, `backend/tests/buzz_live_conformance_test.rs` (live_p13).
- Create `backend/tests/chat_bootstrap_test.rs`.
- Modify `scripts/guard-buzz-no-acl.sh` if it needs a new pattern (verify; likely unchanged).
- Modify `docs/adr/0034-elembra-chat-buzz-boundary.md`, create `docs/adr/0036-elembra-chat-zero-config-bootstrap.md`, create `docs/architecture/elembra-chat-bootstrap.md`, modify `docs/runbooks/elembra-alpha.md`, modify `docs/architecture/elembra-chat-alpha-readiness.md`, modify `CHANGELOG.md`, commit this plan.

---

## Phase A — Buzz: discovery endpoint + CI image (worktree `.worktrees/buzz`)

### Task A1: Community-identity handler

**Files:**
- Modify: `crates/buzz-relay/src/api/relay_access.rs`
- Modify: `crates/buzz-relay/src/router.rs`

- [ ] **Step 1: Add the handler** to `relay_access.rs`, placed after `page_state` (end of the v1alpha1 surface), with this exact code:

```rust
/// Community-identity path: the zero-config bootstrap discovery contract.
pub const COMMUNITY_PATH: &str = "/api/v1/relay/community";

/// Return the community bound to the request `Host` and the relay's stable
/// public key — the bootstrap discovery contract.
///
/// A trusted Elembra service calls this BEFORE creating any mapping row: the
/// response identifies the deployment community (the row auto-seeded at
/// startup from `RELAY_URL`) and the relay pubkey to pin. The response is a
/// relay-signed kind-19030 event whose `content` is
/// `{"community_id","host","relay_pubkey","evaluated_at"}`; the client MUST
/// verify the signature against the `relay_pubkey` from the content (proves
/// the relay controls the private key) and the `evaluated_at` freshness,
/// exactly like every other authorization response.
///
/// The endpoint NEVER creates or mutates state.
///
/// Failures: 404 for an unmapped `Host`; 401 for missing/invalid NIP-98 or an
/// untrusted caller; 500 when no stable relay identity is configured
/// (`BUZZ_RELAY_PRIVATE_KEY` unset — the dev-mode deterministic key must
/// never be handed out as a production pin) or on internal errors.
pub async fn community_identity(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let raw_host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let tenant = crate::tenant::bind_community(&state.db, raw_host)
        .await
        .map_err(|_| {
            api_error(
                StatusCode::NOT_FOUND,
                "relay: no community is configured for this host",
            )
        })?;

    // NIP-98 (GET): `u` tag = exact request URL for this host, `method` = GET.
    let url = nip98_expected_url(&state.config.relay_url, &tenant, COMMUNITY_PATH);
    let (caller_pubkey, event_id_bytes) = verify_bridge_auth(&headers, "GET", &url, None, true)?;

    // Trusted-service gate — the same fail-closed allowlist as the authz
    // surface; an empty allowlist disables discovery entirely.
    let caller_hex = caller_pubkey.to_hex();
    if !state
        .config
        .relay_trusted_service_pubkeys
        .iter()
        .any(|trusted| trusted == &caller_hex)
    {
        return Err(api_error(StatusCode::UNAUTHORIZED, "untrusted caller"));
    }

    enforce_http_admission(&state, &tenant, &caller_pubkey).await?;
    check_nip98_replay(&state, &tenant, event_id_bytes).await?;

    // A deterministic dev key must never be persisted as a production pin.
    if state.config.relay_private_key.is_none() {
        return Err(internal_error("relay identity is not configured"));
    }

    let content = serde_json::json!({
        "community_id": tenant.community().to_string(),
        "host": tenant.host(),
        "relay_pubkey": state.relay_keypair.public_key().to_hex(),
        "evaluated_at": nostr::Timestamp::now().as_secs() as i64,
    });
    sign_response(&state, content)
}
```

Notes for the implementer (verify against the actual code before finalizing):
- `community()` may return a type with `as_uuid()` instead of `Display`; if so use `tenant.community().as_uuid().to_string()`. `host()` exists (tenant.rs tests use `ctx.host()`).
- `verify_bridge_auth(&headers, "GET", &url, None, true)` must match the exact call signature used by `list_channels` (relay_access.rs:557) — copy that call verbatim.
- `config.relay_private_key` is `Option<String>`; `state.relay_keypair` is `nostr::Keys`.

- [ ] **Step 2: Add the route** in `router.rs` after the `state/events` route (:128-131) and extend the comment block at :113-115:

```rust
        .route(
            "/api/v1/relay/community",
            get(api::relay_access::community_identity),
        )
```

Update the comment above the routes to: "…single and batch access checks, the authoritative channel registry, and the community-identity bootstrap discovery endpoint return relay-signed kind-19030 events."

- [ ] **Step 3: Build** — `cargo build -p buzz-relay` (from `.worktrees/buzz`). Expected: compiles.
- [ ] **Step 4: Commit (DCO)** — `git add crates/buzz-relay/src/api/relay_access.rs crates/buzz-relay/src/router.rs && git commit -s -m "feat(relay): add community-identity bootstrap discovery endpoint"`

### Task A2: Handler tests

**Files:**
- Modify: `crates/buzz-relay/src/api/relay_access.rs` (test module)

- [ ] **Step 1: Add tests** to the existing test module, following the existing test helpers exactly (state builder `access_check_test_state_with_allowlist` :910-981, request/assert patterns of the `check_access` tests; if the state builder does not set a `relay_private_key`, set `state.config.relay_private_key = Some(hex)` in the tests that need a stable key, mirroring how other tests configure the keypair):

```rust
// --- community identity (bootstrap discovery) ---

#[tokio::test]
#[ignore = "requires Postgres"]
async fn community_identity_returns_signed_identity_for_bound_host() {
    // seed a community for a test host; allowlist = {service key}; stable key set.
    // GET /api/v1/relay/community with Host = seeded host and a NIP-98 GET
    // event signed by the service key.
    // Assert: 200; response parses as a Nostr event; kind == 19030;
    //   event.verify() == Ok; event.pubkey.to_hex() == content["relay_pubkey"];
    //   content["community_id"] == seeded community id;
    //   content["host"] == normalized host;
    //   content["evaluated_at"] within ±60s of now.
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn community_identity_rejects_untrusted_caller() {
    // same setup but caller pubkey NOT in the trusted-service allowlist.
    // Assert: 401 "untrusted caller".
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn community_identity_rejects_unmapped_host() {
    // Host with no communities row. Assert: 404.
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn community_identity_fails_closed_without_stable_relay_key() {
    // config.relay_private_key = None. Assert: 500.
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn community_identity_rejects_replayed_auth_event() {
    // Reuse the replay test pattern from the authz tests: the same NIP-98
    // event sent twice must yield 401 on the second call.
}
```

- [ ] **Step 2: Run tests** — `DATABASE_URL=... REDIS_URL=... cargo test -p buzz-relay --test ... api::relay_access::community_identity -- --ignored` (or the exact command CI uses for "Relay authorization endpoint tests", `ci.yml:695-707`). Expected: all 5 pass; no existing test regressed.
- [ ] **Step 3: Format + lint** — `cargo fmt --all --check && SQLX_OFFLINE=true cargo clippy -p buzz-relay --all-targets -- -D warnings`. Expected: clean.
- [ ] **Step 4: Commit (DCO)** — `git add -u && git commit -s -m "test(relay): cover community-identity bootstrap endpoint"`

### Task A3: Fork CI publishes to its own namespace

**Files:**
- Modify: `.github/workflows/docker.yml`

- [ ] **Step 1: Change the image default** (env block, :75-79):

```yaml
  # Single source of truth for the image name. Upstream block/buzz publishes
  # ghcr.io/block/buzz; this fork (kubedoio/buzz) publishes the supported
  # Elembra image to its own namespace so a main-built, v1alpha1-capable
  # image exists for production pinning. Repos may still override via the
  # GHCR_IMAGE variable.
  IMAGE_NAME: ${{ vars.GHCR_IMAGE != '' && vars.GHCR_IMAGE || 'ghcr.io/kubedoio/buzz' }}
```

Also update: the header comment line 3, and the attestation owner in the `Summary` step (:313, :339) from `--owner block` to `--owner kubedoio`.

- [ ] **Step 2: Skip push-gateway on forks** — the push-gateway is an upstream (block/buzz) artifact; the fork cannot publish it and does not need it. Add `if: github.repository == 'block/buzz'` to BOTH `push-gateway-build` (:343) and `push-gateway-merge` (:420) jobs, with a one-line comment. Verify the workflow YAML is still valid.
- [ ] **Step 3: Commit (DCO)** — `git add .github/workflows/docker.yml && git commit -s -m "ci(docker): publish fork images to ghcr.io/kubedoio/buzz"`

### Task A4: Buzz PR + CI

- [ ] **Step 1: Run the full local checks** — from `.worktrees/buzz`: `cargo fmt --all --check`, `SQLX_OFFLINE=true cargo clippy --all-targets --all-features -- -D warnings`, `SQLX_OFFLINE=true cargo test --workspace --lib` (fast suite; the `#[ignore]` DB tests are run by CI). Expected: all green.
- [ ] **Step 2: Push + PR** — `git push -u origin feat/community-identity`, then `gh pr create --repo kubedoio/buzz --title "feat(relay): community-identity bootstrap discovery endpoint + fork image publishing" --body "..."` with a security note (endpoint gated by the existing trusted-service allowlist; read-only; fails closed without a stable relay key; no authz behavior changes) and the DCO sign-off note.
- [ ] **Step 3: Wait for CI to be green on the PR** (docker.yml PR job is build-only; the unit tests job must pass; pre-existing unrelated failures documented from the previous task may still appear — check the PR's changed-job results only: any failure in a job that touches the changed files must be fixed before merge).
- [ ] **Do NOT merge.** Record the PR number + branch HEAD for the return report. The merge is a human step (goal: "Do NOT merge automatically").

**Phase A gate:** the RustShare phases B-E proceed against the LOCAL buzz worktree branch (conformance/dogfood build the relay image from `.worktrees/buzz`). Only Phase D's final image pin depends on the Buzz PR merge.

---

## Phase B — RustShare backend

### Task B1: Config — provisioning mode + bootstrap relay URL

**Files:**
- Modify: `backend/server/src/config.rs`

- [ ] **Step 1: Add the mode enum** (near the chat authority parsing, ~:269):

```rust
/// Chat community provisioning mode (zero-config bootstrap, ADR-0036).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatProvisioningMode {
    /// Enable-Chat auto-provisions the deployment Buzz community (single
    /// workspace model). Requires a bootstrap relay URL.
    Auto,
    /// Mapping is configured explicitly by an administrator (existing API).
    Manual,
}

impl ChatProvisioningMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChatProvisioningMode::Auto => "auto",
            ChatProvisioningMode::Manual => "manual",
        }
    }
}
```

- [ ] **Step 2: Parse the env keys** in `AppConfig::from_env`:
  - `RUSTSHARE_CHAT_PROVISIONING` — parse via `ChatProvisioningMode::as_str` round-trip; default `"manual"`; any other value → config error `"invalid RUSTSHARE_CHAT_PROVISIONING {v:?} (expected auto|manual)"`.
  - `RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL` — `Option<String>`.
- [ ] **Step 3: Startup validation** (extend the existing chat-authority validation, `validate_chat_authority` :285-311, or a sibling `validate_chat_provisioning` called at the same place):
  - `Auto` + authority != `buzz` → error `"RUSTSHARE_CHAT_PROVISIONING=auto requires RUSTSHARE_CHAT_AUTHORITY=buzz"`.
  - `Auto` + bootstrap URL missing → error `"RUSTSHARE_CHAT_PROVISIONING=auto requires RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL"`.
  - Bootstrap URL present → parse with `url::Url`, scheme must be `ws` or `wss` (reuse the scheme check from `validate_relay_url` in `handlers/chat_identity.rs` without the DNS part); else error. No DNS/SSRF resolution at startup — the gateway's `validated_http` enforces the SSRF pin per request.
- [ ] **Step 4: Unit tests** in `config.rs`'s test module: valid auto+buzz+ws URL; auto+local → error; auto+missing URL → error; auto+bad scheme → error; invalid mode → error; default is manual.
- [ ] **Step 5: Run + commit** — `cargo test -p rustshare-server config` (or the config test path used by the crate), `cargo fmt --all --check`, then `git add backend/server/src/config.rs && git commit -s -m "feat(server): chat provisioning mode and bootstrap relay URL config"`

### Task B2: Gateway discovery client

**Files:**
- Modify: `backend/server/src/buzz_gateway.rs`

- [ ] **Step 1: Add the identity struct** (near `BuzzStatePage`):

```rust
/// Community-identity discovery response (`GET /api/v1/relay/community`) —
/// the zero-config bootstrap contract (ADR-0036). `relay_pubkey` is the
/// relay's stable public key and must own the response signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuzzCommunityIdentity {
    pub community_id: String,
    pub host: String,
    pub relay_pubkey: String,
    pub evaluated_at: i64,
}
```

- [ ] **Step 2: Add the public method** (model it on `page_state` exactly):

```rust
    /// Discover the community bound to `relay_url`'s host and the relay's
    /// stable public key — the bootstrap step taken before any mapping row
    /// exists, so there is no pin to verify against yet. The response must be
    /// a kind-19030 event whose signature verifies against the `relay_pubkey`
    /// CLAIMED IN THE CONTENT (this proves the relay controls the private
    /// key), with `evaluated_at` fresh (≤ [`MAX_EVALUATED_AT_AGE_SECS`]) and
    /// well-formed fields. Anything else fails closed.
    pub async fn community_identity(
        &self,
        relay_url: &str,
    ) -> Result<BuzzCommunityIdentity, BuzzAuthorityError> {
        let (base, http) = self.validated_http(relay_url).await?;
        let url = base.join("/api/v1/relay/community").map_err(|e| {
            BuzzAuthorityError::Config(format!("cannot build relay community URL: {e}"))
        })?;
        let header = self.nip98_header("GET", &url, None).await?;
        let response = http
            .get(url)
            .timeout(self.timeout)
            .header("Authorization", header)
            .send()
            .await
            .map_err(|e| {
                log_relay_error(relay_url, BuzzAuthorityError::Transport(e.to_string()))
            })?;
        let raw = read_response_json(response)
            .await
            .map_err(|e| log_relay_error(relay_url, e))?;
        self.identity_from_19030(&raw)
            .map_err(|e| log_relay_error(relay_url, e))
    }
```

- [ ] **Step 3: Add the verifier + structural validation** (model `identity_from_19030` on `verify_19030`, but the pubkey check is against the content-claimed key):

```rust
    /// Verify a kind-19030 community-identity response (trust model: see
    /// [`Self::community_identity`]).
    fn identity_from_19030(
        &self,
        raw: &Value,
    ) -> Result<BuzzCommunityIdentity, BuzzAuthorityError> {
        let event = NostrEvent::from_json(raw.to_string()).map_err(|e| {
            BuzzAuthorityError::InvalidResponse(format!(
                "response is not a valid Nostr event: {e}"
            ))
        })?;
        if event.kind.as_u16() != RELAY_RESPONSE_KIND {
            return Err(BuzzAuthorityError::InvalidResponse(format!(
                "response kind {} is not {RELAY_RESPONSE_KIND}",
                event.kind.as_u16()
            )));
        }
        event.verify().map_err(|e| {
            BuzzAuthorityError::InvalidResponse(format!(
                "response signature verification failed: {e}"
            ))
        })?;
        let identity: BuzzCommunityIdentity =
            serde_json::from_str(&event.content).map_err(|e| {
                BuzzAuthorityError::InvalidResponse(format!(
                    "community identity content is invalid: {e}"
                ))
            })?;
        if event.pubkey.to_hex() != identity.relay_pubkey {
            return Err(BuzzAuthorityError::InvalidResponse(
                "response pubkey does not match the claimed relay pubkey".to_string(),
            ));
        }
        validate_community_identity(&identity)?;
        Ok(identity)
    }
```

```rust
/// Structural validation of a discovered community identity — fail closed on
/// any malformed field (ADR-0036).
fn validate_community_identity(
    identity: &BuzzCommunityIdentity,
) -> Result<(), BuzzAuthorityError> {
    let age = nostr::Timestamp::now().as_secs() as i64 - identity.evaluated_at;
    if !(0..=MAX_EVALUATED_AT_AGE_SECS as i64).contains(&age) {
        return Err(BuzzAuthorityError::InvalidResponse(format!(
            "identity evaluated_at is {age}s from the client clock (max {MAX_EVALUATED_AT_AGE_SECS}s)"
        )));
    }
    uuid::Uuid::parse_str(&identity.community_id).map_err(|_| {
        BuzzAuthorityError::InvalidResponse(format!(
            "community_id {:?} is not a UUID",
            identity.community_id
        ))
    })?;
    if identity.host.trim().is_empty() {
        return Err(BuzzAuthorityError::InvalidResponse("host is empty".into()));
    }
    let is_64_hex = identity.relay_pubkey.len() == 64
        && identity.relay_pubkey.bytes().all(|b| b.is_ascii_hexdigit());
    if !is_64_hex {
        return Err(BuzzAuthorityError::InvalidResponse(format!(
            "relay_pubkey {:?} is not 64 hex digits",
            identity.relay_pubkey
        )));
    }
    Ok(())
}
```

(Check whether the file already has a hex/UUID helper to reuse; `uuid` is already a dependency.)

- [ ] **Step 4: Unit tests** in the `buzz_gateway.rs` test module, reusing the existing fake-relay helpers:
  1. `community_identity_round_trips` — fake relay returns a signed 19030 with content pubkey == signing key → `Ok(identity)` with parsed fields.
  2. `community_identity_rejects_response_signed_by_other_key` — event signed by a key ≠ content `relay_pubkey` → `InvalidResponse`.
  3. `community_identity_rejects_stale_evaluated_at` — `evaluated_at = now - 120` → `InvalidResponse`.
  4. `community_identity_rejects_non_19030_kind` → `InvalidResponse`.
  5. `community_identity_rejects_malformed_content` — e.g. `community_id: "not-a-uuid"` → `InvalidResponse`.
  6. `community_identity_fails_closed_when_relay_unreachable` → `Transport`.
  7. `community_identity_maps_relay_401_to_unauthorized` → `Unauthorized` (mirror how the existing tests assert status mapping).
- [ ] **Step 5: Run + commit** — `cargo test -p rustshare-server buzz_gateway` + fmt/clippy, then `git add backend/server/src/buzz_gateway.rs && git commit -s -m "feat(server): gateway bootstrap community-identity discovery"`

### Task B3: Storage — conflict-safe provision insert

**Files:**
- Modify: `backend/crates/storage/src/chat_identity.rs`

- [ ] **Step 1: Add outcome/error types** (near `insert_mapping`):

```rust
/// Idempotent provisioning insert outcome (ADR-0036).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionMappingOutcome {
    /// This call created the mapping row.
    Inserted,
    /// A concurrent call created it; the winner's row is returned and MUST be
    /// verified against the discovered community before being accepted.
    AlreadyExists(WorkspaceCommunityMapping),
}

/// Provisioning insert failure — community-level conflicts fail closed.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionMappingError {
    #[error("community is already mapped to another workspace")]
    CommunityInUse,
    #[error(transparent)]
    Other(#[from] sqlx::Error),
}
```

(Check the crate's existing error conventions; `WorkspaceCommunityMapping` is already exported by this module.)

- [ ] **Step 2: Add `provision_mapping`** (the classifier below must match the REAL constraint names — Step 4 verifies):

```rust
    /// Insert the mapping only if the workspace has none, resolving races on
    /// the (tenant_id, workspace_id) unique key. A concurrent insert for the
    /// same workspace returns the winner (`AlreadyExists`). Any conflict on
    /// the (tenant_id, community_id) unique key or the one-active-per-
    /// community partial index is `CommunityInUse` — a mapping can never be
    /// silently stolen from or shared with another workspace.
    pub async fn provision_mapping(
        &self,
        mapping: &WorkspaceCommunityMapping,
    ) -> Result<ProvisionMappingOutcome, ProvisionMappingError> {
        let result = sqlx::query(
            "INSERT INTO chat_workspace_communities
                (mapping_id, tenant_id, workspace_id, community_id, relay_url, relay_pubkey, active)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(mapping.tenant_id.0)
        .bind(mapping.workspace_id.0)
        .bind(&mapping.community_id)
        .bind(&mapping.relay_url)
        .bind(&mapping.relay_pubkey)
        .bind(mapping.active)
        .execute(&self.pool)
        .await;
        match result {
            Ok(_) => Ok(ProvisionMappingOutcome::Inserted),
            Err(error) => {
                let constraint = error
                    .as_database_error()
                    .and_then(|db| db.constraint())
                    .map(str::to_string);
                match constraint.as_deref() {
                    Some("chat_workspace_communities_tenant_id_workspace_id_key") => {
                        let existing = self
                            .mapping(mapping.tenant_id, mapping.workspace_id)
                            .await?;
                        Ok(ProvisionMappingOutcome::AlreadyExists(existing))
                    }
                    Some("chat_workspace_communities_tenant_id_community_id_key")
                    | Some("chat_workspace_communities_active_community") => {
                        Err(ProvisionMappingError::CommunityInUse)
                    }
                    _ => Err(ProvisionMappingError::Other(error)),
                }
            }
        }
    }
```

- [ ] **Step 3: Verify constraint names** — in the test DB, run `SELECT conname FROM pg_constraint WHERE conrelid = 'chat_workspace_communities'::regclass ORDER BY conname;` and confirm the exact names; fix Step 2's classifier if they differ (the partial index name comes from `20260810000005_unique_active_community_mapping.up.sql`).
- [ ] **Step 4: Compile + commit** — `cargo build -p rustshare-storage`, `cargo fmt --all --check`, then `git add backend/crates/storage/src/chat_identity.rs && git commit -s -m "feat(storage): conflict-safe idempotent mapping provisioning"`

### Task B4: ChatBootstrapService

**Files:**
- Create: `backend/server/src/services/chat_bootstrap.rs` (module file registered in `services/mod.rs`)

- [ ] **Step 1: Create the service** — complete code:

```rust
//! Zero-config Chat bootstrap (ADR-0036): discover the deployment Buzz
//! community over the authoritative relay and create the explicit
//! Workspace↔Community mapping, idempotently and without ever overwriting an
//! existing mapping.

use std::sync::Arc;

use rustshare_resource_auth::chat_identity::WorkspaceCommunityMapping;
use rustshare_storage::chat_identity::{
    ChatIdentityStore, ProvisionMappingError, ProvisionMappingOutcome,
};
use rustshare_types::ids::{TenantId, WorkspaceId};

use crate::buzz_gateway::{BuzzAuthorityError, BuzzGatewayClient};

/// Result of a provisioning attempt — both variants are idempotent successes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionOutcome {
    /// This call created the mapping row.
    Inserted {
        community_id: String,
        relay_url: String,
        relay_pubkey: String,
    },
    /// A mapping already existed for the workspace and matches the discovered
    /// community (or a concurrent provision won with the same community).
    AlreadyConfigured {
        community_id: String,
        relay_url: String,
        relay_pubkey: Option<String>,
    },
}

/// Provisioning failures. `CommunityInUse`/`CommunityMismatch` are expected
/// admin-facing conflicts (HTTP 409); `Discovery` means the relay was
/// reachable-but-invalid or unreachable and Chat stays safely unconfigured.
#[derive(Debug, thiserror::Error)]
pub enum ChatBootstrapError {
    #[error("relay discovery failed: {0}")]
    Discovery(String),
    #[error("community {community_id} is already mapped to another workspace")]
    CommunityInUse { community_id: String },
    #[error(
        "community mismatch: the relay identifies community {relay}, but the workspace is mapped to {mapped}"
    )]
    CommunityMismatch { relay: String, mapped: String },
    #[error(transparent)]
    Storage(#[from] ProvisionMappingError),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

/// Bootstrap service. Constructed only in buzz authority mode (see
/// `bootstrap.rs`); in local/development mode it is `None` and provisioning
/// is unavailable.
pub struct ChatBootstrapService {
    gateway: Arc<BuzzGatewayClient>,
    store: Arc<ChatIdentityStore>,
    bootstrap_relay_url: String,
}

impl ChatBootstrapService {
    pub fn new(
        gateway: Arc<BuzzGatewayClient>,
        store: Arc<ChatIdentityStore>,
        bootstrap_relay_url: String,
    ) -> Self {
        Self {
            gateway,
            store,
            bootstrap_relay_url,
        }
    }

    /// Discover → verify → map. Never overwrites an existing mapping; never
    /// creates a mapping from a read request — only this service (called from
    /// enable-Chat in auto mode and from the admin provision endpoint) writes.
    pub async fn provision(
        &self,
        tenant_id: TenantId,
        workspace_id: WorkspaceId,
    ) -> Result<ProvisionOutcome, ChatBootstrapError> {
        // 1. Discovery (read-only): the relay is the authority on community
        //    identity and its own pubkey (signature-verified, see gateway).
        let identity = self
            .gateway
            .community_identity(&self.bootstrap_relay_url)
            .await
            .map_err(|error| ChatBootstrapError::Discovery(error.to_string()))?;

        // 2. Existing mapping: verify, never overwrite.
        if let Some(existing) = self
            .store
            .mapping(tenant_id, workspace_id)
            .await
            .map_err(|error| ChatBootstrapError::Internal(anyhow::anyhow!(error.to_string())))?
        {
            if existing.community_id == identity.community_id {
                return Ok(ProvisionOutcome::AlreadyConfigured {
                    community_id: existing.community_id,
                    relay_url: existing.relay_url,
                    relay_pubkey: existing.relay_pubkey,
                });
            }
            return Err(ChatBootstrapError::CommunityMismatch {
                relay: identity.community_id,
                mapped: existing.community_id,
            });
        }

        // 3. Idempotent, race-safe insert with a pinned relay pubkey.
        let mapping = WorkspaceCommunityMapping {
            tenant_id,
            workspace_id,
            community_id: identity.community_id.clone(),
            relay_url: self.bootstrap_relay_url.clone(),
            relay_pubkey: Some(identity.relay_pubkey.clone()),
            active: true,
        };
        match self.store.provision_mapping(&mapping).await {
            Ok(ProvisionMappingOutcome::Inserted) => Ok(ProvisionOutcome::Inserted {
                community_id: identity.community_id,
                relay_url: self.bootstrap_relay_url.clone(),
                relay_pubkey: identity.relay_pubkey,
            }),
            Ok(ProvisionMappingOutcome::AlreadyExists(existing)) => {
                if existing.community_id == identity.community_id {
                    Ok(ProvisionOutcome::AlreadyConfigured {
                        community_id: existing.community_id,
                        relay_url: existing.relay_url,
                        relay_pubkey: existing.relay_pubkey,
                    })
                } else {
                    Err(ChatBootstrapError::CommunityMismatch {
                        relay: identity.community_id,
                        mapped: existing.community_id,
                    })
                }
            }
            Err(ProvisionMappingError::CommunityInUse) => {
                Err(ChatBootstrapError::CommunityInUse {
                    community_id: identity.community_id,
                })
            }
            Err(error) => Err(ChatBootstrapError::Storage(error)),
        }
    }
}
```

Verify against the real crate layout before finalizing: the exact module path/name of the mapping row type and the store type (may be `ChatIdentityStore` in `rustshare_storage::chat_identity`, or exposed under a different name — grep for `WorkspaceCommunityMapping` and the store struct), `TenantId`/`WorkspaceId` locations, and whether `anyhow` is available in the server crate (it is used elsewhere in `backend/server`).

- [ ] **Step 2: Compile + fmt** — `cargo build -p rustshare-server`, `cargo fmt --all --check`.
- [ ] **Step 3: Commit** — `git add backend/server/src/services/ && git commit -s -m "feat(server): chat bootstrap service (discover + idempotent map)"`

### Task B5: Handlers + routes

**Files:**
- Modify: `backend/server/src/handlers/chat_identity.rs`
- Modify: `backend/server/src/routes.rs`

- [ ] **Step 1: Provision handler** — add to `handlers/chat_identity.rs`, reusing the exact admin-identity/tenant-scope preamble of `configure_mapping` (:118-129):

```rust
/// Auto-provision the deployment Buzz community for a workspace (idempotent,
/// ADR-0036). Admin-only and tenant-scoped. On success the workspace has a
/// mapping row (either just created or pre-existing and verified identical).
#[utoipa::path(
    post,
    path = "/api/v1/admin/applications/chat/workspaces/{workspace_id}/provision",
    tag = "Chat (admin)",
    responses(
        (status = 201, description = "Provisioned or already configured", body = ProvisionCommunityResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::handlers::ErrorResponse),
        (status = 409, description = "Community in use or mismatch", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn provision_community_mapping(
    AdminUser { user_id: admin_id }: AdminUser,
    auth: AuthenticatedUser,
    State(state): State<AppState>,
    Path(workspace_id): Path<Uuid>,
) -> Result<(StatusCode, Json<ProvisionCommunityResponse>), AppError> {
    // (same admin_tenant + ensure_workspace_scope preamble as configure_mapping)
    let outcome = state
        .chat_bootstrap
        .as_ref()
        .ok_or_else(|| AppError::bad_request("chat provisioning is not enabled in this mode"))?
        .provision(TenantId(auth.tenant_id), WorkspaceId(workspace_id))
        .await
        .map_err(bootstrap_error_to_app_error)?;
    let (status, community_id, relay_url, relay_pubkey) = match outcome {
        ProvisionOutcome::Inserted { community_id, relay_url, relay_pubkey } => {
            ("created", community_id, relay_url, relay_pubkey)
        }
        ProvisionOutcome::AlreadyConfigured { community_id, relay_url, relay_pubkey } => {
            ("already_configured", community_id, relay_url, relay_pubkey.unwrap_or_default())
        }
    };
    Ok((
        StatusCode::CREATED,
        Json(ProvisionCommunityResponse {
            status: status.to_string(),
            community_id,
            relay_url,
            relay_pubkey,
        }),
    ))
}

/// Admin diagnostics: the workspace's current mapping, including the pinned
/// relay pubkey. Never exposed on the user-facing status surface.
#[utoipa::path(
    get,
    path = "/api/v1/admin/applications/chat/workspaces/{workspace_id}/community",
    tag = "Chat (admin)",
    responses(
        (status = 200, description = "Mapping", body = AdminCommunityMappingResponse),
        (status = 404, description = "No mapping", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_community_mapping(
    AdminUser { user_id: admin_id }: AdminUser,
    auth: AuthenticatedUser,
    State(db): State<DatabaseState>,
    Path(workspace_id): Path<WorkspaceId>,
) -> Result<Json<AdminCommunityMappingResponse>, AppError> {
    // (same admin_tenant + ensure_workspace_scope preamble)
    let mapping = db
        .chat_identity_store
        .mapping(TenantId(auth.tenant_id), workspace_id)
        .await
        .map_err(internal_db)?
        .ok_or_else(|| AppError::not_found("no community mapping for this workspace"))?;
    Ok(Json(AdminCommunityMappingResponse {
        community_id: mapping.community_id,
        relay_url: mapping.relay_url,
        relay_pubkey: mapping.relay_pubkey,
        active: mapping.active,
    }))
}

/// Provisioning response — idempotent by design (`status` says which path).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProvisionCommunityResponse {
    pub status: String,
    pub community_id: String,
    pub relay_url: String,
    pub relay_pubkey: String,
}

/// Admin-only mapping diagnostics (includes the pin; not for user surfaces).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AdminCommunityMappingResponse {
    pub community_id: String,
    pub relay_url: String,
    pub relay_pubkey: Option<String>,
    pub active: bool,
}

fn bootstrap_error_to_app_error(error: ChatBootstrapError) -> AppError {
    match error {
        ChatBootstrapError::CommunityInUse { .. } | ChatBootstrapError::CommunityMismatch { .. } => {
            AppError::conflict(error.to_string())
        }
        other => AppError::internal(format!("chat provisioning failed: {other}")),
    }
}
```

Adapt to the real types: `state.chat_bootstrap` field name (Task B6 wiring), the store field accessor on `DatabaseState` (the handler above uses `db.chat_identity_store` — verify the actual accessor), `AppError::not_found`/`AppError::conflict`/`AppError::bad_request` constructors (grep existing usage), and whether `Status`/`workspace scope` helpers are named as in `configure_mapping`.

- [ ] **Step 2: Routes** in `routes.rs` next to the existing mapping routes (:727-734):

```rust
        .route(
            "/api/v1/admin/applications/chat/workspaces/{workspace_id}/community",
            get(crate::handlers::chat_identity::get_community_mapping),
        )
        .route(
            "/api/v1/admin/applications/chat/workspaces/{workspace_id}/provision",
            post(crate::handlers::chat_identity::provision_community_mapping),
        )
```

- [ ] **Step 3: Compile + commit** — `cargo build -p rustshare-server`, fmt, then `git add backend/server/src/handlers/chat_identity.rs backend/server/src/routes.rs && git commit -s -m "feat(server): chat provision and admin diagnostics endpoints"`

### Task B6: Enable hook + wiring

**Files:**
- Modify: `backend/server/src/handlers/admin/applications.rs`
- Modify: `backend/server/src/bootstrap.rs`

- [ ] **Step 1: Enable hook** — in the `enable_application` handler (route `POST /api/v1/admin/applications/{key}/enable`), AFTER the enable succeeds, add:

```rust
        // Zero-config bootstrap (ADR-0036): in auto mode, enabling Chat
        // provisions the deployment Buzz community immediately. A failure is
        // logged and leaves Chat safely unconfigured — the admin retries via
        // POST .../chat/workspaces/{id}/provision.
        if key == crate::authz::chat_owner::CHAT_APPLICATION_ID
            && state.config.chat_provisioning == ChatProvisioningMode::Auto
        {
            if let Some(bootstrap) = &state.chat_bootstrap {
                if let Err(error) = bootstrap
                    .provision(TenantId(tenant_id), WorkspaceId(tenant_id))
                    .await
                {
                    tracing::warn!(%error, "chat auto-provisioning failed; chat remains unconfigured");
                }
            }
        }
```

Adapt: the handler's actual `tenant_id` variable and the `TenantId`/`WorkspaceId` wrapper names used in that module (note: `enable_application` treats workspace == tenant, per application_service.rs:491).

- [ ] **Step 2: Wiring** — in `bootstrap.rs`'s buzz-mode branch (around :850-895), after the `BuzzGatewayClient`/`BuzzGatewayAuthority` are built:

```rust
        // Zero-config bootstrap service (ADR-0036) — only in buzz mode.
        let chat_bootstrap = Some(Arc::new(ChatBootstrapService::new(
            gateway.clone(),
            db.chat_identity_store.clone(),
            config.chat_bootstrap_relay_url.clone().ok_or_else(|| {
                anyhow::anyhow!("RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL required for chat provisioning")
            })?,
        )));
```

and in the local-mode branch: `let chat_bootstrap = None;` (the config validation in Task B1 already guarantees auto-mode never reaches local mode). Add the `chat_bootstrap: Option<Arc<ChatBootstrapService>>` field to `AppState` (or the app-state builder used by handlers — follow the existing `ask_workspace_service` pattern).

- [ ] **Step 3: Compile + fmt** — `cargo build -p rustshare-server`, `cargo fmt --all --check`.
- [ ] **Step 4: Commit** — `git add backend/server/src/handlers/admin/applications.rs backend/server/src/bootstrap.rs && git commit -s -m "feat(server): auto-provision chat community on enable in auto mode"`

### Task B7: Integration tests — bootstrap security matrix

**Files:**
- Create: `backend/tests/chat_bootstrap_test.rs`

- [ ] **Step 1: Test harness** — follow the existing `backend/tests/*.rs` conventions (sqlx pool against `TEST_DATABASE_URL`-style env / the CI postgres service; migrations applied via the same helper other tests use). Spawn a fake relay with `axum` (or reuse an existing fake-relay helper from another test file if one is shareable) exposing:
  - `GET /api/v1/relay/community` — configurable per test: valid signed identity (NIP-98 not verified by the fake), wrong-key signature, stale `evaluated_at`, malformed content, HTTP 500, or connection-refused (unreachable).
  - `POST /api/v1/relay/access/check` — optional, for proof 9.
  Build `ChatBootstrapService` directly with `BuzzGatewayClient::new_for_test(...)` + a real `ChatIdentityStore`.
- [ ] **Step 2: Tests — one per security proof (goal §5):**
  1. `bootstrap_maps_only_authenticated_tenant_workspace` — handler-level (see Step 3): non-admin → 403; admin of tenant A provisioning a workspace of tenant B → 403/404. Service-level: mapping row's `tenant_id` == the caller tenant.
  2. `bootstrap_rejects_untrusted_discovery_response` — fake returns an event signed by a key different from content `relay_pubkey` → `Err(Discovery)`; `mapping()` returns None (no row).
  3. `bootstrap_pins_verified_relay_pubkey` — success stores `relay_pubkey` matching the fake's key, matching `^[0-9a-f]{64}$` (CHECK constraint).
  4. `bootstrap_is_idempotent` — provision twice: first `Inserted`, second `AlreadyConfigured`; exactly one row; community identical.
  5. `cross_tenant_community_collision_fails_closed` — tenant A maps community X; tenant B provisions against a fake returning X → `CommunityInUse`; tenant B has no row.
  6. `existing_manual_mapping_is_never_overwritten` — pre-insert a manual mapping (community M, custom relay_url/pubkey) via `insert_mapping`; fake returns M → `AlreadyConfigured`, row byte-identical; fake returns N → `CommunityMismatch`, row still byte-identical.
  7. `unreachable_relay_leaves_chat_safely_unconfigured` — fake unreachable → `Err(Discovery)`; no row; (status surface: `chat_status` still reports `mapping: None` — assert via the handler if the harness allows, else at storage level).
  8. `workspaces_cannot_share_one_community` — same tenant, workspace1 → X ok; workspace2 → X → `CommunityInUse` (exercises `UNIQUE(tenant_id, community_id)`); repeat with a second tenant (exercises the active partial index) → `CommunityInUse`.
  9. `authorization_continues_after_provisioning` — after success, `mapping()` returns the discovered community/relay/pubkey; a gateway `check_access` against the fake (with the stored pin) round-trips to `Allow`/`Deny` without error.
  10. `no_direct_buzz_db_access` — structural: run `scripts/guard-buzz-no-acl.sh` (CI also runs it); add a unit-level assertion if the guard needs a new pattern for `chat_bootstrap.rs` (update the guard script in Phase E only if it misses the new module).
- [ ] **Step 3: Handler-level tests** (follow the existing handler test harness, e.g. `chat_app_read_test.rs`): `provision_endpoint_requires_admin` (403 non-admin), `provision_endpoint_returns_409_on_community_in_use`, `provision_endpoint_returns_201_with_identity`, `get_community_mapping_returns_pin_for_admin` / `404_when_unconfigured`.
- [ ] **Step 4: sqlx metadata** — run `cargo sqlx prepare --workspace` against a live PG (the new `provision_mapping` query) and commit the updated `.sqlx/` files. Verify `SQLX_OFFLINE=true cargo sqlx prepare --workspace --check` passes.
- [ ] **Step 5: Run the new suite** — `SQLX_OFFLINE=true cargo test -p rustshare-server --test chat_bootstrap_test` (with the local PG). Expected: all pass.
- [ ] **Step 6: Commit** — `git add backend/tests/chat_bootstrap_test.rs .sqlx && git commit -s -m "test(server): chat bootstrap security matrix (10 proofs)"`

---

## Phase C — Frontend

### Task C1: Normal-user copy

**Files:**
- Modify: `frontend/src/lib/components/chat/ChatApplicationView.svelte`
- Modify: `frontend/src/lib/components/chat/ChatApplicationView.test.ts`

- [ ] **Step 1: Replace the notice** (`ChatApplicationView.svelte:181-184`):

```svelte
{:else if !status.mapping}
	<div class="p-6 text-base-content/60">
		Chat is being configured for this workspace.
	</div>
```

This is the ONLY user-facing change to the component — no other branch, query, or polling logic changes. (When provisioning fails, normal users keep seeing this neutral state; diagnostics live in the admin page.)

- [ ] **Step 2: Update the test** (`ChatApplicationView.test.ts:126-131`) — rename to "shows the configuring notice when no community mapping exists" and change the assertion from `/No Buzz community is mapped/` to `/Chat is being configured for this workspace/`. Keep every other test untouched.
- [ ] **Step 3: Run frontend tests** — `cd frontend && npm run test` (vitest). Expected: Chat suite green.
- [ ] **Step 4: Commit** — `git add frontend/src/lib/components/chat/ChatApplicationView.svelte frontend/src/lib/components/chat/ChatApplicationView.test.ts && git commit -s -m "feat(ui): neutral 'being configured' chat state for normal users"`

### Task C2: Admin Chat page

**Files:**
- Modify: `frontend/src/lib/api/chat.ts`
- Create: `frontend/src/routes/admin/applications/chat/+page.svelte`
- Create: `frontend/src/routes/admin/applications/chat/+page.svelte.test.ts` (or follow the repo's existing vitest convention for admin pages)

- [ ] **Step 1: API client** — add to `frontend/src/lib/api/chat.ts`, matching the file's existing error/response conventions:

```ts
export async function provisionChatCommunity(workspaceId: string) {
	// POST /api/v1/admin/applications/chat/workspaces/{workspaceId}/provision
	// Returns { status: 'created' | 'already_configured', community_id, relay_url, relay_pubkey }
}

export async function getChatCommunityMapping(workspaceId: string) {
	// GET /api/v1/admin/applications/chat/workspaces/{workspaceId}/community
	// Returns { community_id, relay_url, relay_pubkey, active } or throws 404
}

export async function connectChatCommunity(
	workspaceId: string,
	body: { community_id: string; relay_url: string; relay_pubkey?: string }
) {
	// POST /api/v1/admin/applications/chat/workspaces/{workspaceId}/community
}
```

- [ ] **Step 2: Page** — `frontend/src/routes/admin/applications/chat/+page.svelte`, an admin-only page (guard matches the existing admin routes):
  - On load: fetch `getChatCommunityMapping(workspaceId)` (404 → "not configured") and the chat status via the existing `getChatStatus()`.
  - **Status card**: "Chat is not enabled" (enable link to the applications list) vs enabled; mapping details when present — `community_id`, `relay_url`, `relay_pubkey` (monospace, admins only), `active`.
  - **Actions card**:
    - Primary button **"Set up automatically"** → `provisionChatCommunity(workspaceId)`; on success show the mapping (status `created` vs `already_configured`); on error show the server message inline (e.g. 409 community-in-use).
    - Secondary button **"Connect existing Chat deployment"** toggling a small form: `relay_url` (ws/wss), `community_id`, optional `relay_pubkey` → `connectChatCommunity(...)`; on success refetch mapping.
    - Caption: "Existing mappings are never overwritten automatically."
  - No Buzz implementation terminology in button/heading copy beyond the config values themselves.
  - Determine `workspaceId` the same way the existing admin pages obtain the current workspace/tenant (check `frontend/src/routes/admin/**` + load data).
  - Add a link to this page from `frontend/src/routes/admin/applications/+page.svelte` (e.g. a "Chat settings" link on the Chat row).
- [ ] **Step 3: Page test** — mocked `fetch` (follow `ChatApplicationView.test.ts` patterns): renders "Set up automatically"; clicking it POSTs `/provision` and renders the returned `community_id`; "Connect existing" form POSTs `/community`; error responses render the server message.
- [ ] **Step 4: Run checks** — `cd frontend && npm run check && npm run lint && npm run test`. Expected: all green.
- [ ] **Step 5: Commit** — `git add frontend/src/lib/api/chat.ts frontend/src/routes/admin/applications/chat && git commit -s -m "feat(ui): admin chat provisioning page (set up automatically / connect existing)"`

---

## Phase D — Deployment and image pinning

### Task D1: Alpha compose + env examples

**Files:**
- Modify: `docker-compose.alpha.yml`
- Modify: `.env.example`
- Modify: `backend/.env.example`

- [ ] **Step 1: Backend env in `docker-compose.alpha.yml`** (backend override block, near the existing `RUSTSHARE_CHAT_*` vars ~:44-48):

```yaml
      RUSTSHARE_CHAT_PROVISIONING: ${RUSTSHARE_CHAT_PROVISIONING:-auto}
      RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL: ${BUZZ_RELAY_WS:-ws://localhost:7447}
```

- [ ] **Step 2: Relay image** in `docker-compose.alpha.yml` (buzz-relay service, ~:107): change the image to

```yaml
    image: ${BUZZ_RELAY_IMAGE:-ghcr.io/kubedoio/buzz:<PINNED_SHA>}
```

where `<PINNED_SHA>` is the 7-hex sha tag of the merged kubedoio/buzz main build that includes the community-identity endpoint (set in Task D3 after the Buzz PR merges; until then the E2E uses `BUZZ_RELAY_IMAGE=buzz-relay:dogfood` built from the worktree — see Phase E). Keep the explanatory comment (image must be a v1alpha1-capable, main-built image; never a floating upstream tag).
- [ ] **Step 3: `.env.example`** (root) — add, with comments:

```
# Chat zero-config bootstrap (see docs/architecture/elembra-chat-bootstrap.md):
# RUSTSHARE_CHAT_PROVISIONING=auto            # auto|manual (server default: manual)
# RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL=ws://localhost:7447
# BUZZ_RELAY_IMAGE=ghcr.io/kubedoio/buzz:sha-<7>   # supported image from merged kubedoio/buzz main
```

- [ ] **Step 4: `backend/.env.example`** — add `RUSTSHARE_CHAT_PROVISIONING` and `RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL` next to the other `RUSTSHARE_CHAT_*` keys.
- [ ] **Step 5: Compose validation** — `docker compose -f docker-compose.yml -f docker-compose.alpha.yml config >/dev/null`. Expected: no error.
- [ ] **Step 6: Commit** — `git add docker-compose.alpha.yml .env.example backend/.env.example && git commit -s -m "chore(deploy): auto-provisioning env and supported relay image default"`

### Task D2: Image publishing (blocked on Buzz PR merge — human step)

- [ ] **Step 1:** Once kubedoio/buzz#<PR> is merged (human), confirm the docker workflow published: `gh api "users/kubedoio/packages/container/buzz/versions" --jq '.[0:3] | map({name, tags})'` shows a `main`-push version tagged `sha-<7>`; and `docker buildx imagetools inspect ghcr.io/kubedoio/buzz:sha-<7>` resolves.
- [ ] **Step 2:** Verify the image is the merged main: `docker buildx imagetools inspect ghcr.io/kubedoio/buzz:main` and confirm provenance (`gh attestation verify oci://ghcr.io/kubedoio/buzz:sha-<7> --owner kubedoio`).
- [ ] **Step 3:** Update the `<PINNED_SHA>` in `docker-compose.alpha.yml` to the exact sha tag, update `.env.example` comment, commit (`-s`), and record the tag in the return report.
- [ ] **Step 4:** If the workflow does NOT publish (e.g. GHCR namespace/package settings missing), report it as a production blocker with the exact error; do NOT fall back to `ghcr.io/block/buzz:main`.

---

## Phase E — E2E and live conformance

### Task E1: Dogfood script — auto path + restart persistence

**Files:**
- Modify: `scripts/run-alpha-dogfood.sh`

- [ ] **Step 1: Replace P02c (manual mapping, ~:160-177)** with an auto-provision assertion (P02c'):

```bash
# P02c — zero-config bootstrap (ADR-0036): enabling Chat above triggered
# automatic provisioning; assert the discovered mapping matches the relay.
log "P02c: auto-provisioned mapping present"
mapping_json=""
for _ in $(seq 1 30); do
  if mapping_json=$(curl -fsS "${ADMIN_API_BASE}/admin/applications/chat/workspaces/${WORKSPACE_ID}/community" -H "Authorization: Bearer ${ADMIN_TOKEN}" 2>/dev/null); then
    break
  fi
  sleep 1
done
[ -n "${mapping_json}" ] || fail "P02c: mapping not provisioned within 30s after enable"
community_id=$(printf '%s' "${mapping_json}" | jq -r .community_id)
relay_url=$(printf '%s' "${mapping_json}" | jq -r .relay_url)
relay_pubkey=$(printf '%s' "${mapping_json}" | jq -r .relay_pubkey)
[ "${community_id}" = "${BUZZ_COMMUNITY_ID}" ] || fail "P02c: community ${community_id} != expected ${BUZZ_COMMUNITY_ID}"
[ "${relay_url}" = "${BUZZ_RELAY_WS}" ] || fail "P02c: relay_url ${relay_url} != ${BUZZ_RELAY_WS}"
[ "${relay_pubkey}" = "${BUZZ_RELAY_PUBKEY}" ] || fail "P02c: relay_pubkey mismatch"
log "P02c: mapping verified (community=${community_id})"
```

(Adapt the variable names/URLs to the script's actual conventions; the script already sources `.env` with `set -a` and defines `fail`/`log`.) Remove the `ALPHA_LOCAL_RELAY=1` fallback path and its comment — auto mode makes it obsolete. Also update P02d's number and any later references to the old P02c.

- [ ] **Step 2: Add restart-persistence proof** — after the existing outage/relogin proofs (P16 area) or as a new P18:

```bash
# P18 — restart persistence: mapping survives a stack restart and Buzz stays
# authoritative.
log "P18: restart persistence"
docker compose -f docker-compose.yml -f docker-compose.alpha.yml -f docker-compose.dogfood.yml restart backend buzz-relay
wait_for_health_backend
wait_for_health_relay
# mapping unchanged
mapping_json2=$(curl -fsS "${ADMIN_API_BASE}/admin/applications/chat/workspaces/${WORKSPACE_ID}/community" -H "Authorization: Bearer ${ADMIN_TOKEN}")
[ "$(printf '%s' "${mapping_json2}" | jq -r .community_id)" = "${BUZZ_COMMUNITY_ID}" ] || fail "P18: mapping changed after restart"
# authorization still enforced: bound user's read still works, revoked/foreign read still denied
# (reuse the existing per-user read/deny helpers from P04/P05)
```

(Reuse the script's existing health-wait and read-proof helpers; verify the exact compose `-f` set the script already uses and the health endpoints.)

- [ ] **Step 3: Run the full dogfood matrix locally** (with the relay image built from the buzz worktree branch: `docker build -t buzz-relay:dogfood .worktrees/buzz` or the script's existing image build; export `BUZZ_RELAY_IMAGE=buzz-relay:dogfood`). Expected: P01–P18 all pass, EXIT=0.
- [ ] **Step 4: Commit** — `git add scripts/run-alpha-dogfood.sh && git commit -s -m "test(e2e): auto-provisioning assertions and restart persistence proof"`

### Task E2: Live conformance — bootstrap proof (live_p13)

**Files:**
- Modify: `backend/tests/buzz_live_conformance_test.rs`

- [ ] **Step 1: Add the test** — a 12th `#[ignore = "requires live Buzz relay (scripts/run-buzz-conformance.sh)"]` test:

```rust
/// live_p13 — bootstrap discovery (ADR-0036): the community-identity
/// endpoint returns the deployment community and the relay pubkey, the
/// returned pubkey matches the harness's pinned expectation, the response is
/// signature-verified, and authorization still works with the discovered
/// identity (the client's pin for everything that follows).
#[tokio::test]
#[ignore = "requires live Buzz relay (scripts/run-buzz-conformance.sh)"]
async fn live_p13_bootstrap_identity_discovery() {
    let (relay_url, service_sk, expected_relay_pubkey) = live_env();
    let client = gateway_client_for(&service_sk);
    let identity = client
        .community_identity(&relay_url)
        .await
        .expect("community identity discovery must succeed against the live relay");
    assert_eq!(
        identity.relay_pubkey, expected_relay_pubkey,
        "discovered relay pubkey must match the harness pin"
    );
    // Cross-check community identity against an independent surface: every
    // state-event entry must report the same community.
    let page = client
        .page_state(&relay_url, &identity.relay_pubkey, None, 1, None)
        .await
        .expect("state paging must succeed with the discovered pin");
    if let Some(entry) = page.entries.first() {
        assert_eq!(
            entry.context.community_id, identity.community_id,
            "state events must report the discovered community"
        );
    }
    // Authorization continues with the discovered identity: a real check
    // round-trips to an Allow/Deny/NotFound decision (never an error).
    let decision = client
        .check_access(
            &relay_url,
            &identity.relay_pubkey,
            &buzz_access_check_request("workspace"),
        )
        .await
        .expect("access check with the discovered pin must succeed");
    assert!(
        matches!(decision, BuzzReadDecision::Allow | BuzzReadDecision::Deny | BuzzReadDecision::NotFound),
        "unexpected decision: {decision:?}"
    );
}
```

Adapt to the test file's actual helpers (`live_env`, `gateway_client_for`, the request-builder used by `live_p1`). The `run-buzz-conformance.sh` harness needs no env changes (`RUSTSHARE_BUZZ_LIVE_RELAY_PUBKEY` already exported); the relay under test must be built from the buzz worktree BRANCH (the harness builds from `.worktrees/buzz` — fine, branch checkout is in `.worktrees/buzz`).

- [ ] **Step 2: Run the live suite locally** — `./scripts/run-buzz-conformance.sh`. Expected: P1–P13 all pass (12 tests), latency budget proof still green.
- [ ] **Step 3: Commit** — `git add backend/tests/buzz_live_conformance_test.rs && git commit -s -m "test(conformance): live bootstrap identity discovery proof (p13)"`

### Task E3: Structural guard sweep

- [ ] **Step 1:** Run `./scripts/guard-buzz-no-acl.sh` — expected green. Inspect its patterns: if `backend/server/src/services/chat_bootstrap.rs` or the new handlers are not covered by its structural scans, add the missing pattern(s) (the guard must prove the new code still has no direct Buzz DB access and no ACL replacement). Commit any change with `-s -m "chore(scripts): extend no-acl guard to bootstrap code"`.
- [ ] **Step 2:** Run the Ask/Search workspace security matrix locally: `./scripts/run-ask-workspace-security.sh`. Expected: 16/16 ×2.

---

## Phase F — Docs

### Task F1: Topology + bootstrap architecture doc

**Files:**
- Create: `docs/architecture/elembra-chat-bootstrap.md`

- [ ] **Step 1: Write the doc** — sections:
  1. **Buzz provisioning model (verified)** — `communities` table (id/host, no name), startup auto-seed from `RELAY_URL` (`ensure_configured_community`, idempotent), community UUID discovery surfaces (startup log, operator API, state/events `context.community_id`, and the new discovery endpoint), relay pubkey derivation from `BUZZ_RELAY_PRIVATE_KEY` (NIP-11 `self` + signed 19030 responses), Host→Community binding (one host = one community; many communities per process; fail-closed on unmapped hosts).
  2. **Supported topology** — single relay host per community; deployment community = the startup-seeded row; multi-workspace deployments need one community (and host) per workspace — the operator API (`POST /operator/communities` `create_only:true`, NIP-98 + `RELAY_OPERATOR_PUBKEYS` + `RELAY_OPERATOR_API_ORIGIN`) is the documented per-workspace host provisioning path; auto mode deliberately maps only the deployment community and fails closed when it is taken.
  3. **Bootstrap model** — auto vs manual modes, the discovery contract (endpoint spec incl. trust chain: operator-configured relay URL → TLS + NIP-98 → signed response → pin stored), idempotency, no-overwrite guarantee, failure semantics.
  4. **Image strategy** — supported image `ghcr.io/kubedoio/buzz:sha-<7>` built from merged main, provenance verification, why floating `:main`/upstream tags are not used.
- [ ] **Step 2: Commit** — `git add docs/architecture/elembra-chat-bootstrap.md && git commit -s -m "docs: elembra chat zero-config bootstrap topology"`

### Task F2: ADR

**Files:**
- Create: `docs/adr/0036-elembra-chat-zero-config-bootstrap.md`
- Modify: `docs/adr/0034-elembra-chat-buzz-boundary.md`

- [ ] **Step 1: ADR-0036** — status Proposed→Accepted (decisions D1-D8), context (previous task proved production authorization; manual mapping was the only path), decision (provisioning modes, discovery endpoint + trust chain, conflict semantics, no persisted provisioning state, image pinning, multi-workspace model), consequences (bridge identity gains NO operator powers; one new read-only surface on the trusted-service allowlist; auto mode requires explicit config; manual mode unchanged). Reference ADR-0034/0035.
- [ ] **Step 2: Amend ADR-0034** — update the mapping section (~:381, "names and URLs are not inferred") to note the ADR-0036 provisioning contract that now allows verified discovery (still never inferred from unauthenticated input; discovery is relay-signed and pubkey-verified).
- [ ] **Step 3: Commit** — `git add docs/adr/ && git commit -s -m "docs(adr): zero-config chat bootstrap (0036) and boundary amendment"`

### Task F3: Runbook + readiness doc + changelog

**Files:**
- Modify: `docs/runbooks/elembra-alpha.md`
- Modify: `docs/architecture/elembra-chat-alpha-readiness.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Runbook** — replace the manual bootstrap instructions (~:91-136: CSRF curl and the SQL `INSERT INTO chat_workspace_communities` snippet) with: auto path (enable Chat in the admin UI or via the script; mapping appears automatically; verify via the Chat admin page), manual path ("Connect existing Chat deployment"), new env vars in the §3 table (`RUSTSHARE_CHAT_PROVISIONING`, `RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL`, `BUZZ_RELAY_IMAGE`), and the image section updated to the supported `ghcr.io/kubedoio/buzz:sha-<7>` with provenance verification. Keep the bind/admit flow docs (§4) unchanged.
- [ ] **Step 2: Readiness doc** — add a short section: provisioning contract (endpoint + client verification), the admin diagnostics surface, the known limitation (status hides mapping until a binding exists, so the UI shows "being configured" during bind entry — pre-existing, out of scope).
- [ ] **Step 3: CHANGELOG** — under Unreleased: "Elembra Chat: zero-config bootstrap — enabling Chat auto-provisions the deployment Buzz community (auto mode), admin provisioning page, neutral user-facing state copy, supported relay image pinned to a main-built SHA tag."
- [ ] **Step 4: Commit** — `git add docs/runbooks/elembra-alpha.md docs/architecture/elembra-chat-alpha-readiness.md CHANGELOG.md && git commit -s -m "docs: runbook/changelog for zero-config chat bootstrap"`
- [ ] **Step 5: Commit this plan** — `git add docs/superpowers/plans/2026-08-15-elembra-chat-zero-config-bootstrap.md && git commit -s -m "docs(plans): elembra chat zero-config bootstrap v1"`

---

## Phase G — Full validation, PR, return

### Task G1: Local validation baseline

- [ ] **Step 1: Rust** — from the repo root: `cargo fmt --all --check`; `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings`; `SQLX_OFFLINE=true cargo test --workspace --all-features --lib`; `SQLX_OFFLINE=true cargo sqlx prepare --workspace --check`. Expected: all green.
- [ ] **Step 2: Frontend** — `cd frontend && npm run check && npm run lint && npm run test && npm run build`. Expected: all green.
- [ ] **Step 3: Integration + conformance** — `scripts/run-ask-workspace-security.sh` (16/16 ×2), `./scripts/run-buzz-conformance.sh` (P1–P13 live, 12 tests), `scripts/run-alpha-dogfood.sh` (P01–P18, EXIT=0). Expected: all green.

### Task G2: RustShare PR + CI

- [ ] **Step 1: Push branch + PR** — `git push -u origin feat/chat-zero-config-bootstrap`; `gh pr create --repo kubedoio/rustshare --title "feat: Elembra Chat zero-config bootstrap v1" --body "..."` — body covers: what/why (ADR-0036), security note (per AGENTS.md safety boundaries: provisioning access control, mapping ownership/lifecycle, fail-closed discovery, image pinning), the 10 security proofs and how each is tested, test plan, doc updates, and the note that the Buzz PR (kubedoio/buzz#<N>, merged) is a dependency.
- [ ] **Step 2: CI** — wait for all checks on the PR (ci, frontend-ci, integration-tests incl. buzz-conformance + Ask matrix + DCO). Fix anything red with follow-up commits (`-s`). Do NOT merge (human step).
- [ ] **Step 3: Worktree hygiene** — confirm both working trees are clean (untracked: none; the plan doc is committed).

### Task G3: Return report

Deliver the 10 requested items: (1) Buzz provisioning analysis/topology; (2) bootstrap model selected; (3) Buzz changes (PR, branch, HEAD); (4) Elembra changes (PR, branch, HEAD); (5) image/version strategy; (6) UX behavior; (7) E2E/security results; (8) CI status; (9) remaining limitations; (10) PRs/branches/HEADs.

---

## Self-review notes

- Spec §1 (inspect provisioning model) → facts section + docs/architecture/elembra-chat-bootstrap.md (F1).
- Spec §2 (bootstrap modes) → D1/D2/D3 + ADR-0036 (F2); multi-workspace documented, not implemented (D6).
- Spec §3 (UX) → C1 (user copy), C2 (admin page); no mapping from GET/status (D3/D4).
- Spec §4 (image) → A3 + D1/D2 (reproducible CI build, SHA tag, documented source, alpha default).
- Spec §5 (10 security proofs) → B7 (matrix), E2 (live p13), E3 (guard), plus existing live suite.
- Spec §6 (E2E) → E1 (dogfood auto path + restart persistence), F3 (runbook), B7/E3 (focused security review).
- Return/limits → G3; no auto-merge anywhere (A4/G2).
