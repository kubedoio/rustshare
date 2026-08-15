//! Elembra Chat source-owner adapter.
//!
//! This adapter is the Chat Application's export surface for the
//! source-authorization contract (ADR-0033/ADR-0034): it implements
//! [`ResourceOwner`] for `io.elembra.chat` `message` refs so
//! [`SourceAuthorizer`](rustshare_resource_auth::SourceAuthorizer)'s
//! `authorize`/`resolve`/`fetch`/`materialize` can be used for Buzz chat
//! content.
//!
//! # Authorization invariant (requirement #11: stale Memory cannot override)
//!
//! Authorization is derived **only** from CURRENT Chat/Buzz state:
//!
//! * the tenant's current Chat Application enablement
//!   (`application_enablements` + per-user preference, via
//!   `ChatIdentityStore::chat_access`);
//! * the principal's current active Buzz binding
//!   (`ChatIdentityStore::active_binding`);
//! * an active admission for the message's community and the bound Buzz
//!   pubkey (`ChatIdentityStore::active_admission`);
//! * the FINAL channel/message decision from the configured
//!   [`BuzzAuthority`] (current membership/visibility/message availability
//!   at the community's authoritative relay).
//!
//! The bridge-owned observation row (`chat_observed_events`) supplies routing
//! context (community/channel) and message existence — it NEVER grants access
//! on its own. Memory-owned state (`memory_catalog`) is never imported or
//! queried here, so a stale or tampered Memory record cannot override a
//! current revocation.
//!
//! # Channel/message authority
//!
//! The gate's FINAL channel/message decision comes from the configured Buzz
//! authority ([`BuzzAuthority::can_read`]; the batch surface uses
//! [`BuzzAuthority::can_read_batch`] — one relay round-trip for a whole
//! page) AFTER the local pre-filter above; local admission is a coarse
//! pre-filter only, never a final allow. When no
//! upstream authority is configured ([`LocalFallbackAuthority`]), behavior
//! matches the historical coarse gate — workspace channels only. That is NOT
//! per-channel authorization: it requires the upstream capability, and the
//! community's relay remains the final authority. Every authority failure
//! fails closed to Deny.
//!
//! # Existence-hiding
//!
//! Unknown and tombstoned messages resolve to [`Decision::NotFound`]; every
//! non-Allow outcome on `resolve`/`fetch` surfaces as
//! [`SourceError::NotFound`], so a caller cannot distinguish "never existed"
//! from "not authorized" through those entry points. `authorize` keeps the
//! Deny/NotFound distinction for typed internal callers.
//!
//! # Tombstone override
//!
//! The gate additionally refuses any message for which a Deleted observation
//! exists at-or-after the latest candidate row: a message is never exposable
//! once it has been deleted, and later pushed edits cannot resurrect it at
//! the authorizer (a same-second delete/edit tie resolves deterministically
//! by `event_id` in `lookup_for_auth`, and the tombstone check then wins).
//!
//! # Signature handling
//!
//! Signatures are NOT re-verified here: the observation index is the bridge's
//! already-verified state (`signature_verified` is set by the observation
//! pipeline); this adapter trusts that boundary and never re-derives Buzz
//! signatures.
//!
//! # Delegated principals
//!
//! Delegation (`PrincipalContext::effective_user_authority`) is intentionally
//! NOT applied here: chat bindings exist only for user principals, so a
//! Service/Agent principal or a delegated request always fails closed to Deny
//! today. Wire `effective_user_authority` when the first delegated consumer
//! (Memory/RAG `materialize` or a transport adapter) lands.

use bytes::Bytes;
use futures_util::{stream, StreamExt};
use rustshare_core::domain::{ActionCapability, ApplicationId, TenantId};
use rustshare_memory::event::{ChatChannelKind, ObservedEventType};
use rustshare_memory::observed::ChatObservedEvent;
use rustshare_resource_auth::{
    BatchDecision, BuzzAuthority, BuzzAuthorityError, BuzzChannelKind, BuzzReadDecision,
    BuzzReadRequest, ChatIdentityBinding, Decision, FetchedResource, LocalFallbackAuthority,
    PrincipalContext, Purpose, Representation, ResolvedResource, ResourceCapability, ResourceOwner,
    ResourceRef, SourceError, WorkspaceCommunityMapping, CHAT_READ, MAX_BATCH_SIZE,
};
use rustshare_storage::{ChatIdentityStore, ChatObservationStore};
use std::sync::OnceLock;

