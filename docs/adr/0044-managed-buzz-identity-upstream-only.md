# ADR-0044: Elembra-Managed Chat Identities on Unmodified Upstream Buzz

- **Status:** Accepted target architecture and implementation planning; independent security approval, feature enablement and release remain gated
- **Date:** 2026-10-31
- **Repository:** `kubedoio/rustshare`
- **Amends by runtime profile:** ADR-0034 (key custody), ADR-0035 (source authorization), ADR-0036 (zero-config bootstrap); existing runtime behavior and tenant/resource-safety requirements are preserved
- **Related:** ADR-0040 (organization/tenant), ADR-0041 (groups/authorization), #196, #214, #215, #323, #424

## Context and selected product decision

**Adoption record (2026-10-31):** The project owner explicitly approved Option A as the target product architecture and authorized isolated Phase 2A implementation planning. This record adopts the architecture and profile-scoped amendments below. It is not an independent security review, approval to hold real-user keys, authorization to enable managed Chat, or release approval. Those gates remain explicit and require named reviewer evidence.

Elembra should provide a native Chat experience without making users create, unlock, back up, or bind Nostr identities manually. Chat is the product goal; Buzz supplies the messaging engine and unmodified desktop clients. A user with an Elembra account should be usable in Chat immediately after asynchronous provisioning completes. Each user can explicitly reveal/export their Buzz identity private key to use stock Buzz clients.

Today Elembra's `frontend/src/lib/chat/keys.ts` creates/holds human signing keys in the browser, and ADR-0034/0035 prohibit server-side human keys. The shipped compatibility file pins `ghcr.io/kubedoio/buzz-elembra` at `v1alpha1`; its signed access-check, channel registry, signed state and community discovery APIs are fork-specific. The current v1alpha2 replacement implementation/evidence also depends on downstream Buzz source, and must not be mistaken for upstream capability. Deploying stock Buzz without changing the Elembra adapter would break authorization or conformance gates.

Verified source snapshot on 2026-10-31: `block/buzz` `eafd77e9f6e96052476bdd88be732c046fdb7ab3` (main; no relay release tag selected) documents NIP-29 kind-9 channels/discovery, NIP-42 and NIP-43 kinds 9030/9031. Membership enforcement is an explicit relay setting; active-session revocation is not established by the docs. `block/buzz-app` `662e91800af8204ec0e492522b8f23ef23b19baa` (main) is a separate Buzz 1.0 beta desktop client, and its README documents existing-`nsec` import. This is documentation evidence, not an interoperability result. Pin and test relay and app separately. Neither source provides the fork's Elembra HTTP access/state contracts or proves current-source authorization, v1alpha2 tombstones, the replacement release gate, or instant session disconnect.

This accepted target architecture deliberately creates a profile-scoped exception to ADR-0034's no-server-human-key rule; it does not alter `legacy_client_held` or deployed `fork_v1alpha1` behavior. ADR-0035's live signed authorization API remains normative for the fork profile and is not claimed as stock upstream capability. Independent security acceptance and all runtime gates remain pending. ADR-0036's rejected relay operator API remains rejected; managed NIP-43 membership must use a separately controlled owner/admin identity over the documented protocol, not revive that API.

## Decision

