# Elembra Chat Application v1 — Implementation Notes

Status: Implemented (slice commits on `feat/elembra-chat-app-v1`)  
Date: 2026-08-12  
Related: ADR-0034 (Elembra Chat and Buzz Boundary), ADR-0035 (Buzz Source Authorization
Gateway), `docs/superpowers/specs/2026-08-12-elembra-chat-app-v1-design.md` (design spec),
`backend/tests/chat_app_read_test.rs` (read-surface acceptance suite).

This document records how Chat Application v1 was implemented: the endpoints, the
per-message authorization chain, the broadcast event, the browser-held key custody
model, the documented limitations, and how to run the real-relay proof harness. Buzz
remains the chat source of truth; Elembra serves only its authorized observation
projection and never reads Buzz's private database.

## 1. Endpoints

Read surface (frontend client: `frontend/src/lib/api/chat.ts`):

- `GET /applications/chat/status` — `ChatStatusResponse { chat_enabled, mapping, binding, admission_active }`.
  `mapping` carries `{ community_id, relay_url }`; `binding` carries `{ status, buzz_pubkey }`.
- `GET /applications/chat/channels` — `ChannelInfo[]` (`{ channel_id, channel_kind, latest_event_at }`),
  channels with observed events only.
- `GET /applications/chat/messages?channel_id=<id>&before=<ts>&limit=<n>` —
  `MessagesResponse { messages: ChatMessageDto[], next_before }`, newest-first timeline.
- `GET /applications/chat/messages/{message_id}` — single `ChatMessageDto` (citation/deep-link focus).

Binding and attachment support:

- `POST /applications/chat/identity-binding/challenge` / `.../verify` — NIP-42 auth-event
  challenge against the mapped relay; `POST /applications/chat/admission` — relay admission.
- `POST /applications/chat/attachments/prepare` / `.../preview` / `.../open` — Files attachment
  reference lifecycle (`elembra-ref` tags). The observation index retains each
  event's identifier-only refs (migration `20260810000007`), and the timeline
  DTO surfaces them as `attachments` so recipients see an affordance
  (issue #242).
- `POST /admin/applications/chat/workspaces/{workspace_id}/community` — admin mapping.

## 2. Attachment retention, edit/tombstone semantics, backfill

- **Identifiers only.** Ingest extracts the verified event's `elembra-ref`
  tags into canonical `ResourceRef`s (application/type/id, optional version)
  and stores them in `chat_observed_events.attachment_refs` (JSONB, `[]`
  default). No blob duplication, no signed URL/token/grant, no tenant hint —
  there is no attachment registry. Malformed refs are dropped per-ref with a
  warning; the event is still accepted (fail closed per ref, not per event).
- **Determinism.** Refs are stored in event tag order with duplicates
  collapsed to the first occurrence (`extract_attachment_refs`,
  `backend/server/src/buzz_observation.rs`).
- **Edit replaces.** Each observation row carries its own refs; the timeline
  fold surfaces the latest event per message, so an `edited` event's refs
  replace the previous ones wholesale — an edit without `elembra-ref` tags
  clears the message's attachments.
- **Tombstone hides.** The fold picks the tombstone row and the read surface
  drops inactive/deleted messages, so a deleted message never exposes
  attachments (list and single-message endpoints).
- **Opening reauthorizes.** The recipient's click posts the identifier back to
  `POST /applications/chat/attachments/open`, which reauthorizes through the
  Files owner at read time. Missing, foreign-tenant, or denied files answer an
  existence-hiding 404 — the timeline itself never resolves refs.
- **Old observations (backfill policy).** Pre-migration observation rows keep
  `attachment_refs = []` permanently. There is deliberately NO synthetic
  backfill job, and re-running Buzz reconciliation cannot help either: an
  already-observed event id is a duplicate no-op, so reconcile never rewrites
  a row's refs. The only paths to attachments for an old message are NEW
  events from the author — an edit carrying the `elembra-ref` tags — or
  delete-and-re-push the message. This keeps the index strictly
  event-derived — Buzz stays authoritative.

## 3. Authorization chain

Every handler first narrows to the caller's own tenant state (chat enabled, community
mapped, identity bound, admission active), then authorizes **each message** through the
Buzz authority — Elembra never applies a local ACL of its own:

