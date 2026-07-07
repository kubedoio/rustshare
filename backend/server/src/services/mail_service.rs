use std::sync::Arc;

use rustshare_core::domain::{MailAttachment, MailMessage, MailMessagePart};
use rustshare_storage::MetadataStore;
use uuid::Uuid;

#[derive(Clone)]
pub struct MailService {
    #[allow(dead_code)]
    metadata_store: Arc<MetadataStore>,
}

impl MailService {
    pub fn new(metadata_store: Arc<MetadataStore>) -> Self {
        Self { metadata_store }
    }

    /// Placeholder: list imported mail messages for a user.
    pub async fn list_messages(
        &self,
        _tenant_id: Uuid,
        _owner_id: Uuid,
    ) -> anyhow::Result<Vec<MailMessage>> {
        Ok(vec![])
    }

    /// Placeholder: get a single mail message by ID.
    pub async fn get_message(
        &self,
        _tenant_id: Uuid,
        _owner_id: Uuid,
        _message_id: Uuid,
    ) -> anyhow::Result<Option<MailMessage>> {
        Ok(None)
    }

    /// Placeholder: list parts for a message.
    pub async fn list_parts(
        &self,
        _tenant_id: Uuid,
        _message_id: Uuid,
    ) -> anyhow::Result<Vec<MailMessagePart>> {
        Ok(vec![])
    }

    /// Placeholder: list attachments for a message.
    pub async fn list_attachments(
        &self,
        _tenant_id: Uuid,
        _message_id: Uuid,
    ) -> anyhow::Result<Vec<MailAttachment>> {
        Ok(vec![])
    }
}
