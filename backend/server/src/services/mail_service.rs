use std::sync::Arc;

use chrono::Utc;
use rustshare_core::domain::{
    Folder, MailAttachment, MailMessage, MailMessagePart, MailSourceMode, MailVisibility, UserId,
};
use rustshare_core::services::eml_parser::EmlParser;
use rustshare_core::services::{FileService, FolderService};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
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
    file_service:
        Arc<FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository>>,
    folder_service: Arc<FolderService<EventStore, MetadataStore, PermissionResolverRepository>>,
}

impl MailService {
    pub fn new(
        metadata_store: Arc<MetadataStore>,
        object_store: Arc<ObjectStore>,
        file_service: Arc<
            FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository>,
        >,
        folder_service: Arc<FolderService<EventStore, MetadataStore, PermissionResolverRepository>>,
    ) -> Self {
        Self {
            metadata_store,
            object_store,
            file_service,
            folder_service,
        }
    }

    /// Import a raw `.eml` source into RustShare as a mail artifact.
    ///
    /// The raw source, plain-text body, HTML body, and attachment payloads are
    /// persisted as content-addressed blobs. Metadata is written to the
    /// `mail_messages`, `mail_message_parts`, and `mail_attachments` tables.
    ///
    /// In addition, a dedicated `/Workspace/Mail/{date}-{subject}-{short-uuid}`
    /// folder is created and the raw `.eml` source is stored inside it as
    /// `source.eml`.
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

        let parsed =
            EmlParser::parse(&raw_source).map_err(|e| MailError::InvalidSource(e.to_string()))?;

        let source_hash = hex::encode(Sha256::digest(&raw_source));
        let source_key = format!("blobs/{source_hash}");
        let source_size = raw_source.len() as i64;

        // Persist the raw source blob first (content-addressed).
        self.object_store
            .put(&source_key, bytes::Bytes::copy_from_slice(&raw_source))
            .await
            .map_err(|e| MailError::Storage(e.to_string()))?;

        // Create the mail artifact folder and store the source as a File.
        let mail_root = self.ensure_mail_root_folder(owner_id, tenant_id).await?;
        let message_folder = self
            .create_message_folder(mail_root.id, owner_id, tenant_id, parsed.subject.as_deref())
            .await?;

        let _source_file = self
            .file_service
            .upload_file(
                owner_id,
                "source.eml".to_string(),
                Some(message_folder.id),
                bytes::Bytes::from(raw_source),
                "message/rfc822".to_string(),
                tenant_id,
            )
            .await
            .map_err(|e| MailError::Storage(e.to_string()))?;

        let mut msg = MailMessage::new(tenant_id, owner_id, imported_by, MailSourceMode::EmlUpload);
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
        msg.visibility = MailVisibility::Private.into();
        msg.folder_id = Some(message_folder.id);

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

