# ADR-0034: Elembra Chat and Buzz Boundary

Status: Proposed  
Date: 2026-08-07

## Context

Elembra needs live team communication as part of the same product and memory experience as Files, Notes, Mail and Agents.

The existing RustChat implementation contains useful product/client/integration work, but maintaining a second chat backend beside Buzz would duplicate a complex domain. Buzz already provides the strategically valuable communication substrate: signed Nostr events, relay ownership, community/tenant boundaries, humans and agents as identities, realtime clients and chat-specific source-of-truth semantics.

Absorbing Buzz into the Elembra backend or replacing its signed identity model with ordinary OIDC bearer authentication would destroy those properties and create a large permanent fork.

## Decision

**Elembra Chat is a first-party Elembra Application. Buzz is the independent communication Engine behind that Application.**

Integration occurs through an Elembra Chat Bridge and explicit contracts. Buzz must not depend on Elembra private internals.

## Dependency direction

```text
Incorrect

Buzz core
  -> Elembra database
  -> Elembra Files internals
  -> Elembra permission internals

Correct

Elembra Chat Bridge
  -> Buzz public protocol/API/SDK
  -> Elembra Platform/Application contracts
```

## Source-of-truth boundaries

### Buzz owns

- signed chat events;
- channels/conversations/threads/DMs;
- chat membership/community state required by the Buzz model;
- chat workflows/reactions and chat-specific projections;
- cryptographic event identity/signatures;
- chat-specific retention/tombstone semantics where applicable.

### Elembra Platform Core owns

- Elembra tenant/workspace identity;
- Principals and OIDC identity mapping;
- Application grants;
- audited external-identity bindings.

### Elembra Files owns

- Elembra file/folder resources;
- Files authorization;
- attachment artifact versions and delivery.

### Elembra Memory owns

- memory catalog/provenance records;
- derived search/index/RAG projections.

No shared Buzz/Elembra database schema is allowed.

## Identity model

Elembra and Buzz have different but complementary identities.

```text
Elembra Principal
  = organizational/account identity and delegated authority

Buzz/Nostr public key
  = cryptographic actor identity for signed Buzz events
```

The bridge maintains an explicit tenant-scoped binding:

```text
Elembra PrincipalId <-> Buzz public key
Elembra WorkspaceId <-> Buzz community
```

Bindings are:

- explicit;
- auditable;
- revocable;
- tenant/workspace scoped;
- fail closed;
- protected from cross-tenant reuse.

## OIDC and Buzz authentication

OIDC is the Elembra account/organizational SSO layer.

OIDC **does not replace** Buzz NIP-42/NIP-98 or event signatures.

An Elembra OIDC session proves the user controls/has authenticated the Elembra account. It is not by itself permission for the server to impersonate that user's Buzz private key.

The SSO/pairing design must specify how a Buzz key is generated/imported/bound, where private signing material lives, multi-device behavior and recovery/rotation.

Preferred security direction:

- preserve client-side signing for sovereign identity;
- support secure encrypted key recovery/synchronization only through an explicit key-management design;
- do not silently move all user private keys to server custody for convenience.

A managed/server-signing mode, if ever offered, must be explicitly named and must document that the service can impersonate those managed identities.

## Offboarding/revocation

Cryptographic possession of a valid Buzz private key can outlive an Elembra employee/account.

Therefore offboarding requires authorization/admission revocation in addition to OIDC disablement.

```text
Elembra Principal disabled/removed
  -> Chat Bridge removes/revokes workspace/community admission/membership
  -> future access fails even if old Buzz key can still produce a valid signature
```

Identity binding and access authorization must not be conflated.

## Attachments and Files references

Preferred flow:

1. user selects/uploads an Elembra Files resource;
2. Files returns an immutable/versioned ResourceRef and safe display metadata;
3. Chat publishes a signed Buzz event containing the reference;
4. recipient opens the reference through Elembra Files;
5. Files reauthorizes the recipient at read time;
6. a short-lived delivery URL/stream is produced only after authorization.

Rules:

- no permanent bearer/presigned URL in the signed chat event;
- chat membership alone does not grant Files access;
- revoking a File/share takes effect even though the chat event remains in history;
- unfurls/previews reauthorize and must not copy sensitive content permanently into a broader chat event.

## Memory projection

