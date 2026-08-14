# Specification: Buzz Upstream Source Authorization & State v1alpha1

Status: Draft — v1alpha1; amended 2026-08-14 (batch checks, channel registry, canonical publish tags)  
Date: 2026-08-11 (amended 2026-08-14)  
Related: `resource-ref-authorization-v1alpha1.md`, Elembra Chat identity/admission contracts

## Purpose

Buzz is the authoritative external chat system behind Elembra's Chat
application. Elembra records a reference/provenance projection of Buzz events
and must ask an authoritative source whether a mapped principal may read a
given channel/message *now*. This spec defines an HTTP capability a Buzz relay
can implement so that a trusted server workload — which **never holds a human
user's signing key** — can:

1. ask the relay whether a given pubkey may currently read a given
   channel/message, individually or in batch,
2. enumerate the channels a given pubkey may currently read (the
   authoritative channel registry), and
3. page the community's signed event state for reconciliation,

while:

- **reusing the relay's own authorization logic** (channel visibility,
  membership, message availability) — Elembra creates no parallel
  authorization code path;
- **preserving host-derived community isolation** — each community's relay
  host answers only checks for its own community;
- **exposing no private DB internals** — the relay's database schema and
  internal state are never visible over this API;
- **creating no second ACL system** — the relay's existing channel/membership
  semantics remain the single source of truth.

## Scope

This document **proposes** the Buzz-side implementation. Buzz is a separate
external repository; the capability is described here as a contract so that:

- this repository implements the **Elembra-side client, contract types, and
  tests** against a contract-faithful test double, and
- the Buzz repository can implement the relay endpoints independently, guided
  by this spec.

The proposal is intentionally generic — the capability is useful
independently of Elembra, reuses the relay's existing authorization logic,
exposes no private database internals, and adds no second ACL system. Any
trusted server workload with a provisioned service key can use it.

## Trust model

- The **service workload** (Elembra) is a trusted server process. It holds a
  Nostr service key (`RUSTSHARE_CHAT_BRIDGE_SECRET_KEY`) and never holds a
  human user's signing key.
- The **relay** is the community's authority. It authenticates the workload
  via NIP-98 and answers access checks and event paging. The workload pins the
  relay's public key per community mapping (`relay_pubkey`) and treats any
  response not signed by that exact key as a denial.

## Authentication (NIP-98)

Every request carries an HTTP `Authorization` header:

```text
Authorization: Nostr <base64(kind-27235 event)>
```

where `<base64(kind-27235 event)>` is the STANDARD base64 encoding of the JSON of
a NIP-98 event of kind 27235 (matching the `nostr` crate's `HttpData::to_authorization`
and `verify_auth_header`, which both use `general_purpose::STANDARD`):

- `kind`: `27235`
- `pubkey`: the workload's service public key
- `tags`:
  - `u` tag — the **exact request URL** (scheme, host, port, path, query)
  - `method` tag — the HTTP method (e.g. `POST`, `GET`)
  - for `POST` requests, a `payload` tag whose value is the hex-encoded
    SHA-256 of the request body
- `content`: `""`
- signed by the workload's service key

The relay must be configured with the workload service public key as trusted.
Missing, invalid, stale, or untrusted signatures → `401`. The relay MUST NOT
accept requests that carry a payload tag mismatching the actual body.

## Response envelope

Every response body is a **raw signed Nostr event** (JSON):

```json
{
  "id": "<64hex>",
  "pubkey": "<64hex>",
  "created_at": 1754000000,
  "kind": 19030,
  "tags": [],
  "content": "<json string>",
  "sig": "<128hex>"
}
```

- `kind` is `19030` — a NIP-01-structured event using an **unregistered
  private kind in the replaceable range (10000–19999)**. It is never published
  to subscribers and is only ever returned inline as an HTTP response; the
  kind was chosen to avoid collision with registered NIP kinds.
- The event is signed by the **relay's key**.
- `content` is a JSON string (the endpoint-specific payload below).

The client MUST verify all of the following before trusting a response;
**any failure is treated as `Deny` (fail closed)**:

1. `kind == 19030`;
2. `Event::verify()` passes (id and Schnorr signature);
3. `pubkey` equals the **pinned relay public key** configured for the
   community mapping (`relay_pubkey`); a mapping without a pinned key cannot
   use this capability;
4. the response content **echoes the request values verbatim** — its `pubkey`,
   `channel_id`, and `message_id` must equal the request's (`message_id: null`
   when the request had none) and its `decision`, `reason`, and `evaluated_at`
   fields must be present and well-typed; any mismatch is an
   `InvalidResponse`;
5. `evaluated_at` (unix seconds) is within **60 seconds** of the client's
   clock when received; a response older than that is stale and is denied.

## Endpoints

### `POST /api/v1/relay/access/check`

Ask whether a pubkey may currently read a channel/message.

Request body:

```json
{
  "pubkey": "<64hex>",
  "channel_id": "<str>",
  "channel_kind": "workspace|dm|private|excluded",
  "message_id": "<64hex>|null",
  "event_created_at": "<unix secs>|null"
}
```