        for (idx, att) in parsed.attachments.into_iter().enumerate() {
            let hash = hex::encode(Sha256::digest(&att.data));
            let key = format!("blobs/{hash}");
            let bytes = bytes::Bytes::from(att.data);

            self.object_store
                .put(&key, bytes.clone())
                .await
                .map_err(|e| MailError::Storage(e.to_string()))?;

            let filename = att.filename.unwrap_or_else(|| format!("attachment-{idx}"));
            let file = self
                .file_service
                .upload_file(
                    owner_id,
                    filename.clone(),
                    Some(message_folder.id),
                    bytes,
                    att.mime_type.clone(),
                    tenant_id,
                )
                .await
                .map_err(|e| MailError::Storage(e.to_string()))?;

            let attachment = MailAttachment {
                id: Uuid::new_v4(),
                tenant_id,
                message_id: msg.id,
                file_id: Some(file.id),
                filename,
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
        let size_bytes = bytes.len() as i64;
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
            size_bytes: Some(size_bytes),
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

    /// List attachments for a message, scoped to the owning user and tenant.
    pub async fn list_attachments(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        message_id: Uuid,
    ) -> Result<Vec<MailAttachment>, MailError> {
        self.metadata_store
            .list_mail_attachments_by_message_id(message_id, tenant_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))
    }

    /// Ensure the canonical `/Workspace/Mail` folder exists.
    ///
    /// Legacy module root policy: new writes are always directed to the
    /// canonical `/Workspace/Mail` path. Legacy roots are read-only.
    async fn ensure_mail_root_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Folder, MailError> {
        let workspace_name = "Workspace";
        let folder_name = "Mail";

        let root_folders = self
            .metadata_store
            .list_folders(None, owner_id, tenant_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        let workspace =
            if let Some(ws) = root_folders.into_iter().find(|f| f.name == workspace_name) {
                ws
            } else {
                self.folder_service
                    .create_folder_or_get(workspace_name.to_string(), None, owner_id, tenant_id)
                    .await
                    .map_err(|e| MailError::Storage(e.to_string()))?
            };

        let ws_folders = self
            .metadata_store
            .list_folders(Some(workspace.id), owner_id, tenant_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        if let Some(mail_root) = ws_folders.into_iter().find(|f| f.name == folder_name) {
            return Ok(mail_root);
        }

        self.folder_service
            .create_folder_or_get(
                folder_name.to_string(),
                Some(workspace.id),
                owner_id,
                tenant_id,
            )
            .await
            .map_err(|e| MailError::Storage(e.to_string()))
    }

    /// Create a unique subfolder under `/Workspace/Mail` for this message.
    async fn create_message_folder(
        &self,
        mail_root_id: Uuid,
        owner_id: UserId,
        tenant_id: Uuid,
        subject: Option<&str>,
    ) -> Result<Folder, MailError> {
        let base_name = subject
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(slug::slugify)
            .unwrap_or_else(|| "message".to_string());

        let short_uuid = &Uuid::new_v4().to_string()[..8];
        let folder_name = format!(
            "{}-{}-{}",
            Utc::now().format("%Y-%m-%d"),
            base_name,
            short_uuid
        );

        self.folder_service
            .create_folder(folder_name, Some(mail_root_id), owner_id, tenant_id)
            .await
            .map_err(|e| MailError::Storage(e.to_string()))
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
    use rustshare_core::domain::MailSourceMode;
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
        let mut msg = MailMessage::new(tenant_id, owner_id, imported_by, MailSourceMode::EmlUpload);
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

    #[test]
    fn parse_attachment_eml_decodes_attachment_fields() {
        let raw = b"From: sender@example.com\r\nTo: recipient@example.com\r\nSubject: With attachment\r\nMessage-ID: <attach789@example.com>\r\nDate: Mon, 06 Jul 2026 14:00:00 +0000\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"boundary123\"\r\n\r\n--boundary123\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nSee attached file.\r\n\r\n--boundary123\r\nContent-Type: text/plain; name=\"note.txt\"\r\nContent-Disposition: attachment; filename=\"note.txt\"\r\nContent-Transfer-Encoding: base64\r\n\r\nYXR0YWNobWVudCBjb250ZW50\r\n\r\n--boundary123--\r\n";
        let parsed = EmlParser::parse(raw.as_slice()).expect("parse .eml with attachment");

        assert!(!parsed.attachments.is_empty());
        assert_eq!(parsed.attachments.len(), 1);

        let att = &parsed.attachments[0];
        assert_eq!(att.filename, Some("note.txt".to_string()));
        assert_eq!(att.mime_type, "text/plain");
        assert_eq!(att.data, b"attachment content");
        assert_eq!(att.size_bytes, att.data.len());
        assert_eq!(
            att.content_disposition,
            Some("attachment; filename=\"note.txt\"".to_string())
        );
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL and S3-compatible object storage"]
    async fn import_eml_sets_folder_id_when_folder_creation_succeeds() {
        use rustshare_core::domain::User;
        use rustshare_core::events::EventBroadcaster;
        use rustshare_core::services::PermissionResolver;
        use rustshare_infrastructure::repositories::PermissionResolverRepository;

        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://rustshare:changeme@localhost:5432/rustshare".to_string()
        });
        let pool = sqlx::postgres::PgPool::connect(&database_url)
            .await
            .expect("failed to connect to database");

        let metadata_store = Arc::new(MetadataStore::new(pool.clone()));
        let event_store = Arc::new(EventStore::new(pool.clone()));
        let broadcaster = Arc::new(EventBroadcaster::new(100));

        let s3_endpoint = std::env::var("S3_ENDPOINT")
            .or_else(|_| std::env::var("RUSTFS_ENDPOINT"))
            .unwrap_or_else(|_| "http://localhost:9000".to_string());
        let s3_region = std::env::var("S3_REGION")
            .or_else(|_| std::env::var("RUSTFS_REGION"))
            .unwrap_or_else(|_| "us-east-1".to_string());
        let s3_bucket = std::env::var("S3_BUCKET")
            .or_else(|_| std::env::var("RUSTFS_BUCKET"))
            .unwrap_or_else(|_| "rustshare".to_string());

        let object_store = Arc::new(
            ObjectStore::new_with_options(
                s3_endpoint,
                s3_region,
                s3_bucket,
                rustshare_storage::ObjectStoreOptions {
                    auto_create_bucket: true,
                },
            )
            .await
            .expect("failed to create object store"),
        );

        let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
            PermissionResolverRepository::new(pool.clone()),
        )));

        let file_service = Arc::new(FileService::new(
            event_store.clone(),
            metadata_store.clone(),
            object_store.clone(),
            broadcaster.clone(),
            permission_resolver.clone(),
        ));
        let folder_service = Arc::new(FolderService::new(
            event_store.clone(),
            metadata_store.clone(),
            broadcaster.clone(),
            permission_resolver,
        ));

        let mail_service = MailService::new(
            metadata_store.clone(),
            object_store.clone(),
            file_service,
            folder_service,
        );

        let tenant_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO tenants (id, name, created_at, updated_at)
            VALUES ($1, $2, NOW(), NOW())
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(tenant_id)
        .bind(format!("Mail Test Tenant {}", tenant_id))
        .execute(&pool)
        .await
        .expect("failed to create test tenant");

        let user = User::new(
            format!("mail_user_{}", Uuid::new_v4()),
            "Mail Test User".to_string(),
            "test_password_hash".to_string(),
            format!("mail_{}@test.local", Uuid::new_v4()),
            false,
            10_737_418_240,
            tenant_id,
        );
        metadata_store
            .create_user(&user)
            .await
            .expect("failed to create test user");

        let raw = b"From: Alice <alice@example.com>\r\nTo: Bob <bob@example.com>\r\nSubject: Artifact Folder Test\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nBody.\r\n";

        let message = mail_service
            .import_eml(tenant_id, user.id, user.id, raw.to_vec())
            .await
            .expect("import_eml should succeed");

        assert!(message.folder_id.is_some(), "folder_id should be set");

        let folder_id = message.folder_id.unwrap();
        let folder = metadata_store
            .find_folder_by_id(folder_id, user.id)
            .await
            .expect("find_folder_by_id should not fail")
            .expect("message folder should exist");
        assert!(folder.path.starts_with("/Workspace/Mail/"));

        let files = metadata_store
            .list_files(Some(folder_id), user.id, tenant_id)
            .await
            .expect("list_files should not fail");
        assert!(files.iter().any(|f| f.name == "source.eml"));

        // Best-effort cleanup.
        let _ = sqlx::query("DELETE FROM mail_attachments WHERE message_id = $1")
            .bind(message.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM mail_message_parts WHERE message_id = $1")
            .bind(message.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM mail_messages WHERE id = $1")
            .bind(message.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM files WHERE owner_id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM folders WHERE owner_id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .execute(&pool)
            .await;
    }
}
