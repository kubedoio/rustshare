# Managed Identity Provisioning (Phase 2B.2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Phase 2B.2 managed-identity provisioning lifecycle — internal, feature-disabled, test-only, idempotent, disposable — on top of the merged Phase 2A vault and Phase 2B.1 signed mapping/transport foundations, exactly as specified in `docs/superpowers/specs/2026-11-03-managed-identity-provisioning-design.md` (read it first; this plan implements it slice by slice).

**Architecture:** A durable, tenant-scoped `managed_chat_identity_operations` table records explicitly authorized provisioning operations with a bounded error catalog and no plaintext-capable columns. A feature-gated (`upstream-buzz-runtime`), `pub(crate)` server provisioner exposes test-called `authorize`/`drive`/`reconcile` — no startup hook, no worker, no route. Key custody reuses `ManagedIdentityEnvelope` + `LocalDekKeyManagement` + `ManagedIdentityVaultStore::insert_or_load`. Admission publishes deterministic NIP-43 kind-9030 events signed by a separate disposable relay-admin key over the existing pinned, generation-fenced `TenantRelaySession` (which gains one test-only `send_text`). Readiness requires a fresh NIP-42 authentication signed by the one new narrow crypto capability (`RecoveredIdentity::sign_nip42_authentication`, feature `nip42-auth-signing`) plus a member-only NIP-29 read. `managed_chat_identities` keeps `active`/`disabled`/`revoked` semantics; readiness lives on the operation.

**Tech Stack:** Rust 1.97.1 workspace (storage crate `rustshare-storage`, crypto crate `rustshare-crypto`, resource-auth crate `rustshare-resource-auth`, server crate `rustshare-server`), SQLx/PostgreSQL 16+, `nostr` 0.44 wire events, unmodified disposable Buzz relay per `config/buzz-compatibility.env`.

**Spec:** `docs/superpowers/specs/2026-11-03-managed-identity-provisioning-design.md`.

**Hard scope rules (check on every task):** no production route/worker/API/startup caller; no general signing or profile selector; no `nsec`/secret export; no upstream Buzz modifications or production pin changes; no legacy-identity or M4.1b2 replacement changes; NIP-17 stays disabled; #442/Gate I posture unchanged; draft PR only, DCO-signed commits (`git commit -s`).

---

## File Structure

New:
- `backend/migrations/20261103100000_managed_chat_identity_operations.up.sql`
- `backend/crates/storage/src/managed_identity_operations.rs` — `ManagedIdentityOperationStore`, `ManagedIdentityOperationRecord`, `ProvisioningRequest`, `ManagedIdentityOperationError`.
- `backend/server/src/upstream_identity_provisioning.rs` — `ManagedIdentityProvisioner`, `ProvisioningSubject`, `DriveOutcome`, in-crate unit tests + ignored live-Buzz tests.
- `backend/tests/managed_identity_provisioning_store_test.rs` — ignored live-PostgreSQL store invariants.

Modified:
- `backend/crates/storage/src/lib.rs` — `pub mod managed_identity_operations;` + root re-exports.
- `backend/crates/storage/src/managed_identity_vault.rs` — add `load_active_by_principal`.
- `backend/crates/crypto/Cargo.toml` — feature `nip42-auth-signing = []`.
- `backend/crates/crypto/src/managed_identity_vault.rs` — `VaultError::AuthenticationSigning`; feature-gated `sign_nip42_authentication`; unit tests.
- `backend/server/src/upstream_buzz_transport.rs` — `TenantRelaySession::send_text` (`pub(crate)`).
- `backend/server/src/lib.rs` — feature-gated `pub(crate) mod upstream_identity_provisioning;` with `#[expect(dead_code, reason = "provisioning is test-called only; no runtime caller until a separately authorized phase")]`.
- `backend/server/Cargo.toml` — `upstream-buzz-runtime` chain gains `rustshare-crypto/nip42-auth-signing`; `[dev-dependencies] rustshare-crypto` gains `local-dek-key-provider` for in-crate tests.
- `docs/contracts/managed-chat-identity-v1.md` — one status-note paragraph (§"Phase boundaries"), contract stays target.