- `event_created_at` is an **optional informational** field — the unix seconds
  of the event being checked, when the workload has one. The relay MAY ignore
  it; the client MAY send it. The Rust contract keeps the field so the client
  can include it without any validation change.

Response `content`:

```json
{
  "decision": "allow|deny|not_found",
  "reason": "<str>",
  "evaluated_at": "<unix secs>",
  "pubkey": "<64hex>",
  "channel_id": "<str>",
  "message_id": "<64hex>|null"
}
```

Semantics:

- The decision derives from the relay's **current** channel visibility /
  membership and message availability.
- The response **echoes the request's `pubkey`, `channel_id`, and
  `message_id` verbatim** (`message_id: null` when the request had none),
  binding the decision to the exact check that was asked for; the client
  rejects any response whose echoed values do not match the request.
- `message_id: null` means a **channel-level read check** — the relay
  evaluates channel visibility/membership only, with no message-level
  availability evaluation. Elembra always sends a message id in this
  integration, so in practice the check is always message-level.
- `allow` only when the pubkey is a **current member/participant** of the
  channel **and** the message is available (exists and is not deleted).
- Unknown channels, and deleted or unknown messages, → `not_found`.
- Non-member, removed, or otherwise excluded principals → `deny`.
- **Existence hiding:** the relay must never return membership lists, and
  `reason` strings must never reveal whether other users exist or their
  membership/activity (e.g. prefer `"not a member"` over enumerating who is).

### `POST /api/v1/relay/access/check-batch`

Ask whether a pubkey may currently read multiple channels/messages in a
single round-trip.

Request body:

```json
{
  "checks": [
    {
      "pubkey": "<64hex>",
      "channel_id": "<str>",
      "channel_kind": "workspace|dm|private|excluded",
      "message_id": "<64hex>|null",
      "event_created_at": "<unix secs>|null"
    }
  ]
}
```

- Each item is exactly a single `access/check` request body, with the same
  semantics for its optional `event_created_at`.
- The request must contain **at most 64 checks**; more than 64 → `400`.

Response `content`:

```json
{
  "results": [
    {
      "decision": "allow|deny|not_found",
      "reason": "<str>",
      "evaluated_at": "<unix secs>",
      "pubkey": "<64hex>",
      "channel_id": "<str>",
      "message_id": "<64hex>|null"
    }
  ],
  "evaluated_at": "<unix secs>"
}
```

Semantics:

- The response is a **single kind-19030 event**; `results` is
  **order-preserving** — `results[i]` corresponds to `checks[i]`.
- **Envelope `evaluated_at` is the freshness authority for the whole
  response** — the relay's evaluation time for the batch. If it is missing,
  malformed, or older than 60 seconds relative to the client clock, every
  item fails closed (`Deny`).
- Each result item keeps the single check's shape: its `pubkey`,
  `channel_id`, and `message_id` echo the request item verbatim
  (`message_id: null` when the item had none), and its `decision`, `reason`,
  and `evaluated_at` fields must be present and well-typed. The item-level
  `evaluated_at` mirrors the envelope value (informational echo — shape
  parity with the single check); item-level freshness is not an independent
  failure class in batch mode.
- **Per-item failure isolation:** one bad item does not fail the others.
  Item-level outcomes are the single check's: unknown channel, deleted or
  unknown message → `not_found`; non-member, removed, or otherwise excluded
  principal → `deny`. A genuine evaluation failure for one item (e.g. a
  relay-side error evaluating it) yields `deny` for that item only, and an
  item whose echoed values do not match its request item is treated as an
  `InvalidResponse` for that item only. The remaining items are evaluated
  and verified normally.
- **Envelope-level verification failure fails all:** if the response itself
  fails verification (wrong kind, invalid signature, wrong pinned pubkey,
  missing/malformed/stale envelope `evaluated_at`, unparseable content),
  every item is `Deny` — fail closed.

### `GET /api/v1/relay/channels?pubkey=<64hex>`

List the channels a given pubkey may currently read — the **authoritative
channel registry**. Channel discovery in Buzz production mode must come from
this endpoint; observation-derived channel discovery is deprecated in Buzz
production mode.

- NIP-98 GET — the `u` tag covers the exact request URL **including the query
  string**, so the `pubkey` parameter is bound by the signature (there is no
  `payload` tag for GET requests).
- `pubkey` — the 64-hex pubkey to list channels for.

Response `content`:

```json
{
  "channels": [
    {
      "channel_id": "<str>",
      "name": "<str>",
      "channel_type": "stream|forum|dm|workflow",
      "visibility": "open|private",
      "member": true
    }
  ],
  "evaluated_at": "<unix secs>",
  "pubkey": "<64hex>"
}
```

Semantics:

- The registry returns **only channels the given pubkey may read**: channels
  the pubkey is a member of (including private ones) plus open channels.
  `member` states whether the pubkey is a member of that channel — an open
  channel the pubkey may read without being a member has `member: false`.
- `channel_type` uses the upstream Buzz channel types
  `stream|forum|dm|workflow` and `visibility` is `open|private` (the relay's
  native vocabulary; Elembra's `workspace|dm|private|excluded` remains a
  client-side projection concern).