const AUTHORIZATION_CONCURRENCY: usize = 16;

/// The Chat resource type served by this owner.
pub const RESOURCE_TYPE_MESSAGE: &str = "message";

/// The Application this owner serves.
pub const CHAT_APPLICATION_ID: &str = "io.elembra.chat";

/// The owner adapter for the `io.elembra.chat` Application.
pub struct ChatResourceOwner {
    chat_identity: ChatIdentityStore,
    observations: ChatObservationStore,
    /// The FINAL channel/message decision maker: the configured Buzz
    /// authority when upstream is wired, otherwise
    /// [`LocalFallbackAuthority`] (historical coarse workspace-only gate).
    authority: Box<dyn BuzzAuthority>,
}

impl ChatResourceOwner {
    /// Build the owner with the coarse local fallback authority
    /// (workspace channels only) — the unconfigured path.
    pub fn new(chat_identity: ChatIdentityStore, observations: ChatObservationStore) -> Self {
        Self::with_authority(
            chat_identity,
            observations,
            Box::new(LocalFallbackAuthority),
        )
    }

    /// Build the owner with an explicit Buzz authority; the final
    /// channel/message decision comes from `authority`.
    pub fn with_authority(
        chat_identity: ChatIdentityStore,
        observations: ChatObservationStore,
        authority: Box<dyn BuzzAuthority>,
    ) -> Self {
        Self {
            chat_identity,
            observations,
            authority,
        }
    }

    /// The stable Application identity this adapter serves.
    fn application_id() -> &'static ApplicationId {
        static APPLICATION_ID: OnceLock<ApplicationId> = OnceLock::new();
        APPLICATION_ID.get_or_init(|| ApplicationId::new(CHAT_APPLICATION_ID))
    }

    /// The resource surface this adapter serves. Registration
    /// (`authz::build_source_authorizer`) validates it against the canonical
    /// `ApplicationRegistry`: the `io.elembra.chat` manifest must declare the
    /// same resource type with the same action capability (see the unit test
    /// at the bottom of this file).
    pub fn declared_capabilities() -> Vec<ResourceCapability> {
        vec![ResourceCapability::new(RESOURCE_TYPE_MESSAGE, &[CHAT_READ])]
    }

    /// The identity store backing this owner, shared with the Chat app read
    /// surface so it can surface the caller's own status (enablement, mapping,
    /// binding, admission) through the same stores as the gate.
    pub fn chat_identity_store(&self) -> &ChatIdentityStore {
        &self.chat_identity
    }

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
        let mapping = match self
            .chat_identity
            .mapping(ctx.tenant_id, ctx.workspace_id)
            .await
        {
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
        let binding = match self
            .chat_identity
            .active_binding(ctx.tenant_id, ctx.principal_id)
            .await
        {
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
        if !matches!(
            decision,
            Ok(rustshare_resource_auth::BuzzReadDecision::Allow)
        ) {
            return false;
        }
        // Post-authority linearization (mirrors `gate`): a revocation racing
        // the relay decision must not still list the channel.
        let mapping = match self
            .chat_identity
            .mapping(ctx.tenant_id, ctx.workspace_id)
            .await
        {
            Ok(Some(mapping)) => mapping,
            Ok(None) => return false,
            Err(error) => {
                tracing::warn!(%error, "chat channel gate: post-check mapping lookup failed");
                return false;
            }
        };
        if !mapping.active || mapping.community_id != community_id {
            return false;
        }
        let binding = match self
            .chat_identity
            .active_binding(ctx.tenant_id, ctx.principal_id)
            .await
        {
            Ok(Some(binding)) => binding,
            Ok(None) => return false,
            Err(error) => {
                tracing::warn!(%error, "chat channel gate: post-check binding lookup failed");
                return false;
            }
        };
        if binding.status != rustshare_resource_auth::BindingStatus::Active {
            return false;
        }
        match self
            .chat_identity
            .active_admission(ctx.tenant_id, community_id, &binding.buzz_pubkey)
            .await
        {
            Ok(true) => true,
            Ok(false) => false,
            Err(error) => {
                tracing::warn!(%error, "chat channel gate: post-check admission lookup failed");
                false
            }
        }
    }
}

