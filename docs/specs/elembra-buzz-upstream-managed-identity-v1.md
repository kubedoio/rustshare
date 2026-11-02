# SPEC: Upstream-only Buzz Chat with Elembra-managed identities v1

**Status:** Target architecture accepted by project-owner direction (2026-10-31); implementation remains profile-gated. Phase 2A permits only an inert, disabled-by-default vault. **Date:** 2026-10-31.

## Goal and non-goals

Target architecture: use unmodified upstream `block/buzz` for Elembra web Chat and direct stock Buzz clients, with explicit Elembra-managed identities and user-controlled `nsec` export. Phase 2A implements only encrypted key storage primitives; it does not provision identities, sign user events, export keys, connect to Buzz or change the runtime profile. Avoid any downstream Buzz patch or private Buzz DB/API access.

Not v1: arbitrary Buzz extension/plugin development, impersonation-safe E2E encryption, automatic key injection into a desktop client, advanced device-specific signing, opaque governance guarantees not supported by upstream, permissive bypass of Chat Memory authorization, migration of existing users without explicit verified treatment.

## Diagram

```
[Elembra OIDC/Users/Tenants]
   |         |             \
   |         |              +--> [Files/Memory] -- read-time Buzz authorization --> [Buzz]
   |         +--> [Chat provisioning/outbox] -- signed NIP-43 --> [Stock Buzz relay]
   +--> [Managed-key vault + signing] -- NIP-42/NIP-01 WS --> [Stock Buzz relay]
                      ^                                       ^
                [Elembra web Chat]                     [Stock Buzz client]
                                                    (manually imported nsec)
```

## Deployment and supported version matrix

1. Proposed configuration names are `upstream_managed` and `fork_v1alpha1`; neither is an existing runtime profile selector. A later implementation must preserve the current deployment configuration and default behavior for existing tenants. Never make an implicit fallback to a weaker mode.
2. Verify GitHub upstream commit/tag, container image digest, platform architecture, Rust/DB migration requirements, and whether an official OCI release exists. If no official image is published, build an Elembra-packaged image from **verbatim upstream source** with recorded tree hash, Dockerfile/lockfile/toolchain, SBOM/provenance and no source patch; do not assert this is an official image.
3. Phase 0 source baselines are `block/buzz` `eafd77e9f6e96052476bdd88be732c046fdb7ab3` (2026-10-31 main) and `block/buzz-app` `662e91800af8204ec0e492522b8f23ef23b19baa` (2026-10-30 main). The relay has no release tag selected here; the app README describes Buzz 1.0 as beta but the inspected source has no 1.0 beta tag. These are audit pins, not approved deployment pins. A tested OCI digest and exact desktop package must be recorded separately before a release decision.
4. Use exactly one **closed, isolated** relay/community per Elembra tenant, with explicit domain/host, TLS, dedicated owner/admin keys and independent configuration/state. Preserve volumes/backup on upgrade. Shared multi-community relay hosting is out of v1 scope.
5. Safe defaults must explicitly enable `BUZZ_REQUIRE_RELAY_MEMBERSHIP=true` and `BUZZ_REQUIRE_AUTH_TOKEN=true`, avoid accidentally open relay/public auth, and restrict relay operator/control-plane access. Phase 1c exercised these settings, but that POC does not establish production enforcement.
6. Bootstrap operator/relay configuration, ready checks, community identity/host mapping, immutable relay identity and restoration. Validate community ID/host via documented upstream APIs and admin configuration; do not invent a signed `/api/v1/relay/community` equivalent or trust an unauthenticated value as the old signed proof.

## Identity and provisioning

| Concept | Owner | Persistence |
|---|---|---|
| Tenant, Principal, account status | Elembra | Elembra DB |
| Community host/relay configuration | Deployment + Buzz | Audited Elembra mapping; Buzz internal state only through public interfaces |
| Nostr key material | Elembra managed vault | AEAD encrypted, KMS-wrapped |
| Per-principal Nostr public key | Elembra | Unique active `(tenant, principal, generation)` binding |
| Community membership and channel membership | Buzz | Buzz relay authoritative state |
| Signed events and chat history | Buzz | Buzz relay authoritative state |
| Chat observations/Memory refs | Elembra | Derived, never granting access |