- The listing is **host-derived community only** — the relay host answers
  only for its own community; another community's channels are never
  included, and the client never routes community A's registry requests to
  community B's relay host.
- The response follows the same envelope rules as every other endpoint:
  kind-19030 signed by the relay's key, `pubkey` echoes the query value
  verbatim, `evaluated_at` within the 60-second freshness window.
- **Existence hiding:** the registry never returns channels the pubkey may
  not read and never returns another community's channels.

### `GET /api/v1/relay/state/events?since=<unix>&limit=<n>&cursor=<opaque>`

Page the community's signed event state for reconciliation.

- `since` — optional; only events whose **own `created_at`** (the unix
  seconds of the kind-1 event, not the entry's observation time) is at or
  after this timestamp.
- `limit` — optional maximum page size.
- `cursor` — opaque continuation token returned by a previous page.

Response `content`:

```json
{
  "entries": [
    {
      "event": "<raw signed kind-1 event JSON>",
      "context": {
        "community_id": "<str>",
        "channel_id": "<str>",
        "channel_kind": "workspace|dm|private|excluded",
        "thread_root_id": "<str>|null",
        "message_id": "<str>",
        "event_type": "created|edited|deleted",
        "supersedes_event_id": "<str>|null"
      }
    }
  ],
  "cursor": "<opaque>|null",
  "complete": true
}
```

Rules:

- Each `context` is exactly the shape of Elembra's webhook push payload
  (`BuzzPushContext`), so Elembra reuses its existing validation unchanged.
- Entries are limited to events the authenticated service workload is
  **entitled to see for the relay's community** — the same visibility the
  relay applies to its own reconciliation consumers; no event of another
  community or outside the workload's entitlement is ever included.
- `complete: true` terminates the stream (the final page).
- `cursor: null` with `complete: false` is malformed; the client must treat it
  as an invalid response (deny/fail closed).

## Canonical publish tags (kind-1 wire format)

The canonical wire format for channel scoping and thread identity on
published kind-1 events, confirmed against the Buzz relay's ingest
implementation. **This is the canonical thread root/reply contract that
Elembra issue #243 was waiting on; it is resolved upstream.**

- **Channel scoping:** `["h", "<channel-uuid>"]` — the NIP-29 group tag
  carrying the channel's UUID. An event without an `h` tag is not
  channel-scoped.
- **Thread root/reply identity:** `["e", "<64-hex-id>", "<relay-url?>",
  "root"|"reply"]` — NIP-10; the relay URL element is optional (the relay
  reads only the id and the marker).
- **Server-validated ancestry** (enforced by the relay at ingest):
  - the referenced parent must exist;
  - the parent must belong to the same channel as the new event;
  - a client-claimed `root` must match the stored thread ancestry — the root
    recorded for the parent, or, when the parent has no stored thread
    metadata, the root the parent itself declares (its own `root`/`reply`
    tag, or the parent itself when it starts the thread);
  - thread depth is capped at 100.
- **Optional:** `["broadcast", "1"]` — marks the event as a broadcast.

## HTTP base derivation

The Elembra client derives the HTTPS base URL from the stored community
`relay_url`:

- `ws://` → `http://`, `wss://` → `https://`, keeping host and port unchanged;
- any **path** on the stored `relay_url` is NOT carried over — the
  access-check, channel-registry, and state endpoints live at the relay host
  root (`/api/v1/relay/access/check`, `/api/v1/relay/access/check-batch`,
  `/api/v1/relay/channels`, `/api/v1/relay/state/events`); a relay served
  under a subpath must expose them at the root.

**Host-derived community isolation:** each community's relay host answers only
its own checks, registry, and state. The client never routes community A's
checks to community B's relay host, and a response from an unexpected
host/pinned key is rejected.

**Transport:** production MUST use `wss://` relay URLs (so the derived base is
`https://`). Plaintext `ws://` is acceptable only for local development and
testing: the client's signature pinning, request-echo, and freshness checks
prevent cross-resource and cross-relay replay and bound response replay to the
60-second freshness window, but plaintext still exposes the traffic to
observation.

## Fail-closed mapping (Elembra client)

| Upstream outcome                                  | Elembra decision |
|---------------------------------------------------|------------------|
| `decision: "allow"`                               | `Allow`          |
| `decision: "deny"`                                | `Deny`           |
| `decision: "not_found"`                           | `NotFound`       |
| transport error / timeout                          | `Deny`           |
| `401` (auth rejected)                              | `Deny`           |
| `5xx`                                              | `Deny`           |
| response signature mismatch / wrong kind / wrong pubkey | `Deny`    |
| response content does not echo request `pubkey`/`channel_id`/`message_id` | `Deny` |
| batch envelope failure (kind / signature / pubkey / missing, malformed, or stale envelope `evaluated_at` / unparseable) | every item `Deny` |
| batch item echo mismatch (single item)             | that item `Deny`   |
| `evaluated_at` older than 60 seconds (stale response) | `Deny`        |
| unparseable or malformed response                  | `Deny`           |

Every failure mode other than an explicit signed `allow` is a denial.
