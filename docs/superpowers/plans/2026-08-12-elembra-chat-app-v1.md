# Elembra Chat Application v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the Elembra Chat Application vertical slice — login → workspace → Chat app → Buzz community/channels → signed messages → Files attachments → Ask-this-channel with exact-message citations — while Buzz stays the authoritative chat engine.

**Architecture:** Reads (channels, timeline) are served by Elembra from the existing Buzz observation projection, gated per message through the existing `ChatResourceOwner`/`SourceAuthorizer` → `BuzzAuthority` chain. Writes are client-direct: the browser holds the user's Buzz key (passphrase-encrypted at rest), signs kind-1 events, and publishes over a NIP-42 relay WebSocket session. Live updates ride the existing `EventBroadcaster`/`/api/ws` on webhook ingest, with a 15 s polling fallback.

**Tech Stack:** Rust/Axum/SQLx/PostgreSQL (backend), SvelteKit 5 + TypeScript + TanStack Query (frontend), `@noble/curves` for BIP-340 Schnorr signing (new frontend dependency), Nostr wire protocol (NIP-01/42).

**Spec:** `docs/superpowers/specs/2026-08-12-elembra-chat-app-v1-design.md` — read it first; this plan implements it slice by slice.

---

## File Structure

Backend:
- Modify: `backend/crates/core/src/events/types.rs` — add `EventType::ChatMessageObserved`, `AggregateType::ChatMessage`, `ChatMessageObservedPayload`.
- Modify: `backend/crates/storage/src/chat_observation.rs` — add `distinct_channels`, `list_for_timeline` (+ `ChannelSummary`).
- Modify: `backend/server/src/authz/chat_owner.rs` — add public `can_read_channel`.
- Modify: `backend/server/src/buzz_observation.rs` — hold + use `Arc<EventBroadcaster>`.
- Modify: `backend/server/src/handlers/sync.rs` — tenant-scoped relevance for chat events.
- Modify: `backend/server/src/state.rs` + `backend/server/src/bootstrap.rs` — expose `chat_owner` on `AppState`.
- Create: `backend/server/src/handlers/chat_app.rs` — `chat_status`, `list_channels`, `list_messages`, `get_message`.
- Modify: `backend/server/src/routes.rs` — register the four routes.
- Test: `backend/tests/chat_app_read_test.rs` (DB-backed, `#[ignore]`).

Frontend:
- Modify: `frontend/package.json` — add `@noble/curves`.
- Create: `frontend/src/lib/api/chat.ts`, `frontend/src/lib/chat/nostr.ts`, `frontend/src/lib/chat/keys.ts`.
- Create: `frontend/src/lib/components/chat/ChatApplicationView.svelte`, `ChannelList.svelte`, `MessageTimeline.svelte`, `MessageComposer.svelte`, `AttachmentPicker.svelte`, `BindingPanel.svelte`.
- Modify: `frontend/src/routes/(app)/apps/[key]/ApplicationPageRenderer.svelte`, `frontend/src/lib/applications/iconRegistry.ts`, `frontend/src/lib/applications/applicationPages.ts`, `frontend/src/lib/websocket/events.ts`, `frontend/src/lib/websocket/manager.ts`, `frontend/src/lib/applications/registry.ts` (if the renderer key needs registration).
- Test: co-located `*.test.ts` per module.

Scripts/docs:
- Create: `scripts/run-chat-e2e.sh`.
- Modify: `CHANGELOG.md`, `docs/implementation/` note.

Validation commands (run at the end of each slice):
- Backend: `cargo fmt --all --check`, `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings`, `SQLX_OFFLINE=true cargo test --workspace --all-features --lib`, `DATABASE_URL=postgres://test:test@localhost:5432/test cargo test --workspace --all-features -- --ignored --test-threads=1` (DB-backed tests; `test` role provisioned in the dev postgres container).
- Frontend: `cd frontend && npm run check && npm run lint && npm run test && npm run build`.

---

## Slice 1 — Backend read surface

### Task 1: Add the ChatMessageObserved event type and payload

**Files:**
- Modify: `backend/crates/core/src/events/types.rs` (EventType enum ends at line 96; AggregateType at line 22; payload structs live in this module)

- [ ] **Step 1: Add the enum variants**

In `AggregateType` (after `MailImportJob,`):

```rust
    MailImportJob,
    ChatMessage,
```

In `EventType` (after `MailRemoteAction,`):

```rust
    MailRemoteAction,
    ChatMessageObserved,
```

- [ ] **Step 2: Add the payload struct next to the other `*Payload` structs in the same file**

```rust
/// Payload of `EventType::ChatMessageObserved`, published after a signed Buzz
/// event is first ingested into the observation index.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ChatMessageObservedPayload {
    pub tenant_id: Uuid,
    pub workspace_id: Uuid,
    pub community_id: String,
    pub channel_id: String,
    pub channel_kind: String,
    pub message_id: String,
    pub event_id: String,
}
```

- [ ] **Step 3: Re-export the payload with the others**

Find the `pub use`/module re-export list that exposes `ShareCreatedPayload` etc. and add `ChatMessageObservedPayload` to it. (`rustshare_core::events::{FileModifiedPayload, ShareCreatedPayload}` must resolve from `backend/server`, which it does today for the existing payloads — mirror their re-export.)

- [ ] **Step 4: Build check**

Run: `SQLX_OFFLINE=true cargo check -p rustshare-core --all-features`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/core/src/events/
git commit -s -m "feat(core): add ChatMessageObserved event type and payload"
```

### Task 2: Observation store timeline + channel queries

**Files:**
- Modify: `backend/crates/storage/src/chat_observation.rs` (append after `list_for_reconcile`, before `row_to_event`)
- Test: none yet (covered by Task 10 integration tests)

- [ ] **Step 1: Add `ChannelSummary` near the top of the file**

```rust
/// Distinct channel derived from the observation index for one community.
#[derive(Debug, Clone)]
pub struct ChannelSummary {
    pub channel_id: String,
    pub channel_kind: ChatChannelKind,
    pub latest_event_at: DateTime<Utc>,
}
```

- [ ] **Step 2: Add `distinct_channels` inside `impl ChatObservationStore`**

```rust
    /// Distinct observed channels for a community, newest activity first.
    /// Derived projection only — Buzz owns channel identity.
    pub async fn distinct_channels(
        &self,
        tenant_id: TenantId,
        community_id: &str,
    ) -> Result<Vec<ChannelSummary>> {
        let rows = sqlx::query(
            "SELECT channel_id, channel_kind, MAX(event_created_at) AS latest_event_at
             FROM chat_observed_events
             WHERE tenant_id = $1 AND community_id = $2 AND active = true
             GROUP BY channel_id, channel_kind
             ORDER BY latest_event_at DESC",
        )
        .bind(tenant_id.0)
        .bind(community_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(ChannelSummary {
                    channel_id: row.try_get("channel_id")?,
                    channel_kind: parse_channel_kind(row.try_get("channel_kind")?)?,
                    latest_event_at: row.try_get("latest_event_at")?,
                })
            })
            .collect()
    }
