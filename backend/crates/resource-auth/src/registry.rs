//! [`ResourceOwnerRegistry`] — resolves an Application ID to its source owner adapter.

use crate::contract::ResourceOwner;
use rustshare_core::domain::ApplicationId;
use std::collections::HashMap;
use std::sync::Arc;

/// Registration errors for the owner registry.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistrationError {
    #[error("an owner is already registered for application `{0}`")]
    Duplicate(ApplicationId),
}

/// Maps an Application ID to the explicit adapter implementing the
/// source-authorization contract for that Application.
///
/// This is a typed contract registry, not a generic service locator: every
/// entry is a [`ResourceOwner`] and the Application identity is fixed at
/// registration. Core never queries owner tables itself.
pub struct ResourceOwnerRegistry {
    owners: HashMap<ApplicationId, Arc<dyn ResourceOwner>>,
}

impl std::fmt::Debug for ResourceOwnerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceOwnerRegistry")
            .field(
                "applications",
                &self
                    .owners
                    .keys()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Default for ResourceOwnerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceOwnerRegistry {
    pub fn new() -> Self {
        Self {
            owners: HashMap::new(),
        }
    }

    /// Register an owner adapter. Rejecting duplicates avoids silently
    /// replacing one Application's authority with another.
    pub fn register(&mut self, owner: Arc<dyn ResourceOwner>) -> Result<(), RegistrationError> {
        let application_id = owner.application_id().clone();
        if self.owners.contains_key(&application_id) {
            return Err(RegistrationError::Duplicate(application_id));
        }
        self.owners.insert(application_id, owner);
        Ok(())
    }

    /// Look up the owner adapter for an Application. `None` for unknown
    /// Applications — callers must fail closed.
    pub fn owner(&self, application: &ApplicationId) -> Option<Arc<dyn ResourceOwner>> {
        self.owners.get(application).cloned()
    }

    /// All registered Application IDs (for diagnostics/audit).
    pub fn applications(&self) -> impl Iterator<Item = &ApplicationId> {
        self.owners.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{Purpose, Representation, ResolvedResource, SourceError};
    use crate::decision::Decision;
    use crate::principal::PrincipalContext;
    use crate::resource_ref::ResourceRef;
    use async_trait::async_trait;
    use bytes::Bytes;
    use rustshare_core::domain::ActionCapability;

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
            _ctx: &PrincipalContext,
            _action: &ActionCapability,
            resource: &ResourceRef,
        ) -> Decision {
            if resource.resource_id == "allow" {
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
        ) -> Vec<crate::decision::BatchDecision> {
            let mut decisions = Vec::with_capacity(resources.len());
            for resource in resources {
                decisions.push(crate::decision::BatchDecision::new(
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
        ) -> Result<crate::contract::FetchedResource, SourceError> {
            Ok(crate::contract::FetchedResource {
                resource: resource.clone(),
                representation: Representation::Text,
                media_type: None,
                size: Some(resource.resource_id.len() as i64),
                data: Bytes::from(format!("content:{}", resource.resource_id)),
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

    #[test]
    fn registers_and_resolves_owners() {
        let mut registry = ResourceOwnerRegistry::new();
        let app = ApplicationId::new("io.elembra.files");
        let owner = Arc::new(FakeOwner {
            application_id: app.clone(),
        });
        registry.register(owner).unwrap();
        assert!(registry.owner(&app).is_some());
        assert!(registry
            .owner(&ApplicationId::new("io.elembra.mail"))
            .is_none());
        assert_eq!(registry.applications().collect::<Vec<_>>(), vec![&app]);
    }

    #[test]
    fn rejects_duplicate_owners() {
        let mut registry = ResourceOwnerRegistry::new();
        let app = ApplicationId::new("io.elembra.files");
        registry
            .register(Arc::new(FakeOwner {
                application_id: app.clone(),
            }))
            .unwrap();
        assert_eq!(
            registry.register(Arc::new(FakeOwner {
                application_id: app.clone()
            })),
            Err(RegistrationError::Duplicate(app))
        );
    }
}
