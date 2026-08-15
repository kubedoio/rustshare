# Elembra Buzz Production Authority Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Buzz the real runtime authority for Elembra Chat channel discovery and access, fail-closed in production, closing #245 and unblocking #243 — with no duplicate Elembra-side authorization system.

**Architecture:** Amend the v1alpha1 contract (batch + channel registry endpoints), extend the existing `BuzzAuthority` seam (`backend/crates/resource-auth/src/buzz_authority.rs`) with a batch method, implement the new endpoints client-side in `BuzzGatewayClient` (`backend/server/src/buzz_gateway.rs`), switch channel discovery in buzz mode from observation-derived listing to the relay registry, and prove everything with a live-relay conformance suite plus a bounded-latency batch test. Local mode stays as an explicit dev/Alpha fallback only.

**Tech Stack:** Rust/Axum backend (`backend/`), SQLx offline mode, fake-relay HTTP test harness (`backend/tests/buzz_authority_gateway_test.rs:218` `start_fake_buzz`), live relay via `docker-compose.alpha.yml` (`ghcr.io/block/buzz:main` → replaced by the kubedoio/buzz build once the companion Buzz PR lands).

**Dependency:** Companion plan `2026-08-14-buzz-relay-authorization-v1alpha1.md` (Buzz repo). Task 1 here lands first; Tasks 6–8 (live conformance) require the Buzz image built from that branch.

**Working rules:**
- Branch `feat/buzz-production-authority` in `/srv/data02/projects/rustshare`; DCO sign-off (`git commit -s`); do NOT merge; open PR at the end.
- Baselines after every task: `cargo fmt --all --check`; `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings`; `SQLX_OFFLINE=true cargo test --workspace --all-features --lib`.
- DB-backed tests are `#[ignore]`d: run with `--ignored --test-threads=1` against the dev DB.
- Safety boundary (AGENTS.md): this touches permissions/authorization — every task keeps fail-closed behavior and adds tests; the PR needs a security note.

---

### Task 1: Contract amendments (spec + ADR-0035)

**Files:**
- Modify: `docs/specs/buzz-upstream-authorization-v1alpha1.md`
- Modify: `docs/adr/0035-buzz-source-authorization-gateway.md`

The draft spec lacks two capabilities the goal requires. Amend it (keep version `v1alpha1`, bump status note):

- [ ] **Step 1:** Add `POST /api/v1/relay/access/check-batch`: request `{ checks: [...] }` (max 64, >64 → 400); response is a kind-19030 event, `content.results[]` order-preserving, each item identical in shape/echo/freshness rules to single check; per-item failure isolation.
- [ ] **Step 2:** Add `GET /api/v1/relay/channels?pubkey=<64hex>`: NIP-98 GET; response content `{ channels: [{ channel_id, name, channel_type, visibility, member }], evaluated_at, pubkey }`; only channels the pubkey may read, host-derived community only; this is the authoritative channel registry — observation-derived discovery is deprecated in buzz mode.
- [ ] **Step 3:** Document the canonical publish tags confirmed upstream: channel scoping `["h", <channel-uuid>]`; thread root/reply NIP-10 `["e", <id>, <relay?>, "root"|"reply"]` with server-validated ancestry (buzz `ingest.rs:637-789`). State that this resolves the thread-contract input to #243.
- [ ] **Step 4:** ADR-0035: amend the deferred-batch consequence (batch endpoint now specified) and note the registry endpoint; leave acceptance checkboxes for real-relay/live-conformance unchecked until Tasks 6–8 land.
- [ ] **Step 5: Commit** (`git commit -s -m "docs: extend buzz authorization v1alpha1 with batch + channel registry"`)

---

### Task 2: `BuzzAuthority::can_read_batch` trait extension

**Files:**
- Modify: `backend/crates/resource-auth/src/buzz_authority.rs`
- Test: `backend/crates/resource-auth/src/buzz_authority.rs` unit tests (`:97-160`)

- [ ] **Step 1: Failing unit test** — default implementation fans out: a stub authority recording calls returns per-item decisions in request order.
- [ ] **Step 2: Implement** — add to the trait (buzz_authority.rs:70-74):

```rust
async fn can_read_batch(
    &self,
    reqs: &[BuzzReadRequest],
) -> Vec<Result<BuzzReadDecision, BuzzAuthorityError>> {
    // default: bounded fan-out of single checks, order-preserving
}
```

Default body uses `futures::stream::iter(reqs).map(can_read).buffer_unordered(16)` then reorders to request order (or sequential `join_all` if ordering is simpler — pick one and keep it). `LocalFallbackAuthority` inherits the default (no relay in local mode).

- [ ] **Step 3: Tests pass; baselines green; commit (`-s`).**

---

### Task 3: Gateway batch + channels client methods

