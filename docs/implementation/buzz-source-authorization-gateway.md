# Buzz Source Authorization & Reconciliation Gateway — Implementation Notes

Status: Implemented v1alpha1  
Date: 2026-08-11  
Related: ADR-0033 (Elembra Memory Architecture), ADR-0034 (Elembra Chat and Buzz Boundary),
`docs/specs/buzz-upstream-authorization-v1alpha1.md` (upstream contract — the source of truth
for the relay side),
`docs/superpowers/specs/2026-08-11-buzz-source-authorization-gateway-design.md` (design spec),
`backend/tests/buzz_authority_gateway_test.rs` (acceptance suite).

This document records how the Buzz source-authorization gateway was implemented: how the
authorization gate asks the community's authoritative Buzz relay whether a bound principal may
read a channel/message *now*, and how the admin reconcile path repairs the observation index and
the Memory projection from the relay's signed state over the public HTTP contract. Buzz remains
the final authority for channel visibility, membership, and message availability; Elembra
creates no second ACL system and never touches Buzz's private database.

## 1. What was built

### Contract crate (`backend/crates/resource-auth`)

- `buzz_authority.rs` — the `BuzzAuthority` trait (`can_read(&BuzzReadRequest) ->
  Result<BuzzReadDecision, BuzzAuthorityError>`), the request shape
  (`tenant_id`, `community_id`, `relay_url`, `relay_pubkey`, `channel_id`, `channel_kind`,
  `message_id`, `pubkey`, `event_created_at`), the decision surface
  (`Allow` / `Deny` / `NotFound`), the fail-closed error enum (`Transport` / `Unauthorized` /
  `InvalidResponse` / `Config`), and `LocalFallbackAuthority` (workspace-channels-only coarse
  gate for deployments with no upstream authority).
- `chat_identity.rs` — `WorkspaceCommunityMapping.relay_pubkey: Option<String>` (the pinned
  relay public key a mapping trusts; migration `20260810000006` adds the column with a
  `^[0-9a-f]{64}$` CHECK).

### Gateway client (`backend/server/src/buzz_gateway.rs`)

- `BuzzGatewayClient` — NIP-98-authenticated HTTP client: every request carries an
  `Authorization: Nostr <base64(kind-27235)>` header signed with the workload's **service key**
  (STANDARD base64, round-trips with `nostr::nips::nip98::verify_auth_header`; the `u` tag is
  the exact request URL, `method` the HTTP method, and `payload` the hex sha256 of the body for
  POSTs). Redirects are disabled (SSRF protection).
- `check_access(relay_url, relay_pubkey, &BuzzAccessCheckRequest) -> Result<BuzzReadDecision, …>`
  — `POST /api/v1/relay/access/check`. Every response must be a raw signed kind-19030 event:
  kind 19030, Schnorr-verified, pubkey == the **pinned** `relay_pubkey`, content echoes the
  request's `pubkey`/`channel_id`/`message_id` verbatim, and `evaluated_at` within 60 seconds of
  the client clock. Any failure (transport, 401, 5xx, signature/kind/pin mismatch, echo
  mismatch, stale, unparseable, oversized body) fails closed.
- `page_state(relay_url, relay_pubkey, since, limit, cursor) -> Result<BuzzStatePage, …>` —
  `GET /api/v1/relay/state/events`, same envelope verification, then parsed as
  `BuzzStatePage { entries: Vec<BuzzStateEntry>, cursor, complete }`. An incomplete page without
  a continuation cursor is malformed and fails closed. Page size is clamped client-side to
  `1..=500`; the response body is capped at 8 MiB.
- `BuzzStateEntry { event, context }` / `BuzzStateContext { community_id, channel_id,
  channel_kind, thread_root_id, message_id, event_type, supersedes_event_id }` — the context is
  field-for-field the webhook `BuzzPushContext`, so reconcile reuses its existing validation
  unchanged. Entry events are NOT verified by the paging client — the reconcile consumer
  verifies each kind-1 event.
- `BuzzGatewayAuthority(pub Arc<BuzzGatewayClient>)` — the orphan-safe adapter presenting the
  `AppState`-shared `Arc<BuzzGatewayClient>` as a `BuzzAuthority`.

### Authorization gate (`backend/server/src/authz/chat_owner.rs`)

- `ChatResourceOwner::with_authority(chat_identity, observations, Box<dyn BuzzAuthority>)`
  (and `new()` = local fallback). The gate now ends with the configured authority's
  channel/message decision after the local pre-filter; `Allow` continues, `Deny` → `Deny`,
  `NotFound` → `NotFound`, authority error → `Deny` (fail closed).
