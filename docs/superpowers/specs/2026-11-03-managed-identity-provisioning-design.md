# Managed Identity Provisioning (Phase 2B.2) — Design

> **Status:** Approved design for implementation planning; no code is implemented by this document.
> **Date:** 2026-11-03
> **Related:** ADR-0043 (legacy nil-tenant compatibility), ADR-0044 (managed Buzz identity upstream-only),
> ADR-0045 (self-hosted managed identity key wrapping),
> `docs/contracts/managed-chat-identity-v1.md` (target contract),
> `docs/plans/elembra-phase2b-gate-i-readiness-2026-11-01.md` (Gate I readiness, still **NOT ACCEPTED**),
> PRs #445/#446/#449/#450 merged foundations (the trusted transport landed via the merged PR #450).
> **Companion plan:** `docs/superpowers/plans/2026-11-03-managed-identity-provisioning.md`.

## 1. Goal

Add an **internal, feature-disabled, test-only, idempotent, disposable** managed-identity
**provisioning lifecycle** on top of the already-merged Phase 2A/2B.1 foundations, so that
automated tests — and only tests — can, against **live disposable PostgreSQL and an unmodified
disposable Buzz relay**:

1. start from a **trusted, authenticated, disposable Principal/session** and validate it through a
   test-only context resolver — tenant/workspace membership, enabled user, and Chat entitlement
   (`ChatIdentityStore::chat_access`) — before any `ProvisioningSubject` exists (raw UUIDs alone
   are never authorization); then authorize provisioning for that subject;
2. verify the tenant's **signed relay mapping**, its provenance, current generation, and endpoint
   before any key exists;
3. create **exactly one** managed identity per subject under concurrency, preserving the existing
   identity's pubkey and generation on every retry;
4. generate the key **only** when an explicitly authorized operation exists and no identity/operation
   already does, encrypt the private key into the **existing** envelope vault, and persist a
   **durable operation record that never holds plaintext**;
5. admit the fresh public key to the mapped community with a **separately provisioned disposable
   relay-admin key** signing NIP-43 **kind 9030** — never the managed user key;
6. mark the identity usable **only after** a fresh NIP-42 authentication of the managed key plus a
   member-only NIP-29 read succeed against the mapped relay;
7. interrupt and reconcile the operation at every step **without duplicates**; and
8. prove that kind **9031** revocation, mapping/tenant/generation mismatch, KMS outage, and
   DB/relay outage all fail closed.

