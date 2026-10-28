# Chat identity replacement protocol

Status: M4.1b2a protocol foundation; replacement remains disabled until the
Buzz read-back contract and implementation land. Pairing and replacement UX
are not enabled.

Authority is split as follows:

- Elembra owns the tenant, Principal, workspace mapping, binding, operation
  state, and local authorization decision.
- The client owns every human Buzz private key and signs proof of possession.
- Buzz owns signed events, membership, and the decision to admit a public key.
- Elembra never treats an outbox enqueue or local row update as Buzz completion.

## Distinct operations

### Add or remove one device

Each device would need a distinct keypair and a device record bound to the
tenant, Principal, workspace, key, authenticated initiating account, expiry,
and single-use challenge. Enrollment must be approved by an existing
authorized key or an explicitly authorized recovery administrator. A platform
device token is not proof of Buzz-key ownership.

The pinned Buzz relay authorizes public keys independently for community
membership, but it has no account/device relation. It can remove a public key;
it cannot distinguish two devices holding the same private key or establish
that two different keys belong to one Elembra Principal. Therefore Elembra
must not present shared-key copies as individually revocable devices.

### Replace a compromised or lost identity

Replacement changes the Principal's Buzz actor key. It is explicit, requires
the replacement key's NIP-42 proof, and binds the operation to tenant,
Principal, workspace/community, the exact previous binding, and one unconsumed
challenge. OIDC or an administrator action alone never proves possession of
the replacement private key.

The operation record must retain the previous and replacement binding IDs,
workspace/mapping, challenge ID, audit actor, state, timestamps, and redacted
failure reason. A unique live operation per tenant/Principal and a transaction
lock on the Principal serialize attempts. Challenge consumption, old local
revocation, replacement-pending creation, operation creation, and the first
outbox event commit atomically. A consumed challenge is never restored.

## State machine

```text
pending_revocation
  -> pending_revocation_verification
  -> pending_replacement_admission
  -> pending_membership_verification
  -> completed

Transient errors -> at most five lease-fenced retries with backoff
Definitive rejection or exhausted retries -> blocked in the current state
```

At challenge consumption Elembra revokes the old binding and local admissions
immediately, preserving the binding row and historical author attribution.
The replacement binding stays pending and cannot send. The bridge then removes
the old public key from the mapped Buzz community, verifies the relay's current
membership state, admits the replacement key, and verifies that state again.
Only the final verified state may atomically activate the replacement binding
and its local admission. Failure, timeout, restart, dead-letter, or an
unavailable relay leaves replacement sending denied.

Each conditional mutation attempt has a stable command ID and payload. The old
key's revocation step advances only after Buzz reports its durable `revoked`
tombstone; `absent` alone is insufficient because the key could otherwise be
admitted later. Before adding the replacement key, Buzz must report it as
`absent` with no tombstone. If it is already `active` before this operation's
add has a recorded outcome, the operation fails closed instead of adopting
that membership. After an ambiguous delivery, the bridge retries the same
command ID and then performs a fresh read; it does not infer completion from
current membership alone. Stale-revision receipts may recover only after a
fresh signed read and a new command ID. Replays return the persisted state.
Blocked operations are never automatically retried after a long delay and
cannot reactivate the old binding or reuse the consumed challenge. Any future
operator recovery must be authenticated, audited, and retain all proof and
membership checks; until implemented, operators must not clear block fields
directly. Duplicate delivery and process restart must not create duplicate
bindings, admissions, audit transitions, or a second rotation.

Status is visible only to the owning authenticated Principal or an explicitly
authorized tenant security administrator. Responses distinguish pending,
failed, and completed. Logs and status responses contain operation and public
binding identifiers only, never credentials, private keys, or backups.

## Threat boundaries

- An SSO session or tenant administrator may request recovery but cannot prove
  possession of the replacement Buzz key; only that key's valid challenge
  signature supplies that proof.
- A stolen old key remains cryptographically capable of signing. Buzz
  community removal must deny its future relay admission; Elembra cannot
  invalidate old signatures or distinguish copies of that key.
- A bridge outage or unverifiable relay state keeps replacement sending
  disabled. Local state and an outbox receipt cannot override this rule.
- Bridge credentials can mutate community membership and therefore remain
  separately provisioned, restricted to the mapped relay, and excluded from
  human identity/key custody. Audit records identify the operation and actor
  without recording secret material.
- Database or relay compromise is outside the protection offered by client
  key custody alone. Historical events remain attributed to their original
  Buzz signer during incident response.

## Buzz capability boundary

The compatibility-pinned Buzz relay commit
`6e5c462ac524de60d7edb46c66130fd779cc9006` accepts NIP-43 kind 9031 removal
and kind 9030 addition for individual public keys. Its relay membership table
is the access gate, so a key removal affects every copy of that key in that
community. Different keys can be members at the same time, but Buzz does not
associate them with one account or Principal. Historical signed events retain
their original public-key attribution.

The current Elembra bridge publishes these commands and treats a matching
relay `OK` response as successful outbox delivery. That delivery result is not
an operation state, doesn't prove the expected roster was reconciled, and is
not correlated to a durable replacement operation. The v1alpha1 Buzz
authorization API covers content-access decisions and channel discovery, not
an authoritative identity membership read-back. Buzz publishes kind 13534 as
a relay-signed membership snapshot, but a retrieved historical event has no
request-bound freshness and may precede later membership changes. It is not a
current-state query. The exact proposed read and
versioned-command semantics are specified in [Buzz authoritative membership
contract v1alpha2](../specs/buzz-authoritative-membership-v1alpha2.md); the
current Buzz compatibility pin does not implement it.

Before enabling replacement, Buzz must provide a supported, relay-authenticated
query of authoritative membership state that is fresh and scoped to the exact
community and target key, plus conformance evidence that removal denies the
old key and that duplicate/retried add/remove operations converge. Elembra
must not infer revocation from a NIP-43 roster event, local admission row, or
queued/processed outbox delivery.

## M4.1b2a implementation boundary

Until that Buzz contract is available, `rotation_of` verification is rejected
at both the HTTP and storage boundaries. Ordinary binding is limited to a
Principal with no previous binding history. Revoking a binding does not make
that Principal eligible for fresh-key onboarding with `rotation_of` omitted;
such attempts remain rejected while Buzz removal may still be pending. These
checks run under the Principal lock before consuming the challenge.
The earlier path revoked local rows,
queued Buzz removal, and activated the replacement before the asynchronous
bridge had applied anything. The guard prevents that unsafe transition; it is
not a replacement implementation. No per-device pairing, replacement status
API, or UX is enabled by this slice.

The smallest safe M4.1b2b slice is: agree and implement the Buzz read-back
contract; add durable Elembra replacement operations and redacted status/audit;
sequence remove → verify → add → verify in the bridge; then add PostgreSQL
crash/concurrency/replay tests and live conformance tests against the pinned
Buzz image. Expose no UX until both suites pass and independent security
review approves the protocol.