- `AppState.buzz_gateway: Option<Arc<BuzzGatewayClient>>` — configured when
  `RUSTSHARE_CHAT_AUTHORITY=buzz`.

### Reconcile-from-Buzz (`backend/server/src/handlers/memory_reconcile.rs`)

- `reconcile_chat_memory_from_buzz_for_tenant` (`pub`, exported for the acceptance suite) —
  pages the mapping's relay signed state, re-verifies each event and upserts the observation
  index via `BuzzObservationService::ingest_without_outbox` (validate + signature-verify +
  observation upsert, NO outbox), then folds the Memory catalog from the repaired index.
- `POST /api/v1/admin/applications/memory/chat/reconcile` with body
  `{ tenant_id, since?, source: "observation"|"buzz" }` — `"buzz"` requires the gateway
  configured (else 503; no silent fallback to the observation index).

### Admin mapping relay-pin rotation (`backend/server/src/handlers/chat_identity.rs`)

- `PATCH /api/v1/admin/applications/chat/workspaces/{workspace_id}/community` with body
  `UpdateCommunityMappingRequest { relay_url, relay_pubkey? }` — admin/tenant-scoped
  rotation of the mapping's relay endpoint and/or pinned pubkey WITHOUT changing
  `community_id`/`workspace_id` (which would orphan admissions). Both fields are always
  written: pass the current value for the side you are not rotating. Omitting
  `relay_pubkey` **unpins** the mapping, which fails closed in buzz mode (an unpinned
  mapping cannot use the upstream capability) — the safe direction when a pin is no
  longer trusted. 404 when the mapping is missing; 400 on an invalid `relay_url` /
  malformed `relay_pubkey`; 403 cross-tenant. Backed by
  `ChatIdentityStore::update_mapping_relay` (a single UPDATE of `relay_url` +
  `relay_pubkey`; returns whether exactly one mapping row matched). Needed because the
  community mapping was previously insert-only: without this, a relay signing-key
  rotation would brick buzz-mode reads with no supported remediation.

### Tests

- `backend/tests/buzz_authority_gateway_test.rs` — the 14-requirement + 4-negative acceptance
  suite (see §6 and the design spec §6) against an in-test fake Buzz relay, plus the admin
  relay-pin rotation test and handler-level tests for the mapping update endpoint (24 tests
  total; see §6).

## 2. Architecture

- **The relay is the final authority.** Elembra never re-derives channel membership or message
  availability. The gate asks the community's relay whether the bound pubkey may read the
  channel/message *now*, and the relay's signed answer is final.
- **Two upstream capabilities** (defined in the v1alpha1 spec):
  - `POST /api/v1/relay/access/check` — current access decision for a pubkey/channel/message;
  - `GET /api/v1/relay/state/events` — paged signed event state for reconciliation.
- **NIP-98 service authentication.** Requests are signed with the workload's Nostr *service*
  key — a human user's key is never held server-side. The relay is configured with the service
  public key and rejects any other signer (401).
- **Signed, pinned, echoed, fresh responses.** Every response is a raw kind-19030 event signed
  by the relay's key; the client verifies kind + Schnorr signature + pinned `relay_pubkey`,
  checks the content echoes the request verbatim, and requires `evaluated_at` within 60 seconds.
  Any mismatch is an invalid response → Deny.
- **Host-derived community isolation.** The HTTP base is derived from the mapping's `relay_url`
  (`ws://`→`http://`, `wss://`→`https://`, host+port unchanged); a community's checks never
  route to another community's relay host.

## 3. The gate flow

The final decision for `authorize`/`resolve`/`fetch`/`materialize` is the ordered composition of:

1. **Shape checks** — the ref must be the `io.elembra.chat` `message` type with a 64-hex message
   id; anything else is `Invalid`/`NotFound` (malformed refs look absent).
2. **Observation lookup** — the message's current observation row (tenant-scoped
   `lookup_for_auth`). No row → `NotFound` (existence-hiding). A `deleted`/inactive row → not
   found. The authorizer-level tombstone override additionally refuses any message with a
   Deleted observation at-or-after the candidate row (a later-pushed edit can never resurrect).
3. **Local pre-filter (coarse, never a final allow)** — tenant Chat enablement + per-user
   preference, the principal's current active binding, an active admission for the message's
   community and bound pubkey, and the mapping active. Any of these failing (or erroring) fails
   closed to `Deny`/`NotFound`.
