# Buzz → Elembra Memory Projection — Implementation Notes

Status: Implemented (commits `63e952f3..9d5e3060` on `codex/buzz-memory-projection`)  
Date: 2026-08-11  
Related: ADR-0033 (Elembra Memory Architecture), ADR-0034 (Elembra Chat and Buzz Boundary),
`docs/superpowers/specs/2026-08-11-buzz-memory-projection-design.md` (design spec),
`backend/tests/buzz_memory_projection_e2e_test.rs` (acceptance suite).

This document records how the Buzz → Elembra Memory projection was implemented,
defines the behavior of the edge cases the code must get right (and why), and
captures the failure and security semantics. Buzz remains authoritative for
messages, channels, and membership; Elembra never becomes a second Chat
database.

## 1. What was built

### Migrations

- `backend/migrations/20260810000004_create_chat_observation_and_memory_catalog.{up,down}.sql` —
  creates the bridge-owned observation index `chat_observed_events` (PK
  `(tenant_id, event_id)`, reference-first `body`, `active` flag) and the
  Memory-owned catalog `memory_catalog` (unique
  `(tenant_id, source_application, source_type, message_id)`, append-only
  `provenance` JSONB, `indexing_status`, `tombstoned_at`).
- `backend/migrations/20260810000005_unique_active_community_mapping.{up,down}.sql` —
  partial unique index `chat_workspace_communities_active_community` on
  `chat_workspace_communities (community_id) WHERE active`, making the
  cross-tenant ambiguity in `mapping_by_community` unrepresentable going
  forward (the code still defends against legacy duplicates).

### `rustshare-memory` crate (`backend/crates/memory`)

- `event.rs` — `ObservedChatEventData` payload of the durable event, with
  `BuzzEventMeta` (signed-event identity/provenance), `ChatContext`,
  `PrincipalMeta`, `ObservedEventType` (`created`/`edited`/`deleted`),
  `ChatChannelKind` (`workspace`/`dm`/`private`/`excluded`), and fail-closed
  `ObservedChatEventData::validate`.
- `observed.rs` — `ChatObservedEvent`, the `chat_observed_events` row shape;
  `active = event_type != Deleted`.
- `record.rs` — `MemoryCatalogRecord` (the `memory_catalog` row) +
  `ProvenanceEntry` + `IndexingStatus`; canonical constants (`SOURCE_APPLICATION`,
  `SOURCE_TYPE_MESSAGE`, `AUTHORIZATION_SOURCE_BUZZ`, `DEFAULT_CLASSIFICATION`),
  `source_ref_for` (`elembra://io.elembra.chat/message/<message_id>`),
  `authorization_ref_for` (`community:<id>:pubkey:<pubkey>`).
- `policy.rs` — `ProjectionPolicy` (`memory_projection` / `content_indexing`,
  defaults OFF, fail closed on absent/non-boolean config) + `decision(channel_kind)`
  hard gate (only `workspace` projects).
- `project.rs` — pure, deterministic projection functions `project_record`,
  `apply_event`, `apply_tombstone`, and the reconciliation rebuild
  `rebuild_records` (groups rows by `message_id`, applies the out-of-order
  guard, never un-tombstones, keeps tombstoned records).

### Storage stores (`backend/crates/storage`)

- `chat_observation.rs` — new `ChatObservationStore`: idempotent
  `upsert_event_in_tx` (PK conflict ⇒ already observed, never rewrite),
  `lookup_for_auth` (latest event per message, for the authorization owner),
  `get_by_event_id` (body fetch for the consumer), `get_by_message_id`,
  `list_for_reconcile`.
- `memory_catalog.rs` — new `MemoryCatalogStore`:
  `upsert_from_event_in_tx` (consumer receipt + policy gate + load-or-project
  atomically in one tx), `upsert_records` (reconciliation upsert with
  create/update counts), `get`, `count_for_tenant`, and `ReconcileCounts`.
- `chat_identity.rs` — additions to the existing `ChatIdentityStore`:
  `projection_policy` (reads `application_enablements.configuration`),
  `mapping_by_community` (+ `CommunityMappingError::Ambiguous`),
  `chat_access` (tenant enablement AND per-user preference), `active_binding`,
  `binding_by_pubkey`, `active_admission` (admission AND mapping both active),
  and `revoke_principal` (queues `io.elembra.chat.buzz.admission.revoked.v1`).