/// The message id shape: 64 lowercase hex characters (the content-addressed
/// Buzz message id). Anything else fails closed as not-found, so malformed
/// refs look absent rather than present-but-denied.
fn is_message_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Map the observed event's channel classification onto the authority
/// contract's wire-identical enum (both are `workspace|dm|private|excluded`).
/// A `From` impl is not possible here (both types are foreign to this
/// crate), so the gate converts through this helper. Shared with the Chat
/// app read surface (`handlers::chat_app::list_channels`).
pub(crate) fn buzz_channel_kind(kind: ChatChannelKind) -> BuzzChannelKind {
    match kind {
        ChatChannelKind::Workspace => BuzzChannelKind::Workspace,
        ChatChannelKind::Dm => BuzzChannelKind::Dm,
        ChatChannelKind::Private => BuzzChannelKind::Private,
        ChatChannelKind::Excluded => BuzzChannelKind::Excluded,
    }
}

impl ChatResourceOwner {
    /// Run the full authorization gate and return the observation row the
    /// decision is based on. `Err(decision)` fails closed with that decision.
    ///
    /// Every store failure logs and fails closed to Deny — never error-open.
    async fn gate(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
    ) -> Result<ChatObservedEvent, Decision> {
        let prefilter = self.gate_prefilter(ctx, resource).await?;
        // FINAL channel/message decision from the configured Buzz authority
        // (current membership/visibility/message availability). Any
        // authority failure fails closed to Deny.
        gate_authority(&*self.authority, &prefilter.request, resource).await?;
        self.gate_post_authority(ctx, resource, &prefilter).await
    }

    /// Steps 1–7 of the gate, up to (but excluding) the Buzz authority call:
    /// shape/app checks, observation lookup, tombstone/inactive checks, and
    /// the local pre-filters (enablement, binding, admission, mapping).
    /// `Err(decision)` short-circuits that ref; `Ok` carries the authority
    /// request plus the admission state the post-authority re-reads compare
    /// against (a racing revocation must change at least one of them).
    async fn gate_prefilter(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
    ) -> Result<GatePrefilter, Decision> {
        if resource.application.0 != CHAT_APPLICATION_ID
            || resource.resource_type != RESOURCE_TYPE_MESSAGE
        {
            return Err(Decision::Invalid);
        }
        let message_id = resource.resource_id.as_str();
        if !is_message_id(message_id) {
            return Err(Decision::NotFound);
        }
        let Some(row) = self.lookup_observation(ctx, resource).await? else {
            return Err(Decision::NotFound);
        };
        // A tombstoned/deleted or deactivated observation row is not an
        // exposable message: it looks absent (existence-hiding).
        if !row.active || row.event_type == ObservedEventType::Deleted {
            return Err(Decision::NotFound);
        }
        // Authorizer-level tombstone override: a message is never exposable
        // once any Deleted observation exists at-or-after the latest candidate
        // row's time. A later-pushed edit that ties the delete's created_at
        // (Nostr timestamps are second-resolution) and wins the `event_id`
        // tie-break must not resurrect the message at the authorizer.
        match self
            .observations
            .has_tombstone_since(ctx.tenant_id, message_id, row.event_created_at)
            .await
        {
            Ok(true) => return Err(Decision::NotFound),
            Ok(false) => {}
            Err(error) => {
                tracing::error!(
                    application = CHAT_APPLICATION_ID,
                    %resource,
                    tenant = %ctx.tenant_id,
                    %error,
                    "chat tombstone check failed; denying access"
                );
                return Err(Decision::Deny);
            }
        }
        // Local pre-filter (coarse): the tenant must have Chat enabled, the
        // principal an active Buzz binding, and the bound pubkey an active
        // admission in the message's community. None of these alone is a
        // final allow; the Buzz authority below makes the channel/message
        // decision.
        if !self.current_chat_enabled(ctx).await {
            return Err(Decision::Deny);
        }
        let Some(binding) = self.current_binding(ctx).await else {
            return Err(Decision::Deny);
        };
        if !self
            .current_admission(ctx, &row.community_id, &binding.buzz_pubkey)
            .await
        {
            return Err(Decision::Deny);
        }
        // Admission requires an active community mapping, so a missing
        // mapping here is an internal inconsistency; fail closed.
        let Some(mapping) = self.current_mapping(ctx).await else {
            tracing::error!(
                application = CHAT_APPLICATION_ID,
                %resource,
                tenant = %ctx.tenant_id,
                "mapping lookup failed or returned no active mapping despite an active admission; denying access"
            );
            return Err(Decision::Deny);
        };
        // Fail-closed guard: the mapping may have been deactivated between
        // the `active_admission` check above and the authority call below;
        // an inactive mapping must never reach the authority.
        if !mapping.active {
            return Err(Decision::Deny);
        }
        let request = BuzzReadRequest {
            tenant_id: ctx.tenant_id,
            community_id: mapping.community_id.clone(),
            relay_url: mapping.relay_url.clone(),
            relay_pubkey: mapping.relay_pubkey.clone(),
            channel_id: row.channel_id.clone(),
            channel_kind: buzz_channel_kind(row.channel_kind),
            message_id: Some(resource.resource_id.clone()),
            pubkey: binding.buzz_pubkey.clone(),
            event_created_at: row.event_created_at,
        };
        Ok(GatePrefilter {
            request,
            row,
            binding,
            mapping,
        })
    }

