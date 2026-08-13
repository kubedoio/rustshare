# Elembra Chat Application v1 — Design

> **Status:** Design (on `feat/elembra-chat-app-v1`).
> **Date:** 2026-08-12
> **Related:** ADR-0034 (Elembra Chat and Buzz Boundary), ADR-0035 (Buzz Source
> Authorization Gateway), ADR-0036 (Unified Search),
> `docs/specs/buzz-upstream-authorization-v1alpha1.md` (upstream contract),
> `docs/specs/resource-ref-authorization-v1alpha1.md`,
> `docs/specs/unified-search-v1alpha1.md`,
> `docs/implementation/buzz-source-authorization-gateway.md`,
> `docs/implementation/buzz-memory-projection.md`.

## 1. Goal

Deliver the usable vertical slice: Elembra login → Workspace → Chat Application →
Buzz community/channels → channel messages → send/receive signed messages →
attach an Elembra File → Ask this Channel → citation opens the exact Chat message.

Buzz remains authoritative for communities/channels, messages/events/signatures,
membership, Chat authorization, and thread/reply protocol semantics. Elembra owns
Principal/login, the Workspace/Application shell, Files/ResourceRef,
Memory/Search/Ask, and product navigation/UX. No chat backend is rebuilt inside
Elembra, no private Buzz database is read, and no second chat source of truth is
created.

## 2. Architecture decisions

1. **Projection-first reads, Buzz-authoritative authorization.** The Chat UI's
   channel list and message timeline are served by Elembra from the existing
   observation projection (`chat_observed_events`, folded through
   `memory_catalog` semantics) — the same authorized path Search, Ask, and
   citation-open already use. Every message is gated through the existing
   `ChatResourceOwner` → `BuzzAuthority` chain, which ends in the community
   relay's current decision in `buzz` mode and the coarse workspace-only gate in
   `local` mode. Rationale: no private Buzz DB reads, no parallel ACL, and
   revocations take effect on the very next read. When the Buzz repository
   implements the v1alpha1 `access/check` capability, `buzz` mode upgrades this
   surface with zero Elembra-side read changes.

2. **Client-direct publish; the server never holds a human key.** The browser
   holds the user's Buzz key, signs kind-1 events locally, opens a NIP-42 relay
   session, and publishes. Elembra adds no send endpoint. Rationale: this is the
   only publish path proven against a real relay (ADR-0034 live proof), and it
   preserves the sovereign-identity property — the server workload can never
   sign as a user.

3. **Live updates ride the existing broadcast infrastructure.** On webhook
   ingest, `BuzzObservationService` broadcasts a `ChatMessageObserved` event on
   the existing `EventBroadcaster` (`/api/ws`), and the frontend websocket
   manager invalidates chat query keys. A modest polling fallback covers
   sessions where the websocket is unavailable. Rationale: new messages reach
   the UI within seconds of ingestion without a browser-to-relay subscription
   dependency, and the transport is already proven in this codebase for Files,
   Notes, and other applications.

4. **Read surface is derived, not stored.** Channel lists and timelines are
   queries over the existing observation index; no new tables, no chat message
   table, no schema migration. Rationale: ADR-0034 forbids copying Buzz events
   into Elembra as authoritative chat rows; the observation index is the
   documented projection and already the source for search/ask candidates.

5. **Key custody is client-side and explicit.** First use generates a Buzz key
   in the browser, encrypts it at rest with a user passphrase (WebCrypto
   PBKDF2 + AES-GCM), and offers export/import for multi-device use. Binding
   reuses the existing challenge/verify endpoints unchanged (NIP-42 proof).
   Recovery without an exported key is impossible by design and the UI says so.
   Rationale: ADR-0034 preserves client-side signing and forbids silent server
   custody; v1 implements the minimal honest custody model.

6. **Attachments remain Elembra Files resources.** The composer's picker uses
   the existing Files list APIs; `POST .../chat/attachments/prepare` returns the
   exact `["elembra-ref", "<uri>"]` tag which the browser includes in the signed
   event; preview/open go through the existing reauthorizing endpoints. No blob
   is copied into Buzz and no permanent URL enters the signed event. Rationale:
   ADR-0034's ResourceRef attachment slice is already implemented and proven;
   Chat membership never grants Files access.

