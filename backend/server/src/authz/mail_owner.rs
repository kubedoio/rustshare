//! Mail source-owner adapter for imported message references.

use crate::services::{
    application_service::{ApplicationError, ApplicationService},
    mail_service::{MailError, MailService},
};
use async_trait::async_trait;
use bytes::Bytes;
use rustshare_core::domain::{ActionCapability, ApplicationId};
use rustshare_resource_auth::{
    BatchDecision, Decision, FetchedResource, PrincipalContext, Purpose, Representation,
    ResolvedResource, ResourceCapability, ResourceOwner, ResourceRef, SourceError, MAIL_READ,
    MAX_BATCH_SIZE,
};
use std::sync::Arc;
use uuid::Uuid;

const MAIL_APPLICATION: &str = "io.elembra.mail";
const MAIL_MESSAGE_RESOURCE: &str = "message";
const MAX_MEMORY_BODY_BYTES: usize = 256 * 1024;

pub struct MailResourceOwner {
    application_id: ApplicationId,
    mail: Arc<MailService>,
    applications: Arc<ApplicationService>,
}

impl MailResourceOwner {
    pub fn new(mail: Arc<MailService>, applications: Arc<ApplicationService>) -> Self {
        Self {
            application_id: ApplicationId::new(MAIL_APPLICATION),
            mail,
            applications,
        }
    }

    pub fn declared_capabilities() -> Vec<ResourceCapability> {
        vec![ResourceCapability::new(MAIL_MESSAGE_RESOURCE, &[MAIL_READ])]
    }

    async fn require_message(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
    ) -> Result<rustshare_core::domain::MailMessage, SourceError> {
        if resource.application != self.application_id {
            return Err(SourceError::UnknownApplication(
                resource.application.to_string(),
            ));
        }
        if resource.resource_type != MAIL_MESSAGE_RESOURCE {
            return Err(SourceError::UnknownResourceType {
                application: MAIL_APPLICATION.into(),
                resource_type: resource.resource_type.clone(),
            });
        }
        if resource.version.is_some() {
            return Err(SourceError::VersionUnavailable);
        }
        self.require_enabled(ctx.tenant_id.0).await?;
        let owner_id = ctx
            .effective_user_authority(&ActionCapability::from(MAIL_READ), Some(resource))
            .map_err(|error| SourceError::Delegation(error.to_string()))?
            .user_authority_id;
        let message_id =
            Uuid::parse_str(&resource.resource_id).map_err(|_| SourceError::NotFound)?;
        let message = self
            .mail
            .get_message(ctx.tenant_id.0, owner_id.0, message_id)
            .await
            .map_err(map_mail_error)?;
        if message.deleted_at.is_some() || message.source_mode == "draft" {
            return Err(SourceError::NotFound);
        }
        Ok(message)
    }

    async fn require_enabled(&self, tenant_id: Uuid) -> Result<(), SourceError> {
        match self
            .applications
            .get_application(MAIL_APPLICATION, tenant_id)
            .await
        {
            Ok(application) if application.enabled => Ok(()),
            Ok(_) | Err(ApplicationError::NotFound(_)) => Err(SourceError::NotFound),
            Err(error) => {
                tracing::warn!(%error, "Mail application enablement lookup failed");
                Err(SourceError::OwnerUnavailable)
            }
        }
    }

    async fn authorize_message(&self, ctx: &PrincipalContext, resource: &ResourceRef) -> Decision {
        match self.require_message(ctx, resource).await {
            Ok(_) => Decision::Allow,
            Err(SourceError::NotFound) => Decision::NotFound,
            Err(error) => {
                tracing::debug!(%resource, %error, "Mail message authorization denied");
                Decision::Deny
            }
        }
    }
}