Not touched: routes, `AppState`/bootstrap wiring, workers, frontend, desktop, obsidian plugin, upstream Buzz, legacy chat-identity/m4.1b2 code paths.

Validation commands (run at the end of every slice; full matrix at the end):
```bash
cargo fmt --all --check
SQLX_OFFLINE=true cargo check -p rustshare-crypto --all-features
SQLX_OFFLINE=true cargo check -p rustshare-storage --features upstream-buzz
SQLX_OFFLINE=true cargo check -p rustshare-server --features upstream-buzz-runtime --all-targets
SQLX_OFFLINE=true cargo clippy -p rustshare-crypto --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo clippy -p rustshare-storage --all-targets --features upstream-buzz -- -D warnings
SQLX_OFFLINE=true cargo clippy -p rustshare-server --all-targets --features upstream-buzz-runtime -- -D warnings
SQLX_OFFLINE=true cargo test -p rustshare-crypto --all-features --lib
SQLX_OFFLINE=true cargo test -p rustshare-storage --features upstream-buzz --lib
SQLX_OFFLINE=true cargo test -p rustshare-server --features upstream-buzz-runtime --lib
git diff --check
```

---

## Slice 1 — Operation persistence (migration + storage store)

### Task 1: Migration for `managed_chat_identity_operations`

**Files:**
- Create: `backend/migrations/20261103100000_managed_chat_identity_operations.up.sql`

- [ ] **Step 1: Write the failing test first.** In `backend/tests/managed_identity_provisioning_store_test.rs`, create the ignored harness scaffold and the first invariant test (table exists with RLS and constraints). Model it on `backend/tests/managed_identity_vault_test.rs:154` (`#[tokio::test] #[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]`, `DATABASE_URL`, `sqlx::migrate!("../migrations")`).

```rust
#[tokio::test]
#[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
async fn operations_table_is_tenant_scoped_and_cannot_hold_plaintext() {
    let database_url = std::env::var("DATABASE_URL").unwrap();
    let pool = PgPool::connect(&database_url).await.unwrap();
    sqlx::migrate!("../migrations").run(&pool).await.unwrap();
    // No bytea / no free-form large text columns: key material cannot be persisted.
    let plaintext_capable: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns \
         WHERE table_name='managed_chat_identity_operations' \
           AND (data_type IN ('bytea','text') AND column_name NOT IN ('status','error_code','admission_event_id'))",
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(plaintext_capable, 0);
    let rls: bool = sqlx::query_scalar(
        "SELECT relrowsecurity AND relforcerowsecurity FROM pg_class WHERE relname='managed_chat_identity_operations'",
    ).fetch_one(&pool).await.unwrap();
    assert!(rls);
}
```

- [ ] **Step 2: Run it red.**
```bash
SQLX_OFFLINE=true cargo test -p rustshare-server --test managed_identity_provisioning_store_test -- --ignored --test-threads=1
```
(It must fail: relation does not exist. Start a disposable PostgreSQL if needed, e.g. `docker run --rm -e POSTGRES_PASSWORD=postgres -p 55432:5432 postgres:16`, then `export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres`.)

- [ ] **Step 3: Write the migration** per design §5.1: columns, `CHECK`s (status catalog, error-code catalog, hex admission event id), `UNIQUE (tenant_id, principal_id, idempotency_key)`, partial unique index `managed_chat_identity_operations_one_active` (`WHERE status IN ('authorized','provisioning')`), `ENABLE/FORCE ROW LEVEL SECURITY` + tenant-isolation policy identical in shape to `backend/migrations/20261101090000_managed_chat_identity_vault.up.sql:50-55`. No `DOWN` file (forward-only, matching the vault migration).

- [ ] **Step 4: Run it green** with the same command as Step 2.

### Task 2: `ManagedIdentityOperationStore` core (authorize/load/CAS transitions)

**Files:**
- Create: `backend/crates/storage/src/managed_identity_operations.rs`
- Modify: `backend/crates/storage/src/lib.rs`