7. **Ask Channel is wired through the existing pipeline, not a new one.** The
   Chat app builds `askHref({ type: 'chatChannel', communityId, channelId })`
   and passes `onChatCitationOpen` to `AskExperience`; the server-side
   `POST /memory/ask` and `POST /memory/citations/open` handlers are unchanged.
   The Chat app's citation handler only navigates the timeline to the message id
   embedded in the already-authorized `OpenCitationResponse`. Rationale: no
   Chat-specific RAG, no browser-side retrieval, and the citation cannot bypass
   backend authorization because the backend performs it before returning.

8. **Fail closed everywhere.** A principal with no active binding/admission,
   no mapping, or a disabled Chat application sees safe empty/error states and
   cannot reach message content; `dm`/`private`/`excluded` channels are denied
   by the local gate by design; reference-first messages render as "content
   unavailable in Elembra" placeholders rather than exposing anything.
   Rationale: every boundary in ADR-0034/0035 fails closed; the UI must not
   paper over a denial.

9. **Chat behaves as a first-party Application.** The app plugs into the
   existing renderer registry, icon registry, object-href map, sidebar query,
   and per-tenant enablement gate; no new navigation surface is invented.
   Rationale: the manifest (`io.elembra.chat`, route `/apps/chat`) already
   exists — the gap is purely the renderer and wiring.

## 3. Backend design

New endpoints under the existing authenticated chat route group:

| Endpoint | Contract |
| --- | --- |
| `GET /api/v1/applications/chat/status` | Chat enablement, mapping (`community_id`, `relay_url`), binding status, admission status, own `pubkey`. Safe summary only — never other users' data. |
| `GET /api/v1/applications/chat/channels` | Distinct observed channels for the mapped community (id, kind, latest activity), derived from the observation index. |
| `GET /api/v1/applications/chat/messages?channel_id=&before=&limit=` | Folded timeline for one channel: newest-active message rows (created/edited/deleted resolved to the latest event), `thread_root_id` grouping, `body` when an indexing copy exists, otherwise `body: null`. Paginated by `event_created_at`. |
| `GET /api/v1/applications/chat/messages/{message_id}` | Single folded message (used for citation focus/scroll and thread expansion). |

Authorization:

- `status` starts from `PrincipalContext`, verifies the Chat application is
  enabled for the tenant, then resolves the tenant's active
  `WorkspaceCommunityMapping` (missing/inactive → safe empty response).
- `channels` and `messages` additionally gate per-channel/per-message through
  the existing `SourceAuthorizer`/`ChatResourceOwner` path (existence-hiding,
  tombstone-aware); `get_message` relies on that per-message gate alone. In
  `local` mode this admits `workspace` channels only; in `buzz` mode each
  message is a live relay decision. A denied message is omitted from list
  responses and 404 from the single-message endpoint.
- Message rows are gated through the existing `SourceAuthorizer`/`ChatResourceOwner`
  path (per-message, existence-hiding, tombstone-aware). In `local` mode this
  admits `workspace` channels only; in `buzz` mode each message is a live relay
  decision. A denied message is omitted from list responses and 404 from the
  single-message endpoint.
- No new authorization module, no new ACL, no new tables.

Store additions (read-only): `ChatObservationStore::list_for_timeline`
(tenant + community + channel, folded latest-event-per-message, paginated) and
`get_message_for_timeline` (single folded row). No writes.

Broadcast: `BuzzObservationService` publishes a `ChatMessageObserved` envelope
(community_id, channel_id, message_id) on the existing `EventBroadcaster` after
a successful ingest; the frontend maps it to chat query invalidation. The
broadcast is best-effort and never affects ingest durability.

## 4. Frontend design

New modules:

- `src/lib/api/chat.ts` — client for status/channels/messages endpoints, with
  TanStack Query hooks following the existing `query-compat.ts` idiom.
- `src/lib/chat/nostr.ts` — minimal Nostr event construction/signing (BIP-340
  Schnorr via `@noble/curves`, SHA-256 via WebCrypto) and a NIP-42 relay
  WebSocket publish client (connect → AUTH challenge → publish → close).
