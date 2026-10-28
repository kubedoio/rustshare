# M4.1b2c: durable Buzz identity replacement

Status: proposed implementation specification; replacement and pairing UX remain disabled.
Baseline: Elembra `3fbfd641690e07a109feb7294a255f5de3ba88cc`.
Tracks #424 and #215. Extends [the identity contract](../contracts/chat-identity-replacement.md)
and [Buzz v1alpha2](buzz-authoritative-membership-v1alpha2.md).

## Decision and prerequisites

Elembra owns intent, proof verification, local bindings, admission, operation
status and audit. Buzz alone owns remote membership. Implement one durable
operation with the following states; an outbox delivery receipt never activates
a replacement:

`pending_revocation -> pending_revocation_verification -> pending_replacement_admission -> pending_membership_verification -> completed`

Do not implement or enable the end-user replacement/pairing flow before Buzz
release qualification, an immutable compatibility pin, live acceptance and
independent security approval. M4.1b2c may implement and test the worker behind
the existing disabled boundary; it must not silently remove the storage/HTTP
replacement guards. No production community opt-in is included.

## Existing seams and smallest scope

- `backend/crates/storage/src/chat_identity.rs`: reuse Principal locking,
  challenge consumption, tenant-scoped bindings/admissions and transactional outbox.
- `backend/server/src/services/chat_membership.rs`: reuse request persistence,
  signature/freshness validation and community/key revision fencing. Its existing
  helper commits acceptance independently; calling it and later advancing an
  operation is insufficient. Extract an acceptance-in-transaction path used by
  the operation transition while retaining current callers and guarantees.
- `backend/server/src/buzz_gateway.rs`: reuse the pinned kind-19030 verifier;
  never inspect Buzz's database from Elembra.
- `backend/server/src/buzz_bridge.rs` and the existing outbox dispatcher/store:
  reuse delivery/lease/retry infrastructure. The current bridge emits legacy
  commands and treats relay OK as delivery. Add explicit versioned replacement
  command handling; do not reinterpret legacy receipts as verified membership.
- Add one forward migration, a focused replacement store/service and focused
  PostgreSQL/live tests. Update event schemas/registrations, callers, fixtures,
  status/audit authorization, identity contract and changelog as needed. No new
  queue framework, key custody service, dependency or UI.

## Durable records

An operation records operation UUID, tenant, Principal, workspace, exact mapping
ID/version, community, relay URL and pinned identity/config generation, old and
replacement binding IDs/public keys, consumed challenge ID, explicit intent,
audit actor, state/version, timestamps, lease/retry metadata and bounded error
code. Keep one live operation per `(tenant, Principal)` using a partial unique
index, backed by Principal row locking. Verify every referenced row belongs to
that tenant/Principal and exact mapping. Reject key reuse across historical
bindings, including revoked keys; historical absence on Buzz is insufficient.

Persist separate removal/admission command records before dispatch: immutable
operation ID, actor, community, exact target, kind/role, expected revision and
outbox reference. IDs for the two effects differ and remain stable across
ambiguous delivery. If a fresh observation proves a definitive stale rejection,
record that outcome and create a new conditional attempt with a new ID; never
change the payload bound to an existing ID. Never turn a timeout into success.

Verification requests bind operation ID/state/version, tenant/mapping, expected
key/state, pinned relay identity, minimum revision and a fresh request UUID.
Accepted evidence records signed event ID, state/revision, evaluated/accepted
times and operation linkage. It contains no private key or recovery material.
Add tenant ownership to operation-linked verification access; the current
platform-wide community/key monotonic floor must still survive pin rotation.
Never allow a tenant to reset or lower that floor.

## Transition rules

