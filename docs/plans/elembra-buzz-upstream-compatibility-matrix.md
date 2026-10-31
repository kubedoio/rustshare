# Elembra Chat to upstream Buzz: Phase 0 compatibility matrix

| Target | Audited ref |
|---|---|
| Audit date | 2026-10-31 |
| RustShare | Protected `origin/main` `aea1cc8baec281a3d5cc1453468373c06021ed33` |
| Relay | [`block/buzz` `eafd77e9f6e96052476bdd88be732c046fdb7ab3`](https://github.com/block/buzz/tree/eafd77e9f6e96052476bdd88be732c046fdb7ab3), main (2026-10-31); no relay release tag selected |
| Desktop | [`block/buzz-app` `662e91800af8204ec0e492522b8f23ef23b19baa`](https://github.com/block/buzz-app/tree/662e91800af8204ec0e492522b8f23ef23b19baa), main (2026-10-30); README calls Buzz 1.0 beta, no 1.0 beta tag selected |

This is a source/documentation audit, not a live interoperability result. RustShare's compatibility file records upstream relay tag `relay-v0.2.1` / commit `6e5c462ac524de60d7edb46c66130fd779cc9006` as its fork baseline, and separately pins the tested downstream implementation at commit `c9a373beca1d602d02308c687db5535c63f3921f`, image `ghcr.io/kubedoio/buzz-elembra@sha256:2b6c4b2a1800984283f1893a407997b2d5101b01fd0195669ee5deb84814906a`, contract `v1alpha1`, and `BUZZ_REPLACEMENT_V1ALPHA2_QUALIFIED=0`. No upstream relay OCI digest was selected or qualified. This Phase 0 changes none of those values.

## Upstream capability findings

| Capability | Evidence at audited source | Phase 0 finding |
|---|---|---|
| NIP-42 connection AUTH | `block/buzz/NOSTR.md`, “What Works”; proactive challenge | Stock protocol documented; handshake and failure modes need POC |
| NIP-43 relay admission | `NOSTR.md`, “Relay Membership”: kinds 9030 add and 9031 remove, owner/admin-authenticated; `BUZZ_REQUIRE_RELAY_MEMBERSHIP=true` checks relay membership | Usable candidate for automatic membership. This is relay/community admission, distinct from NIP-29 channel membership. Owner/admin key custody and idempotency need POC |
| NIP-29 channel messages and discovery | `NOSTR.md`: kind 9, kinds 39000/39001/39002, channel operations, reactions, deletion and NIP-10 threads | Direct Buzz client capability documented; Elembra adapter has no verified parity yet |
| DMs | `NOSTR.md`: NIP-17 gift wraps (kind 1059), community-global storage and standard group discovery | Documented subset only; NIP-04/NIP-44 and kind 10050 are explicitly absent/deferred. Elembra DM UI is gated pending compatibility |
| Operator member CLI | `NOSTR.md`: `buzz-admin add-member/remove-member/list-members`; requires database, Redis and relay private key | Exists, but is an operator surface and not a proposed app-server integration. Prefer tested NIP-43 signed events |
| Host/community isolation | `README.md` and `NOSTR.md`: single relay URL maps to one community; multi-community routes derive scope from host | Use one isolated relay and host per disposable tenant in Phase 1. Shared multi-tenant hosting is not qualified |
| NIP-11 relay info | `NOSTR.md` | Discovery metadata is available but does not replace RustShare's pinned signed community identity and authorization proofs |
| Buzz desktop key import/export | `block/buzz-app` README at the separate SHA: create/use existing `nsec1…`; Settings can copy private key | Source documentation only. GUI import, relay join and same-key two-way messaging remain POC acceptance items |
| Elembra signed authorization/state API | No stock counterpart in audited `NOSTR.md`; Elembra APIs are fork-specific | `check`, `check-batch`, `channels`, `state/events`, community discovery and membership-check must not be called in upstream profile |
| Existing-user identity replacement protocol | RustShare M4.1b2 replacement/recovery guards; downstream v1alpha2 operation/revision receipts | No upstream equivalent established. Leave disabled and preserve all guards |

Sources: [relay NOSTR.md at audited SHA](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/NOSTR.md), [relay README](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/README.md), [desktop README at audited SHA](https://github.com/block/buzz-app/blob/662e91800af8204ec0e492522b8f23ef23b19baa/README.md).

## File/function-level migration matrix

| RustShare file / symbol | Current behavior and fork dependency | Stock Buzz status | Proposed migration / gate |
|---|---|---|---|
| `config/buzz-compatibility.env`; `docker-compose.alpha.yml`, `docker-compose.conformance.yml` | Pin downstream commit/image/digest and conformance contract | No qualified stock digest or runtime profile | Keep unchanged. After POC, add a separate profile and provenance-pinned image in a later reviewed PR |
| `frontend/src/lib/chat/keys.ts::{saveChatKey,loadChatKey,exportChatKey,importChatKey}`; `session.ts` | Browser-held identity encrypted in local storage and loaded for local signing | Nostr keys work with stock clients; custodial lifecycle is an Elembra feature | New isolated KMS-backed identity mode only after ADR-0044 approval. Preserve legacy mode; no silent key migration |
| `frontend/src/lib/chat/nostr.ts::{signEvent,publishEvent}` | Browser signs events and performs NIP-42 relay AUTH/publish | NIP-42 and kind-9 protocol are documented upstream | POC should use disposable client keys; server-side typed signing remains unimplemented and must not enable web Chat yet |
| `backend/server/src/handlers/chat_identity.rs::{create_challenge,verify_binding,initiate_replacement,recover_replacement,replacement_initiation_enabled}`; `backend/crates/storage/src/chat_identity.rs` | Public-key proof, binding, replacement state and release/runtime gates | Stock Buzz does not replace Elembra principal binding or the downstream replacement receipt protocol | Do not bypass or weaken. `BUZZ_REPLACEMENT_V1ALPHA2_QUALIFIED=0`; existing-key migration/replacement remains blocked |
| `backend/crates/resource-auth/src/chat_identity.rs` | Builds NIP-43 membership commands for the existing bridge | Upstream documents NIP-43 9030/9031 relay membership | Reuse protocol knowledge only after POC establishes accepted events, signer role, idempotency and retry behavior; owner key is distinct from every managed user key |
| `backend/server/src/buzz_bridge.rs::BuzzAdmissionBridge`; `frontend/scripts/alpha-buzz-ops.mjs` | Durable admission/replacement operations; bridge service identity sends NIP-43 plus fork conditional revision/operation receipts | Basic NIP-43 events are stock; `elembra-op`, expected-revision and downstream replacement receipts are not established upstream | Keep current bridge profile and tests. New profile can use only plain documented NIP-43 after POC; no reuse of replacement receipt assumptions |
| `backend/server/src/services/chat_bootstrap.rs::provision`; `handlers/chat_identity.rs::{configure_mapping,provision_community_mapping}` | Current zero-config bootstrap and bridge provisioning against fork contract | Stock NIP-43 can add relay members; no verified signed community-discovery HTTP endpoint or shared tenant creation API | Provisioning can be proposed as NIP-43 by a separately protected tenant relay owner. Keep workspace mapping/host validation separate; Phase 1 uses manually provisioned single relay |
| `backend/server/src/buzz_gateway.rs::{check_access,check_access_batch,page_state,...}`; `backend/crates/resource-auth/src/buzz_authority.rs::BuzzAuthority` | Calls signed fork HTTP access checks/state pages; current source authorization fails closed | No matching stock HTTP contract established | Do not substitute a local ACL or admin key. Design subject-bound NIP-42 reads and prove per-resource authorization before enabling any dependent view |
| `backend/server/src/handlers/chat_app.rs::{list_channels_from_registry,list_authorization_still_valid,list_messages,get_message}` | Channel discovery and message reads use fork registry plus current authorization checks | Standard NIP-29 discovery is documented, but it is not a proven equivalent to each RustShare resource authorization result | Keep these reads on existing fork path; upstream-only timeline stays disabled until request identity, access denial and pagination are proven |
| `frontend/scripts/buzz-observer.mjs` | Verifies signed kind-19030 responses, channel registry and fork state feed | Stock NIP-29 events support direct client discovery; signed 19030 registry is fork-specific | Replace only with protocol observer proven for host, event identity and membership changes; no Memory/timeline feed from unverified public queries |
| `backend/server/src/handlers/memory_reconcile.rs::{reconcile_chat_memory,reconcile_chat_memory_from_buzz_for_tenant}`; `backend/server/src/memory_projection.rs`; `backend/crates/memory/src/project.rs` | Rebuilds catalog from fork signed paginated state; catalog is candidate data, not authority | Standard `REQ` exists, but stable paginated reconciliation and equivalent authority are not established | Keep current profile only. Disable Chat-backed Memory in upstream profile until fresh subject-bound read, denial, cursor, tombstone and tenant isolation gates pass |
| `backend/server/src/handlers/chat_resource.rs::{prepare_attachment,preview_attachment,open_attachment}` | Reauthorizes Elembra Files ResourceRefs | Buzz does not replace Elembra Files authorization | Preserve source authorization on every open; attachment wire/UI compatibility is a separate gate |
| `frontend/scripts/alpha-buzz-ops.mjs`; `frontend/scripts/chat-bootstrap.mjs`; `scripts/run-buzz-conformance.sh` | Alpha membership/bootstrap and custom v1alpha1/v1alpha2 relay contract probes | Standard NIP-43 operations exist; custom endpoints and revision/conditional receipts do not | Keep these checks bound to the current fork profile. Add separate upstream POC scripts; never reinterpret current conformance as stock compatibility |
| `backend/server/src/handlers/chat_identity.rs::revoke_principal`; `backend/crates/storage/src/chat_identity.rs` offboarding state | Stops Elembra binding and queues relay lifecycle work | 9031 removal is documented; active socket denial and existing exported-key limits are unknown | POC fresh reconnect and existing socket after removal. No immediate-revocation claim or rollout until observed behavior is accepted |

## Architecture adoption and current phase gates (2026-10-31)

The project owner accepted ADR-0044 Option A as the target architecture and
approved NIP-29 channels plus native private-channel DMs as the initial scope.
NIP-17 remains excluded/disabled; its randomized-timestamp relay rejection
remains a historical failure. Existing browser-held and fork-backed runtime
behavior is unchanged. Phase 2A is only an inert, disabled-by-default vault;
no user provisioning, signing, key export, relay integration, migration or
deployment is authorized in that phase.

One isolated relay/community per tenant is the accepted topology. The mapping
model is not a production enforcement path. `strict_public` remains the
current policy; a private/bundled `trusted_service` exception requires
independent security review, exact service identity and endpoint allowlisting,
TLS/peer pinning, network-level egress enforcement and integration tests
before any internal destination is permitted.

| Gate | Current status |
|---|---|
| Gate D — adopted design docs | In the focused ADR PR; protected review/merge pending |
| Gate V — inert Phase 2A vault | Authorized to implement; not yet implemented/reviewed |
| Gate I — managed Chat integration | NO-GO: native desktop test remains unverified and is tracked by issue #442; mapping transport/security approval is pending |
| Gate R — production release/cutover | NO-GO: separate operational, security, recovery and per-tenant migration decisions remain |

These decisions represent accepted product architecture, not independent
security acceptance or runtime enforcement. The historical Phase 1 evidence
and result tables above are not rewritten by this adoption record.

## Immediately usable vs gated

**Documented stock capability:** direct Buzz client NIP-42 AUTH, NIP-29 channel discovery/messages, NIP-43 relay member commands, NIP-17 gift-wrap DM discovery, and desktop `nsec` import. These are source-level findings only; Phase 1 must exercise the selected build.

**Must remain disabled for an upstream-only Elembra profile:** Chat-backed Memory, any timeline/read path whose final gate is the fork's `BuzzAuthority`, automatic Elembra web sends using managed signing, identity migration/replacement, shared multi-tenant relay mode, and any guarantee of immediate offboarding. Keep current fork-backed users and production image on their current path.

## Executable Phase 1 disposable POC

Run this only on a disposable host/worktree. It must not connect to production, use real identities, or change the RustShare compatibility file.

1. Fetch the immutable relay source and record a clean tree:

   ```sh
   git clone https://github.com/block/buzz.git /tmp/elembra-buzz-poc
   git -C /tmp/elembra-buzz-poc checkout --detach eafd77e9f6e96052476bdd88be732c046fdb7ab3
   test "$(git -C /tmp/elembra-buzz-poc rev-parse HEAD)" = eafd77e9f6e96052476bdd88be732c046fdb7ab3
   git -C /tmp/elembra-buzz-poc status --porcelain
   ```

2. Follow that checkout's **Build & run from source** instructions and the disposable compose profile at `deploy/compose/`. Configure `BUZZ_REQUIRE_AUTH_TOKEN=true`, `BUZZ_REQUIRE_RELAY_MEMBERSHIP=true`, a test-only `RELAY_OWNER_PUBKEY`, fresh local Postgres/Redis and a stable test relay key. Record the exact image digest. Do not add source patches.
3. Generate one throwaway relay owner and two throwaway user keys. Use the upstream `NOSTR.md` `nak` examples: authenticate the owner with NIP-42; publish kind 9030 for each user's pubkey; then authenticate each user with NIP-42. Verify a nonmember is rejected before admission and each admitted user can access the relay. Never print secret values into retained logs.
4. Create a public and private channel using the documented NIP-29 flow. Verify kind-39000/39001/39002 discovery, kind-9 send/history, unauthorized private-channel reads, and the documented NIP-17 DM discovery subset. Save sanitized event IDs, protocol frame types and outcomes only.
5. Install/run the separate `block/buzz-app` beta package at audited revision `662e91800af8204ec0e492522b8f23ef23b19baa` (or record why that exact package cannot be built). Import one disposable user's `nsec`, configure the test relay, and prove relay-to-desktop and desktop-to-relay messaging use the same pubkey and event ID/history. This is GUI coverage; README instructions alone do not count as pass.
6. While a member has an open socket, send NIP-43 kind 9031 for that user. Test both the existing socket and a fresh reconnect. Record when denial takes effect, whether a new NIP-42 session is rejected, and whether old subscriptions still return content. Any open-session access after removal blocks a claim of immediate revocation.
7. Capture exact source/image hashes, compose configuration with secrets removed, sanitized protocol traces and a pass/fail table. Confirm no request touched `/api/v1/relay/access/*`, `/api/v1/relay/channels`, `/api/v1/relay/state/events`, `/api/v1/relay/community` or `/api/v1/relay/membership/check`. Stop and destroy only the disposable services after evidence is saved.

**Phase 1 exit gate:** exact unmodified source and image provenance; NIP-42; NIP-43 add/remove semantics; kind-9 history and channel discovery; private channel and DM behavior; desktop `nsec` import and bidirectional messaging; active-session/offboarding behavior; and isolation/failure behavior are evidenced. Passing this POC does not authorize KMS implementation, managed key custody, production pin changes or migration; those require ADR-0044 and independent human security review.

## Phase 1c acceptance update (2026-10-31)

| Gate | Current result | Evidence / limitation |
|---|---|---|
| Exact stock source | PASS for protocol baseline; desktop runtime pending | Relay source/tree and image provenance remain recorded in `elembra-buzz-phase1-poc-evidence.md`; desktop GUI must be run at the audited app SHA. |
| NIP-29 channel protocol | PASS | The 2026-10-31 disposable run passed NIP-42/NIP-43, kind-9, history, discovery, and private-channel membership. New outsider private-DM denial and bidirectional protocol-client DM assertions also passed; desktop compatibility remains unverified. |
| Stock desktop interoperability | UNVERIFIED | Use `scripts/upstream-buzz-poc/desktop-acceptance.md`. Desktop source/build or a rendered window is not proof of import, key match, bidirectional event IDs, restart/history, or native private-channel DM behavior. |
| NIP-29 native private-channel DM | Accepted v1 scope; UNVERIFIED in stock desktop | Distinct from encrypted NIP-17. Protocol-client relay checks do not replace desktop acceptance, tracked in issue #442 and required before Phase 2B. |
| NIP-17 encrypted DM | Excluded and disabled in accepted v1 scope; production behavior unchanged | Historical randomized timestamp FAIL remains a FAIL in the conformance report and is excluded from the accepted v1 acceptance tally. Keep NIP-17 disabled in the upstream v1 profile. |
| Tenant-to-relay contract | Target topology accepted; 14 policy tests PASS, transport integration UNVERIFIED | `SQLX_OFFLINE=true cargo test -p rustshare-resource-auth --lib` passed 77/77 package tests, including 14 mapping tests. See `docs/contracts/upstream-buzz-tenant-relay-mapping-v1.md`. The model is not production-wired; independent security acceptance and real connector tests remain required. |
| ADR-0044 custody decision | Target architecture accepted; independent security approval pending | The owner-approved Option A and its custodial consequences are recorded in ADR-0044. PR approval is not independent security acceptance. |
| Phase 2A readiness | **GO after Gate D merge; inert vault only** | Phase 2A is limited to a disabled-by-default vault. Desktop acceptance gates Phase 2B; independent security review gates real-user provisioning, signing and export. |

No production Buzz pin, identity mode, cutover behavior or replacement guard is changed by this Phase 1c work.
