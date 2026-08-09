//! [`SourceAuthorizer`] — the Platform-Core-side facade over the
//! source-authorization contract.
//!
//! Core responsibilities kept here:
//!
//! * syntactic validation of [`ResourceRef`]s (fail closed on malformed refs);
//! * routing a ref to the adapter registered for its owning Application;
//! * bounded batch authorization with explicit ref/decision association;
//! * the Search/RAG proof contract: materialize candidates only after source
//!   reauthorization, and never let stale/malicious index text into context.
//!
//! Delegation validity and resource-level authorization are enforced by each
//! owner adapter through the shared [`PrincipalContext::effective_user_authority`]
//! helper — the owner remains the final authority.

use crate::contract::{
    check_delegation, Candidate, FetchedResource, MaterializedCandidate, Purpose, Representation,
    ResolvedResource, ResourceOwner, SourceError, MAX_BATCH_SIZE,
};
use crate::decision::{BatchDecision, Decision};
use crate::principal::PrincipalContext;
use crate::registry::ResourceOwnerRegistry;
use crate::resource_ref::ResourceRef;
use rustshare_core::domain::ActionCapability;
use tracing::warn;

/// Platform-Core facade over the source-authorization contract.
pub struct SourceAuthorizer {
    registry: ResourceOwnerRegistry,
}

impl SourceAuthorizer {
    /// Build the facade from an owner registry (see
    /// `rustshare_server::authz::build_source_authorizer` for the seeded set).
    pub fn new(registry: ResourceOwnerRegistry) -> Self {
        Self { registry }
    }

    /// An authorizer with no registered owners (tests/standalone use).
    pub fn empty() -> Self {
        Self::new(ResourceOwnerRegistry::new())
    }

    /// Direct access to a registered owner adapter (diagnostics/tests).
    pub fn owner(
        &self,
        application: &rustshare_core::domain::ApplicationId,
    ) -> Option<std::sync::Arc<dyn ResourceOwner>> {
        self.registry.owner(application)
    }

    /// Authorize one action on one resource.
    ///
    /// Malformed refs and unknown Applications yield [`Decision::Invalid`];
    /// the owner decides allow/deny/not-found using its authoritative rules.
    pub async fn authorize(
        &self,
        ctx: &PrincipalContext,
        action: &ActionCapability,
        resource: &ResourceRef,
    ) -> Decision {
        if let Err(error) = resource.validate() {
            warn!(%resource, %error, "rejected malformed resource ref");
            return Decision::Invalid;
        }
        let Some(owner) = self.registry.owner(&resource.application) else {
            warn!(%resource, "no owner adapter for application {}", resource.application);
            return Decision::Invalid;
        };
        owner.authorize(ctx, action, resource).await
    }

    /// Authorize an action on a bounded batch of resources.
    ///
    /// Results are explicitly associated with their refs and returned in input
    /// order. One denied/missing/invalid ref never authorizes another. A batch
    /// exceeding [`MAX_BATCH_SIZE`] is rejected outright (fail closed).
    pub async fn authorize_batch(
        &self,
        ctx: &PrincipalContext,
        action: &ActionCapability,
        resources: &[ResourceRef],
    ) -> Result<Vec<BatchDecision>, SourceError> {
        if resources.len() > MAX_BATCH_SIZE {
            return Err(SourceError::BatchTooLarge {
                actual: resources.len(),
                limit: MAX_BATCH_SIZE,
            });
        }

        let mut results: Vec<Option<BatchDecision>> = vec![None; resources.len()];
        // Group refs by owning Application, preserving input order. Groups are
        // dispatched once per owner (no per-ref routing), avoiding repeated
        // Application lookups.
        let mut groups: Vec<(rustshare_core::domain::ApplicationId, Vec<usize>)> = Vec::new();
        for (index, resource) in resources.iter().enumerate() {
            if let Err(error) = resource.validate() {
                warn!(%resource, %error, "rejected malformed resource ref in batch");
                results[index] = Some(BatchDecision::new(resource.clone(), Decision::Invalid));
                continue;
            }
            match groups
                .iter_mut()
                .find(|(application, _)| application == &resource.application)
            {
                Some((_, indices)) => indices.push(index),
                None => groups.push((resource.application.clone(), vec![index])),
            }
        }

        for (application, indices) in groups {
            let Some(owner) = self.registry.owner(&application) else {
                warn!(%application, "no owner adapter for application in batch");
                for index in &indices {
                    results[*index] = Some(BatchDecision::new(
                        resources[*index].clone(),
                        Decision::Invalid,
                    ));
                }
                continue;
            };
            let group_refs: Vec<ResourceRef> = indices
                .iter()
                .map(|&index| resources[index].clone())
                .collect();
            let decisions = owner.authorize_batch(ctx, action, &group_refs).await;
            for (offset, &index) in indices.iter().enumerate() {
                // Only trust a decision whose ref identity matches the one we
                // asked about; anything else fails closed to Deny.
                let decision = decisions
                    .get(offset)
                    .filter(|batch| batch.resource == resources[index])
                    .map(|batch| batch.decision)
                    .unwrap_or(Decision::Deny);
                results[index] = Some(BatchDecision::new(resources[index].clone(), decision));
            }
        }

        Ok(resources
            .iter()
            .enumerate()
            .map(|(index, resource)| {
                results[index]
                    .take()
                    .unwrap_or_else(|| BatchDecision::new(resource.clone(), Decision::Deny))
            })
            .collect())
    }