```

- [ ] **Step 3: Add `list_for_timeline` (folded latest-event-per-message, paginated)**

```rust
    /// Timeline fold: the latest active event of each message in one channel,
    /// `before` being an exclusive `event_created_at` watermark, newest first.
    pub async fn list_for_timeline(
        &self,
        tenant_id: TenantId,
        community_id: &str,
        channel_id: &str,
        before: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<ChatObservedEvent>> {
        let rows = sqlx::query(
            "SELECT DISTINCT ON (message_id)
                    tenant_id, workspace_id, event_id, message_id, event_type,
                    supersedes_event_id, community_id, channel_id, channel_kind,
                    thread_root_id, author_pubkey, author_principal_id,
                    event_created_at, observed_at, checksum, signature,
                    signature_verified, body, active
             FROM chat_observed_events
             WHERE tenant_id = $1 AND community_id = $2 AND channel_id = $3
               AND ($4::timestamptz IS NULL OR event_created_at < $4)
             ORDER BY message_id, event_created_at DESC, event_id DESC",
        )
        .bind(tenant_id.0)
        .bind(community_id)
        .bind(channel_id)
        .bind(before)
        .fetch_all(&self.pool)
        .await?;
        let mut events: Vec<ChatObservedEvent> =
            rows.iter().map(row_to_event).collect::<Result<_>>()?;
        events.sort_by(|a, b| {
            b.event_created_at
                .cmp(&a.event_created_at)
                .then_with(|| b.event_id.cmp(&a.event_id))
        });
        events.truncate(limit as usize);
        Ok(events)
    }
```

(All column names match the existing `row_to_event` reader; `parse_channel_kind` is already in this file.)

- [ ] **Step 4: Build check**

Run: `SQLX_OFFLINE=true cargo check -p rustshare-storage --all-features`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/storage/src/chat_observation.rs
git commit -s -m "feat(storage): timeline and channel queries for the observation index"
```

### Task 3: `ChatResourceOwner::can_read_channel`

**Files:**
- Modify: `backend/server/src/authz/chat_owner.rs` (add after `declared_capabilities`, ~line 166)

- [ ] **Step 1: Add the channel-level gate method**

```rust
    /// Channel-level gate used by the Chat app's channel list: the same
    /// Elembra pre-filters as [`Self::gate`], then the authority's
    /// channel decision (message_id: None). Fail closed everywhere.
    pub async fn can_read_channel(
        &self,
        ctx: &PrincipalContext,
        community_id: &str,
        channel_id: &str,
        channel_kind: rustshare_resource_auth::BuzzChannelKind,
    ) -> bool {
        let mapping = match self.chat_identity.mapping(ctx.tenant_id, ctx.workspace_id).await {
            Ok(Some(mapping)) => mapping,
            Ok(None) => return false,
            Err(error) => {
                tracing::warn!(%error, "chat channel gate: mapping lookup failed");
                return false;
            }
        };
        if !mapping.active || mapping.community_id != community_id {
            return false;
        }
        let access = match self
            .chat_identity
            .chat_access(ctx.tenant_id, ctx.workspace_id, ctx.principal_id)
            .await
        {
            Ok(access) => access,
            Err(error) => {
                tracing::warn!(%error, "chat channel gate: access lookup failed");
                return false;
            }
        };
        if !access {
            return false;
        }
        let binding = match self.chat_identity.active_binding(ctx.tenant_id, ctx.principal_id).await {
            Ok(Some(binding)) => binding,
            Ok(None) => return false,
            Err(error) => {
                tracing::warn!(%error, "chat channel gate: binding lookup failed");
                return false;
            }
        };
        if binding.status != rustshare_resource_auth::BindingStatus::Active {
            return false;
        }
        let admitted = match self
            .chat_identity
            .active_admission(ctx.tenant_id, community_id, &binding.buzz_pubkey)
            .await
        {
            Ok(admitted) => admitted,
            Err(error) => {
                tracing::warn!(%error, "chat channel gate: admission lookup failed");
                return false;
            }
        };
        if !admitted {
            return false;
        }
        let decision = self
            .authority
            .can_read(&rustshare_resource_auth::BuzzReadRequest {
                tenant_id: ctx.tenant_id,
                community_id: community_id.to_string(),
                relay_url: mapping.relay_url.clone(),
                relay_pubkey: mapping.relay_pubkey.clone(),
                channel_id: channel_id.to_string(),
                channel_kind,
                message_id: None,
                pubkey: binding.buzz_pubkey.clone(),
                event_created_at: chrono::Utc::now(),
            })
            .await;
        matches!(decision, Ok(rustshare_resource_auth::BuzzReadDecision::Allow))
    }
```

Field/method names used above are the verified public API of `ChatIdentityStore` (`mapping`, `chat_access`, `active_binding`, `active_admission`) and the `BuzzReadRequest` shape from `backend/crates/resource-auth/src/buzz_authority.rs`. If the compiler points at a different `BindingStatus` import path, use the one already imported in this file (it imports from `rustshare_resource_auth`).

- [ ] **Step 2: Build check**

Run: `SQLX_OFFLINE=true cargo check -p rustshare-server --all-features`
Expected: compiles (fix only import paths, never the gate semantics).

- [ ] **Step 3: Commit**

```bash
git add backend/server/src/authz/chat_owner.rs
git commit -s -m "feat(authz): channel-level ChatResourceOwner gate"
```

### Task 4: Expose `chat_owner` on `AppState`

**Files:**
- Modify: `backend/server/src/state.rs` (~line 234, after `pub buzz_observation_service`)
- Modify: `backend/server/src/bootstrap.rs` (where `ChatResourceOwner` is constructed and `source_authorizer` seeded)

- [ ] **Step 1: Add the state field**

```rust
    /// Final Chat authorization owner (binding/admission/mapping pre-filters
    /// + Buzz authority), also used by the Chat app read surface.
    pub chat_owner: Arc<crate::authz::chat_owner::ChatResourceOwner>,
```

- [ ] **Step 2: Populate it in bootstrap**

Find where `ChatResourceOwner` is constructed today (search `ChatResourceOwner::` in `bootstrap.rs` — it builds `SourceAuthorizer` with the owner). Store that same instance in an `Arc` and assign it to the new `AppState` field (constructor already returns a `ChatResourceOwner`; wrap once). Do not construct a second owner.

- [ ] **Step 3: Build check**

Run: `SQLX_OFFLINE=true cargo check -p rustshare-server --all-features`
Expected: compiles.

- [ ] **Step 4: Commit**

```bash
git add backend/server/src/state.rs backend/server/src/bootstrap.rs
git commit -s -m "feat(server): expose ChatResourceOwner on AppState"
```

### Task 5: Broadcast on ingest

**Files:**
- Modify: `backend/server/src/buzz_observation.rs` (struct at line 137, `new` at ~150, `verify_and_ingest` at line 172)

- [ ] **Step 1: Hold the broadcaster**

Add the field and parameter:

```rust
pub struct BuzzObservationService {
    pool: sqlx::PgPool,
    chat_identity: ChatIdentityStore,
    observations: ChatObservationStore,
    outbox: Arc<OutboxStore>,
    signer: WebhookSigner,
    max_age_seconds: u64,
    broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
}
```

In `new(..., max_age_seconds: u64, broadcaster: Arc<rustshare_core::events::EventBroadcaster>)`, assign it.

- [ ] **Step 2: Publish after first observation commits**

In `verify_and_ingest`, immediately after the successful `tx.commit()` (the `Ok(IngestOutcome::FirstObservation)` return site), publish best-effort — the event must never fail ingest:

```rust
        tx.commit()
            .await
            .map_err(|e| BuzzPushError::Persistence(e.to_string()))?;
        let author_user_id = data
            .principal
            .principal_id
            .map(|principal_id| principal_id.0)
            .unwrap_or_default();
        self.broadcaster
            .publish(rustshare_core::events::Event::new(
                rustshare_core::events::EventType::ChatMessageObserved,
                uuid::Uuid::new_v4(),
                rustshare_core::events::AggregateType::ChatMessage,
                serde_json::json!(rustshare_core::events::ChatMessageObservedPayload {
                    tenant_id: tenant.0,
                    workspace_id: workspace.0,
                    community_id: data.community_id.clone(),
                    channel_id: data.channel_id.clone(),
                    channel_kind: data.channel_kind.as_str().to_string(),
                    message_id: data.message_id.clone(),
                    event_id: data.event_id.clone(),
                }),
                author_user_id,
            ));
        Ok(IngestOutcome::FirstObservation)
```

Adjust field names to the local `data`/`tenant`/`workspace` bindings that already exist in this function (they are named `tenant`, `workspace`, `row`, `data` in the current body). If `data` does not expose `channel_kind.as_str()`, use `format!("{:?}", data.channel_kind).to_lowercase()` — but prefer the enum's `as_str` (same one `upsert_event_in_tx` already uses via `event.channel_kind.as_str()`).

- [ ] **Step 3: Update the bootstrap call site**

`BuzzObservationService::new(...)` in `bootstrap.rs` now needs the broadcaster — pass `Arc::clone(&broadcaster)` (the same `Arc` assigned to `AppState.broadcaster`).

- [ ] **Step 4: Build check**

Run: `SQLX_OFFLINE=true cargo check -p rustshare-server --all-features`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add backend/server/src/buzz_observation.rs backend/server/src/bootstrap.rs
git commit -s -m "feat(observation): broadcast ChatMessageObserved after ingest"
```

### Task 6: Tenant-scoped WS relevance for chat events

**Files:**
- Modify: `backend/server/src/handlers/sync.rs` (`should_send_event_to_client` line 273, `should_send_event_to_user` line 307)

- [ ] **Step 1: Pass `tenant_id` through**

Change `should_send_event_to_client`'s user arm to destructure the tenant:

```rust
        ClientIdentity::User { user_id, tenant_id } => {
            should_send_event_to_user(event, *user_id, *tenant_id, metadata_store).await
        }
```

Change `should_send_event_to_user`'s signature and add the chat arm before the `match event.event_type` share block (after the `event.user_id == user_id` early return):

```rust
async fn should_send_event_to_user(
    event: &Event,
    user_id: UserId,
    tenant_id: Uuid,
    metadata_store: &rustshare_storage::MetadataStore,
) -> Result<bool, String> {
    if event.event_type == EventType::ChatMessageObserved {
        let payload: ChatMessageObservedPayload = serde_json::from_value(event.payload.clone())
            .map_err(|e| format!("Failed to deserialize ChatMessageObservedPayload: {e}"))?;
        return Ok(payload.tenant_id == tenant_id);
    }

    // For most events, send to the user who triggered them
    if event.user_id == user_id {
        return Ok(true);
    }
    // ... existing match stays unchanged
```

Add the import next to the existing payload imports (`use rustshare_core::events::{...}`) — add `ChatMessageObservedPayload`.

- [ ] **Step 2: Update the existing unit test call sites in this file**

The tests at the bottom call `should_send_event_to_user(event, user_id, &store)` — add the tenant argument (the test user's `tenant_id`) to each call. Grep for `should_send_event_to_user(` in the `#[cfg(test)]` block and fix every call.

- [ ] **Step 3: Run the sync unit tests**

Run: `SQLX_OFFLINE=true cargo test -p rustshare-server --lib handlers::sync`
Expected: pass (non-DB tests).

- [ ] **Step 4: Commit**

```bash
git add backend/server/src/handlers/sync.rs
git commit -s -m "feat(sync): tenant-scoped ChatMessageObserved relevance"
```

### Task 7: Chat app handlers

**Files:**
- Create: `backend/server/src/handlers/chat_app.rs`
- Modify: `backend/server/src/handlers/mod.rs` (add `pub mod chat_app;`)

- [ ] **Step 1: Write the handlers file**

```rust
//! Read surface for the Elembra Chat application: status, channels, and the
//! message timeline. Buzz remains the chat source of truth; these endpoints
//! serve Elembra's authorized observation projection and never read Buzz's
//! private database.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_resource_auth::{
    ApplicationId, ActionCapability, PrincipalContext, ResourceRef,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AppError, AuthenticatedUser};
use crate::state::AppState;

const CHAT_APPLICATION: &str = "io.elembra.chat";
const CHAT_READ: &str = "chat.read";

fn principal(auth: &AuthenticatedUser) -> PrincipalContext {
    PrincipalContext::user(
        PrincipalId(auth.user_id),
        TenantId(auth.tenant_id),
        WorkspaceId(auth.tenant_id),
    )
}

#[derive(Debug, Serialize)]
pub struct CommunityMappingInfo {
    pub community_id: String,
    pub relay_url: String,
}

#[derive(Debug, Serialize)]
pub struct BindingInfo {
    pub status: String,
    pub buzz_pubkey: String,
}

#[derive(Debug, Serialize)]
pub struct ChatStatusResponse {
    pub chat_enabled: bool,
    pub mapping: Option<CommunityMappingInfo>,
    pub binding: Option<BindingInfo>,
    pub admission_active: bool,
}

#[derive(Debug, Serialize)]
pub struct ChannelInfo {
    pub channel_id: String,
    pub channel_kind: String,
    pub latest_event_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ChatMessageDto {
    pub message_id: String,
    pub event_id: String,
    pub community_id: String,
    pub channel_id: String,
    pub channel_kind: String,
    pub author_pubkey: String,
    pub event_created_at: DateTime<Utc>,
    pub thread_root_id: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MessagesResponse {
    pub messages: Vec<ChatMessageDto>,
    pub next_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct ListMessagesQuery {
    pub channel_id: String,
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

/// Own chat state: enablement, mapping, binding, admission. Only ever the
/// caller's own data.
pub async fn chat_status(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<ChatStatusResponse>, AppError> {
    let ctx = principal(&auth);
    let chat_identity = state.chat_owner.chat_identity_store();
    let chat_enabled = chat_identity
        .chat_access(ctx.tenant_id, ctx.workspace_id, ctx.principal_id)
        .await
        .map_err(|e| AppError::internal(format!("chat status lookup failed: {e}")))?;
    let mapping = chat_identity
        .mapping(ctx.tenant_id, ctx.workspace_id)
        .await
        .map_err(|e| AppError::internal(format!("chat mapping lookup failed: {e}")))?;
    let binding = chat_identity
        .active_binding(ctx.tenant_id, ctx.principal_id)
        .await
        .map_err(|e| AppError::internal(format!("chat binding lookup failed: {e}")))?;
    let mut admission_active = false;
    let mut mapping_info = None;
    if let (Some(mapping), Some(binding)) = (&mapping, &binding) {
        if mapping.active && binding.status == rustshare_resource_auth::BindingStatus::Active {
            admission_active = chat_identity
                .active_admission(ctx.tenant_id, &mapping.community_id, &binding.buzz_pubkey)
                .await
                .map_err(|e| AppError::internal(format!("chat admission lookup failed: {e}")))?;
        }
        mapping_info = Some(CommunityMappingInfo {
            community_id: mapping.community_id.clone(),
            relay_url: mapping.relay_url.clone(),
        });
    }
    Ok(Json(ChatStatusResponse {
        chat_enabled,
        mapping: mapping_info,
        binding: binding.map(|b| BindingInfo {
            status: format!("{:?}", b.status),
            buzz_pubkey: b.buzz_pubkey,
        }),
        admission_active,
    }))
}

/// Channels of the mapped community the caller may currently read. Derived
/// from the observation index and gated channel-by-channel.
pub async fn list_channels(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<ChannelInfo>>, AppError> {
    let ctx = principal(&auth);
    let chat_identity = state.chat_owner.chat_identity_store();
    let Some(mapping) = chat_identity
        .mapping(ctx.tenant_id, ctx.workspace_id)
        .await
        .map_err(|e| AppError::internal(format!("chat mapping lookup failed: {e}")))?
    else {
        return Ok(Json(Vec::new()));
    };
    if !mapping.active {
        return Ok(Json(Vec::new()));
    }
    let summaries = state
        .chat_observation_store
        .distinct_channels(ctx.tenant_id, &mapping.community_id)
        .await
        .map_err(|e| AppError::internal(format!("chat channel lookup failed: {e}")))?;
    let mut channels = Vec::new();
    for summary in summaries {
        let kind = match summary.channel_kind {
            rustshare_memory::event::ChatChannelKind::Workspace => {
                rustshare_resource_auth::BuzzChannelKind::Workspace
            }
            rustshare_memory::event::ChatChannelKind::Dm => {
                rustshare_resource_auth::BuzzChannelKind::Dm
            }
            rustshare_memory::event::ChatChannelKind::Private => {
                rustshare_resource_auth::BuzzChannelKind::Private
            }
            rustshare_memory::event::ChatChannelKind::Excluded => {
                rustshare_resource_auth::BuzzChannelKind::Excluded
            }
        };
        if state
            .chat_owner
            .can_read_channel(&ctx, &mapping.community_id, &summary.channel_id, kind)
            .await
        {
            channels.push(ChannelInfo {
                channel_id: summary.channel_id,
                channel_kind: summary.channel_kind.as_str().to_string(),
                latest_event_at: summary.latest_event_at,
            });
        }
    }
    Ok(Json(channels))
}

/// Timeline for one channel: folded latest-event-per-message, newest first,
/// per-message authorization, deleted messages hidden.
pub async fn list_messages(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<ListMessagesQuery>,
) -> Result<Json<MessagesResponse>, AppError> {
    let ctx = principal(&auth);
    let chat_identity = state.chat_owner.chat_identity_store();
    let Some(mapping) = chat_identity
        .mapping(ctx.tenant_id, ctx.workspace_id)
        .await
        .map_err(|e| AppError::internal(format!("chat mapping lookup failed: {e}")))?
    else {
        return Ok(Json(MessagesResponse {
            messages: Vec::new(),
            next_before: None,
        }));
    };
    if !mapping.active {
        return Ok(Json(MessagesResponse {
            messages: Vec::new(),
            next_before: None,
        }));
    }
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let events = state
        .chat_observation_store
        .list_for_timeline(
            ctx.tenant_id,
            &mapping.community_id,
            &query.channel_id,
            query.before,
            limit,
        )
        .await
        .map_err(|e| AppError::internal(format!("chat timeline lookup failed: {e}")))?;

    // Fold: drop tombstones (latest event deleted) and inactive rows.
    let visible: Vec<_> = events
        .into_iter()
        .filter(|event| {
            event.active
                && !matches!(
                    event.event_type,
                    rustshare_memory::event::ObservedEventType::Deleted
                )
        })
        .collect();

    // Per-message authorization through the existing Chat owner (fail closed).
    let action = ActionCapability::new(CHAT_READ);
    let refs: Vec<ResourceRef> = visible
        .iter()
        .map(|event| {
            ResourceRef::new(
                ApplicationId::new(CHAT_APPLICATION),
                "message",
                event.message_id.clone(),
            )
        })
        .collect();
    let decisions = if refs.is_empty() {
        Vec::new()
    } else {
        state
            .source_authorizer
            .authorize_batch(&ctx, &action, &refs)
            .await
            .map_err(|e| {
                tracing::warn!(%e, "chat timeline: batch authorization failed");
                AppError::internal("chat timeline authorization unavailable")
            })?
    };

    let mut messages = Vec::new();
    for (event, decision) in visible.iter().zip(&decisions) {
        if !decision.decision.is_allow() {
            continue;
        }
        messages.push(ChatMessageDto {
            message_id: event.message_id.clone(),
            event_id: event.event_id.clone(),
            community_id: event.community_id.clone(),
            channel_id: event.channel_id.clone(),
            channel_kind: event.channel_kind.as_str().to_string(),
            author_pubkey: event.author_pubkey.clone(),
            event_created_at: event.event_created_at,
            thread_root_id: event.thread_root_id.clone(),
            body: event.body.clone(),
        });
    }
    let next_before = messages.last().map(|m| m.event_created_at);
    Ok(Json(MessagesResponse {
        messages,
        next_before,
    }))
}

/// Single folded message, for citation focus/scroll. Existence-hiding 404.
pub async fn get_message(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(message_id): Path<String>,
) -> Result<Json<ChatMessageDto>, AppError> {
    let ctx = principal(&auth);
    let events = state
        .chat_observation_store
        .get_by_message_id(ctx.tenant_id, &message_id)
        .await
        .map_err(|e| AppError::internal(format!("chat message lookup failed: {e}")))?;
    let Some(latest) = events.into_iter().next_back() else {
        return Err(AppError::not_found("resource unavailable"));
    };
    if !latest.active
        || matches!(
            latest.event_type,
            rustshare_memory::event::ObservedEventType::Deleted
        )
    {
        return Err(AppError::not_found("resource unavailable"));
    }
    let resource = ResourceRef::new(
        ApplicationId::new(CHAT_APPLICATION),
        "message",
        latest.message_id.clone(),
    );
    let decision = state
        .source_authorizer
        .authorize(&ctx, &ActionCapability::new(CHAT_READ), &resource)
        .await;
    if !decision.is_allow() {
        return Err(AppError::not_found("resource unavailable"));
    }
    Ok(Json(ChatMessageDto {
        message_id: latest.message_id.clone(),
        event_id: latest.event_id.clone(),
        community_id: latest.community_id.clone(),
        channel_id: latest.channel_id.clone(),
        channel_kind: latest.channel_kind.as_str().to_string(),
        author_pubkey: latest.author_pubkey.clone(),
        event_created_at: latest.event_created_at,
        thread_root_id: latest.thread_root_id.clone(),
        body: latest.body.clone(),
    }))
}
```

- [ ] **Step 2: Expose the identity store handle the handlers need**

`chat_app.rs` uses `state.chat_owner.chat_identity_store()`; observation queries go through
`state.chat_observation_store` (already on `AppState`). Add one accessor to `ChatResourceOwner`:

```rust
    pub fn chat_identity_store(&self) -> &ChatIdentityStore {
        &self.chat_identity
    }
```

- [ ] **Step 3: Register the module**

In `backend/server/src/handlers/mod.rs`, add `pub mod chat_app;` next to `pub mod chat_identity;`.

- [ ] **Step 4: Register the routes**

In `routes.rs`, inside `chat_integration_routes()` add before the admin routes:

```rust
        .route(
            "/api/v1/applications/chat/status",
            get(crate::handlers::chat_app::chat_status),
        )
        .route(
            "/api/v1/applications/chat/channels",
            get(crate::handlers::chat_app::list_channels),
        )
        .route(
            "/api/v1/applications/chat/messages",
            get(crate::handlers::chat_app::list_messages),
        )
        .route(
            "/api/v1/applications/chat/messages/{message_id}",
            get(crate::handlers::chat_app::get_message),
        )
```

(`get` is already imported in that function via `use axum::routing::{get, patch, post};`.)

- [ ] **Step 5: Build + fix**

Run: `SQLX_OFFLINE=true cargo check -p rustshare-server --all-features`
Expected: compiles. Fix import-path or field-name mismatches only; do not change the authorization semantics. Note: `ApplicationId::new` and `ActionCapability` are re-exported from `rustshare_resource_auth` — if the import paths differ, mirror the imports used in `backend/server/src/services/unified_search.rs`.

- [ ] **Step 6: OpenAPI check**

Run: `SQLX_OFFLINE=true cargo test -p rustshare-server --test openapi_export_test`
Expected: if it fails because the four routes are missing from the generated spec, add them to the OpenAPI registration in `backend/server/src/lib.rs` (or wherever `openapi_export_test` enumerates paths — grep for `"/api/v1/memory/ask"` and add the chat routes beside it with the same helper). Add `utoipa::ToSchema` derives to `ChatStatusResponse`, `ChannelInfo`, `ChatMessageDto`, `MessagesResponse`.

- [ ] **Step 7: Commit**

```bash
git add backend/server/src/handlers/chat_app.rs backend/server/src/handlers/mod.rs backend/server/src/routes.rs backend/server/src/authz/chat_owner.rs
git commit -s -m "feat(chat): status, channel and message read endpoints"
```

### Task 8: Backend integration tests for the read surface

**Files:**
- Create: `backend/tests/chat_app_read_test.rs`

Follow the harness of `backend/tests/chat_owner_authorization_test.rs` (DB-backed `#[ignore]`, uses `DATABASE_URL`, seeds users/workspaces/bindings/mappings/admissions and observation rows). Read that file and `backend/tests/buzz_push_test.rs` first for the exact helpers; replicate their setup style.

Tests (each `#[tokio::test] #[ignore]`):

1. `status_unconfigured_returns_empty_shape` — tenant without mapping: `chat_enabled: false`, `mapping: None`, `binding: None`, `admission_active: false`.
2. `channels_requires_mapping_and_gate` — with mapping + active binding + admission and seeded observation rows: a `workspace` channel is listed, a `dm` channel is not (local mode).
3. `timeline_returns_authorized_messages_folded` — two tenants share a relay-adjacent setup; tenant B's observation rows for community A are never visible to tenant A's timeline query (cross-workspace proof #4).
4. `timeline_hides_deleted_and_inactive` — a message with a later `deleted` event is absent; an `active=false` row is absent.
5. `get_message_hides_denied_and_deleted` — deleted message → 404; cross-tenant message_id → 404.
6. `revoked_binding_denies_reads` — revoke the binding/admission (via `ChatIdentityStore::revoke_principal`), then `list_messages`/`get_message` return empty/404 (proof #6 read half).

Run: `DATABASE_URL=postgres://test:test@localhost:5432/test SQLX_OFFLINE=true cargo test --workspace --all-features --test chat_app_read_test -- --ignored --test-threads=1`
Expected: all pass.

- [ ] **Step: Commit**

```bash
git add backend/tests/chat_app_read_test.rs
git commit -s -m "test(chat): read-surface authorization matrix"
```

### Task 9: Slice 1 validation gate

- [ ] Run the full backend baseline:

```bash
cargo fmt --all --check
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --workspace --all-features --lib
cargo sqlx prepare --workspace --check
DATABASE_URL=postgres://test:test@localhost:5432/test SQLX_OFFLINE=true cargo test --workspace --all-features --test chat_app_read_test -- --ignored --test-threads=1
DATABASE_URL=postgres://test:test@localhost:5432/test SQLX_OFFLINE=true cargo test --workspace --all-features -- --ignored --test-threads=1
```

Expected: everything green. Do not proceed to Slice 2 with red checks.

- [ ] Commit any fixes produced by this gate.

---

## Slice 2 — Frontend chat application

### Task 10: Add the signing dependency

**Files:**
- Modify: `frontend/package.json`, `frontend/package-lock.json`

- [ ] **Step 1: Install**

Run: `cd frontend && npm install @noble/curves`
Expected: installs `@noble/curves` (pure JS, no wasm) and updates the lockfile.

- [ ] **Step 2: Commit**

```bash
git add frontend/package.json frontend/package-lock.json
git commit -s -m "feat(frontend): add @noble/curves for Nostr BIP-340 signing"
```

### Task 11: Chat API client

**Files:**
- Create: `frontend/src/lib/api/chat.ts`
- Test: `frontend/src/lib/api/chat.test.ts`

- [ ] **Step 1: Write the client**

```ts
import { apiClient } from './client';

export interface ChatCommunityMapping {
	community_id: string;
	relay_url: string;
}

export interface ChatBinding {
	status: string;
	buzz_pubkey: string;
}

export interface ChatStatus {
	chat_enabled: boolean;
	mapping: ChatCommunityMapping | null;
	binding: ChatBinding | null;
	admission_active: boolean;
}

export interface ChatChannelInfo {
	channel_id: string;
	channel_kind: string;
	latest_event_at: string;
}

export interface ChatMessageDto {
	message_id: string;
	event_id: string;
	community_id: string;
	channel_id: string;
	channel_kind: string;
	author_pubkey: string;
	event_created_at: string;
	thread_root_id: string | null;
	body: string | null;
}

export interface ChatMessagesPage {
	messages: ChatMessageDto[];
	next_before: string | null;
}

export function getChatStatus(): Promise<ChatStatus> {
	return apiClient.get<ChatStatus>('/applications/chat/status');
}

export function getChatChannels(): Promise<ChatChannelInfo[]> {
	return apiClient.get<ChatChannelInfo[]>('/applications/chat/channels');
}

export function getChatMessages(
	channelId: string,
	before?: string | null
): Promise<ChatMessagesPage> {
	const params = new URLSearchParams({ channel_id: channelId });
	if (before) params.set('before', before);
	return apiClient.get<ChatMessagesPage>(`/applications/chat/messages?${params.toString()}`);
}

export function getChatMessage(messageId: string): Promise<ChatMessageDto> {
	return apiClient.get<ChatMessageDto>(`/applications/chat/messages/${encodeURIComponent(messageId)}`);
}

/** Canonical message ResourceRef for the UI's citation/deep-link handling. */
export function chatMessageRef(messageId: string): string {
	return `elembra://io.elembra.chat/message/${messageId}`;
}

/** Parse a message id out of a chat message ResourceRef URI. */
export function chatMessageIdFromRef(resourceRef: string): string | null {
	const prefix = 'elembra://io.elembra.chat/message/';
	if (!resourceRef.startsWith(prefix)) return null;
	const id = resourceRef.slice(prefix.length).split('?')[0];
	return id.length > 0 ? id : null;
}
```

- [ ] **Step 2: Write the tests (mirror `src/lib/api/ask.test.ts` — mock `globalThis.fetch`, assert method/path/body)**

```ts
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { getChatMessages, getChatMessage, chatMessageRef, chatMessageIdFromRef } from './chat';

const API_URL = 'http://localhost:8080/api/v1';

function mockFetch(response: unknown, status = 200): ReturnType<typeof vi.fn> {
	const fn = vi.fn(async () => ({
		ok: status >= 200 && status < 300,
		status,
		json: async () => response
	}));
	globalThis.fetch = fn as unknown as typeof fetch;
	return fn;
}

beforeEach(() => {
	sessionStorage.clear();
});

describe('getChatMessages', () => {
	it('calls the messages endpoint with the channel id', async () => {
		const fn = mockFetch({ messages: [], next_before: null });
		await getChatMessages('general');
		expect(fn).toHaveBeenCalledWith(
			expect.stringContaining(`${API_URL}/applications/chat/messages?channel_id=general`),
			expect.objectContaining({ method: 'GET' })
		);
	});

	it('passes the before watermark when present', async () => {
		const fn = mockFetch({ messages: [], next_before: null });
		await getChatMessages('general', '2026-08-12T10:00:00Z');
		const url = fn.mock.calls[0][0] as string;
		expect(url).toContain('before=2026-08-12T10%3A00%3A00Z');
	});
});

describe('getChatMessage', () => {
	it('calls the single-message endpoint', async () => {
		const fn = mockFetch({ message_id: 'abc', body: null });
		await getChatMessage('abc');
		expect(fn).toHaveBeenCalledWith(
			expect.stringContaining(`${API_URL}/applications/chat/messages/abc`),
			expect.objectContaining({ method: 'GET' })
		);
	});
});

describe('chatMessageRef helpers', () => {
	it('builds and parses the canonical ref', () => {
		const ref = chatMessageRef('deadbeef');
		expect(ref).toBe('elembra://io.elembra.chat/message/deadbeef');
		expect(chatMessageIdFromRef(ref)).toBe('deadbeef');
	});

	it('returns null for non-chat refs', () => {
		expect(chatMessageIdFromRef('elembra://io.elembra.files/file/x')).toBeNull();
	});
});
```

- [ ] **Step 3: Run the tests**

Run: `cd frontend && npx vitest run src/lib/api/chat.test.ts`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/api/chat.ts frontend/src/lib/api/chat.test.ts
git commit -s -m "feat(chat): frontend chat API client"
```

### Task 12: Nostr signing + publish client

**Files:**
- Create: `frontend/src/lib/chat/nostr.ts`
- Test: `frontend/src/lib/chat/nostr.test.ts`

- [ ] **Step 1: Write the module**

```ts
// Minimal Nostr client for the Chat application: key generation, kind-1/kind-22242
// signing (BIP-340 Schnorr), and NIP-42 relay publish. The private key never
// leaves the browser; the backend never sees it.
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/curves/abstract/utils';

export const NOSTR_KIND_TEXT = 1;
export const NOSTR_KIND_AUTH = 22242;

export type NostrTag = string[];

export interface NostrEvent {
	id: string;
	pubkey: string;
	created_at: number;
	kind: number;
	tags: NostrTag[];
	content: string;
	sig: string;
}

export function generateSecretKey(): string {
	const bytes = crypto.getRandomValues(new Uint8Array(32));
	return bytesToHex(bytes);
}

export function publicKeyOf(secretKey: string): string {
	return bytesToHex(schnorr.getPublicKey(hexToBytes(secretKey)));
}

function serializeForId(event: Omit<NostrEvent, 'id' | 'sig'>): string {
	return JSON.stringify([
		0,
		event.pubkey,
		event.created_at,
		event.kind,
		event.tags,
		event.content
	]);
}

async function sha256Hex(input: string): Promise<string> {
	const bytes = new TextEncoder().encode(input);
	const digest = await crypto.subtle.digest('SHA-256', bytes);
	return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

export async function buildUnsignedEvent(
	kind: number,
	content: string,
	tags: NostrTag[],
	pubkey: string
): Promise<Omit<NostrEvent, 'id' | 'sig'>> {
	return {
		pubkey,
		created_at: Math.floor(Date.now() / 1000),
		kind,
		tags,
		content
	};
}

export async function signEvent(
	unsigned: Omit<NostrEvent, 'id' | 'sig'>,
	secretKey: string
): Promise<NostrEvent> {
	const id = await sha256Hex(serializeForId(unsigned));
	const sig = bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(secretKey)));
	return { ...unsigned, id, sig };
}

/** Sign and publish one event over a NIP-42 relay session. Returns false on
 * any failure (timeout, rejected auth, relay error) — never throws. */
export async function publishEvent(
	relayUrl: string,
	unsigned: Omit<NostrEvent, 'id' | 'sig'>,
	secretKey: string
): Promise<boolean> {
	const signed = await signEvent(unsigned, secretKey);
	return await new Promise<boolean>((resolve) => {
		let settled = false;
		const finish = (ok: boolean) => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			socket.close();
			resolve(ok);
		};
		const timer = setTimeout(() => finish(false), 10_000);
		const socket = new WebSocket(relayUrl);

		socket.onopen = () => {
			// Ask for the AUTH challenge by sending an empty subscription.
			socket.send(JSON.stringify(['REQ', 'auth-probe', { limit: 0 }]));
		};
		socket.onmessage = async (raw) => {
			let message: unknown;
			try {
				message = JSON.parse(String(raw.data));
			} catch {
				return;
			}
			if (!Array.isArray(message)) return;
			if (message[0] === 'AUTH' && typeof message[1] === 'string') {
				const auth = await signEvent(
					await buildUnsignedEvent(
						NOSTR_KIND_AUTH,
						'',
						[
							['relay', relayUrl],
							['challenge', message[1]]
						],
						unsigned.pubkey
					),
					secretKey
				);
				socket.send(JSON.stringify(['AUTH', auth]));
			}
			if (message[0] === 'OK' && message[1] === signed.id) {
				finish(message[2] === true);
			}
		};
		socket.onerror = () => finish(false);
		socket.onclose = () => finish(false);
	});
}

/** Derive the ws:// URL for a relay_url (ws/wss pass through unchanged). */
export function relayWebSocketUrl(relayUrl: string): string {
	return relayUrl;
}
```

- [ ] **Step 2: Write the tests**

```ts
import { describe, expect, it } from 'vitest';
import { schnorr } from '@noble/curves/secp256k1';
import { hexToBytes, bytesToHex } from '@noble/curves/abstract/utils';
import {
	generateSecretKey,
	publicKeyOf,
	signEvent,
	buildUnsignedEvent,
	NOSTR_KIND_TEXT
} from './nostr';

describe('nostr signing', () => {
	it('generates a 32-byte key and its x-only pubkey', () => {
		const sk = generateSecretKey();
		expect(hexToBytes(sk)).toHaveLength(32);
		const pk = publicKeyOf(sk);
		expect(hexToBytes(pk)).toHaveLength(32);
	});

	it('produces a verifiable BIP-340 signature over the event id', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_TEXT, 'hello', [], pk);
		const signed = await signEvent(unsigned, sk);
		expect(schnorr.verify(hexToBytes(signed.id), hexToBytes(pk), hexToBytes(signed.sig))).toBe(
			true
		);
	});

	it('is deterministic: same input gives the same event id and signature', async () => {
		const sk = generateSecretKey();
		const pk = publicKeyOf(sk);
		const unsigned = await buildUnsignedEvent(NOSTR_KIND_TEXT, 'hello', [], pk);
		const a = await signEvent(unsigned, sk);
		const b = await signEvent(unsigned, sk);
		expect(a.id).toBe(b.id);
		expect(a.sig).toBe(b.sig);
		expect(bytesToHex(schnorr.getPublicKey(hexToBytes(sk)))).toBe(pk);
	});
});
```

- [ ] **Step 3: Run the tests**

Run: `cd frontend && npx vitest run src/lib/chat/nostr.test.ts`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/chat/nostr.ts frontend/src/lib/chat/nostr.test.ts
git commit -s -m "feat(chat): browser Nostr signing and NIP-42 publish client"
```

### Task 13: Browser key custody

**Files:**
- Create: `frontend/src/lib/chat/keys.ts`
- Test: `frontend/src/lib/chat/keys.test.ts`

- [ ] **Step 1: Write the module**

```ts
// Client-held Buzz key custody: passphrase-encrypted at rest (WebCrypto
// PBKDF2 + AES-GCM). The raw key never leaves the browser; export/import is
// the only recovery path (ADR-0034: no silent server custody).
import { bytesToHex, hexToBytes } from '@noble/curves/abstract/utils';

const STORAGE_KEY = 'elembra.chat.key.v1';
const PBKDF2_ITERATIONS = 100_000;

export interface EncryptedChatKey {
	v: 1;
	salt: string; // hex
	iv: string; // hex
	ciphertext: string; // hex
}

export function hasChatKey(): boolean {
	return localStorage.getItem(STORAGE_KEY) !== null;
}

export function storedKeyPubkey(): string | null {
	try {
		const raw = localStorage.getItem(STORAGE_KEY);
		if (!raw) return null;
		const envelope: { pubkey?: string } = JSON.parse(raw);
		return envelope.pubkey ?? null;
	} catch {
		return null;
	}
}

async function deriveKey(passphrase: string, salt: Uint8Array): Promise<CryptoKey> {
	const material = await crypto.subtle.importKey(
		'raw',
		new TextEncoder().encode(passphrase),
		'PBKDF2',
		false,
		['deriveKey']
	);
	return crypto.subtle.deriveKey(
		{ name: 'PBKDF2', salt, iterations: PBKDF2_ITERATIONS, hash: 'SHA-256' },
		material,
		{ name: 'AES-GCM', length: 256 },
		false,
		['encrypt', 'decrypt']
	);
}

export async function saveChatKey(
	secretKey: string,
	pubkey: string,
	passphrase: string
): Promise<void> {
	const salt = crypto.getRandomValues(new Uint8Array(16));
	const iv = crypto.getRandomValues(new Uint8Array(12));
	const key = await deriveKey(passphrase, salt);
	const ciphertext = await crypto.subtle.encrypt(
		{ name: 'AES-GCM', iv },
		key,
		hexToBytes(secretKey)
	);
	const envelope: EncryptedChatKey = {
		v: 1,
		salt: bytesToHex(salt),
		iv: bytesToHex(iv),
		ciphertext: bytesToHex(new Uint8Array(ciphertext))
	};
	localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...envelope, pubkey }));
}

export async function loadChatKey(passphrase: string): Promise<string> {
	const raw = localStorage.getItem(STORAGE_KEY);
	if (!raw) throw new Error('no stored chat key');
	const envelope = JSON.parse(raw) as EncryptedChatKey;
	if (envelope.v !== 1) throw new Error('unsupported chat key format');
	const key = await deriveKey(passphrase, hexToBytes(envelope.salt));
	const plaintext = await crypto.subtle.decrypt(
		{ name: 'AES-GCM', iv: hexToBytes(envelope.iv) },
		key,
		hexToBytes(envelope.ciphertext)
	);
	return bytesToHex(new Uint8Array(plaintext));
}

export function exportChatKey(): string {
	const raw = localStorage.getItem(STORAGE_KEY);
	if (!raw) throw new Error('no stored chat key');
	return raw;
}

export async function importChatKey(json: string, passphrase: string): Promise<string> {
	const parsed = JSON.parse(json) as EncryptedChatKey & { secret_key?: string; pubkey?: string };
	if (parsed.v !== 1) throw new Error('unsupported chat key format');
	if (typeof parsed.secret_key !== 'string') throw new Error('backup does not contain a key');
	const pubkey = parsed.pubkey ?? storedKeyPubkey() ?? '';
	await saveChatKey(parsed.secret_key, pubkey, passphrase);
	return parsed.secret_key;
}

export function clearChatKey(): void {
	localStorage.removeItem(STORAGE_KEY);
}
```

- [ ] **Step 2: Write the tests**

```ts
import { describe, expect, it, beforeEach } from 'vitest';
import {
	saveChatKey,
	loadChatKey,
	hasChatKey,
	clearChatKey,
	exportChatKey,
	importChatKey,
	storedKeyPubkey
} from './keys';
import { generateSecretKey, publicKeyOf } from './nostr';

beforeEach(() => {
	localStorage.clear();
});

describe('chat key custody', () => {
	it('round-trips the key with the correct passphrase', async () => {
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'correct horse');
		expect(hasChatKey()).toBe(true);
		expect(storedKeyPubkey()).toBe(publicKeyOf(sk));
		await expect(loadChatKey('correct horse')).resolves.toBe(sk);
	});

	it('rejects the wrong passphrase', async () => {
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'correct horse');
		await expect(loadChatKey('wrong')).rejects.toThrow();
	});

	it('rejects tampered ciphertext', async () => {
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'correct horse');
		const raw = localStorage.getItem('elembra.chat.key.v1')!;
		const parsed = JSON.parse(raw);
		parsed.ciphertext = '00' + parsed.ciphertext.slice(2);
		localStorage.setItem('elembra.chat.key.v1', JSON.stringify(parsed));
		await expect(loadChatKey('correct horse')).rejects.toThrow();
	});

	it('imports a backup and clears on demand', async () => {
		const sk = generateSecretKey();
		await saveChatKey(sk, publicKeyOf(sk), 'pass');
		const backup = exportChatKey();
		clearChatKey();
		expect(hasChatKey()).toBe(false);
		await expect(importChatKey(backup, 'pass')).resolves.toBe(sk);
		await expect(loadChatKey('pass')).resolves.toBe(sk);
	});
});
```

- [ ] **Step 3: Run the tests**

Run: `cd frontend && npx vitest run src/lib/chat/keys.test.ts`
Expected: all pass (happy-dom provides `crypto.subtle` via Node's webcrypto in vitest; if `crypto` is undefined in the test env, add `globalThis.crypto ??= require('node:crypto').webcrypto;` to `src/test-setup.ts`).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/chat/keys.ts frontend/src/lib/chat/keys.test.ts
git commit -s -m "feat(chat): client-held passphrase-encrypted key custody"
```