- [ ] **Step 1: Write failing store tests.** Extend `backend/tests/managed_identity_provisioning_store_test.rs` (all ignored, live PG):

  - `authorize_is_idempotent_per_key_and_rejects_second_active_operation` — same `(tenant, principal, idempotency_key)` twice returns the same `operation_id`; a different key while one is active returns `Conflict`; `identity_id`/`identity_generation` are stable across the retry; `identity_generation` is `max(vault generation)+1` when a vault row exists.
  - `cas_transitions_are_monotonic` — `attach_identity` authorized→provisioning ok; second `attach_identity` returns `InvalidTransition`-class error; `mark_ready` only from `provisioning`; `mark_failed` from any non-ready; `record_admission_sent` set-if-null returns `true` then `false` with the same event id preserved.
  - `list_retryable_respects_bounds_and_order` — seeded ready/failed/authorized rows; only retryable ones returned, ordered by `created_at`, limited.
  - `operations_are_tenant_isolated` — cross-tenant `load`/`list_retryable` sees nothing (RLS probe pattern from `managed_identity_vault_test.rs:253-297`, role `NOSUPERUSER NOBYPASSRLS`).

- [ ] **Step 2: Run red.**
```bash
SQLX_OFFLINE=true cargo test -p rustshare-server --test managed_identity_provisioning_store_test -- --ignored --test-threads=1
```

- [ ] **Step 3: Implement** `managed_identity_operations.rs`: `ManagedIdentityOperationStatus` (as_str/parse), `ManagedIdentityOperationRecord` (`FromRow`), `ProvisioningRequest { tenant_id, principal_id, workspace_id, idempotency_key }`, `ManagedIdentityOperationError { Database, Conflict, NotFound, InvalidTransition, InvalidRequest }`, and `ManagedIdentityOperationStore(PgPool)` with local `set_tenant` helper copied from `managed_identity_vault.rs:270-280` (house style), `authorize` (transaction: compute next `identity_generation` via `SELECT COALESCE(MAX(generation),0)+1 FROM managed_chat_identities WHERE tenant_id=$1 AND principal_id=$2`, insert-or-load on the idempotency key, surface 23505 as `Conflict` via `map_database_error` like `managed_identity_vault.rs:282-289`), `load`, `attach_identity`, `record_admission_sent` (returns `bool`), `mark_ready`, `mark_failed` (increments `attempt_count`, validates catalog membership), `list_retryable`. Re-export from `lib.rs` next to the vault re-exports.

- [ ] **Step 4: Run green**, then the slice validation block. Also run the pre-existing vault integration test to prove no regression:
```bash
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres \
  cargo test -p rustshare-server --test managed_identity_vault_test -- --ignored --test-threads=1
```

---

## Slice 2 — Narrow NIP-42 signing capability (crypto crate)

### Task 3: Feature-gated `sign_nip42_authentication`

**Files:**
- Modify: `backend/crates/crypto/Cargo.toml`
- Modify: `backend/crates/crypto/src/managed_identity_vault.rs`

- [ ] **Step 1: Write failing unit tests** in `managed_identity_vault.rs`'s `#[cfg(test)] mod tests` (they compile under `--all-features`, which enables the new feature):

  - `sign_nip42_authentication_binds_challenge_and_relay` — generate envelope, recover, sign with a fixed `Timestamp`, assert `event.kind == Kind::Authentication` (22242), `event.verify().is_ok()`, `event.pubkey.to_hex() == envelope.public_key`, relay/challenge tags match, content empty.
  - `sign_nip42_authentication_is_deterministic_and_rejects_bad_relay` — same inputs → same event id; `relay_url = "not a url"` → `VaultError::AuthenticationSigning`.
  - `signed_auth_event_exposes_no_secret` — `format!("{:?}", event)` and `serde_json::to_string(&event)` contain neither the secret bytes' hex nor a 64-hex bech32-decodable secret; `RecoveredIdentity` Debug remains non-exhaustive (existing test covers).

