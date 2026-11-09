# Elembra Chat Conversations — Implementation Notes

Status: Implemented (stacked on `feat/chat-channel-management`, PR #480)
Date: 2026-11-09
Related: `docs/implementation/elembra-chat-app-v1.md` (read surface),
`docs/implementation/elembra-chat-channel-management.md` (channel slice),
ADR-0046 (managed NIP-29 publishing authorization), the pinned Buzz fork's
`NOSTR.md` (reactions/deletion/threads rows + "Canonical Publish Tags").

This document records how the conversation slice is implemented against the
pinned Buzz fork (`c9a373b`, image
`ghcr.io/kubedoio/buzz-elembra@sha256:2b6c4b2a…`): reply threading with
NIP-10 tags, kind-7 reactions, authorized message deletion (kind 5 / 9005),
per-channel and per-thread drafts, and membership-driven conversation state.
All signing stays client-side with the browser-held Chat identity
(`frontend/src/lib/chat/nostr.ts`); the backend never signs conversation
events. Every wire behavior below was verified live against the pinned image
on 2026-11-09 (23 protocol checks, see §6).

## 1. Relay operations used

| Operation        | Kind          | Tags sent                              | Authorization (fork, verified live)                                                                                                                          |
| ---------------- | ------------- | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Reply            | 9             | `h`, `["e",root,"","root"]`, `["e",parent,"","reply"]` | Same as any kind-9 post (channel membership rules). The relay validates thread ancestry (`resolve_nip10_thread_meta`): parent must exist in the same channel; the claimed root must match the parent's stored root (or the parent's own declared root, or the parent itself). |
| React            | 7 (NIP-25)    | `h` (recommended), `e` (target, last)  | Relay derives the channel from the target's stored channel; unknown/deleted targets rejected. Dedupes one reaction per (target, actor, emoji) — a duplicate is rejected `duplicate: reaction already exists`. |
| Remove reaction  | 5 (NIP-09)    | `h`, `e` (the kind-7 event id)         | Self-authored targets only. The relay soft-deletes the reaction event and removes its reaction row. Deletion events must reference exactly one target.      |
| Delete (self)    | 5 (NIP-09)    | `h`, `e` (target)                      | Target must be self-authored (`must be event author`). Soft-deletes the event; thread counters decrement atomically; reactions to the target are unaffected. |
| Delete (admin)   | 9005 (NIP-29) | `h`, `e` (target)                      | Owner/admin of the `h` channel; the target must belong to that channel. Additionally emits a relay-signed kind-40099 in-channel system message `{"type":"message_deleted","actor":…,"target_event_id":…}`. |
| Thread history   | read          | `REQ {kinds:[9], "#e":[rootId]}`       | Channel access control applies before returning; deleted events are filtered by the query path (`deleted_at IS NULL`).                                      |
| Conversation overlay | read      | `REQ {kinds:[5,7,9005,40099], "#h":[channelId]}` | Same access control; private-channel non-disclosure is structural, so losing membership fails closed exactly like the roster query.                    |

Thread tag semantics that shape the client (verified live):

- The relay reads only the event id (position 1) and marker (position 3);
  the relay URL position is optional and omitted.
- Accepted replies effectively always carry a root tag; a bare `reply` tag
  is only valid for parents with no thread ancestry — on a parent that
  already has thread metadata the relay rejects it
  (`root tag does not match thread ancestry`). The client therefore always
  derives the root from the parent's own tags (`parent.rootId ?? parent`)
  and sends the canonical root+reply pair.
- Depth is capped at 100 by the relay; replies beyond that are rejected.

## 2. Confirm-via-relay-state pattern

Identical contract to the channel slice: a publish `OK` is never
authoritative. `frontend/src/lib/chat/conversations.ts` builds and signs in
the browser, then:

1. On `ok`, re-queries relay state and requires the expected outcome: the
   new event id present in the thread's `#e` history (reply) or the target's
   `#e` reaction set (reaction), the reaction event id absent from that set
   (reaction removal), or the target absent from an `ids` probe (deletion —
   the relay's query path filters soft-deleted rows, verified live).
2. On `uncertain`, the exact signed event and its confirmation closure are
   kept; the UI offers "Retry same request" which replays the identical
   bytes via `publishSignedEvent` (relay dedupes by event id — no double
   application, nothing is ever re-signed).

## 3. Where conversation state lives

The backend observation path only accepts chat-message kinds (1/9/40002 —
see `buzz_observation.rs::is_chat_kind`), so reactions, deletion requests,
and tombstones never reach the Elembra projection. The frontend therefore
reads a per-channel **conversation overlay** from the relay
(kinds 5/7/9005/40099 filtered by `#h`), parsed by `parseReactions` and
`parseDeletionTargets`:

- kind 5/9005 events are user-signed but were authorization-checked at the
  relay's ingest (self-author for kind 5, owner/admin for 9005), so their
  single `e` target is applied as deleted;
- kind 40099 tombstones are accepted **only** from the trusted relay
  signer — `queryRelayEvents` only enforces that signer for kinds
  39000–39003, so `parseDeletionTargets` re-checks `pubkey === relayPubkey`
  and ignores malformed payloads. A user-signed lookalike must never drive
  UI state.

The observer (`frontend/scripts/buzz-observer.mjs`) now projects
`thread_root_id` from the event's own NIP-10 tags (root marker, or the
reply marker for bare replies — relay-accepted events always resolve their
thread root locally, see §1). That drives the timeline reply badges and the
`thread_root_id` field of the messages API; before this slice the observer
hardcoded `null`.