The bridge publishes selected Chat records to Elembra Memory using ResourceRefs/provenance.

Default projection should avoid treating Elembra Memory as a second complete Buzz event database.

Store/reference as needed:

- Buzz event ID/ref;
- community/channel context;
- mapped Principal;
- event timestamp/type;
- signature/checksum/provenance;
- classification/retention policy;
- authorization owner/ref;
- indexing state.

Materialize message content according to explicit workspace indexing/retention policy.

## Unified search

Initial search is federated/controlled projection:

- Buzz/Chat searches Buzz-owned content under Chat authorization;
- Files/Mail/Notes authorize their own resources;
- Memory can rank/collate results;
- opening/materializing a result reauthorizes against the source.

A single unrestricted shared search index is forbidden.

## Agents

Buzz's ability to treat agents as identities is preserved.

An Elembra Agent has its own Principal plus, when it participates in Chat, its own mapped Buzz identity.

Channel membership does not grant Files/Mail/Memory access. Cross-Application tools require delegated action capabilities evaluated by the owning Application.

Agent Chat events must remain attributable to:

- Buzz signer/agent identity;
- Elembra Agent Principal;
- initiating user/workflow when applicable;
- delegation/grant;
- source ResourceRefs/evidence.

## Failure isolation

- Buzz/Chat should remain useful when optional Elembra Memory indexing is down.
- Files/Notes/Mail remain useful if Buzz is unavailable.
- delayed memory/file integration is handled by durable events/retries.
- no distributed transaction spans Buzz and Elembra.

## Repository strategy

Recommended target:

```text
kubedoio/elembra            # current RustShare repository after rename/cutover
kubedoio/elembra-chat       # Elembra Chat product integration around Buzz
```

`elembra-chat` should contain only what must differ/integrate:

- Buzz upstream tracking strategy;
- branding/product overlay;
- Elembra Chat Bridge;
- identity binding adapter;
- Files/ResourceRef integration;
- Memory/search integration;
- Agent integration;
- deployment and compatibility tests.

Keep the Buzz delta minimal and upstream-trackable.

The current RustChat repository should be mined intentionally for product behavior, clients/integration ideas and migration requirements. It should not force preservation of the old RustChat backend if Buzz replaces that source of truth.

## Consequences

### Positive

- One Elembra product experience without merging two backend architectures.
- Buzz's signed-event model is preserved.
- Upstream Buzz remains trackable.
- Elembra authorization/files/memory remain independently correct.
- Agents retain explicit identities.

### Negative

- Identity binding/key recovery is a real design problem and cannot be hidden by ordinary SSO.
- Two authorization domains (Elembra workspace/resource + Buzz chat/community) must be reconciled deliberately.
- Eventual consistency is required across Chat/Memory/Files integrations.

## Rejected alternatives

### Rewrite Buzz authentication to accept OIDC bearer tokens as the primary actor identity

Rejected. This changes a core trust property and still does not solve signed event identity cleanly.

### Embed Elembra database/permission calls into Buzz core

Rejected. It creates a hard fork and reverses the correct dependency direction.

### Run old RustChat backend and Buzz backend side by side indefinitely

Rejected. Two sources of truth for team chat create migration, consistency and product complexity without strategic benefit.

### Copy all Buzz events into Elembra PostgreSQL as authoritative chat rows

Rejected. Memory/search projections may exist, but Buzz remains the chat source of truth.

## Acceptance criteria

## Identity and admission foundation (2026-08-10)

## ResourceRef attachment slice (2026-08-10)

The first Chat×Files slice uses the existing `ResourceRef`, `PrincipalContext`,
`SourceAuthorizer`, and Files owner adapter. No second Files ACL or resource
reference type was added.

The signed Buzz representation is a normal Nostr event (normally kind `1`)
with exactly one generic tag:

```json
["elembra-ref", "elembra://io.elembra.files/file/<uuid>?version=sha256%3A<64-hex>"]
```

The tag is an identifier only. It contains no tenant hint, URL, token, cookie,
authorization grant, storage key, or private metadata. `rustshare-resource-auth`
provides the tag builder/parser and rejects malformed, duplicate, or extended
tags. Buzz requires no upstream modification: its existing generic Nostr tag
transport preserves the tag and its existing signed-event/relay behavior owns
the event.

The authenticated Elembra API is:

