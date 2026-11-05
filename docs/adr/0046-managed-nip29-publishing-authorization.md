# ADR-0046: Managed NIP-29 Publishing Authorization and Signing Boundary

- **Status:** Proposed target: Option A, explicitly selected by project owner on 2026-11-04; independent security acceptance and upstream membership-cache fix required before implementation
- **Date:** 2026-11-04
- **Related:** ADR-0044, ADR-0045, managed-chat-identity-v1 contract, issues #442/#458/#460, merged PR #457
- **Upstream source basis:** unmodified `block/buzz` commit `eafd77e9f6e96052476bdd88be732c046fdb7ab3` (tree `04e3a40545bd3b23510ec32b4c8cccee2acef7c5`)

## Decision summary

This ADR defines the target architecture for a future internal Elembra-managed NIP-29 kind-9 publisher. It does not authorize implementation, signing, publishing, route activation, Memory access, production use, or a Buzz modification.

**Selected target: Option A — typed Elembra-side authorization and signing, with unmodified Buzz as final ingest authority.** Project owner `senolcolak` explicitly selected Option A and accepted the portable signed-artifact, plaintext visibility, replay and revocation risks in [PR #459 comment](https://github.com/kubedoio/rustshare/pull/459#issuecomment-6640798406). This is a product architecture decision only. Independent security acceptance remains pending, and no signing implementation or activation is authorized.

Option B is preserved below as a rejected/deferred alternative. The pinned stock Buzz source has no trusted pre-sign write-authorization capability bound to the exact operation. Signed kind-39000/39002 snapshots and NIP-42 private-channel read access are useful for discovery and conservative denial, but are not write permits. Option B would require a new upstream atomic authorization capability unavailable at the pinned revision.

Source review identified an upstream membership-cache/role-transition authorization defect. Its reproduction details have been submitted through Buzz's private vulnerability-reporting process and are intentionally omitted from this public ADR. Managed publishing is blocked until upstream triages and fixes the issue without breaking intended anonymous/open-channel behavior, the regression tests below pass, and Elembra adopts an official unmodified Buzz pin containing the fix. The `senolcolak` [`ACCEPT` on PR #457](https://github.com/kubedoio/rustshare/pull/457#issuecomment-6637880199) remains scoped to the history reader and Cilium deferral; it is not independent security approval.

## Scope and trust boundaries

The contemplated operation is one canonical NIP-29 kind-9 event for an authorized channel message or native private-channel DM. NIP-29 private channels are relay-access-controlled, not end-to-end encrypted; message content is plaintext to the relay and any holder of the signed event. NIP-17 remains disabled.

Elembra is authoritative for the authenticated Principal, current tenant/workspace membership, Chat entitlement, application profile, managed identity and generation, trusted relay mapping and generation, local resource access, and request idempotency. Buzz is authoritative for its community, channel state, membership, role and ingest-time write decision. A successful private-channel REQ proves only that Buzz permitted that read; it does not prove write permission.

No design option may use a caller-selected private key, raw caller-provided Nostr event, arbitrary event kind/tags, caller-selected relay URL, relay-admin key, fork-only Buzz API, or Buzz private database access. Direct stock Buzz clients remain direct clients; Elembra must not require them to use an Elembra-specific API.

## Pinned Buzz kind-9 ingest behavior

The code path below is verified at the pinned commit above. It receives an already signed event; no pre-sign write-authorization endpoint or receipt exists in this source.

1. WebSocket `EVENT` and HTTP event ingestion converge on `ingest_event_inner` ([source](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/crates/buzz-relay/src/handlers/ingest.rs#L2440-L2540)). The relay first checks the community serving fence and rejects relay-only kinds.
2. `verify_event` validates the event ID and signature. Kind 9 is `KIND_STREAM_MESSAGE = 9` ([kind constant](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/crates/buzz-core/src/kind.rs#L527)). Ingest checks the timestamp is within ±900 seconds and content is at most 256 KiB. On the managed WebSocket path, the event pubkey must match the connection's NIP-42-authenticated `IngestAuth` pubkey; the shared HTTP path binds to its transport-specific authenticated pubkey. Kind 9 requires `MessagesWrite`; account write restrictions also apply ([ingest validation](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/crates/buzz-relay/src/handlers/ingest.rs#L2582-L2690)).
3. The event's `h` channel ID is parsed and required for channel-scoped kinds. A channel-scoped token must include that channel. Buzz loads the community-scoped channel row and calls `check_channel_membership` with `ChannelWrite::Post` ([channel gate](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/crates/buzz-relay/src/handlers/ingest.rs#L2770-L2925)).
4. For an active member, Buzz requires the current database role to satisfy `MemberRole::can_post()`; owner/admin/member/bot roles can post and guest cannot. A nonmember may post only in an open channel whose `join_role` is `member`. Unknown join roles fail closed ([role rules](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/crates/buzz-relay/src/handlers/ingest.rs#L895-L977)). Buzz separately rejects archived channels. Kind-specific message/tag checks then run before persistence and fanout.
5. Membership is checked with `AppState::is_member_cached`; the cache TTL is 10 seconds ([cache configuration](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/crates/buzz-relay/src/state.rs#L1601-L1612), [lookup](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/crates/buzz-relay/src/state.rs#L1776-L1790)). Membership-change invalidation is local and published to other pods asynchronously; TTL is the backstop ([invalidation](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/crates/buzz-relay/src/state.rs#L1840-L1868)). Role lookup behavior differs depending on the cached membership result, so role transitions can observe different authorization paths. Buzz reads channel/membership/role state before the separate event-insert transaction; these checks are not atomic with Buzz persistence or Elembra's pre-sign operation. Revocation, role change, archive or channel-state change can race a prior read or snapshot. A potential defect in this interaction has been privately reported upstream; see issue #460 for the public publisher gate without reproduction details.

Relay-signed kind 39002 exposes an active roster as `p` tags with roles ([member snapshot](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/crates/buzz-relay/src/handlers/side_effects.rs#L1301-L1354)). Kind 39000 metadata includes visibility, `join_role` and archived tags ([metadata snapshot](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/crates/buzz-db/src/store/channel.rs#L695-L769)). These signed replaceable snapshots have no request nonce, Elembra operation ID, mapping/identity generation, or atomic binding to a later EVENT. The member list does not itself establish write permission: open-channel nonmember posting also depends on visibility/join role, member posting depends on role, and archived/serving/token/account restrictions still apply. Current private-channel read access is not a write check.

The pinned code and documented protocol provide no trusted pre-sign write-check endpoint, one-use authorization capability, or event reservation. Kinds 39000/39001/39002 describe state; kind 9000 and related events mutate membership; an EVENT/OK response occurs after signing and submission. A client-generated snapshot check is advisory only.

## Selected option A — typed signing, Buzz final enforcement

Elembra would perform all current local authorization and construct a narrow typed intent before signing:

- authenticated, active Principal, tenant and workspace membership;
- active Chat entitlement and explicitly approved non-production profile;
- ready identity, public-key binding and current identity/key generation;
- valid trusted mapping and unchanged mapping generation;
- local channel resource authorization and permitted attachment references;
- scoped idempotency key and canonical request digest;
- current managed identity membership plus a posting role as a conservative precheck (never treating an open channel's public posting rule as a substitute for application authorization);
- fixed kind 9, canonical `h` tag, bounded content and a closed set of permitted tags.

The signer recovers the managed key only inside this typed operation. Buzz remains the final write authority and may reject the event after it has been signed. The typed internal input is limited to `ChannelMessage { target_resource, content, permitted_attachments, idempotency_key }` or `PrivateChannelDm { target_resource, content, permitted_attachments, idempotency_key }`. Both construct fixed kind 9; the exact `h` channel tag and permitted attachment tags are derived from trusted resources. The caller cannot select the key, signer, relay URL, pubkey, raw event, kind, timestamp, tags or arbitrary metadata.

Before key recovery/signing, require the current authenticated active Principal, tenant/workspace membership, Chat entitlement and approved non-production profile; an active ready managed identity/public-key binding and identity generation; current trusted mapping ID/generation; exact target resource authorization; bounded policy-checked content and authorized attachment references; and a scoped idempotency key. Resolve the relay destination and channel ID exclusively from trusted state. Verify the most recently fetched relay-signed channel/member snapshots for the exact channel and managed pubkey, with a post-capable role; enforce a configured maximum snapshot age as a stale-data denial threshold, not as proof of freshness. If membership, role, age or relay authority is unknown, deny. This is a conservative precheck, not an atomic grant; Buzz makes the final ingest decision. The current pin is disallowed for a managed publisher pending the privately reported cache/role finding and an official upstream fix.

Only the designated typed signing service may recover the key through the existing vault. Minimize plaintext-key lifetime and zeroize where feasible; never return key material and provide no general sign/decrypt API. Revalidate subject, target, entitlement, identity generation and mapping generation immediately before network transmission and before every retry. If state changes after signature persistence, do not send or retry the event.

### Option A threat model and residual risk

The managed signer is a high-impact impersonation boundary. A bug or compromised caller could ask it to create a valid message as the user. A race can make a request authorized by Elembra at precheck time invalid by Buzz ingest time. A relay `OK false` does not erase the signature. Anyone who obtains the event can verify and forward it to another relay, possibly one that accepts it; the event's `h` tag is not encryption. Content can disclose private-channel text outside the intended relay ACL. Signature revocation is impossible. A lost ACK is ambiguous and must not be treated as a refusal or as recipient delivery.

Option A preserves the currently documented stock Buzz write path and direct-client compatibility, but the typed signer cannot promise that a signed event will be accepted by Buzz. This is a security risk, not just a user-visible failed send. The project owner accepted the architecture and these residual risks in [PR #459 comment](https://github.com/kubedoio/rustshare/pull/459#issuecomment-6640798406). A separate independent security ACCEPT remains required; normal code review and the history-reader ACCEPT do not suffice. The privately reported membership-cache/role-transition defect is a separate implementation blocker until fixed in an official unmodified pin.

## Deferred alternative B — require stronger pre-sign authorization

Do not sign until a trusted mechanism can prove write eligibility for the current managed public key, channel and canonical operation. The proof must be bound to at least:

- relay/community identity and channel ID;
- managed public key and key/identity generation;
- kind 9 and canonical event/request digest (including content, timestamp and exact allowed tags);
- operation/idempotency identifier and a one-use nonce;
- current membership, write role, channel policy/archive revision, and short expiry;
- an authorization decision that Buzz can atomically validate and consume at ingest.

A simple read-only preflight endpoint or signed 39000/39002 snapshot is not sufficient: state can change between the response, signature creation and ingest. To provide useful semantics, an upstream mechanism needs a reservation or linearized capability with explicit revocation ordering. For example, Buzz could issue a short-lived one-use permit bound to the exact unsigned canonical event digest and authenticated pubkey, then validate/consume the permit under the same channel-state lock/transaction as ingest. Revocation and permit consumption must serialize: if revocation commits first, submission denies; if permit consumption commits first, the operation is authorized at that linearization point. A preflight-only response that is not consumed atomically leaves the TOCTOU gap.

This capability does **not** exist in the pinned stock Buzz. Delivering it requires an additive upstream protocol/API capability adopted from an official unmodified Buzz release, or another separately accepted architecture that preserves an unmodified upstream deployment. Do not implement the capability in an Elembra middleware, fork, custom relay endpoint, or private database query. If no suitable upstream contract is available, Option B means managed publishing stays blocked; native direct stock clients continue independently.

Even a permit does not make signatures revocable. The project and independent security reviewer must decide whether a valid event rejected after permit expiry/revocation can exist outside Buzz. If the requirement is that no user signature can exist for a rejected publication, external signing cannot meet it: only a signing/commit boundary that can prevent a signature from escaping until the relay commit would address that stronger property, and that would require a new custody architecture and decision.

## Decision status and independent review

Option A is the project-owner-selected target. Option B is not selected because the required upstream operation-bound atomic authorization is unavailable; it remains a higher-assurance alternative to revisit if an official stock Buzz release supplies it. An authorized independent security reviewer must separately ACCEPT, REJECT or REQUIRE CHANGES for Option A, its custody boundary, the signed-artifact/replay/plaintext/revocation risks, the cache defect and fix evidence, idempotency/recovery, relay trust and redaction. Neither decision authorizes production Chat, Memory, NIP-17, desktop migration or deployment.

## Durable operation protocol (design only)

The later implementation must use a typed internal intent; no generic Nostr signer. Resolve tenant/workspace, Principal, entitlement, identity, generations, mapping, relay and resource from trusted application state. The request supplies only target resource, message content, allowed references, and a scoped idempotency key.

1. **Claim atomically before signing.** Insert/claim under a unique scope binding tenant, workspace, Principal, identity ID/generation, mapping ID/generation, channel ID, and idempotency key. Store a canonical request digest. A matching duplicate returns the existing operation; the same key with another digest is a conflict. Concurrent callers cannot sign different operations under one key.
2. **Prepare one immutable event template.** Persist the canonical kind-9 preimage, timestamp, exact channel context/tags and request digest before signing. Because the preimage contains plaintext message content, encrypt it before persistence under a tenant-scoped key. It must be impossible to silently change content or event fields on recovery.
3. **Sign only after authorization.** Recover the existing key through the typed vault boundary, zeroize plaintext as soon as possible, sign only the prepared event, and persist the exact signed serialized bytes encrypted before any network send. Persist event ID, author and signature metadata separately for indexed equality checks; never persist plaintext bytes in logs, audit, tracing or generic jobs.
4. **Close the sign-to-persist crash gap.** The design must prove that a crash after signature creation but before event-byte persistence cannot produce a second distinct signed event for the same operation. Use a reviewed deterministic signing/recovery contract or a durable signer journal that atomically retains the output before acknowledging and returns the exact prior bytes for a repeated operation. If the persistence boundary can lose a signature, or the existing signer cannot guarantee byte-identical recovery, fail closed and leave the operation recoverable; do not silently re-sign. No custom nonce construction is allowed.
5. **Publish exact bytes only.** After signing and durable persistence, revalidate the active subject, tenant/workspace, Chat entitlement/profile, conservative membership/role precheck, channel authorization, identity/key generation and mapping generation immediately before the first network send. Store `publish_attempted` before sending and send the stored signed event verbatim to the mapped relay. Revalidate the same state before every retry. Buzz's kind-9 timestamp window is ±900 seconds: after expiry, do not alter the timestamp or regenerate the event; resolve an ambiguous send by exact-ID history lookup where authorized, otherwise retain `uncertain/expired` and require a new user operation with a new idempotency key. A lost ACK becomes `uncertain`, not `failed`; never retry after revocation or stale generation.
6. **Separate outcomes.** Distinguish `prepared`, `signed`, `publish_attempted`, `relay_accepted`, `observed`, `failed`, `uncertain`, and `delivered` only if an independent recipient receipt exists. A relay OK for the exact event ID means only `relay_accepted`; later REQ history can establish `observed`. Neither proves recipient delivery.
7. **Protect stored content.** Encrypt both the canonical preimage and exact signed event bytes, plus any retained message content, with tenant-scoped storage keys. Bind AEAD associated data to tenant, workspace, Principal, identity ID/generation, mapping ID/generation, channel ID and operation/idempotency key so ciphertext cannot be transplanted across same-tenant operations. Restrict row access with existing tenant RLS/service boundaries. Specify bounded retention, deletion and tombstoning before implementation. Keep audit entries to actor/resource/generation/operation/status and redacted reason—never message bodies, keys or unkeyed content-derived digests. If an audit correlation digest is necessary, use a tenant-scoped keyed HMAC; retain the canonical request digest only in protected operation state. KMS/database failure denies new signing and prevents retry until current authorization can be re-established.

## Required executable acceptance tests

The implementation proposal must add Rust unit and PostgreSQL coverage for:

- missing, wrong, disabled or revoked Principal; wrong tenant/workspace; missing Chat entitlement/profile;
- cross-tenant and wrong-channel targets; unauthorized private channel; active member with guest/read-only role;
- the managed precheck must deny unknown/nonmember or guest roles even when a public channel may accept an actual nonmember under `join_role=member`; the live test must assert Buzz's documented policy outcomes rather than expect all nonmembers to be refused;
- **upstream regression blocker:** test stale membership-cache results across membership creation/removal and role upgrades/downgrades, within the TTL and across relay pods. Assert that current role policy is enforced while intended true-nonmember behavior for open channels is preserved. Test async invalidation and TTL expiry. Reproduction details are in the private upstream report, not this public plan. Do not implement or ship the Elembra publisher against a Buzz pin until the upstream fix passes these tests;
- membership removal, role downgrade, identity/key-generation change, mapping-generation change and channel archive between precheck, signing, send and retry;
- operation/idempotency scope, concurrent duplicates, same-key/different-digest conflict and exact canonical event/event ID preservation;
- crash after claim, after sign-before-persist, after durable signed-event persistence, after send-before-ACK, and lost/negative ACK followed by exact-ID history observation;
- retry after revocation or authorization generation change (no resend), timeout/relay refusal, DB/KMS outage and recovery;
- NIP-42 author binding; arbitrary kind/tag/key/relay injection; content/attachment validation; no secret/message-body leakage in logs, audit, traces, operation rows or evidence;
- separate relay accepted, observed and recipient-delivered state assertions.

Live publisher acceptance must use an official unmodified Buzz pin that contains the upstream cache/role fix, in two isolated communities with Cilium default-deny egress. It must record exact signed event IDs, authors, channels and relay responses; test that a refused signed artifact remains independently verifiable and can be replayed on a disposable accepting relay; and exercise documented nonmember and role-transition policies. Include authorized reads, private denial with zero EVENT frames, cross-tenant/wrong-identity reads, revocation on active sessions, mapping changes, and database/KMS faults. No Buzz patch, private database access, production credential or deployed image is allowed.

Issue #458 tracks the managed-history Cilium qualification and remains mandatory before managed Chat activation. Issue [#460](https://github.com/kubedoio/rustshare/issues/460) separately tracks publisher conformance and the upstream cache-fix blocker; do not count #458 history coverage as publisher evidence. The current development host lacks `kind`; the repository's existing GitHub Actions fleet uses ephemeral hosted Linux runners but has no Cilium job. The candidate runner is GitHub-hosted `ubuntu-24.04` amd64 with Docker Buildx and pinned kind v0.27.0, Kubernetes v1.32.2, Cilium v1.16.4, Cilium CLI v0.16.22, Hubble v0.13.2, Rust 1.97.1 and Node 22. Add/approve the dedicated workflow separately and do not claim a run until exact evidence exists. Issue #442 remains the separate stock-desktop interoperability gate.

## Implementation entry criteria

No signing or publishing implementation may start until all of the following are recorded:

1. Project-owner Option A selection and acceptance of artifact portability, replay, plaintext and revocation risks is recorded in [PR #459](https://github.com/kubedoio/rustshare/pull/459#issuecomment-6640798406).
2. A named authorized independent security reviewer records a separate ACCEPT/REJECT/REQUIRE CHANGES verdict for authorization, custody, relay trust, idempotency/recovery, replay, stale-cache finding/fix and redaction. This remains pending.
3. The privately reported upstream membership-cache/role-transition authorization defect is fixed and regression-tested in an official unmodified Buzz revision; current pin is disallowed for a managed publisher. Track remediation and conformance in #460.
4. Typed request model, conservative precheck limits, operation uniqueness scope, exact-byte crash recovery, encrypted retention/deletion and audit schema are reviewed before coding.
5. Rust/PostgreSQL and disposable Buzz/Cilium test plans are executable; #458 managed-history qualification, #460 publisher conformance, and #442 desktop interoperability remain independent gates. No production route, worker, default-on feature, real-user signing, Memory shortcut, credential export, deployment or legacy migration is included.

## Conditional Phase 2B.3 implementation prompt

After independent security acceptance and the upstream guest-cache fix is verified at an official unmodified Buzz pin, implement only the selected typed Option A NIP-29 kind-9 publishing boundary. Start from latest protected main. Reuse the existing managed identity vault, operation store, trusted mapping and upstream-only transport. Implement no arbitrary-event signer or production route. If the upstream fix or exact-event recovery cannot be proven, stop with the publisher feature disabled and report the blocking predicate. Do not begin Phase 2B.4 runtime integration until implementation tests, #458 history Cilium evidence, separate publisher conformance and #442 desktop acceptance are complete.
