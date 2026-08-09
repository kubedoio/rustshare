//! Elembra Files source-owner adapter.
//!
//! This adapter is the Files Application's export surface for the
//! source-authorization contract (ADR-0032). It implements
//! [`ResourceOwner`] by **delegating** to Files' existing authoritative
//! permission semantics — no ACL/share rules are duplicated here:
//!
//! * decisions go through `PermissionResolver::check_file_permission` /
//!   `check_folder_permission` (owner + direct user share + group share +
//!   folder-ancestry inheritance, exactly as the recently hardened Files
//!   handlers use);
//! * existence/tenant scoping goes through the tenant-scoped
//!   `PermissionResolverRepository` lookups (deleted files/folders resolve to
//!   not-found);
//! * content and short-lived delivery URLs come from object storage **only
//!   after** current authorization.
//!
//! `PrincipalContext::effective_user_authority` is applied before every
//! decision: a Service/Agent Principal acts only under its explicit bounded
//! delegation, and the owner evaluates the delegating user's current
//! authority (revocations apply immediately).

use rustshare_core::domain::{ActionCapability, ApplicationId, File, Folder, SharePermissions};
use rustshare_core::services::{PermissionResolver, PermissionResolverOps};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_resource_auth::contract::SourceError;
use rustshare_resource_auth::{
    Decision, FetchedResource, PrincipalContext, Purpose, Representation, ResolvedResource,
    ResourceCapability, ResourceOwner, ResourceRef, FILES_DELETE, FILES_READ, FILES_SHARE,
    FILES_WRITE,
};
use rustshare_storage::{MetadataStore, ObjectStore};
use std::fmt::Display;
use std::sync::Arc;
use uuid::Uuid;

/// Files resource types owned by this Application.
pub const RESOURCE_TYPE_FILE: &str = "file";
pub const RESOURCE_TYPE_FOLDER: &str = "folder";

/// Maximum TTL for short-lived delivery URLs (seconds).
pub const MAX_DELIVERY_URL_TTL_SECS: u64 = 900;

/// The owner adapter for the `io.elembra.files` Application.
pub struct FilesResourceOwner {
    application_id: ApplicationId,
    resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    repo: Arc<PermissionResolverRepository>,
    metadata: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
}

impl FilesResourceOwner {
    pub fn new(
        resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
        repo: Arc<PermissionResolverRepository>,
        metadata: Arc<MetadataStore>,
        object_store: Arc<ObjectStore>,
    ) -> Self {
        Self {
            application_id: ApplicationId::new("io.elembra.files"),
            resolver,
            repo,
            metadata,
            object_store,
        }
    }

    /// The resource surface this adapter serves. Registration
    /// (`authz::build_source_authorizer`) validates it against the canonical
    /// `ApplicationRegistry`: the `io.elembra.files` manifest must declare
    /// the same resource types with the same action capabilities (see the
    /// unit test at the bottom of this file).
    pub fn declared_capabilities() -> Vec<ResourceCapability> {
        vec![
            ResourceCapability::new(
                RESOURCE_TYPE_FILE,
                &[FILES_READ, FILES_WRITE, FILES_DELETE, FILES_SHARE],
            ),
            ResourceCapability::new(
                RESOURCE_TYPE_FOLDER,
                &[FILES_READ, FILES_WRITE, FILES_DELETE, FILES_SHARE],
            ),
        ]
    }

    /// Map an action capability to the existing Files permission level.
    ///
    /// `files.read -> View`, `files.write -> Edit`,
    /// `files.delete -> Admin`, `files.share -> Admin`.
    ///
    /// `files.delete`/`files.share` map to Admin, matching Files semantics
    /// where Admin is required to delete (incl. shared-Admin subtree
    /// deletion) and to manage shares. Like the legacy
    /// `resolve_permission`-based recipient share management, the resolver
    /// check includes folder-ancestry inheritance: a Principal with Admin on
    /// a parent folder authorizes `files.share` on files inside it. The
    /// legacy operation-level endpoints remain the final gate (e.g. the
    /// public-link `create_share` additionally requires file ownership);
    /// the contract decision is a pre-check and never bypasses them.
    /// Unsupported actions for the resource type return `None` (fail closed).
    fn required_permission(
        action: &ActionCapability,
        resource_type: &str,
    ) -> Option<SharePermissions> {
        if !matches!(resource_type, RESOURCE_TYPE_FILE | RESOURCE_TYPE_FOLDER) {
            return None;
        }
        match action.0.as_str() {
            FILES_READ => Some(SharePermissions::View),
            FILES_WRITE => Some(SharePermissions::Edit),
            FILES_DELETE => Some(SharePermissions::Admin),
            FILES_SHARE => Some(SharePermissions::Admin),
            _ => None,
        }
    }

