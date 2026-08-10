# Buzz → Elembra Memory Projection — Design

> **Status:** Implemented (shipped on `codex/buzz-memory-projection`, commits `63e952f3..9d5e3060`).  
> **Date:** 2026-08-11  
> **Related:** ADR-0033 (Elembra Memory Architecture), ADR-0034 (Elembra Chat and Buzz Boundary),
> `docs/implementation/buzz-memory-projection.md` (implementation notes + semantics analysis),
> `backend/tests/buzz_memory_projection_e2e_test.rs` (acceptance suite).

## 1. Goal

**Elembra can remember eligible Buzz conversations while Buzz remains authoritative and current Chat authorization is always required before Chat content can be exposed.**

"Remember" means durable, reference-first Memory catalog records (metadata + provenance + optional indexing copy) for signed Buzz messages observed by the Elembra Chat Bridge. "Eligible" means a `workspace`-channel message in a tenant with the `memory_projection` policy enabled. "Authoritative" means Buzz stays the source of truth for messages, channels, and membership. "Current Chat authorization always required" means the authorization path (`ChatResourceOwner`) derives every decision from live Chat/Buzz state at request time and never trusts Memory state.

## 2. Architecture decisions

The ten decisions from the plan, as implemented:

1. **Push-webhook observation.** Buzz pushes signed chat events to `POST /api/v1/integrations/buzz/events`; the bridge authenticates the delivery with an HMAC (`X-RustShare-Signature`, timestamped `t=<ts>,v1=<hex>` form) and enforces a replay window. No pull from Buzz, no Buzz database access.

2. **Bridge-owned verification and mapping.** `BuzzObservationService` verifies the Nostr event id and Schnorr signature, requires kind 1 (`TextNote`), maps `community_id` → Workspace (which must equal the tenant), and maps the author pubkey → Principal (active binding only). Everything fail-closed.

3. **Reference-first default with policy-gated content indexing copy.** The durable event and the default catalog record carry no message body. A body is stored only when the tenant's `content_indexing` flag is on: captured at observation time (in `chat_observed_events.body`) and copied to the record at projection time. The body is never part of the durable envelope.

4. **Deterministic durable event identity.** The outbox event id is a UUIDv5 (`Uuid::NAMESPACE_URL`) of `elembra://io.elembra.chat/event/<nostr event id>`; `time` is the Buzz event's own `created_at`, not the observation time; the durable event is published only on first observation. Duplicate observations of the same Buzz event id are a no-op.

5. **One Memory record per Buzz message, with per-event idempotency receipts.** `memory_catalog` has a unique key `(tenant_id, source_application, source_type, message_id)` — exactly one record per Buzz message per tenant, updated by later edited/deleted events. The consumer records a durable receipt per `(consumer_id, source, event_id)` in the same transaction as the effect (at-least-once safe).

6. **Manifest-declared chat publishes + `memory: reference-first`.** The canonical `ApplicationRegistry` manifest for `io.elembra.chat` declares the published integration event `io.elembra.chat.buzz.event.observed.v1` and a `memory` contract with `source_types: ["message"]` and `publication: "reference-first"`; the outbox validates published events against the manifest.

7. **Per-tenant policy flags with a channel-kind hard gate.** `memory_projection` (master switch) and `content_indexing` (body storage) live in `application_enablements.configuration` for `io.elembra.chat`; absent or non-boolean values mean OFF (fail closed). Independently of the flags, only `workspace` channels are ever projected; `dm` / `private` / `excluded` are never projected.

8. **Authorization never trusts Memory.** `ChatResourceOwner` implements the source-authorization contract for `io.elembra.chat` `message` refs using only current state: tenant Chat enablement + per-user preference, the principal's current active binding, and an active admission for the message's community and bound pubkey. Memory-owned state is never imported or queried, so stale or restored Memory records cannot re-grant access.

9. **Reconciliation from the signature-verified observation index.** `POST /api/v1/admin/applications/memory/chat/reconcile` rebuilds the tenant's catalog from `chat_observed_events` (the bridge's verified state) using pure, deterministic projection functions; it never replays outbox events and never touches consumer receipts.

