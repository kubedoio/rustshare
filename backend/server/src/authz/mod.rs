//! Source-authorization adapters wiring the Platform-Core contract
//! ([`rustshare_resource_auth`]) to the owning Applications.
//!
//! Elembra Files is the first concrete implementation. Its adapter delegates
//! every resource-level decision to the existing, authoritative Files
//! permission semantics (`PermissionResolver`), tenant-scoped metadata
//! repositories and object storage — it never duplicates ACL/share rules.

pub mod files_owner;

pub use files_owner::FilesResourceOwner;

use rustshare_core::services::PermissionResolver;
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_resource_auth::{ResourceOwnerRegistry, SourceAuthorizer};
use rustshare_storage::{MetadataStore, ObjectStore};
use std::sync::Arc;

/// Build the Platform-Core source authorizer seeded with the Elembra Files
/// owner adapter — the first Application implementing the source-authorization
/// contract. Registered by `ApplicationRegistry`-known identity
/// (`io.elembra.files`) and resolved through the typed owner registry.
pub fn build_source_authorizer(
    permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    permission_resolver_repository: Arc<PermissionResolverRepository>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
) -> SourceAuthorizer {
    let mut registry = ResourceOwnerRegistry::new();
    registry
        .register(Arc::new(FilesResourceOwner::new(
            permission_resolver,
            permission_resolver_repository,
            metadata_store,
            object_store,
        )))
        .expect("the io.elembra.files owner is registered exactly once");
    SourceAuthorizer::new(registry)
}