1. **Accepted managed identity target.** When Gate I is approved, Elembra generates one secp256k1/Nostr keypair per explicitly enabled `(tenant, principal, identity_generation)` using a cryptographic RNG. It stores the private key only as encrypted key material in a tenant/principal-bound vault protected by configured KMS envelope encryption. Phase 2A may implement this storage only for tests and with the feature disabled. User signing requires separate typed intents, authorization and independent review. This is explicit custodial identity, not client-held sovereignty or end-to-end key exclusivity.
2. **Elembra owns accounts; Buzz owns communication.** A Buzz "user" is a public key admitted to a relay/community, not a username/password row inserted into a shared database. Keep Elembra Principal ↔ Nostr pubkey and tenant ↔ relay/host mappings in Elembra; eventual provisioning uses NIP-43 kind 9030 signed by a separately protected relay owner/admin key. Never read/write Buzz's private database. Phase 1 source/runtime evidence is recorded separately; it does not authorize real-user provisioning.
3. **Deploy only unmodified upstream Buzz.** Pin official source commit or release and digest-verified OCI artifact; an Elembra-owned build from *unchanged* official source is allowed if provenance is documented. No `kubedoio/buzz` patches, custom relay endpoints, middleware, or code forks in the upstream-only profile. Use exactly one independently isolated Buzz relay/community per Elembra tenant.
4. **Elembra web Chat signs server-side.** Every send and authenticated read is authorized through the active Elembra session, tenant, Principal, user binding and current mapping. The signing service signs only a validated allowlisted Chat intent, never arbitrary client-submitted events or API-provided key IDs. Buzz still verifies event signatures and enforces its native channel/community authorization.
5. **Standard Buzz clients remain direct clients.** User Settings exposes the exact community URL and an explicitly requested `nsec` export, masked by default and guarded by a fresh step-up authentication event. Optionally produce NIP-49 `ncryptsec` only after round-trip compatibility tests; never call the custom Elembra JSON backup a Buzz-compatible credential. Exported keys are permanently copyable; Elembra cannot revoke a private key cryptographically.
6. **The source of truth stays Buzz.** Elembra must not recreate Buzz channel, DM, thread or membership ACLs. Gate downstream Chat views and Chat-backed Memory on an authenticated, current upstream Buzz read under the requesting user's managed identity. If the current upstream release cannot establish equivalent per-channel authorization, disable that feature until proven. No observation index, cached ACL, message snapshot, or local toggle may turn a denial into an allow.
7. **Availability and rollout are explicit.** Future reconciliation, outbox and lifecycle operations must be idempotent; failed provisioning leaves Chat pending/unavailable, never open. Keep legacy fork-backed deployments on their existing pin until a separately approved migration and parity gate passes. No in-place weakening of historical replacement guards or production schema changes merely to make the new profile start.

## Security model and accepted tradeoffs

- Elembra backend/KMS administrators and a compromised Elembra signing service potentially can impersonate managed users. Server custody therefore changes the compromise blast radius and the confidentiality model, including for DMs. This is an accepted **product direction**, not a claim of security equivalence with client-held signing keys.
- Keys are wrapped with a KMS-backed KEK; each identity uses a versioned envelope with AEAD additional authenticated data containing version, tenant ID, principal ID, identity ID, public key and generation. No plaintext key in database, logs, traces or jobs. Plaintext may exist only during authorized signing or explicit user export under the managed identity contract. Minimize its lifetime and signing-service access. A DB backup alone must not include the KEK.
- Decryption happens for authorized signing and explicit user export only. A normal Chat HTTP response never includes secret material. Require step-up, CSRF/session protection, explicit action and audit for private-key export. Prefer short-lived single-use export grants and `Cache-Control: no-store`; prevent browser auto-prefetch/analytics and secret capture by screenshots where practical.
- Distinct relay owner/operator, membership-admin, service/agent and human keys. Member keys must never inherit owner/admin role. Separate tenant keys and domains. KMS loss can make recovery impossible; tested restore of DB + KMS bindings is mandatory.
- Exported identities may remain usable in an independent client after Elembra logout or account disable until Buzz membership is removed and existing sessions are cut off. Stock Buzz revocation convergence, reconnect behavior, live-session denial and roster limitations must be measured; do not claim instant revocation without proof. Never reuse an exported old private key for a new person.
- Ephemeral server signing does not imply message confidentiality from Elembra. User documentation must say that Elembra holds and can use their Chat signing identity.

## Accepted v1 messaging scope

The project owner accepted this deliberately narrow initial scope:

- NIP-29 channel messaging is in scope for the upstream profile.
- Native Buzz private-channel DMs are in scope for v1. Phase 1 protocol evidence proves relay-side participant access denial and protocol-client messaging; stock desktop discovery and bidirectional DM interoperability remain unverified and are tracked by issue #442. This scope decision does not claim desktop acceptance.
- NIP-17 encrypted DMs (NIP-44 encrypted seals and NIP-59 gift wraps) are excluded from initial v1 and must remain disabled. This is a separate protocol from Buzz's hidden NIP-29 channels: hidden metadata and relay ACLs do not provide end-to-end message encryption. Phase 1's valid randomized-timestamp NIP-17 event was rejected by stock Buzz; that historical failure remains visible as out-of-scope evidence and is not counted as a required v1 acceptance test.
- Managed mode never creates a private key in the user's browser. Browser key creation remains only in the existing legacy mode until a separately approved migration.
- No upstream Buzz source modification, endpoint, middleware, or private database access is allowed.

