//! Source-authorization adapters wiring the Platform-Core contract
//! ([`rustshare_resource_auth`]) to the owning Applications.
//!
//! Elembra Files is the first concrete implementation. Its adapter delegates
//! every resource-level decision to the existing, authoritative Files
//! permission semantics (`PermissionResolver`), tenant-scoped metadata
//! repositories and object storage — it never duplicates ACL/share rules.
//!
//! Owner registration is validated against the canonical `ApplicationRegistry`
//! (#210): the Application must exist there and its manifest must declare the
//! resource types/actions the owner serves. An unknown Application or an
//! undeclared surface is a startup failure, never a silent owner.

pub mod files_owner;

pub use files_owner::FilesResourceOwner;

use rustshare_core::domain::ApplicationRegistry;
use rustshare_core::services::PermissionResolver;
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_resource_auth::{RegistrationError, ResourceOwnerRegistry, SourceAuthorizer};
use rustshare_storage::{MetadataStore, ObjectStore};
use std::sync::Arc;

/// Build the Platform-Core source authorizer seeded with the Elembra Files
/// owner adapter — the first Application implementing the source-authorization
/// contract.
///
/// The owner is registered **against the canonical `ApplicationRegistry`**
/// (the declarative source of Application ownership truth): `io.elembra.files`
/// must exist there and its manifest must declare the `file`/`folder` resource
/// types with the `files.*` action capabilities the adapter serves. A
/// mismatch (e.g. manifest drift or an unknown Application) is a startup
/// error, not a silent registration.
pub fn build_source_authorizer(
    application_registry: Arc<ApplicationRegistry>,
    permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    permission_resolver_repository: Arc<PermissionResolverRepository>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
) -> Result<SourceAuthorizer, RegistrationError> {
    let mut registry = ResourceOwnerRegistry::new();
    registry.register(
        Arc::new(FilesResourceOwner::new(
            permission_resolver,
            permission_resolver_repository,
            metadata_store,
            object_store,
        )),
        &application_registry,
    )?;
    Ok(SourceAuthorizer::new(registry))
}