### Task 14: App-shell wiring (renderer, icon, object href)

**Files:**
- Modify: `frontend/src/routes/(app)/apps/[key]/ApplicationPageRenderer.svelte`
- Modify: `frontend/src/lib/applications/iconRegistry.ts`
- Modify: `frontend/src/lib/applications/applicationPages.ts`

- [ ] **Step 1: Renderer entry**

In `ApplicationPageRenderer.svelte`, add the import and map entry:

```ts
	import ChatApplicationView from '$lib/components/chat/ChatApplicationView.svelte';
```

```ts
	const rendererMap: Record<string, any> = {
		notes: NotesApplicationView,
		'okf-note': NotesApplicationView,
		meetings: MeetingsApplicationView,
		standups: StandupsApplicationView,
		kanban: KanbanApplicationView,
		decisions: DecisionsApplicationView,
		shares: SharesApplicationView,
		brainstorming: BrainstormingApplicationView,
		'mail-list': MailApplicationView,
		chat: ChatApplicationView
	};
```

- [ ] **Step 2: Icon**

In `iconRegistry.ts`, add `'message-circle'` to `APPROVED_MODULE_ICONS` (after `'mail'`). Check `ApplicationIcon.svelte`'s `iconMap` (lines ~33-53) — if it maps icon names to lucide components and lacks `message-circle`, add `'message-circle': MessageCircle` with the corresponding `lucide-svelte` import.

