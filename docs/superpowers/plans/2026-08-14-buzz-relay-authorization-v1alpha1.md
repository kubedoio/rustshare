# Buzz Relay Authorization v1alpha1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the ADR-0035 / `buzz-upstream-authorization-v1alpha1` relay capability in the Buzz repository so Elembra can use Buzz as the live production authority for channel discovery and Chat access.

**Architecture:** Add a NIP-98-authenticated, trusted-service-only HTTP surface to `buzz-relay` (`/api/v1/relay/access/check`, `/access/check-batch`, `/channels`, `/state/events`). Every response is a relay-signed kind-19030 Nostr event whose `content` echoes the request verbatim and carries the decision. Decisions derive exclusively from relay-owned `channels` / `channel_members` / `relay_members` / `events` tables. Admission (kind 9030) and revocation (kind 9031) command handling already exists (`crates/buzz-relay/src/handlers/relay_admin.rs`) and is verified, not rebuilt.

**Tech Stack:** Rust workspace at `/srv/data02/projects/rustshare/.worktrees/buzz` (fork `kubedoio/buzz`, head `f53bbd1`), Axum, SQLx/Postgres, Redis (NIP-98 replay guard), `nostr` crate. GitHub repo: `https://github.com/kubedoio/buzz`.

**Contract source of truth:** `docs/specs/buzz-upstream-authorization-v1alpha1.md` in the rustshare repo, as amended by Task 1 of the companion Elembra plan (`2026-08-14-elembra-buzz-production-authority.md`). **That task must land before Task 3 here** — it defines the batch and channels endpoints that the current draft spec lacks.

**Working rules:**
- Work on branch `feat/relay-authorization-v1alpha1` in `/srv/data02/projects/rustshare/.worktrees/buzz`, commits with DCO sign-off (`git commit -s`). Do NOT merge; open a PR at the end.
- Test infra: `docker compose up -d postgres redis minio minio-init` (from the buzz checkout), DB `postgres://buzz:buzz_dev@localhost:5432/buzz`.
- Postgres-backed handler tests are `#[tokio::test] #[ignore = "requires Postgres"]`, template: `crates/buzz-relay/src/api/invites.rs:540-720` (`invite_test_state`, `nip98_auth_header`, `AlwaysFreshReplayGuard`, fresh uuid-suffixed host per test, drive with `build_router(state).oneshot(...)`).
- Run focused tests: `cargo nextest run -p buzz-relay --run-ignored all -E 'test(/api::relay_access/)'` (needs Postgres+Redis up).

---

### Task 1: Trusted service pubkey config

**Files:**
- Modify: `crates/buzz-relay/src/config.rs` (field near `:205`, parsing near `:662-683`)
- Modify: `.env.example`
- Test: `crates/buzz-relay/src/config.rs` tests module

- [ ] **Step 1: Add failing config tests**

Follow the `RELAY_OPERATOR_PUBKEYS` fail-closed pattern (config.rs:662-683). Tests: unset → empty vec; valid comma-separated 64-hex list parses; one invalid entry → hard `ConfigError::InvalidValue`.

- [ ] **Step 2: Run tests, verify failure**

Run: `cargo test -p buzz-relay --lib config` — Expected: FAIL (field does not exist).

- [ ] **Step 3: Implement**

Add `pub relay_trusted_service_pubkeys: Vec<String>` to `Config`, parsed in `Config::from_env()` from `RELAY_TRUSTED_SERVICE_PUBKEYS` (comma-separated 64-lower-hex; any invalid entry is a hard config error). Document in `.env.example`: "pubkeys permitted to call `/api/v1/relay/*` authorization endpoints".

- [ ] **Step 4: Run tests, verify pass; `cargo clippy -p buzz-relay -- -D warnings`**

- [ ] **Step 5: Commit** (`git commit -s -m "feat(relay): trusted service pubkey config for authorization API"`)

---

### Task 2: Kind-19030 constant

**Files:**
- Modify: `crates/buzz-core/src/kind.rs` (constants near `:398-402`)

- [ ] **Step 1:** Add `pub const KIND_RELAY_AUTHZ_RESPONSE: u16 = 19030;` with a doc comment (unregistered private replaceable-range kind, never published to subscribers, returned inline over HTTP only).
- [ ] **Step 2:** Verify 19030 is not routed by any WS ingest/REQ gate (grep kind gate lists at kind.rs:680-830, `handlers/req.rs`); no changes expected — these events are never submitted, only returned inline.
- [ ] **Step 3:** `cargo test -p buzz-core` passes; commit (`-s`).

