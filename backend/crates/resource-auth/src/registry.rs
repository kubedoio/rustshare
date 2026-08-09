//! [`ResourceOwnerRegistry`] — resolves an Application ID to its source owner adapter.
//!
//! The `ApplicationRegistry` (#210) is the declarative source of Application
//! ownership truth. This registry is only the runtime binding layer: every
//! owner is validated against the ApplicationRegistry at registration and an
//! owner that claims an unknown Application, an undeclared resource type or
//! an undeclared action is rejected instead of silently becoming an owner.

use crate::contract::{ResourceCapability, ResourceOwner};
use rustshare_core::domain::{ApplicationId, ApplicationRegistry};
use std::collections::HashMap;
use std::sync::Arc;

/// Registration errors for the owner registry.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistrationError {
    #[error("an owner is already registered for application `{0}`")]
    Duplicate(ApplicationId),
    #[error("application `{0}` is not registered in the ApplicationRegistry")]
    UnknownApplication(ApplicationId),
    #[error(
        "application `{application}` manifest does not declare resource type `{resource_type}`"
    )]
    UndeclaredResourceType {
        application: ApplicationId,
        resource_type: String,
    },
    #[error("application `{application}` manifest does not declare action `{action}` for resource type `{resource_type}`")]
    UndeclaredAction {
        application: ApplicationId,
        resource_type: String,
        action: String,
    },
}

