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

pub mod chat_owner;
pub mod files_owner;

pub use chat_owner::ChatResourceOwner;
pub use files_owner::FilesResourceOwner;

use rustshare_core::domain::ApplicationRegistry;
use rustshare_core::services::PermissionResolver;
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_resource_auth::{
    BuzzAuthority, RegistrationError, ResourceOwnerRegistry, SourceAuthorizer,
};
use rustshare_storage::{ChatIdentityStore, ChatObservationStore, MetadataStore, ObjectStore};
use std::sync::Arc;

/// Build the Platform-Core source authorizer seeded with the Elembra Files
/// and Elembra Chat owner adapters — the Applications implementing the
/// source-authorization contract.
///
/// Each owner is registered **against the canonical `ApplicationRegistry`**
/// (the declarative source of Application ownership truth): `io.elembra.files`
/// must exist there and its manifest must declare the `file`/`folder` resource
/// types with the `files.*` action capabilities, and `io.elembra.chat` must
/// declare the `message` resource type with `chat.read`. A mismatch (e.g.
/// manifest drift or an unknown Application) is a startup error, not a silent
/// registration.
#[allow(clippy::too_many_arguments)]
pub fn build_source_authorizer(
    application_registry: Arc<ApplicationRegistry>,
    permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    permission_resolver_repository: Arc<PermissionResolverRepository>,
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
    chat_identity_store: ChatIdentityStore,
    chat_observation_store: ChatObservationStore,
    buzz_authority: Box<dyn BuzzAuthority>,
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
    // The Chat owner evaluates CURRENT Chat/Buzz admission/binding/enablement
    // state only, then defers the FINAL channel/message decision to the
    // configured Buzz authority; the observation store supplies routing
    // context and message existence, never an allow, and Memory-owned state
    // is never consulted.
    registry.register(
        Arc::new(ChatResourceOwner::with_authority(
            chat_identity_store,
            chat_observation_store,
            buzz_authority,
        )),
        &application_registry,
    )?;
    Ok(SourceAuthorizer::new(registry))
}