    /// Resolve the acting Principal's authority and fail closed on any
    /// delegation violation.
    fn effective_user_id(
        &self,
        ctx: &PrincipalContext,
        action: &ActionCapability,
        resource: &ResourceRef,
    ) -> Option<Uuid> {
        match ctx.effective_user_authority(action, Some(resource)) {
            Ok(effective) => Some(effective.user_authority_id.0),
            Err(error) => {
                tracing::warn!(
                    application = %self.application_id,
                    principal = %ctx.principal_id,
                    action = %action,
                    resource = %resource,
                    %error,
                    "source authorization rejected: delegation not honored"
                );
                None
            }
        }
    }

    async fn authorize_file(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
        action: &ActionCapability,
        resource: &ResourceRef,
    ) -> Decision {
        let Some(required) = Self::required_permission(action, RESOURCE_TYPE_FILE) else {
            tracing::debug!(action = %action, "unsupported action for Files file");
            return Decision::Invalid;
        };
        let Ok(file_id) = Uuid::parse_str(&resource.resource_id) else {
            // Not a Files file id — the owner cannot resolve it.
            return Decision::NotFound;
        };
        match self.repo.find_file_by_id(file_id, tenant_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return Decision::NotFound,
            Err(error) => {
                tracing::error!(%error, %resource, "file existence check failed");
                return Decision::Deny;
            }
        }
        match self
            .resolver
            .check_file_permission(user_id, tenant_id, file_id, required)
            .await
        {
            Ok(true) => Decision::Allow,
            Ok(false) => Decision::Deny,
            Err(error) => {
                tracing::error!(%error, %resource, "file permission check failed");
                Decision::Deny
            }
        }
    }

    async fn authorize_folder(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
        action: &ActionCapability,
        resource: &ResourceRef,
    ) -> Decision {
        let Some(required) = Self::required_permission(action, RESOURCE_TYPE_FOLDER) else {
            tracing::debug!(action = %action, "unsupported action for Files folder");
            return Decision::Invalid;
        };
        let Ok(folder_id) = Uuid::parse_str(&resource.resource_id) else {
            return Decision::NotFound;
        };
        match self.repo.find_folder_by_id(folder_id, tenant_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return Decision::NotFound,
            Err(error) => {
                tracing::error!(%error, %resource, "folder existence check failed");
                return Decision::Deny;
            }
        }
        match self
            .resolver
            .check_folder_permission(user_id, tenant_id, folder_id, required)
            .await
        {
            Ok(true) => Decision::Allow,
            Ok(false) => Decision::Deny,
            Err(error) => {
                tracing::error!(%error, %resource, "folder permission check failed");
                Decision::Deny
            }
        }
    }

    /// Authorized lookup of a file (read). Delegation bounds are enforced
    /// first; the tenant-scoped existence lookup never reveals cross-tenant
    /// resources; the canonical resolver decides the read permission.
    async fn require_read_file(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
    ) -> Result<File, SourceError> {
        let Some(user_id) =
            self.effective_user_id(ctx, &ActionCapability::from(FILES_READ), resource)
        else {
            return Err(SourceError::Unauthorized);
        };
        let Ok(file_id) = Uuid::parse_str(&resource.resource_id) else {
            return Err(SourceError::NotFound);
        };
        // Tenant scoping below is the authoritative resource boundary.
        // RustShare maps workspace 1:1 to tenant today, so a delegation's
        // `workspace_id` bound (checked by `effective_user_authority`)
        // coincides with the tenant scope; if they ever diverge, the
        // delegation workspace bound must be re-checked against the
        // resource's actual workspace here.
        let tenant_id = ctx.tenant_id.0;
        let file = self
            .repo
            .find_file_by_id(file_id, tenant_id)
            .await
            .map_err(internal)?
            .ok_or(SourceError::NotFound)?;
        let allowed = self
            .resolver
            .check_file_permission(user_id, tenant_id, file_id, SharePermissions::View)
            .await
            .map_err(internal)?;
        if !allowed {
            return Err(SourceError::Unauthorized);
        }
        Ok(file)
    }

