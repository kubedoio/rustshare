//! Elembra Notes source-owner adapter.
//!
//! Notes are Files-backed, so every decision delegates to `NoteService`,
//! which applies the current Files authorization and tenant checks.

use crate::services::note_service::{Note, NoteError, NoteService};
use async_trait::async_trait;
use bytes::Bytes;
use rustshare_core::domain::{ActionCapability, ApplicationId};
use rustshare_resource_auth::{
    BatchDecision, Decision, FetchedResource, PrincipalContext, Purpose, Representation,
    ResolvedResource, ResourceCapability, ResourceOwner, ResourceRef, SourceError,
};
use std::sync::Arc;
use uuid::Uuid;

const NOTES_READ: &str = "notes.read";

pub struct NotesResourceOwner {
    application_id: ApplicationId,
    notes: Arc<NoteService>,
}

impl NotesResourceOwner {
    pub fn new(notes: Arc<NoteService>) -> Self {
        Self {
            application_id: ApplicationId::new("io.elembra.notes"),
            notes,
        }
    }

    pub fn declared_capabilities() -> Vec<ResourceCapability> {
        vec![ResourceCapability::new("notes.resource", &[NOTES_READ])]
    }

    async fn require_note(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
    ) -> Result<Note, SourceError> {
        if resource.application != self.application_id {
            return Err(SourceError::UnknownApplication(
                resource.application.to_string(),
            ));
        }
        if resource.resource_type != "notes.resource" {
            return Err(SourceError::UnknownResourceType {
                application: self.application_id.to_string(),
                resource_type: resource.resource_type.clone(),
            });
        }
        if resource.version.is_some() {
            return Err(SourceError::VersionUnavailable);
        }
        let user_id = ctx
            .effective_user_authority(&ActionCapability::from(NOTES_READ), Some(resource))
            .map_err(|error| SourceError::Delegation(error.to_string()))?
            .user_authority_id;
        let note_id = Uuid::parse_str(&resource.resource_id).map_err(|_| SourceError::NotFound)?;
        self.notes
            .get_note_for_resource(note_id, user_id.0, ctx.tenant_id.0)
            .await
            .map_err(map_note_error)
    }
}

#[async_trait]
impl ResourceOwner for NotesResourceOwner {
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
        if action.0 != NOTES_READ || resource.resource_type != "notes.resource" {
            return Decision::Invalid;
        }
        match self.require_note(ctx, resource).await {
            Ok(_) => Decision::Allow,
            Err(SourceError::NotFound) => Decision::NotFound,
            Err(error) => {
                tracing::warn!(%resource, %error, "Notes authorization denied");
                Decision::Deny
            }
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
            let decision = if resources.len() > rustshare_resource_auth::MAX_BATCH_SIZE {
                Decision::Deny
            } else {
                self.authorize(ctx, action, resource).await
            };
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
        let note = self.require_note(ctx, resource).await?;
        Ok(ResolvedResource {
            resource: resource.clone(),
            display_name: note.metadata.title,
            media_type: Some("text/markdown".into()),
            size: Some(note.content.len() as i64),
            updated_at: Some(note.modified_at),
            available: true,
        })
    }

    async fn fetch(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        representation: Representation,
    ) -> Result<FetchedResource, SourceError> {
        let note = self.require_note(ctx, resource).await?;
        let size = note.content.len() as i64;
        let data = match representation {
            Representation::Metadata => Bytes::new(),
            Representation::Raw | Representation::Text => Bytes::from(note.content),
            other => {
                return Err(SourceError::UnsupportedRepresentation(format!("{other:?}")));
            }
        };
        Ok(FetchedResource {
            resource: resource.clone(),
            representation,
            media_type: Some("text/markdown".into()),
            size: Some(size),
            data,
        })
    }

    async fn fetch_delivery_url(
        &self,
        _ctx: &PrincipalContext,
        _resource: &ResourceRef,
        _purpose: Purpose,
        _ttl_secs: u64,
    ) -> Result<String, SourceError> {
        Err(SourceError::UnsupportedRepresentation(
            "Notes do not provide delivery URLs".into(),
        ))
    }
}

fn map_note_error(error: NoteError) -> SourceError {
    match error {
        NoteError::NotFound(_) => SourceError::NotFound,
        NoteError::PermissionDenied => SourceError::Unauthorized,
        error => SourceError::Internal(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::ApplicationRegistry;

    #[test]
    fn declared_surface_matches_notes_manifest() {
        let registry = ApplicationRegistry::first_party().expect("first-party manifests");
        let manifest = registry
            .manifest(&ApplicationId::new("io.elembra.notes"))
            .expect("Notes manifest");
        let capabilities = NotesResourceOwner::declared_capabilities();
        assert_eq!(manifest.resources.len(), capabilities.len());
        for capability in capabilities {
            let declared = manifest
                .resources
                .iter()
                .find(|resource| resource.resource_type == capability.resource_type)
                .expect("manifest declares owner resource type");
            assert_eq!(declared.actions, capability.actions);
        }
    }
}
