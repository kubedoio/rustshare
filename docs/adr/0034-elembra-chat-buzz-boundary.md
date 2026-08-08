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

- [ ] Principal↔Buzz pubkey binding contract exists.
- [ ] Workspace↔Buzz community mapping contract exists.
- [ ] OIDC design explicitly preserves Buzz signing authentication.
- [ ] Offboarding revokes Chat admission independently of key validity.
- [ ] Files attachments use ResourceRefs and read-time reauthorization.
- [ ] Memory projection preserves Buzz provenance/source ownership.
- [ ] No shared Buzz/Elembra private database access exists.
- [ ] Agent Chat identity and delegated Elembra authority remain separate.
- [ ] Buzz upstream/delta compatibility tests are defined before large fork changes.