---

### Task 3: `POST /api/v1/relay/access/check`

**Files:**
- Create: `crates/buzz-relay/src/api/relay_access.rs`
- Modify: `crates/buzz-relay/src/api/mod.rs` (add `pub mod relay_access;`)
- Modify: `crates/buzz-relay/src/router.rs` (register on `api_router`, `:62-132`)

**Request:** `{ pubkey: String(64hex), channel_id: String, channel_kind: "workspace"|"dm"|"private"|"excluded", message_id: Option<String(64hex)>, event_created_at: Option<u64> }`
**Response:** kind-19030 signed event, `content` = JSON `{ decision: "allow"|"deny"|"not_found", reason: String, evaluated_at: u64, pubkey, channel_id, message_id }` echoing request values verbatim.

- [ ] **Step 1: Write failing handler tests** (template `api/invites.rs:652-719`)

Cases: member+existing message → `allow`; non-member → `deny` with existence-hiding `reason: "not a member"`; unknown channel → `not_found`; deleted/unknown message → `not_found`; request from untrusted pubkey → 401; missing/invalid NIP-98 → 401; payload-tag mismatch → 401; response verifies as kind 19030 signed by `state.relay_keypair` and echoes `pubkey`/`channel_id`/`message_id` verbatim.

- [ ] **Step 2: Run tests, verify 404/failure.**

- [ ] **Step 3: Implement handler**

Pipeline (bridge recipe, `api/bridge.rs:617-660`):
1. `crate::tenant::bind_community(&state.db, raw_host)` → tenant (host-derived community isolation; never trust a client-supplied community id).
2. `nip98_expected_url(&state.config.relay_url, &tenant, "/api/v1/relay/access/check")`; `verify_bridge_auth(&headers, "POST", &url, Some(&body), state.config.require_auth_token)`.
3. Trusted-service gate: 401 unless verified pubkey hex ∈ `config.relay_trusted_service_pubkeys`.
4. `enforce_http_admission` rate limit + `check_nip98_replay` (fail-closed).
5. Decision from relay state: resolve channel by `channel_id` within tenant (`Db::get_channel`); unknown → `not_found`. `can_read = Db::is_member(community, channel_uuid, pubkey_bytes) || channel.visibility == "open"` (mirror `get_accessible_channel_ids`, `buzz-db/src/channel.rs:752`). Non-member of non-open → `deny`, reason `"not a member"` (never enumerate membership). If `message_id` present: `Db::get_event_by_id` soft-delete-aware variant (lib.rs:1799); missing/deleted → `not_found`; existing + not readable → `deny`; else `allow`.
6. Sign: `nostr::EventBuilder::new(Kind::Custom(19030), content_json).sign_with_keys(&state.relay_keypair)` (pattern at `handlers/side_effects.rs:770`); return the raw event JSON.

Errors: `api_error`/`internal_error` helpers (`api/mod.rs:19-26`); NIP-98/auth failures → 401; malformed body → 400; never 5xx for an authz negative.

- [ ] **Step 4: Run tests, verify pass.** Include a cross-community case: seed channel in community A, call with Host of community B → `not_found`.

- [ ] **Step 5: Commit** (`-s`).

---

### Task 4: `POST /api/v1/relay/access/check-batch`

**Files:** Modify `crates/buzz-relay/src/api/relay_access.rs`, `crates/buzz-relay/src/router.rs`

**Request:** `{ checks: [ <single-check body>, … ] }`, cap 64 items (matches Elembra `MAX_BATCH_SIZE`); >64 → 400.
**Response `content`:** `{ results: [ { decision, reason, evaluated_at, pubkey, channel_id, message_id }, … ] }` — same per-item shape and echo rules as single check, order-preserving.

- [ ] **Step 1: Failing tests:** N mixed items resolved in one call; per-item decisions equal the single-check decisions for identical inputs (conformance seed); order preserved; >64 → 400; one bad item → that item `deny`/`not_found`, others unaffected.
- [ ] **Step 2: Implement** — same auth pipeline as Task 3; resolve membership in one statement via `Db::membership_pairs(community, channel_ids, pubkeys)` (`buzz-db/src/channel.rs:668`), then one `get_events_by_ids` (lib.rs:1876) for message availability; fold per item. Sign one kind-19030 event.
- [ ] **Step 3: Tests pass; commit (`-s`).**