    /// Resolve safe metadata for a resource. The owner authorizes first and
    /// must not reveal cross-tenant resource existence.
    pub async fn resolve(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        purpose: Purpose,
    ) -> Result<ResolvedResource, SourceError> {
        resource.validate().map_err(SourceError::InvalidRef)?;
        let owner = self
            .registry
            .owner(&resource.application)
            .ok_or_else(|| SourceError::UnknownApplication(resource.application.to_string()))?;
        owner.resolve(ctx, resource, purpose).await
    }

    /// Fetch an authorized representation. Content is returned only after the
    /// owner's current authorization.
    pub async fn fetch(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        representation: Representation,
    ) -> Result<FetchedResource, SourceError> {
        resource.validate().map_err(SourceError::InvalidRef)?;
        let owner = self
            .registry
            .owner(&resource.application)
            .ok_or_else(|| SourceError::UnknownApplication(resource.application.to_string()))?;
        owner.fetch(ctx, resource, representation).await
    }

    /// Generate a short-lived delivery URL only after current authorization.
    /// The URL must never be persisted as the canonical relationship.
    pub async fn fetch_delivery_url(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        purpose: Purpose,
        ttl_secs: u64,
    ) -> Result<String, SourceError> {
        resource.validate().map_err(SourceError::InvalidRef)?;
        let owner = self
            .registry
            .owner(&resource.application)
            .ok_or_else(|| SourceError::UnknownApplication(resource.application.to_string()))?;
        owner
            .fetch_delivery_url(ctx, resource, purpose, ttl_secs)
            .await
    }

    /// Search/RAG proof contract: reauthorize candidate refs with their owners
    /// and materialize **only** authorized source content.
    ///
    /// A candidate's `cached_text` (a stale or malicious index hint) never
    /// enters the output. Denied/not-found/invalid candidates are dropped; a
    /// candidate whose source authorization or fetch fails between batch
    /// authorization and fetch is omitted rather than materialized.
    ///
    /// This is the step Memory/RAG must run before assembling LLM context
    /// (ADR-0032: post-generation filtering is prohibited).
    pub async fn materialize(
        &self,
        ctx: &PrincipalContext,
        action: &ActionCapability,
        candidates: Vec<Candidate>,
    ) -> Result<Vec<MaterializedCandidate>, SourceError> {
        if candidates.len() > MAX_BATCH_SIZE {
            return Err(SourceError::BatchTooLarge {
                actual: candidates.len(),
                limit: MAX_BATCH_SIZE,
            });
        }
        let refs: Vec<ResourceRef> = candidates
            .iter()
            .map(|candidate| candidate.resource.clone())
            .collect();
        let decisions = self.authorize_batch(ctx, action, &refs).await?;

        let mut materialized = Vec::new();
        for (candidate, decision) in candidates.into_iter().zip(decisions) {
            if !decision.decision.is_allow() {
                // Denied / not-found / invalid / stale candidates are dropped;
                // their cached text never reaches context assembly.
                continue;
            }
            match self
                .resolve(ctx, &candidate.resource, Purpose::RagContext)
                .await
            {
                Ok(display) => match self
                    .fetch(ctx, &candidate.resource, Representation::Text)
                    .await
                {
                    Ok(fetched) => materialized.push(MaterializedCandidate {
                        resource: candidate.resource,
                        display,
                        data: fetched.data,
                    }),
                    Err(error) => {
                        warn!(resource = %candidate.resource, %error,
                              "authorized candidate could not be fetched; omitting source");
                    }
                },
                Err(error) => {
                    warn!(resource = %candidate.resource, %error,
                          "authorized candidate could not be resolved; omitting source");
                }
            }
        }
        Ok(materialized)
    }

