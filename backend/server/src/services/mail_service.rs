use std::sync::Arc;

use rustshare_core::domain::{MailAttachment, MailMessage, MailMessagePart};
use rustshare_storage::MetadataStore;
use uuid::Uuid;

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("Mail message not found: {0}")]
    NotFound(uuid::Uuid),
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Invalid mail source: {0}")]
    InvalidSource(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
}

// ============================================================================
// Service
// ============================================================================

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
