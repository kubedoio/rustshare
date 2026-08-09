//! The source-authorization contract (`ResourceOwner`) and its payload types.
//!
//! The Application that owns a resource type implements [`ResourceOwner`] and
//! remains the final authority for resource-level access. Platform Core never
//! queries an owner's private tables; it routes through this contract.

use crate::decision::Decision;
use crate::principal::{EffectivePrincipal, PrincipalContext};
use crate::resource_ref::ResourceRef;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use rustshare_core::domain::ActionCapability;
use serde::{Deserialize, Serialize};

/// Maximum number of refs in a single batch authorization/materialization.
///
/// Owners must bound batches so no unbounded global scans occur; callers must
/// split larger candidate sets before calling the contract.
pub const MAX_BATCH_SIZE: usize = 64;

/// Purpose of a sensitive source access. Purpose does not grant authority; it
/// allows policy/audit/representation decisions (v1alpha1 spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    UserOpen,
    SearchPreview,
    MemoryIndex,
    RagContext,
    AgentTool,
    ChatUnfurl,
    Export,
}

/// Requested representation for [`ResourceOwner::fetch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Representation {
    /// Authorized source bytes.
    Raw,
    /// Authorized text (the owner returns bytes; extraction is caller-side).
    Text,
    /// Authorized safe preview metadata/rendering.
    Preview,
    /// Authorized thumbnail.
    Thumbnail,
    /// Authorized safe metadata only (no content).
    Metadata,
}

/// Authorized safe metadata for a resource (never content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedResource {
    pub resource: ResourceRef,
    pub display_name: String,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    /// Whether the requested version/current resource is available for fetch.
    pub available: bool,
}

/// Authorized content or metadata produced by [`ResourceOwner::fetch`].
///
/// Content is only ever produced after current source authorization.
#[derive(Debug, Clone, PartialEq)]
pub struct FetchedResource {
    pub resource: ResourceRef,
    pub representation: Representation,
    pub media_type: Option<String>,
    pub size: Option<i64>,
    pub data: Bytes,
}

/// A search/index candidate offered for (re)authorized materialization.
///
/// `cached_text` is a candidate hint from a potentially stale or malicious
/// index. It is never treated as source content and never enters the
/// materialization output — the source owner is reauthorized and the real
/// content is fetched only for allowed candidates.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub resource: ResourceRef,
    pub cached_text: Option<String>,
}

/// One source materialized for LLM context after source reauthorization.
#[derive(Debug, Clone)]
pub struct MaterializedCandidate {
    pub resource: ResourceRef,
    pub display: ResolvedResource,
    pub data: Bytes,
}

/// Errors raised by the source-authorization contract.
///
/// Consumers must distinguish authorization denial/not-found (do not retry as
/// infrastructure failure) from owner-unavailable (retry where appropriate,
/// but never materialize stale unauthorized content). Timeout/error defaults
/// to deny/no materialization for security-sensitive consumers such as RAG.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("invalid resource reference: {0}")]
    InvalidRef(String),
    #[error("unknown application: {0}")]
    UnknownApplication(String),
    #[error("unknown resource type `{resource_type}` for application `{application}`")]
    UnknownResourceType {
        application: String,
        resource_type: String,
    },
    #[error("action `{action}` is not supported for resource type `{resource_type}`")]
    UnsupportedAction {
        action: String,
        resource_type: String,
    },
    #[error("delegation rejected: {0}")]
    Delegation(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("resource not found")]
    NotFound,
    #[error("cross-tenant resource reference")]
    CrossTenant,
    #[error("resource owner unavailable")]
    OwnerUnavailable,
    #[error("batch of {actual} refs exceeds the limit of {limit}")]
    BatchTooLarge { actual: usize, limit: usize },
    #[error("representation {0} is not supported by this owner")]
    UnsupportedRepresentation(String),
    #[error("requested version is unavailable")]
    VersionUnavailable,
    #[error("internal source error: {0}")]
    Internal(String),
}

/// The source-authorization contract implemented by the Application that owns
/// a resource type. The owner is the final authority.
///
/// Transport is intentionally not part of this trait: an in-process adapter
/// (Files today) and a future HTTP adapter share identical semantics.
#[async_trait]
pub trait ResourceOwner: Send + Sync {
    /// The Application this owner serves.
    fn application_id(&self) -> &rustshare_core::domain::ApplicationId;

    /// Authorize one action on one resource. The owner evaluates the effective
    /// principal (delegation bounds) and its authoritative resource rules.
    async fn authorize(
        &self,
        ctx: &PrincipalContext,
        action: &ActionCapability,
        resource: &ResourceRef,
    ) -> Decision;

    /// Authorize an action on a bounded batch of resources owned by this
    /// Application. Every result is explicitly associated with its ref.
    ///
    /// One denied/missing ref never authorizes another; a batch larger than
    /// the owner's bound fails closed (every item denied). Callers must split
    /// oversized batches via the Platform-Core facade, which enforces
    /// [`MAX_BATCH_SIZE`].
    async fn authorize_batch(
        &self,
        ctx: &PrincipalContext,
        action: &ActionCapability,
        resources: &[ResourceRef],
    ) -> Vec<crate::decision::BatchDecision>;

    /// Resolve safe metadata for a resource **after** authorization. Must not
    /// become a way to enumerate cross-tenant resources.
    async fn resolve(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        purpose: Purpose,
    ) -> Result<ResolvedResource, SourceError>;

    /// Fetch an authorized representation. Content is returned only after
    /// current source authorization. Large content should stream where the
    /// transport supports it.
    async fn fetch(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        representation: Representation,
    ) -> Result<FetchedResource, SourceError>;

    /// Generate a short-lived delivery URL **only after** current source
    /// authorization. Permanent bearer/presigned URLs must never be persisted
    /// as the canonical cross-Application relationship.
    async fn fetch_delivery_url(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        purpose: Purpose,
        ttl_secs: u64,
    ) -> Result<String, SourceError>;
}

/// Shared mapping of a fail-closed policy decision into a contract error.
pub fn decision_to_source_error(decision: Decision) -> SourceError {
    match decision {
        Decision::Allow => unreachable!("Allow is not an error"),
        Decision::Deny => SourceError::Unauthorized,
        Decision::NotFound => SourceError::NotFound,
        Decision::Invalid => SourceError::InvalidRef("invalid resource reference".into()),
    }
}

/// Verify the delegation bounds of a context for an action/ref. This is the
/// Platform-Core-side delegation gate; owners apply the same check internally.
pub fn check_delegation(
    ctx: &PrincipalContext,
    action: &ActionCapability,
    resource: Option<&ResourceRef>,
) -> Result<EffectivePrincipal, SourceError> {
    ctx.effective_user_authority(action, resource)
        .map_err(|error| SourceError::Delegation(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purpose_and_representation_serialize_snake_case() {
        assert_eq!(
            serde_json::to_string(&Purpose::RagContext).unwrap(),
            "\"rag_context\""
        );
        assert_eq!(
            serde_json::to_string(&Representation::Raw).unwrap(),
            "\"raw\""
        );
    }

    #[test]
    fn fail_closed_decisions_map_to_errors() {
        assert!(matches!(
            decision_to_source_error(Decision::Deny),
            SourceError::Unauthorized
        ));
        assert!(matches!(
            decision_to_source_error(Decision::NotFound),
            SourceError::NotFound
        ));
        assert!(matches!(
            decision_to_source_error(Decision::Invalid),
            SourceError::InvalidRef(_)
        ));
    }
}
