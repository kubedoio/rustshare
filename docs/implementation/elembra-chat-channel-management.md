# Elembra Chat Channel Management — Implementation Notes

Status: Implemented (stacked on `feat/chat-client-capabilities`, PR #479)
Date: 2026-11-08
Related: `docs/implementation/elembra-chat-app-v1.md` (read surface), ADR-0046
(managed NIP-29 publishing authorization), the pinned Buzz fork's `NOSTR.md`
(group discovery, moderation kinds, role model).

This document records how the Chat channel-management slice is implemented:
which fork operations are used, how every mutation is confirmed against
authoritative relay state, how roles gate the UI, and which relay capabilities
are deliberately not offered. All signing stays client-side with the
browser-held Chat identity (`frontend/src/lib/chat/nostr.ts`); the backend
never signs channel events.

## 1. Relay operations used

All wire semantics verified against the pinned fork (`c9a373b`): `NOSTR.md`
"NIP-29 Direct / What Works", "Group Discovery", "Canonical Publish Tags",
and `crates/buzz-core/src/channel.rs` (role hierarchy),
`crates/buzz-relay/src/handlers/side_effects.rs` (validators and emitted
state), `crates/buzz-db/src/channel.rs` (`add_member`/`remove_member`).

| Operation      | Kind | Tags sent                                                                              | Authorization (fork)                                                                                                                                                                 |
| -------------- | ---- | -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Create channel | 9007 | `h` (client UUID), `name`, `visibility`, `channel_type=stream`, `about` (if non-empty) | Any authenticated identity; creator becomes owner. A client-supplied UUID `h` lets the relay pre-create the channel with that id, so the client knows the channel id before publish. |
| Edit metadata  | 9002 | `h`, `name`/`about`                                                                    | `name`/`about` are privileged tags: owner/admin (`validate_admin_event`, kind 9002 case).                                                                                            |
| Add member     | 9000 | `h`, `p` (no `role` tag)                                                               | Open: any member (elevated roles need owner/admin granter). Private: owner/admin. No role tag means "no role change": new members default to `member`, existing roles are preserved. |
| Remove member  | 9001 | `h`, `p`                                                                               | Self-removal always (last-owner guard); others: owner/admin.                                                                                                                         |
| Join           | 9021 | `h`                                                                                    | Open channels only; private joins are rejected at ingest.                                                                                                                            |
| Leave          | 9022 | `h`                                                                                    | Any member; last-owner guard.                                                                                                                                                        |

Reads reuse `queryRelayEvents`, which already enforces the trusted relay
signer for kinds 39000–39003 and NIP-42 subject authentication:

- Directory: one historical `REQ {kinds:[39000]}` — the fork's group-discovery
  query. It returns every channel the authenticated identity may see,
  **including channels with zero messages** (directory membership is
  metadata-driven, not history-driven). Hidden DM channels and archived
  channels are filtered client-side for the sidebar. The relay-signed
  metadata always carries the channel type as the `t` tag
  (`["t", channel.channel_type]`, emitted unconditionally by
  `side_effects.rs::emit_group_discovery_events` at c9a373b with the value
  stored at the 9007 ingest, defaulting to `stream`); NOSTR.md's 39000 tag
  table omits it, but the fork source is authoritative. The client reads the
  type from `t` and sends it on 9007 as the `channel_type` tag.
- Roster/confirmation: `REQ {kinds:[39002], "#d":[<uuid>]}` — the newest
  relay-signed snapshot carries `["p", pubkey, "", role]` for every member;
  the role is read from the fourth field.

## 2. Confirm-via-relay-state pattern

A publish `OK` is never treated as authoritative. Every operation in
`frontend/src/lib/chat/channels.ts` follows:

1. Build and sign with the in-memory Chat key (`publishEvent`).
2. On `ok`, re-query the relay-signed snapshot and require the expected
   state: created channel's 39000 matches the submitted name/description;
   fresh 39002 contains (or omits) the target member. A missing match is
   reported as _unconfirmed_, not success.
3. On `uncertain` (ACK lost), the exact signed event and its confirmation
   closure are kept; the UI offers "Retry same request", which replays the
   identical bytes through `publishSignedEvent`. The relay deduplicates by
   event id, so the retry can confirm the original outcome and can never
   apply the operation twice. Nothing is ever re-signed.

## 3. Role gating in the UI

The relay reports roles; the UI only decides what to offer
(`channels.ts` gating helpers, enforced again at the relay):

- Edit metadata: owner/admin only.
- Add member: owner/admin in private channels; any full member in open
  channels. Guests are read-only and never see the form.
- Remove member (others): owner/admin only. Self-removal is the leave flow.
- Join: open, non-archived channels where you are not already a member.
- Leave: members except the last owner (the fork rejects last-owner leave;
  the UI explains instead of attempting it).

Guest rendering: a `guest` role appearing in a relay-signed roster is
displayed as-is, but **no UI assigns the guest role** — the guest
write-denial fix is still in review upstream (kubedoio/buzz#8), and the
fork only emits 39001/39002 (kind 39003 group roles is not emitted).
Elevated grants (owner/admin via the `role` tag) are likewise not offered
in this slice; the add-member flow sends no role tag.

## 4. Security notes

- Private-channel non-disclosure is structural: 39000/39002 events are
  stored channel-scoped and the relay applies access control before
  returning them. A private channel the caller may not see never reaches
  the client, so it cannot leak through the directory, the roster, or
  search. The Chat UI performs no NIP-50 search today, so there is no
  search surface to audit; any future chat search must scope to the
  selected channel's `#h` like the fork's documented search usage.
- Metadata events are only parsed after `queryRelayEvents` has verified
  them against `status.mapping.relay_pubkey` (the trusted relay signer);
  the parsers in `channels.ts` never accept client-supplied snapshots.
- Posting eligibility stays exactly as #479 defined it
  (`capabilities.ts` default-deny, `canPostToChannel` on relay-confirmed
  state). The one refinement: in `fork_client_held` mode the backend's
  `member` observation flag no longer gates posting for relay-only
  (empty) channels — relay-confirmed membership is authoritative there.
- Capability gating is unchanged: the directory, create, and management
  UI render only when `getChatCapabilities(profile)` reports the fork
  protocol available (`fork_client_held`).

## 5. Selection, deep links, reload

- Selection is URL-addressable: `?channel=<id>` is read on navigation and
  selection updates the URL via `history.replaceState` (no history spam).
- Reload reconstructs from the relay: the directory query has no local
  cache beyond the in-memory TanStack cache for the page lifetime, and
  selection is only validated once the directory outcome is known, so a
  deep link to an empty channel survives the directory round-trip.
- The observation-derived channel list (backend) is merged with the relay
  directory; observation rows win for shared ids (they carry
  `latest_event_at`), relay rows contribute channels with no observed
  messages yet.

## 6. Known gaps (relay limits, not faked)

- Live directory push does not exist in the fork (channel-scoped storage
  means live global 39000 subscriptions receive nothing); the directory
  is refreshed by historical REQ on mount and on the 15 s polling
  fallback, matching the fork's documented discovery flow.
- Kind 39003 (group roles) is registered but not emitted by the fork;
  roles come from 39002's fourth `p`-tag field.
- Guest assignment and elevated role grants are intentionally not offered
  (see §3).