---

### Task 5: `GET /api/v1/relay/channels` (authoritative channel registry)

**Files:** Modify `crates/buzz-relay/src/api/relay_access.rs`, `crates/buzz-relay/src/router.rs`

**Request:** `GET /api/v1/relay/channels?pubkey=<64hex>` (NIP-98 GET; expected URL built from path + raw query, pattern `authorize_moderation_read` at `api/bridge.rs:2138`).
**Response `content`:** `{ channels: [ { channel_id, name, channel_type, visibility, member: bool } ], evaluated_at, pubkey }` — only channels the given pubkey may read, via `Db::get_accessible_channels(community, pubkey_bytes, None, None)` (`buzz-db/src/channel.rs:942`); never another community's channels (host-derived tenant).

- [ ] **Step 1: Failing tests:** member sees private member channels + open channels; non-member sees only open; DM/hidden channels of others absent; cross-community Host → only that community's channels; untrusted caller → 401.
- [ ] **Step 2: Implement; tests pass; commit (`-s`).**

---

### Task 6: `GET /api/v1/relay/state/events`

**Files:** Modify `crates/buzz-relay/src/api/relay_access.rs`, `crates/buzz-relay/src/router.rs`

Per spec: `?since=<unix>&limit=<n>&cursor=<opaque>`, `content` = `{ entries: [ { event: <raw signed event JSON>, context: { community_id, channel_id, channel_kind, thread_root_id, message_id, event_type, supersedes_event_id } } ], cursor: Option<String>, complete: bool }`; `cursor: null` with `complete: false` is malformed (never emit).

- [ ] **Step 1: Failing tests:** paging two pages yields all entries in `created_at` order with no dupes; `since` filters on the event's own `created_at`; context shape matches Elembra `BuzzPushContext` field-for-field; entries limited to the Host-derived community; limit clamp 1..=500.
- [ ] **Step 2: Implement** — `Db::query_events` with `EventQuery { kinds: Some(vec![1]), since, … }` keyset-paginated (before_id pattern, `buzz-db/src/event.rs:29`); derive `thread_root_id` from `thread_metadata`; `event_type` from tombstone/edit markers already tracked at ingest; opaque cursor = base64 `(created_at, event_id)`; final page `complete: true, cursor: null`.
- [ ] **Step 3: Tests pass; commit (`-s`).**

---

### Task 7: Route registration + CI wiring

**Files:** `crates/buzz-relay/src/router.rs`, `.github/workflows/ci.yml`

- [ ] **Step 1:** Confirm all four routes registered on `api_router` under the existing `RequestBodyLimitLayer`; batch route needs the 1 MiB cap reviewed (64 checks fit comfortably).
- [ ] **Step 2:** Add the new test selection to the "Backend Integration (relay e2e)" CI job next to ci.yml:689-703: `cargo nextest run -p buzz-relay --run-ignored all -E 'test(/api::relay_access/)'`.
- [ ] **Step 3:** `just check` (fmt+clippy) green; `just test-unit` green; integration selection green locally against compose Postgres/Redis.
- [ ] **Step 4: Commit (`-s`).**

---

### Task 8: Document canonical publish tags (feeds #243)

**Files:** `NOSTR.md` (or `docs/` equivalent) in the buzz repo

- [ ] **Step 1:** Document the wire format as implemented at `crates/buzz-relay/src/handlers/ingest.rs:637-789`: channel scoping via `["h", <channel-uuid>]` (NIP-29); thread identity via NIP-10 `["e", <64-hex-id>, <relay-url?>, "root"|"reply"]` with server-validated ancestry (parent must exist, same channel, root must match stored ancestry, depth cap 100); optional `["broadcast", "1"]`.
- [ ] **Step 2:** Note explicitly that this is the canonical thread root/reply contract Elembra #243 was waiting on.
- [ ] **Step 3: Commit (`-s`).**

---

### Task 9: PR (no merge)

- [ ] Push branch, open PR against `kubedoio/buzz:main` with: contract summary, security note (new authenticated surface; trusted-service gate; existence-hiding reasons), test/CI evidence, link to rustshare #245. **Do not merge.**