| Endpoint | Contract |
| --- | --- |
| `POST /api/v1/applications/chat/attachments/prepare` | Files reauthorizes the selected ref and returns safe metadata plus the exact Buzz tag for client-side signing. |
| `POST /api/v1/applications/chat/attachments/preview` | Files reauthorizes the current Principal and returns safe preview metadata. |
| `POST /api/v1/applications/chat/attachments/open` | Files reauthorizes again and returns content through Elembra; it never redirects to a stored URL. |

The existing authenticated Files list/picker APIs remain the selection surface;
Chat does not create a second browser or ACL model. The selected ref preserves
the exact file version. A newer version is not substituted. Deleted,
unavailable, unauthorized, malformed, and cross-tenant refs produce the same
`resource unavailable` response where existence hiding applies. A Files outage
degrades these attachment calls without affecting Buzz event publication or
history.

The security-critical path is current authorization at every preview/open
request. Chat membership is never consulted as Files authority, and Files
access never grants Buzz channel access. No attachment event is copied into an
Elembra message database and no outbox implementation was added.

The implementation tests signed-event tag round-tripping, credential absence,
malformed/duplicate/extended tag rejection, and standard-tag isolation. The
existing Files owner and SourceAuthorizer tests cover tenant/workspace scope,
delegation, permission denial, version selection, and fail-closed routing.

The live vertical-slice proof used a disposable real RustShare server backed by
Postgres/RustFS and a disposable real Buzz relay. It passed: A selected an
existing Files resource; the signed Buzz event stored the exact ResourceRef;
B previewed and opened it; a Chat-only Principal and a different-tenant
Principal were denied; Files access was revoked;
the historical Buzz event was retrieved unchanged with a valid signature; B's
subsequent preview/open requests were denied; and deletion returned the safe
unavailable result. Temporary users, file rows, relay containers, and relay
database were removed afterward.

An isolated Buzz relay proof on 2026-08-10 accepted and returned a signed kind
`1` event carrying the exact `elembra-ref` tag (`event_id`
`699ff87ec08e9e4d68a961f217e0f3da48e8812ec4314e0ab60a818b8ec945c`); the
retrieved event signature verified. The temporary client and relay database
were destroyed afterward. The later vertical-slice proof above supplied the
authenticated Files fixture and covered the complete attach/revoke behavior.

### Current-state findings

The current Buzz `main` branch checked for this foundation (head `f53bbd1152464ecbb1de495e2d1d959e156138f0`)
uses Nostr x-only public keys as user/agent identities. Events are signed Nostr
events; relay authentication is NIP-42 (`crates/buzz-auth/src/nip42.rs`) and
stateless HTTP authentication is NIP-98 (`crates/buzz-auth/src/nip98.rs`). Buzz
derives the community from the relay host and scopes membership rows by that
community. Relay-wide admission/revocation is already a generic public NIP-43
surface: signed kind 9030 adds a member and kind 9031 removes one, with admin or
owner authorization (`crates/buzz-relay/src/handlers/relay_admin.rs`). The
`buzz-admin` CLI is a separate operator path that writes local relay state and
must not be used by Elembra. Community moderation commands (9040–9044) are a
different ban/timeout surface.

The RustShare branch already has the trusted `PrincipalContext` contract in
`backend/crates/resource-auth/src/principal.rs`, but it had no Chat identity
binding or workspace/community admission contract. The new
`resource-auth::chat_identity` module is deliberately policy-only: it does not
read Buzz tables, store private keys, or claim to be the durable adapter.

### Decisions

* A Principal and a Buzz key are different identities. OIDC proves Principal
  authentication only; a NIP-42 AUTH event proves possession of the candidate
  Buzz key.
* Binding challenges are tenant/Principal/key scoped, short-lived, relay scoped,
  and one-use. The verifier checks kind, Schnorr signature, pubkey, challenge,
  relay, and timestamp before consuming the challenge.
* The default device model is one user-controlled Buzz identity shared through
  the user's encrypted key backup/device enrollment. Device-specific keys are
  deferred: Buzz's signed history is pubkey-continuous, and silently changing
  pubkeys would make rotation/recovery and attribution a different product.
  A second key therefore requires `rotation_of` to name the current binding;
  the transaction revokes its old admission before activating the replacement.
  A second active key without explicit rotation is rejected by the tenant/
  Principal uniqueness constraint.