- `src/lib/chat/keys.ts` — key generation, passphrase-encrypted storage
  (PBKDF2 + AES-GCM), export/import; the raw key never leaves the browser.
- `src/lib/components/chat/` — `ChatApplicationView.svelte` (channel list +
  timeline + composer), `MessageTimeline.svelte`, `MessageComposer.svelte`,
  `ChannelList.svelte`, `AttachmentPicker.svelte`, `BindingPanel.svelte`.

Wiring:

- `ApplicationPageRenderer.svelte` gains a `chat` renderer entry; the
  `message-circle` icon joins `iconRegistry.ts`/`ApplicationIcon.svelte`; an
  `io.elembra.chat` entry joins `getApplicationObjectHref` (deep link
  `/apps/chat?channel=<id>&message=<id>`).
- `websocket/manager.ts` + `events.ts` gain the `ChatMessageObserved` handler
  (invalidate `['chat-messages', ...]` keys); polling fallback refetches every
  15 s while the chat view is mounted and the websocket is disconnected.
- The composer's attachment flow: picker (existing Files list APIs) →
  `attachments/prepare` → `elembra-ref` tag appended to the signed event;
  rendered attachments call `attachments/preview`/`attachments/open`.
- Ask wiring: the "Ask this channel" button constructs
  `askHref({ type: 'chatChannel', communityId, channelId })`; the `/ask` route
  supplies `onChatCitationOpen` to `AskExperience`, which parses
  `elembra://io.elembra.chat/message/<id>` and navigates to the Chat app deep
  link `/apps/chat?channel=<id>&message=<id>`, where the chat view fetches the
  single message (when outside the loaded window) and focuses/scrolls to it.

States: loading skeletons, empty channel/message states, relay-offline banner
on the composer (publish unavailable; reads unaffected), binding-pending state
that blocks the composer but not the read surface, per-message "content
unavailable in Elembra" placeholder for reference-first rows, and error mapping
that never leaks provider/relay internals.

## 5. Identity/signing flow

1. Login → `PrincipalContext`; `GET .../chat/status` shows unbound state.
2. First use: generate a Buzz key locally; encrypt-at-rest with a user
   passphrase; offer export. (Import path for existing keys.)
3. `POST .../chat/identity-binding/challenge` → browser opens the relay WS,
   signs the NIP-42 AUTH event with the local key, submits the proof to
   `POST .../chat/identity-binding/verify` (one-use, tenant/pubkey/relay
   scoped).
4. `POST .../chat/admission` → `202 queued`; the bridge publishes NIP-43
   kind-9030 to the relay.
5. Publish: the browser signs kind-1 and publishes over its own NIP-42 relay
   session. The server never sees the private key; revocation (user disabled →
   DB trigger → binding/admission revoked → bridge kind-9031) stops future
   authorized reads in Elembra immediately and stops future relay publication
   via Buzz membership.

## 6. Files attachment flow

Select/upload in Elembra Files (unchanged) → pick in composer →
`attachments/prepare` (Files reauthorizes, returns safe metadata + exact
`elembra-ref` tag) → tag included in the signed kind-1 event → publish.
Recipients render the reference and open it through
`attachments/preview`/`attachments/open`, which reauthorize against current
Files permissions — a later share/file revocation denies opening even though
the signed event persists in Buzz history. No attachment blob is duplicated
into Buzz.

## 7. Ask/citation flow

Ask this channel → `POST /memory/ask` with `ChatChannel { community_id,
channel_id }` (existing handler; scope enforced server-side) → cited answer →
user clicks a chat citation → `POST /memory/citations/open` reauthorizes
against Chat authority (existing handler) → `onChatCitationOpen` receives the
`OpenCitationResponse` → the Chat app navigates to
`/apps/chat?channel=<id>&message=<message_id>` and focuses/scrolls to the exact
message. The Ask backend never escapes the selected channel (existing search
scope enforcement); the frontend adds no retrieval logic.

## 8. Security/E2E proof mapping