- [ ] **Step 2: Run red.**
```bash
SQLX_OFFLINE=true cargo test -p rustshare-crypto --all-features --lib managed_identity_vault
```

- [ ] **Step 3: Implement** — `Cargo.toml`: `nip42-auth-signing = []` under `[features]`. In `managed_identity_vault.rs`: add `VaultError::AuthenticationSigning` variant (`#[error("managed identity authentication signing failed")]`); add the feature-gated method per design §5.3 (hex-parse + zeroize temp pattern exactly as `recover` does at lines 201-204; `nostr::EventBuilder::auth(challenge, RelayUrl::parse(relay_url).map_err(|_| VaultError::AuthenticationSigning)?)`; `.custom_created_at(created_at)`; `sign_with_keys`; drop keys before returning).

- [ ] **Step 4: Run green + lint.**
```bash
SQLX_OFFLINE=true cargo test -p rustshare-crypto --all-features --lib
SQLX_OFFLINE=true cargo clippy -p rustshare-crypto --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo check -p rustshare-crypto   # default features: method absent, build identical
```

---

## Slice 3 — Additive storage/transport seams

### Task 4: `ManagedIdentityVaultStore::load_active_by_principal`

**Files:**
- Modify: `backend/crates/storage/src/managed_identity_vault.rs`

- [ ] **Step 1: Failing test** in `backend/tests/managed_identity_vault_test.rs` (extend the existing ignored test or add `load_active_by_principal_returns_none_then_active_record`): absent → `Ok(None)`; after `insert_or_load` → `Ok(Some)` with matching `identity_id`/`generation`/`public_key`; after `disable_or_revoke` → `Ok(None)` again.

- [ ] **Step 2: Run red** (`DATABASE_URL=... cargo test -p rustshare-server --test managed_identity_vault_test -- --ignored --test-threads=1`), then implement (tenant-scoped select of the `status='active'` row for the subject; `Ok(None)` on empty), then green.

### Task 5: `TenantRelaySession::send_text`

**Files:**
- Modify: `backend/server/src/upstream_buzz_transport.rs`

- [ ] **Step 1: Failing unit test** in the module's `#[cfg(test)] mod tests` (feature `upstream-buzz-runtime`): `send_text_without_socket_fails_closed` — construct `TenantRelaySession` directly (in-module tests can use the struct literal; set `socket: None`) and assert `send_text` returns `Connection`. The `SessionFenced` revalidation path is asserted live in Slice 5 (`live_mapping_deactivation_fences_provisioning_sessions`), since `revalidate` needs a real mapping store.

- [ ] **Step 2: Run red.**
```bash
SQLX_OFFLINE=true cargo test -p rustshare-server --features upstream-buzz-runtime --lib upstream_buzz_transport
```

- [ ] **Step 3: Implement** `send_text` per design §5.4: return `Connection` when `self.socket` is `None`; then `self.revalidate().await?`; then `socket.send(Message::Text(text.into())).await` mapping send errors to `UpstreamBuzzTransportError::Connection`. Keep it `pub(crate)`; doc-comment "test-called provisioning only; no production caller".

- [ ] **Step 4: Run green** (same command) + confirm the six existing focused transport unit tests still pass.

---

## Slice 4 — Provisioner (DB-driven behavior, no relay)

### Task 6: Provisioner skeleton + `authorize`/`drive` identity stage

**Files:**
- Modify: `backend/server/src/lib.rs`
- Modify: `backend/server/Cargo.toml`
- Create: `backend/server/src/upstream_identity_provisioning.rs`

