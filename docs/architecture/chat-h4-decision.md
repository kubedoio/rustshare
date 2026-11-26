# H4 Chat authority decision

**Snapshot:** Elembra protected `main` `b9baaaefe59c9b0fd4f42abddd4763e576617066`; RustChat `main` `b195719d40212fbfba227ce9266ac2420572860c`, verified 2026-11-26. The Buzz compatibility pin and deployment flags were rechecked on b9 and are unchanged. This record summarizes the architecture decision and gates. It is not deployment inventory, security acceptance, or migration authorization. See [the detailed retain/migrate/drop comparison](chat-retain-migrate-drop.md).

## Decision

**Approved strategic target: A, managed Elembra identities on unmodified upstream Buzz**, as already recorded in [ADR-0044](../adr/0044-managed-buzz-identity-upstream-only.md) and [ADR-0046](../adr/0046-managed-nip29-publishing-authorization.md). These decisions authorize neither real-user key custody/signing nor production integration, publishing, release, or migration; independent custody security acceptance remains required.

**Current operating posture: D.** Preserve the supported fork-backed Buzz deployment with bounded B maintenance until the accepted upstream target's independent gates pass. Keep maintenance to security/compatibility fixes, reviewed immutable pins, and conformance/regression tests. Do not add fork-only product surface. C is not recommended on current evidence. No new A-versus-D owner choice is pending.

## Options

| Option | Identity and authorization | Features, history and Files | Cost, risk, migration and rollback |
|---|---|---|---|
| **A. Unmodified upstream Buzz** | Approved target, but managed identity/custody, tenant mapping, and subject-bound authorization remain gated. Fork-only signed registry/gateway APIs are not stock upstream contracts. | Must prove channel, membership, history, search and Memory parity. Attachments must remain Files ResourceRefs and reauthorize on every open. | Removes downstream fork carry only after upstream compatibility; adds KMS/key recovery, offboarding, independent security and integration work. Requires per-tenant migration, signed-history parity, fresh restore and rollback evidence. No cutover is authorized. |
| **B. Existing fork-backed Buzz** | Matches the current Elembra Principal/workspace binding and current fork authority gateway. | Retains the existing signed event authority and supported workflows; Files and Memory stay derived/re-authorized. | Lowest transition cost, with ongoing fork patch/rebase and immutable-pin obligations. Maintain only security/compatibility and tests. No migration required now. |
| **C. RustChat authoritative** | RustChat implements local auth and GitHub/Google/generic OIDC, plus its own org/team/channel memberships. Elembra tenant/Principal mapping, grants, live revocation, and per-user Buzz identities are not integrated or qualified. | RustChat has posts, threads/replies, reactions, search, moderation and its own Files/S3 system. Its PostgreSQL history and files do not yet map to Elembra's Buzz, Files or Memory source-authority contracts. Its inspected Buzz bridge is outbound and uses a connection-level signer, not individual Nostr authorship. | A real, substantial alternative, not a drop-in. Adds a second product/release/security lifecycle and high integration cost. Requires data/identity mapping, event provenance preservation, attachment authorization, export, backup/restore and rollback proof. Do not migrate on current evidence. |
| **D. Retain current deployment and defer cutover** | Keeps current mappings and supported authorization unchanged. | No intentional behavior change; Buzz remains event/history authority, Files owns attachments, Memory remains derived. | Lowest immediate risk. Fork obligations continue; no migration, data rewrite or removal. This is the current posture while A's gates remain open. |

## Current supported Buzz configuration

Protected main pins `b3aaabf635d992b08b57773071686a92196884e1` in `config/buzz-compatibility.env`, image `ghcr.io/kubedoio/buzz-elembra@sha256:38c362bb37d15ea879634b0f56a867401df4e4e0074a84382dff6b07bb578b4e`, contract `v1alpha1`. The bundled runtime has Git disabled. Preserve this pin and configuration. `BUZZ_REPLACEMENT_V1ALPHA2_QUALIFIED=0`; do not bypass the release/runtime gate.

Elembra's accepted boundary is in [ADR-0034](../adr/0034-elembra-chat-buzz-boundary.md) and [ADR-0035](../adr/0035-buzz-source-authorization-gateway.md): Buzz owns signed chat events, channels/membership and chat retention; Elembra owns tenant/workspace/Principal grants; Files owns attachment resources; Memory owns derived catalog/index projections. The systems do not share a private database schema.

## RustChat evidence and limits

At the cited RustChat SHA, [ADR-005](https://github.com/kubedoio/rustchat/blob/b195719d40212fbfba227ce9266ac2420572860c/docs/adr/ADR-005-rustchat-core-and-buzz-integration.md) makes RustChat an independent authority and Buzz optional. Code includes OIDC flows (`backend/src/api/oauth/`, `backend/src/services/oidc_discovery.rs`), team/channel membership APIs (`backend/src/api/v4/teams/`, `backend/src/api/v4/channels/`), posts/reactions/search/files (`backend/src/api/v4/posts/`, `backend/src/api/v4/files.rs`) and an outbound Buzz connector (`backend/src/integrations/buzz/`). These are implemented product features, not proof of Elembra integration or acceptance.

The inspected bridge contract [`docs/integrations/buzz.md`](https://github.com/kubedoio/rustchat/blob/b195719d40212fbfba227ce9266ac2420572860c/docs/integrations/buzz.md) is outbound-only. `backend/src/integrations/buzz/events.rs` builds kind-9 events; `connector.rs` holds a resolved relay connection and signing key. RustChat owns its PostgreSQL history and S3 files. There is no accepted bidirectional history migration, Elembra Principal/tenant map, Files ResourceRef authorization adapter, Memory provenance boundary, or rollback plan. Repository inspection does not establish whether any RustChat instance or data is live.

## Gates before changing this posture

Keep these gates distinct; a workflow PASS is not independent security acceptance:

- **#309:** non-destructive live deployment, data, identities, clients, secrets and backup inventory; then owner-approved export/parity/restore/rollback plan.
- **#458:** preserve independent security acceptance as pending separately from its recorded execution PASS.
- **#442:** interactive unmodified Buzz Desktop/native-keychain interoperability.
- **#460:** upstream write authorization and publisher conformance; do not bypass its security block.
- **#215 and #323:** managed identity, key custody/recovery, revocation and rollout gates; preserve existing identity mappings and disabled replacement/publishing guards.

Until these gates are independently accepted, retain D, continue bounded B maintenance, and make no signer, identity, deployment, data, or migration change. Any reconsideration of C requires the owner to explicitly reopen the accepted A target.