    /// Step 9 of the gate: re-read Elembra's local admission state AFTER the
    /// external authority decision, giving revocations a final linearization
    /// point before any caller can materialize the row body. Returns the
    /// final observation row.
    async fn gate_post_authority(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        prefilter: &GatePrefilter,
    ) -> Result<ChatObservedEvent, Decision> {
        if !self.current_chat_enabled(ctx).await {
            return Err(Decision::Deny);
        }
        let Some(final_binding) = self.current_binding(ctx).await else {
            return Err(Decision::Deny);
        };
        if final_binding.buzz_pubkey != prefilter.binding.buzz_pubkey
            || !self
                .current_admission(ctx, &prefilter.row.community_id, &final_binding.buzz_pubkey)
                .await
        {
            return Err(Decision::Deny);
        }
        let Some(final_row) = self.lookup_observation(ctx, resource).await? else {
            return Err(Decision::Deny);
        };
        if !final_row.active || final_row.event_type == ObservedEventType::Deleted {
            return Err(Decision::Deny);
        }
        if self
            .observations
            .has_tombstone_since(
                ctx.tenant_id,
                resource.resource_id.as_str(),
                final_row.event_created_at,
            )
            .await
            .map_err(|_| Decision::Deny)?
        {
            return Err(Decision::Deny);
        }
        let Some(final_mapping) = self.current_mapping(ctx).await else {
            return Err(Decision::Deny);
        };
        if !final_mapping.active
            || final_mapping.community_id != prefilter.mapping.community_id
            || final_mapping.relay_url != prefilter.mapping.relay_url
            || final_mapping.relay_pubkey != prefilter.mapping.relay_pubkey
        {
            return Err(Decision::Deny);
        }
        Ok(final_row)
    }

    async fn lookup_observation(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
    ) -> Result<Option<ChatObservedEvent>, Decision> {
        match self
            .observations
            .lookup_for_auth(ctx.tenant_id, &resource.resource_id)
            .await
        {
            Ok(row) => Ok(row),
            Err(error) => {
                tracing::error!(
                    application = CHAT_APPLICATION_ID,
                    %resource,
                    tenant = %ctx.tenant_id,
                    %error,
                    "chat observation lookup failed; denying access"
                );
                Err(Decision::Deny)
            }
        }
    }

