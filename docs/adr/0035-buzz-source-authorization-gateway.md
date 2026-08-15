# ADR-0035: Buzz Source Authorization Gateway

Status: Accepted (implemented v1alpha1; amended 2026-08-14 — batch + channel registry endpoints specified; stream-message wire format adopted)  
Date: 2026-08-11 (amended 2026-08-14)

## Context

Elembra Chat's exposure of channel/message content must be authorized by the
community's authoritative Buzz relay. Elembra must ask Buzz whether a mapped
Principal may currently read a channel/message, and must be able to repair its
Chat projections — the observation index and the Memory catalog — from Buzz's
public state.

ADR-0034 established the Elembra/Buzz boundary: Buzz owns signed events,
channels, membership, and chat source-of-truth semantics, while Elembra keeps
tenants, principals, bindings, and admissions. But it left per-channel
authorization to "a future Buzz adapter". This ADR closes that gap.

The constraints that shaped the design:

- Elembra must not read Buzz's private database;
- Elembra must not duplicate Buzz's authorization rules — no second ACL system;
- the server workload must never hold a human user's signing key;
- a stored authorization snapshot is unacceptable — decisions must reflect
  current Buzz state, so revocations take effect on the very next read;
- any authorization failure must fail closed.

## Decision

**Elembra gates every Chat content read on a current, cryptographically
authenticated decision from the community's authoritative Buzz relay, and
repairs Chat projections from the relay's public signed state.**

1. **`BuzzAuthority` is the final authority.** `rustshare-resource-auth`
   defines the `BuzzAuthority` contract (`can_read` → `Allow` / `Deny` /
   `NotFound`). The gate's final channel/message decision derives from CURRENT
   Buzz authority via a live access check. Local admission, binding, and
   enablement remain a coarse pre-filter that can only reduce access; the
   observation index and Memory catalog are routing/existence only, never
   authorization. Every authority failure — transport, auth rejection, invalid
   response, staleness, misconfiguration — fails closed to `Deny`.

2. **A generic upstream capability** is proposed for the external Buzz
   repository (spec `docs/specs/buzz-upstream-authorization-v1alpha1.md`):
   NIP-98-authenticated `POST /api/v1/relay/access/check` and
   `POST /api/v1/relay/access/check-batch` (≤64 checks per round-trip),
   `GET /api/v1/relay/channels` (the authoritative channel registry), and
   `GET /api/v1/relay/state/events`. Responses are raw signed kind-19030
   events signed by the relay's key, pinned per-community mapping via
   `relay_pubkey` (admin-rotatable), echoing the request verbatim, and fresh
   within a 60-second window. The capability is generic — any trusted service
   workload with a provisioned service key can use it.

3. **Unconfigured mode (`local`) keeps the previous coarse workspace-only
   gate**, documented as not final per-channel authorization. A deployment
   that never sets `RUSTSHARE_CHAT_AUTHORITY=buzz` behaves exactly as before;
   there is no silent fallback from `buzz` to `local`.

4. **Reconciliation repairs projections from Buzz state, idempotently.** The
   admin reconcile path pages the relay's signed state over the public HTTP
   contract, re-verifies each event, repairs the observation index, and
   re-projects the Memory catalog — with no outbox/receipt writes and no
   private DB access.

5. **Pins are per-mapping and rotatable; tombstones are immutable.**
   `relay_pubkey` is a per-community-mapping column rotated via
   `PATCH /api/v1/admin/applications/chat/workspaces/{workspace_id}/community`
   without remapping the community; a stale or missing pin fails closed
   rather than weakening trust. The Memory-catalog fold's `upsert_records`
   never re-writes a tombstoned conflict row (`WHERE indexing_status <>
   'tombstoned'`), so a backdated relay delete cannot un-tombstone a Memory
   record.

6. **Elembra Chat adopts the Buzz stream-message wire format.** Chat
   messages are published as kind 9 (`KIND_STREAM_MESSAGE`) with an
   `["h", "<channel-uuid>"]` tag; kind-1 remains accepted on Elembra
   ingestion as legacy during the transition.

## Consequences

### Positive

- Chat authorization is delegated to the authority that owns it: no parallel
  ACL system, no duplicated rules, no private DB access.
- Revocations and membership changes at the relay take effect on the very
  next read.
- The server never holds a human signing key; requests are NIP-98-signed with
  a provisioned service key.
- Reconcile repairs are idempotent and cannot re-trigger the durable pipeline
  (no outbox or consumer-receipt writes).

### Negative

- Buzz-mode reads fail closed when the relay is unreachable or the mapping is
  unpinned — read availability depends on the relay.
- One relay access-check round-trip per message on single-check paths; batch
  consumers use `POST /api/v1/relay/access/check-batch` (≤64 checks in one
  round-trip), now specified in the v1alpha1 contract.
- Channel discovery in buzz mode comes from the relay's authoritative channel
  registry (`GET /api/v1/relay/channels`) instead of the observation-derived
  listing — observation-derived discovery is deprecated in buzz mode.
- No body backfill for never-eligible channels: bodies are captured only for
  channels with `content_indexing`.
- The upstream endpoints must be implemented in the Buzz repository before
  buzz mode is enabled in production; until then every deployment runs
  `local` against the coarse gate.

## Rejected alternatives

### Cache the relay's decision instead of a live check

Rejected. A cached `allow` would let revocations and membership removals
outlive their effective time, and the cache would become a second
authorization system — the failure mode ADR-0034 warns against.

### Replicate Buzz's authorization rules in Elembra

Rejected. Duplicated rules drift from the authority, and Elembra would have
to re-implement channel visibility, membership, and message availability.

### Hold human signing keys server-side for direct relay queries

Rejected. This destroys the sovereign-identity property ADR-0034 preserves:
the workload is trusted but must never be able to sign as a user.

## Acceptance criteria

- [x] `BuzzAuthority` contract, local fallback, fail-closed error surface.
- [x] NIP-98 service authentication; kind-19030 pinned/echoed/fresh responses.
- [x] Admin reconcile over the public HTTP contract, idempotent, no outbox writes.
- [x] Admin relay-pin rotation endpoint; tombstone-immutable `upsert_records`.
- [x] 24-test suite (18 acceptance + admin pin-rotation + 5 handler-level) against a contract-faithful fake relay.
- [x] Real relay endpoints implemented in the Buzz repository (proposed spec).
  **Satisfied:** kubedoio/buzz PR #1 (merged; the v1alpha1 authorization API
  is on `kubedoio/buzz` main) implements `POST /api/v1/relay/access/check`,
  `/access/check-batch`, `GET /api/v1/relay/channels` and
  `GET /api/v1/relay/state/events` — NIP-98 authenticated,
  trusted-service-gated (`RELAY_TRUSTED_SERVICE_PUBKEYS`), kind-19030 signed
  responses.
- [x] Live-relay conformance test replacing the fake before production buzz mode.
  **Satisfied:** `backend/tests/buzz_live_conformance_test.rs` (proofs 1–12,
  run by `scripts/run-buzz-conformance.sh` against the real relay built from
  merged Buzz main) replaces the fake relay for the production-authority
  proofs; the fake remains for the contract-faithful unit/integration suites.
  RustShare production buzz mode (kubedoio/rustshare PR #249, merged) consumes
  that merged contract.
