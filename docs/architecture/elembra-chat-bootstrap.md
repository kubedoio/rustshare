# Elembra Chat Zero-Config Bootstrap

> **Status:** Implemented v1alpha1 (branch `feat/chat-zero-config-bootstrap`)
> **Decision record:** ADR-0036 (this design); boundary and gateway context: ADR-0034, ADR-0035
> **Scope:** How a supported Elembra deployment obtains a correct, authoritative
> Workspace↔Buzz Community mapping automatically — without direct Buzz DB
> access, without manual SQL/curl mapping, and without weakening Buzz's
> Host→Community isolation model.

The one-line contract: **in `auto` mode, enabling Chat discovers the deployment
Buzz community over the relay's own signed discovery endpoint, verifies it, and
idempotently inserts the mapping (with the relay pubkey pinned).** Manual mode
keeps the explicit admin path unchanged.

---

## 1. Buzz provisioning model (verified against the buzz worktree)

These facts are verified against the `kubedoio/buzz` codebase (`main` at
`8ce4dac`, which includes the merged community-identity contract):

- **`communities` has no name.** The table is
  `communities(id UUID PRIMARY KEY DEFAULT gen_random_uuid(), host VARCHAR(255)
  NOT NULL, ...)` with a UNIQUE constraint on `lower(host)`. A community is
  identified by its UUID and bound to exactly one host string — there is no
  human-readable community name to discover or match against.
- **The deployment community is auto-seeded at every startup.** `Db::
  ensure_configured_community` inserts (idempotently:
  `INSERT ... ON CONFLICT DO UPDATE ... RETURNING id, host, (xmax = 0)`) the
  community for `RELAY_URL`'s host. A correctly configured relay therefore
  always has exactly one row for its own host, with a stable UUID across
  restarts.
- **Community UUID discovery surfaces.** The deployment community id is
  observable from: the relay's startup log; the operator API
  (`GET /operator/communities`); every signed `state/events` entry
  (`context.community_id`); and the new read-only discovery endpoint
  `GET /api/v1/relay/community` (§3). Elembra uses only the last of these.
- **The relay pubkey is derived from `BUZZ_RELAY_PRIVATE_KEY`** (an
  operator-provided secret — there is no relay-pubkey env var and the pubkey is
  not exposed on `/health`). The relay's NIP-11 `self` metadata and **every
  signed kind-19030 response** (authorization checks and the community-identity
  response alike) are signed by that stable keypair. Discovery fails closed
  (HTTP 500) when `BUZZ_RELAY_PRIVATE_KEY` is unset, because the dev-mode
  deterministic key must never be handed out as a production pin.
- **Host→Community binding is one host = one community.** `bind_community(db,
  host)` normalizes the request `Host` header to a `communities` row and yields
  the tenant context for the request. A relay process can serve many
  communities on distinct hosts; an unmapped host gets a generic 404 with **no
  fallback community**. This is the isolation property the bootstrap contract
  builds on: the mapping Elembra pins is always the community the relay itself
  binds for the configured host.

## 2. Supported topology

- **Single relay host per community.** Because Buzz derives the community from
  the connection host, one host serves exactly one community.
- **The deployment community is the startup-seeded row** (§1). In the supported
  single-workspace deployment, that community is the one Elembra's workspace
  maps to.
- **Multi-workspace deployments need one community (and host) per workspace.**
  The documented per-workspace provisioning path is the relay **operator API**:
  `POST /operator/communities` with `create_only: true`, gated by NIP-98 plus
  `RELAY_OPERATOR_PUBKEYS` and `RELAY_OPERATOR_API_ORIGIN`. The operator
  provisions a new host (DNS + TLS + relay config) and its community row, then
  the Elembra admin maps the workspace to it with "Connect existing Chat
  deployment".
- **Auto mode deliberately maps only the deployment community.** It is a
  single-workspace convenience, not a general provisioning system. When the
  deployment community is already mapped to another workspace, auto-provisioning
  fails closed with HTTP 409 (`CommunityInUse`) rather than sharing the
  community or inventing a host. The schema already enforces this: the partial
  unique index `chat_workspace_communities_active_community ON (community_id)
  WHERE active` allows only one active mapping per community.

## 3. Bootstrap model

### Modes

| Mode | Configuration | Behavior |
|---|---|---|
| `auto` | `RUSTSHARE_CHAT_PROVISIONING=auto` **requires** `RUSTSHARE_CHAT_AUTHORITY=buzz` and `RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL` (`ws://`/`wss://`); any violation is a startup error | Enabling Chat discovers → verifies → provisions the deployment community mapping automatically |
| `manual` (default) | nothing extra | The existing explicit admin API (`POST .../community`) and admin page remain the only mapping path |

### The discovery contract

`GET /api/v1/relay/community` lives on the existing v1alpha1 surface and reuses
its exact authorization pipeline: Host→community bind (404 when unmapped) →
mandatory NIP-98 → the trusted-service gate on `RELAY_TRUSTED_SERVICE_PUBKEYS`
(an empty allowlist disables discovery entirely) → HTTP admission + NIP-98
replay checks. It is **read-only — it never creates or mutates state** — and its
response is a relay-signed kind-19030 event whose `content` is

```json
{"community_id": "<uuid>", "host": "<normalized host>", "relay_pubkey": "<64-hex>", "evaluated_at": <unix secs>}
```

