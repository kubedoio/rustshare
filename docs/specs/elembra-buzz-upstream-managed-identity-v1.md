# SPEC: Upstream-only Buzz Chat with Elembra-managed identities v1

**Status:** Proposed; implementation requires ADR-0044 acceptance and protected CI. **Date:** 2026-10-31.

## Goal and non-goals

Provide automatic tenant Chat setup and zero-touch user provisioning; use unmodified upstream `block/buzz` relay with Elembra web Chat; allow direct connection from stock Buzz 0.x/1.0 clients through user-controlled `nsec` export. Avoid any long-lived downstream Buzz patch or private Buzz DB/API access.

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

1. Define profile `upstream_managed` in Elembra configuration; keep current `fork_v1alpha1` untouched and default for existing deployments until migration. Never make an implicit fallback to a weaker mode.
2. Verify GitHub upstream commit/tag, container image digest, platform architecture, Rust/DB migration requirements, and whether an official OCI release exists. If no official image is published, build an Elembra-packaged image from **verbatim upstream source** with recorded tree hash, Dockerfile/lockfile/toolchain, SBOM/provenance and no source patch; do not assert this is an official image.
3. Phase 0 source baselines are `block/buzz` `eafd77e9f6e96052476bdd88be732c046fdb7ab3` (2026-10-31 main) and `block/buzz-app` `662e91800af8204ec0e492522b8f23ef23b19baa` (2026-10-30 main). The relay has no release tag selected here; the app README describes Buzz 1.0 as beta but the inspected source has no 1.0 beta tag. These are audit pins, not approved deployment pins. A tested OCI digest and exact desktop package must be recorded separately before a release decision.
4. Start with one **closed** relay/community per Elembra tenant, each with explicit domain/host, TLS, dedicated owner/admin keys and independent configuration/state. Preserve volumes/backup on upgrade. Shared multi-community relay mode is optional only after host/tenant isolation and community creation tests using current upstream operator APIs.
5. Safe defaults must explicitly enable `BUZZ_REQUIRE_RELAY_MEMBERSHIP=true` and `BUZZ_REQUIRE_AUTH_TOKEN=true` as documented by the audited relay source, avoid accidentally open relay/public auth, and restrict relay operator/control-plane access. This source-level support is not yet exercised in a POC. Do not assume upstream sample defaults are production-safe.
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

On Elembra user activation in a Chat-enabled tenant:

1. Transactionally create/claim an idempotent provisioning request, no plaintext key in queue. Worker verifies account active, tenant-scoped community mapping ready and no existing identity. Generate new secp256k1 key exactly once; encrypt+persist the envelope with unique identity generation. Retries must reuse the persisted key; a failed initial write does not leak a valid but untracked key.
2. Enroll the public key in that relay/community with authenticated stock NIP-43 kind 9030, signed by the separately protected relay owner/admin key. The upstream docs describe both the event and `buzz-admin` CLI; the latter requires relay credentials/database access and is not the proposed app path. Treat a relay `OK` as processing only. Reconnect with the managed identity using NIP-42 and demonstrate actual member access before setting status `ready`. If verification fails, stay `pending/failed`; never grant access through a local flag alone.
3. Optionally provision native default channel membership via upstream-supported operations; verify exact community, channel role and visibility. The user sees `Chat is being prepared` until ready, then the native Chat UI; they never see the key setup flow.
4. Existing human identities remain stable. Disabled/deleted Elembra users are prevented from managed signing immediately; independently issue stock Buzz member removal (9031), verify denial on fresh NIP-42 connection and handle remaining live sessions per measured upstream behavior. Do not assume a roster snapshot is fresh proof of forced disconnect.
5. If Buzz is down, queue bounded retries with idempotency and operator diagnostics. Never create duplicate active keys or cross-tenant membership. Reconcile at startup and periodically; incomplete steps are visible as pending, not silently succeeded.

## Managed signing and Chat API

The browser authenticates only to Elembra. Backend verifies current Principal, tenant, enabled Chat, mapping and per-action intent before invoking vault signing. It constructs the Nostr event server-side from typed trusted fields (not arbitrary JSON): allowed message kinds, target channel/DM, content size, attachment `ResourceRef` and sender key resolved from the session; enforces application rate limits and audits sensitive operations. Server signs the event and publishes to Buzz over WS; all incoming relay receipts and messages are checked for exact signed event ID, publisher, community, and expected semantics. A publish acknowledgement is not observational or recipient delivery; model UI status accurately.

Read/subscribe as the current user's managed identity via the upstream relay, which is final on channel/DM authorization. The relay read is the gating source for visible content. Bind pooled sessions to tenant+principal+generation+host; never mix a session or reuse one user's subscriptions for another user's data. Direct desktop clients publish/read under that *same public key*; the Elembra UI observes history via the same stock protocol. Include reconnect, pagination, large channels, reactions, threads and media compatibility in tests; do not claim these work until tested.

## External Buzz clients and credential export

Expose Settings > Chat > Buzz connection, showing relay URL and masked public key. After explicit fresh step-up (e.g., OIDC re-authentication), allow a short-lived, one-time export request returning the user's `nsec` only over TLS with no-cache headers and audit, never by email, URL, logs, analytics or background jobs. Advise users that Buzz 1.0 beta imports `nsec1…` under "Use an existing key"; a user may have to paste the relay/community URL separately. Support optional NIP-49 export only after compatibility proof; the existing Elembra JSON backup is not NIP-49. Do not call the secret an ordinary password or WebAuthn passkey.

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