Native NIP-29 private channels rely on Buzz's relay membership and channel access controls. They are visible to the relay operator and are not equivalent to NIP-17 end-to-end encryption. Excluding NIP-17 from v1 is a product-scope decision, not evidence that the randomized timestamp failure was fixed. The pinned desktop's actual private-channel DM discovery and send/receive behavior remains a runtime acceptance gate; source inspection and protocol-client-only checks do not close it.

## Explicit custody consequences requiring independent security acceptance

Selecting Option A means Elembra controls encrypted user signing keys and the signing service can technically sign as a user. This is a deliberate custodial model and is not end-to-end exclusive user custody. Implementation requires KMS-backed envelope encryption with tenant/principal/identity/generation AAD; typed, scoped signing permissions that reject arbitrary event signing; separate owner/admin and user keys; least-privilege service access; audit of every signing and export; step-up and no-store export controls; tested key recovery and database-plus-KMS backup/restore; documented user export and recovery policy; and incident/offboarding procedures. Loss of required KMS material may make identities unrecoverable. Once a user exports an `nsec`, Elembra cannot cryptographically revoke that copy; relay removal and session termination are separate, eventually effective controls and cannot invalidate an offline key.

ADR-0034's no-server-human-key rule and ADR-0035's accepted live authorization gateway remain normative for existing `legacy_client_held` and `fork_v1alpha1` deployments. For a separately configured future `upstream_managed` profile, this adopted ADR explicitly scopes those rules as described in the profile amendments: custody is managed, the relay is unmodified upstream, subject-bound authorization and tenant isolation remain mandatory, and fork-only APIs are not carried forward. ADR-0036's rejected operator database/API bootstrap remains rejected; a separately protected owner/admin key may use upstream NIP-43 only. This architecture adoption authorizes an inert Phase 2A vault implementation; it does not authorize provisioning, signing, export, production integration or deployment.

## Runtime profile amendments to ADR-0034/0035/0036

- `legacy_client_held` and `fork_v1alpha1` retain their existing identity, authorization, bootstrap, pins, migrations and runtime behavior. No global rule is weakened and no existing user is silently moved.
- `upstream_managed` is a new, separately selected profile built only on unmodified upstream Buzz. Its eventual managed user keys are Elembra-custodied; the signing service can technically impersonate those identities. The profile still uses Buzz as messaging and channel-authorization authority and must fail closed when the current upstream protocol cannot prove subject-bound access.
- ADR-0035's signed `BuzzAuthority` endpoints remain required for currently deployed fork mode. Upstream mode must not call them or emulate them with Elembra ACLs; unproven timeline, reconciliation and Chat-backed Memory reads stay disabled.
- ADR-0036's fork-specific signed community discovery remains unchanged for fork mode. Upstream mode requires an independently trusted deployment mapping and standard NIP-43 membership operations; it must not infer community trust from unauthenticated NIP-11 or assume a stock operator API exists.

## Tenant endpoint exception design — independent review pending

The accepted topology is one isolated relay/community per tenant. Strict public-destination validation remains the default. A separately typed `trusted_service` mapping may support bundled or private-network relays only after independent review and implementation of all of the following: immutable orchestrator/service identity; administrator-approved exact host, port, community, relay key/provenance and endpoint allowlist; DNS answers constrained to that allowlist and pinned to the actual socket; TLS with exact hostname and pinned peer identity; redirects and environment proxy overrides disabled; network-level egress policy that permits only the approved relay destinations; connection/session fencing by tenant and mapping generation; and negative SSRF, DNS-rebinding, substitution and cross-tenant tests. No user-controlled private endpoint or broad RFC1918/loopback allowance is permitted. Until approved and implemented, the exception is disabled.