**Files:**
- Modify: `backend/server/src/buzz_gateway.rs`
- Test: `backend/tests/buzz_authority_gateway_test.rs` (extend `start_fake_buzz`, `:218`)

- [ ] **Step 1: Extend the fake relay** with `/api/v1/relay/access/check-batch` and `/api/v1/relay/channels`, contract-faithful (kind-19030 signed, verbatim echo, `evaluated_at` fresh).
- [ ] **Step 2: Failing tests:** batch decisions equal single decisions for the same inputs (property-style over mixed allow/deny/not_found); batch response failing pin/kind/echo/freshness verification → all items `Deny` (fail closed); channels listing returns only allowed channels; cross-community request → error/Deny.
- [ ] **Step 3: Implement** on `BuzzGatewayClient`:
  - `check_access_batch(&self, reqs: &[BuzzReadRequest]) -> Vec<Result<BuzzReadDecision, BuzzAuthorityError>>` — one POST; verify envelope with the existing `verify_19030`/`decision_from_19030` machinery (`buzz_gateway.rs:383-464`) applied per item; any envelope-level failure → every item `Err`/Deny.
  - `list_channels(&self, relay_url, relay_pubkey, pubkey) -> Result<Vec<BuzzChannelInfo>, BuzzAuthorityError>` — NIP-98 GET, same verification; unpinned mapping → `Config` error (fail closed, matching `:467-486`).
  - `impl BuzzAuthority for BuzzGatewayClient`: override `can_read_batch` to call `check_access_batch`.
- [ ] **Step 4: Tests pass; baselines green; commit (`-s`).**

---

### Task 4: Wire batch authority into timeline/search/Ask paths

**Files:**
- Modify: `backend/server/src/authz/chat_owner.rs` (`authorize_batch` `:633-660`, gate `:398-409`)
- Test: `backend/tests/chat_owner_authorization_test.rs`, `backend/tests/buzz_authority_gateway_test.rs`

- [ ] **Step 1: Failing test (fake relay):** a 64-message timeline page in buzz mode costs **one** batch relay round-trip (fake counts requests), decisions identical to per-message single checks, and revoked-mid-page still denies (post-authority linearization re-reads preserved).
- [ ] **Step 2: Implement** — in `ChatResourceOwner::authorize_batch`, replace the per-ref `authorize` fan-out with: per-ref pre-filters (unchanged, chat_owner.rs:361-394), collect `BuzzReadRequest`s for survivors, one `authority.can_read_batch`, then the existing per-ref post-authority re-reads (`:411-450`) before final decisions. `> MAX_BATCH_SIZE` all-Deny behavior unchanged (`:633-640`). Single-message `authorize` path unchanged.
- [ ] **Step 3: Tests pass; baselines green; commit (`-s`).**

---

### Task 5: Authoritative channel registry in buzz mode

**Files:**
- Modify: `backend/server/src/handlers/chat_app.rs` (`list_channels` `:152-189`)
- Modify: `backend/server/src/state.rs` (needs gateway access in the handler — `buzz_gateway: Option<Arc<BuzzGatewayClient>>` already at `:231-237`)
- Test: `backend/tests/chat_app_read_test.rs`, `backend/tests/buzz_authority_gateway_test.rs`

- [ ] **Step 1: Failing tests (fake relay, buzz mode):** channel list comes from the registry, not `distinct_channels`; inaccessible channels absent; a channel with zero observed events but registry-visible is listed; membership revocation is reflected on the very next list call; local mode still uses the observation-derived path unchanged.
- [ ] **Step 2: Implement** — in `list_channels`: after the mapping check (`:158-167`), if `state.buzz_gateway` is `Some` (buzz mode), resolve the caller's bound pubkey (`active_binding`) and call `gateway.list_channels`; map registry entries to the existing channel-summary response shape; gateway error → fail closed (empty list + log, consistent with existence hiding). Local mode: existing `distinct_channels` + `can_read_channel` path untouched. No Elembra-side channel catalog is introduced.
- [ ] **Step 3: Tests pass; baselines green; commit (`-s`).**

---

### Task 6: Live Buzz↔Elembra conformance suite

**Files:**
- Create: `backend/tests/buzz_live_conformance_test.rs`
- Create: `scripts/run-buzz-conformance.sh`
- Modify: `docker-compose.alpha.yml` (buzz-relay image → build from the kubedoio/buzz feature branch), `.env.example`/`backend/.env.example` (document conformance env vars)
- Modify: `.github/workflows/` — add a conformance job (or extend the chat e2e workflow) that starts the Buzz stack and runs the suite

