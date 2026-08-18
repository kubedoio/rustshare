//! Read surface for the Elembra Chat application: status, channels, and the
//! message timeline. Buzz remains the chat source of truth; these endpoints
//! serve Elembra's authorized observation projection and never read Buzz's
//! private database.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use rustshare_core::domain::{ActionCapability, ApplicationId, PrincipalId, TenantId, WorkspaceId};
use rustshare_resource_auth::{
    ChatIdentityBinding, PrincipalContext, ResourceRef, WorkspaceCommunityMapping,
};
use serde::{Deserialize, Serialize};

use super::{AppError, AuthenticatedUser};
use crate::state::AppState;

fn principal(auth: &AuthenticatedUser) -> PrincipalContext {
    PrincipalContext::user(
        PrincipalId(auth.user_id),
        TenantId(auth.tenant_id),
        WorkspaceId(auth.tenant_id),
    )
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CommunityMappingInfo {
    pub community_id: String,
    pub relay_url: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BindingInfo {
    pub status: String,
    pub buzz_pubkey: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ChatStatusResponse {
    pub chat_enabled: bool,
    pub mapping: Option<CommunityMappingInfo>,
    pub binding: Option<BindingInfo>,
    pub admission_active: bool,
    pub ask_available: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ChannelInfo {
    pub channel_id: String,
    pub channel_kind: String,
    pub latest_event_at: DateTime<Utc>,
}

/// Identifier-only attachment reference surfaced on a message. Never
/// authority and never tenant-hinting: opening reauthorizes through the
/// Files owner at read time (`POST /applications/chat/attachments/open`).
///
/// Serializes camelCase (`resourceType`/`resourceId`) to match the canonical
/// `ResourceRef` wire shape — the same field names the open/preview endpoints
/// accept back as the request body.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatAttachmentDto {
    pub application: String,
    pub resource_type: String,
    pub resource_id: String,
    pub version: Option<String>,
}

fn attachment_dto(reference: &ResourceRef) -> ChatAttachmentDto {
    ChatAttachmentDto {
        application: reference.application.0.clone(),
        resource_type: reference.resource_type.clone(),
        resource_id: reference.resource_id.clone(),
        version: reference.version.clone(),
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
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
    /// Identifier-only `elembra-ref` attachment references from the message's
    /// latest event, in event tag order (deduplicated at ingest).
    pub attachments: Vec<ChatAttachmentDto>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MessagesResponse {
    pub messages: Vec<ChatMessageDto>,
    /// Opaque pagination cursor (`"<unix_secs>:<event_id>"`); pass it back as
    /// the `before` query parameter to fetch the next page.
    pub next_before: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListMessagesQuery {
    pub channel_id: String,
    /// Opaque pagination cursor returned by `next_before`.
    pub before: Option<String>,
    pub limit: Option<i64>,
}

/// Own chat state: enablement, mapping, binding, admission. Only ever the
/// caller's own data.
#[utoipa::path(
    get,
    path = "/api/v1/applications/chat/status",
    tag = "Chat",
    responses(
        (status = 200, description = "Chat status", body = ChatStatusResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
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
    // An active mapping is visible independently of whether the caller has
    // created a binding yet; an inactive mapping must not disclose
    // community/relay configuration (existence-hiding).
    let mapping_info = mapping
        .as_ref()
        .filter(|m| m.active)
        .map(|m| CommunityMappingInfo {
            community_id: m.community_id.clone(),
            relay_url: m.relay_url.clone(),
        });

    let mut admission_active = false;
    if let (Some(mapping), Some(binding)) = (&mapping, &binding) {
        if mapping.active && binding.status == rustshare_resource_auth::BindingStatus::Active {
            admission_active = chat_identity
                .active_admission(ctx.tenant_id, &mapping.community_id, &binding.buzz_pubkey)
                .await
                .map_err(|e| AppError::internal(format!("chat admission lookup failed: {e}")))?;
        }
    }
    Ok(Json(ChatStatusResponse {
        chat_enabled,
        mapping: mapping_info,
        binding: binding.map(|b| BindingInfo {
            status: format!("{:?}", b.status),
            buzz_pubkey: b.buzz_pubkey,
        }),
        admission_active,
        ask_available: state.ask_workspace_service.is_available(),
    }))
}

/// Channels of the mapped community the caller may currently read. Derived
/// from the observation index and gated channel-by-channel.
#[utoipa::path(
    get,
    path = "/api/v1/applications/chat/channels",
    tag = "Chat",
    responses(
        (status = 200, description = "Readable channels", body = Vec<ChannelInfo>),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
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
    // Buzz mode: channel discovery comes from the community's authoritative
    // relay (the channel registry), never from the observation index.
    if let Some(gateway) = &state.buzz_gateway {
        return list_channels_from_registry(&state, &ctx, &mapping, gateway).await;
    }
    // Local mode: observation-derived discovery with per-channel gating
    // (historical behavior, unchanged).
    let summaries = state
        .chat_observation_store
        .distinct_channels(ctx.tenant_id, &mapping.community_id)
        .await
        .map_err(|e| AppError::internal(format!("chat channel lookup failed: {e}")))?;
    let mut channels = Vec::new();
    for summary in summaries {
        let kind = crate::authz::chat_owner::buzz_channel_kind(summary.channel_kind);
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

/// Buzz-mode channel discovery: the relay's authoritative channel registry
/// (`GET /api/v1/relay/channels`) is the discovery source, already filtered
/// by the relay to channels the caller's bound pubkey may read — no
/// per-channel re-gating here (that would cost one relay round-trip per
/// channel). Every failure is non-disclosing: missing/inactive local
/// authorization, an unpinned mapping, or a gateway error yields an empty
/// list, never an error response.
async fn list_channels_from_registry(
    state: &AppState,
    ctx: &PrincipalContext,
    mapping: &WorkspaceCommunityMapping,
    gateway: &crate::buzz_gateway::BuzzGatewayClient,
) -> Result<Json<Vec<ChannelInfo>>, AppError> {
    let chat_identity = state.chat_owner.chat_identity_store();
    // Pre-check (fail closed, existence hiding): the caller must currently
    // have Chat enabled, an active binding, an active admission for that
    // binding in the mapped community, and a pinned mapping — the same
    // local pre-filters the message gate applies before its relay call.
    if !chat_identity
        .chat_access(ctx.tenant_id, ctx.workspace_id, ctx.principal_id)
        .await
        .map_err(|e| AppError::internal(format!("chat access lookup failed: {e}")))?
    {
        return Ok(Json(Vec::new()));
    }
    let Some(binding) = chat_identity
        .active_binding(ctx.tenant_id, ctx.principal_id)
        .await
        .map_err(|e| AppError::internal(format!("chat binding lookup failed: {e}")))?
    else {
        return Ok(Json(Vec::new()));
    };
    if !chat_identity
        .active_admission(ctx.tenant_id, &mapping.community_id, &binding.buzz_pubkey)
        .await
        .map_err(|e| AppError::internal(format!("chat admission lookup failed: {e}")))?
    {
        return Ok(Json(Vec::new()));
    }
    let Some(relay_pubkey) = mapping.relay_pubkey.as_deref() else {
        tracing::warn!(
            application = crate::authz::chat_owner::CHAT_APPLICATION_ID,
            tenant = %ctx.tenant_id,
            "buzz-mode channel discovery: mapping has no pinned relay pubkey; returning an empty list"
        );
        return Ok(Json(Vec::new()));
    };
    let registry_channels = match gateway
        .list_channels(&mapping.relay_url, relay_pubkey, &binding.buzz_pubkey)
        .await
    {
        Ok(channels) => channels,
        Err(error) => {
            tracing::warn!(
                application = crate::authz::chat_owner::CHAT_APPLICATION_ID,
                tenant = %ctx.tenant_id,
                %error,
                "buzz-mode channel discovery failed; returning an empty list"
            );
            return Ok(Json(Vec::new()));
        }
    };
    // POST-check: re-read the local authorization state AFTER the registry
    // response and fail closed if anything changed. A revocation or a
    // reconfiguration racing the registry call must not surface a stale
    // list — this mirrors the message gate's post-authority linearization
    // (`ChatResourceOwner::gate_post_authority`).
    if !list_authorization_still_valid(state, ctx, mapping, &binding).await {
        tracing::warn!(
            application = crate::authz::chat_owner::CHAT_APPLICATION_ID,
            tenant = %ctx.tenant_id,
            "buzz-mode channel discovery: local authorization changed during the registry call; returning an empty list"
        );
        return Ok(Json(Vec::new()));
    }
    // The registry carries no per-channel event timestamp; `latest_event_at`
    // is the response time (the client verified the response fresh, ≤60s).
    let latest_event_at = Utc::now();
    Ok(Json(
        registry_channels
            .into_iter()
            .map(|channel| ChannelInfo {
                channel_id: channel.channel_id,
                channel_kind: registry_channel_kind(&channel.channel_type, &channel.visibility)
                    .to_string(),
                latest_event_at,
            })
            .collect(),
    ))
}

/// Re-read the local authorization state after the registry response: the
/// same lookups the pre-check used (Chat enablement, active binding with the
/// same pubkey, an active admission for it, and an active + unchanged
/// mapping). Any change — or any store failure — fails closed, mirroring the
/// message gate's post-authority linearization.
async fn list_authorization_still_valid(
    state: &AppState,
    ctx: &PrincipalContext,
    mapping: &WorkspaceCommunityMapping,
    binding: &ChatIdentityBinding,
) -> bool {
    let chat_identity = state.chat_owner.chat_identity_store();
    if !chat_identity
        .chat_access(ctx.tenant_id, ctx.workspace_id, ctx.principal_id)
        .await
        .ok()
        .unwrap_or(false)
    {
        return false;
    }
    let Some(final_binding) = chat_identity
        .active_binding(ctx.tenant_id, ctx.principal_id)
        .await
        .ok()
        .flatten()
    else {
        return false;
    };
    if final_binding.buzz_pubkey != binding.buzz_pubkey {
        return false;
    }
    if !chat_identity
        .active_admission(
            ctx.tenant_id,
            &mapping.community_id,
            &final_binding.buzz_pubkey,
        )
        .await
        .ok()
        .unwrap_or(false)
    {
        return false;
    }
    let Some(final_mapping) = chat_identity
        .mapping(ctx.tenant_id, ctx.workspace_id)
        .await
        .ok()
        .flatten()
    else {
        return false;
    };
    final_mapping.active
        && final_mapping.community_id == mapping.community_id
        && final_mapping.relay_url == mapping.relay_url
        && final_mapping.relay_pubkey == mapping.relay_pubkey
}

/// Project a registry channel onto the Elembra channel-kind vocabulary —
/// the SAME mapping the relay applies to its `channel_type`/`visibility`
/// (`dm` → `"dm"`; anything else private → `"private"`; everything else →
/// `"workspace"`). `"excluded"` is an Elembra-side concept the relay never
/// emits.
fn registry_channel_kind(channel_type: &str, visibility: &str) -> &'static str {
    if channel_type == "dm" {
        "dm"
    } else if visibility == "private" {
        "private"
    } else {
        "workspace"
    }
}

/// Timeline for one channel: folded latest-event-per-message, newest first,
/// per-message authorization, deleted messages hidden.
#[utoipa::path(
    get,
    path = "/api/v1/applications/chat/messages",
    tag = "Chat",
    params(
        ("channel_id" = String, Query, description = "Channel id"),
        ("before" = Option<String>, Query, description = "Opaque pagination cursor from `next_before` (\u{201c}<unix_secs>:<event_id>\u{201d})"),
        ("limit" = Option<i64>, Query, description = "Maximum number of messages to return (clamped to 1..=64)"),
    ),
    responses(
        (status = 200, description = "Channel timeline", body = MessagesResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
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
    let limit = query.limit.unwrap_or(50).clamp(1, 64);
    // The opaque cursor is "<unix_secs>:<event_id>" — the (created_at,
    // event_id) keyset the store pages with. Reject malformed cursors.
    let before = query
        .before
        .as_deref()
        .map(|raw| -> Result<(DateTime<Utc>, String), AppError> {
            let (ts, event_id) = raw
                .split_once(':')
                .ok_or_else(|| AppError::bad_request("invalid pagination cursor"))?;
            let secs: i64 = ts
                .parse()
                .map_err(|_| AppError::bad_request("invalid pagination cursor"))?;
            let created_at = DateTime::<Utc>::from_timestamp(secs, 0)
                .ok_or_else(|| AppError::bad_request("invalid pagination cursor"))?;
            Ok((created_at, event_id.to_string()))
        })
        .transpose()?;
    let events = state
        .chat_observation_store
        .list_for_timeline(
            ctx.tenant_id,
            &mapping.community_id,
            &query.channel_id,
            before,
            limit,
        )
        .await
        .map_err(|e| AppError::internal(format!("chat timeline lookup failed: {e}")))?;

    // The cursor comes from the last *fetched* row, not the last visible one:
    // if a whole page is tombstoned or denied, pagination must still advance
    // so older authorized messages stay reachable. The event_id tiebreak
    // guarantees same-second messages are never skipped.
    let next_before = events
        .last()
        .map(|event| format!("{}:{}", event.event_created_at.timestamp(), event.event_id));

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
    let action = ActionCapability::new(rustshare_resource_auth::CHAT_READ);
    let refs: Vec<ResourceRef> = visible
        .iter()
        .map(|event| {
            ResourceRef::new(
                ApplicationId::new(crate::authz::chat_owner::CHAT_APPLICATION_ID),
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
            metrics::counter!("chat_authorization_denials_total").increment(1);
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
            attachments: event.attachment_refs.iter().map(attachment_dto).collect(),
        });
    }
    Ok(Json(MessagesResponse {
        messages,
        next_before,
    }))
}

/// Single folded message, for citation focus/scroll. Existence-hiding 404.
#[utoipa::path(
    get,
    path = "/api/v1/applications/chat/messages/{message_id}",
    tag = "Chat",
    params(("message_id" = String, Path, description = "Buzz message id")),
    responses(
        (status = 200, description = "Single message", body = ChatMessageDto),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
        (status = 404, description = "Not found", body = crate::handlers::ErrorResponse),
    ),
)]
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
        ApplicationId::new(crate::authz::chat_owner::CHAT_APPLICATION_ID),
        "message",
        latest.message_id.clone(),
    );
    let decision = state
        .source_authorizer
        .authorize(
            &ctx,
            &ActionCapability::new(rustshare_resource_auth::CHAT_READ),
            &resource,
        )
        .await;
    if !decision.is_allow() {
        metrics::counter!("chat_authorization_denials_total").increment(1);
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
        attachments: latest.attachment_refs.iter().map(attachment_dto).collect(),
    }))
}