    /// Authorized lookup of a folder (read).
    async fn require_read_folder(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
    ) -> Result<Folder, SourceError> {
        let Some(user_id) =
            self.effective_user_id(ctx, &ActionCapability::from(FILES_READ), resource)
        else {
            return Err(SourceError::Unauthorized);
        };
        let Ok(folder_id) = Uuid::parse_str(&resource.resource_id) else {
            return Err(SourceError::NotFound);
        };
        let tenant_id = ctx.tenant_id.0;
        let folder = self
            .repo
            .find_folder_by_id(folder_id, tenant_id)
            .await
            .map_err(internal)?
            .ok_or(SourceError::NotFound)?;
        let allowed = self
            .resolver
            .check_folder_permission(user_id, tenant_id, folder_id, SharePermissions::View)
            .await
            .map_err(internal)?;
        if !allowed {
            return Err(SourceError::Unauthorized);
        }
        Ok(folder)
    }

    /// Extract the content hash from a `sha256:<64-hex>` version selector.
    /// Strict: non-hex or wrong-length selectors are rejected (fail closed)
    /// instead of silently never matching.
    fn version_hash(version: &str) -> Option<String> {
        let hex = version.strip_prefix("sha256:")?;
        if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        Some(hex.to_lowercase())
    }

    /// Whether the requested immutable version exists for a file.
    async fn version_available(
        &self,
        file: &File,
        version: Option<&str>,
    ) -> Result<bool, SourceError> {
        let Some(version) = version else {
            return Ok(true);
        };
        let Some(content_hash) = Self::version_hash(version) else {
            return Ok(false);
        };
        let versions = self
            .metadata
            .list_file_versions(file.id, file.owner_id)
            .await
            .map_err(internal)?;
        Ok(versions
            .iter()
            .any(|candidate| candidate.content_hash == content_hash))
    }

    /// Resolve the object-storage key honoring the immutable version selector.
    async fn resolve_storage_key(
        &self,
        file: &File,
        version: Option<&str>,
    ) -> Result<String, SourceError> {
        let Some(version) = version else {
            return Ok(file.storage_key());
        };
        let Some(content_hash) = Self::version_hash(version) else {
            return Err(SourceError::VersionUnavailable);
        };
        let versions = self
            .metadata
            .list_file_versions(file.id, file.owner_id)
            .await
            .map_err(internal)?;
        let matching = versions
            .into_iter()
            .find(|candidate| candidate.content_hash == content_hash)
            .ok_or(SourceError::VersionUnavailable)?;
        Ok(matching.storage_key())
    }
}

#[async_trait::async_trait]
impl ResourceOwner for FilesResourceOwner {
    fn application_id(&self) -> &ApplicationId {
        &self.application_id
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
        let Some(user_id) = self.effective_user_id(ctx, action, resource) else {
            return Decision::Deny;
        };
        let tenant_id = ctx.tenant_id.0;
        match resource.resource_type.as_str() {
            RESOURCE_TYPE_FILE => {
                self.authorize_file(user_id, tenant_id, action, resource)
                    .await
            }
            RESOURCE_TYPE_FOLDER => {
                self.authorize_folder(user_id, tenant_id, action, resource)
                    .await
            }
            other => {
                tracing::warn!(%resource, resource_type = other, "unknown Files resource type");
                Decision::Invalid
            }
        }
    }

    async fn authorize_batch(
        &self,
        ctx: &PrincipalContext,
        action: &ActionCapability,
        resources: &[ResourceRef],
    ) -> Vec<rustshare_resource_auth::BatchDecision> {
        // Oversized batches fail closed; the Platform-Core facade already
        // enforces the batch bound before dispatching.
        if resources.len() > rustshare_resource_auth::MAX_BATCH_SIZE {
            return resources
                .iter()
                .map(|resource| {
                    rustshare_resource_auth::BatchDecision::new(resource.clone(), Decision::Deny)
                })
                .collect();
        }
        let mut decisions = Vec::with_capacity(resources.len());
        for resource in resources {
            let decision = self.authorize(ctx, action, resource).await;
            decisions.push(rustshare_resource_auth::BatchDecision::new(
                resource.clone(),
                decision,
            ));
        }
        decisions
    }

