# ADR-0036: Elembra Chat Zero-Config Bootstrap

Status: Accepted (implemented v1alpha1)  
Date: 2026-08-16

## Context

ADR-0034 established the Elembra/Buzz boundary and ADR-0035 proved production
authorization: the live conformance suite (P1–P12) runs Elembra against the
real relay, and the alpha stack runs `buzz` mode. But obtaining the
Workspace↔Community mapping itself was still a manual operator step: the admin
`POST /api/v1/admin/applications/chat/workspaces/{workspace_id}/community`
call (with CSRF) or a direct SQL `INSERT` into `chat_workspace_communities`.
Both require the operator to already know the community UUID and the relay
pubkey — which live inside the Buzz relay and are discoverable only through the
relay's operator API or its private database.

Verified Buzz facts that shape the design (see
`docs/architecture/elembra-chat-bootstrap.md` §1):

- `communities(id UUID, host, ...)` has no name column; the deployment
  community is auto-seeded at every startup from `RELAY_URL`
  (`ensure_configured_community`, idempotent), so a correctly configured relay
  has exactly one stable row for its own host.
- The relay pubkey derives from `BUZZ_RELAY_PRIVATE_KEY` (not an env var, not
  in `/health`); every signed kind-19030 response is signed by it.
- Host→Community binding is one host = one community, fail-closed on unmapped
  hosts.

The goal: a supported deployment can enable Chat and obtain a correct,
authoritative Workspace↔Community mapping automatically — without direct Buzz
DB access, without manual SQL/curl mapping, and without weakening the
Host→Community isolation model.

## Decision

**Elembra gains a config-gated provisioning mode (`auto|manual`). In `auto`
mode, enabling Chat discovers the deployment Buzz community over one new
read-only, relay-signed discovery endpoint, verifies it, and idempotently
inserts the mapping with the relay pubkey pinned. Manual mode is unchanged.**

1. **Provisioning mode config.** `RUSTSHARE_CHAT_PROVISIONING` = `auto|manual`,
   default `manual` (backward compatible). `auto` requires
   `RUSTSHARE_CHAT_AUTHORITY=buzz` and `RUSTSHARE_CHAT_BOOTSTRAP_RELAY_URL`
   (`ws`/`wss`); any violation is a startup error (fail closed). The alpha
   compose — the supported deployment — sets `auto` and the bootstrap relay
   URL.

2. **Discovery contract.** New Buzz endpoint
   `GET /api/v1/relay/community` on the existing v1alpha1 surface: read-only,
   never mutates state, and runs the same pipeline as the authorization
   endpoints (Host→community bind → NIP-98 → trusted-service gate on
   `RELAY_TRUSTED_SERVICE_PUBKEYS` → admission + replay checks). It returns a
   relay-signed kind-19030 event whose content is
   `{"community_id","host","relay_pubkey","evaluated_at"}`. The Elembra client
   verifies the signature against the `relay_pubkey` **claimed in the content**
   (no prior pin exists at bootstrap; this proves key possession), plus
   `evaluated_at` freshness (≤ 60 s) and structural validity (UUID community,
   64-hex pubkey, non-empty host). Any deviation fails closed.

3. **Bootstrap semantics.** `provision(tenant, workspace)` = discover
   (read-only) → if a mapping already exists: same community → idempotent
   no-op; different community → HTTP 409 `CommunityMismatch` (never
   overwritten silently) → else conflict-safe insert; a workspace-unique race
   returns the winner after verifying the community; a community-unique /
   active-partial violation → HTTP 409 `CommunityInUse`. No mapping is ever
   created from a GET/status request.

4. **Trigger.** In `auto` mode, enabling Chat (`POST
   /api/v1/admin/applications/io.elembra.chat/enable`) attempts provisioning
   inline after the enable succeeds. A failure is logged and leaves Chat
   safely unconfigured — enable still succeeds. The admin retries with
   `POST /api/v1/admin/applications/chat/workspaces/{workspace_id}/provision`
   ("Set up automatically"). In `manual` mode that endpoint returns 400.

5. **No persisted provisioning state.** Failures live in logs and in the retry
   response. Admin diagnostics = `GET /api/v1/admin/applications/chat/
   workspaces/{workspace_id}/community` (admin-only; returns the mapping
   including the pinned relay pubkey; never exposed on the user-facing status
   surface).

6. **Multi-workspace model.** Not auto-provisioned. One host = one community;
   the deployment community is the startup-seeded row. Additional workspaces
   need per-workspace communities on distinct hosts; the relay operator API
   (`POST /operator/communities`, `create_only: true`, NIP-98 +
   `RELAY_OPERATOR_PUBKEYS` + `RELAY_OPERATOR_API_ORIGIN`) is documented as
   that provisioning path, and auto mode deliberately maps only the deployment
   community (fails closed with `CommunityInUse` when it is taken). The schema
   already fails closed on sharing (partial unique index on active mappings
   per community).

7. **Image strategy.** The Buzz fork CI publishes the supported image to
   `ghcr.io/kubedoio/buzz` — built from merged `kubedoio/buzz` main with
   `:main` + immutable `:sha-<7>` tags and provenance attestation. RustShare
   pins `BUZZ_RELAY_IMAGE` to the `sha-<7>` tag; floating/upstream
   (`block/buzz`) images are never used because their API contract is stale or
   absent. Until the merged build exists, local E2E builds the relay image from
   the worktree branch.

8. **UX scope.** The user-facing "No Buzz community is mapped…" string becomes
   the neutral "Chat is being configured for this workspace." The admin page
   `/admin/applications/chat` offers "Set up automatically" (auto mode) and
   "Connect existing Chat deployment" (manual path). Identity/binding flow and
   its pre-existing reachability quirk are unchanged.