Future Gate I managed Chat integration (not Phase 2A): on an explicitly enabled user activation in a Chat-enabled tenant:

1. Transactionally create/claim an idempotent provisioning request, no plaintext key in queue. Worker verifies account active, tenant-scoped community mapping ready and no existing identity. Generate new secp256k1 key exactly once; encrypt+persist the envelope with unique identity generation. Retries must reuse the persisted key; a failed initial write does not leak a valid but untracked key.
2. Enroll the public key in that relay/community with authenticated stock NIP-43 kind 9030, signed by the separately protected relay owner/admin key. The upstream docs describe both the event and `buzz-admin` CLI; the latter requires relay credentials/database access and is not the proposed app path. Treat a relay `OK` as processing only. Reconnect with the managed identity using NIP-42 and demonstrate actual member access before setting status `ready`. If verification fails, stay `pending/failed`; never grant access through a local flag alone.
3. Optionally provision native default channel membership via upstream-supported operations; verify exact community, channel role and visibility. The user sees `Chat is being prepared` until ready, then the native Chat UI; they never see the key setup flow.
4. Existing human identities remain stable. Disabled/deleted Elembra users are prevented from managed signing immediately; independently issue stock Buzz member removal (9031), verify denial on fresh NIP-42 connection and handle remaining live sessions per measured upstream behavior. Do not assume a roster snapshot is fresh proof of forced disconnect.
5. If Buzz is down, queue bounded retries with idempotency and operator diagnostics. Never create duplicate active keys or cross-tenant membership. Reconcile at startup and periodically; incomplete steps are visible as pending, not silently succeeded.

## Managed signing and Chat API

Phase 2A exposes no signing API and performs no signing. In a later independently reviewed integration, the backend verifies current Principal, tenant, enabled Chat, mapping and typed per-action intent before invoking the vault; arbitrary event JSON, caller-selected keys and user-controlled relay destinations are prohibited. A publish acknowledgement is not observational or recipient delivery.

Read/subscribe as the current user's managed identity via the upstream relay, which is final on channel/DM authorization. The relay read is the gating source for visible content. Bind pooled sessions to tenant+principal+generation+host; never mix a session or reuse one user's subscriptions for another user's data. Direct desktop clients publish/read under that *same public key*; the Elembra UI observes history via the same stock protocol. Include reconnect, pagination, large channels, reactions, threads and media compatibility in tests; do not claim these work until tested.

## External Buzz clients and credential export

Phase 2A has no export endpoint or UI. A later independently gated feature may expose a deliberate one-time `nsec` export only after fresh step-up, CSRF protection, audit, no-store responses and a clear warning that exported copies cannot be cryptographically revoked. Buzz desktop acceptance is deferred to issue #442. Never call the secret an ordinary password or WebAuthn passkey.

## Authorization, Chat-backed Memory, and unsupported existing contracts

Current Elembra relies on fork-only signed `POST /api/v1/relay/access/check`, `/check-batch`, `GET /api/v1/relay/channels`, `/state/events`, and community discovery, consumed by `BuzzGatewayClient` and the observer/reconciler. The upstream-only adapter **must not call these paths**. Stock NIP-29 channel discovery and authenticated NIP-42 `REQ`/`EVENT` can support a direct Buzz client immediately; the exact Elembra adapter semantics and history pagination remain unproven. There is no blanket equivalent for the old per-resource read/check contract. Keep Chat-backed Memory and Elembra timeline reads that depend on the fork authority disabled until an independent test proves subject-bound reads for private channels, DMs, recently revoked membership, stale indexes and tenant boundaries. Never use the managed owner/admin key to read user-private content for ordinary user requests.

