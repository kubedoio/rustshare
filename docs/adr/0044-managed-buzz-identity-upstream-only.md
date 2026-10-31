# ADR-0044: Elembra-Managed Chat Identities on Unmodified Upstream Buzz

- **Status:** Proposed for architectural and security review — does not amend earlier ADRs or authorize implementation/rollout until accepted
- **Date:** 2026-10-31
- **Repository:** `kubedoio/rustshare`
- **Would amend if accepted:** ADR-0034 (key custody), ADR-0035 (source authorization), ADR-0036 (zero-config bootstrap); preserves their tenant/resource-safety requirements
- **Related:** ADR-0040 (organization/tenant), ADR-0041 (groups/authorization), #196, #214, #215, #323, #424

## Context and selected product decision

Elembra should provide a native Chat experience without making users create, unlock, back up, or bind Nostr identities manually. Chat is the product goal; Buzz supplies the messaging engine and unmodified desktop clients. A user with an Elembra account should be usable in Chat immediately after asynchronous provisioning completes. Each user can explicitly reveal/export their Buzz identity private key to use stock Buzz clients.

Today Elembra's `frontend/src/lib/chat/keys.ts` creates/holds human signing keys in the browser, and ADR-0034/0035 prohibit server-side human keys. The shipped compatibility file pins `ghcr.io/kubedoio/buzz-elembra` at `v1alpha1`; its signed access-check, channel registry, signed state and community discovery APIs are fork-specific. The current v1alpha2 replacement implementation/evidence also depends on downstream Buzz source, and must not be mistaken for upstream capability. Deploying stock Buzz without changing the Elembra adapter would break authorization or conformance gates.

Verified source snapshot on 2026-10-31: `block/buzz` `eafd77e9f6e96052476bdd88be732c046fdb7ab3` (main; no relay release tag selected) documents NIP-29 kind-9 channels/discovery, NIP-42 and NIP-43 kinds 9030/9031. Membership enforcement is an explicit relay setting; active-session revocation is not established by the docs. `block/buzz-app` `662e91800af8204ec0e492522b8f23ef23b19baa` (main) is a separate Buzz 1.0 beta desktop client, and its README documents existing-`nsec` import. This is documentation evidence, not an interoperability result. Pin and test relay and app separately. Neither source provides the fork's Elembra HTTP access/state contracts or proves current-source authorization, v1alpha2 tombstones, the replacement release gate, or instant session disconnect.

This proposal conflicts with the current ADR-0034 rule that Elembra never holds a human signing key and ADR-0035's live signed authorization API. Those rules remain operative until human review accepts this ADR and later implementation gates pass. ADR-0036's rejected relay operator API remains rejected; managed NIP-43 membership must use a separately controlled owner/admin identity over the documented protocol, not revive that API.

## Decision

1. **Proposed managed identity for new Chat identities.** If accepted and implemented, Elembra generates one secp256k1/Nostr keypair per active `(tenant, principal, identity_generation)` using a cryptographic RNG. It stores the private key only as encrypted key material in a tenant/principal-bound vault protected by configured KMS envelope encryption. A separate typed-intent signing service may temporarily decrypt it for actions authorized by the authenticated user. This is explicit custodial identity, not client-held sovereignty or end-to-end key exclusivity.
2. **Elembra owns accounts; Buzz owns communication.** A Buzz "user" is a public key admitted to a relay/community, not a username/password row inserted into a shared database. Keep Elembra Principal ↔ Nostr pubkey and tenant ↔ relay/host mappings in Elembra; the proposed provisioning path is NIP-43 kind 9030 signed by a separately protected relay owner/admin key. Never read/write Buzz's private database. The relay's default and member-enforcement configuration still need a real-source POC.
3. **Deploy only unmodified upstream Buzz.** Pin official source commit or release and digest-verified OCI artifact; an Elembra-owned build from *unchanged* official source is allowed if provenance is documented. No `kubedoio/buzz` patches, custom relay endpoints, middleware, or code forks in the upstream-only profile. Default to one independently isolated Buzz relay/community per Elembra tenant until multi-community host isolation and operator provisioning pass equivalent proof.
4. **Elembra web Chat signs server-side.** Every send and authenticated read is authorized through the active Elembra session, tenant, Principal, user binding and current mapping. The signing service signs only a validated allowlisted Chat intent, never arbitrary client-submitted events or API-provided key IDs. Buzz still verifies event signatures and enforces its native channel/community authorization.
5. **Standard Buzz clients remain direct clients.** User Settings exposes the exact community URL and an explicitly requested `nsec` export, masked by default and guarded by a fresh step-up authentication event. Optionally produce NIP-49 `ncryptsec` only after round-trip compatibility tests; never call the custom Elembra JSON backup a Buzz-compatible credential. Exported keys are permanently copyable; Elembra cannot revoke a private key cryptographically.
6. **The source of truth stays Buzz.** Elembra must not recreate Buzz channel, DM, thread or membership ACLs. Gate downstream Chat views and Chat-backed Memory on an authenticated, current upstream Buzz read under the requesting user's managed identity. If the current upstream release cannot establish equivalent per-channel authorization, disable that feature until proven. No observation index, cached ACL, message snapshot, or local toggle may turn a denial into an allow.
7. **Availability and rollout are explicit.** Reconciliation, outbox and lifecycle operations must be idempotent; failed provisioning leaves Chat pending/unavailable, never open. Keep legacy fork-backed deployments on their existing pin until a separately approved migration and parity gate passes. No in-place weakening of historical replacement guards or production schema changes merely to make the proposed profile start.