10. **Ownership boundaries.** Bridge owns `chat_observed_events` (verified observation index); Memory owns `memory_catalog` (provenance records); Buzz owns everything else (events, channels, membership, community state). No shared schema and no cross-owner transactions.

## 3. Data flow

### End-to-end: signed Buzz message → Memory record

1. **Delivery authentication.** Buzz POSTs the push (signed Nostr event as opaque JSON + Chat context) with `X-RustShare-Signature: t=<ts>,v1=<hex>`. `BuzzObservationService` verifies the HMAC over the raw body and rejects anything outside the replay window (`RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS`, default 300 s; a plain `v1=` signature without a timestamp fails closed).
2. **Envelope parse + context sanity.** The push is parsed into `BuzzEventPush`; `validate_context` checks that message/thread/supersedes ids are 64 lowercase hex and that the event-type identity rules hold (see §4).
3. **Cryptographic verification.** The raw `id` field must equal the parsed event's id; the Nostr event must be kind 1; `nostr::Event::verify` recomputes the id from the canonical NIP-01 serialization and checks the Schnorr signature. Anything else → `VerificationFailed`.
4. **Mapping.** `community_id` → one active workspace mapping (ambiguity is a 409, fail-closed); the mapping must satisfy the platform invariant `workspace == tenant`. The author pubkey → a live binding whose status is `active` (revoked rows are excluded; `pending` fails).
5. **Policy + body gate.** The tenant's projection policy is read; the message body is kept for storage only when `content_indexing` is on (reference-first otherwise).
6. **Payload + row construction.** `ObservedChatEventData` (signed-event meta + Chat context + admitted principal + `observed_at`) and the `ChatObservedEvent` row are built; the checksum is `sha256:<event id>` because the Nostr id *is* the sha256 of the canonical NIP-01 serialization.
7. **One transaction (observation).** The observation row is upserted into `chat_observed_events` (PK `(tenant_id, event_id)`, `ON CONFLICT DO NOTHING`); only when the row was actually inserted is the deterministic durable event `io.elembra.chat.buzz.event.observed.v1` published into the transactional outbox. A duplicate observation rolls back and returns `DuplicateObservation` — the durable event was already published.
8. **One transaction (projection).** `MemoryChatProjectionConsumer` (id `io.elembra.memory.chat-projection.v1`) validates the envelope and payload (poison → dead-letter), reads the per-tenant policy *before* the receipt gate, fetches the indexing-copy body when `content_indexing` is on (a missing body never blocks projection), and calls `MemoryCatalogStore::upsert_from_event_in_tx`: durable receipt + policy gate + load-or-project + persist in a single consumer-local transaction.

### Failure recovery (offline worker)

Observation and projection are decoupled by the durable outbox (ADR-0031). When the Memory worker is disabled or down, Buzz pushes still ingest (observation + outbox row + pending delivery obligation per registered consumer), so nothing is lost; re-enabling the worker drains the pending deliveries and projects the records. A memory outage cannot even block observation: the ingest path never touches `memory_catalog` (proven by the e2e DROP-table test, §11). Failed consumer work is retried with backoff by the dispatcher; poison envelopes are dead-lettered, never retried.

## 4. Event contract

### Request shape (`BuzzEventPush`)

```json
{
  "event":   { "id": "<64-hex>", "pubkey": "<64-hex>", "created_at": 1752000000, "kind": 1, "tags": [], "content": "...", "sig": "<128-hex>" },
  "context": {
    "community_id":        "string",
    "channel_id":          "string",
    "channel_kind":        "workspace | dm | private | excluded",
    "thread_root_id":      "<64-hex> | null",
    "message_id":          "<64-hex>",
    "event_type":          "created | edited | deleted",
    "supersedes_event_id": "<64-hex> | null"
  }
}
```

### HMAC + replay window

Timestamped signature only: `t=<unix-seconds>,v1=<hex>`. The timestamp must be parseable, the HMAC must verify over the raw body, and `now - t` must be within `RUSTSHARE_WEBHOOK_MAX_AGE_SECONDS` (default 300 s). All failures are `Unauthorized`.

### Schnorr verification

