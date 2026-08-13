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
use rustshare_resource_auth::{PrincipalContext, ResourceRef};
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
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ChannelInfo {
    pub channel_id: String,
    pub channel_kind: String,
    pub latest_event_at: DateTime<Utc>,
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
    let mut admission_active = false;
    let mut mapping_info = None;
    if let (Some(mapping), Some(binding)) = (&mapping, &binding) {
        if mapping.active && binding.status == rustshare_resource_auth::BindingStatus::Active {
            admission_active = chat_identity
                .active_admission(ctx.tenant_id, &mapping.community_id, &binding.buzz_pubkey)
                .await
                .map_err(|e| AppError::internal(format!("chat admission lookup failed: {e}")))?;
        }
        // An inactive mapping must not disclose community/relay configuration.
        if mapping.active {
            mapping_info = Some(CommunityMappingInfo {
                community_id: mapping.community_id.clone(),
                relay_url: mapping.relay_url.clone(),
            });
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