#[async_trait]
impl ResourceOwner for MailResourceOwner {
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
        if action.0 != MAIL_READ || resource.resource_type != MAIL_MESSAGE_RESOURCE {
            return Decision::Invalid;
        }
        self.authorize_message(ctx, resource).await
    }

    async fn authorize_batch(
        &self,
        ctx: &PrincipalContext,
        action: &ActionCapability,
        resources: &[ResourceRef],
    ) -> Vec<BatchDecision> {
        if resources.len() > MAX_BATCH_SIZE || action.0 != MAIL_READ {
            return resources
                .iter()
                .map(|resource| {
                    BatchDecision::new(
                        resource.clone(),
                        if resources.len() > MAX_BATCH_SIZE {
                            Decision::Deny
                        } else {
                            Decision::Invalid
                        },
                    )
                })
                .collect();
        }
        let mut decisions = Vec::with_capacity(resources.len());
        for resource in resources {
            decisions.push(BatchDecision::new(
                resource.clone(),
                self.authorize_message(ctx, resource).await,
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
        let message = self.require_message(ctx, resource).await?;
        Ok(ResolvedResource {
            resource: resource.clone(),
            display_name: message
                .subject
                .filter(|subject| !subject.trim().is_empty())
                .unwrap_or_else(|| "Mail message".into()),
            media_type: Some("message/rfc822".into()),
            size: message.size_bytes,
            updated_at: Some(message.updated_at),
            available: true,
        })
    }

    async fn fetch(
        &self,
        ctx: &PrincipalContext,
        resource: &ResourceRef,
        representation: Representation,
    ) -> Result<FetchedResource, SourceError> {
        let message = self.require_message(ctx, resource).await?;
        let owner_id = ctx
            .effective_user_authority(&ActionCapability::from(MAIL_READ), Some(resource))
            .map_err(|error| SourceError::Delegation(error.to_string()))?
            .user_authority_id;
        let mut data = Vec::new();
        for part in self
            .mail
            .list_parts(ctx.tenant_id.0, owner_id.0, message.id)
            .await
            .map_err(map_mail_error)?
            .into_iter()
            .filter(|part| part.is_body && part.content_type.starts_with("text/plain"))
        {
            let (_, bytes) = self
                .mail
                .get_message_part(ctx.tenant_id.0, owner_id.0, message.id, part.id)
                .await
                .map_err(map_mail_error)?;
            if data.len().saturating_add(bytes.len()) > MAX_MEMORY_BODY_BYTES {
                return Err(SourceError::Internal(
                    "Mail body exceeds Memory fetch limit".into(),
                ));
            }
            if !data.is_empty() {
                data.push(b'\n');
            }
            data.extend_from_slice(&bytes);
        }
        if data.is_empty() {
            return Err(SourceError::VersionUnavailable);
        }
        let (data, media_type) = match representation {
            Representation::Text => (
                Bytes::from(String::from_utf8_lossy(&data).into_owned()),
                "text/plain",
            ),
            Representation::Raw => (Bytes::from(data), "text/plain"),
            other => {
                return Err(SourceError::UnsupportedRepresentation(format!("{other:?}")));
            }
        };
        let size = data.len() as i64;
        Ok(FetchedResource {
            resource: resource.clone(),
            representation,
            media_type: Some(media_type.into()),
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
            "Mail messages do not provide delivery URLs".into(),
        ))
    }
}

fn map_mail_error(error: MailError) -> SourceError {
    match error {
        MailError::NotFound(_) => SourceError::NotFound,
        MailError::PermissionDenied => SourceError::Unauthorized,
        error => SourceError::Internal(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_surface_matches_mail_manifest() {
        let registry = rustshare_core::domain::ApplicationRegistry::first_party()
            .expect("first-party manifests");
        let manifest = registry
            .manifest(&ApplicationId::new(MAIL_APPLICATION))
            .expect("Mail manifest");
        let capabilities = MailResourceOwner::declared_capabilities();
        assert!(capabilities
            .iter()
            .all(|capability| manifest
                .resources
                .iter()
                .any(
                    |resource| resource.resource_type == capability.resource_type
                        && resource.actions == capability.actions
                )));
    }
}