The event must be a nostr kind 1 text note. `Event::verify` recomputes the id from the canonical NIP-01 serialization and verifies the Schnorr signature over it; the raw JSON `id` is only ever compared against the parsed, verified id. Tampered signatures and wrong kinds are rejected with nothing written.

### Context validation rules (fail closed)

- `created` ⇒ the event id **must equal** `message_id` (the first event of a message IS the message id) **and** `supersedes_event_id` must be `None`.
- `edited` / `deleted` ⇒ the event id **must differ** from `message_id` (an edit/delete is a distinct event) **and** `supersedes_event_id` must not equal the event's own id. `supersedes == message_id` is valid — it is the first edit/delete of the root message.

### Mapping / binding requirements

- `community_id` maps to exactly one active workspace mapping (`chat_workspace_communities`); the mapped workspace must equal the tenant (platform invariant). Unknown → 403 `UnknownCommunity`; ambiguous (>1 active) → 409 `AmbiguousCommunity`.
- The author pubkey has a live binding (`revoked_at IS NULL`) with status `active` in that tenant → otherwise 403 `UnboundAuthor`.

### Durable envelope (`io.elembra.chat.buzz.event.observed.v1`)

| Field | Value |
| --- | --- |
| `id` | UUIDv5 of `elembra://io.elembra.chat/event/<nostr event id>` (deterministic, publish-once) |
| `source` | `elembra://io.elembra.chat` |
| `type` | `io.elembra.chat.buzz.event.observed.v1` |
| `subject` | the `resource` URI (`elembra://io.elembra.chat/message/<message_id>`) |
| `resource` | `ResourceRef(io.elembra.chat, "message", <message_id>)` |
| `tenant_id` / `workspace_id` | from the mapping (workspace == tenant) |
| `actor` | `principal:<principal_id>` (the admitted Elembra Principal) |
| `time` | the Buzz event's `created_at` (not observation time) |
| `data` | `ObservedChatEventData` (below) |

`data` (`ObservedChatEventData`):

```text
buzz:      { event_id, message_id, event_type, supersedes_event_id,
             created_at, pubkey, signature, checksum, signature_verified }
context:   { community_id, channel_id, channel_kind, thread_root_id }
principal: { principal_id }
observed_at
```

`signature_verified` is always `true` for events the bridge publishes (unverified payloads are rejected by the consumer).

## 5. Memory record representation

### `memory_catalog` (Memory-owned)

One row per Buzz message per tenant, keyed by the unique `(tenant_id, source_application, source_type, message_id)`:

`record_id` (PK), `tenant_id`, `workspace_id`, `source_application` (`io.elembra.chat`), `source_type` (`message`), `source_ref` (`elembra://io.elembra.chat/message/<message_id>`), `message_id`, `latest_event_id`, `event_type`, `community_id`, `channel_id`, `channel_kind`, `author_pubkey`, `author_principal_id`, `occurred_at` (Buzz event time), `observed_at`, `checksum` (`sha256:<event id>`), `signature`, `signature_verified`, `provenance` (JSONB, append-only), `classification` (`general`), `retention_policy_ref`, `legal_hold_ref`, `authorization_source` (`buzz`), `authorization_ref` (`community:<community_id>:pubkey:<pubkey>`), `content_indexing`, `content` (indexing copy, optional), `indexing_status` (`reference_only` / `content_stored` / `tombstoned`), `tombstoned_at`, `created_at`, `updated_at`.

### `chat_observed_events` (Bridge-owned)

One row per signed Buzz event observed, content-addressed by PK `(tenant_id, event_id)`; because the Nostr id is the sha256 of the signed event, a PK conflict means the identical event was already observed and is never rewritten. Columns: `tenant_id`, `workspace_id`, `event_id`, `message_id`, `event_type`, `supersedes_event_id`, `community_id`, `channel_id`, `channel_kind`, `thread_root_id`, `author_pubkey`, `author_principal_id`, `event_created_at`, `observed_at`, `checksum`, `signature`, `signature_verified`, `body` (indexing copy, `Some` only when `content_indexing` was on at observation), `active` (false for `deleted` events). Indexed on `(tenant_id, message_id)` and `(tenant_id, community_id)`.

### Identity + provenance model

