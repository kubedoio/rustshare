# Proposed Buzz authoritative membership contract v1alpha2

Status: proposed only. The pinned Buzz compatibility image does not implement
this contract. Do not change the compatibility pin, enable identity replacement,
or treat this proposal as live conformance evidence.

## Capability gap

The supported image is built from Buzz fork commit
`c9a373beca1d602d02308c687db5535c63f3921f`, based on upstream relay commit
`6e5c462ac524de60d7edb46c66130fd779cc9006`. The relay's authoritative
community admission table is `relay_members`.

The existing `POST /api/v1/relay/access/check` answers whether a key can read a
channel/message. Open channels can return `allow` without community membership.
`GET /api/v1/relay/channels` lists readable channels and reports channel-level
membership; it does not report the community-level `relay_members` record.
`GET /api/v1/relay/state/events` returns signed events, not current membership.
Kind 13534 is a signed membership snapshot that can lag the underlying table.
None of these interfaces can prove current community admission for one key.

The relay's `is_relay_member` helper is not suitable for a security read-back:
it is intentionally routed through a bounded-stale read replica. The new
endpoint must read the writer's authoritative membership state directly.

## Required upstream patch scope

The bounded Buzz change is one relay API extension and one shared membership
mutation path:

1. Add the signed `POST /api/v1/relay/membership/check` endpoint below, using
   the existing NIP-98 trusted-service authentication and relay-signed
   kind-19030 response validation path.
2. Add durable per-`(community_id, pubkey)` revision and revocation-tombstone
   state plus durable operation receipts. An unseen key reads as revision zero
   and `absent`; a removed key reads as `revoked` and is never re-admitted.
   Seed current `relay_members` rows as active during migration. Prior removed
   keys have no authoritative tombstones in v1alpha1 and cannot be reconstructed
   from signed event history; Elembra's historical-key prohibition remains in
   force, and each v1alpha2 removal creates a new sticky Buzz tombstone.
3. Route every kind-9030/9031 writer for an enabled v1alpha2 community through
   one transaction that validates authorization, operation ID, expected
   revision, membership, tombstone, and receipt. Reject unversioned writers
   for those communities; leave other communities on their pinned contract.
4. Keep roster/event publication outside the authoritative membership commit.
   It may retry, but cannot change the signed read result or represent command
   completion.
5. Add relay unit/integration tests for the cases below and run the live
   conformance suite against the disposable image before publishing a new
   compatibility pin. Do not change the pin before the suite passes.

The revision/tombstone state and receipt can share one schema or use separate
tables; the externally observable semantics below are mandatory. Buzz must
not expose its database through Elembra or copy community membership into an
Elembra authorization store.

## Read endpoint

Add `POST /api/v1/relay/membership/check`. It is read-only and never grants,
removes, or changes membership.

Request body, covered by the NIP-98 payload signature:

```json
{
  "request_id": "<fresh UUID>",
  "community_id": "<canonical Buzz community UUID>",
  "pubkey": "<64 lowercase hex characters>"
}
```

The relay derives the community from its normalized request host and requires
it to equal `community_id`. The NIP-98 service caller must be in the configured
trusted-service allowlist. A missing or untrusted caller is rejected. The
query reads the primary `relay_members` state and its revision in one database
snapshot; it must not use a cache, read replica, Elembra projection, channel
visibility, or NIP-43 event history.

On an evaluable request, return a raw relay-signed kind-19030 event using the
existing pinned relay key. Its signed JSON content is:

```json
{
  "request_id": "<echo>",
  "community_id": "<echo>",
  "pubkey": "<echo>",
  "membership": "active | absent | revoked",
  "membership_revision": 42,
  "evaluated_at": 1790000000
}
```

All three request fields must echo exactly. `request_id` is a cryptographically
random UUID generated for one read. Elembra persists it with the pending
operation, accepts at most one response for it, and commits acceptance with
the operation-state transition. After restart, if that acceptance was not
committed, Elembra discards the ID and issues a new read; it never reuses an
old response as a fresh decision. `evaluated_at` must be no more than 30
seconds old and no more than 5 seconds in the future. The caller verifies the
event ID and signature, kind, pinned relay key, exact echoes, and freshness
before using the answer.

`membership_revision` is a monotonically increasing nonnegative integer scoped
to `(community_id, pubkey)`, encoded as an unsigned JSON integer. It starts at
zero for a key never observed by the membership state machine and never
resets. Revision overflow must fail closed; it must never wrap. `active` requires a current
`relay_members` row. `absent` means no current row and no revocation tombstone.
`revoked` means no current row and a durable revocation tombstone. Every
successful kind 9031 removal writes the tombstone, including removal of a key
that was already absent. A revoked key cannot be admitted again by kind 9030;
recovery uses a new key. Tombstones and revisions survive relay restart.
If the relay cannot make that determination, it returns
`membership: "unknown"` with a bounded error code and no usable revision, or
HTTP 503 when it cannot produce a signed response. Transport failure, timeout,
HTTP errors, malformed data, signature failure, stale response, and `unknown`
all map to unavailable/deny in Elembra; none means absent.

Malformed key/request is HTTP 400, unconfigured host or community mismatch is
HTTP 404, and missing/invalid/untrusted service authentication is HTTP 401.
The endpoint never returns a positive membership result for an unresolved or
different community.

