use std::sync::Arc;

use chrono::Utc;
use rustshare_core::domain::{MailAttachment, MailMessage, MailMessagePart, MailVisibility};
use rustshare_core::services::eml_parser::EmlParser;
use rustshare_storage::{MetadataStore, ObjectStore};
use sha2::{Digest, Sha256};
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
    metadata_store: Arc<MetadataStore>,
    object_store: Arc<ObjectStore>,
}

impl MailService {
    pub fn new(metadata_store: Arc<MetadataStore>, object_store: Arc<ObjectStore>) -> Self {
        Self {
            metadata_store,
            object_store,
        }
    }

    /// Import a raw `.eml` source into RustShare as a mail artifact.
    ///
    /// The raw source, plain-text body, HTML body, and attachment payloads are
    /// persisted as content-addressed blobs. Metadata is written to the
    /// `mail_messages`, `mail_message_parts`, and `mail_attachments` tables.
    pub async fn import_eml(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        imported_by: Uuid,
        raw_source: Vec<u8>,
    ) -> Result<MailMessage, MailError> {
        if raw_source.is_empty() {
            return Err(MailError::InvalidSource("Empty .eml source".to_string()));
        }

        let mut parsed =
            EmlParser::parse(&raw_source).map_err(|e| MailError::InvalidSource(e.to_string()))?;

        let source_hash = hex::encode(Sha256::digest(&raw_source));
        let source_key = format!("blobs/{source_hash}");
        let source_size = raw_source.len() as i64;

        self.object_store
            .put(&source_key, bytes::Bytes::from(raw_source))
            .await
            .map_err(|e| MailError::Storage(e.to_string()))?;

        let mut msg = MailMessage::new(tenant_id, owner_id, imported_by, "eml_upload");
        msg.blob_key = Some(source_key.clone());
        msg.blob_sha256 = Some(source_hash);
        msg.size_bytes = Some(source_size);
        msg.message_id = parsed.message_id.clone();
        msg.in_reply_to = parsed.in_reply_to.clone();
        msg.references = Some(parsed.references.clone()).filter(|v| !v.is_empty());
        msg.subject = parsed.subject.clone();
        msg.from_address = parsed.from.as_ref().map(|a| a.address.clone());
        msg.from_name = parsed.from.as_ref().and_then(|a| a.name.clone());
        msg.to_addresses = addresses_to_json(&parsed.to);
        msg.cc_addresses = addresses_to_json(&parsed.cc);
        msg.bcc_addresses = addresses_to_json(&parsed.bcc);
        msg.sent_at = parsed.sent_at;
        msg.has_attachments = !parsed.attachments.is_empty();
        msg.visibility = String::from(MailVisibility::Private);

        self.metadata_store
            .create_mail_message(&msg)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        let mut part_index = 0i32;

        if let Some(text) = &parsed.body_text {
            part_index = self
                .persist_body_part(tenant_id, msg.id, part_index, "text/plain", text)
                .await?;
        }

        if let Some(html) = &parsed.body_html {
            self.persist_body_part(tenant_id, msg.id, part_index, "text/html", html)
                .await?;
        }

        let attachments = std::mem::take(&mut parsed.attachments);
        for (idx, att) in attachments.into_iter().enumerate() {
            let hash = hex::encode(Sha256::digest(&att.data));
            let key = format!("blobs/{hash}");
            self.object_store
                .put(&key, bytes::Bytes::from(att.data))
                .await
                .map_err(|e| MailError::Storage(e.to_string()))?;

            let attachment = MailAttachment {
                id: Uuid::new_v4(),
                tenant_id,
                message_id: msg.id,
                file_id: None,
                filename: att.filename.unwrap_or_else(|| format!("attachment-{idx}")),
                mime_type: Some(att.mime_type),
                size_bytes: Some(att.size_bytes as i64),
                part_index: None,
                content_disposition: att.content_disposition,
                blob_key: Some(key),
                created_at: Utc::now(),
            };
            self.metadata_store
                .create_mail_attachment(&attachment)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
        }

        Ok(msg)
    }