    /// Current Chat enablement for the principal in its workspace
    /// (`application_enablements.enabled` AND the per-user preference).
    async fn current_chat_enabled(&self, ctx: &PrincipalContext) -> bool {
        match self
            .chat_identity
            .chat_access(ctx.tenant_id, ctx.workspace_id, ctx.principal_id)
            .await
        {
            Ok(true) => true,
            Ok(false) => false,
            Err(error) => {
                tracing::error!(
                    application = CHAT_APPLICATION_ID,
                    tenant = %ctx.tenant_id,
                    principal = %ctx.principal_id,
                    %error,
                    "chat enablement check failed; denying access"
                );
                false
            }
        }
    }

    /// The principal's current active Buzz binding, if any.
    async fn current_binding(&self, ctx: &PrincipalContext) -> Option<ChatIdentityBinding> {
        match self
            .chat_identity
            .active_binding(ctx.tenant_id, ctx.principal_id)
            .await
        {
            Ok(binding) => binding,
            Err(error) => {
                tracing::error!(
                    application = CHAT_APPLICATION_ID,
                    tenant = %ctx.tenant_id,
                    principal = %ctx.principal_id,
                    %error,
                    "chat binding lookup failed; denying access"
                );
                None
            }
        }
    }

    /// The workspace's current Buzz community mapping, if any. Any store
    /// error fails closed (None + log), mirroring `current_binding`.
    async fn current_mapping(&self, ctx: &PrincipalContext) -> Option<WorkspaceCommunityMapping> {
        match self
            .chat_identity
            .mapping(ctx.tenant_id, ctx.workspace_id)
            .await
        {
            Ok(mapping) => mapping,
            Err(error) => {
                tracing::error!(
                    application = CHAT_APPLICATION_ID,
                    tenant = %ctx.tenant_id,
                    workspace = %ctx.workspace_id,
                    %error,
                    "chat community mapping lookup failed; denying access"
                );
                None
            }
        }
    }

    /// Whether the bound Buzz pubkey currently has an active admission in the
    /// message's community (admission AND community mapping must both be
    /// active).
    async fn current_admission(
        &self,
        ctx: &PrincipalContext,
        community_id: &str,
        buzz_pubkey: &str,
    ) -> bool {
        match self
            .chat_identity
            .active_admission(ctx.tenant_id, community_id, buzz_pubkey)
            .await
        {
            Ok(true) => true,
            Ok(false) => false,
            Err(error) => {
                tracing::error!(
                    application = CHAT_APPLICATION_ID,
                    tenant = %ctx.tenant_id,
                    community = community_id,
                    %error,
                    "chat admission check failed; denying access"
                );
                false
            }
        }
    }
}

/// Everything the gate captures before the Buzz authority call: the authority
/// request plus the admission state the post-authority re-reads compare
/// against (a racing revocation must change at least one of these).
struct GatePrefilter {
    request: BuzzReadRequest,
    row: ChatObservedEvent,
    binding: ChatIdentityBinding,
    mapping: WorkspaceCommunityMapping,
}

/// Map one authority outcome to the shared gate decision: `Allow` maps to
/// `Allow`, `Deny`/`NotFound` map directly, and any authority error logs and
/// fails closed to `Deny` — the single and batch authority paths use this
/// same fail-closed mapping.
fn authority_outcome_to_decision(
    outcome: Result<BuzzReadDecision, BuzzAuthorityError>,
    resource: &ResourceRef,
    tenant: TenantId,
) -> Decision {
    match outcome {
        Ok(BuzzReadDecision::Allow) => Decision::Allow,
        Ok(BuzzReadDecision::Deny) => Decision::Deny,
        Ok(BuzzReadDecision::NotFound) => Decision::NotFound,
        Err(error) => {
            tracing::error!(
                application = CHAT_APPLICATION_ID,
                %resource,
                tenant = %tenant,
                %error,
                "Buzz authority check failed; denying access"
            );
            Decision::Deny
        }
    }
}

