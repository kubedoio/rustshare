# Buzz Source Authorization & Reconciliation Gateway — Design

> **Status:** Implemented v1alpha1 (on `codex/buzz-source-authorization`).  
> **Date:** 2026-08-11  
> **Related:** ADR-0033 (Elembra Memory Architecture), ADR-0034 (Elembra Chat and Buzz Boundary),
> `docs/specs/buzz-upstream-authorization-v1alpha1.md` (upstream contract),
> `docs/implementation/buzz-source-authorization-gateway.md` (implementation notes),
> `backend/tests/buzz_authority_gateway_test.rs` (acceptance suite).

## 1. Goal

**Elembra never decides chat authorization itself: every Chat content exposure is gated on a
current, cryptographically-authenticated decision from the community's authoritative Buzz relay,
and the Memory projection can be repaired from the relay's signed state over the public HTTP
contract — with no second ACL system and no access to Buzz's private database.**

"Current" means the relay's channel visibility / membership / message availability at request
time — never a stored snapshot. "Cryptographically-authenticated" means NIP-98 service
authentication on the request and pinned, echoed, freshness-checked kind-19030 signatures on the
response; any failure mode other than an explicit signed `allow` is a denial.

## 2. Architecture decisions

1. **The relay is the final authority for channel/message visibility.** Elembra's gate ends
   with `BuzzAuthority::can_read`, whose answer comes from CURRENT Buzz state. Rationale:
   Elembra creates no parallel authorization code path, and revocations/changes at the relay
   take effect on the very next decision.

2. **A coarse local pre-filter runs first, and can only ever deny.** Tenant Chat enablement,
   active binding, and active community admission are checked before the authority is consulted,
   so Elembra's own trust boundaries still fail closed (a revoked admission can never reach the
   relay). Rationale: these are Elembra-side administrative facts (who is bound, which community
   is mapped, is the app enabled); they are never a final allow.

3. **NIP-98 service authentication; no human user key server-side.** Every request carries a
   kind-27235 event signed with the workload's Nostr service key (`RUSTSHARE_CHAT_BRIDGE_SECRET_KEY`).
   The relay is provisioned with the service public key and rejects any other signer. Rationale:
   the server workload is trusted but must never hold a user's signing key; NIP-98 binds the
   request to the exact URL/method/body, defeating request forgery and replay across endpoints.

4. **Signed, pinned, echoed, fresh responses (kind-19030).** The relay returns a raw signed
   Nostr event of private replaceable kind 19030 whose `pubkey` must equal the mapping's pinned
   `relay_pubkey`, whose content echoes the request's `pubkey`/`channel_id`/`message_id`
   verbatim, and whose `evaluated_at` is within 60 seconds. Rationale: pinning ties the trust
   to the configured community relay (host-derived isolation), echo binds the decision to the
   exact check that was asked, and freshness defeats response replay. Every mismatch fails
   closed to `Deny`.

5. **The state capability reuses the webhook push contract.** `BuzzStateEntry.context` is
   field-for-field `BuzzPushContext`, so the reconcile consumer reuses its existing validation
   unchanged. Rationale: one event shape end-to-end; the bridge already knows how to verify and
   ingest these events.

6. **Reconcile-from-Buzz repairs the observation index, then folds the catalog — no outbox
   replay.** `ingest_without_outbox` re-verifies each event and upserts the observation row in
   one transaction, publishing nothing; the existing pure fold then rebuilds the Memory
   projection. Rationale: the repair path must not re-trigger the durable pipeline (consumer
   receipts/deliveries stay untouched) and must be idempotent (per-event-id upserts).

7. **The tenant-scope guard bounds a shared relay.** During tenant A's repair, any entry whose
   community maps elsewhere is skipped. Rationale: `validate_and_build` routes by community to
   derive the row's tenant; without the guard a shared relay could write rows into another
   tenant's observation index during A's admin repair.

8. **Fail closed everywhere.** Transport errors, 401, 5xx, signature/kind/pin mismatches, echo
   mismatches, stale responses, unparseable bodies, oversized bodies, malformed pages, and
   unknown decision strings all map to denial — never to allow. Rationale: authorization
   failures must be safe by default; only an explicit signed `allow` opens exposure.

## 3. Upstream capabilities

Two HTTP endpoints on the community's relay host, defined in
`docs/specs/buzz-upstream-authorization-v1alpha1.md` (the source of truth for the relay side and
for the contract-faithful fake relay in the acceptance suite):

- `POST /api/v1/relay/access/check` — body `{ pubkey, channel_id, channel_kind, message_id?,
  event_created_at? }`; response content
  `{ decision: allow|deny|not_found, reason, evaluated_at, pubkey, channel_id, message_id }`,
  echoing the request verbatim. Deleted/unknown messages and unknown channels → `not_found`;
  non-members → `deny`; current members of a visible channel with an available message →
  `allow`. Existence-hiding: no membership lists, no revealing reasons.
- `GET /api/v1/relay/state/events?since=&limit=&cursor=` — paged `{ entries: [{ event, context }],
  cursor, complete }`; `complete: true` terminates the stream and a page that is incomplete
  without a cursor is malformed. Entries are limited to what the authenticated service workload
  is entitled to see for the relay's community.

The HTTP base is derived from the stored `relay_url` (`ws://`→`http://`, `wss://`→`https://`,
host+port unchanged), preserving host-derived community isolation. Production MUST use `wss://`.

## 4. Authorization model

**Final decision from Buzz; memory never authorization; membership/ACL coarse-filter only.**

