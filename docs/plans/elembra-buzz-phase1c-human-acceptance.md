# Elembra Buzz Phase 1c — Human Acceptance Decisions

**Status: pending authorized human decisions.** PR approval or merge does not accept these security and product decisions. Signing below records design acceptance only; it does not prove that production enforcement has been implemented or tested.

## A. ADR-0044 custody model — Option A

Approve managed Nostr identities whose encrypted signing keys are controlled by Elembra. This grants the backend signing service technical capability to sign as a user; a compromised service or authorized operator could impersonate managed users. Accept that custody tradeoff only with KMS-backed envelope encryption, tenant/principal/identity/generation-bound AEAD, typed and scoped signing intents, least-privilege service permissions, separation of user and relay-admin keys, audited key lifecycle and export, backup/restore and tested recovery. A user's exported `nsec` is an independent copy and cannot be cryptographically revoked; recovery requires a new identity/key and corresponding relay membership transition.

**Decision:** ☐ Accept Option A and these required controls  ☐ Reject / revise

Authorized architecture/security approver: ____________________  Date: __________

## B. Initial messaging scope

Proposed v1 scope is NIP-29 channel messaging and native NIP-29 private-channel DMs only. Native private-channel DMs use Buzz's hidden NIP-29 channel membership and ACL behavior; they are not end-to-end encrypted NIP-17 gift-wrap DMs. NIP-17 is excluded and must remain disabled in v1. The recorded randomized-timestamp NIP-17 relay rejection remains a historical protocol failure, outside the proposed v1 acceptance suite. Managed mode creates no private key in the user's browser. Desktop channel and DM interoperability still requires the manual acceptance procedure in `scripts/upstream-buzz-poc/desktop-acceptance.md`; the current GUI assertions are unverified.

**Decision:** ☐ Accept this NIP-29-only scope  ☐ Reject / revise

Authorized product/security approver: ____________________  Date: __________

## C. Tenant-to-relay mapping security contract

Proposed v1 topology is one isolated relay/community per tenant. The trusted control plane binds tenant and workspace to an exact relay URL, canonical host, community, deployment provenance and pinned relay identity. DNS is validated and pinned per connection; TLS chain and hostname are verified; user-controlled destinations, redirects, proxy overrides, SSRF/rebinding, cross-tenant session reuse and stale generations fail closed. Mapping updates are versioned, audited and generation-fenced. Missing, stale, conflicting or changed mappings have no open-relay, other-tenant or legacy-fork fallback. Admin and user signing identities remain separate.

The checked-in policy model and negative tests do **not** enforce the contract in the production connector. Production acceptance needs transport integration and independent review of DNS/TLS, trusted mapping storage, egress/proxy policy, connection cancellation, session isolation and audit behavior.

The current blanket rejection of internal and IANA special-use addresses is incompatible with a relay bundled in-process/container-local or reachable only on a private network/VPC. Do not relax it as part of this POC. If private relay deployment is required, approve a narrowly scoped trusted-service exception with immutable service identity and endpoint allowlist plus network-level egress enforcement, and test DNS pinning, peer identity, TLS and tenant binding end to end. Otherwise require a publicly routable relay endpoint. The preferred v1 deployment model must be decided explicitly.

**Decision:** ☐ Accept the contract and public-endpoint requirement  ☐ Accept a separately reviewed private-service exception  ☐ Reject / revise

Authorized security approver: ____________________  Date: __________

## Enforcement and release gate

These are proposed decisions only; no approval is recorded here. Even after design approval, Phase 2A remains blocked until the stock Buzz desktop import, public-key match, channel and private-DM event-ID interoperability, restart/history and cleanup checks pass, required CI and independent reviews pass, and production mapping enforcement has its own reviewed implementation gate. A human acceptance signature cannot substitute for executable proof.