- Record identity is the Buzz message id (the root event id) within the tenant; the record's `latest_event_id` + `event_type` mirror the newest signed event applied.
- `provenance` is an append-only array of `{ event_id, event_type, occurred_at, observed_at }` entries — one per signed event projected into the record (create, every edit, tombstone). Provenance is preserved by edits and tombstones and survives reconciliation rebuilds.
- The record carries the author's pubkey (from the signature) and the Elembra Principal admitted at observation time, plus the authorization ref that describes the community+pubkey the record was admitted under.

## 6. Content / indexing policy

- **Flags.** `memory_projection` (project at all) and `content_indexing` (store message bodies) are read from `application_enablements.configuration` for `io.elembra.chat`. **Defaults are OFF**: absent, null, or non-boolean values mean `false` (fail closed).
- **Channel-kind gate.** Only `workspace` channels can ever be projected. `dm`, `private`, and `excluded` are skipped at projection regardless of the flags (observations of such events are still recorded; their durable event is consumed with no record effect).
- **Body gate at observation.** `BuzzObservationService` stores `chat_observed_events.body` only when `content_indexing` is on at observation time. With the flag off, observation rows carry no body.
- **Record copy at projection.** The consumer copies the body from the observation index (by event id) only when `content_indexing` is on; a missing body never blocks projection — the record is still created with `indexing_status = reference_only`.
- **Reference-first guarantee.** The message body is never present in the durable envelope; the durable event is reference/provenance data plus context. `content_indexing` on the *record* reflects whether a copy is actually stored, not the tenant policy at projection time.

## 7. Authorization model

`ChatResourceOwner` (`backend/server/src/authz/chat_owner.rs`) implements `ResourceOwner` for `io.elembra.chat` `message` refs, declaring exactly `chat.read` (validated against the Application manifest at startup).

**Algorithm** (`gate`), in order — every failure closes:

1. Application/type mismatch → `Invalid`; malformed message id (not 64 lowercase hex) → `NotFound`.
2. Observation lookup for the message (`chat_observed_events`, latest event by `event_created_at`): no row → `NotFound`.
3. `active == false` or `event_type == deleted` → `NotFound` (existence-hiding tombstone).
4. `channel_kind != workspace` → `Deny`.
5. Current Chat enablement: tenant `application_enablements.enabled` AND per-user preference (`COALESCE(pref.enabled, true)`) → else `Deny`.
6. Current active binding for the principal (`status = 'active'`, `revoked_at IS NULL`) → else `Deny`.
7. Current active admission for the message's community and the bound pubkey (admission row `active` AND community mapping `active`) → else `Deny`.

**Current-state-only rule.** Every check is a live database read at request time. The observation row supplies routing context (community/channel) and existence only — it never grants access on its own. Memory-owned state (`memory_catalog`) is never imported or queried, so a stale or restored Memory record cannot override a current revocation.

**Delegation limitation (documented).** `PrincipalContext::effective_user_authority` is intentionally not applied: chat bindings exist only for user principals, so a Service/Agent principal or a delegated request always fails closed to `Deny` today. Wire `effective_user_authority` when the first delegated consumer (Memory/RAG `materialize` or a transport adapter) lands.

**Fail-closed / NotFound semantics.** `authorize` keeps the typed `Deny` / `NotFound` / `Invalid` distinction for internal callers. On `resolve` / `fetch`, every non-Allow outcome surfaces as `SourceError::NotFound`, so callers cannot distinguish "never existed" from "not authorized". A `fetch` of a reference-only message returns `VersionUnavailable` (there is no stored body, and that is never a retryable infrastructure failure). Store failures log and deny — never error-open.

## 8. Edit / delete / retention semantics

Summarized from the semantics analysis (full treatment in `docs/implementation/buzz-memory-projection.md`):