- [ ] **Step 3: Object href**

In `applicationPages.ts`, add to `applicationRouteMap`:

```ts
		'io.elembra.chat': `/apps/chat?message=${objectId}`
```

- [ ] **Step 4: Check + tests**

Run: `cd frontend && npm run check && npx vitest run src/lib/applications/`
Expected: check clean; registry/icon tests pass (add a small expectation to `registry.test.ts` if it asserts the approved-icon list exhaustively).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/routes/\(app\)/apps/\[key\]/ApplicationPageRenderer.svelte frontend/src/lib/applications/
git commit -s -m "feat(chat): register the Chat application renderer, icon and deep link"
```

### Task 15: Live updates (websocket + polling)

**Files:**
- Modify: `frontend/src/lib/websocket/events.ts`
- Modify: `frontend/src/lib/websocket/manager.ts`

- [ ] **Step 1: Event type**

In `events.ts`, add `| 'ChatMessageObserved'` to the `WebSocketEventType` union.

- [ ] **Step 2: Handler**

In `manager.ts`, register in `registerEventHandlers`:

```ts
	wsClient.on('ChatMessageObserved', handleChatMessageObserved);
```

and add (the generic event envelope carries no payload, so invalidation is prefix-wide — the chat view refetches the active channel):

```ts
function handleChatMessageObserved(): void {
	queryClient.invalidateQueries({ queryKey: ['chat-messages'] });
	queryClient.invalidateQueries({ queryKey: ['chat-channels'] });
}
```

- [ ] **Step 3: Polling fallback lives in the view (Task 16), not the manager.**

- [ ] **Step 4: Check**

Run: `cd frontend && npm run check`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/websocket/events.ts frontend/src/lib/websocket/manager.ts
git commit -s -m "feat(chat): invalidate chat queries on observed messages"
```