    async fn resolve(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        _purpose: Purpose,
    ) -> Result<ResolvedResource, SourceError> {
        match resource.resource_type.as_str() {
            RESOURCE_TYPE_FILE => {
                let file = self.require_read_file(ctx, resource).await?;
                let available = self
                    .version_available(&file, resource.version.as_deref())
                    .await?;
                Ok(ResolvedResource {
                    resource: resource.clone(),
                    display_name: file.name.clone(),
                    media_type: Some(file.mime_type.clone()),
                    size: Some(file.size),
                    updated_at: Some(file.modified_at),
                    available,
                })
            }
            RESOURCE_TYPE_FOLDER => {
                let folder = self.require_read_folder(ctx, resource).await?;
                // Folders have no content versions; a version selector on a
                // folder ref fails closed as unavailable.
                let available = resource.version.is_none();
                Ok(ResolvedResource {
                    resource: resource.clone(),
                    display_name: folder.name.clone(),
                    media_type: None,
                    size: None,
                    updated_at: Some(folder.updated_at),
                    available,
                })
            }
            other => Err(SourceError::UnknownResourceType {
                application: self.application_id.to_string(),
                resource_type: other.into(),
            }),
        }
    }

    async fn fetch(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        representation: Representation,
    ) -> Result<FetchedResource, SourceError> {
        match representation {
            Representation::Metadata => {
                let resolved = self.resolve(ctx, resource, Purpose::UserOpen).await?;
                Ok(FetchedResource {
                    resource: resource.clone(),
                    representation,
                    media_type: resolved.media_type,
                    size: resolved.size,
                    data: bytes::Bytes::new(),
                })
            }
            Representation::Raw | Representation::Text => {
                if resource.resource_type != RESOURCE_TYPE_FILE {
                    return Err(SourceError::UnsupportedRepresentation(
                        "folder has no fetchable content".into(),
                    ));
                }
                let file = self.require_read_file(ctx, resource).await?;
                let storage_key = self
                    .resolve_storage_key(&file, resource.version.as_deref())
                    .await?;
                let data = self
                    .object_store
                    .get(&storage_key)
                    .await
                    .map_err(internal)?;
                Ok(FetchedResource {
                    resource: resource.clone(),
                    representation,
                    media_type: Some(file.mime_type.clone()),
                    size: Some(file.size),
                    data,
                })
            }
            Representation::Preview | Representation::Thumbnail => Err(
                SourceError::UnsupportedRepresentation(format!("{representation:?}")),
            ),
        }
    }

    async fn fetch_delivery_url(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        _purpose: Purpose,
        ttl_secs: u64,
    ) -> Result<String, SourceError> {
        if resource.resource_type != RESOURCE_TYPE_FILE {
            return Err(SourceError::UnsupportedRepresentation(
                "folder has no delivery URL".into(),
            ));
        }
        let file = self.require_read_file(ctx, resource).await?;
        let storage_key = self
            .resolve_storage_key(&file, resource.version.as_deref())
            .await?;
        // Short-lived, generated only after current authorization.
        let ttl = ttl_secs.clamp(1, MAX_DELIVERY_URL_TTL_SECS);
        let url = self
            .object_store
            .get_presigned_url(&storage_key, ttl)
            .await
            .map_err(internal)?;
        Ok(url)
    }
}

fn internal(error: impl Display) -> SourceError {
    SourceError::Internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::ApplicationRegistry;

    /// The adapter's declared resource/action surface must exactly match the
    /// `io.elembra.files` manifest in the canonical ApplicationRegistry —
    /// this is the invariant `ResourceOwnerRegistry::register` enforces at
    /// bootstrap.
    #[test]
    fn declared_surface_matches_application_registry_manifest() {
        let registry = ApplicationRegistry::first_party().expect("first-party manifests are valid");
        let manifest = registry
            .manifest(&ApplicationId::new("io.elembra.files"))
            .expect("the io.elembra.files manifest is present");
        let surface = FilesResourceOwner::declared_capabilities();
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