/// Run the FINAL channel/message decision from the configured Buzz authority
/// against the request built by the gate. `Ok(())` only on `Allow`; every
/// other outcome fails closed (see [`authority_outcome_to_decision`]).
async fn gate_authority(
    authority: &dyn BuzzAuthority,
    req: &BuzzReadRequest,
    resource: &ResourceRef,
) -> Result<(), Decision> {
    match authority_outcome_to_decision(authority.can_read(req).await, resource, req.tenant_id) {
        Decision::Allow => Ok(()),
        decision => Err(decision),
    }
}

/// Map ONE batch authority outcome to the gate's final decision: `Allow`
/// proceeds to the post-authority linearization re-reads (identical to the
/// single path); every other outcome fails closed exactly like
/// [`gate_authority`].
async fn gate_batch_item(
    owner: &ChatResourceOwner,
    ctx: &PrincipalContext,
    resource: &ResourceRef,
    prefilter: &GatePrefilter,
    authority_outcome: Result<BuzzReadDecision, BuzzAuthorityError>,
) -> Decision {
    match authority_outcome_to_decision(authority_outcome, resource, ctx.tenant_id) {
        Decision::Allow => match owner.gate_post_authority(ctx, resource, prefilter).await {
            Ok(_) => Decision::Allow,
            Err(decision) => decision,
        },
        decision => decision,
    }
}

#[async_trait::async_trait]
impl ResourceOwner for ChatResourceOwner {
    fn application_id(&self) -> &ApplicationId {
        Self::application_id()
    }

    fn resource_capabilities(&self) -> Vec<ResourceCapability> {
        Self::declared_capabilities()
    }

    async fn authorize(
        &self,
        ctx: &PrincipalContext,
        action: &ActionCapability,
        resource: &ResourceRef,
    ) -> Decision {
        // The owner declares exactly `chat.read`; any other action fails
        // closed (mirrors the Files owner's unsupported-action handling).
        if action.0.as_str() != CHAT_READ {
            tracing::debug!(
                application = CHAT_APPLICATION_ID,
                %resource,
                action = %action,
                "unsupported action for chat message"
            );
            return Decision::Invalid;
        }
        match self.gate(ctx, resource).await {
            Ok(_) => Decision::Allow,
            Err(decision) => decision,
        }
    }

