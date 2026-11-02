# Elembra Buzz — Architecture Adoption and Security Gates

**Decision record date:** 2026-10-31. **Status:** Target product architecture and v1 messaging scope accepted by project-owner direction. Independent security approvals and runtime enforcement remain pending. PR approval or merge is not security acceptance.

The project owner explicitly approved ADR-0044 Option A as the target product architecture and authorized an isolated, disabled-by-default Phase 2A vault implementation. This records that instruction; it does not claim an independent security review, production enforcement, implementation approval or release authorization.

## A. ADR-0044 — managed identity custody

**Product/architecture direction: ACCEPTED.** Elembra will own KMS-protected Nostr identities for the future `upstream_managed` profile. The backend signing service will technically be capable of signing as those users; this is custodial identity, not end-to-end key exclusivity. Target controls are KMS-backed envelope encryption, tenant/principal/identity/generation-bound AEAD, typed and scoped signing permissions, least privilege, audit, separation of user and relay-admin identities, tested backup/restore and recovery, and explicit export policy. A user-exported `nsec` copy cannot be cryptographically revoked.

**Independent security approval: PENDING.** An independent reviewer must accept the threat model, signer boundary, KMS permissions and recovery, export/offboarding treatment and operational controls before any real-user provisioning, signing or export. No reviewer signature is fabricated here.

## B. Initial messaging scope

**Product scope: ACCEPTED.** V1 targets NIP-29 channel messaging and native NIP-29 private-channel DMs. These private DMs are hidden relay-ACL-protected channels, not end-to-end encrypted NIP-17/NIP-44 gift wraps. NIP-17 is excluded and must remain disabled in v1. Its randomized-timestamp compatibility failure remains in the historical evidence. Managed mode creates no browser-held key.

**Runtime acceptance: PENDING.** Stock Buzz Desktop native-keychain acceptance is deferred to [issue #442](https://github.com/kubedoio/rustshare/issues/442). It remains mandatory before Phase 2B managed Chat integration, not before inert vault work. Protocol-client proof does not satisfy desktop acceptance.

## C. Tenant-to-relay architecture

**Target topology: ACCEPTED.** One closed, isolated relay/community per tenant, selected from trusted tenant/workspace mapping. No user-controlled endpoint; exact URL, host, community, relay identity/provenance and mapping generation; TLS hostname/peer validation; DNS pinning; tenant-scoped sessions; generation-fenced changes; fail-closed behavior; no fallback to another tenant, an open relay or the legacy fork. Relay admin and user signing identities remain separate.

**Independent security approval and production enforcement: PENDING.** The checked-in mapping model is policy code, not a production connector. The current strict policy rejects internal/special-use destinations. A proposed `trusted_service` exception for private/bundled relays must bind immutable service identity and exact admin-approved endpoints, preserve TLS and peer pinning, disable redirects/proxy overrides, and enforce destination limits at the network layer. It must receive independent threat-model review and integration tests before it is enabled. Do not allow arbitrary private endpoints or weaken the current SSRF guard.

## D. Separate implementation and release gates

| Gate | Status | Permitted scope |
|---|---|---|
| D — design adoption | Target direction accepted; repository PR/review pending | Profile-scoped ADR and contract updates |
| V — Phase 2A vault | Authorized as implementation planning | Isolated KMS-backed test-only vault, disabled by default; no real-user provisioning, signing, export, relay integration or enablement |
| I — Phase 2B managed Chat | NOT READY | Requires Gate V review, independent custody approval, issue #442 desktop results, production mapping enforcement and stock protocol tests |
| R — production release/cutover | NOT READY | Requires separate security acceptance, backup/recovery, operational/rollback evidence and per-tenant migration approval |

An accepted security design is not proof that runtime behavior enforces it. No production custody behavior or deployment change is authorized by this record.