- [ ] **Step 1:** Env-gated `#[ignore]` suite (`RUSTSHARE_BUZZ_LIVE_RELAY_URL`, `RUSTSHARE_BUZZ_LIVE_HTTP`, service key, two test user keys) against a real relay from the companion Buzz branch, full stack via `docker-compose.alpha.yml` with `RUSTSHARE_CHAT_AUTHORITY=buzz`. Cover the goal's 10 proofs:
  1. allowed channel read succeeds;
  2. denied/private channel fails (existence-hiding 404);
  3. cross-workspace/community access fails;
  4. revoked user (kind-9031 via bridge) denied on the very next read — no caching;
  5. relay unreachable (stop container / blackhole port) → reads fail closed;
  6. batch decisions equal single decisions over the same 64-message set;
  7. channel listing is authoritative (registry content, membership changes reflected immediately, no zero-event channel invented locally);
  8. no Elembra ACL/private Buzz DB dependency — assert code-level (grep guard in CI: no new `chat_*acl*` tables; conformance uses only public endpoints) and architectural note in the PR;
  9. Memory/Search/Ask cannot bypass Buzz: unified search + Ask materialization against the live stack return nothing for a revoked user (`unified_search.rs:718-770`, `materialize_for_rag_scoped` `:472-578` already route through the authorizer — prove it live);
  10. large-timeline authorization bounded (Task 7).
- [ ] **Step 2:** `scripts/run-buzz-conformance.sh` — brings up the stack, runs migrations/seed (keys via `frontend/scripts/alpha-gen-buzz-keys.mjs`, admission via existing bridge), runs the suite `--ignored --test-threads=1`, tears down.
- [ ] **Step 3:** CI job wiring; suite green locally end-to-end; commit (`-s`).

---

### Task 7: Batch latency budget proof

**Files:**
- Modify: `backend/tests/buzz_live_conformance_test.rs` (or a dedicated `backend/tests/buzz_batch_latency_test.rs`)
- Modify: `docs/architecture/elembra-chat-alpha-readiness.md` (document the budget)

- [ ] **Step 1:** Define and document the budget: a 64-message timeline page authorization completes in ≤ 2 relay HTTP round-trips (1 batch + margin) and p95 wall time ≤ 500 ms against a local live relay (budget rationale documented; no weakening of authorization to hit it).
- [ ] **Step 2:** Test: seed 64+ messages across mixed allow/deny channels; `GET /applications/chat/messages?limit=64`; assert round-trip count (relay-side request log/metric) and wall time within budget; assert denied messages still dropped.
- [ ] **Step 3: Green; commit (`-s`).**

---

### Task 8: Production enablement + issue/ADR status

**Files:**
- Modify: `docker-compose.alpha.yml` / `backend/.env.example` — `RUSTSHARE_CHAT_AUTHORITY=buzz` for the dogfood/alpha stack (local mode remains available as an explicit dev fallback; startup validation `config.rs:281-310` already hard-fails a misconfigured buzz mode — no silent fallback exists, keep it that way)
- Modify: `docs/adr/0035-buzz-source-authorization-gateway.md` — check "real relay endpoints implemented" and "live-relay conformance test" once green
- Modify: `docs/runbooks/elembra-alpha.md` — update the #243/#245 blocker dispositions (`:334`)
- Modify: `CHANGELOG.md`
- GitHub: comment status on #245 (all four acceptance boxes evidence) and #243 (wire format confirmed upstream: `h` tag + NIP-10 `e` root/reply, server-validated ancestry; no Elembra thread UI built, per non-goals)

- [ ] **Step 1-4:** Apply, verify compose stack boots in buzz mode and passes `scripts/final-launch-smoke.sh` + the conformance suite; commit (`-s`).

---

### Task 9: Focused security review + PR (no merge)

- [ ] **Step 1:** Write the security note for the PR covering AGENTS.md safety boundaries: fail-closed matrix (transport/401/5xx/bad signature/stale `evaluated_at`/echo mismatch → Deny), existence hiding preserved end-to-end, no decision caching (point at the fresh-call tests), no Elembra ACL tables, no direct Buzz DB reads, no buzz→local fallback, batch endpoint cannot amplify (64 cap, trusted-service-only, NIP-98 + replay guard).
- [ ] **Step 2:** Run a security-review subagent pass over the full diff; address findings.
- [ ] **Step 3:** Full validation: rust baselines + `cargo sqlx prepare --workspace --check` + frontend `npm run check && npm run lint && npm run test && npm run build` (frontend untouched — confirm) + conformance script + smoke script.
- [ ] **Step 4:** Push branch, open PR against `kubedoio/rustshare:main` with the 10-item return report (current-state analysis, Buzz contract, integration changes, flow, perf results, conformance tests, CI results, #243/#245 status, remaining blockers, PRs/branches/HEADs for both repos). **Do not merge.**

---

## Execution order across both plans

1. Elembra Task 1 (contract amendments) → 2. Buzz Tasks 1–8 (relay capability) → 3. Elembra Tasks 2–5 (integration, against fake relay) → 4. Buzz PR + image build → 5. Elembra Tasks 6–8 (live conformance, latency, enablement) → 6. Elembra Task 9 (security review, PRs). Human review gates both merges.
