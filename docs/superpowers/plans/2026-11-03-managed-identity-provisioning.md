# Managed Identity Provisioning (Phase 2B.2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Phase 2B.2 managed-identity provisioning lifecycle — internal, feature-disabled, test-only, idempotent, disposable — on top of the merged Phase 2A vault and Phase 2B.1 signed mapping/transport foundations, exactly as specified in `docs/superpowers/specs/2026-11-03-managed-identity-provisioning-design.md` (read it first; this plan implements it slice by slice).

**Architecture:** A durable, tenant-scoped `managed_chat_identity_operations` table records explicitly authorized provisioning operations with a bounded error catalog and no plaintext-capable columns; retryable `failed` operations stay inside its one-active-operation partial unique index (`ready` alone is lifecycle-terminal; exhaustion is escaped only by raising `max_attempts` or an explicit test-called `abandon` that first disables any attached never-ready identity). A feature-gated (`upstream-buzz-runtime`), `pub(crate)` server provisioner exposes test-called `authorize`/`drive`/`reconcile` — no startup hook, no worker, no route. `authorize` starts from a test-held authenticated Principal/session and runs a test-only `ProvisioningContextResolver` (tenant/workspace membership, enabled user, `ChatIdentityStore::chat_access`) before constructing a `ProvisioningSubject`, then binds `mapping_id`/community/generation from the signed mapping store into the operation; raw UUIDs are never accepted as authorization. Key custody reuses `ManagedIdentityEnvelope` + `LocalDekKeyManagement` + `ManagedIdentityVaultStore::insert_or_load`, and an existing active identity is reused with its actual public key and generation recorded on the operation (no phantom preallocation survives). Admission publishes deterministic NIP-43 kind-9030 events signed by a separate disposable relay-admin key over the existing pinned, generation-fenced `TenantRelaySession` (which gains one test-only `send_text`). Membership proof uses a fresh NIP-42 authentication signed by the one new narrow crypto capability (`RecoveredIdentity::sign_nip42_authentication`, feature `nip42-auth-signing`) — exercised only inside an explicitly authorized operation after encrypted storage and recovery, before `ready`; arbitrary Chat/message signing remains absent — plus a member-only NIP-29 read. `managed_chat_identities` keeps `active`/`disabled`/`revoked` semantics; readiness lives on the operation. The live `upstream_identity_provisioning::tests::live_*` suites run only under the disposable-Buzz harness and are excluded from the aggregate CI ignored-test sweep via `--skip upstream_identity_provisioning::tests::live_` in `.github/workflows/integration-tests.yml`.

**Tech Stack:** Rust 1.97.1 workspace (storage crate `rustshare-storage`, crypto crate `rustshare-crypto`, resource-auth crate `rustshare-resource-auth`, server crate `rustshare-server`), SQLx/PostgreSQL 16+, `nostr` 0.44 wire events, unmodified disposable Buzz relay per `config/buzz-compatibility.env`.

**Spec:** `docs/superpowers/specs/2026-11-03-managed-identity-provisioning-design.md`.

**Hard scope rules (check on every task):** no production route/worker/API/startup caller; no production session constructor for the test-only context resolver (it is reachable only from the feature-gated test-called provisioner); no general signing or profile selector; no `nsec`/secret export; no upstream Buzz modifications or production pin changes; no legacy-identity or M4.1b2 replacement changes; NIP-17 stays disabled; #442/Gate I posture unchanged; the aggregate CI sweep must skip `upstream_identity_provisioning::tests::live_`; draft PR only, DCO-signed commits (`git commit -s`).

---

## File Structure