| State | Work and atomic transition |
| --- | --- |
| `pending_revocation` | Entry transaction consumes a fresh, intent-bound replacement-key NIP-42 proof, locks Principal, revokes old local binding and admission, creates replacement as pending/non-sending, operation, audit and durable work item. Read old membership freshly and persist a conditional 9031 removal command using its revision; even ABSENT must receive removal to create a tombstone. After delivery attempt, move to verification; ambiguous delivery retains original command ID. |
| `pending_revocation_verification` | Fetch fresh old-key evidence. Only verified REVOKED permits progress. ACTIVE/ABSENT cannot pass. Retry pending/ambiguous removal with original ID; definitive stale outcome requires fresh evidence and a separately recorded new attempt. Verify replacement is ABSENT with no tombstone and no local historical prohibition. Atomically accept both observations, persist conditional 9030 command with replacement's observed revision, audit and next work, and advance state. |
| `pending_replacement_admission` | Dispatch the persisted admission command. Retry ambiguity with the same ID and payload. Delivery leads only to `pending_membership_verification`; it cannot activate local rows. Reconfirm mapping/operation generation and old revocation before proceeding. |
| `pending_membership_verification` | Read replacement freshly. Only ACTIVE from the exact mapped/pinned relay with sufficient revision can complete. Reverify old REVOKED as needed for freshness at completion; tombstones are sticky. In one transaction accept evidence, verify operation CAS/lease and current authorization/mapping, activate replacement binding and admission, mark completed and audit. ABSENT retries the recorded admission; REVOKED blocks completion permanently for this key. |
| `completed` | Idempotent terminal result. Old binding/admission stay revoked. Historical signer identity is never rewritten. Duplicate work cannot mutate bindings or emit a second admission. |

Fresh observations may be fetched outside a DB transaction, but acceptance,
state CAS, local activation/revocation, audit and next outbox insertion must
commit together. Acquire locks in one documented order (Principal, operation,
community/key fences in sorted order) and recheck freshness after waiting.
Use READ COMMITTED and enforce one-use request IDs, exact echoes, signature,
kind, pin, timestamp and monotonic revision checks at acceptance. A crash before
commit discards the response and uses a fresh read UUID; command IDs persist.
Do not hold database locks across network I/O. Expired leases cannot commit.

## Failure and authorization rules

Outages get at most five lease-fenced retries with capped exponential backoff.
Stale conditional revisions are recoverable only by recording the stale
receipt, taking fresh signed observations and using a new command ID; these
attempts share the same retry limit. Exhaustion, upstream rejection, a revoked
candidate, invalid evidence, or a changed mapping permanently blocks the
operation in its current state. Blocked operations are excluded from worker
claims; there is no 365-day automatic retry, rollback to the old key,
unrevocation, or alternate onboarding bypass. Mapping changes require explicit
reconciliation rather than silently continuing on another community or relay.
Re-activating a blocked operation needs an authenticated, audited recovery path
that preserves the fresh-proof and membership checks; until that path exists,
operators must not edit retry or block fields directly. Replacing/revoking the
candidate during the operation invalidates completion under the same Principal
lock.

Status lookup is `(tenant_id, operation_id)` plus current actor authorization;
cross-tenant IDs return the existing non-disclosing not-found behavior. Audit
includes tenant, actor, operation, transition and bounded reason, with no
private keys, backups, challenge signature/token, authentication headers or
raw upstream error bodies. Apply the existing tenant retention/access policy.
History continues to identify each message by its original signing key;
replacement is not reattribution or signature invalidation.

## Required acceptance tests

Use isolated PostgreSQL and the qualified immutable Buzz image, not only mocks.
Test valid flow and wrong tenant/Principal/mapping/old binding/key/proof; missing
intent, replayed/expired challenge, historical candidate and two concurrent
requests. Assert old local revocation precedes every remote operation and no
intermediate state can send through any existing authorization path.

Inject crashes before/after every transaction commit, network send and receipt;
lost ACK, lease expiry and duplicate workers retain command IDs and cannot
double-transition. Inject stale conditional rejection, concurrent remote
mutation, ACTIVE/ABSENT old key, REVOKED replacement, regression across relay
pin rotation, stale/replayed/tampered evidence, bigint overflow, DB/relay outage,
mapping changes and candidate/Principal revocation during verification.
Prove evidence acceptance rolls back with a failed operation transition and
activation rolls back with failed audit/outbox persistence. Prove status/audit
isolation and log redaction, unchanged historical attribution and v1alpha1
compatibility. Run existing read-back fence tests as regressions.

Target affected packages first, then Rust format/full Clippy/lib baseline and
SQLx prepare check for schema changes, protected integration, Chat Product
Acceptance and live replacement conformance. Request fresh human security
review after substantive edits. Do not activate UX, opt in communities, merge,
deploy or update production pins as part of implementation.