**Target architecture decision:** `ACCEPTED — project owner direction recorded 2026-10-31.` This is product/architecture direction only. **Independent custody security acceptance remains PENDING** and must record reviewer identity, role, date, scope and conditions before any real-user key is generated, decrypted for signing, or exported. PR approval alone is not that security acceptance.

## Existing work and migration policy

- Keep existing client-held identities usable through an explicit `legacy_client_held` mode. For conversion, require the user to deliberately import/upload their existing key through a dedicated authenticated migration; verify that its derived public key equals the active binding, and encrypt immediately. **Never silently rotate a user's key** or misattribute historical messages.
- Existing M4.1b2 durable identity replacement remains protected by its current disabled/release guards. The new managed-identity flow cannot use a legacy `rotation_of` bypass. Migration of legacy keys, key replacement, forced offboarding, and old relay access must be separately accepted.
- In the inspected protected-main snapshot, `BUZZ_REPLACEMENT_V1ALPHA2_QUALIFIED=0`; `handlers/chat_identity.rs::replacement_initiation_enabled` and the runtime qualification checks continue to gate replacement. This proposal does not change those values or code.
- `ADR-0035` current live-source authorization and `ADR-0036` signed discovery mechanisms cannot simply be deleted. Prove stock upstream equivalents via protocol-level live reads and independently checked host/relay identity, or mark dependent features unsupported/fail-closed for the upstream-only profile.
- If stock Buzz cannot meet a **required** product authorization or revocation property, choose documented scope reduction, an external authorization boundary that is also enforced for direct Buzz clients, or wait for an accepted upstream change. Never silently fork Buzz or pretend an Elembra web-only check constrains standalone Buzz clients.

## Options considered

| Option | Decision | Rationale |
|---|---|---|
| A: Elembra-managed Nostr keys | **Accepted target architecture** | Zero-friction product and deliberate export to upstream clients; production default remains gated |
| B: browser-only Nostr keys | Maintain for existing `legacy_client_held` mode | Better self-custody but manual recovery and complicated multi-device onboarding |
| Reimplement Buzz messaging in Elembra | Rejected | Duplicates protocol, event store, clients, and membership |
| Maintain a downstream Buzz fork | Rejected for target architecture | Couples releases and security changes to custom relay implementation |
| Allow any client to post to open relay | Rejected | Breaks tenant admission and offboarding boundaries |

## Consequences and go/no-go

**Positive:** Native Elembra Chat, one user lifecycle, upstream desktop interoperability, one release source for Buzz, no per-user setup. **Negative:** custodial key-management liability, export/revocation limitations, potential performance/cost of per-user relay sessions and current product features that cannot be carried to stock Buzz unchanged.

**Phase 2A implementation GO:** the owner has accepted the target architecture and NIP-29-only v1 scope; only an isolated, disabled-by-default vault may be developed. Independent cryptographic/security review, protected CI and review are mandatory before merge. No user provisioning, user signing, export, relay integration, production migration or deployment is permitted in Phase 2A. **Phase 2B integration GO** requires the inert vault implementation to pass Gate V and an independent security review; stock desktop acceptance in issue #442; production-wired, independently accepted tenant mapping/transport including any trusted-service exception; and live NIP-42/NIP-43/channel/private-DM tests. **Production release/cutover GO** is separately gated on upstream source/digest provenance; backup and KMS recovery; offboarding/reconnect and tenant tests; permission-aware Chat Memory denial tests or explicit disabling; rollback and per-tenant migration plans; and security approval. A partial feature-compatible POC is not production acceptance. Current evidence and the file/function migration matrix are in `docs/plans/elembra-buzz-upstream-compatibility-matrix.md`.

## References

- https://github.com/kubedoio/rustshare/blob/main/docs/adr/0034-elembra-chat-buzz-boundary.md
- https://github.com/kubedoio/rustshare/blob/main/docs/adr/0035-buzz-source-authorization-gateway.md
- https://github.com/kubedoio/rustshare/blob/main/docs/adr/0036-elembra-chat-zero-config-bootstrap.md
- https://github.com/kubedoio/rustshare/blob/main/config/buzz-compatibility.env
- https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/NOSTR.md
- https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/ARCHITECTURE.md
- https://github.com/block/buzz-app/blob/662e91800af8204ec0e492522b8f23ef23b19baa/README.md