4. **FINAL channel/message decision from the configured `BuzzAuthority`** — the gate builds a
   `BuzzReadRequest` from the mapping (`relay_url`, `relay_pubkey`) and the observation row
   (`channel_id`, `channel_kind`, `event_created_at`) and asks the authority. `Allow` continues;
   `Deny` → `Deny`; `NotFound` → `NotFound`; any authority error logs and fails closed to `Deny`.

Existence-hiding: `resolve`/`fetch`/`materialize` map every non-Allow outcome to
`SourceError::NotFound` (or `VersionUnavailable` when the body was never captured), so callers
cannot distinguish "never existed", "deleted", "not a member", and "relay unreachable". The
typed `Deny`/`NotFound` distinction is kept only on `authorize` for internal callers.

## 4. Reconcile-from-Buzz flow

`reconcile_chat_memory_from_buzz_for_tenant` is the admin repair path over the public HTTP
contract (never Buzz's private DB):

1. **Fail closed on the mapping** — the tenant's mapping must exist, be active, have a
   non-empty `relay_url`, and carry a pinned `relay_pubkey`; otherwise the repair aborts before
   any write.
2. **Page loop** — `page_state` with the `since` watermark floored to whole seconds (stored
   `event_created_at` values are whole-second), page size 200, opaque cursor. The stream
   terminates on `complete`; a relay that never terminates aborts at 10_000 pages (2M entries
   cap).
3. **Per-entry processing** — every entry is counted as `processed`. An unknown `event_type` on
   the wire skips the entry (logged). **Tenant-scope guard**: an entry whose
   `context.community_id` differs from the mapping's community is skipped (logged) — a shared
   relay serving several communities must not write into another tenant's observation index
   during this tenant's repair. Otherwise the entry is converted to a `BuzzEventPush` and
   ingested with `ingest_without_outbox`: validate context, verify the kind-1 event (id +
   Schnorr), map community → tenant and author → active binding, apply the body gate, and upsert
   the observation row in one transaction — **no durable envelope, no outbox insert**.
4. **Entry-level resilience** — one poisoned entry (invalid context, unverifiable signature,
   unbound author) is skipped and logged loudly; it must not abort the repair.
5. **Fold** — the (now repaired) observation index is re-projected with the existing pure
   `rebuild_records` fold into `memory_catalog` (idempotent `upsert_records`).
6. **Idempotency** — observation upserts are keyed on `(tenant_id, event_id)` with
   `ON CONFLICT DO NOTHING`; re-running with no drift creates nothing (`created = 0`; existing rows
   count as `updated`).

**No outbox writes, no receipts.** The repair path deliberately bypasses the durable pipeline:
consumer receipts and delivery ledgers are untouched, and the outbox gains no rows, so a
reconcile can never re-trigger projection work.

## 5. Edit / delete / retention semantics

- **Edits fold into one record.** An `edited` event for a message is a new observation row
  (per-event id) that updates the same `memory_catalog` record (latest-event fields + appended
  provenance) — exactly the projection semantics of the observation path; reconcile applies the
  same fold, order-protected by the out-of-order guard and never un-tombstoning.
- **Tombstone immutability at the fold.** `upsert_records` never re-writes a tombstoned
  conflict row: the `DO UPDATE` is guarded by
  `WHERE memory_catalog.indexing_status <> 'tombstoned'`, so a backdated relay delete
  (excluded by a reconcile `since` window) cannot re-flip a tombstoned Memory record back to
  `created`/`content_stored`. A skipped conflict row returns no row and counts as neither
  created nor updated.
- **Tombstones → NotFound end-to-end.** A `deleted` message is `not_found` at the relay (the
  fake's `mark_deleted`), which the client maps to `NotFound`; the observation-side tombstone
  rules (`active = false`, authorizer-level tombstone override) keep it NotFound at the gate
  regardless. The projected record stays (provenance survives; erasure is retention's job).
- **Retention metadata is recorded, not enforced** — unchanged from the memory projection (see
  `docs/implementation/buzz-memory-projection.md` §2.5); reconcile preserves those fields.

## 6. Negative-test summary

The suite (`backend/tests/buzz_authority_gateway_test.rs`, 24 tests) proves the fail-closed
surface. 19 are DB-only: the 18 acceptance tests against the in-test fake Buzz relay (the 14
requirements + N1–N4, see the design spec §6) plus the admin pin-rotation test
`admin_can_rotate_mapping_relay_pin`. The other 5 are handler-level tests for the
`update_community_mapping` endpoint (happy path, missing mapping → 404, bad `relay_url` →
400, bad `relay_pubkey` → 400, cross-tenant → 403); they additionally require S3 (RustFS)
because the handler runs against the full `DatabaseState` (DATABASE_URL is required by all
24).

- non-member → `Deny`; fetch is existence-hiding `NotFound` (also for private channels);
- membership removal at the relay is visible on the very next decision;
- a DM participant is `Allow` (metadata only; body fetch is `VersionUnavailable`), an outsider
  is denied;
- an active Elembra admission cannot bypass the relay's channel decision;
- stale Memory/observation state cannot grant access;
- cross-tenant refs → `NotFound`; a wrong `relay_pubkey` pin → `InvalidResponse` → `Deny`;
- an unreachable relay → `Deny` / `NotFound`;
- the production path signs only with the service key (the relay records the authenticated
  signer); a forged user-key-signed request → 401; `chat_identity_bindings` has no
  private-key column;
- deleted messages → `NotFound`; binding rotation asks the relay for the new pubkey; unknown
  channels → `NotFound`;
- reconcile-from-Buzz repairs a missing projection (no outbox writes), repairs a missing
  observation index via replay, is idempotent, and flows over the public HTTP contract (the
  fake has no database by construction); the tenant-scope guard skips foreign-community entries.
- the admin relay-pin rotation endpoint pins the mapping to the new key and fails closed
  while the relay still signs with the old key (stale pin → Deny), then allows again after
  the relay rotates to the new key;
- `update_community_mapping` writes both `relay_url` and `relay_pubkey`, 404s on a missing
  mapping, 400s on an invalid `relay_url` / malformed `relay_pubkey`, and 403s cross-tenant.

## 7. Configuration

- `RUSTSHARE_CHAT_AUTHORITY` — `local` (default; coarse local fallback gate) or `buzz`
  (upstream gateway). `buzz` requires `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` to be a valid Nostr
  secret key (64 lowercase hex); otherwise bootstrap fails closed — a silent fallback to local
  would be a wrong-authorization bug.
- `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` — the workload's Nostr service key; every NIP-98 request
  is signed with it.
- Per-community mapping — `relay_url` (ws/wss; the HTTP base is derived) and
  `relay_pubkey` (optional pinned relay key; a mapping without it cannot use the upstream
  capability and fails closed). The pin is rotated by admins via
  `PATCH /api/v1/admin/applications/chat/workspaces/{workspace_id}/community` (both fields
  always written; omitting `relay_pubkey` unpins and fails closed).

## 8. Open risks

- **The relay capability is proposed, not yet live in Buzz.** The v1alpha1 spec defines the
  endpoints so the Buzz repository can implement them independently; until then every
  deployment runs `local`. The acceptance suite's fake relay is the contract-faithful reference
  implementation the real relay can be validated against.
- **`wss://` enforcement is operational.** Production MUST use `wss://` relay URLs (so the
  derived base is `https://`); plaintext is acceptable only for local development/testing.
  A captured signed `allow` replayed within the 60s freshness window against the same
  (pubkey, channel, message) query can pass on plaintext transport; production `wss://` is
  what prevents capture, and the echo binding prevents cross-resource replay. Plaintext
  also exposes the traffic to observation.
- **Freshness window.** The 60-second `evaluated_at` window assumes loosely synchronized clocks
  between the workload and the relay; a relay that cannot serve within the window will be
  treated as failing closed (a denial, never an error-open).
- **Reconcile volume.** A full re-reconcile pages the relay's whole community state; the
  `since` watermark bounds the window in practice, and the page/page-entry caps bound a
  misbehaving relay. Reconcile is an admin repair path, not the steady-state ingestion path.
- **Reconcile-entry author binding.** An entry whose author's binding was revoked is skipped
  (unbound author) — the relay's state is authoritative for what happened, but Elembra only
  records authors it can map to a current active binding.
- **Batch/latency limitation.** BUZZ mode performs one sequential relay access-check
  round-trip per message: `authorize_batch` over 64 candidates (`MAX_BATCH_SIZE`) is a `for`
  loop of single `authorize` calls, each with the client's 10s timeout — up to 640s worst
  case with a degraded relay before the batch fails closed. Current consumers (Chat
  attachments) use single-`authorize` and are unaffected; future RAG/search consumers of
  `materialize`/`authorize_batch` should add bounded concurrency client-side or a batch
  access-check endpoint to the v1alpha1 spec before production scale.
