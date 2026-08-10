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
//!   pubkey (`ChatIdentityStore::active_admission`).
//!
//! The bridge-owned observation row (`chat_observed_events`) supplies routing
//! context (community/channel) and message existence — it NEVER grants access
//! on its own. Memory-owned state (`memory_catalog`) is never imported or
//! queried here, so a stale or tampered Memory record cannot override a
//! current revocation.
//!
//! # Existence-hiding
//!
//! Unknown and tombstoned messages resolve to [`Decision::NotFound`]; every
//! non-Allow outcome on `resolve`/`fetch` surfaces as
//! [`SourceError::NotFound`], so a caller cannot distinguish "never existed"
//! from "not authorized" through those entry points. `authorize` keeps the
//! Deny/NotFound distinction for typed internal callers.
//!
//! # Signature handling
//!
//! Signatures are NOT re-verified here: the observation index is the bridge's
//! already-verified state (`signature_verified` is set by the observation
//! pipeline); this adapter trusts that boundary and never re-derives Buzz
//! signatures.

use bytes::Bytes;
use rustshare_core::domain::{ActionCapability, ApplicationId};
use rustshare_memory::event::{ChatChannelKind, ObservedEventType};
use rustshare_memory::observed::ChatObservedEvent;
use rustshare_resource_auth::{
    BatchDecision, ChatIdentityBinding, Decision, FetchedResource, PrincipalContext, Purpose,
    Representation, ResolvedResource, ResourceCapability, ResourceOwner, ResourceRef, SourceError,
    CHAT_READ, MAX_BATCH_SIZE,
};
use rustshare_storage::{ChatIdentityStore, ChatObservationStore};
use std::sync::OnceLock;

/// The Chat resource type served by this owner.
pub const RESOURCE_TYPE_MESSAGE: &str = "message";

/// The Application this owner serves.
pub const CHAT_APPLICATION_ID: &str = "io.elembra.chat";

/// The owner adapter for the `io.elembra.chat` Application.
pub struct ChatResourceOwner {
    chat_identity: ChatIdentityStore,
    observations: ChatObservationStore,
}

impl ChatResourceOwner {
    pub fn new(chat_identity: ChatIdentityStore, observations: ChatObservationStore) -> Self {
        Self {
            chat_identity,
            observations,
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
        // dm/private/excluded channels are never candidate-exposable through
        // Elembra; coarse channel filter.
        if row.channel_kind != ChatChannelKind::Workspace {
            return Err(Decision::Deny);
        }
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
        Ok(row)
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
        let mut decisions = Vec::with_capacity(resources.len());
        for resource in resources {
            let decision = self.authorize(ctx, action, resource).await;
            decisions.push(BatchDecision::new(resource.clone(), decision));
        }
        decisions
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
    use rustshare_core::domain::ApplicationRegistry;

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
}