    /// Persist a single body part (text/plain or text/html) as a content-addressed
    /// blob and a `mail_message_parts` row, then return the next part index.
    async fn persist_body_part(
        &self,
        tenant_id: Uuid,
        message_id: Uuid,
        part_index: i32,
        content_type: &str,
        body: &str,
    ) -> Result<i32, MailError> {
        let bytes = body.as_bytes().to_vec();
        let hash = hex::encode(Sha256::digest(&bytes));
        let key = format!("blobs/{hash}");
        self.object_store
            .put(&key, bytes::Bytes::from(bytes))
            .await
            .map_err(|e| MailError::Storage(e.to_string()))?;

        let part = MailMessagePart {
            id: Uuid::new_v4(),
            tenant_id,
            message_id,
            part_index,
            content_type: content_type.to_string(),
            charset: Some("utf-8".to_string()),
            blob_key: Some(key),
            blob_sha256: Some(hash),
            size_bytes: Some(body.len() as i64),
            is_body: true,
            created_at: Utc::now(),
        };
        self.metadata_store
            .create_mail_message_part(&part)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        Ok(part_index + 1)
    }

    /// Get a single imported mail message if owned by `owner_id`.
    pub async fn get_message(
        &self,
        _tenant_id: Uuid,
        owner_id: Uuid,
        message_id: Uuid,
    ) -> Result<MailMessage, MailError> {
        self.metadata_store
            .find_mail_message_by_id(message_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or(MailError::NotFound(message_id))
    }

    /// Placeholder: list imported mail messages for a user.
    pub async fn list_messages(
        &self,
        _tenant_id: Uuid,
        _owner_id: Uuid,
    ) -> anyhow::Result<Vec<MailMessage>> {
        Ok(vec![])
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

fn addresses_to_json(
    addresses: &[rustshare_core::services::eml_parser::ParsedAddress],
) -> serde_json::Value {
    serde_json::Value::Array(
        addresses
            .iter()
            .map(|a| {
                serde_json::json!({
                    "name": a.name,
                    "address": a.address,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::services::eml_parser::{EmlParser, ParsedAddress};
    use serde_json::json;

    #[test]
    fn addresses_to_json_maps_name_and_address() {
        let addresses = vec![
            ParsedAddress {
                name: Some("Alice".to_string()),
                address: "alice@example.com".to_string(),
            },
            ParsedAddress {
                name: None,
                address: "bob@example.com".to_string(),
            },
        ];

        assert_eq!(
            addresses_to_json(&addresses),
            json!([
                {"name": "Alice", "address": "alice@example.com"},
                {"name": null, "address": "bob@example.com"},
            ])
        );
    }

    #[test]
    fn parse_simple_plain_eml_populates_message_fields() {
        let raw = b"From: Alice <alice@example.com>\r\nTo: Bob <bob@example.com>\r\nSubject: Hello\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nThis is the body.\r\n";
        let parsed = EmlParser::parse(raw.as_slice()).expect("parse .eml");

        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let imported_by = Uuid::new_v4();
        let mut msg = MailMessage::new(tenant_id, owner_id, imported_by, "eml_upload");
        msg.message_id = parsed.message_id.clone();
        msg.in_reply_to = parsed.in_reply_to.clone();
        msg.references = Some(parsed.references.clone()).filter(|v| !v.is_empty());
        msg.subject = parsed.subject.clone();
        msg.from_address = parsed.from.as_ref().map(|a| a.address.clone());
        msg.from_name = parsed.from.as_ref().and_then(|a| a.name.clone());
        msg.to_addresses = addresses_to_json(&parsed.to);
        msg.cc_addresses = addresses_to_json(&parsed.cc);
        msg.bcc_addresses = addresses_to_json(&parsed.bcc);
        msg.sent_at = parsed.sent_at;
        msg.has_attachments = !parsed.attachments.is_empty();

        assert_eq!(msg.subject, Some("Hello".to_string()));
        assert_eq!(msg.from_address, Some("alice@example.com".to_string()));
        assert_eq!(msg.from_name, Some("Alice".to_string()));
        assert_eq!(
            msg.to_addresses,
            json!([{"name": "Bob", "address": "bob@example.com"}])
        );
        assert_eq!(msg.cc_addresses, json!([]));
        assert_eq!(msg.bcc_addresses, json!([]));
        assert!(!msg.has_attachments);
        assert_eq!(parsed.body_text.as_deref(), Some("This is the body.\r\n"));
    }
}