## Mutation and ordering semantics

Buzz continues to use NIP-43 kind 9030 for add and kind 9031 for remove.
Commands remain signed by an authorized Buzz relay administrator/service
identity; Elembra's human Buzz private keys remain client-held.

At the pinned version, duplicate kind 9030 is a successful no-op. Kind 9031
for an absent key is rejected as not found. Commands have no operation ID or
expected revision, so relay `OK` is a command-processing result, not proof of
current membership. Concurrent or delayed add/remove commands are resolved by
arrival/transaction order and can overwrite a more recent desired state.

For v1alpha2, versioned bridge commands carry two signed NIP-43 tags:

```text
["elembra-op", "<stable UUID for this mutation step>"]
["elembra-expected-revision", "<membership_revision from the last read>"]
```

The relay serializes every mutation for one `(community_id, pubkey)` on the
same durable revision/tombstone row used by the read endpoint. For an unseen
key, it creates the revision-zero row with a conflict-safe insert and then
locks that row before checking the expected revision; concurrent first writes
must not create two independent revision histories. It validates
service/admin authority and role restrictions, checks the expected revision,
applies the NIP-43 add/remove effect, records a receipt, and advances the
revision in one database transaction. Removal of the relay owner remains
forbidden. For a community enabled for v1alpha2, every membership writer must
obey these rules. Unversioned commands are rejected for that community. A
legacy writer must never bypass a revocation tombstone or restore a revoked
key.

- A duplicate `elembra-op` with identical kind, target, community and expected
  revision returns its stored outcome and does not repeat the mutation.
- Reuse of an operation ID with different contents is rejected.
- Receipts are durable and never automatically pruned; a repeated operation ID
  must remain idempotent after a relay restart or a long-delayed duplicate.
- A new operation with a stale expected revision is rejected without changing
  membership. That rejection is idempotently returned for the same operation
  ID; after a fresh read, a retry uses a new operation ID and its new revision.
- A distinct add when already active and remove when already revoked are
  successful no-ops that record their receipt without incrementing the
  revision. Removing a never-seen/absent key creates a revocation tombstone
  and advances the revision; this makes revocation sticky.
- A real `absent → active` or `active → revoked` transition increments the
  revision once. An add against `revoked` is rejected even with the current
  revision. A tombstone is never cleared by the replacement bridge.
- All other membership writers for a v1alpha2-enabled community must use the
  same conditional mutation path; old unversioned Elembra admission events
  must be drained or quarantined and every bridge worker upgraded before
  enabling the contract. No writer may bypass the tombstone rule.
- For racing commands at revision `r`, exactly one transition can commit from
  `r`; losers receive a stale-revision rejection and must read again. Delayed
  commands based on `r` cannot restore a key after a later revocation.

The Nostr `OK` result acknowledges only whether that versioned command was
accepted or rejected. It is never a replacement-completion signal. Elembra
must perform a fresh signed read after each required mutation. A fresh response
that reports the wrong membership state, a later revision with conflicting
state, or an unknown result prevents completion.

Elembra serializes the remove-old → verify-absent → add-new → verify-active
steps per tenant/Principal in its durable replacement operation. Buzz's
per-key revisions fence stale/duplicate commands for each key; they do not
replace Elembra's cross-key operation state machine.

## Restart, consistency, and failure behavior

`relay_members`, revocation tombstones, revisions, and operation receipts are durable
database state. The read endpoint bypasses replica routing and caches, so a
successful response reflects the primary state at `evaluated_at`. It does not
promise that no later authorized mutation can occur. A caller must check
immediately before advancing its state machine and retain the returned
revision for its next conditional mutation.

A database outage, replica-only visibility, cache-only result, unknown
community, or unavailable signing identity cannot produce `active` or
`absent`. A failed response remains unavailable; it is never synthesized from
an outbox receipt, NIP-43 announcement, historical event, or Elembra row.
Membership mutation and revision/receipt commit are atomic. Snapshot/event
publication may be retried separately and does not change the authoritative
membership result.

## Required Buzz conformance tests

Run against the supported disposable Buzz image, not a RustShare mock:

1. An initially absent key reports `absent`; wrong community and wrong key
   echoes cannot return `active`.
2. A successful 9030 add reports `active`; a 9031 remove reports `revoked`.
3. The read API cannot add, remove, or otherwise grant membership.
4. Invalid NIP-98, untrusted service keys, invalid relay signatures, forged
   responses, stale timestamps and replayed request IDs are rejected.
5. Retrying identical add/remove operation IDs converges without extra state
   transitions; reused IDs with changed payloads fail.
6. Out-of-order and concurrent add/remove commands with the same expected
   revision cannot both commit; stale commands cannot re-admit a removed key,
   and no authorized writer can bypass its revocation tombstone.
7. Unauthorized add/remove and owner-removal attempts do not mutate state.
8. Relay restart preserves active/absent/revoked state, tombstones, revisions
   and operation receipts.
9. Database/relay unavailability returns unknown/error; it never fabricates
   `absent` or `active`.
10. After every accepted command, a fresh read returns the authoritative state
    and revision; NIP-43 roster publication alone does not pass this check.

Until Buzz implements this contract and the compatibility manifest pins the
tested image and contract version, replacement remains unsupported. Do not
modify `config/buzz-compatibility.env` or claim live conformance for this
proposal.
