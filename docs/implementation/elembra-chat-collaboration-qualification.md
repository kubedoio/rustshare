# Elembra Chat collaboration qualification

Audit: 2026-11-08. **Runtime implementation STOPPED; collaboration acceptance incomplete.**

Inspected protected main `390c84ee50db90841683061c847390da83c58caa` before editing.
This report is a source/contract audit, not a browser or live-relay acceptance run.

## Decision and stop condition

Keep Buzz as the channel, membership and message authority. Do not infer a write
permit from a readable channel, a roster, a successful history request, or an
unlocked signing key. No duplicated Elembra channel ACL, private Buzz database
access, custom Buzz endpoint, or upstream source change is permitted.

[ADR-0046](../adr/0046-managed-nip29-publishing-authorization.md) and
[issue #460](https://github.com/kubedoio/rustshare/issues/460) identify an unresolved
upstream membership-cache/role-transition authorization defect. The selected
upstream revision is disallowed for managed publishing; an official fixed revision
and regression evidence are required. Do not run the affected-pin publisher suite
or disclose private reproduction details. The requested role-downgrade and verified
publication acceptance cannot safely be completed on this basis. Honor the task's
STOP condition rather than adding controls that imply authorization.

The [#323 WebUI pilot waiver](https://github.com/kubedoio/rustshare/issues/323#issuecomment-6683016378)
applies to the existing browser-held-key client-side pilot. It does not establish
upstream remediation or authorize managed signing. This audit changes neither
profile, any gate, tenant isolation, migration history nor deployment pin.

## Exact provenance

| Profile/artifact                    | Recorded revision                                                                                            |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| Current fork's upstream baseline    | `relay-v0.2.1`, `6e5c462ac524de60d7edb46c66130fd779cc9006`                                                   |
| Current supported fork source       | `c9a373beca1d602d02308c687db5535c63f3921f`                                                                   |
| Current supported image             | `ghcr.io/kubedoio/buzz-elembra@sha256:2b6c4b2a1800984283f1893a407997b2d5101b01fd0195669ee5deb84814906a`      |
| Current contract / replacement gate | `v1alpha1` / `BUZZ_REPLACEMENT_V1ALPHA2_QUALIFIED=0`                                                         |
| Selected upstream protocol source   | `block/buzz` `eafd77e9f6e96052476bdd88be732c046fdb7ab3`, tree `04e3a40545bd3b23510ec32b4c8cccee2acef7c5`     |
| Selected desktop source             | `block/buzz-app` `662e91800af8204ec0e492522b8f23ef23b19baa`, tree `80275983372cc8dd840e732c0bb3610e97c7c283` |
| Qualified upstream release image    | **None**; no desktop image/package qualified                                                                 |

The 2026-11-05 local qualification records image/config build digest
`sha256:5048f16debb4355f651bc67301b1123da4a75a42da8d3beed2cbbdb87c550846`
in [its exact evidence artifact](../pilot/evidence/upstream-buzz-local-qualification-20261105T145026Z.json).
Its release provenance is explicitly unverified and not security approved.
The earlier historical corrected-wrapper POC records local relay image/config build digest
`sha256:2a1375c668d31e2d91dccf09155a0e2f1b4b1041eb3a1e21f6f92460efd45459`.
These are historical builds, not selected release registry digests and not
images executed during this audit. Other historical runs have distinct digests;
see [the original evidence](../plans/elembra-buzz-phase1-poc-evidence.md).
The compatibility file, not this report, remains the supported fork pin authority.

## Actual implementation versus required behavior

| Area                      | Current frontend/API                                                                                                                                                      | Required / disposition                                                                                                                |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Profile and capabilities  | `ChatStatus` has enablement, mapping, binding, admission and Ask fields; no typed profile/capability surface                                                              | Explicitly separate legacy client-held/fork from upstream-managed; do not advertise gated operations                                  |
| Discovery                 | `chat_app.rs::list_channels` uses authoritative fork registry in Buzz gateway mode; only local mode uses observation summaries                                            | Upstream must use authenticated NIP-29 discovery, including empty channels; fork registry is not stock protocol                       |
| Discovery metadata        | Nullable name, kind/type, visibility, member, latest event time                                                                                                           | Role, join policy, archive state, authority and unavailable/unknown states needed; membership alone is no write permit                |
| Channel creation          | No native create UI or API in `frontend/src/lib/api/chat.ts`                                                                                                              | Validate name/description/visibility; exact-event relay confirmation; unsupported until qualified                                     |
| Channel info and roster   | Header shows kind, private marker and nonmember label; no roster API                                                                                                      | Subject-bound signed metadata/member reads with denial handling                                                                       |
| Join/leave and membership | No channel lifecycle/member mutation methods or UI                                                                                                                        | Actual NIP-29 rules and relay-confirmed partial/error outcomes; do not substitute relay admission for channel membership              |
| Role controls             | No role field or management controls                                                                                                                                      | Buzz authority required; guest/unknown/nonmember cannot imply posting permission                                                      |
| Composer                  | Browser-local key; `canSend` checks key, draft and sending state, not channel role; header's nonmember label does not disable composer                                    | Known gap, not fixed by this stopped audit; preserve signer and require qualified posting state before calling collaboration complete |
| Message wire/results      | Kind 9 plus `h` for UUID channels, kind 1 fallback otherwise; rejection preserves draft; accepted send and later observation distinguished                                | Prove kind-9 exact event ID/author/channel against selected relay; ACK is not recipient delivery                                      |
| History                   | Paged/single-message API serves authorized observation projection with current source checks                                                                              | Stock profile requires live subject-bound authorization; no reuse of fork authority claims                                            |
| Files/ResourceRef         | Attachment prepare/preview/open retain Files authorization; references are identifiers                                                                                    | Preserve reauthorization on every open, including wrong-user/wrong-tenant denial                                                      |
| Threads                   | No reply composer                                                                                                                                                         | [#243](https://github.com/kubedoio/rustshare/issues/243) remains future scope; fork thread evidence is not stock compatibility        |
| Tests                     | Chat Vitest component/API/protocol tests mock API/publishing or use a fake WebSocket; Playwright configuration exists but existing browser journeys contain no Chat tests | Separate-user browser/relay journeys and their observable results remain unverified by this audit                                     |

Primary implementation references: `frontend/src/lib/api/chat.ts`,
`frontend/src/lib/components/chat/{ChatApplicationView,MessageComposer,ChannelHeader}.svelte`,
`frontend/src/lib/chat/nostr.ts`, and
`backend/server/src/handlers/{chat_app,chat_resource}.rs`.

## Upstream contract for a later qualified implementation

At the selected upstream source, authenticated historical `REQ` kind 39000
provides channel discovery; 39001/39002 provide admins/members using the channel
`d` identifier. Global live discovery does not fan out channel-scoped metadata:
refresh and reconnect must issue historical discovery again. Empty-channel
acceptance must create a channel without a message and assert discovery.

Channel operations are 9007 create, 9002 metadata, 9021 join, 9022 leave,
9000 add/change role, and 9001 remove. Open/private visibility, `join_role`,
archived state, target add policy, last-owner protection and partial multi-add
results matter. NIP-43 9030/9031 are relay admission, not channel membership.
Guests cannot post. True nonmembers may post to open `join_role=member` channels
under Buzz's documented policy; do not claim universal relay nonmember denial.
A conservative Elembra unavailable state is not a new relay ACL.

A future typed capability surface should cover discovery, lifecycle, roster/roles,
history and publication with supported/unavailable/unknown outcomes. Operation
results must distinguish denied, unsupported, invalid, offline/timeout, relay
accepted and observed; timeout after submission is uncertain, not confirmed
failure. Signed snapshots support conservative UI decisions but are not atomic
write permits. No runtime interface or mutation path is introduced in this report.

Source: [pinned NOSTR.md](https://github.com/block/buzz/blob/eafd77e9f6e96052476bdd88be732c046fdb7ab3/NOSTR.md),
[adapter contract](../contracts/upstream-buzz-relay-adapter-v1.md),
[ADR-0044](../adr/0044-managed-buzz-identity-upstream-only.md).

## Feature acceptance checklist

- [x] Inspect protected main, AGENTS.md, ADR-0044, ADR-0046, #323, #243 and adapter contract before edits.
- [x] Record current capabilities, exact source/image provenance and profile separation.
- [x] Record upstream stop predicate without reproducing the private finding.
- [ ] Official fixed unmodified upstream source and exact disposable image selected and reviewed; no production pin change.
- [ ] Typed capabilities/errors implemented and unavailable states rendered explicitly.
- [ ] Authenticated discovery includes empty channels and excludes unauthorized private channels.
- [ ] Create-channel form validates name/description/open-private and displays relay-confirmed results.
- [ ] Channel info, roster, join/leave, add/remove and role controls follow actual relay decisions.
- [ ] Guest/read-only, role downgrade, nonmember and unknown states cannot imply posting authorization.
- [ ] Existing composer, identity recovery and Files/ResourceRef authorization preserved.
- [ ] Disposable two-tenant private discovery/history denial, role transitions and reconnect verified.
- [ ] Exact selected relay publication/history event IDs and authors recorded without message bodies or keys.
- [ ] Real Playwright journeys use separate user contexts; exercise two-way messaging, empty/private creation, denied history, role downgrade, reconnect and attachment authorization.
- [ ] Browser outcomes include exact source/image, commands, sanitized frames/event IDs, failures and unverified cases; mocks alone do not count.
- [ ] Small runtime PRs independently reviewed and required area checks pass.

No production deployment, release pin change or managed publisher activation is
part of this acceptance checklist's execution authorization.

## Commands and results from this audit

- `git fetch origin main` — exit 0; resolved main to the SHA above.
- `gh api repos/kubedoio/rustshare/branches/main --jq '{name,protected,sha:.commit.sha}'` — exit 0; `protected: true`, same SHA.
- `git worktree add -b codex/chat-capabilities /tmp/rustshare-chat origin/main` — exit 0; isolated worktree; pre-existing workspace changes preserved.
- Read-only GitHub connector reads of #323, #243, #460 and #323 comments — successful after correcting the comments tool's repository argument.
- Targeted `rg` and bounded source reads — confirmed the matrix above.

No runtime files changed. No compiler, application test, browser, Docker, relay
mutation or publisher suite was executed for this documentation-only stop report.
Historical POC results are references, not rerun evidence. All unchecked acceptance
items remain incomplete; this report does not make Chat production-ready.