| # | Proof | Where |
| --- | --- | --- |
| 1 | login → Chat → channel → messages | Backend endpoint tests + frontend view tests |
| 2 | send signed message works | Real-relay script (disposable relay, ADR-0034 recipe) + signing unit tests |
| 3 | live/new message appears | Ingest → broadcast → invalidation test + polling test |
| 4 | cross-workspace/community forgery fails | Endpoint authz tests (tenant A rows never visible to tenant B) |
| 5 | restricted channel access follows Buzz | Local gate denies dm/private/excluded; buzz-mode fake-relay decision tests (existing harness) |
| 6 | revoked user cannot publish/read | Read: revocation → read endpoints deny (automated tests). Publish: relay membership revocation (kind 9031) is verified manually via the disposable-relay checklist in `scripts/run-chat-e2e.sh` — the 9030/9031 orchestration needs the relay's own admin CLI |
| 7 | Files attachment uses Files authorization | Endpoint-level prepare/preview/open denial tests (`chat_app_read_test.rs`) + existing `source_authorization_test.rs` authorizer coverage |
| 8 | unauthorized attachment fails | Denied/inaccessible prepare/open tests (`chat_app_read_test.rs`) |
| 9 | Ask Channel cannot escape selected channel | Existing `unified_search_test` Ask security matrix + citation cross-channel test |
| 10 | citation opens exact authorized message | Single-message endpoint + frontend focus/scroll test |
| 11 | no private Buzz DB reads | Architectural: no new code path outside the existing gateway/observation contracts |
| 12 | no second Chat source of truth | No new tables; timeline derived from the projection |
| 13 | Buzz outage does not break Files/other Applications | Existing suites continue to pass with no chat configuration |

## 9. Testing plan

- Backend: endpoint-level tests for status/channels/messages (bound, unbound,
  unadmitted, unmapped, cross-tenant, tombstones, pagination), reusing the
  existing DB-backed `#[ignore]` harness and fake-relay contract tests for
  buzz-mode decisions.
- Frontend: vitest suites for `chat.ts`, `nostr.ts` (signing vectors), `keys.ts`
  (round-trip, wrong passphrase), the chat components (timeline folding,
  composer states, attachment tag inclusion, citation focus), and the
  websocket invalidation handler.
- Real relay: `scripts/run-chat-e2e.sh` automates the signed publish probe
  against a disposable relay (ADR-0034 recipe); the relay-side revocation
  check (kind 9031) is a documented manual step using the relay's admin CLI.

## 10. Limitations (documented, intentional)

- Channel list contains channels with observed events only (no Buzz channel
  registry API exists in the current contract).
- Reference-first messages (no `content_indexing`) render without body text.
- `buzz`-mode per-message relay round-trips make large timelines slower; a
  batch endpoint is deferred upstream (ADR-0035).
- Composer replies/thread writes are deferred until Buzz's thread tag wire
  format is confirmed; v1 renders `thread_root_id` grouping read-side.
- Attachments are sender-side only in v1: the observation index does not
  retain event tags, so recipients see the message body but not an attachment
  link; surfacing `elembra-ref` tags for recipients requires a future
  projection change.
- On publish the client does not tag a target channel: channel attribution is
  determined by the Buzz bridge under the current contract, and a client
  channel-tag wire format is deferred upstream until confirmed (same status as
  thread tags above).
- `dm`/`private`/`excluded` channels are unreadable under the local gate by
  design until the upstream `access/check` capability ships.
- Browser-held keys mean device loss without an export is unrecoverable; the UI
  states this (ADR-0034: no silent server custody).

## 11. Implementation slices

- S1: backend read surface (status/channels/messages endpoints + store
  queries + broadcast on ingest) and their authz tests.
- S2: frontend chat application (renderer, channel list, timeline, composer,
  live updates, key custody, NIP-42 publish client, states).
- S3: attachments picker + Ask Channel wiring (askHref, onChatCitationOpen,
  focus/scroll).
- S4: security/E2E matrix completion, real-relay script, docs/CHANGELOG, final
  review against the boundary checklist.

## 12. Out of scope

Agents, Mail, new Memory/Search/RAG architecture, Buzz rewrite, Slack parity,
administration UI, calls/video, broad UI redesign, and any server-side signing
of human messages.