### Task 16: Chat application view (channels, timeline, composer, states)

**Files:**
- Create: `frontend/src/lib/components/chat/ChatApplicationView.svelte`
- Create: `frontend/src/lib/components/chat/ChannelList.svelte`
- Create: `frontend/src/lib/components/chat/MessageTimeline.svelte`
- Create: `frontend/src/lib/components/chat/MessageComposer.svelte`
- Create: `frontend/src/lib/components/chat/BindingPanel.svelte`
- Test: `frontend/src/lib/components/chat/ChatApplicationView.test.ts`

- [ ] **Step 1: `ChatApplicationView.svelte`**

```svelte
<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { createQuery } from '$lib/query-compat';
	import { getChatStatus, getChatChannels, getChatMessages, getChatMessage, type ChatMessageDto } from '$lib/api/chat';
	import BindingPanel from './BindingPanel.svelte';
	import ChannelList from './ChannelList.svelte';
	import MessageTimeline from './MessageTimeline.svelte';
	import MessageComposer from './MessageComposer.svelte';

	let { module }: { module: { key: string } } = $props();

	const statusQuery = createQuery({
		queryKey: ['chat-status'],
		queryFn: () => getChatStatus(),
		staleTime: 30_000
	});
	const channelsQuery = createQuery({
		queryKey: ['chat-channels'],
		queryFn: () => getChatChannels(),
		enabled: $statusQuery.data?.mapping != null && $statusQuery.data?.binding != null
	});

	let selectedChannelId = $state<string | null>(null);
	let focusedMessageId = $state<string | null>(null);

	// Deep links: /apps/chat?channel=<id>&message=<id>
	$effect(() => {
		const params = $page.url.searchParams;
		const channel = params.get('channel');
		const message = params.get('message');
		if (channel) selectedChannelId = channel;
		if (message) focusedMessageId = message;
	});

	$effect(() => {
		const channels = $channelsQuery.data;
		if (!channels || channels.length === 0) return;
		if (selectedChannelId && channels.some((c) => c.channel_id === selectedChannelId)) return;
		selectedChannelId = channels[0].channel_id;
	});

	// A citation deep link names only the message; once it resolves, switch to
	// the channel that owns it so focus/scroll lands in the right timeline.
	$effect(() => {
		const message = $focusedMessageQuery.data;
		if (message && message.channel_id !== selectedChannelId) {
			selectedChannelId = message.channel_id;
		}
	});

	const messagesQuery = createQuery({
		queryKey: ['chat-messages', selectedChannelId],
		queryFn: () => getChatMessages(selectedChannelId!),
		enabled: selectedChannelId != null
	});

	const focusedMessageQuery = createQuery({
		queryKey: ['chat-message', focusedMessageId],
		queryFn: () => getChatMessage(focusedMessageId!),
		enabled: focusedMessageId != null
	});

	// Polling fallback: 15 s while mounted, regardless of websocket state.
	onMount(() => {
		const interval = setInterval(() => {
			if (selectedChannelId) messagesQuery.refetch();
		}, 15_000);
		return () => clearInterval(interval);
	});

	function handleSendFailure(message: string): void {
		relayError = message;
	}

	let relayError = $state('');

	const status = $derived($statusQuery.data);
	const bindingActive = $derived(status?.binding != null && status.binding.status === 'Active');
	const focusTarget: ChatMessageDto | null = $derived(
		$focusedMessageQuery.data && $focusedMessageQuery.data.channel_id === selectedChannelId
			? $focusedMessageQuery.data
			: null
	);
</script>

{#if $statusQuery.isLoading}
	<div class="p-6 text-base-content/60">Loading Chat…</div>
{:else if !status || !status.chat_enabled}
	<div class="p-6 text-base-content/60">Chat is not enabled for this workspace.</div>
{:else if !status.mapping}
	<div class="p-6 text-base-content/60">
		No Buzz community is mapped for this workspace yet. An administrator can configure it.
	</div>
{:else if !bindingActive}
	<BindingPanel onBound={() => { statusQuery.refetch(); channelsQuery.refetch(); }} />
{:else if !status.admission_active}
	<div class="p-6 text-base-content/60">
		Your Chat admission is still being processed by the community relay.
	</div>
{:else}
	<div class="flex h-full">
		<ChannelList
			channels={$channelsQuery.data ?? []}
			loading={$channelsQuery.isLoading}
			selectedChannelId={selectedChannelId}
			onSelect={(id: string) => {
				selectedChannelId = id;
				focusedMessageId = null;
			}}
		/>
		<div class="flex min-w-0 flex-1 flex-col">
			<MessageTimeline
				messages={$messagesQuery.data?.messages ?? []}
				loading={$messagesQuery.isLoading}
				focusTarget={focusTarget}
				onLoadMore={() => {
					const before = $messagesQuery.data?.next_before;
					if (before) messagesQuery.refetch();
				}}
			/>
			<MessageComposer
				relayUrl={status.mapping.relay_url}
				channelId={selectedChannelId ?? ''}
				onSendFailure={handleSendFailure}
			/>
			{#if relayError}
				<div class="px-4 py-2 text-sm text-error">
					Relay unreachable — message not sent. Reads are unaffected.
				</div>
			{/if}
		</div>
	</div>
{/if}
```