The Elembra client's verification chain (fail closed on any deviation):

1. **operator-configured relay URL** — the bootstrap URL comes from
   `RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL`, not from any network input;
2. **TLS + NIP-98** — the request goes over `wss` in production and is signed
   by Elembra's provisioned service key, which the relay must trust
   (`RELAY_TRUSTED_SERVICE_PUBKEYS`);
3. **signed kind-19030** — the response must be a valid Nostr event of kind
   19030 whose signature verifies against the `relay_pubkey` **claimed in the
   content** (at bootstrap no pin exists yet; key possession is the proof), with
   `evaluated_at` fresh (≤ 60 s against the client clock) and structurally
   valid fields (UUID community id, 64-hex pubkey, non-empty host);
4. **pin stored** — the verified community id, relay URL, and `relay_pubkey`
   are written to the workspace's mapping row, which every later gateway
   exchange pins against (ADR-0035).

### Idempotency and conflict semantics

`provision(tenant, workspace)` = discover (read-only) → then:

- **no existing mapping** → conflict-safe insert; a workspace-unique race
  returns the winner after verifying it is the same community; a
  community-unique / active-partial violation becomes HTTP 409
  `CommunityInUse`;
- **existing mapping, same community** → idempotent no-op
  (`already_configured`); the pin is never silently rewritten;
- **existing mapping, different community** → HTTP 409 `CommunityMismatch` —
  a mapping is **never overwritten silently**.

No mapping is ever created from a `GET`/status request, and there is **no
persisted provisioning state**: failures live in logs and in the retry
response.

### Trigger and failure semantics

- **Trigger.** In `auto` mode, `POST /api/v1/admin/applications/io.elembra.chat/
  enable` attempts provisioning inline after the enable succeeds.
- **Failure semantics.** A provisioning failure is logged ("chat
  auto-provisioning failed; chat remains unconfigured") and leaves Chat safely
  unconfigured — **enable still succeeds**; the failure never rolls back the
  enablement and never half-writes a mapping. The admin retries with
  `POST /api/v1/admin/applications/chat/workspaces/{workspace_id}/provision`
  ("Set up automatically" on the admin page), which returns HTTP 201 with
  `{"status": "created" | "already_configured", "community_id", "relay_url",
  "relay_pubkey"}`. The provision endpoint is available whenever the buzz
  authority is active AND a bootstrap relay URL is configured (manual mode
  included); it is unavailable in local-authority mode (400, "chat provisioning
  requires the buzz chat authority") — the admin page then offers "Connect
  existing Chat deployment" instead.
- **Diagnostics.** `GET /api/v1/admin/applications/chat/workspaces/
  {workspace_id}/community` is the admin-only mapping surface (community id,
  relay URL, pinned relay pubkey, active flag; 404 when unmapped). The
  user-facing status surface never exposes the mapping until a binding exists
  (deliberate existence-hiding; see the readiness doc's known limitation).

## 4. Image strategy

- **Supported image:** `ghcr.io/kubedoio/buzz`, built **from merged
  `kubedoio/buzz` main** by the fork's CI (`docker.yml`). A main push publishes
  two tags — `:main` (floating) and `:sha-<7>` (immutable 7-hex commit tag) —
  and the build carries a provenance attestation verifiable with
  `gh attestation verify oci://ghcr.io/kubedoio/buzz:sha-<7> --owner kubedoio`.
- **Pin to the SHA tag.** `BUZZ_RELAY_IMAGE` must be pinned to the `sha-<7>`
  tag of the merged-main build that includes the v1alpha1 API and the
  community-identity endpoint. The alpha compose default tracks the fork build
  (`ghcr.io/kubedoio/buzz:main`) and its comment instructs operators to pin
  once the merged build exists.
- **Why not floating/upstream tags.** Upstream `ghcr.io/block/buzz` predates
  the v1alpha1 authorization API and the discovery endpoint — its contract is
  stale or absent, and a "relay-v\*" tag there is a different lineage. A
  floating `:main` can change under a running deployment with no signal. The
  immutable `sha-<7>` tag identifies the exact main commit the image was built
  from, so the pinned contract is reproducible and auditable.

**Current status (2026-08-16):** the community-identity endpoint and the fork
image publishing are merged in `kubedoio/buzz` main (`8ce4dac`, PR #2). The
supported image is `ghcr.io/kubedoio/buzz:sha-8ce4dac` (built from the merged
main by the fork CI; see `docker-compose.alpha.yml`'s `BUZZ_RELAY_IMAGE`
default). No dependence on floating upstream tags, no stale images.

---

## Related documents

- ADR-0034 (`docs/adr/0034-elembra-chat-buzz-boundary.md`) — the Elembra/Buzz
  boundary and mapping rules; §"Contracts implemented" now cross-references the
  discovery contract.
- ADR-0035 (`docs/adr/0035-buzz-source-authorization-gateway.md`) — the
  NIP-98/kind-19030 gateway contract the discovery endpoint reuses.
- ADR-0036 (`docs/adr/0036-elembra-chat-zero-config-bootstrap.md`) — this
  design's decision record.
- Runbook (`docs/runbooks/elembra-alpha.md`) — operator instructions for the
  alpha deployment (§2.2, §3).
- Readiness (`docs/architecture/elembra-chat-alpha-readiness.md`) — Alpha
  contract and the provisioning known limitation (§15).