New:
- `backend/migrations/20261103100000_managed_chat_identity_operations.up.sql`
- `backend/crates/storage/src/managed_identity_operations.rs` — `ManagedIdentityOperationStore`, `ManagedIdentityOperationRecord`, `ProvisioningRequest`, `ManagedIdentityOperationError`.
- `backend/server/src/upstream_identity_provisioning.rs` — `ManagedIdentityProvisioner`, test-only `ProvisioningContextResolver`, `ProvisioningSubject`, `DriveOutcome`, in-crate unit tests + ignored live-Buzz tests (`live_*`).
- `backend/tests/managed_identity_provisioning_store_test.rs` — ignored live-PostgreSQL store invariants.

Modified:
- `.github/workflows/integration-tests.yml` — the aggregate ignored-test sweep gains `--skip upstream_identity_provisioning::tests::live_` (exact module-path skip, same mechanism as the existing `--skip upstream_buzz_transport::disposable_service_tests`).
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

- [ ] **Step 3: Write the migration** per design §5.1: columns, `CHECK`s (status catalog `authorized`/`provisioning`/`ready`/`failed`/`abandoned`; error-code catalog incl. `database_unavailable`; hex admission event id), `UNIQUE (tenant_id, principal_id, idempotency_key)`, partial unique index `managed_chat_identity_operations_one_active` (`WHERE status IN ('authorized','provisioning','failed')` — retryable `failed` rows stay in the index so `authorize` cannot race reconciliation; `ready` alone is lifecycle-terminal), `ENABLE/FORCE ROW LEVEL SECURITY` + tenant-isolation policy identical in shape to `backend/migrations/20261101090000_managed_chat_identity_vault.up.sql:50-55`. No `DOWN` file (forward-only, matching the vault migration).

- [ ] **Step 4: Run it green** with the same command as Step 2.

### Task 2: `ManagedIdentityOperationStore` core (authorize/load/CAS transitions)

**Files:**
- Create: `backend/crates/storage/src/managed_identity_operations.rs`
- Modify: `backend/crates/storage/src/lib.rs`

- [ ] **Step 1: Write failing store tests.** Extend `backend/tests/managed_identity_provisioning_store_test.rs` (all ignored, live PG):

  - `authorize_is_idempotent_per_key_and_rejects_second_active_operation` — same `(tenant, principal, idempotency_key)` twice returns the same `operation_id`; a different key while one is active returns `Conflict`; `identity_id`/`identity_generation` are stable across the retry; `identity_generation` is `max(vault generation)+1` when a vault row exists.
  - `failed_rows_stay_in_the_one_active_index_until_abandoned` — a `failed` operation still blocks a different-key `authorize` (`Conflict`, no race with reconciliation); after `abandon` (from `failed`) a fresh `authorize` succeeds; an `abandoned` row never resumes.
  - `cas_transitions_are_monotonic` — `attach_identity(tenant, op, actual_identity_id, actual_generation)` performs `authorized|failed → provisioning` and records the actual identity values; a second `attach_identity` returns `InvalidTransition`-class error; `mark_ready` only from `provisioning`; `mark_failed` from any non-ready; `record_admission_sent` set-if-null returns `true` then `false` with the same event id preserved; reuse via `attach_identity` leaves the operation's `identity_id`/`identity_generation` equal to the reused vault row's values (no phantom preallocation).
  - `list_retryable_respects_bounds_and_order` — seeded ready/failed/authorized/abandoned rows; only retryable ones (`authorized`/`provisioning`/`failed` below `max_attempts`) returned, ordered by `created_at`, limited.
  - `operations_are_tenant_isolated` — cross-tenant `load`/`list_retryable` sees nothing (RLS probe pattern from `managed_identity_vault_test.rs:253-297`, role `NOSUPERUSER NOBYPASSRLS`).

- [ ] **Step 2: Run red.**
```bash
SQLX_OFFLINE=true cargo test -p rustshare-server --test managed_identity_provisioning_store_test -- --ignored --test-threads=1
```