This product design prioritizes native messaging. Enterprise policies not enforceable by the stock relay for *both* direct Buzz clients and Elembra users (e.g. a custom parent/student DM prohibition) are explicitly unsupported, not an Elembra-only false guarantee. Files attachments must reauthorize at open time through Elembra Files; a raw Buzz client may not understand an Elembra-specific attachment tag. Avoid storing reusable Files bearer URLs in Chat events.

## Cutover

Maintain legacy fork mode in place. Add upstream mode as a distinct profile, using separate side-by-side test tenants first. Record old/new relay URL, community mapping, identity/public-key equivalence, channel IDs, history availability and any nonportable message types. Do not claim message/history migration unless an upstream documented import path exists and is tested. Never point existing users to a new empty relay as a silent upgrade. Cut over per tenant with backup + rollback decision; freeze or drain writes as necessary; reauthorize all users and test old exported clients. Disable old fork image only when all retained features are either proved on upstream or intentionally disabled with product approval.

## Required acceptance table

| Gate | Expected evidence |
|---|---|
| Upstream no-fork | Git source/tree hash, exact digest, zero modifications to Buzz code, test traces contain no custom `/api/v1/relay/...` endpoints |
| Auto deploy | New tenant starts a closed relay, idempotent bootstrap and mapped host, TLS, backup/restore |
| Auto user | First login to Chat requires no key screen; same key after retries, account switch isolation, no local browser secret |
| Two-way messaging | Elembra web → Buzz client and Buzz client → Elembra, exact public key and channel, reconnect/history |
| Offboarding | Disabled Principal cannot sign in Elembra, new direct client connection denied after stock NIP-43 removal, active-session semantics documented/tested |
| Secrets | DB-only compromise cannot decrypt, KMS-bound backup restore, key export audited and blocked without step-up |
| Permissions | Tenant/channel/private/DM reads obey stock relay; revoked channel content not exposed in Elembra/Memory; unsupported features explicitly fail closed |
| Regression | Existing fork deployment behavior and identity records remain intact, no unreviewed migration or production pin change |

## Source-backed anchors

- `config/buzz-compatibility.env`, `scripts/elembra.sh`, `frontend/src/lib/chat/keys.ts`, `docs/adr/0034-elembra-chat-buzz-boundary.md`, `docs/adr/0035-buzz-source-authorization-gateway.md`, `docs/adr/0036-elembra-chat-zero-config-bootstrap.md`
- https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/NOSTR.md
- https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/ARCHITECTURE.md
- https://github.com/block/buzz-app/blob/662e91800af8204ec0e492522b8f23ef23b19baa/README.md

## Accepted v1 messaging scope and phase gates

The initial product scope is NIP-29 channel messaging and native NIP-29
private-channel DMs. These DMs are hidden/relay-ACL-protected channels, not
NIP-17/NIP-44 end-to-end encrypted gift wraps. NIP-17 is excluded and must be
disabled in v1; its randomized-timestamp rejection remains in the historical
compatibility evidence. The exact desktop client runtime test is deferred to
[#442](https://github.com/kubedoio/rustshare/issues/442) and remains required
before Gate I, not before inert vault work.

| Gate | Permitted work | Prohibited until next gate |
|---|---|---|
| D — design record | Accepted target architecture and profile-scoped ADR updates | Runtime behavior change |
| V — Phase 2A vault | Isolated, disabled-by-default storage and test-only cryptographic round trips | Real-user provisioning, user signing, export, relay connection, profile enablement |
| I — Phase 2B managed Chat | After Gate V review, desktop acceptance, production-wired mapping and stock protocol tests | Production defaults, migration or deployment |
| R — release/cutover | Explicit release and per-tenant migration after operational evidence and security acceptance | Implicit migration or weakening of legacy guards |

An accepted architecture is not evidence of enforcement. The tenant mapping
policy model is not connected to the production connector; private or bundled
relay access remains disabled until the `trusted_service` exception in
[`upstream-buzz-tenant-relay-mapping-v1.md`](../contracts/upstream-buzz-tenant-relay-mapping-v1.md)
is independently reviewed and network-enforced.