* Elembra stores only public binding metadata and admission state. Sovereign
  mode never stores a plaintext Buzz private key. Managed/KMS identities, if
  added later, must be an explicit identity mode and must not reuse this path.
* Admission requires all of: active Principal, Chat application access, active
  binding, active tenant-scoped workspace/community mapping, and active Buzz
  admission. Cryptographic validity is not authorization.
* Agent identities are separate Principals and separate Buzz keys. A delegation
  may authorize Elembra actions but never supplies or aliases a human key.

### Contracts implemented

`rustshare-resource-auth` now provides `BindingChallenge`,
`ChatIdentityBinding`, `WorkspaceCommunityMapping`, `BuzzAdmission`, and
`authorize_admission`. Persistence, HTTP handlers, and the Buzz adapter must
implement these contracts at a trusted boundary; a serialized contract is not
client authorization proof.

The Elembra implementation now exposes the following trusted boundary:

| Endpoint | Contract |
| --- | --- |
| `POST /api/v1/admin/applications/chat/workspaces/{workspace_id}/community` | Admin-only explicit tenant mapping; names and URLs are not inferred. |
| `POST /api/v1/applications/chat/identity-binding/challenge` | Authenticated active Principal + enabled Chat Application + active mapping → five-minute challenge. |
| `POST /api/v1/applications/chat/identity-binding/verify` | Authenticated same Principal + NIP-42 proof → atomic one-use challenge consumption and active binding. |
| `POST /api/v1/applications/chat/admission` | Active binding + mapping + Chat access → Elembra admission row and durable Buzz bridge operation (`202 queued`). |

The migration adds durable bindings, challenges, mappings, and admissions.
Admission/revocation requests use the existing transactional
`integration_outbox` rather than a second queue. A database trigger revokes local binding
and admission state and queues Buzz revocation whenever a user is disabled,
including SCIM/admin paths. Queue delivery is intentionally not reported as
Buzz success: until the separately provisioned Buzz bridge identity has
submitted and received success for NIP-43 9030/9031, `queued` is not
`admitted`.

The server includes an optional `io.elembra.chat.buzz-bridge.v1` outbox
consumer. It is enabled only when `RUSTSHARE_CHAT_BRIDGE_SECRET_KEY` contains
the separately provisioned Buzz service/admin key. It consumes canonical
CloudEvents, authenticates with NIP-42 when the relay challenges the session,
publishes kind 9030/9031, and reports relay failures as retryable outbox
failures. It never reads or stores a human binding private key. The consumer
must be configured before publishing events; the outbox's eager fan-out does
not create obligations for an unregistered consumer retroactively.

### Live relay proof

On 2026-08-10, an isolated Buzz relay was started with
`BUZZ_REQUIRE_RELAY_MEMBERSHIP=true`, a throwaway owner/service key, and a clean
database. A temporary client using the same NIP-42 and NIP-43 wire contract
completed the lifecycle: kind 9030 admission was accepted, the admitted key
published a signed kind 1 message, kind 9031 revocation was accepted, the
previously signed event still verified cryptographically, and a subsequent
publish by that old key was denied. The temporary client held the human key only
in memory; it was not passed to RustShare or persisted.

### Sequence diagrams

```mermaid
sequenceDiagram
  participant P as Principal
  participant E as Elembra
  participant K as Client key custody
  participant B as Buzz relay
  P->>E: OIDC login -> PrincipalContext
  E-->>K: tenant + principal + pubkey challenge
  K->>B: NIP-42 AUTH(kind 22242, nonce, relay)
  K-->>E: signed AUTH proof
  E->>E: verify, consume once, activate binding
  E->>B: admit pubkey to mapped community
  K->>B: signed normal message
  B-->>K: accepted; Buzz owns event history
```

```mermaid
sequenceDiagram
  participant P as Principal
  participant E as Bridge
  participant B as Buzz
  P->>E: enter workspace
  E->>E: tenant + workspace + binding + access checks
  E->>B: NIP-98/API request or NIP-42 relay session
  B-->>E: community-scoped result
  P->>E: disable Principal
  E->>B: revoke community admission (retryable outbox)
  P->>B: old key signs event
  B-->>P: denied for revoked community; old history remains valid
```

