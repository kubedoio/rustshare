# PLAN: Transition Elembra Chat to stock Buzz without destructive cutover

**Status:** Proposed. **Goal:** make `upstream_managed` the default for new installations once qualified; migrate existing fork deployments only with explicit operator approval.

## Phase 0 — architecture and compatibility audit (docs only)

Inventory references to `BUZZ_SUPPORTED_COMMIT`, `BUZZ_CONTRACT_VERSION`, `BUZZ_REPLACEMENT_V1ALPHA2_QUALIFIED`, `buzz_gateway`, `buzz_bridge`, `chat_membership`, existing Chat identity API and `config/buzz-compatibility.env`. For each call classify it as upstream-native, fork-specific, or proposed only. Separately pin official `block/buzz` relay and `block/buzz-app` client commits/releases; verify that stock image provenance is reproducible and unchanged. Produce matrix with risk and named replacement for every fork-only feature. ADR-0044 owner direction does not waive independent security review.

**Result (2026-10-31):** source audit and file/function migration matrix are in `docs/plans/elembra-buzz-upstream-compatibility-matrix.md`. The exact source snapshots are `block/buzz` `eafd77e9f6e96052476bdd88be732c046fdb7ab3` and `block/buzz-app` `662e91800af8204ec0e492522b8f23ef23b19baa`. This phase did not run a relay smoke test, test desktop import, or qualify an OCI digest. ADR-0044 remains proposed; existing ADRs and production behavior remain operative.

**Gate:** source inventory is documented; Phase 1 must supply real-source protocol/GUI evidence. No production changes.

## Phase 1 — stock Buzz disposable POC

Use the executable sequence in `docs/plans/elembra-buzz-upstream-compatibility-matrix.md`: check out the exact audited relay SHA into a disposable directory; build/run only the unmodified source with isolated backing services; set explicit auth and membership enforcement; create a dedicated relay owner and two disposable members; add members with NIP-43 9030; authenticate through NIP-42; exercise NIP-29 discovery and two-way kind-9 history with the separate Buzz 1.0 beta desktop client. Test NIP-43 9031 against both a fresh reconnect and the already-open socket. Save only sanitized traces and container/source digests. Do not connect the relay to Elembra production or modify the current Chat authority.

**Gate:** all transport/conformance and isolation checks against exact source/digest; desktop import result and session-removal behavior are evidenced; any gap stays disabled.

## Phase 2 — managed vault and Elembra native Chat

Add isolated managed-key storage with KMS, identity generation, workers, OIDC-protected typed signer, authenticated WS adapter and user Settings export. New accounts are automatically provisioned when Chat enabled. Legacy browser-key onboarding remains enabled on the old profile but not silently rotated. Chat UI uses server signing, not a browser private-key prompt. Add login/logout, audit and failed-provisioning operator diagnostics.

**Gate:** per-user two-way interoperability; backup/restore; security/tenant/credential tests and protected CI.

## Phase 3 — capability parity and data/memory boundaries

Replace channel discovery and reconciliation only where native Buzz access is proven. Enable Chat-backed Memory only after current-user live relay read-based authorization is demonstrated for private/revoked channels, DMs and race conditions; otherwise keep disabled. Preserve attachment authorization through Elembra Files. Document workflows/huddles/notifications/complex roles that are out of scope; never fake parity. Independently review security/custody and offboarding.

**Gate:** all MUST capabilities satisfy contract; unsupported optional capabilities are explicitly disabled, visible to operators and non-leaking.

## Phase 4 — release and cutover

Cut immutable digest-pinned release; test PostgreSQL backup/restore, state migration, identity continuity, channel IDs/historical messages and direct client reconnection. Fresh new deployments can select upstream profile after qualification. Migrate existing tenants individually with traffic quiescence as needed and explicit revert window. Preserve the previous fork image and state for rollback until proof. Never destructively overwrite a Buzz DB with a different upstream migration history. If import/export of messages is unsupported, label migration BLOCKED and offer clean-start tenant only by explicit choice.

**Gate:** independent architecture/security review, CI on exact pinned source/image, operator go/no-go, documented status and rollback rehearsal.

## Explicit incompatibilities to resolve before claiming no-fork parity

1. Signed `access/check`/batch and channel/state registry used by Elembra authorization, reconciliation and Memory.
2. Signed `community` discovery and release/pin validation from zero-config bootstrap.
3. Custom v1alpha2 revision/tombstone/operation receipts, pre-send release gate and `rotation_of` replacement protocol from downstream Buzz PRs #6/#7.
4. Managed vs legacy client-held identities; historic signer continuity, user key loss, exported key reuse and zero-tenant migration.
5. Ordinary NIP-43 membership removal vs true live client disconnection; no stronger claim without tests.
6. One host/community topology and tenant-isolated Postgres, operator controls, TLS/cert issuance, backups and upgrades.

**Release guard:** "No fork" is a property of source and execution, not branding. Validate exact image source/attestation and no calls into unsupported relay endpoints. Keep existing fork profile installed but inactive/segregated until every migration gate succeeds.