    /// Delegation gate exposed for callers that want the Core-side check
    /// explicitly before dispatching (e.g. audit logging).
    pub fn verify_delegation(
        &self,
        ctx: &PrincipalContext,
        action: &ActionCapability,
        resource: &ResourceRef,
    ) -> Result<crate::principal::EffectivePrincipal, SourceError> {
        check_delegation(ctx, action, Some(resource))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::principal::{Delegation, PrincipalKind};
    use async_trait::async_trait;
    use bytes::Bytes;
    use chrono::{Duration, Utc};
    use rustshare_core::domain::{ApplicationId, PrincipalId};
    use uuid::Uuid;

    struct FakeOwner {
        application_id: ApplicationId,
    }

    #[async_trait]
    impl ResourceOwner for FakeOwner {
        fn application_id(&self) -> &ApplicationId {
            &self.application_id
        }

        async fn authorize(
            &self,
            ctx: &PrincipalContext,
            action: &ActionCapability,
            resource: &ResourceRef,
        ) -> Decision {
            // Mirror the real owner contract: delegation bounds first.
            if ctx
                .effective_user_authority(action, Some(resource))
                .is_err()
            {
                return Decision::Deny;
            }
            if resource.resource_id.starts_with("allow-") {
                Decision::Allow
            } else {
                Decision::Deny
            }
        }

        async fn authorize_batch(
            &self,
            ctx: &PrincipalContext,
            action: &ActionCapability,
            resources: &[ResourceRef],
        ) -> Vec<BatchDecision> {
            let mut decisions = Vec::with_capacity(resources.len());
            for resource in resources {
                decisions.push(BatchDecision::new(
                    resource.clone(),
                    self.authorize(ctx, action, resource).await,
                ));
            }
            decisions
        }

        async fn resolve(
            &self,
            _ctx: &PrincipalContext,
            resource: &ResourceRef,
            _purpose: Purpose,
        ) -> Result<ResolvedResource, SourceError> {
            Ok(ResolvedResource {
                resource: resource.clone(),
                display_name: resource.resource_id.clone(),
                media_type: None,
                size: None,
                updated_at: None,
                available: true,
            })
        }

        async fn fetch(
            &self,
            _ctx: &PrincipalContext,
            resource: &ResourceRef,
            _representation: Representation,
        ) -> Result<FetchedResource, SourceError> {
            Ok(FetchedResource {
                resource: resource.clone(),
                representation: Representation::Text,
                media_type: None,
                size: Some(resource.resource_id.len() as i64),
                data: Bytes::from(format!("real-content:{}", resource.resource_id)),
            })
        }

        async fn fetch_delivery_url(
            &self,
            _ctx: &PrincipalContext,
            _resource: &ResourceRef,
            _purpose: Purpose,
            _ttl_secs: u64,
        ) -> Result<String, SourceError> {
            Ok("https://delivery.example.invalid/blob".into())
        }
    }

    fn files_app() -> ApplicationId {
        ApplicationId::new("io.elembra.files")
    }

    fn ref_for(id: &str) -> ResourceRef {
        ResourceRef::new(files_app(), "file", id)
    }

    fn user_ctx() -> PrincipalContext {
        PrincipalContext::user(
            PrincipalId(Uuid::new_v4()),
            rustshare_core::domain::TenantId(Uuid::new_v4()),
            rustshare_core::domain::WorkspaceId(Uuid::new_v4()),
        )
    }

    fn authorizer() -> SourceAuthorizer {
        let mut registry = ResourceOwnerRegistry::new();
        registry
            .register(std::sync::Arc::new(FakeOwner {
                application_id: files_app(),
            }))
            .unwrap();
        SourceAuthorizer::new(registry)
    }

    fn read_action() -> ActionCapability {
        ActionCapability::new(crate::FILES_READ)
    }

    #[tokio::test]
    async fn authorize_routes_allow_and_deny() {
        let authorizer = authorizer();
        let ctx = user_ctx();
        assert!(authorizer
            .authorize(&ctx, &read_action(), &ref_for("allow-1"))
            .await
            .is_allow());
        assert_eq!(
            authorizer
                .authorize(&ctx, &read_action(), &ref_for("deny-1"))
                .await,
            Decision::Deny
        );
    }

    #[tokio::test]
    async fn malformed_ref_and_unknown_application_fail_closed() {
        let authorizer = authorizer();
        let ctx = user_ctx();
        let malformed = ResourceRef {
            application: files_app(),
            resource_type: "FILE".into(),
            resource_id: "x".into(),
            version: None,
        };
        assert_eq!(
            authorizer.authorize(&ctx, &read_action(), &malformed).await,
            Decision::Invalid
        );
        let unknown = ref_for("allow-1").with_version("no-separator");
        assert_eq!(
            authorizer.authorize(&ctx, &read_action(), &unknown).await,
            Decision::Invalid
        );
        let other_app = ResourceRef::new(ApplicationId::new("io.elembra.mail"), "file", "allow-1");
        assert_eq!(
            authorizer.authorize(&ctx, &read_action(), &other_app).await,
            Decision::Invalid
        );
    }

    #[tokio::test]
    async fn batch_preserves_ref_association_and_order() {
        let authorizer = authorizer();
        let ctx = user_ctx();
        let refs = vec![ref_for("allow-1"), ref_for("deny-1"), ref_for("allow-2")];
        let decisions = authorizer
            .authorize_batch(&ctx, &read_action(), &refs)
            .await
            .unwrap();
        assert_eq!(decisions.len(), 3);
        // Every decision carries its own ref.
        for (decision, reference) in decisions.iter().zip(refs.iter()) {
            assert_eq!(&decision.resource, reference);
        }
        assert_eq!(decisions[0].decision, Decision::Allow);
        assert_eq!(decisions[1].decision, Decision::Deny);
        assert_eq!(decisions[2].decision, Decision::Allow);
    }

    #[tokio::test]
    async fn batch_one_malformed_ref_does_not_authorize_others() {
        let authorizer = authorizer();
        let ctx = user_ctx();
        let malformed = ResourceRef {
            application: files_app(),
            resource_type: "file".into(),
            resource_id: "allow-1".into(),
            version: Some("bad version".into()),
        };
        let refs = vec![ref_for("allow-1"), malformed.clone()];
        let decisions = authorizer
            .authorize_batch(&ctx, &read_action(), &refs)
            .await
            .unwrap();
        assert_eq!(decisions[0].decision, Decision::Allow);
        assert_eq!(decisions[1].resource, malformed);
        assert_eq!(decisions[1].decision, Decision::Invalid);
    }

    #[tokio::test]
    async fn batch_rejects_oversized_input() {
        let authorizer = authorizer();
        let ctx = user_ctx();
        let refs: Vec<ResourceRef> = (0..=MAX_BATCH_SIZE)
            .map(|i| ref_for(&format!("allow-{i}")))
            .collect();
        assert!(matches!(
            authorizer
                .authorize_batch(&ctx, &read_action(), &refs)
                .await,
            Err(SourceError::BatchTooLarge {
                actual: 65,
                limit: 64
            })
        ));
    }

    #[tokio::test]
    async fn service_identity_alone_cannot_bypass_principal() {
        let authorizer = authorizer();
        let service = PrincipalContext {
            principal_id: PrincipalId(Uuid::new_v4()),
            principal_kind: PrincipalKind::Service,
            tenant_id: rustshare_core::domain::TenantId(Uuid::new_v4()),
            workspace_id: rustshare_core::domain::WorkspaceId(Uuid::new_v4()),
            group_ids: vec![],
            grants: vec![],
            authentication: None,
            delegation: None,
            workload_identity: Some(crate::principal::WorkloadIdentity {
                application_id: Some(ApplicationId::new("io.elembra.memory")),
                subject: Some("memory-index-worker".into()),
            }),
            correlation_id: None,
        };
        assert_eq!(
            authorizer
                .authorize(&service, &read_action(), &ref_for("allow-1"))
                .await,
            Decision::Deny
        );
        let decisions = authorizer
            .authorize_batch(&service, &read_action(), &[ref_for("allow-1")])
            .await
            .unwrap();
        assert_eq!(decisions[0].decision, Decision::Deny);
    }

    #[tokio::test]
    async fn agent_delegation_is_bounded_by_actions() {
        let authorizer = authorizer();
        let issuer = PrincipalId(Uuid::new_v4());
        let agent = PrincipalId(Uuid::new_v4());
        let context = PrincipalContext {
            principal_id: agent,
            principal_kind: PrincipalKind::Agent,
            tenant_id: rustshare_core::domain::TenantId(Uuid::new_v4()),
            workspace_id: rustshare_core::domain::WorkspaceId(Uuid::new_v4()),
            group_ids: vec![],
            grants: vec![],
            authentication: None,
            delegation: Some(Delegation {
                issuer_principal_id: issuer,
                delegate_principal_id: agent,
                actions: vec![read_action()],
                workspace_id: None,
                resource_scope: None,
                expires_at: None,
                grant_id: Some("grant-1".into()),
            }),
            workload_identity: None,
            correlation_id: None,
        };
        // Reading is delegated -> the owner evaluates the issuer's authority.
        assert!(authorizer
            .authorize(&context, &read_action(), &ref_for("allow-1"))
            .await
            .is_allow());
        // Writing is not delegated -> fail closed.
        assert_eq!(
            authorizer
                .authorize(
                    &context,
                    &ActionCapability::new(crate::FILES_WRITE),
                    &ref_for("allow-1")
                )
                .await,
            Decision::Deny
        );
        // Expired delegation fails closed.
        let mut expired = context.clone();
        expired.delegation.as_mut().unwrap().expires_at = Some(Utc::now() - Duration::seconds(1));
        assert_eq!(
            authorizer
                .authorize(&expired, &read_action(), &ref_for("allow-1"))
                .await,
            Decision::Deny
        );
    }

    #[tokio::test]
    async fn materialize_never_leaks_denied_candidate_text() {
        let authorizer = authorizer();
        let ctx = user_ctx();
        let candidates = vec![
            Candidate {
                resource: ref_for("allow-1"),
                cached_text: Some("allowed cached hint".into()),
            },
            Candidate {
                resource: ref_for("deny-1"),
                cached_text: Some("ATTACKER SECRET FROM STALE INDEX".into()),
            },
            Candidate {
                resource: ResourceRef::new(
                    ApplicationId::new("io.elembra.mail"),
                    "file",
                    "allow-2",
                ),
                cached_text: Some("unknown app secret".into()),
            },
            Candidate {
                resource: ref_for("deny-2"),
                cached_text: None,
            },
        ];
        let materialized = authorizer
            .materialize(&ctx, &read_action(), candidates)
            .await
            .unwrap();
        assert_eq!(materialized.len(), 1);
        assert_eq!(materialized[0].resource, ref_for("allow-1"));
        // The materialized content is the real fetched source content, never
        // the cached/stale hint.
        assert_eq!(materialized[0].data, Bytes::from("real-content:allow-1"));
        let output = String::from_utf8_lossy(&materialized[0].data);
        assert!(!output.contains("ATTACKER SECRET"));
        assert!(!output.contains("allowed cached hint"));
    }
    // Temporary probe: appended into authorizer.rs tests
}