- [ ] **Step 1: Register the module and wire features.** In `lib.rs` add the feature-gated `pub(crate) mod` with `#[expect(dead_code, reason = "provisioning is test-called only; no runtime caller until a separately authorized phase")]` (mirror `upstream_buzz_transport`'s registration at lines 44-49). In `Cargo.toml`: add `rustshare-crypto/nip42-auth-signing` to the `upstream-buzz-runtime` feature (the provisioner is feature-gated but not `cfg(test)`-gated, so the capability must compile in non-test feature builds), and add `[dev-dependencies] rustshare-crypto = { path = "../crates/crypto", features = ["local-dek-key-provider"] }` so tests can construct `LocalDekKeyManagement`.

- [ ] **Step 2: Write failing tests** in `upstream_identity_provisioning.rs` `#[cfg(test)] mod tests` (ignored, live PG via `DATABASE_URL`; sign a test mapping into `upstream_buzz_tenant_relays` using the existing signed-statement test helpers pattern from `upstream_relay_mapping_store.rs` tests / PR #449: an ephemeral Ed25519 operator signer + `MappingProvisioningVerifier` + `store.apply`; see the disposable-DB evidence recipe in `docs/plans/elembra-phase2b-gate-i-readiness-2026-11-01.md` §"Scope and validation record"):

  - `concurrent_drive_yields_one_identity` — N=8 tokio tasks `authorize` (same subject, different keys) then `drive` identity stage; assert one vault row, equal pubkeys, `identity_conflict`-free; non-winning authorize calls return `Conflict` or the same operation.
  - `retry_preserves_pubkey_and_generation` — drive, then drive again + `reconcile`; same pubkey/generation; `attempt_count` bounded.
  - `existing_active_identity_short_circuits` — pre-insert an active identity for the subject; `authorize` (which pre-allocates a new `identity_id` and `identity_generation = max+1`) then the `ensure_identity` step reuses the stored identity: same pubkey and generation, no second vault row, and `attach_identity` records the reused `identity_id`.

  For Slice 4, drive stops before the network: factor `ensure_identity` as its own `pub(crate)` async fn and test it plus `authorize` mapping/generation validation. Tests that need the relay are Slice 5.

- [ ] **Step 3: Run red.**
```bash
SQLX_OFFLINE=true cargo test -p rustshare-server --features upstream-buzz-runtime --lib upstream_identity_provisioning
# with DATABASE_URL for the ignored ones:
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres \
  cargo test -p rustshare-server --features upstream-buzz-runtime --lib upstream_identity_provisioning -- --ignored --test-threads=1
```

- [ ] **Step 4: Implement** the provisioner per design §5.5: struct holding the three stores; `ProvisioningSubject`; `ProvisioningError` (mapping store/vault/operation/transport error wrappers + catalog mapping); `authorize` delegating to the operation store; `ensure_identity` (mapping load + generation CAS → `load_active_by_principal` → generate+`insert_or_load(idempotency_key = operation_id)` → `attach_identity`); error paths call `mark_failed(<catalog code>)` before returning.

### Task 7: Crash reconciliation without duplicates (DB-only)

**Files:**
- Modify: `backend/server/src/upstream_identity_provisioning.rs`

- [ ] **Step 1: Failing tests** (ignored, live PG), simulating crashes by driving partial sequences through the `pub(crate)` steps and store mutations:

  - `crash_before_encrypted_storage_reconciles` — `authorize`; drive with KMS failure injected (wrap/unwrap fail) so no insert happens; the operation is `failed`/`kms_unavailable` and no identity row exists; remove the injection; `reconcile` → identity exists exactly once and its `identity_id` equals the operation's pre-allocated id.
  - `crash_after_encrypted_storage_reuses_identity` — `authorize`; generate + `insert_or_load` out-of-band with the operation's own `identity_id`/`identity_generation` (simulating the crash window after storage, before `attach_identity`); `reconcile` → `ensure_identity` finds and reuses the active row: no new key, `attach_identity` completes, pubkey equals the out-of-band envelope's.
  - `kms_outage_fails_closed_and_is_retryable` — wrap the real `LocalDekKeyManagement` in an in-module, test-only `DekKeyManagement` decorator with an `unavailable` flag (mirrors the crypto crate's `TestKms`) so wrap/unwrap fail on demand → per-drive `kms` error maps to `kms_unavailable`, no identity usable, `list_retryable` includes the operation; flip the flag off and `reconcile` → ready-path proceeds.
  - `cross_tenant_identity_substitution_is_rejected` — tenant A identity row; a tenant B operation cannot observe or reference it (tenant RLS on both stores); copying A's envelope fields into a fresh insert for B fails the globally unique `public_key`, and envelope `recover` under B's context fails AAD (`InvalidEnvelope`/`Recovery`).
  - `database_outage_is_retryable_and_unauthorized` — with a second disposable DB handle, `pg_ctl stop -m immediate` mid-drive (pattern: gate report 2026-11-02 entry); drive returns retryable DB error; operation not ready; signing cannot occur (no capability use before ready); `pg_ctl start`; `reconcile` → completes.

- [ ] **Step 2: Run red → Step 3: implement `reconcile` (sequential `list_retryable` + `drive`, summary counts) and any missing catalog codes → Step 4: green.**
```bash
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres \
  cargo test -p rustshare-server --features upstream-buzz-runtime --lib upstream_identity_provisioning -- --ignored --test-threads=1
```

---

## Slice 5 — Live disposable Buzz conformance (ignored in-crate tests)

### Task 8: Admission → ready → NIP-43 replay idempotency

**Files:**
- Modify: `backend/server/src/upstream_identity_provisioning.rs` (tests + `admit`/`prove_membership` steps)

Environment: the disposable unmodified Buzz harness (pinned revision in `config/buzz-compatibility.env`; owner/admin `nsec` from the harness-owned mode-0600 file, test-held only; see `scripts/upstream-buzz-poc/README.md` and the PR #450 test recipe in the gate report). Kind/Cilium environment per the qualifying runs is optional; a plain disposable relay + disposable PostgreSQL is the minimum. Tests are `#[ignore]` and named `live_*`.

- [ ] **Step 1: Failing tests:**

  - `live_provisioning_reaches_ready_after_nip42_and_member_only_read` — signed test mapping (ephemeral operator signer, `trusted_service` or `strict_public` per harness); seed a private NIP-29 channel + member-only message as relay admin; `authorize` → `drive` → `Ready`; assert the member-only `REQ` (exact filter pinned during TDD against stock Buzz; start from the kind-9 private-channel shape used in `trusted_service_transport_connects_and_authenticates_member`) returned the seeded event, and that a pre-admission negative control (same `REQ` from a fresh key) returns zero events.
  - `live_admission_failure_never_marks_ready` — drive with a **wrong** relay-admin key → 9030 rejected → operation `failed` (`admission_rejected`); NIP-42 as the managed key still denied; identity row preserved but unused; `reconcile` retries and fails the same way (bounded attempts).
  - `live_stale_mapping_generation_fails_closed` — signed generation `g→g+1` update between `authorize` and `drive` → `stale_mapping_generation`; refresh the operation's mapping binding via a new `authorize` after the update → proceeds.
  - `live_nip43_replay_is_idempotent_at_operation_layer` — complete drive; reset `admission_event_id` to NULL directly via the store (simulating crash after relay acceptance, before CAS); `drive` again → rebuilt kind-9030 event id is **identical** and relay-accepted; still one operation, still `ready`.

- [ ] **Step 2: Run red.**
```bash
cargo test -p rustshare-server --features upstream-buzz-runtime --lib upstream_identity_provisioning -- --ignored --test-threads=1 --nocapture
```

- [ ] **Step 3: Implement** `admit` (build/send 9030 conditional event with pinned `created_at` from `operation.created_at`, require relay acceptance, `record_admission_sent`) and `prove_membership` (NIP-42 handshake: send member-only `REQ`, parse `["AUTH", challenge]`, sign via `RecoveredIdentity::sign_nip42_authentication`, send `["AUTH", event]`, await OK, re-`REQ`, require ≥1 event within a bounded timeout; any denial → catalog failure) and wire them into `drive` after `ensure_identity`.

- [ ] **Step 4: Run green** (same command), then capture sanitized evidence (sha256 of the test log) per the gate-report hygiene rules; keep no `nsec`, transcript, or relay private material.

### Task 9: kind-9031 revocation, fencing, reconnect denial, secret sentinel

**Files:**
- Modify: `backend/server/src/upstream_identity_provisioning.rs` (tests)

- [ ] **Step 1: Failing tests (ignored, live disposable Buzz):**

  - `live_kind9031_revokes_future_admission` — after `ready`, publish kind 9031 (`build_buzz_admission_event_at(Revoke, pubkey, relay_admin, now)`); assert: new connect + NIP-42 as the revoked key is rejected; member-only `REQ` returns zero events; an in-flight operation re-driven after the revocation cannot reach `ready` (`membership_unproven`).
  - `live_revoked_session_is_fenced_and_reconnect_fails` — on the pre-revocation authenticated session: issue a fresh member-only `REQ` and assert denial/`CLOSED_RESTRICTED` with zero events (deterministic); record whether stock Buzz also initiates socket closure within the observation window and assert the observed behavior explicitly (no upstream modification to force closure).
  - `live_mapping_deactivation_fences_provisioning_sessions` — signed deactivation at `g+1` while a provisioning session is active → session fails closed with `SessionFenced` within the revalidation bound (mirrors the merged transport assertion; proves fencing composes with provisioning).
  - `live_no_key_material_in_relay_payloads_or_error_catalog` — sentinel: every payload the test sent contains no 32-byte secret / `nsec1` / 64-hex private material (only pubkeys); every persisted `error_code` seen across the suite belongs to the migration `CHECK` catalog; `RecoveredIdentity`/envelope `Debug` outputs contain no secret hex (reuse the crypto-crate assertion pattern).

- [ ] **Step 2: Run red → Step 3: fix (tests/assertions only; no production code expected) → Step 4: green + evidence capture.**
```bash
cargo test -p rustshare-server --features upstream-buzz-runtime --lib upstream_identity_provisioning -- --ignored --test-threads=1 --nocapture
```

---

## Slice 6 — Contract status note + final validation

### Task 10: Contract status note

**Files:**
- Modify: `docs/contracts/managed-chat-identity-v1.md`

- [ ] **Step 1:** Add one paragraph to "Phase boundaries" recording that Phase 2B.2 adds the durable, tenant-scoped, test-only `managed_chat_identity_operations` persistence and inert feature-gated provisioner, that `managed_chat_identities` semantics are unchanged (`active`/`disabled`/`revoked` at the vault, readiness at the operation), and that the API/provisioning sections remain later-phase targets with no runtime route. Do not soften any MUST.

### Task 11: Full workspace validation + commit/PR hygiene

- [ ] **Step 1: Full Rust baseline.**
```bash
cargo fmt --all --check
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --workspace --all-features --lib
cargo sqlx prepare --workspace --check   # only if any query macro changed; the new store uses query/query_as like existing code — confirm with prepare --check either way
```
- [ ] **Step 2: Ignored live suites.** Re-run all ignored suites against fresh disposables and record sanitized evidence hashes:
```bash
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres \
  cargo test -p rustshare-server --test managed_identity_provisioning_store_test --test managed_identity_vault_test -- --ignored --test-threads=1
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres \
  cargo test -p rustshare-server --features upstream-buzz-runtime --lib upstream_identity_provisioning -- --ignored --test-threads=1
```
- [ ] **Step 3: Hygiene.** `git status` (no stray artifacts), `git diff --check`, CHANGELOG: **no entry** (no user-visible change; internal test-only slice — note this explicitly in the PR description instead), every commit `git commit -s`, PR remains **draft** until independent review, and the PR description restates the scope exclusions and that Gate I/#442 are untouched.

---

## Assumptions (settled per auto mode; revisit only with new evidence)

- The disposable harness relay owner/admin key is an acceptable relay-admin identity for 9030/9031; if stock Buzz rejects the conditional tagged form, fall back to `build_buzz_admission_event_at` (both builders already merged).
- Exact member-only NIP-29 filter shape is pinned during Task 8 TDD against unmodified Buzz; requirement is only that it be member-gated and empty for non-members.
- Post-9031 proactive socket closure is observed, not forced; deterministic assertions are denial of new auth and zero-event reads.
- `max_attempts` default 5; outage tests may raise it per operation via the column.
- No CHANGELOG entry: internal, default-off, test-only surface.