- [ ] **Step 2: `ChannelList.svelte`**

```svelte
<script lang="ts">
	import type { ChatChannelInfo } from '$lib/api/chat';

	interface Props {
		channels: ChatChannelInfo[];
		loading: boolean;
		selectedChannelId: string | null;
		onSelect: (channelId: string) => void;
	}

	let { channels, loading, selectedChannelId, onSelect }: Props = $props();
</script>

<aside class="w-56 shrink-0 border-r border-base-300 p-2">
	{#if loading}
		<div class="text-sm text-base-content/60">Loading channels…</div>
	{:else if channels.length === 0}
		<div class="text-sm text-base-content/60">No channels yet.</div>
	{:else}
		<ul>
			{#each channels as channel (channel.channel_id)}
				<li>
					<button
						type="button"
						class="w-full rounded px-2 py-1 text-left text-sm {channel.channel_id ===
						selectedChannelId
							? 'bg-base-200 font-medium'
							: 'hover:bg-base-200/60'}"
						onclick={() => onSelect(channel.channel_id)}
					>
						# {channel.channel_id}
					</button>
				</li>
			{/each}
		</ul>
	{/if}
</aside>
```

- [ ] **Step 3: `MessageTimeline.svelte`**

```svelte
<script lang="ts">
	import type { ChatMessageDto } from '$lib/api/chat';

	interface Props {
		messages: ChatMessageDto[];
		loading: boolean;
		focusTarget: ChatMessageDto | null;
		onLoadMore: () => void;
	}

	let { messages, loading, focusTarget, onLoadMore }: Props = $props();

	let focusEl = $state<HTMLDivElement | null>(null);
	$effect(() => {
		if (focusTarget) focusEl?.scrollIntoView({ behavior: 'smooth', block: 'center' });
	});
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-y-auto p-4">
	{#if loading && messages.length === 0}
		<div class="text-sm text-base-content/60">Loading messages…</div>
	{:else if messages.length === 0}
		<div class="text-sm text-base-content/60">No messages yet — say hello.</div>
	{:else}
		<button type="button" class="mb-2 text-sm text-primary" onclick={onLoadMore}>
			Load earlier
		</button>
		{#each messages as message (message.event_id)}
			<div
				class="mb-3 {message.thread_root_id ? 'ml-6' : ''}"
				bind:this={focusTarget?.message_id === message.message_id ? focusEl : null}
			>
				<div class="text-xs text-base-content/50">
					{message.author_pubkey.slice(0, 8)}… · {message.event_created_at}
					{message.thread_root_id ? ' · reply' : ''}
				</div>
				{#if message.body != null}
					<div class="whitespace-pre-wrap text-sm">{message.body}</div>
				{:else}
					<div class="text-sm text-base-content/50 italic">
						Content unavailable in Elembra (reference-only message).
					</div>
				{/if}
				<a class="text-xs text-primary" href="/ask?scope=chat&communityId={message.community_id}&channelId={message.channel_id}">Ask</a>
			</div>
		{/each}
	{/if}
</div>
```

- [ ] **Step 4: `MessageComposer.svelte`**

```svelte
<script lang="ts">
	import { createQuery } from '$lib/query-compat';
	import { getChatStatus } from '$lib/api/chat';
	import { buildUnsignedEvent, publishEvent, publicKeyOf, NOSTR_KIND_TEXT, type NostrTag } from '$lib/chat/nostr';
	import { hasChatKey, loadChatKey } from '$lib/chat/keys';
	import AttachmentPicker from './AttachmentPicker.svelte';

	interface Props {
		relayUrl: string;
		channelId: string;
		onSendFailure: (message: string) => void;
	}

	let { relayUrl, channelId, onSendFailure }: Props = $props();

	let draft = $state('');
	let sending = $state(false);
	let passphrase = $state('');
	let needsPassphrase = $state(false);
	let attachmentTag = $state<NostrTag | null>(null);

	async function send(): Promise<void> {
		const content = draft.trim();
		if (!content && !attachmentTag) return;
		const status = $statusQuery.data;
		if (!status?.binding) return;
		const secretKey = await unlockKey();
		if (!secretKey) return;
		if (publicKeyOf(secretKey) !== status.binding.buzz_pubkey) {
			onSendFailure('local key does not match your bound Buzz identity');
			return;
		}
		const tags: NostrTag[] = [];
		if (attachmentTag) tags.push(attachmentTag);
		sending = true;
		const ok = await publishEvent(
			relayUrl,
			await buildUnsignedEvent(NOSTR_KIND_TEXT, content, tags, status.binding.buzz_pubkey),
			secretKey
		);
		sending = false;
		if (ok) {
			draft = '';
			attachmentTag = null;
			onSendFailure('');
		} else {
			onSendFailure('relay unreachable');
		}
	}

	async function unlockKey(): Promise<string | null> {
		if (!hasChatKey()) {
			onSendFailure('no local chat key — bind your identity first');
			return null;
		}
		try {
			return await loadChatKey(passphrase || '');
		} catch {
			needsPassphrase = true;
			return null;
		}
	}

	const statusQuery = createQuery({
		queryKey: ['chat-status'],
		queryFn: () => getChatStatus(),
		staleTime: 30_000
	});
</script>

<div class="border-t border-base-300 p-3">
	{#if attachmentTag}
		<div class="mb-1 text-xs text-base-content/60">
			Attachment: {attachmentTag[1]}
			<button type="button" class="ml-2 text-error" onclick={() => (attachmentTag = null)}>
				remove
			</button>
		</div>
	{/if}
	{#if needsPassphrase}
		<div class="mb-1 flex gap-2">
			<input
				type="password"
				class="input input-sm"
				placeholder="key passphrase"
				bind:value={passphrase}
			/>
			<button
				type="button"
				class="btn btn-sm"
				onclick={async () => {
					needsPassphrase = false;
					await send();
				}}
			>
				unlock
			</button>
		</div>
	{/if}
	<div class="flex items-end gap-2">
		<AttachmentPicker onSelect={(tag) => (attachmentTag = tag)} />
		<textarea
			class="textarea textarea-sm min-h-0 flex-1"
			rows={2}
			placeholder="Message #{channelId}"
			bind:value={draft}
			onkeydown={(e) => {
				if (e.key === 'Enter' && !e.shiftKey) {
					e.preventDefault();
					send();
				}
			}}
		></textarea>
		<button type="button" class="btn btn-sm btn-primary" disabled={sending} onclick={send}>
			{sending ? 'Sending…' : 'Send'}
		</button>
	</div>
</div>
```