- **Edited/replaced events** update the *same* catalog record (latest event fields + checksum/signature + append-only provenance) with an out-of-order guard: an older event never regresses a newer one; equal timestamps apply.
- **Deleted/tombstoned messages** mark the record `tombstoned` (kept for provenance, `tombstoned_at` set, `event_type = deleted`, `indexing_status = tombstoned`); edits never un-tombstone; a tombstone for a never-projected message is a no-op; authorization returns `NotFound`; reconciliation preserves tombstones.
- **Membership revocation** is immediate fail-closed at `ChatResourceOwner` (only current admission/binding/enablement counts) and is pushed to Buzz via the existing admission-revoked bridge events; stale Memory state can never re-grant access.
- **Channel access changes** are Buzz-owned; Elembra's community-level admission is the coarse candidate filter, and anything not positively confirmable fails closed. Full channel-level enforcement requires a future Buzz API/relay adapter (documented limitation).
- **Retention changes** never delete catalog rows: retention/classification/legal-hold refs are recorded metadata on the record; provenance outlives indexes; future retention enforcement targets indexes, never the catalog (matches ADR-0033).

## 9. Reconciliation

- **Endpoint:** `POST /api/v1/admin/applications/memory/chat/reconcile` (admin-only; the admin's tenant must equal both the authenticated tenant and the body `tenant_id`). Body: `{ tenant_id, since?: ISO8601 }`; a malformed `since` is a 400. Response: `{ processed, created, updated }`.
- **Rebuild algorithm:** read the tenant's projection policy → load observation rows (`chat_observed_events`, `event_created_at >= since` optional, ordered by `event_created_at, event_id`) → `rebuild_records(rows, policy)` (pure, deterministic; groups by `message_id`; applies create/edit/tombstone rules; keeps tombstoned records) → upsert into `memory_catalog` (`ON CONFLICT ... DO UPDATE`). `processed` counts observation rows examined.
- **Repair paths:** restores missing records, corrects drifted latest-event fields and provenance, and re-marks tombstones. Idempotent — re-running with no drift changes nothing. It is the repair path only: no outbox replay, no consumer receipts, no private Buzz database access.

## 10. Non-goals

Out of scope for this feature (unchanged):

- RAG / embeddings / vector retrieval over chat content;
- unified cross-application search and ranking;
- Agents participating in Chat or delegated access (see §7 delegation limitation);
- mail or other source projections into Memory;
- historical import of pre-bridge Buzz history;
- migration of the legacy RustChat backend;
- a second broker/outbox — the feature builds on the existing transactional outbox (ADR-0031).

## 11. Testing summary

- **Unit suites:** `rustshare-memory` crate (payload validation, policy flags/gates, projection functions incl. out-of-order guards, never-un-tombstone, rebuild determinism), `buzz_observation.rs` (context rules, deterministic UUIDv5, checksum semantics, tampered-signature rejection), `memory_projection.rs` (poison/dead-letter fail-closed paths), `chat_owner.rs` (declared surface matches the manifest).
- **Acceptance suite:** `backend/tests/buzz_memory_projection_e2e_test.rs` — DB-backed, `#[ignore]`d, run with `--test-threads=1` against the real stack (real signed kind-1 events, HMAC bridge, real dispatcher, real consumer, real `ChatResourceOwner` authorizer, real reconciliation). It proves the **14 acceptance requirements**:
  1. exactly one `memory_catalog` record per message (idempotent);
  2. duplicate observations and at-least-once redeliveries never duplicate;
  3. event id / signature / checksum / provenance preserved from the signed event;
  4. the author's live binding principal is recorded;
  5. workspace == tenant and community mapping are recorded;
  6. cross-tenant pushes fail closed (no observation, no outbox row);
  7. an offline Memory worker recovers without event loss;
  8. a Memory outage does not affect Chat ingestion (DROP-table proof);
  9. `dm` / `private` / `excluded` channels are never projected;
  10. revoking a member's admission blocks future exposure immediately;
  11. stale (or restored) Memory state can never override Buzz authorization;
  12. tombstone behavior follows Buzz semantics (deleted ⇒ tombstoned ⇒ not-found, irreversibly);
  13. reconciliation repairs a missing projection idempotently;
  14. only signed events are ingested (unsigned/tampered ⇒ rejected with nothing written) and the pipeline is reference-first (no body in the durable envelope or the record unless the tenant opted in).

  Requirement #14's "no private Buzz database dependency" is structural: asserted by the adversarial architecture review (forbidden table names), not by runtime tests — `chat_observed_events` is the bridge's verified state and the only Chat-side source the projection reads.