- [ ] **Step 3: Implement** `managed_identity_operations.rs`: `ManagedIdentityOperationStatus` (as_str/parse; `authorized`/`provisioning`/`ready`/`failed`/`abandoned`), `ManagedIdentityOperationRecord` (`FromRow`), `ProvisioningRequest { tenant_id, principal_id, workspace_id, mapping_id, mapping_generation, idempotency_key }` — the mapping binding is supplied by the provisioner from the signed store, never discovered here — `ManagedIdentityOperationError { Database, Conflict, NotFound, InvalidTransition, InvalidRequest }`, and `ManagedIdentityOperationStore(PgPool)` with local `set_tenant` helper copied from `managed_identity_vault.rs:270-280` (house style), `authorize` (transaction: compute next `identity_generation` via `SELECT COALESCE(MAX(generation),0)+1 FROM managed_chat_identities WHERE tenant_id=$1 AND principal_id=$2`, insert-or-load on the idempotency key, surface 23505 as `Conflict` via `map_database_error` like `managed_identity_vault.rs:282-289`), `load`, `attach_identity(tenant_id, operation_id, identity_id, identity_generation)` (CAS `authorized|failed → provisioning`, recording the actual identity values — reused or newly created), `record_admission_sent` (returns `bool`), `mark_ready` (CAS `provisioning → ready`, `ready_at = now()`), `mark_failed` (CAS any non-`ready` status to `failed`, increments `attempt_count`, validates catalog membership), `abandon` (CAS `failed → abandoned`), `list_retryable`. Re-export from `lib.rs` next to the vault re-exports.

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

  - `context_resolver_accepts_authenticated_entitled_principal` — build the authenticated disposable Principal/session fixture exactly like the existing live suites do (a real user/principal row in the disposable DB, an authenticated session subject, Chat application enablement + user preference, workspace membership); `ProvisioningContextResolver::resolve(...)` returns a `ProvisioningSubject` matching the session's tenant/principal/workspace.
  - `wrong_tenant_or_unentitled_authenticated_context_is_rejected` — an authenticated session whose user belongs to tenant A asked to authorize in tenant B, a workspace the user does not belong to, a disabled user, and a user with `ChatIdentityStore::chat_access == false` each return `invalid_subject` and **no operation row and no identity** are created.
  - `authorize_binds_signed_mapping_into_the_operation` — after resolution, `authorize` loads the mapping from `UpstreamRelayMappingStore` and persists its `mapping_id` and current `generation` on the operation; a signed generation bump makes the stored binding stale (surfaced at `drive` as `stale_mapping_generation`, tested in Slice 5).
  - `concurrent_drive_yields_one_identity` — N=8 tokio tasks `authorize` (same subject, different keys) then `drive` identity stage; assert one vault row, equal pubkeys; non-winning authorize calls return `Conflict` or the same operation.
  - `retry_preserves_pubkey_and_generation` — drive, then drive again + `reconcile`; same pubkey/generation; `attempt_count` bounded.
  - `existing_active_identity_short_circuits` — pre-insert an active identity for the subject; `authorize` (which pre-allocates a new `identity_id` and `identity_generation = max+1`) then the `ensure_identity` step reuses the stored identity: same pubkey and generation, no second vault row, and `attach_identity` rewrites the operation record to the reused `identity_id`/`identity_generation` — assert via `load` that no phantom preallocated values remain.

  For Slice 4, drive stops before the network: factor `ensure_identity` as its own `pub(crate)` async fn and test it plus resolution/`authorize` mapping validation. Tests that need the relay are Slice 5.

- [ ] **Step 3: Run red.**
```bash
SQLX_OFFLINE=true cargo test -p rustshare-server --features upstream-buzz-runtime --lib upstream_identity_provisioning
# with DATABASE_URL for the ignored ones:
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres \
  cargo test -p rustshare-server --features upstream-buzz-runtime --lib upstream_identity_provisioning -- --ignored --test-threads=1
```