### Server (`backend/server`)

- `buzz_observation.rs` — `BuzzObservationService::verify_and_ingest` (the
  observation half): HMAC + replay window, context sanity, Nostr id/Schnorr
  verification (kind 1), mapping + binding, body gate, and the single
  transaction that upserts the observation and publishes the durable event on
  first observation. `BuzzEventPush` / `BuzzPushContext` / `BuzzPushError` /
  `IngestOutcome`; `build_envelope` (deterministic UUIDv5 id, `time` = Buzz
  `created_at`).
- `memory_projection.rs` — `MemoryChatProjectionConsumer` (id
  `io.elembra.memory.chat-projection.v1`): validates envelope/payload (poison
  → dead-letter), reads policy before the receipt gate, fetches the indexing
  copy, then `upsert_from_event_in_tx`.
- `authz/chat_owner.rs` — `ChatResourceOwner`, the source-authorization adapter
  for `io.elembra.chat` `message` refs (`chat.read` only), current-state-only
  gate (§2.3).
- `handlers/buzz_events.rs` — `POST /api/v1/integrations/buzz/events`
  (`receive_buzz_event`): maps outcomes/errors to
  202/400/401/403/409/500 in the standard `{error, details}` shape.
- `handlers/memory_reconcile.rs` — `POST /api/v1/admin/applications/memory/chat/reconcile`
  (`reconcile_chat_memory`): admin tenant scoping, optional `since` watermark,
  `reconcile_chat_memory_for_tenant` (exported for the e2e suite).
- `routes.rs` / `state.rs` / `bootstrap.rs` — `buzz_observation_routes()`, the
  admin reconcile route, `AppState` fields (`chat_observation_store`,
  `memory_catalog_store`, `buzz_observation_service`), and bootstrap wiring
  incl. always-registered consumer and the `WebhookSigner` built from
  `RUSTSHARE_CHAT_WEBHOOK_SECRET` with
  `RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS` (default 300 s) replay window.

### Tests

- `backend/tests/buzz_memory_projection_e2e_test.rs` — the 14-requirement
  acceptance suite (see the design spec §11).
- Unit tests inside every module above (payload validation, policy gates,
  projection edge cases, context rules, deterministic id, consumer
  fail-closed paths, manifest-surface match).

## 2. Semantics analysis

Each bullet states the defined behavior and why.

### 2.1 Edited / replaced Buzz events

**Defined behavior.** An edit is a new signed Nostr event for the same message
(`message_id` unchanged, `supersedes_event_id` naming the root or a prior
event). The observation records it as a new `chat_observed_events` row (PK is
per event id). The consumer updates the **same** `memory_catalog` record:
`latest_event_id`, `checksum`, `signature`, `signature_verified`, `event_type`,
`occurred_at`, `observed_at`, and (per the content rule) `content` /
`content_indexing` / `indexing_status` are refreshed, and a
`ProvenanceEntry { event_id, event_type, occurred_at, observed_at }` is
appended. Identity fields (record id, tenant/workspace, source ref, message id,
community/channel, author, classification, retention/legal-hold refs,
authorization ref, `created_at`, `tombstoned_at`) are preserved.

**Out-of-order delivery guard.** `apply_event` applies an edit only when the
new event's `created_at >=` the record's `occurred_at`; an older event delivered
late is ignored (record unchanged, no provenance entry). Equal timestamps apply
(the first of the two to arrive wins the fold but both are provenance).

**Why.** Buzz event identity is content-addressed, so redeliveries and re-orders
are normal; the guard makes projection order-independent against time and
prevents a late older edit from regressing a newer one. Provenance stays
append-only so the record's history reflects everything observed, in
observation order.

### 2.2 Deleted / tombstoned messages

**Defined behavior.** A deletion is a signed `deleted` event for the message:
observation inserts a row with `active = false`; the consumer applies
`apply_tombstone`, which sets `event_type = deleted`, `indexing_status =
tombstoned`, `tombstoned_at = observed_at`, refreshes the latest-event fields,
and appends a provenance entry. The record is **kept** (provenance must
survive; body copies are retained for retention/legal-hold policy, which owns
erasure — not the projection). A tombstone for a message that was never
projected (no record) is a no-op: the durable fact of the deletion already
lives in the observation index, and the receipt records that the event was
processed with no effect. Reconciliation (`rebuild_records`) applies the same
rule and returns tombstoned records.