1. **Pre-filters** — status/channels/messages check the workspace's chat enablement and
   community mapping, and the caller's binding + admission state, before touching data.
2. **BuzzAuthority** — `ChatResourceOwner::can_read_channel(ctx, community_id, channel_id, kind)`
   (`backend/crates/resource-auth`) asks the mapped relay's authority endpoint whether the
   bound principal may read that channel _now_ (ADR-0035 gateway), so revoked members stop
   reading as soon as the relay revokes them. `dm`/`private`/`excluded` channels fail closed
   under the local gate until the upstream `access/check` capability ships.

## 4. Broadcast event

Ingest of an observed event publishes a tenant-scoped `ChatMessageObserved` durable event
(`rustshare_core::events::EventType::ChatMessageObserved`); the sync handler relays it over
the workspace websocket. The frontend manager (`frontend/src/lib/websocket/manager.ts`)
invalidates the `['chat-messages']` and `['chat-channels']` query prefixes on receipt, and
the chat view additionally polls every 15 s as a fallback.

## 5. Frontend key custody model

The signing key lives only in the browser (`frontend/src/lib/chat/keys.ts`); the backend
never sees it. The key is generated with `crypto.getRandomValues`, encrypted at rest with
WebCrypto PBKDF2 (600k iterations, SHA-256; legacy envelopes record their own
iterations and still decrypt) + AES-GCM under a user passphrase, and stored in
`localStorage` (scoped per user, migrated from `elembra.chat.key.v1`) together
with its x-only pubkey. Export/import is
the only recovery path (encrypted-envelope or plaintext-key backup formats); there is no
server custody (ADR-0034). Signing is BIP-340 Schnorr (`@noble/curves`); publishing goes
client-direct over a NIP-42 relay session (`frontend/src/lib/chat/nostr.ts`), with an AUTH
challenge exchange and a 10 s timeout. The composer refuses to publish when the unlocked
key's pubkey differs from the bound `buzz_pubkey`.

## 6. Limitations (from design spec §10, documented, intentional)

- Channel list contains channels with observed events only (no Buzz channel
  registry API exists in the current contract).
- Reference-first messages (no `content_indexing`) render without body text.
- `buzz`-mode per-message relay round-trips make large timelines slower; a
  batch endpoint is deferred upstream (ADR-0035).
- Composer replies/thread writes are deferred until Buzz's thread tag wire
  format is confirmed; v1 renders `thread_root_id` grouping read-side.
- Attachment affordances carry no display name: the timeline surfaces
  identifier-only refs and the client opens without previewing, so no
  existence leak; showing file names is a future polish.
- `dm`/`private`/`excluded` channels are unreadable under the local gate by
  design until the upstream `access/check` capability ships.
- Browser-held keys mean device loss without an export is unrecoverable; the UI
  states this (ADR-0034: no silent server custody).

## 7. Running the proof harness

- **Relay publish probe** (`frontend/scripts/chat-relay-probe.mjs`): signs a kind-1 event with
  a given secret key and publishes it through a NIP-42 AUTH session against any relay. Exit 0
  on acceptance, 1 on rejection/unreachable/invalid key. Smoke test:

  ```bash
  cd frontend && node scripts/chat-relay-probe.mjs ws://127.0.0.1:9 <secret-key-hex> test
  # expect: exit 1, "FAILED: relay rejected or unreachable" (no crash)
  ```

- **Disposable-relay orchestration** (`scripts/run-chat-e2e.sh`): starts a disposable
  membership-enforcing relay image (`BUZZ_RELAY_IMAGE`), runs the publish probe with the
  service key (`BUZZ_SERVICE_SK`), and documents the relay-side revocation denial step and the
  Elembra-side read gate. See the header comments for the required environment variables.

- **Read-surface acceptance**: `backend/tests/chat_app_read_test.rs` covers the authorization
  matrix including `revoked_binding_denies_reads`; run with `DATABASE_URL` set.