- The gate reads only current state: Chat enablement, active binding, active admission, active
  mapping, and the current observation row for routing/existence. `memory_catalog` is never
  imported or queried by the gate, so stale or tampered Memory records cannot re-grant access.
- The relay's answer is final. `deny`/`not_found` map 1:1; every other outcome (transport,
  auth, invalid response, staleness) is a denial.
- Elembra's own admission/binding state is a coarse pre-filter that can only reduce access —
  never expand it beyond what the relay's channel decision allows.
- Existence-hiding: `resolve`/`fetch`/`materialize` collapse all non-Allow outcomes to
  `SourceError::NotFound` (bodies never captured → `VersionUnavailable`), so "never existed",
  "deleted", "not a member", and "relay unreachable" are indistinguishable to callers.

## 5. Reconciliation design

`reconcile_chat_memory_from_buzz_for_tenant` (admin endpoint
`POST /api/v1/admin/applications/memory/chat/reconcile`, body `{ tenant_id, since?, source }`
with `source: "buzz"`; the gateway must be configured else 503 — no silent fallback):

1. Fail closed on the mapping (exists, active, `relay_url` non-empty, `relay_pubkey` pinned).
2. Page the relay's signed state (`since` floored to whole seconds, page size 200, opaque
   cursor, `complete` terminates, 10_000-page cap).
3. Per entry: unknown `event_type` → skip; **tenant-scope guard** (community mismatch → skip);
   convert to `BuzzEventPush` and `ingest_without_outbox` (context validation, kind-1 id +
   Schnorr verification, community → tenant + author → active binding mapping, body gate, single
   observation upsert transaction, NO outbox insert). One bad entry skips itself; it never
   aborts the repair.
4. Fold the repaired observation index into `memory_catalog` with the existing idempotent
   `rebuild_records` / `upsert_records`.

Idempotency: observation upserts keyed on `(tenant_id, event_id)` with `ON CONFLICT DO NOTHING`;
re-running with no drift yields `created = 0`, `updated = 0`. No outbox writes, no consumer
receipts, no delivery ledgers — the durable pipeline is never replayed by a repair.

## 6. Security tests

The acceptance suite (`backend/tests/buzz_authority_gateway_test.rs`) runs the full stack
against an in-test fake relay (in-memory, no database) and a real dev database:

| # | Requirement | Proof |
| --- | --- | --- |
| 1 | Member can materialize a workspace message | push signed note (content_indexing on) → authorize Allow → resolve available → fetch == note body → materialize == note body |
| 2 | Non-member cannot materialize | not in fake's member set → Deny; materialize empty; fetch NotFound |
| 3 | Private-channel non-member denied | channel_kind private, non-member → Deny |
| 4 | Membership removal immediate | remove_member in fake ONLY → next decision Deny |
| 5 | DM participant allowed, outsider denied | participant → Allow (resolve metadata, fetch VersionUnavailable — DM bodies never captured); outsider → denied |
| 6 | Local admission cannot bypass relay | Elembra admission active, relay denies → Deny |
| 7 | Stale Memory cannot grant | projected record + observation row present, relay denies → Deny; fetch NotFound |
| 8 | Cross-tenant + wrong pin | foreign tenant ctx → NotFound; mapping pinned to wrong key → InvalidResponse → Deny |
| 9 | Unreachable relay fails closed | dead port, local admission passes → Deny; fetch NotFound |
| 10 | No user key server-side | fake records the authenticated signer == SERVICE pubkey; forged user-key-signed request → 401; `chat_identity_bindings` has no private-key column |
| 11 | Reconcile repairs missing projection | catalog rows deleted → rebuilt exactly once; outbox count unchanged |
| 12 | Reconcile repairs missing observation | observation + catalog rows deleted → rebuilt from relay state over HTTP |
| 13 | Reconcile idempotent | second run `created == 0`, `updated == 0`, one row per message |
| 14 | Reconcile over public HTTP only | fake's `state_requests > 0`; fake has no database by construction |
| N1 | Deleted message not_found | `mark_deleted` at the fake → not_found → NotFound end-to-end |
| N2 | Binding rotation asks the new pubkey | old binding revoked + new bound/admitted → Allow; recorded request carries the NEW pubkey; direct check with the old pubkey → Deny |
| N3 | Unknown channel not_found | message known, channel never registered → not_found → NotFound |
| N4 | Tenant-scope guard | shared relay pages a foreign-community entry → skipped; no row written into either tenant's index |

## 7. Non-goals

- Elembra will not re-implement channel ACLs, membership, or message availability.
- No pull-based steady-state ingestion: the webhook push remains the live path; the state
  capability exists for admin repair.
- No delegation/agent support in the gate (unchanged: chat bindings are user-principal only and
  delegated requests fail closed).
- No retention enforcement changes (recorded metadata only; see the memory projection design).
- The relay side is NOT implemented in this repository — the v1alpha1 spec and the fake relay
  are the contract; the Buzz repository implements the real endpoints.

## 8. Open questions

- **Relay capability adoption.** When the real relay endpoints ship, the fake relay must be
  replaced by a contract-conformance test against the live relay, and the `local`→`buzz`
  migration of a production mapping requires provisioning the service key and pinning
  `relay_pubkey` on each community mapping.
- **Clock skew tolerance.** The 60-second freshness window is a single constant; deployments
  with unusual skew may need it configurable.
- **Reconcile scale.** The `since` watermark and page caps bound a repair; a full first-time
  backfill of a large community is an operator decision (page budget), not automated here.