    async fn authorize_batch(
        &self,
        ctx: &PrincipalContext,
        action: &ActionCapability,
        resources: &[ResourceRef],
    ) -> Vec<BatchDecision> {
        // Oversized batches fail closed; the Platform-Core facade already
        // enforces the batch bound before dispatching.
        if resources.len() > MAX_BATCH_SIZE {
            return resources
                .iter()
                .map(|resource| BatchDecision::new(resource.clone(), Decision::Deny))
                .collect();
        }
        // Unsupported actions fail closed per ref, exactly like `authorize`.
        if action.0.as_str() != CHAT_READ {
            tracing::debug!(
                application = CHAT_APPLICATION_ID,
                action = %action,
                "unsupported action for chat message"
            );
            return resources
                .iter()
                .map(|resource| BatchDecision::new(resource.clone(), Decision::Invalid))
                .collect();
        }
        // Phase 1 (per ref, concurrent): shape/app checks, observation
        // lookup, tombstone/inactive checks and the local pre-filters —
        // everything up to the authority call. Refused refs short-circuit
        // here and never reach the relay.
        let mut phase1 = stream::iter(resources.iter().cloned().enumerate())
            .map(|(index, resource)| async move {
                let outcome = self.gate_prefilter(ctx, &resource).await;
                (index, resource, outcome)
            })
            .buffer_unordered(AUTHORIZATION_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
        phase1.sort_unstable_by_key(|(index, _, _)| *index);
        let mut refused: Vec<(usize, ResourceRef, Decision)> = Vec::new();
        let mut survivors: Vec<(usize, ResourceRef, GatePrefilter)> = Vec::new();
        for (index, resource, outcome) in phase1 {
            match outcome {
                Err(decision) => refused.push((index, resource, decision)),
                Ok(prefilter) => survivors.push((index, resource, prefilter)),
            }
        }

        // Phase 2: ONE authority round-trip for every surviving ref. The
        // batch authority preserves request order and isolates per-item
        // failures; `LocalFallbackAuthority` inherits the same bounded
        // fan-out the previous per-ref path used, so local mode is
        // behavior-identical.
        let requests: Vec<BuzzReadRequest> = survivors
            .iter()
            .map(|(_, _, prefilter)| prefilter.request.clone())
            .collect();
        let authority_results = self.authority.can_read_batch(&requests).await;

        // Phase 3 (per ref, concurrent): the post-authority linearization
        // re-reads still run for every survivor — a revocation racing the
        // batch call denies here, exactly like the single path.
        let mut phase3 = stream::iter(survivors.into_iter().zip(authority_results).map(
            |((index, resource, prefilter), authority_outcome)| async move {
                let decision =
                    gate_batch_item(self, ctx, &resource, &prefilter, authority_outcome).await;
                (index, BatchDecision::new(resource, decision))
            },
        ))
        .buffer_unordered(AUTHORIZATION_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
        phase3.sort_unstable_by_key(|(index, _)| *index);

        // Assemble in input order: pre-filter refusals plus post-authority
        // outcomes.
        let mut decisions: Vec<(usize, BatchDecision)> = refused
            .into_iter()
            .map(|(index, resource, decision)| (index, BatchDecision::new(resource, decision)))
            .chain(phase3)
            .collect();
        decisions.sort_unstable_by_key(|(index, _)| *index);
        decisions
            .into_iter()
            .map(|(_, decision)| decision)
            .collect()
    }

    async fn resolve(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        _purpose: Purpose,
    ) -> Result<ResolvedResource, SourceError> {
        let row = self
            .gate(ctx, resource)
            .await
            .map_err(|_| SourceError::NotFound)?;
        let message_id = resource.resource_id.as_str();
        Ok(ResolvedResource {
            resource: resource.clone(),
            display_name: format!("buzz message {}", &message_id[..8]),
            media_type: Some("text/plain".into()),
            size: None,
            updated_at: Some(row.event_created_at),
            // Reference-first: an indexing copy exists only when the tenant
            // had content_indexing enabled at observation time.
            available: row.body.is_some(),
        })
    }

    async fn fetch(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        representation: Representation,
    ) -> Result<FetchedResource, SourceError> {
        // `Raw` and `Text` both return the authorized body bytes (a Buzz text
        // note IS text); `Text` is what `SourceAuthorizer::materialize`
        // requests, so chat candidates can materialize. Preview/Thumbnail/
        // Metadata are unsupported.
        match representation {
            Representation::Raw | Representation::Text => {
                let row = self
                    .gate(ctx, resource)
                    .await
                    .map_err(|_| SourceError::NotFound)?;
                let Some(body) = row.body else {
                    // Reference-first default: no indexing copy exists
                    // (content_indexing was off at observation time). The
                    // contract has no generic `Unavailable` variant;
                    // `VersionUnavailable` is the closest "content for this
                    // version is not available" signal and is never a
                    // retryable infrastructure failure.
                    return Err(SourceError::VersionUnavailable);
                };
                Ok(FetchedResource {
                    resource: resource.clone(),
                    representation,
                    media_type: Some("text/plain".into()),
                    size: Some(body.len() as i64),
                    data: Bytes::from(body),
                })
            }
            Representation::Preview | Representation::Thumbnail | Representation::Metadata => Err(
                SourceError::UnsupportedRepresentation(format!("{representation:?}")),
            ),
        }
    }

    async fn fetch_delivery_url(
        &self,
        _ctx: &PrincipalContext,
        _resource: &ResourceRef,
        _purpose: Purpose,
        _ttl_secs: u64,
    ) -> Result<String, SourceError> {
        // Chat content has no object-storage delivery URLs.
        Err(SourceError::UnsupportedRepresentation(
            "chat messages have no delivery URLs".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::{ApplicationRegistry, TenantId};
    use rustshare_resource_auth::BuzzAuthorityError;

    /// The adapter's declared resource/action surface must exactly match the
    /// `io.elembra.chat` manifest in the canonical ApplicationRegistry —
    /// this is the invariant `ResourceOwnerRegistry::register` enforces at
    /// bootstrap.
    #[test]
    fn declared_surface_matches_application_registry_manifest() {
        let registry = ApplicationRegistry::first_party().expect("first-party manifests are valid");
        let manifest = registry
            .manifest(&ApplicationId::new(CHAT_APPLICATION_ID))
            .expect("the io.elembra.chat manifest is present");
        let surface = ChatResourceOwner::declared_capabilities();
        assert_eq!(
            manifest.resources.len(),
            surface.len(),
            "manifest and adapter must declare the same number of resource types"
        );
        for capability in &surface {
            let declared = manifest
                .resources
                .iter()
                .find(|resource| resource.resource_type == capability.resource_type)
                .unwrap_or_else(|| {
                    panic!(
                        "manifest does not declare resource type `{}`",
                        capability.resource_type
                    )
                });
            assert_eq!(
                declared.actions, capability.actions,
                "action surface for `{}` must match the manifest",
                capability.resource_type
            );
        }
    }

    /// A minimal read request; the fake authority below ignores its contents.
    fn read_request() -> BuzzReadRequest {
        BuzzReadRequest {
            tenant_id: TenantId(uuid::Uuid::new_v4()),
            community_id: "community-1".to_string(),
            relay_url: String::new(),
            relay_pubkey: None,
            channel_id: "channel-1".to_string(),
            channel_kind: BuzzChannelKind::Workspace,
            message_id: Some("a".repeat(64)),
            pubkey: "a".repeat(64),
            event_created_at: chrono::Utc::now(),
        }
    }

    /// A message ref shaped like the gate's; only used by the error log.
    fn message_ref() -> ResourceRef {
        ResourceRef::new(
            ApplicationId::new(CHAT_APPLICATION_ID),
            RESOURCE_TYPE_MESSAGE,
            "a".repeat(64),
        )
    }

    /// Local fake authority returning a fixed canned outcome.
    struct FakeAuthority {
        outcome: Result<BuzzReadDecision, BuzzAuthorityError>,
    }

    #[async_trait::async_trait]
    impl BuzzAuthority for FakeAuthority {
        async fn can_read(
            &self,
            _req: &BuzzReadRequest,
        ) -> Result<BuzzReadDecision, BuzzAuthorityError> {
            match &self.outcome {
                Ok(decision) => Ok(*decision),
                // `BuzzAuthorityError` is not `Clone`; every authority error
                // maps to Deny in `gate_authority`, so a fresh transport
                // error is equivalent to the stored one.
                Err(_) => Err(BuzzAuthorityError::Transport(
                    "fake authority failure".to_string(),
                )),
            }
        }
    }

    #[tokio::test]
    async fn gate_authority_maps_allow_to_ok() {
        let authority = FakeAuthority {
            outcome: Ok(BuzzReadDecision::Allow),
        };
        assert!(gate_authority(&authority, &read_request(), &message_ref())
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn gate_authority_maps_deny_to_deny() {
        let authority = FakeAuthority {
            outcome: Ok(BuzzReadDecision::Deny),
        };
        assert_eq!(
            gate_authority(&authority, &read_request(), &message_ref()).await,
            Err(Decision::Deny)
        );
    }

    #[tokio::test]
    async fn gate_authority_maps_not_found_to_not_found() {
        let authority = FakeAuthority {
            outcome: Ok(BuzzReadDecision::NotFound),
        };
        assert_eq!(
            gate_authority(&authority, &read_request(), &message_ref()).await,
            Err(Decision::NotFound)
        );
    }

    #[tokio::test]
    async fn gate_authority_maps_error_to_deny() {
        let authority = FakeAuthority {
            outcome: Err(BuzzAuthorityError::Transport(
                "relay unreachable".to_string(),
            )),
        };
        assert_eq!(
            gate_authority(&authority, &read_request(), &message_ref()).await,
            Err(Decision::Deny)
        );
    }
}