- [ ] **Step 4: Implement** the provisioner per design §5.5: struct holding the stores (`ManagedIdentityVaultStore`, `ManagedIdentityOperationStore`, `UpstreamRelayMappingStore`, plus a test-constructed `ChatIdentityStore` for the resolver); `ProvisioningSubject` (output of resolution only); `ProvisioningError` (mapping store/vault/operation/transport error wrappers + catalog mapping, `invalid_subject` for resolution failures); `ProvisioningContextResolver::resolve(authenticated: &AuthenticatedTestPrincipal) -> Result<ProvisioningSubject, ProvisioningError>` validating tenant membership, workspace membership, enabled user, and `ChatIdentityStore::chat_access(...) == true`; `authorize(authenticated, idempotency_key)` = resolve → `mapping_store.load(tenant, workspace)` → `op_store.authorize(ProvisioningRequest { subject fields, mapping_id: record.mapping_id, mapping_generation: record.generation, idempotency_key })`; `ensure_identity` (compare op's mapping binding to the loaded mapping → `load_active_by_principal` → reuse-and-record-actual via `attach_identity(..., actual_identity_id, actual_generation)` or generate+`insert_or_load(idempotency_key = operation_id)` then `attach_identity` with the created values); error paths call `mark_failed(<catalog code>)` before returning.

### Task 7: Crash reconciliation without duplicates (DB-only)

**Files:**
- Modify: `backend/server/src/upstream_identity_provisioning.rs`

- [ ] **Step 1: Failing tests** (ignored, live PG), simulating crashes by driving partial sequences through the `pub(crate)` steps and store mutations:

  - `crash_before_encrypted_storage_reconciles` — `authorize`; drive with KMS failure injected (wrap/unwrap fail) so no insert happens; the operation is `failed`/`kms_unavailable` and no identity row exists; remove the injection; `reconcile` → identity exists exactly once and its `identity_id` equals the operation's pre-allocated id.
  - `crash_after_encrypted_storage_reuses_identity` — `authorize`; generate + `insert_or_load` out-of-band with the operation's own `identity_id`/`identity_generation` (simulating the crash window after storage, before `attach_identity`); `reconcile` → `ensure_identity` finds and reuses the active row: no new key, `attach_identity` completes, pubkey equals the out-of-band envelope's.
  - `kms_outage_fails_closed_and_is_retryable` — wrap the real `LocalDekKeyManagement` in an in-module, test-only `DekKeyManagement` decorator with an `unavailable` flag (mirrors the crypto crate's `TestKms`) so wrap/unwrap fail on demand → per-drive `kms` error maps to `kms_unavailable`, no identity usable, `list_retryable` includes the operation; flip the flag off and `reconcile` → ready-path proceeds.
  - `cross_tenant_identity_substitution_is_rejected` — tenant A identity row; a tenant B operation cannot observe or reference it (tenant RLS on both stores); copying A's envelope fields into a fresh insert for B fails the globally unique `public_key`, and envelope `recover` under B's context fails AAD (`InvalidEnvelope`/`Recovery`).
  - `database_outage_is_retryable_and_unauthorized` — with a second disposable DB handle, `pg_ctl stop -m immediate` mid-drive (pattern: gate report 2026-11-02 entry); drive returns a retryable `database_unavailable` failure; the operation is `failed`/retryable, never `ready`; the NIP-42 capability is never exercised (it is used only inside an explicitly authorized operation that reached storage+recovery, and this one never did); `pg_ctl start`; `reconcile` → completes.
  - `attempt_exhaustion_blocks_reauthorize_until_abandon` — `max_attempts = 1` with a failing KMS decorator: drive → `failed`/`attempts_exhausted`; a new-key `authorize` → `Conflict` (the `failed` row stays in the one-active index); raising `max_attempts` makes it retryable again (no duplicate identity); and an explicit `abandon` first disables the attached never-`ready` identity and leaves the index, after which a fresh `authorize` succeeds at the next generation with exactly one active identity.

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
  - `live_relay_outage_is_retryable_and_unauthorized` — stop the unmodified disposable Buzz relay before `drive` → connect/admission fails closed (`transport_unavailable`/`admission_rejected`), the operation is `failed`/retryable, never `ready`, and the NIP-42 capability is never exercised; restart the relay; `reconcile` → `ready`.
  - `live_stale_mapping_generation_fails_closed` — signed generation `g→g+1` update between `authorize` and `drive` → `stale_mapping_generation`; refresh the operation's mapping binding via a new `authorize` after the update → proceeds.
  - `live_nip43_replay_is_idempotent_at_operation_layer` — simulate the crash window honestly: with the operation in `provisioning` (identity stored, `admission_event_id NULL`), deterministically build the kind-9030 event out-of-band from the same stored inputs drive uses (`created_at` from `operation.created_at`, `command_id = operation_id`, `expected_revision = mapping.generation`), send it on a session, and observe relay acceptance; then `drive` → the `admit` step rebuilds the **identical** event id, sends it, and records it via `record_admission_sent`; assert the operation-layer outcomes only — one operation, one identity, `admission_event_id` equals the out-of-band event id, `ready` — without asserting anything about the relay's duplicate-id response (observed and recorded, not assumed). Do NOT clear `admission_event_id` on an already-`ready` row.

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
  - `live_no_key_material_in_logs_durable_records_errors_or_payloads` — strengthened sentinel: (a) wrap `send_text` in a test-only recorder that keeps every relay payload the suite sent; (b) keep the relay-admin `nsec` and its secret bytes as a known canary (the managed key's secret never leaves the vault and is unknown to the test, so the canary plus patterns is the honest check); (c) after the suite, scan — the captured sanitized test log (bounded, sha256-recorded per gate-report hygiene), **every value of every column of every durable operation row** for the tenant (all statuses, `ready`/`abandoned` included), the bounded `ProvisioningError`/catalog strings, and the recorded relay payloads — asserting the canary, `nsec1`, and 32-byte-secret patterns never appear (pubkeys and event ids, which are public, are expected); (d) reuse the crypto-crate assertion that envelope/`RecoveredIdentity` `Debug` is non-exhaustive; (e) assert no production error response exists for this lifecycle — there is no route — so the only error surface is the persisted catalog code.

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

- [ ] **Step 1: Full Rust baseline + sweep wiring.**
```bash
cargo fmt --all --check
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --workspace --all-features --lib
cargo sqlx prepare --workspace --check   # only if any query macro changed; the new store uses query/query_as like existing code — confirm with prepare --check either way
```
Confirm the aggregate ignored-test sweep in `.github/workflows/integration-tests.yml` (the `cargo test --workspace --all-features -j 1 -- --ignored ...` step) now carries **both** skips:
```bash
rg -n -- '--skip upstream_buzz_transport::disposable_service_tests|--skip upstream_identity_provisioning::tests::live_' .github/workflows/integration-tests.yml
```
CI-parity sweep shape (full run happens in CI; locally verify skip behavior on the server lib):
```bash
SQLX_OFFLINE=true cargo test -p rustshare-server --features upstream-buzz-runtime --lib upstream_identity_provisioning -- --ignored --skip upstream_identity_provisioning::tests::live_ --test-threads=1
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
- `max_attempts` defaults to 5 and is a column; attempt exhaustion keeps the `failed` row in the one-active index (blocking re-authorization), and the only escapes are raising `max_attempts` or the explicit test-called `abandon` (which first disables any attached never-`ready` identity).
- The context resolver's entitlement checks reuse the Chat authz layer's facts — `ChatIdentityStore::chat_access` plus tenant/workspace membership and enabled-user state; no new entitlement tables are introduced.
- The aggregate CI sweep exclusion relies on libtest substring matching of the unit-test path (`--skip upstream_identity_provisioning::tests::live_`); keep the workflow skip in sync with any module/test rename (Slice 6 greps for it).
- No CHANGELOG entry: internal, default-off, test-only surface.
