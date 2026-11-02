# CONTRACT: No-Fork Upstream Buzz Relay Adapter v1

**Status:** Accepted target architecture; production adapter and security enforcement remain gated. Protocol evidence is pinned to relay `eafd77e9f6e96052476bdd88be732c046fdb7ab3` and app `662e91800af8204ec0e492522b8f23ef23b19baa`; no upstream OCI release/image digest is qualified. Never assume current `main` is a deployed release.

## Strict boundary

`Elembra Chat Adapter -> standard Nostr WebSocket + documented upstream Buzz protocol/operator surface -> upstream relay`. No `buzz-db` imports, SQL reads/writes, overlay-owned Chat ACL, custom handlers in Buzz, shared database schema, unreviewed monkey patches, or patched image. Buzz continues as source of truth for messages, channels, DM membership and native access policy. Public keys are portable; secret keys are exported only on user request.

## Accepted v1 scope and implementation gates

The target v1 scope is NIP-29 channel messages and native private-channel DMs.
The latter are hidden relay-ACL-protected NIP-29 channels and are **not**
end-to-end encrypted. NIP-17/NIP-44 gift-wrap DMs are excluded and must be
disabled. Phase 2A is limited to an inert vault; it does not implement this
adapter, user signing, provisioning or export. Desktop GUI acceptance is
tracked by issue #442 and blocks Gate I/Phase 2B, not the isolated vault.

## Supported primitives and classification

| Purpose | Required upstream primitive | Status of assumption |
|---|---|---|
| Sign/authenticate user | Nostr Schnorr events, NIP-42 kind 22242 | Documented upstream; verify pinned build |
| Post/subscribe/query | NIP-01 `EVENT`, `REQ`, `EOSE`, `OK`, `CLOSE`, `AUTH` WebSocket frames | Documented; verify event kinds and pagination |
| Send in channel | Buzz kind 9 with `h` channel tag | Documented in `block/buzz/NOSTR.md`; verify pinned release |
| Add/remove relay member | NIP-43 signed kinds 9030/9031, admin/owner identity | Documented; inspect receipt/replay/active-session behavior |
| Read relay metadata | NIP-11 relay info | Documented; not authoritative community UUID proof by itself |
| Relay/community membership | Host-scoped relay membership when `BUZZ_REQUIRE_RELAY_MEMBERSHIP=true`; NIP-43 is documented as relay membership | Source-documented; closed defaults and host/tenant isolation need POC |
| Channel discovery | NIP-29 kinds 39000/39001/39002; DM discovery uses group discovery | Source-documented; Elembra parity/pagination needs POC |
| NIP-29 private-channel DMs | Native hidden NIP-29 channels and relay membership ACL | Accepted v1 scope; protocol-client outsider denial passed, desktop discovery and bidirectional DM acceptance deferred to issue #442 |
| NIP-17 encrypted DMs | NIP-44 encrypted seals and NIP-59 gift wraps (kind 1059) | Excluded and disabled in v1; randomized-timestamp rejection remains historical compatibility failure, not a v1 pass |
| Community/tenant creation | Deployment/operator configuration | No verified upstream HTTP community-creation contract; one isolated relay per tenant for POC |
| Desktop onboarding | `block/buzz-app` README documents `nsec1…` import and Settings key copy; distinct commit `662e91800af8204ec0e492522b8f23ef23b19baa` | GUI acceptance deferred to [issue #442](https://github.com/kubedoio/rustshare/issues/442); unverified until native keychain import, identity match, channel/DM IDs, restart/history and credential cleanup execute |
| Permissions for Chat-backed Memory | Live read as subject managed Nostr identity | Proposed replacement for private signed check; must prove leakage-free behavior |

Existing downstream-only endpoints that MUST NOT be used in this profile:

```
POST /api/v1/relay/access/check
POST /api/v1/relay/access/check-batch
GET  /api/v1/relay/channels
GET  /api/v1/relay/state/events
GET  /api/v1/relay/community
POST /api/v1/relay/membership/check
```

Even if a similarly named upstream endpoint appears in a future version, it must be accepted, documented and tested *as upstream*, not imported by assumption from the fork. The v1alpha2 conditional `elembra-op` and `elembra-expected-revision` additions are not baseline stock-Buzz behavior. The inspected `NOSTR.md` documents NIP-43 relay member add/remove, but does not establish forced termination of already-open sessions; require a reconnect and active-session POC before describing removal as revocation.

## Trust and failure behavior

Every event (including from Buzz desktop) must be associated with its actual signed pubkey, host-derived community and current native channel scope. The normal user's managed key authenticates ordinary reads and sends; the separate admin key may only provision memberships. A relay owner/admin identity must never be substituted for subject identity when deciding what the subject can read.

When a relay operation gives only ACK/no-op/roster notice, mark its exact semantic reach accordingly. Do not treat ACK as verified revocation, roster event as a fresh authority snapshot, or an Elembra-local status flag as relay permission. For removal require reconnect-denial test; if active sessions remain, report that limitation and block false guarantees of immediate revocation.

When source content must appear in Elembra Memory/RAG: authorize by current live subject-bound relay read, matching channel/DM and resource identity; do not treat public historical event lookup, observer queue, stale index, or cached read as sufficient. Fail closed on unknown/timeout/offline, including to the point of disabling Chat-backed Memory in this initial messaging-first profile. The same conditions apply to the Elembra web timeline if it consumes derived observations.

## Conformance matrix

- **C1:** Exact upstream source hash and image digest; assert no downstream patch/extension registered and no calls to fork-only URLs.
- **C2:** One tenant+host+closed community, TLS/host mismatch fail-closed; two-tenant isolation.
- **C3:** Generated member identity NIP-42 AUTH succeeds; non-member rejected; malformed signature rejected.
- **C4:** NIP-43 add/remove as admin; delayed/replayed operations and reconnect proof; no unsupported idempotency claim.
- **C5:** Elembra->Buzz desktop and Buzz desktop->Elembra same pubkey, channel and historical event ID; consistent reaction/thread subset.
- **C6:** Channel-private and DM visibility; a revoked or wrong-tenant identity sees nothing in timeline or Memory.
- **C7:** Old connected client revocation and reauthentication behavior characterized, with clear limitation if stock relay does not force disconnect.
- **C8:** Connection outages, backing-store restart and old/new mapping mismatch cannot create a second Chat authority.
- **C9:** Key export exact public-key match; Buzz 1.0 `nsec` import round trip and Buzz 0.x compatibility separately.
- **C10:** Observability and rollback: no keys or message bodies in operational logs, preserved existing fork deployment until acceptance.

## Upstream/client references

- https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/NOSTR.md
- https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/ARCHITECTURE.md
- https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/crates/buzz-relay/src/handlers/relay_admin.rs
- https://github.com/block/buzz-app/blob/662e91800af8204ec0e492522b8f23ef23b19baa/README.md
- https://github.com/kubedoio/rustshare/blob/main/docs/adr/0035-buzz-source-authorization-gateway.md