## Security model and accepted tradeoffs

- Elembra backend/KMS administrators and a compromised Elembra signing service potentially can impersonate managed users. Server custody therefore changes the compromise blast radius and the confidentiality model, including for DMs. This is an accepted **product direction**, not a claim of security equivalence with client-held signing keys.
- Keys are wrapped with a managed encryption service/KMS-backed KEK; each identity uses a versioned envelope, AEAD authenticated with tenant ID, principal ID, public key and generation. No plaintext key in database/logs/traces/jobs/exports; never store plaintext in general session or request state. Minimize plaintext lifetime, access to the signing service, and egress. A DB backup alone must not include the KEK.
- Decryption happens for authorized signing and explicit user export only. A normal Chat HTTP response never includes secret material. Require step-up, CSRF/session protection, explicit action and audit for private-key export. Prefer short-lived single-use export grants and `Cache-Control: no-store`; prevent browser auto-prefetch/analytics and secret capture by screenshots where practical.
- Distinct relay owner/operator, membership-admin, service/agent and human keys. Member keys must never inherit owner/admin role. Separate tenant keys and domains. KMS loss can make recovery impossible; tested restore of DB + KMS bindings is mandatory.
- Exported identities may remain usable in an independent client after Elembra logout or account disable until Buzz membership is removed and existing sessions are cut off. Stock Buzz revocation convergence, reconnect behavior, live-session denial and roster limitations must be measured; do not claim instant revocation without proof. Never reuse an exported old private key for a new person.
- Ephemeral server signing does not imply message confidentiality from Elembra. User documentation must say that Elembra holds and can use their Chat signing identity.

## Existing work and migration policy

- Keep existing client-held identities usable through an explicit `legacy_client_held` mode. For conversion, require the user to deliberately import/upload their existing key through a dedicated authenticated migration; verify that its derived public key equals the active binding, and encrypt immediately. **Never silently rotate a user's key** or misattribute historical messages.
- Existing M4.1b2 durable identity replacement remains protected by its current disabled/release guards. The new managed-identity flow cannot use a legacy `rotation_of` bypass. Migration of legacy keys, key replacement, forced offboarding, and old relay access must be separately accepted.
- In the inspected protected-main snapshot, `BUZZ_REPLACEMENT_V1ALPHA2_QUALIFIED=0`; `handlers/chat_identity.rs::replacement_initiation_enabled` and the runtime qualification checks continue to gate replacement. This proposal does not change those values or code.
- `ADR-0035` current live-source authorization and `ADR-0036` signed discovery mechanisms cannot simply be deleted. Prove stock upstream equivalents via protocol-level live reads and independently checked host/relay identity, or mark dependent features unsupported/fail-closed for the upstream-only profile.
- If stock Buzz cannot meet a **required** product authorization or revocation property, choose documented scope reduction, an external authorization boundary that is also enforced for direct Buzz clients, or wait for an accepted upstream change. Never silently fork Buzz or pretend an Elembra web-only check constrains standalone Buzz clients.

## Options considered

| Option | Decision | Rationale |
|---|---|---|
| A: Elembra-managed Nostr keys | **Chosen default** | Zero-friction product and deliberate export to upstream clients |
| B: browser-only Nostr keys | Maintain as legacy / optional future mode | Better self-custody but manual recovery and complicated multi-device onboarding |
| Reimplement Buzz messaging in Elembra | Rejected | Duplicates protocol, event store, clients, and membership |
| Maintain a downstream Buzz fork | Rejected for target architecture | Couples releases and security changes to custom relay implementation |
| Allow any client to post to open relay | Rejected | Breaks tenant admission and offboarding boundaries |

## Consequences and go/no-go

**Positive:** Native Elembra Chat, one user lifecycle, upstream desktop interoperability, one release source for Buzz, no per-user setup. **Negative:** custodial key-management liability, export/revocation limitations, potential performance/cost of per-user relay sessions and current product features that cannot be carried to stock Buzz unchanged.

**GO for migration only when:** upstream source/digest provenance and unchanged-source proof; active-user credential, cross-tenant, NIP-42/NIP-43 and channel tests; Buzz desktop import round-trips; two-way messaging; offboarding/reconnect tests; permission-aware Chat Memory denial tests; backup/KMS restore; clear failed-compatibility behavior; independent security review; and approved per-tenant cutover plan. A partial feature-compatible POC is not production acceptance. Current evidence and the file/function migration matrix are in `docs/plans/elembra-buzz-upstream-compatibility-matrix.md`.

## References

- https://github.com/kubedoio/rustshare/blob/main/docs/adr/0034-elembra-chat-buzz-boundary.md
- https://github.com/kubedoio/rustshare/blob/main/docs/adr/0035-buzz-source-authorization-gateway.md
- https://github.com/kubedoio/rustshare/blob/main/docs/adr/0036-elembra-chat-zero-config-bootstrap.md
- https://github.com/kubedoio/rustshare/blob/main/config/buzz-compatibility.env
- https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/NOSTR.md
- https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/ARCHITECTURE.md
- https://github.com/block/buzz-app/blob/662e91800af8204ec0e492522b8f23ef23b19baa/README.md