- [ ] **Step 5: `BindingPanel.svelte`** — key setup + challenge/verify against the existing endpoints

```svelte
<script lang="ts">
	import { apiClient } from '$lib/api/client';
	import { generateSecretKey, publicKeyOf, signEvent, buildUnsignedEvent, NOSTR_KIND_AUTH } from '$lib/chat/nostr';
	import { saveChatKey, hasChatKey, loadChatKey, exportChatKey } from '$lib/chat/keys';

	interface Props {
		onBound: () => void;
	}

	let { onBound }: Props = $props();

	let busy = $state(false);
	let error = $state('');
	let notice = $state('');
	let passphrase = $state('');

	async function bind(): Promise<void> {
		if (!passphrase) {
			error = 'choose a passphrase to encrypt your key';
			return;
		}
		busy = true;
		error = '';
		try {
			const secretKey = hasChatKey() ? await loadChatKey(passphrase) : generateSecretKey();
			const pubkey = publicKeyOf(secretKey);
			if (!hasChatKey()) {
				await saveChatKey(secretKey, pubkey, passphrase);
			}
			const challenge: {
				challenge_id: string;
				nonce: string;
				buzz_pubkey: string;
				relay_url: string;
				expires_at: string;
			} = await apiClient.post('/applications/chat/identity-binding/challenge', {
				workspace_id: workspaceId(),
				buzz_pubkey: pubkey
			});
			const auth = await signEvent(
				await buildUnsignedEvent(
					NOSTR_KIND_AUTH,
					'',
					[
						['relay', challenge.relay_url],
						['challenge', challenge.nonce]
					],
					pubkey
				),
				secretKey
			);
			await apiClient.post('/applications/chat/identity-binding/verify', {
				challenge_id: challenge.challenge_id,
				event: auth
			});
			await apiClient.post('/applications/chat/admission', {
				workspace_id: workspaceId()
			});
			notice = 'Bound and admission queued.';
			onBound();
		} catch (err) {
			error = err instanceof Error ? err.message : 'binding failed';
		} finally {
			busy = false;
		}
	}

	function workspaceId(): string {
		// Workspace id == tenant id in this deployment (see backend handlers,
		// e.g. PrincipalContext::user(.., WorkspaceId(auth.tenant_id))).
		const user = sessionStorage.getItem('currentUser');
		if (user) {
			try {
				const parsed = JSON.parse(user);
				if (typeof parsed?.tenant_id === 'string') return parsed.tenant_id;
			} catch {
				// fall through
			}
		}
		throw new Error('workspace id unavailable');
	}
</script>

<div class="p-6">
	<h2 class="mb-2 text-lg font-semibold">Set up Chat</h2>
	<p class="mb-4 text-sm text-base-content/60">
		Chat messages are signed with a key held only in this browser. Choose a passphrase to
		encrypt it. Export it after setup — without a backup, another device cannot use the same
		identity.
	</p>
	<div class="mb-2 flex gap-2">
		<input
			type="password"
			class="input input-sm"
			placeholder="key passphrase"
			bind:value={passphrase}
		/>
		<button type="button" class="btn btn-sm btn-primary" disabled={busy} onclick={bind}>
			{busy ? 'Binding…' : 'Generate key & bind'}
		</button>
	</div>
	{#if hasChatKey()}
		<button
			type="button"
			class="btn btn-sm"
			onclick={() => {
				const backup = exportChatKey();
				navigator.clipboard.writeText(backup);
				notice = 'Backup copied to clipboard.';
			}}
		>
			Export key backup
		</button>
	{/if}
	{#if notice}<p class="mt-2 text-sm text-success">{notice}</p>{/if}
	{#if error}<p class="mt-2 text-sm text-error">{error}</p>{/if}
</div>
```

Note: the challenge/verify/admission bodies above match the verified wire contract in `backend/server/src/handlers/chat_identity.rs` (`ChallengeRequest { workspace_id, buzz_pubkey }`, `VerifyRequest { challenge_id, event }`, `AdmissionRequest { workspace_id }`; `ChallengeResponse { challenge_id, nonce, buzz_pubkey, relay_url, expires_at }`). The workspace id equals the tenant id in this deployment.

- [ ] **Step 6: Component tests (view-level, mocking the API module)**

`ChatApplicationView.test.ts` — mock `$lib/api/chat` via `vi.mock`, render with `@testing-library/svelte` (existing pattern in `AskExperience.test.ts`):

1. disabled chat shows "Chat is not enabled for this workspace."
2. unmapped workspace shows the mapping notice.
3. unbound user renders the BindingPanel heading "Set up Chat".
4. bound+admitted user with channels renders channel names from the mock.
5. deep-link `message` param triggers a `getChatMessage` call (mock `$app/stores` page).

Run: `cd frontend && npx vitest run src/lib/components/chat/`
Expected: all pass.

- [ ] **Step 7: Full frontend check**

Run: `cd frontend && npm run check && npm run lint && npm run test`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/lib/components/chat/
git commit -s -m "feat(chat): Chat application view, timeline, composer and binding UI"
```

---

## Slice 3 — Attachments + Ask Channel wiring

### Task 17: Attachment picker (Elembra Files, `elembra-ref` tag)

**Files:**
- Create: `frontend/src/lib/components/chat/AttachmentPicker.svelte`
- Test: `frontend/src/lib/components/chat/AttachmentPicker.test.ts`

- [ ] **Step 1: The prepare wire shape (verified)**

`ResourceRequest { resource: ResourceRef }` deserializes the struct form: `ResourceRef` is
`#[derive(Deserialize)]` with fields `application` (plain string), `resourceType`, `resourceId`
(serde renames, `backend/crates/resource-auth/src/resource_ref.rs:76-94`). The client therefore
posts `{ "resource": { "application": "io.elembra.files", "resourceType": "file", "resourceId": "<id>" } }`.
Use that exact shape in Step 2.

- [ ] **Step 2: Write the picker**

```svelte
<script lang="ts">
	import { onMount } from 'svelte';
	import { listAllFiles, type File } from '$lib/api/files';
	import { apiClient } from '$lib/api/client';
	import type { NostrTag } from '$lib/chat/nostr';

	interface Props {
		onSelect: (tag: NostrTag) => void;
	}

	let { onSelect }: Props = $props();

	let open = $state(false);
	let files = $state<File[]>([]);
	let loading = $state(false);
	let error = $state('');

	async function loadFiles(): Promise<void> {
		loading = true;
		error = '';
		try {
			files = await listAllFiles();
		} catch {
			error = 'Could not list files.';
		} finally {
			loading = false;
		}
	}

	onMount(loadFiles);

	async function pick(file: File): Promise<void> {
		try {
			const response = await apiClient.post<{ buzz_tag: NostrTag }>(
				'/applications/chat/attachments/prepare',
				{
					resource: {
						application: 'io.elembra.files',
						resourceType: 'file',
						resourceId: file.id
					}
				}
			);
			onSelect(response.buzz_tag);
			open = false;
		} catch (err) {
			error = err instanceof Error ? err.message : 'Attachment unavailable.';
		}
	}
</script>

<button type="button" class="btn btn-sm" onclick={() => (open = true)}>Attach file</button>

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onclick={() => (open = false)}>
		<div class="max-h-[70vh] w-96 overflow-y-auto rounded bg-base-100 p-4" onclick={(e) => e.stopPropagation()}>
			<h3 class="mb-2 font-semibold">Attach a file</h3>
			{#if loading}
				<div class="text-sm text-base-content/60">Loading files…</div>
			{:else if files.length === 0}
				<div class="text-sm text-base-content/60">No files yet.</div>
			{:else}
				<ul>
					{#each files as file (file.id)}
						<li>
							<button
								type="button"
								class="w-full rounded px-2 py-1 text-left text-sm hover:bg-base-200"
								onclick={() => pick(file)}
							>
								{file.name}
							</button>
						</li>
					{/each}
				</ul>
			{/if}
			{#if error}<p class="mt-2 text-sm text-error">{error}</p>{/if}
			<button type="button" class="btn btn-sm mt-2" onclick={() => (open = false)}>Cancel</button>
		</div>
	</div>
{/if}
```