The same binding/admission contracts cover first identity creation, existing-key
binding, additional-device enrollment, lost-device revocation, recovery, and
rotation. Creation and recovery happen in the client; lost/all-device recovery
cannot restore the exact old pubkey without an explicit encrypted backup or
escrow, and the system must say so rather than impersonate the user.

The nine required lifecycle cases are specified as follows:

```mermaid
sequenceDiagram
  participant U as User/client
  participant E as Elembra
  participant B as Buzz
  U->>E: 1. First login via OIDC
  E-->>U: PrincipalContext; no Buzz key
  U->>U: 2. Generate/store Buzz key locally
  U->>E: 3. Existing-key challenge + NIP-42 proof
  E->>E: Consume nonce; activate binding
  U->>E: 4. Enter mapped workspace
  E->>E: Check Principal, Chat, mapping, binding
  E->>B: 5. Queue generic community admission
  B-->>E: Admission result (upstream adapter contract)
  U->>B: 6. Sign and post normal Buzz event
  B-->>U: Signed event accepted; Buzz owns history
  U->>E: 7. Enroll additional device using encrypted key backup
  U->>E: 8. Report loss/compromise; rotate and revoke old binding
  E->>B: 9. Revoke old community admission
```

Lost-device recovery creates a new key unless the user restores an encrypted
backup. Full recovery without escrow cannot reconstruct the old identity. Key
rotation preserves old signed history and denies the old key future admission.

### Threat model and failure behavior

| Threat | Required result |
| --- | --- |
| IdP or Elembra DB compromise | No plaintext sovereign private key and no ability to sign as a user. |
| Wrong tenant/principal/key, replay, expiry, bad relay/signature | Binding fails closed; challenge is not reusable. |
| Malicious tenant administrator | Cannot cross tenant boundaries or bind a key without its proof. Audit and rotation semantics remain explicit. |
| Device theft/key compromise | Revoke binding/admission; signed history remains valid; rotate to a new key when custody permits. |
| Disabled Principal | Bridge revokes admission; a mathematically valid old signature cannot regain workspace access. |
| Buzz/Elembra/bridge outage | No new admission or access is granted on stale success; signed Buzz history remains Buzz-owned; revocation delivery is retryable and audited. |
| Agent key confusion | Agent has its own Principal/key; human key is never used as an agent signer. |

### Upstream boundary

Use existing Buzz NIP-42/NIP-98 and relay-host community semantics directly.
Elembra-specific tenant/workspace mapping and binding records remain downstream.
The minimal bridge can use Buzz's existing NIP-43 kind 9030/9031 commands over
an authenticated relay session. It requires a separately provisioned Buzz
bridge/service identity with admin/owner authority; it never uses a human's
private key. The bridge must select the relay from the Elembra mapping, verify
the host-derived community, use an idempotency key from the queue, and record
the relay response. No upstream Buzz code is required for this slice. An
upstream SDK helper for these commands is optional future ergonomics, not a
security dependency. NIP-42 remains the user relay-session authentication
path and normal events remain signed by the user's Buzz key.

The wire mapping is fixed by Buzz: admission emits kind `9030` with a `p` tag
containing the bound x-only pubkey; revocation emits kind `9031` with the same
tag. The relay host, not an Elembra-supplied tenant or community string, selects
the Buzz community.

### Remaining implementation slice

The Elembra-side foundation and optional bridge consumer are implemented:
durable binding/challenge/mapping/admission tables, trusted routes, NIP-42
verification, canonical CloudEvents, and NIP-43 relay commands. The live Buzz
relay proof is complete: bind/admit, sign and publish, disable, revoke, and
verify that old signed history remains valid while new publication is denied.
The current unit tests cover cryptographic proof, replay/expiry/key/relay
failures, tenant mismatch, inactive Principal, revoked binding, and service-key
command signing.

- [x] Principal↔Buzz pubkey binding contract exists.
- [x] Workspace↔Buzz community mapping contract exists.
- [x] OIDC design explicitly preserves Buzz signing authentication.
- [x] Offboarding revokes Chat admission independently of key validity.
- [x] Files attachments use ResourceRefs and read-time reauthorization.
- [ ] Memory projection preserves Buzz provenance/source ownership.
- [x] No shared Buzz/Elembra private database access exists.
- [ ] Agent Chat identity and delegated Elembra authority remain separate.
- [ ] Buzz upstream/delta compatibility tests are defined before large fork changes.