Deleted messages: the backend's deletion reconciliation
(`buzz_observation.rs`, `ObservedEventType::Deleted` — predates this slice)
marks projected rows inactive once the relay's deletion signal lands, so
the messages API drops the row. The UI renders the relay-overlay tombstone
("Message deleted.") for the propagation window (and for any row still
present in the accumulated page), then the row disappears with the next
page refresh. Acceptance asserts both renderings.

## 4. UI affordances and gating

- Reply: per-message hover action; the composer shows a "Replying in
  thread" banner and attaches canonical NIP-10 tags. Inside a thread view
  the composer defaults to the thread root; an explicit reply target can be
  cancelled back to the root, and cancelling the root closes the thread.
- Thread view: replaces the timeline when opened (relay history grouped by
  root, root first, then oldest-first). Renders relay events directly —
  author pubkeys only, no Elembra display names (relay history has none).
- Reactions: per-message chips (count, author shortlist on hover,
  `aria-pressed` for the viewer's own) plus a small palette for new emoji;
  click toggles the viewer's reaction (kind 7, or kind 5 targeting the
  viewer's own kind-7 event).
- Deletion: menu entry shown only with a relay-derived mode — own messages
  (kind 5) or owner/admin (kind 9005), from `canDeleteMessage` on the
  relay-confirmed channel state exposed by `ChannelPermissions`
  (`onStateChange`). Confirmed before sending.
- All affordances gate on the existing capability matrix
  (`threads`/`reactions`/`deleteMessage` are fork-protocol capabilities,
  exactly like the channel slice) and on `postingAllowed`.

## 5. Drafts and membership changes

Draft keys extend the existing per-account scoping with the thread
dimension: `draftStorageKey(relayUrl, boundPubkey, channelId, threadRootId)`
(`conversations.ts`). Legacy three-element keys are read as channel-level
drafts, so pre-upgrade drafts survive. Pending (uncertain) events are only
restored when their thread tags match the current scope.

Losing posting eligibility (removal from a private channel, role change,
offline) clears the open thread, the reply target, and stale operation
status, matching the composer's default-deny; the relay-backed overlay and
thread queries fail closed on their own because the relay applies channel
access control before returning anything.

## 6. Verification evidence

- **Live protocol probes (2026-11-09, pinned image 2b6c4b2a):** 23/23 checks
  — canonical/nested/bare-reply tag acceptance and rejections, `#e` thread
  queries, duplicate-reaction rejection, `#h` reaction queries, NIP-09
  reaction removal, kind-5/9005 authorization denials, self/admin deletion
  application, kind-5 events queryable in-channel, 9005's relay-signed
  40099 tombstone, fail-closed reaction to deleted targets, `ids` probes on
  live/deleted events.
- **Unit tests:** `frontend/src/lib/chat/conversations.test.ts` (16 tests:
  tag construction, marker parsing, thread grouping/sorting, reaction
  aggregation, deletion overlay trust boundary, delete gating, draft keys);
  MessageComposer (19 tests incl. reply tags, per-thread drafts, legacy
  draft migration); MessageRow (6 tests incl. deleted state, reaction
  chips, gating).
- **Product acceptance:**
  `frontend/tests/chat-conversation-acceptance.e2e.ts` drives the bundled
  stack through real browsers; relay-direct assertions decrypt each user's
  browser-held key backup with the known acceptance passphrase (same
  PBKDF2/AES-GCM envelope as `keys.ts`) and query the relay over
  NIP-42-authenticated WebSocket to assert exact signed event ids, authors,
  and NIP-10 tags.

## 7. Known gaps (relay limits, not faked)

- Message edits remain unavailable (`capabilities.ts` — "not compatible
  with standard Buzz clients"); the relay does register kind 902/40003
  edit kinds, but no edit UI is offered in this slice.
- Reaction palette is a fixed emoji set; custom emoji (`:shortcode:` with
  `emoji` tags) are relay-supported but not offered.
- The observer kind whitelist still excludes reactions and deletion request
  kinds; deletion *state* reaches the projection through the backend's
  separate deletion reconciliation (rows go inactive), and per-message
  deleted rendering between relay deletion and projection refresh comes
  from the relay overlay. Thread reply badges count the projection, so a
  deleted reply disappears from the badge with the next page refresh.
- Kind 39005 thread summaries are relay-emitted on reply/delete but not
  consumed; counts are derived from the overlay/timeline instead.