**Never un-tombstone.** `apply_event` refuses to modify a record whose
`event_type` is `Deleted` or whose `indexing_status` is `Tombstoned` (the
`event_type` check is defense in depth for the inconsistent state where a
record is `Deleted` but not yet `Tombstoned`). A later edit arriving after a
tombstone cannot resurrect the record. `apply_tombstone` itself always applies
(a tombstone supersedes any newer-looking edit; a second tombstone re-applies
and grows provenance).

**Authorization.** `ChatResourceOwner` treats a row with `active = false` or
`event_type = deleted` as not-found, so a deleted message is indistinguishable
from one that never existed (existence-hiding). Tombstone behavior follows Buzz
semantics: deleted ⇒ tombstoned ⇒ not-found, irreversibly.

**Why.** Deletions are user-intent statements that must win over edits that
were signed but never observed (or observed out of order); keeping the record
preserves audit/provenance, while authorization time is the enforcement point
for "gone".

### 2.3 Membership revocation

**Defined behavior.** Admission/binding revocation is **immediate and
fail-closed** at the `ChatResourceOwner`: every decision re-reads current state
— Chat enablement (`application_enablements.enabled` AND per-user preference),
the principal's current active binding (`status = 'active'` AND `revoked_at IS
NULL`), and a currently-active admission for the message's community and bound
pubkey (admission row AND community mapping both `active`). Revocations are
pushed to Buzz through the existing bridge: `revoke_principal` and the rotation
path of `consume_and_activate` publish
`io.elembra.chat.buzz.admission.revoked.v1` into the outbox, and the
user-disable trigger revokes local binding/admission state and queues Buzz
revocation (ADR-0034). The Memory worker never touches admission state, and
`ChatResourceOwner` never reads `memory_catalog` — so stale Memory records
(restored from a backup, drifted, tampered) can never re-grant access.

**Why.** Cryptographic validity is not authorization (ADR-0034); a revoked
binding must stop exposure even though old signed events remain valid in Buzz
history. Because authorization is derived only from current state, the
projection's eventual consistency never weakens it.

### 2.4 Channel access changes

**Defined behavior.** Channel-level membership is Buzz-owned; Elembra only
records `channel_id`/`channel_kind` as observation metadata. The Elembra-side
gate is deliberately coarse: `channel_kind == workspace` (dm/private/excluded
are never candidate-exposable) plus the community-level admission. Anything not
positively confirmable fails closed to `Deny`.

**Documented limitation.** Elembra does not verify per-channel membership
(e.g. "can this member read this specific channel inside the community") —
that requires a future Buzz API/relay adapter and is out of scope here. A
message in a community the viewer is admitted to is candidate-exposable even if
the specific channel's membership changed on the Buzz side; the channel-level
adapter is the follow-up that closes that gap.

**Why.** The community admission is the trust boundary Elembra owns; duplicating
Buzz's channel ACL model inside Elembra would create a second authority and
reintroduce the exact coupling ADR-0034 forbids. Coarse-and-fail-closed is the
safe interim.

### 2.5 Retention changes

**Defined behavior.** Retention, classification, and legal-hold are recorded
**metadata on the record** (`classification` default `general`,
`retention_policy_ref`, `legal_hold_ref`), not enforcement. Changing a tenant's
retention configuration does not delete catalog rows; `apply_event` /
`apply_tombstone` / `rebuild_records` preserve these fields. Provenance outlives
indexes: the catalog row is the durable record and is never the target of
content erasure.

**Documented.** Future retention enforcement (body/copy deletion) targets
indexes — the `content`/indexing copies — never the catalog record (matches
ADR-0033's "Memory catalog/provenance records" ownership and the reference-first
posture).

**Why.** The catalog is a provenance ledger; deleting rows to satisfy retention
would destroy the audit trail the projection exists to provide. Retention is a
content-lifecycle policy and belongs at the indexing-copy layer.

## 3. Failure semantics

- **At-least-once delivery.** Observation publishes into the transactional
  outbox (ADR-0031); the dispatcher claims → processes → acks with lease
  fencing, retry backoff, and dead-lettering. Deliveries are never lost for an
  offline or disabled consumer: obligations are created at publish time
  regardless of the consumer's `enabled` flag.
- **Consumer receipts.** `MemoryCatalogStore::upsert_from_event_in_tx` writes
  the durable receipt (`integration_consumer_receipts`, unique
  `(consumer_id, source, event_id)`, `ON CONFLICT DO NOTHING`) and the business
  effect in **one** consumer-local transaction; `rows_affected() == 1` gates the
  effect, so a duplicate delivery is a no-op and a rollback undoes both. The
  receipt is written on first processing even when policy skips the event (that
  event will never produce a record).
- **Retry / dead-letter via the dispatcher.** `ConsumerOutcome::Retryable`
  covers transient failures (tx begin, policy read — which happens **before**
  the receipt gate so a failed read never consumes the event — body lookup,
  persist, commit). `ConsumerOutcome::Permanent` dead-letters poison envelopes
  (invalid envelope, wrong event type, malformed/unverified payload) — never
  retried.
- **Offline-worker recovery.** Disabled worker ⇒ events stay `pending`; a
  dispatch pass must not claim them. Re-enabling drains pending deliveries and
  projects one record per message (e2e requirement #7).
- **Memory-outage independence.** The ingest path never touches `memory_catalog`
  or any Memory state; the e2e suite proves it by holding an uncommitted
  `DROP TABLE memory_catalog` (which takes an ACCESS EXCLUSIVE lock) on one
  connection while pushing from another — ingest succeeds and commits during
  the "outage", then the DROP rolls back and the table returns. If a future
  change made ingest touch the catalog, the push would block on the DROP lock
  and the 30 s test timeout converts that regression into a clean failure.
- **Duplicate-observation idempotency.** Observation upsert is keyed on
  `(tenant_id, event_id)` with `ON CONFLICT DO NOTHING`; the durable event is
  published only when a row was actually inserted, so a duplicate push returns
  `DuplicateObservation` with nothing written — and the deterministic event id
  (UUIDv5 of the Buzz event id) means the durable event itself is stable across
  retries.

## 4. Security notes

- **HMAC trust scope.** `X-RustShare-Signature` (timestamped
  `t=<ts>,v1=<hex>`) authenticates the **delivery** of the push (Buzz → bridge)
  and bounds replay; it says nothing about message authenticity. Only the
  timestamped form is accepted — a plain `v1=` signature cannot have a replay
  window enforced and fails closed as unauthorized.
- **Schnorr verification (message authenticity).** The Nostr id (sha256 of the
  canonical NIP-01 serialization) and the Schnorr signature are verified before
  anything is written; kind must be 1; the raw JSON `id` must equal the parsed,
  verified id. Tampered or unsigned events are rejected with nothing written.
- **No private Buzz DB access.** The bridge stores only public metadata
  (bindings, admissions, mappings) and the observation index; it never reads
  Buzz tables and never stores a Buzz private key. The projection reads only
  `chat_observed_events` — the bridge's already-verified state (structural
  requirement #14, asserted by the adversarial architecture review).
- **No shared DB / no distributed transactions.** No shared Buzz/Elembra
  schema (ADR-0034); observation and projection each run in a single local
  transaction; the outbox makes the cross-boundary handoff durable.
- **Existence-hiding errors.** Unknown and tombstoned messages resolve to
  `NotFound`; every non-Allow outcome on `resolve`/`fetch` surfaces as
  `SourceError::NotFound`, so callers cannot distinguish "never existed" from
  "not authorized". `authorize` keeps the typed `Deny`/`NotFound` distinction
  for internal callers.
- **Cross-tenant fail-closed.** Observation maps `community_id` to exactly one
  active workspace and enforces `workspace == tenant`; the partial unique index
  `chat_workspace_communities_active_community` makes a second active mapping
  unrepresentable, and the code still fails closed on legacy duplicates
  (`AmbiguousCommunity` → 409, distinct from a persistence 500). Authorization
  is tenant-scoped throughout (`lookup_for_auth` is keyed on tenant + message).
- **Delegation limitation.** `ChatResourceOwner` does not apply
  `PrincipalContext::effective_user_authority`: chat bindings exist only for
  user principals, so Service/Agent principals and delegated requests fail
  closed to `Deny` today. The delegation wiring lands with the first delegated
  consumer (Memory/RAG `materialize` or a transport adapter) and is documented
  in the owner adapter.