This is deliberately **not** Gate I work. It creates no production route, worker, API, general
signing surface, or profile selector, and it changes nothing about the desktop acceptance
gate ([#442](https://github.com/kubedoio/rustshare/issues/442)), which remains a Gate I
prerequisite and stays deferred/unverified.

## 2. Non-goals and exclusions

The following are **out of scope** for Phase 2B.2 and must not be enabled or weakened by it:

- No production route, HTTP API, background worker, startup hook, or runtime caller of any kind.
  The only callers are explicit test invocations (`drive` / `reconcile`). The context resolver
  has no production session constructor either — it accepts only the test-held authenticated
  fixture type and is unreachable outside the feature-gated, test-called provisioner module.
- No general event/message signing and no signer/profile selector. The managed key can sign
  **only** NIP-42 kind-22242 authentication events, and only through one narrow feature-gated
  method with no secret-bytes or `nsec` getter.
- No credential export (`nsec`, `ncryptsec`, or otherwise), no real-user keys, no production KMS
  provider, and no KEK material in the repository, environment, or database.
- No modification to upstream Buzz (relay stays at the pinned, unmodified revision), no
  production image/pin changes, no legacy (`legacy_client_held`) identity migration, and no change
  to the M4.1b2 replacement guards in `chat_identity_lifecycle_test.rs` or the replacement store.
- NIP-17/NIP-44 gift-wrap DMs remain excluded and disabled.
- Gate I posture is unchanged: this slice produces test-only evidence for planning; it is not an
  independent security acceptance, not a custody approval, and not a merge/deploy authorization.
- The target contract `docs/contracts/managed-chat-identity-v1.md` remains a *target* contract;
  where its proposed schema names differ from the merged Phase 2A schema, the merged schema wins.

## 3. Existing foundations reused unchanged

Every cryptographic, storage, and transport primitive below already exists at the reviewed
baseline (`67703989915c48d736afea56c188503f315b1eee`) and is reused as-is; Phase 2B.2 adds only the
narrow additions listed in §5.

| Capability | Existing symbol | File |
|---|---|---|
| Envelope encryption (AES-256-GCM, per-identity DEK, AAD bound to tenant/principal/identity/generation/version/pubkey, zeroization) | `ManagedIdentityEnvelope::generate`, `ManagedIdentityEnvelope::recover`, `IdentityContext`, `WrappedDek`, `VaultError` | `backend/crates/crypto/src/managed_identity_vault.rs` |
| `RecoveredIdentity` — exposes **only** `public_key()`; no signing/export API | `RecoveredIdentity` | `backend/crates/crypto/src/managed_identity_vault.rs` |
| DEK wrapping trait (KEK never exposed) | `DekKeyManagement` | `backend/crates/crypto/src/managed_identity_vault.rs` |
| Default-off software KEK provider (operator-mounted key files; test-only here) | `LocalDekKeyManagement`, `LocalDekKeyProviderConfig` (feature `local-dek-key-provider`) | `backend/crates/crypto/src/local_dek_key_provider.rs` |
| Vault persistence (idempotent `insert_or_load`, rewrap CAS, `disable_or_revoke`, tenant RLS, immutable attribution trigger, global unique `public_key`) | `ManagedIdentityVaultStore`, `ManagedIdentityRecord`, `ManagedIdentityStatus::{Active,Disabled,Revoked}` | `backend/crates/storage/src/managed_identity_vault.rs` |
| Vault schema | `managed_chat_identities` | `backend/migrations/20261101090000_managed_chat_identity_vault.up.sql` |
| Signed mapping policy (immutable identity, generation CAS, endpoint policy) | `TrustedRelayMapping`, `RelayMappingRecord`, `MappingScope`, `select_mapping`, `verify_connected_relay`, `RelayEndpointPolicy` | `backend/crates/resource-auth/src/upstream_relay_mapping.rs` |
| Signed mapping store (verifies operator Ed25519 statements, replay-protected `apply`, tenant RLS, append-only audit) | `UpstreamRelayMappingStore::{apply,load,load_for_management}`, `MappingProvisioningVerifier`, `MappingProvisioningStatement` | `backend/crates/storage/src/upstream_relay_mapping_store.rs`, `backend/crates/resource-auth/src/upstream_relay_provisioning.rs` |
| Trusted transport (pinned DNS/TLS/SPKI, NIP-11 relay-identity check, generation-fenced receive-only session; feature `upstream-buzz-runtime`, `pub(crate)`, no runtime caller) | `connect`, `TenantRelaySession::receive`, `RelayTlsTrustConfig`, `UpstreamBuzzTransportError::SessionFenced` | `backend/server/src/upstream_buzz_transport.rs` |
| NIP-43 membership admission builders (kinds 9030/9031; deterministic variants for idempotent replay) | `build_buzz_admission_event_at`, `build_buzz_conditional_admission_event_at`, `BuzzAdmissionOperation::{Admit,Revoke}`, `BUZZ_RELAY_ADD_MEMBER_KIND`, `BUZZ_RELAY_REMOVE_MEMBER_KIND` | `backend/crates/resource-auth/src/chat_identity.rs` |
| NIP-42 proof verification policy (existing binding flow, untouched) | `BindingChallenge::verify_proof`, `authorize_admission` | `backend/crates/resource-auth/src/chat_identity.rs` |
| Test conventions for live PostgreSQL | `#[tokio::test] #[ignore]` + `DATABASE_URL` disposable database, `sqlx::migrate!` | `backend/tests/managed_identity_vault_test.rs`, `backend/tests/chat_identity_lifecycle_test.rs` |
| Test conventions for live disposable Buzz | ignored in-crate tests (e.g. `trusted_service_transport_connects_and_authenticates_member`) over the pinned unmodified relay harness | `backend/server/src/upstream_buzz_transport.rs` tests, `scripts/run-buzz-conformance.sh`, `scripts/upstream-buzz-poc/` |

## 4. Architecture decisions

1. **Durable provisioning-operation persistence plus explicit test-called `drive`/`reconcile`;
   no startup hook, no worker.** A new tenant-scoped table `managed_chat_identity_operations`
   records each explicitly authorized logical provisioning request, its state, bounded error
   catalog values, attempt counters, and public correlation IDs (operation id, identity id,
   mapping id/generation, admission event id). A new in-server provisioner exposes
   `authorize` → `drive` → `reconcile`, each invoked directly by tests. *Rationale:* the Gate I
   readiness report (§5 of `docs/plans/elembra-phase2b-gate-i-readiness-2026-11-01.md`) requires
   idempotent reconciliation of interruptions without duplicates; a durable operation row with
   unique partial indexes is the smallest structure that survives process restarts and concurrent
   retries. A worker/startup hook would create a runtime caller, which is explicitly excluded.

2. **Authorization starts from a trusted authenticated context, never from raw UUIDs.** The
   provisioner's `authorize` takes a test-held authenticated Principal/session fixture — the same
   kind of authenticated subject the existing live suites construct (cf. `auth(...)` in
   `backend/tests/buzz_live_conformance_test.rs`) — and passes it through a test-only
   `ProvisioningContextResolver` before any `ProvisioningSubject` is built. The resolver
   validates, against the disposable database: the authenticated Principal/session belongs to the
   tenant; the user is a member of the target workspace and is not disabled (the same
   principal-state facts the Chat authz layer and `user_disable` paths rely on); and Chat
   entitlement holds via `ChatIdentityStore::chat_access(tenant_id, workspace_id, principal_id)
   == true`. Any mismatch → `invalid_subject`: no operation row, no identity, no key.
   *Rationale:* the Gate I report's Phase 2B.2 predicate 1 requires an authenticated, active,
   entitled subject in the same authorized workflow; raw caller-supplied UUIDs are not
   authorization. The resolver is compiled only inside the feature-gated, test-called provisioner
   module — there is no route, no production session constructor, and no runtime way to invoke it.

3. **Keep `managed_chat_identities` `active`/`disabled`/`revoked` semantics unchanged.** The
   contract's identity-level `pending_provisioning | ready | suspended | revoked | failed` ladder
   is realized at the **operation** layer instead: the vault row becomes `active` when the
   encrypted identity is durably stored, while `ready` is a property of the provisioning
   operation (admission proven by fresh NIP-42 + member-only NIP-29 read). *Rationale:* the Phase
   2A schema is merged with RLS, immutable-attribution triggers, and tests; renaming/altering its
   status domain would churn a safety-boundary migration for no behavioral gain, and the contract
   itself says names/shapes "MUST be reconciled" with implementation. `RecoveredIdentity` recovery
   and envelope AAD stay exactly as merged.

4. **Reuse the existing envelope vault and stores; add no parallel key material.** Key generation,
   DEK wrapping (`LocalDekKeyManagement` in tests), envelope insert (`insert_or_load`), and
   recovery all go through the existing Phase 2A path. *Rationale:* the contract requires durable
   encryption with no plaintext; any test-only side channel of unencrypted keys is forbidden
   (see §4.5).

5. **The narrowest possible NIP-42 signing capability lives in the crypto crate behind a new
   default-off Cargo feature; server-side trait injection cannot avoid this and is rejected.**
   `RecoveredIdentity` gains exactly one feature-gated method
   `sign_nip42_authentication(challenge, relay_url, created_at) -> Result<nostr::Event, VaultError>`
   that signs **only** kind-22242 `AUTH` events (challenge + relay tags, empty content), exposing
   no secret bytes, no `nsec`, no arbitrary kind/content/tag parameters. Alternatives considered:
   - *Server test-only trait injection* (define `Nip42AuthSigner` in the server, implement in
     tests): **rejected.** The managed secret never leaves the crypto crate — `RecoveredIdentity`
     deliberately holds `_secret_key` privately — so any implementation must live in
     `rustshare-crypto` anyway; moving the trait to the server adds indirection without removing
     the crypto API addition, and a test-side implementation would need a parallel unencrypted
     key, which is forbidden.
   - *Parallel test-only key held by the server/test (no crypto change)*: **rejected.** Violates
     the no-plaintext rule and would make the admitted Buzz key differ from the custodied key.
   - *Generic `sign(kind, content, tags)` on `RecoveredIdentity`*: **rejected.** Arbitrary signing
     is explicitly out of scope; a typed per-operation method keeps the custody boundary reviewable.
   - *`#[cfg(test)]`-only method*: **rejected.** The disposable-Buzz acceptance tests are ignored
     **unit** tests inside the server crate, so `cfg(test)` would compile there; but the capability
     must also be visible to storage-level integration checks and to `--all-features` CI, where
     dependency crates are compiled without `cfg(test)`.
   *Rationale:* one typed method, one default-off Cargo feature (`nip42-auth-signing`, mirroring
   `local-dek-key-provider`, `upstream-relay-provisioning`, `upstream-buzz`,
   `upstream-buzz-runtime`), one `VaultError` variant. The feature is enabled through the existing
   default-off server chain (`upstream-buzz-runtime`) rather than dev-dependencies, because the
   provisioner module is feature-gated but *not* `cfg(test)`-gated (it mirrors the transport
   module), and dev-dependency-only feature unification would leave non-test feature builds unable
   to compile it. What keeps the lifecycle test-only is reachability, not compile-time
   visibility: the only caller of this method is the test-called `pub(crate)` provisioner, and
   default production builds have neither the feature nor any caller.

6. **Admission uses a separately provisioned disposable relay-admin key over the trusted
   transport; the managed user key never signs membership commands.** Tests load the disposable
   relay's owner/admin `nsec` from the harness-owned mode-0600 file (never persisted to Elembra
   storage), build the kind-9030 event with the existing
   `build_buzz_conditional_admission_event_at` (operation UUID + expected revision + pinned
   `created_at` from the durable operation, making retries replay-identical), and publish it on the
   same pinned, generation-fenced `TenantRelaySession`. *Rationale:* the contract requires a
   separate service/relay-owner identity; the conditional builder's deterministic id gives
   idempotent NIP-43 replay at the operation layer without upstream modifications.

7. **Readiness requires a fresh NIP-42 authentication of the managed key plus a member-only NIP-29
   read; a NIP-43 command ACK alone never marks `ready`.** After the 9030 admission event is
   accepted, the provisioner authenticates the just-custodied key with the relay-issued challenge
   and performs one member-only NIP-29 read (e.g. kind-9 private-channel history in the disposable
   harness) that must return events; an empty result, an `auth-required` closure, or a denial marks
   the operation failed (retryable) and the identity is never usable as a Chat identity. Note the
   NIP-42 authentication that proves membership necessarily happens **before** `ready` — readiness
   is the *conclusion* of that proof, not a precondition for signing. *Rationale:* directly
   implements the contract's success criterion with the same wire flow already qualified in the
   merged transport conformance (PR #450).

8. **Every interruption point is reconciled from durable state with exactly-once effect; retryable
   `failed` operations stay inside the one-active-operation index.** `insert_or_load(operation_id
   as idempotency key)` deduplicates the identity row; the partial unique index covers
   `status IN ('authorized','provisioning','failed')`, so an `authorize` with a new key issued
   while a `failed` operation is being reconciled returns `Conflict` instead of racing
   reconciliation into a second operation; the deterministic 9030 event id deduplicates admission;
   `mark_ready` is a compare-and-swap. `ready` is the only lifecycle-terminal status. `failed` is
   resumable: `reconcile` (and re-`drive`) transition it back to `provisioning` from its durable
   checkpoint (identity reuse, recorded admission event id, and bounded attempts all survive).
   Attempt exhaustion is resolved conservatively: an exhausted `failed` row keeps blocking new
   authorizations for the subject — no duplicate identity can be created — and the only escapes
   are (a) raising `max_attempts` (a column, so infrastructure outages stay retryable without code
   changes) or (b) an explicit test-called `abandon` that first disables any attached
   never-`ready` identity via the existing vault `disable_or_revoke` and only then transitions the
   row to `abandoned`, leaving the index. *Rationale:* satisfies the crash-before/after-storage
   and duplicate-worker requirements with the smallest state machine
   (`authorized → provisioning → ready`, `→ failed` from any non-ready point and back,
   `→ abandoned` only explicitly).

9. **Fail closed everywhere; errors are a bounded catalog and carry no secrets.** KMS unwrap/AAD
   failure, mapping load/validation failure, generation mismatch, transport failure, admission
   rejection, authentication rejection, and unproven membership all fail the operation with a
   fixed `error_code` from a `CHECK`-constrained catalog. No column of the operation table holds
   key material, ciphertext, nonces, or free-form error text; `RecoveredIdentity`'s non-exhaustive
   `Debug` and the existing envelope invariants are unchanged. There is no production error
   response for this lifecycle at all — no route exists — so the only error surface is the catalog
   code persisted on the operation row plus bounded `Debug` text in test output, all covered by
   the sentinel scan (§6 row 12). *Rationale:* the contract's must-pass "secret logging sentinel"
   and "DB-only dump" tests are satisfied structurally.

10. **The provisioner is feature-gated and `pub(crate)` with no runtime caller, mirroring the
   transport module's pattern.** `backend/server/src/lib.rs` adds the module under
   `#[cfg(feature = "upstream-buzz-runtime")]` with an `#[expect(dead_code, ...)]`-style
   justification, exactly like `upstream_buzz_transport`. *Rationale:* keeps the code compiled and
   linted under `--all-features` CI so it cannot bit-rot, while remaining unreachable at runtime.

11. **Live evidence uses only live disposable PostgreSQL and the unmodified disposable Buzz
    harness — and the aggregate CI ignored-test sweep must not try to run it.** DB-level tests
    follow `managed_identity_vault_test.rs` (`#[ignore]`, `DATABASE_URL`, disposable DB).
    Relay-level tests follow the merged PR #450 in-crate ignored tests against the pinned Buzz
    revision from `config/buzz-compatibility.env`, with the kind/Cilium harness as the qualifying
    environment but a plain disposable relay as the minimum; they are named
    `upstream_identity_provisioning::tests::live_*` and are excluded from the aggregate sweep in
    `.github/workflows/integration-tests.yml` (the `cargo test --workspace --all-features -- --ignored`
    step) via libtest `--skip upstream_identity_provisioning::tests::live_` — the same mechanism as
    the existing `--skip upstream_buzz_transport::disposable_service_tests` — because they require
    the disposable Buzz harness, which the sweep job does not provision for this module. No
    assertion depends on modified upstream behavior; where stock Buzz behavior is the object
    (e.g. post-9031 socket closure), the test asserts the deterministic denial path and records
    the observed relay behavior as evidence.

## 5. Component design

### 5.1 New: provisioning-operation persistence (storage crate)

New module `backend/crates/storage/src/managed_identity_operations.rs`, always compiled like its
vault sibling, documented "no runtime caller; test-called provisioning lifecycle only."

Migration `backend/migrations/20261103100000_managed_chat_identity_operations.up.sql`
(forward-only, matching the vault migration which ships no down file), tenant-aware per ADR-0043:

```sql
CREATE TABLE managed_chat_identity_operations (
    operation_id       UUID PRIMARY KEY,
    tenant_id          UUID NOT NULL,
    principal_id       UUID NOT NULL,
    workspace_id       UUID NOT NULL,
    mapping_id         UUID NOT NULL,
    mapping_generation BIGINT NOT NULL CHECK (mapping_generation > 0),
    identity_id        UUID NOT NULL,          -- pre-allocated at authorize; corrected to the actual (reused) identity at attach_identity
    identity_generation BIGINT NOT NULL CHECK (identity_generation > 0),
    idempotency_key    UUID NOT NULL,
    status TEXT NOT NULL DEFAULT 'authorized'
        CHECK (status IN ('authorized','provisioning','ready','failed','abandoned')),
    error_code TEXT CHECK (error_code IS NULL OR error_code IN (
        'invalid_subject','mapping_unavailable','stale_mapping_generation',
        'kms_unavailable','identity_conflict','transport_unavailable',
        'admission_rejected','authentication_rejected','membership_unproven',
        'database_unavailable','attempts_exhausted','internal_error')),
    attempt_count  INT NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_attempts   INT NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
    admission_event_id TEXT CHECK (admission_event_id IS NULL OR admission_event_id ~ '^[0-9a-f]{64}$'),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    admitted_at    TIMESTAMPTZ,
    ready_at       TIMESTAMPTZ,
    last_attempt_at TIMESTAMPTZ,
    UNIQUE (tenant_id, principal_id, idempotency_key)
);
-- Retryable `failed` rows stay in the one-active-operation index so `authorize`
-- cannot race reconciliation into a second operation; `ready` and `abandoned` are outside it.
CREATE UNIQUE INDEX managed_chat_identity_operations_one_active
    ON managed_chat_identity_operations(tenant_id, principal_id)
    WHERE status IN ('authorized','provisioning','failed');
-- RLS: ENABLE + FORCE, policy on tenant_id = app.current_tenant_id, same shape as the vault table.
```

Notes on the schema:
- `identity_id`/`identity_generation` are pre-allocated at `authorize` time so a crash before/after
  vault insert reconciles to the same identity when none exists yet; when an active identity
  already exists, `attach_identity` overwrites both with the reused row's actual values, so no
  phantom preallocated generation ever remains attached.
- No bytea, no ciphertext, no nonce, no free-form error text: the table cannot carry key
  material by construction.
- Exactly one in-index operation per subject (`authorized`/`provisioning`/`failed`);
  `ready` is lifecycle-terminal; `abandoned` is the explicit, test-called exit from the index
  after attempt exhaustion (with any attached never-`ready` identity disabled first).

New store `ManagedIdentityOperationStore` (methods, all tenant-scoped via local
`set_config('app.current_tenant_id', ...)` transactions like the vault store). The store never
discovers mappings itself: the mapping binding arrives in `ProvisioningRequest`, supplied by the
provisioner from the signed mapping store after trusted subject resolution.

- `ProvisioningRequest { tenant_id, principal_id, workspace_id, mapping_id, mapping_generation,
  idempotency_key }`.
- `authorize(request: ProvisioningRequest) -> Result<ManagedIdentityOperationRecord, _>` —
  insert-or-load on `(tenant_id, principal_id, idempotency_key)`; a different key while an
  in-index operation exists returns `Conflict` (a `failed` row therefore blocks re-authorization
  until it is resumed or abandoned — no race with reconciliation); computes
  `identity_generation = max(existing vault generation for subject) + 1` in the same transaction.
- `load(tenant_id, operation_id)`.
- `attach_identity(tenant_id, operation_id, identity_id, identity_generation)` — CAS
  `authorized|failed → provisioning`, recording the actual identity values (reused or newly
  created) so the operation always reflects the real vault row.
- `record_admission_sent(tenant_id, operation_id, event_id)` — CAS set-if-null inside
  `provisioning`; returns whether this call won (drives the replay test).
- `mark_ready(tenant_id, operation_id)` — CAS `provisioning → ready` (`ready_at = now()`); the
  only transition out of the index by success.
- `mark_failed(tenant_id, operation_id, error_code)` — CAS any non-`ready`/`abandoned` status to
  `failed`, increments `attempt_count`, sets bounded `error_code`. The row remains in the index
  and resumable.
- `abandon(tenant_id, operation_id)` — CAS `failed → abandoned`, leaving the index; the
  provisioner wrapper disables any attached never-`ready` identity before calling it.
- `list_retryable(tenant_id, limit)` — `status IN ('authorized','provisioning','failed') AND
  attempt_count < max_attempts ORDER BY created_at LIMIT $n`.

`ProvisioningError` enum mirrors the existing store-error style (`Database`, `Conflict`,
`NotFound`, `InvalidTransition`, `InvalidRequest`).

### 5.2 Extended: vault store (one additive method)

`ManagedIdentityVaultStore::load_active_by_principal(tenant_id, principal_id) ->
Result<Option<ManagedIdentityRecord>, ManagedIdentityStoreError>` — the provisioner needs an
active-identity lookup by subject (the merged store only has `get` by `identity_id`). Return type
is `Option` so "absent" (generate) is distinguishable from "stored" (reuse) during reconciliation.

### 5.3 Extended: crypto crate (one feature, one method, one error variant)

`backend/crates/crypto/Cargo.toml` gains default-off feature `nip42-auth-signing = []`.

In `managed_identity_vault.rs`:

```rust
#[cfg(feature = "nip42-auth-signing")]
impl RecoveredIdentity {
    /// Sign one NIP-42 kind-22242 AUTH event binding `challenge` and `relay_url`.
    /// This is the only signing capability of the managed identity: no arbitrary
    /// kinds, no caller-supplied tags, no secret bytes or nsec exposure.
    pub fn sign_nip42_authentication(
        &self,
        challenge: &str,
        relay_url: &str,
        created_at: nostr::Timestamp,
    ) -> Result<nostr::Event, VaultError>;
}
```

Implementation notes: parse `relay_url` as `nostr::RelayUrl` (reject on failure), build
`EventBuilder::auth`, `custom_created_at`, `sign_with_keys` from the internal zeroized secret
(the same hex-parse-and-zeroize pattern `recover` already uses), drop the temporary `Keys`, and
return the signed event. `VaultError` gains one unconditional variant
`AuthenticationSigning` ("managed identity authentication signing failed"). The method is compiled
only with the feature; the feature is wired into the server's default-off `upstream-buzz-runtime`
chain (§7), so default production builds are unchanged and contain no caller.

### 5.4 Extended: trusted transport (one additive send)

`TenantRelaySession` gains:

```rust
/// Test-called provisioning only; no production caller sends on this session.
/// A closed socket fails with `Connection`; otherwise the mapping generation
/// is revalidated before sending and any mapping change fails closed with
/// `SessionFenced`, mirroring `receive`.
pub(crate) async fn send_text(&mut self, text: &str) -> Result<(), UpstreamBuzzTransportError>;
```

The session stays receive-only for all existing callers; provisioning tests use `send_text` for
the 9030 admission event, the 22242 AUTH envelope, NIP-29 `REQ`/`CLOSE`, and 9031 revocation
events signed by the disposable relay-admin key.

### 5.5 New: provisioner (server crate, feature-gated, `pub(crate)`)

`backend/server/src/upstream_identity_provisioning.rs`, registered in `lib.rs` as:

```rust
#[cfg(feature = "upstream-buzz-runtime")]
#[expect(dead_code, reason = "provisioning is test-called only; no runtime caller until a separately authorized phase")]
pub(crate) mod upstream_identity_provisioning;
```

`ManagedIdentityProvisioner` holds `ManagedIdentityVaultStore`,
`ManagedIdentityOperationStore`, `UpstreamRelayMappingStore`, and a test-constructed
`ChatIdentityStore` (for the resolver). It is deliberately constructed directly by tests (no
`AppState` wiring, no config, no route). Authorization is two real steps, never raw UUIDs:

1. `ProvisioningContextResolver::resolve(authenticated: &AuthenticatedTestPrincipal) ->
   Result<ProvisioningSubject, ProvisioningError>` — `AuthenticatedTestPrincipal` is the
   test-held authenticated Principal/session fixture (the same kind of authenticated subject the
   existing live suites construct). The resolver validates, in the disposable database: the
   session's Principal belongs to the tenant; the user is a member of the target workspace and
   is not disabled (the same principal-state facts the Chat authz layer and `user_disable` paths
   rely on); and `ChatIdentityStore::chat_access(tenant_id, workspace_id, principal_id)` is
   `true`. Any mismatch → `invalid_subject`, nothing is created. Only a successfully resolved
   `ProvisioningSubject` (tenant_id, principal_id, workspace_id) can reach `authorize`.
2. `authorize` loads the tenant/workspace mapping from the signed store
   (`UpstreamRelayMappingStore::load` — freshness, uniqueness, and generation validation included)
   and binds `mapping_id` and `mapping_generation` from that record into the operation via
   `ProvisioningRequest`. The caller never supplies a mapping, community, relay URL, or
   generation, and the operation store never discovers them itself.

Key entry points:

```rust
pub(crate) async fn authorize(
    &self,
    authenticated: &AuthenticatedTestPrincipal,  // trusted session fixture; resolved, never trusted raw
    idempotency_key: Uuid,
) -> Result<ManagedIdentityOperationRecord, ProvisioningError>;

pub(crate) async fn drive(
    &self,
    tenant_id: Uuid,
    operation_id: Uuid,
    kms: &dyn DekKeyManagement,        // LocalDekKeyManagement in tests
    relay_admin: &nostr::Keys,         // disposable harness relay-admin key; test-held only
    trust: &RelayTlsTrustConfig,       // built by the test from the signed mapping
) -> Result<DriveOutcome, ProvisioningError>;   // Ready{pubkey, generation} | NotReady{status}

pub(crate) async fn reconcile(
    &self,
    tenant_id: Uuid,
    limit: usize,
    /* same test-supplied collaborators as drive */
) -> Result<ReconcileSummary, ProvisioningError>;

pub(crate) async fn abandon(
    &self,
    tenant_id: Uuid,
    operation_id: Uuid,
) -> Result<(), ProvisioningError>;   // disables any attached never-ready identity, then CAS out of the index
```

`drive` is composed from smaller `pub(crate)` steps (`ensure_identity`, `admit`,
`prove_membership`) so in-crate tests can interrupt after any step, simulate a crash, and let
`reconcile` finish without duplicates.

Drive sequence (each failure maps to `mark_failed(<catalog code>)` and a retryable error):

1. Load the operation (tenant-scoped). `ready` → return `Ready` immediately (idempotent replay),
   reporting the attached identity's actual public key and generation (loaded from the vault via
   the operation's `identity_id`). `abandoned` → error.
   `attempt_count >= max_attempts` → `attempts_exhausted`; the row stays in the index and blocks
   re-authorization until `max_attempts` is raised or `abandon` is called explicitly.
   `failed` is **resumable**: a re-`drive`/`reconcile` transitions it back to `provisioning` and
   continues from the durable checkpoint.
2. Load the tenant/workspace mapping via `UpstreamRelayMappingStore::load` (validates freshness,
   uniqueness, generation). Reject when `mapping_id`/`mapping_generation` on the operation differ
   from the loaded mapping (`stale_mapping_generation`) — this covers wrong-generation and
   wrong-community attempts without any caller-supplied destination.
3. `ensure_identity`: `load_active_by_principal` → on `Some`, reuse the stored active identity
   (idempotent replay preserves its pubkey and generation); on `None`, `ManagedIdentityEnvelope::generate`
   with the operation's pre-allocated `identity_id`/`identity_generation` and
   `insert_or_load(idempotency key = operation_id)` — concurrent retries return the committed
   winner. Either way, `attach_identity(tenant_id, operation_id, actual_identity_id,
   actual_generation)` records the **actual** vault values on the operation, so no phantom
   preallocated generation ever remains attached and `DriveOutcome::Ready` reports the real
   pubkey and generation. Cross-tenant key substitution is blocked here by tenant RLS on both
   stores, by the globally unique `public_key`, and at step 4 by envelope AAD binding.
4. `envelope().recover(context, kms)` — KMS/AAD failure → `kms_unavailable`, fail closed; no
   plaintext ever exists outside the vault.
5. `connect(...)` on the trusted transport (pinned DNS/TLS/SPKI/NIP-11, mapping generation fenced)
   → `transport_unavailable` on failure.
6. `admit` (only when `admission_event_id IS NULL`): build kind 9030 with
   `build_buzz_conditional_admission_event_at(Admit, managed_pubkey, relay_admin,
   created_at = operation.created_at (second-precision, stored), command_id = operation_id,
   expected_revision = mapping.generation)`, send, require relay acceptance, then
   `record_admission_sent(event.id)`. A retry after a crash rebuilds the identical event id, so
   the operation layer deduplicates admission by construction; the test proves this by sending
   the deterministic event out-of-band before the CAS (§6 row 8) rather than by assuming relay
   replay semantics.
7. `prove_membership`: fresh NIP-42 handshake (relay challenge →
   `RecoveredIdentity::sign_nip42_authentication(challenge, mapping.relay_url, now)` → AUTH) and
   one member-only NIP-29 `REQ`; denial, `auth-required` closure, or an empty result →
   `admission_rejected`/`authentication_rejected`/`membership_unproven`. The NIP-42 capability is
   exercised **only** here — inside an explicitly authorized operation, after encrypted storage
   and recovery, before `ready`; arbitrary Chat/message signing remains absent. On success
   `mark_ready` and return `Ready { pubkey, generation }` (the attached identity's actual values).

`reconcile` loads `list_retryable` and drives each operation sequentially, transitioning
resumable `failed` rows back to `provisioning`; it never invents operations, never runs at
startup, and is only invoked by tests. `abandon` is the conservative exhaustion escape: it
disables any attached never-`ready` identity via the existing vault `disable_or_revoke`, then
CAS the `failed` row to `abandoned`, leaving the index; a subsequent `authorize` computes the next
generation over all prior rows and can never duplicate an active identity.

### 5.6 Revocation and fencing (test-driven evidence path)

Kind 9031 is not part of the provisioner's lifecycle; tests publish it with the disposable
relay-admin key via `build_buzz_admission_event_at(Revoke, pubkey, relay_admin, now)` on the
trusted transport, then assert the fail-closed consequences deterministically:

- a **new** connection + NIP-42 authentication of the revoked key is rejected and a member-only
  NIP-29 read returns **zero events**;
- an **existing** authenticated session is denied on its next member-only request (the expanded
  PR #450 conformance observed `CLOSED_RESTRICTED` / zero events for cross-member reads);
- whether stock Buzz additionally initiates socket closure is **observed and recorded** (the
  upstream harness previously left exact post-revocation socket behavior unverified), without
  modifying upstream to force it;
- a subsequent `drive` of an operation whose admission precedes the revocation cannot become
  `ready` (`membership_unproven`).

## 6. Correctness invariants and their tests

The contract's must-pass list maps to this slice as follows (T = new test, E = existing coverage
extended). Full command lines are in the companion plan.

| # | Requirement | Mechanism | Test |
|---|---|---|---|
| 1 | Trusted authenticated context required | test-only `ProvisioningContextResolver` validates session→tenant/workspace membership, enabled user, `ChatIdentityStore::chat_access` before any `ProvisioningSubject`; `authorize` then binds `mapping_id`/generation from the signed store | T: `wrong_tenant_or_unentitled_authenticated_context_is_rejected` — cross-tenant session, non-member workspace, disabled user, and `chat_access == false` each → `invalid_subject`, no operation, no identity |
| 2 | Concurrency yields one identity | `insert_or_load` idempotency + one-active-operation partial index (incl. `failed`) | T: N concurrent `authorize`/`drive` → one row, one pubkey |
| 3 | Retry preserves pubkey + generation | deterministic pre-allocated `identity_id`/`identity_generation`; reuse records the actual values via `attach_identity` | T: repeated `drive`/`reconcile` → same pubkey+generation, one vault row, operation record holds the actual reused values (no phantom preallocation) |
| 4 | Crash before encrypted storage | durable `authorized` operation; identity absent → regenerate same `identity_id` | T: authorize, simulate pre-insert crash, `reconcile` → ready, exactly one identity |
| 5 | Crash after encrypted storage | `load_active_by_principal` reuse; `attach_identity` CAS records the actual values | T: store identity row out-of-band, `reconcile` → no second key, admission replay same event id |
| 6 | Admission failure never `ready` | 9030 rejection/timeout → `admission_rejected`, identity preserved but unusable | T: wrong relay-admin key → failed, NIP-42/member-only read as the key still denied |
| 7 | Wrong tenant/community/generation rejected | mapping `load` validation + `mapping_id`/`mapping_generation` binding; envelope AAD | T: advance signed mapping generation between `authorize` and `drive` → `stale_mapping_generation`; recover under another tenant → AAD failure |
| 8 | KMS outage fails closed | `VaultError::KeyManagement`/`Recovery` → `kms_unavailable`; no plaintext fallback exists | T: test-only decorator over `LocalDekKeyManagement` with `unavailable` flag → drive fails closed, retryable |
| 9 | NIP-43 replay idempotent at operation layer | deterministic 9030 event id (`created_at` + `command_id` + `expected_revision`) | T: deterministic out-of-band send observed accepted by the relay **before** `record_admission_sent`, then `drive` rebuilds the identical event id and records it — one operation, one identity, `ready`; the relay's duplicate-id response is observed and recorded, not assumed |
| 10 | kind 9031 revokes future admission | §5.6 | T: post-9031 new-session auth denied, zero events, in-flight op `membership_unproven` |
| 11 | Socket close/fencing observed; reconnect fails | transport `SessionFenced` on mapping change; relay denial on revoked key | T: mapping deactivation fences session (existing transport coverage) + §5.6 reconnect assertions |
| 12 | No keys in logs, durable records, errors, or payloads | operation schema has no bytea/secret columns; `error_code` is `CHECK`-constrained; bounded `Debug`; no production error response exists (no route) | T: sentinel scans the captured sanitized test log, **every value of every durable operation row** (all statuses incl. `ready`/`abandoned`), every bounded error string, and every recorded relay payload for the relay-admin `nsec` canary, `nsec1`, and the managed-key secret patterns — only pubkeys appear; envelope/`RecoveredIdentity` `Debug` non-exhaustive (existing) |
| 13 | Cross-tenant key substitution rejected | tenant RLS on both stores, globally unique `public_key`, envelope AAD binding | T: attempt to drive another tenant's operation/identity → reject at store and recover layers (extends existing vault test) |
| 14 | DB outage retryable and unauthorized | bounded attempts; `failed` stays in the index and resumes via `reconcile` | T: `database_outage_is_retryable_and_unauthorized` — stop disposable PostgreSQL mid-drive → retryable `database_unavailable`, never `ready`, NIP-42 never exercised; restore → `reconcile` → ready |
| 15 | Relay outage retryable and unauthorized | same state machine; connect/admission failure → `transport_unavailable`/`admission_rejected` | T: `live_relay_outage_is_retryable_and_unauthorized` — stop unmodified Buzz, drive → `failed`/retryable, never `ready`; restart, `reconcile` → ready |
| 16 | Fresh user NIP-42 + member-only NIP-29 read before `ready` | §5.5 step 7 precedes `mark_ready`; the capability is used only inside the authorized operation after storage+recovery | T: member-only `REQ` with pre-admission key returns zero events (negative control); post-admission returns seeded private-channel events |

## 7. File-level change map

New files:
- `backend/migrations/20261103100000_managed_chat_identity_operations.up.sql`
- `backend/crates/storage/src/managed_identity_operations.rs` (store + operation record/types)
- `backend/server/src/upstream_identity_provisioning.rs` (provisioner + in-crate tests incl. ignored live-Buzz tests)
- `backend/tests/managed_identity_provisioning_store_test.rs` (ignored live-PostgreSQL store invariants)

Modified files:
- `backend/crates/storage/src/lib.rs` — module + re-exports
- `backend/crates/storage/src/managed_identity_vault.rs` — `load_active_by_principal`
- `backend/crates/crypto/Cargo.toml` — `nip42-auth-signing` feature
- `backend/crates/crypto/src/managed_identity_vault.rs` — `sign_nip42_authentication` + `VaultError::AuthenticationSigning` + unit tests
- `backend/server/src/upstream_buzz_transport.rs` — `TenantRelaySession::send_text`
- `backend/server/src/lib.rs` — feature-gated module registration
- `backend/server/Cargo.toml` — `upstream-buzz-runtime` chain gains `rustshare-crypto/nip42-auth-signing`; `[dev-dependencies]` gains `rustshare-crypto` with `local-dek-key-provider` so in-crate tests can construct the test KEK provider.
- `.github/workflows/integration-tests.yml` — the aggregate ignored-test sweep gains `--skip upstream_identity_provisioning::tests::live_` alongside the existing `--skip upstream_buzz_transport::disposable_service_tests`, since the live provisioning suites require the disposable Buzz harness.
- `docs/contracts/managed-chat-identity-v1.md` — status note only (Phase 2B.2 test-only operation persistence; contract remains target)

Nothing else: no routes, no `AppState` wiring, no workers, no frontend, no desktop, no
`rustshare-obsidian-plugin`, no upstream Buzz changes.

## 8. Validation

Per-slice and final commands are enumerated in the companion plan; the gate for this slice is:

```bash
cargo fmt --all --check
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --workspace --all-features --lib
SQLX_OFFLINE=true cargo test -p rustshare-server --features upstream-buzz-runtime --lib
SQLX_OFFLINE=true cargo test -p rustshare-crypto --all-features --lib
# live disposable PostgreSQL (ignored):
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres \
  cargo test -p rustshare-server --features upstream-buzz-runtime --test managed_identity_provisioning_store_test -- --ignored --test-threads=1
# live disposable unmodified Buzz (ignored, via the disposable harness):
cargo test -p rustshare-server --features upstream-buzz-runtime --lib upstream_identity_provisioning::tests -- --ignored --test-threads=1
```

The aggregate CI ignored-test sweep (`.github/workflows/integration-tests.yml`) must carry both
module skips so the disposable-Buzz-only suites never run without their harness:
`--skip upstream_buzz_transport::disposable_service_tests` (existing) and
`--skip upstream_identity_provisioning::tests::live_` (this slice). Verify with
`rg -n -- '--skip upstream_buzz_transport::disposable_service_tests|--skip upstream_identity_provisioning::tests::live_' .github/workflows/integration-tests.yml`.

## 9. Assumptions and open questions

- **Authoritative caller of admission:** the disposable harness's relay owner/admin key acts as
  the relay-admin identity for kind 9030/9031. Stock Buzz accepts owner-signed NIP-43 membership
  events (recorded as passing in the upstream protocol seed); if a given harness revision rejects
  the conditional (`elembra-op`/`elembra-expected-revision` tagged) form, the plain
  `build_buzz_admission_event_at` form is the fallback — both already exist and either satisfies
  "separate relay-admin NIP-43 kind 9030". The pinned filter for the member-only read (exact
  kind/tag shape) is fixed during TDD against the unmodified relay; the design only requires that
  it be member-gated and empty for non-members.
- **Post-9031 socket closure** is upstream behavior this slice observes, not controls; the
  deterministic assertions are the denial of new authentication and zero-event reads (§5.6).
- **Attempt bound** defaults to 5 and is a column, not a constant, so outage tests can raise it
  per operation without code changes. Attempt exhaustion is conservative: the exhausted `failed`
  row stays in the one-active-operation index and blocks re-authorization (no duplicate identity
  is possible), and the only escapes are raising `max_attempts` or the explicit test-called
  `abandon`, which first disables any attached never-`ready` identity.
- The contract's proposed operations table name (`chat_managed_identity_operations`) and status
  ladder are adjusted to the merged schema (`managed_chat_identity_operations`,
  operation-level states over vault `active`/`disabled`/`revoked`); the contract itself requires
  names/shapes to be reconciled with the implementation.
- Gate I remains **NOT ACCEPTED**; this slice is implemented as a draft PR with DCO-signed commits
  and requires the same independent review discipline as the foundation PRs before any merge
  decision.