(If the `File` type's id field has a different name in `src/lib/api/files.ts`, use the actual field; the URI format `elembra://io.elembra.files/file/<id>` is the canonical ref per ADR-0034.)

- [ ] **Step 3: Test**

`AttachmentPicker.test.ts` (mock `$lib/api/files` and `$lib/api/client`): selecting a file calls `prepare` with the exact URI and emits the returned `buzz_tag`; a 404 from `prepare` shows the error text and does not emit. Follow the `AskExperience.test.ts` mocking style.

Run: `cd frontend && npx vitest run src/lib/components/chat/AttachmentPicker.test.ts`
Expected: pass.

- [ ] **Step 4: Render attachment refs in the timeline**

In `MessageComposer.svelte` the tag is already included in the signed event (Task 16). In `MessageTimeline.svelte`, render any `["elembra-ref", "<uri>"]` tag present on a message as a link that calls `POST /applications/chat/attachments/open` (via `apiClient.post('/applications/chat/attachments/open', { resource: uri })`) — open the resulting blob in a new tab. Extract the tag in the composer input side: keep a per-message `attachment_uri` in `ChatMessageDto`? No — the DTO has no tags. Add to the backend `ChatMessageDto` a `attachment_refs: Vec<String>` field, populated from the observed event's tags. Backend change (Task 7 file): the observation row does not store tags — check `ChatObservedEvent`; if tags are not retained, leave this out and note it as a limitation in the final report (the attachment still works for senders via the composer; recipients see the message body). Do NOT add schema columns in this PR.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/components/chat/AttachmentPicker.svelte frontend/src/lib/components/chat/AttachmentPicker.test.ts
git commit -s -m "feat(chat): Files attachment picker with elembra-ref tagging"
```

### Task 18: Ask-this-channel + citation deep link

**Files:**
- Modify: `frontend/src/routes/(app)/ask/+page.svelte`
- Modify: `frontend/src/lib/components/chat/ChatApplicationView.svelte`
- Test: `frontend/src/routes/(app)/ask/page.chat.test.ts` (new, if the existing ask page tests are absent)

- [ ] **Step 1: Citation navigation in the ask page**

In `routes/(app)/ask/+page.svelte`, the page already parses the chat scope. Pass `onChatCitationOpen` to `AskExperience`:

```ts
	import { chatMessageIdFromRef } from '$lib/api/chat';

	function handleChatCitationOpen(opened: OpenCitationResponse): void {
		const messageId = chatMessageIdFromRef(opened.resource_ref);
		if (!messageId) return;
		goto(`/apps/chat?message=${encodeURIComponent(messageId)}`);
	}
```

and in the component usage:

```svelte
<AskExperience {scope} {scopeLabel} {heading} {onChatCitationOpen} />
```

(`onChatCitationOpen` is the existing prop; only the `/ask` route supplies the navigation behavior. The channel id is resolved inside the Chat view from the fetched message — the deep link needs only `message`.)

- [ ] **Step 2: "Ask this channel" button in the chat view**

In `ChatApplicationView.svelte`, in the composer area header (or above the timeline), add when a channel is selected:

```svelte
	<a
		class="text-sm text-primary"
		href="/ask?scope=chat&communityId={status.mapping.community_id}&channelId={selectedChannelId}"
	>
		Ask this channel
	</a>
```

- [ ] **Step 3: Test the wiring**

Extend/replace with `routes/(app)/ask/page.chat.test.ts`: mock `AskExperience` and `$app/stores`; assert (a) the chat scope in the URL produces `scope={{ type: 'chatChannel', communityId, channelId }}`, and (b) invoking the passed `onChatCitationOpen` with `elembra://io.elembra.chat/message/abc` calls `goto('/apps/chat?message=abc')`.

Run: `cd frontend && npx vitest run src/routes/\(app\)/ask/`
Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/routes/\(app\)/ask/ frontend/src/lib/components/chat/ChatApplicationView.svelte
git commit -s -m "feat(chat): wire Ask-this-channel and citation deep links"
```

---

## Slice 4 — Real-relay proof, docs, final gate

### Task 19: Real Buzz relay publish/revoke probe

**Files:**
- Create: `frontend/scripts/chat-relay-probe.mjs`
- Create: `scripts/run-chat-e2e.sh`

- [ ] **Step 1: The Node probe (runs from `frontend/` so `@noble/curves` resolves)**

```js
// frontend/scripts/chat-relay-probe.mjs
// Real-relay proof helper: NIP-42 auth + signed kind-1 publish against any
// Buzz relay (Node 22+ has global WebSocket). Mirrors ADR-0034's live proof.
// Usage:
//   node scripts/chat-relay-probe.mjs <wss://relay> <secret-key-hex> <text>
// Exit 0 when the relay accepted the event, 1 otherwise.
import { schnorr } from '@noble/curves/secp256k1';
import { bytesToHex, hexToBytes } from '@noble/curves/abstract/utils';

const [, , relayUrl, secretKey, content] = process.argv;
if (!relayUrl || !secretKey || !content) {
  console.error('usage: chat-relay-probe.mjs <wss://relay> <secret-key-hex> <text>');
  process.exit(2);
}

const pubkey = bytesToHex(schnorr.getPublicKey(hexToBytes(secretKey)));
const sha256 = async (input) => {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(input));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
};
const sign = async (kind, tags, text) => {
  const event = { pubkey, created_at: Math.floor(Date.now() / 1000), kind, tags, content: text };
  const id = await sha256(JSON.stringify([0, event.pubkey, event.created_at, event.kind, event.tags, event.content]));
  const sig = bytesToHex(schnorr.sign(hexToBytes(id), hexToBytes(secretKey)));
  return { ...event, id, sig };
};

const signed = await sign(1, [], content);
const accepted = await new Promise((resolve) => {
  const socket = new WebSocket(relayUrl);
  const timer = setTimeout(() => { socket.close(); resolve(false); }, 10_000);
  socket.onopen = () => socket.send(JSON.stringify(['REQ', 'auth-probe', { limit: 0 }]));
  socket.onmessage = async (raw) => {
    const message = JSON.parse(String(raw.data));
    if (!Array.isArray(message)) return;
    if (message[0] === 'AUTH' && typeof message[1] === 'string') {
      const auth = await sign(22242, [['relay', relayUrl], ['challenge', message[1]]], '');
      socket.send(JSON.stringify(['AUTH', auth]));
      socket.send(JSON.stringify(['EVENT', signed]));
    }
    if (message[0] === 'OK' && message[1] === signed.id) {
      clearTimeout(timer);
      socket.close();
      resolve(message[2] === true);
    }
  };
  socket.onerror = () => { clearTimeout(timer); resolve(false); };
  socket.onclose = () => { clearTimeout(timer); resolve(false); };
});

console.log(accepted ? `OK published ${signed.id}` : 'FAILED: relay rejected or unreachable');
process.exit(accepted ? 0 : 1);
```

- [ ] **Step 2: The orchestration script**

```bash
#!/usr/bin/env bash
# scripts/run-chat-e2e.sh
# Real Buzz relay proof for the Chat v1 critical paths (publish + revocation
# denial). Operator-supplied disposable relay image, per the ADR-0034 recipe.
# Required env:
#   BUZZ_RELAY_IMAGE   docker image of the Buzz relay (e.g. ghcr.io/.../buzz-relay:main)
#   BUZZ_RELAY_WS      wss:// or ws:// URL of the started relay
#   BUZZ_SERVICE_SK    hex service/bridge key with relay admin authority
# Optional:
#   ELEMBRA_API        Elembra API base (default http://localhost:8080/api/v1)
#   ADMIN_EMAIL / ADMIN_PASSWORD   for the Elembra-side binding steps
set -euo pipefail

: "${BUZZ_RELAY_IMAGE:?set BUZZ_RELAY_IMAGE to the Buzz relay docker image}"
: "${BUZZ_RELAY_WS:?set BUZZ_RELAY_WS to the relay websocket URL}"
: "${BUZZ_SERVICE_SK:?set BUZZ_SERVICE_SK to a relay admin service secret key}"
ELEMBRA_API="${ELEMBRA_API:-http://localhost:8080/api/v1}"

echo "== 1. start disposable relay =="
docker run -d --rm --name rustshare-buzz-proof \
  -e BUZZ_REQUIRE_RELAY_MEMBERSHIP=true \
  -p "${BUZZ_RELAY_PORT:-7447}:7447" \
  "$BUZZ_RELAY_IMAGE" >/dev/null
trap 'docker stop rustshare-buzz-proof >/dev/null 2>&1 || true' EXIT

echo "== 2. publish a signed kind-1 event =="
node frontend/scripts/chat-relay-probe.mjs "$BUZZ_RELAY_WS" "$BUZZ_SERVICE_SK" "chat-e2e hello $(date +%s)"

echo "== 3. revocation denial (relay-side) =="
echo "Publish accepted above; after the relay revokes the member (kind 9031),"
echo "re-run the probe with the same key — it must exit 1. Orchestrate the"
echo "9030/9031 admission/revocation with the relay's own admin CLI; Elembra's"
echo "bridge already emits these commands (backend/crates/server/src/buzz_bridge.rs)."

echo "== 4. Elembra-side read gate after revocation =="
echo "Covered by backend/tests/chat_app_read_test.rs (revoked_binding_denies_reads);"
echo "run it with DATABASE_URL set. See docs/superpowers/specs/2026-08-12-elembra-chat-app-v1-design.md §8."

echo "proof harness complete"
```

(Steps 3-4 are orchestration notes for the operator because admission/revocation requires the relay's admin CLI and a running Elembra stack — the automated parts of the proof are the probe and the DB-backed read tests. This matches how ADR-0034's disposable-relay proof was executed.)

- [ ] **Step 3: Smoke-test the probe against a throwaway relay if the operator has an image; otherwise verify it at least fails cleanly**

Run: `cd frontend && node scripts/chat-relay-probe.mjs ws://127.0.0.1:9 $(node -e "console.log('00'.repeat(32))") test`
Expected: exit 1 with "FAILED: relay rejected or unreachable" (no crash).

- [ ] **Step 4: Commit**

```bash
git add frontend/scripts/chat-relay-probe.mjs scripts/run-chat-e2e.sh
git commit -s -m "test(chat): real-relay publish/revoke proof harness"
```

### Task 20: Changelog + implementation notes

**Files:**
- Modify: `CHANGELOG.md`
- Create: `docs/implementation/elembra-chat-app-v1.md`

- [ ] **Step 1: Changelog entry**

Under the top "Unreleased" section (or its current equivalent), add:

```markdown
### Added
- Elembra Chat Application v1: read surface for Buzz communities/channels/messages
  (authorized per message through the Chat owner → Buzz authority), browser-held
  signing keys with client-direct NIP-42 publish, live updates via the event
  broadcaster, Files attachments via `elembra-ref` tags, and Ask-this-channel
  with exact-message citation focus.
```

- [ ] **Step 2: Implementation note**

Write `docs/implementation/elembra-chat-app-v1.md` summarizing: the endpoints, the authorization chain (pre-filters → BuzzAuthority), the broadcast event, the frontend key custody model, the limitations list copied from spec §10, and how to run the proof harness. Reference the spec and ADR-0034/0035.

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md docs/implementation/elembra-chat-app-v1.md
git commit -s -m "docs: Chat Application v1 changelog and implementation notes"
```

### Task 21: Final validation gate + boundary review

- [ ] **Step 1: Backend gate**

```bash
cargo fmt --all --check
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings
SQLX_OFFLINE=true cargo test --workspace --all-features --lib
cargo sqlx prepare --workspace --check
DATABASE_URL=postgres://test:test@localhost:5432/test SQLX_OFFLINE=true cargo test --workspace --all-features -- --ignored --test-threads=1
```

Expected: all green (this includes the pre-existing Ask/Unified Search security matrix and the new `chat_app_read_test`).

- [ ] **Step 2: Frontend gate**

```bash
cd frontend && npm run check && npm run lint && npm run test && npm run build
```

Expected: all green (991+ tests including the new chat suites).

- [ ] **Step 3: Boundary review checklist (from the goal's final review)**

Verify each, fix BLOCKER/IMPORTANT only:
1. Buzz is still source of truth — no new chat tables; timeline derives from the observation index.
2. Identity/auth not duplicated — binding/admission endpoints untouched; UI consumes them.
3. Signing-key handling sound — server never receives the key; encrypted at rest; no silent custody.
4. Tenant/channel boundaries fail closed — mapping/admission/binding checks precede every read; per-message gate; cross-tenant tests.
5. Attachments preserve Files authority — prepare/preview/open unchanged and reauthorizing.
6. Ask reuses the existing pipeline — `/memory/ask` + `/memory/citations/open` untouched.
7. Chat behaves as an Elembra Application — renderer/icon/deep-link/enablement only.
8. Upstream Buzz modifications — none; the optional relay capability is already spec'd separately.

- [ ] **Step 4: Commit any fixes; then report per the goal's return list (architecture, flows, exact changes, tests, CI results, limitations, branch/PR/HEAD). Do NOT merge.**