/// Maps an Application ID to the explicit adapter implementing the
/// source-authorization contract for that Application.
///
/// This is a typed contract registry, not a generic service locator: every
/// entry is a [`ResourceOwner`], the Application identity is fixed at
/// registration, and the registration is validated against the canonical
/// [`ApplicationRegistry`] (the owning Application must exist and declare the
/// resource types/actions the owner serves). Core never queries owner tables
/// itself.
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

    /// Register an owner adapter, validated against the canonical
    /// `ApplicationRegistry`:
    ///
    /// * the Application must exist in the registry (an unknown Application is
    ///   a startup/registration failure, never a silent owner);
    /// * the manifest must declare every resource type the owner serves;
    /// * every action capability the owner exposes on a resource type must be
    ///   declared for that resource type in the manifest.
    ///
    /// Rejecting duplicates avoids silently replacing one Application's
    /// authority with another.
    pub fn register(
        &mut self,
        owner: Arc<dyn ResourceOwner>,
        application_registry: &ApplicationRegistry,
    ) -> Result<(), RegistrationError> {
        let application_id = owner.application_id().clone();
        if self.owners.contains_key(&application_id) {
            return Err(RegistrationError::Duplicate(application_id));
        }
        let manifest = application_registry
            .manifest(&application_id)
            .ok_or_else(|| RegistrationError::UnknownApplication(application_id.clone()))?;
        for capability in owner.resource_capabilities() {
            self.validate_capability(&application_id, manifest, &capability)?;
        }
        self.owners.insert(application_id, owner);
        Ok(())
    }

    fn validate_capability(
        &self,
        application: &ApplicationId,
        manifest: &rustshare_core::domain::ApplicationManifest,
        capability: &ResourceCapability,
    ) -> Result<(), RegistrationError> {
        let declared = manifest
            .resources
            .iter()
            .find(|resource| resource.resource_type == capability.resource_type)
            .ok_or_else(|| RegistrationError::UndeclaredResourceType {
                application: application.clone(),
                resource_type: capability.resource_type.clone(),
            })?;
        for action in &capability.actions {
            if !declared.actions.contains(action) {
                return Err(RegistrationError::UndeclaredAction {
                    application: application.clone(),
                    resource_type: capability.resource_type.clone(),
                    action: action.0.clone(),
                });
            }
        }
        Ok(())
    }

    /// Look up the owner adapter for an Application. `None` for unknown
    /// Applications — callers must fail closed.
    ///
    /// This method is only meant for the [`SourceAuthorizer`](crate::SourceAuthorizer)
    /// facade, which applies workspace/tenant scope validation and `ResourceRef`
    /// validation before consulting an owner. Direct callers bypass those
    /// guards; new call sites must go through `SourceAuthorizer` instead.
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
    use rustshare_core::domain::{ActionCapability, ApplicationRegistry};

    /// The canonical `io.elembra.files` resource surface (matches the Files
    /// adapter and the first-party manifest).
    fn files_capabilities() -> Vec<ResourceCapability> {
        vec![
            ResourceCapability::new(
                "file",
                &["files.read", "files.write", "files.delete", "files.share"],
            ),
            ResourceCapability::new(
                "folder",
                &["files.read", "files.write", "files.delete", "files.share"],
            ),
        ]
    }

    fn first_party() -> ApplicationRegistry {
        ApplicationRegistry::first_party().expect("first-party manifests are valid")
    }

    struct FakeOwner {
        application_id: ApplicationId,
        capabilities: Vec<ResourceCapability>,
    }

    #[async_trait]
    impl ResourceOwner for FakeOwner {
        fn application_id(&self) -> &ApplicationId {
            &self.application_id
        }

        fn resource_capabilities(&self) -> Vec<ResourceCapability> {
            self.capabilities.clone()
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
            capabilities: files_capabilities(),
        });
        registry.register(owner, &first_party()).unwrap();
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
            .register(
                Arc::new(FakeOwner {
                    application_id: app.clone(),
                    capabilities: files_capabilities(),
                }),
                &first_party(),
            )
            .unwrap();
        assert_eq!(
            registry.register(
                Arc::new(FakeOwner {
                    application_id: app.clone(),
                    capabilities: files_capabilities(),
                }),
                &first_party(),
            ),
            Err(RegistrationError::Duplicate(app))
        );
    }

    /// A valid Files owner registration succeeds against the canonical
    /// first-party ApplicationRegistry.
    #[test]
    fn valid_files_owner_registration_succeeds() {
        let mut registry = ResourceOwnerRegistry::new();
        let app = ApplicationId::new("io.elembra.files");
        let owner = Arc::new(FakeOwner {
            application_id: app.clone(),
            capabilities: files_capabilities(),
        });
        assert!(registry.register(owner, &first_party()).is_ok());
        assert!(registry.owner(&app).is_some());
    }

    /// An owner claiming an Application absent from the ApplicationRegistry is
    /// rejected at registration — it must never silently become an owner.
    #[test]
    fn unknown_application_registration_fails() {
        let mut registry = ResourceOwnerRegistry::new();
        let ghost = ApplicationId::new("io.elembra.ghost");
        let owner = Arc::new(FakeOwner {
            application_id: ghost.clone(),
            capabilities: files_capabilities(),
        });
        assert_eq!(
            registry.register(owner, &first_party()),
            Err(RegistrationError::UnknownApplication(ghost))
        );
        assert!(registry.applications().next().is_none());
    }

    /// An owner cannot claim a resource type its Application's manifest does
    /// not declare.
    #[test]
    fn owner_cannot_claim_undeclared_resource_type() {
        let mut registry = ResourceOwnerRegistry::new();
        // `io.elembra.notes` exists in the first-party registry but only
        // declares the `notes.resource` type.
        let notes = ApplicationId::new("io.elembra.notes");
        let owner = Arc::new(FakeOwner {
            application_id: notes.clone(),
            capabilities: vec![ResourceCapability::new("file", &["files.read"])],
        });
        assert_eq!(
            registry.register(owner, &first_party()),
            Err(RegistrationError::UndeclaredResourceType {
                application: notes,
                resource_type: "file".into(),
            })
        );
    }

    /// An owner cannot claim an action capability its Application's manifest
    /// does not declare for the resource type.
    #[test]
    fn owner_cannot_claim_undeclared_action() {
        let mut registry = ResourceOwnerRegistry::new();
        let files = ApplicationId::new("io.elembra.files");
        let owner = Arc::new(FakeOwner {
            application_id: files.clone(),
            capabilities: vec![ResourceCapability::new(
                "file",
                &["files.read", "mail.read"],
            )],
        });
        assert_eq!(
            registry.register(owner, &first_party()),
            Err(RegistrationError::UndeclaredAction {
                application: files,
                resource_type: "file".into(),
                action: "mail.read".into(),
            })
        );
    }

    /// The declared Files resource/action surface exactly matches the manifest
    /// in the canonical registry (the invariant registration relies on).
    ///
    /// Note: `register()` accepts a subset of the manifest's actions, so this
    /// exact-match assertion is stricter than enforcement — it deliberately
    /// locks the Files adapter and the first-party manifest to identical
    /// surfaces so manifest drift fails loudly in tests, not silently in
    /// production.
    #[test]
    fn declared_files_surface_matches_manifest() {
        let registry = first_party();
        let manifest = registry
            .manifest(&ApplicationId::new("io.elembra.files"))
            .expect("files manifest is present");
        let surface = files_capabilities();
        assert_eq!(
            manifest.resources.len(),
            surface.len(),
            "manifest and adapter must declare the same resource types"
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