## Consequences

### Positive

- **The bridge identity gains no operator powers.** Auto-provisioning uses a
  read-only discovery endpoint gated by the existing trusted-service allowlist
  — it cannot create communities, archive, or transfer. The operator API,
  which grants those powers, remains operator-only.
- **One new read-only surface** behind the existing trusted-service allowlist;
  an empty allowlist disables discovery entirely.
- **Zero-config bootstrap** for supported single-workspace deployments:
  enabling Chat produces a correct mapping with a verified relay pin — no SQL,
  no CSRF curl, no knowledge of the community UUID in advance.
- **The mapping stays explicit and auditable** — it is still a tenant-scoped
  row created by one of two explicit paths (auto-provision or admin form/API),
  never inferred from unauthenticated input (ADR-0034).
- **Manual mode is unchanged** and backward compatible; the default stays
  `manual`.
- Failure is honest: enable succeeds, Chat stays unconfigured, and the admin
  sees the exact retry/diagnostic surface — no half-written state.

### Negative

- `auto` requires explicit configuration (mode + bootstrap relay URL); a
  misconfiguration fails startup rather than degrading.
- The bootstrap contract is tied to the operator-configured relay URL and the
  deployment host — auto-provisioning maps exactly one community; any other
  workspace still needs the operator API + manual mapping.
- The discovery endpoint is one more surface on the trusted-service allowlist
  (read-only, narrow, but present); it must stay behind the allowlist.
- The user-facing status surface still hides the mapping until a binding
  exists, so the UI shows "being configured" during bind entry (pre-existing
  existence-hiding behavior, unchanged by this ADR).
- Until the Buzz PR with the discovery endpoint merges, the supported image is
  not yet published; the alpha compose default tracks the fork build and E2E
  uses a locally built relay image.

## Rejected alternatives

### Use the relay operator API for auto-provisioning

Rejected. `POST /operator/communities` (and its GET/archive/transfer siblings)
grant the caller community lifecycle powers. Issuing that credential to the
Elembra bridge identity would widen the bridge's privilege scope far beyond
what bootstrap needs. The narrow, read-only trusted-service endpoint is the
smallest honest contract: it can discover, never mutate.

### Read the community id from NIP-11

Rejected. NIP-11 advertises the relay's pubkey but **no community id**, and it
is an unauthenticated, unversioned document. Bootstrap needs the host-bound
community UUID plus a signed identity statement; only the kind-19030 discovery
response provides both under the existing NIP-98 + trusted-service pipeline.
NIP-11 alone would leave the community id to be guessed or inferred — exactly
what ADR-0034 forbids.

### Auto-provision multiple workspaces from one deployment community

Rejected. The Host→Community model is one host = one community, and sharing one
community across workspaces breaks tenant isolation (the schema's active-community
partial unique index already refuses it). Per-workspace communities are a
deliberate operator API + manual mapping flow, documented but not automated.

### Persist provisioning state (status rows, "provisioning" states)

Rejected. Bootstrap is a single idempotent step; durable state would duplicate
the mapping row and require lifecycle handling (stale "provisioning" rows,
cleanup, races). Logs + the retry response + the admin diagnostics endpoint
are sufficient and simpler.

## Acceptance criteria

- [x] `RUSTSHARE_CHAT_PROVISIONING` (`auto|manual`, default `manual`) with
  fail-closed startup validation (auto requires `buzz` authority + bootstrap
  relay URL, `ws`/`wss`); unit-tested.
- [x] Buzz `GET /api/v1/relay/community` — read-only, NIP-98 +
  trusted-service-gated, relay-signed kind-19030 `{"community_id","host",
  "relay_pubkey","evaluated_at"}`; fails closed without a stable relay key
  (`BUZZ_RELAY_PRIVATE_KEY` unset → 500); 404 on unmapped host; 401 for
  untrusted callers; handler tests + live proof.
- [x] Elembra gateway discovery client verifies signature against the
  content-claimed pubkey, `evaluated_at` freshness (≤ 60 s), and structural
  validity; unit-tested (wrong key, stale, wrong kind, malformed, transport,
  401 mapping).
- [x] Conflict-safe provisioning: idempotent same-community no-op; 409
  `CommunityMismatch` (never overwrite); 409 `CommunityInUse`; race-safe
  insert; storage + service + handler tests (`backend/tests/chat_bootstrap_test.rs`).
- [x] Enable hook: auto mode provisions inline after enable; failure logged,
  Chat unconfigured, enable succeeds; admin retry endpoint
  (`POST .../provision`, 201 `created|already_configured`) and admin
  diagnostics (`GET .../community`, includes pin) implemented.
- [x] No mapping created from GET/status; no persisted provisioning state.
- [x] UX: neutral "Chat is being configured for this workspace." copy;
  `/admin/applications/chat` page with "Set up automatically" and "Connect
  existing Chat deployment".
- [x] Security proofs: the no-ACL/no-direct-Buzz-DB structural guard and the
  Ask/Search workspace security matrix stay green; live conformance extended
  with the bootstrap discovery proof (live_p13) and the dogfood script with
  auto-path + restart-persistence proofs.
- [x] Image: fork CI publishes `ghcr.io/kubedoio/buzz` (`:main` + `:sha-<7>`,
  provenance attestation) — pending PR merge; alpha compose default updated.

## References

- ADR-0034 — Elembra Chat and Buzz Boundary (mapping rules, "names and URLs
  are not inferred").
- ADR-0035 — Buzz Source Authorization Gateway (NIP-98 / kind-19030 / pinning
  contract the discovery endpoint reuses).
- `docs/architecture/elembra-chat-bootstrap.md` — topology and contract
  details.
- `docs/runbooks/elembra-alpha.md` — operator instructions.
