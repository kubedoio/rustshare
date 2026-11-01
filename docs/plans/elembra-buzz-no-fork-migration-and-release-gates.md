# PLAN: Elembra Chat on unmodified upstream Buzz

**Status:** Target architecture accepted; execution and release gates are separate. Existing fork-backed deployments and browser-held identities remain unchanged.

## Phase 0 — source and architecture audit (complete)

The relay and desktop source baselines are recorded in `elembra-buzz-upstream-compatibility-matrix.md` and the Phase 1c evidence report. The POC runs unmodified upstream source. This evidence does not establish desktop GUI acceptance or production transport enforcement.

## Phase 1 — disposable upstream interoperability (evidence complete with gaps)

Preserve the exact-source protocol harness and prior passing evidence. The latest run confirms NIP-42, NIP-43, channel history/discovery and protocol-client native private-DM denial; randomized-timestamp NIP-17 remains a historical failure. Desktop assertions are unverified and deferred to issue #442. Do not use the POC relay with real users or production data.

## Gate D — accepted design record

Record the owner-approved Option A architecture, profile-scoped ADR-0034/0035/0036 amendments, NIP-29-only v1 scope and one relay/community per tenant. The ADR/documentation PR must be DCO-signed, reviewed and merged through protected repository policy. Independent security acceptance of custody and private-service endpoint policy remains separately pending.

**Allows:** isolated Phase 2A vault code behind a disabled-by-default boundary.

**Does not allow:** real-user provisioning, signing, key export, Buzz integration, production migration or deployment.

## Gate V — Phase 2A inert vault

Implement a tenant/principal/generation-bound KMS envelope vault and isolated schema. Generate only test identities in tests. A database dump alone cannot decrypt envelopes. Enforce explicit disabled state with no route, worker, CLI or runtime selector that can provision a real user, sign, export or contact Buzz. Preserve legacy browser-held keys, current fork deployments and M4.1b2 replacement guards.

Require race/retry, failure, tamper/AAD, tenant/generation, disabled account, KMS outage/rotation, DB-only compromise, backup/restore, audit redaction and old-mode regression tests. Independent cryptographic/security review and protected CI are required before merge.

**Allows:** merge of reviewed inert storage code only.

**Does not allow:** identity activation, managed Chat integration or key export.

## Gate I — Phase 2B managed Chat integration

Before integration is enabled, close desktop acceptance issue #442 using the native OS keychain and exact audited upstream relay/desktop sources. Prove identity continuity, correct relay/community, channel and private-DM event IDs in both directions, restart/history, no duplicate enrollment and credential cleanup. Build a production-wired tenant mapping store, resolver/TLS transport and session/generation fencing. The default endpoint policy remains strict public. A private/bundled `trusted_service` exception requires independent threat-model review, immutable service identity, exact endpoint allowlisting, TLS peer pinning and network-enforced egress controls; no broad internal-IP exception is allowed.

Prove NIP-42 subject-bound reads, NIP-43 add/removal and effective revocation, tenant isolation and NIP-29 channel/native private-DM semantics. NIP-17 stays disabled. Keep timeline, Chat-backed Memory, reconciliation, and other reads disabled unless current-subject relay authorization is demonstrated. Require independent custody security acceptance before any real-user signing or export.

**Allows:** a separately approved non-production managed Chat integration test.

**Does not allow:** production defaults, automatic rollout, historical migration or deployment.

## Gate R — new-install enablement and per-tenant cutover

Qualify immutable upstream source/image digest, SBOM/provenance, PostgreSQL and KMS backup/restore, key recovery, rollback, host and relay-key rotation, offboarding/reconnect, and feature-specific authorization tests. Decide exported-`nsec` incident/recovery treatment. Enable new installations only through a reviewed release change. Migrate existing tenants one at a time with explicit operator approval, verified identity/history treatment and rollback; never rewrite keys/history or lower production/fork identity guards silently.

## Non-negotiable compatibility constraints

- No patch, fork, custom endpoint or private database access in upstream Buzz.
- One isolated relay/community per tenant; no cross-tenant socket or subscription reuse.
- No NIP-17 in v1; the recorded failure remains visible.
- No private endpoint without the reviewed `trusted_service` exception and network-layer egress enforcement.
- Preserve existing fork pins, browser-held identities, identity-replacement guards and runtime behavior until separate release approval.
