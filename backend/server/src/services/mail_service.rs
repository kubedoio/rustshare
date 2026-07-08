use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::Utc;
use rustshare_core::domain::{
    Folder, LinkTargetType, MailAccount, MailAccountId, MailAttachment, MailImportJob,
    MailImportJobId, MailLink, MailMessage, MailMessagePart, MailSourceMode, MailTlsMode,
    MailVisibility, SharePermissions, UserId,
};
use rustshare_core::events::{
    AggregateType, Event, EventType, MailLinkedPayload, MailUnlinkedPayload,
};
use rustshare_core::services::eml_parser::EmlParser;
use rustshare_core::services::{FileService, FolderService, ObjectStoreOps, PermissionResolver};
use rustshare_crypto::{encrypt_secret, SecretEncryptionKey};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::imap_client::{ImapClient, ImapError, ImapMessageSummary, ImapSession, MailFolder};

const MAX_MAIL_ARTIFACT_NAME_LEN: usize = 200;
const MAX_MAIL_FOLDER_SUBJECT_SLUG_LEN: usize = 200;

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("Mail message not found: {0}")]
    NotFound(uuid::Uuid),
    #[error("Mail account not found: {0}")]
    AccountNotFound(Uuid),
    #[error("Import job not found: {0}")]
    JobNotFound(Uuid),
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Invalid mail source: {0}")]
    InvalidSource(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("IMAP error: {0}")]
    Imap(String),
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
    permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
    event_store: Arc<EventStore>,
    secret_key: Arc<SecretEncryptionKey>,
}

impl MailService {
    pub fn new(
        metadata_store: Arc<MetadataStore>,
        object_store: Arc<ObjectStore>,
        file_service: Arc<
            FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository>,
        >,
        folder_service: Arc<FolderService<EventStore, MetadataStore, PermissionResolverRepository>>,
        permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
        event_store: Arc<EventStore>,
        secret_key: Arc<SecretEncryptionKey>,
    ) -> Self {
        Self {
            metadata_store,
            object_store,
            file_service,
            folder_service,
            permission_resolver,
            event_store,
            secret_key,
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
        self.import_raw_source(
            tenant_id,
            owner_id,
            imported_by,
            MailSourceMode::EmlUpload,
            None,
            None,
            raw_source,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn import_raw_source(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        imported_by: Uuid,
        source_mode: MailSourceMode,
        source_folder: Option<&str>,
        source_uid: Option<i64>,
        raw_source: Vec<u8>,
    ) -> Result<MailMessage, MailError> {
        if raw_source.is_empty() {
            return Err(MailError::InvalidSource("Empty mail source".to_string()));
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

        let mut msg = MailMessage::new(tenant_id, owner_id, imported_by, source_mode);
        msg.source_folder = source_folder.map(|s| s.to_string());
        msg.source_uid = source_uid;
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

        // Insert the mail row before parts/attachments because they reference
        // it through foreign-key constraints. A future transaction wrap can
        // hide partially imported messages; for now, failing after this point
        // leaves the message row visible, which is acceptable for Phase 1.
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

        let mut artifact_name_counts = HashMap::new();
        let mut artifact_names = HashSet::from(["source.eml".to_string()]);
        for (idx, att) in parsed.attachments.into_iter().enumerate() {
            let hash = hex::encode(Sha256::digest(&att.data));
            let key = format!("blobs/{hash}");
            let bytes = bytes::Bytes::from(att.data);

            self.object_store
                .put(&key, bytes.clone())
                .await
                .map_err(|e| MailError::Storage(e.to_string()))?;

            let filename = att.filename.unwrap_or_else(|| format!("attachment-{idx}"));
            let safe_filename = safe_attachment_artifact_filename(&filename, idx);
            let artifact_filename = unique_artifact_filename(
                &safe_filename,
                &mut artifact_name_counts,
                &mut artifact_names,
            );
            let file = self
                .file_service
                .upload_file(
                    owner_id,
                    artifact_filename,
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

    /// Verify the caller can read the link target.
    async fn require_target_read(
        &self,
        tenant_id: Uuid,
        caller: UserId,
        target_type: &LinkTargetType,
        target_id: Uuid,
    ) -> Result<(), MailError> {
        match target_type {
            LinkTargetType::File | LinkTargetType::Note => {
                let permitted = self
                    .permission_resolver
                    .check_file_permission(caller, tenant_id, target_id, SharePermissions::View)
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?;
                if permitted {
                    Ok(())
                } else {
                    Err(MailError::PermissionDenied)
                }
            }
            LinkTargetType::Folder
            | LinkTargetType::KanbanCard
            | LinkTargetType::KanbanBoard
            | LinkTargetType::Meeting => {
                let permitted = self
                    .permission_resolver
                    .check_folder_permission(caller, tenant_id, target_id, SharePermissions::View)
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?;
                if permitted {
                    Ok(())
                } else {
                    Err(MailError::PermissionDenied)
                }
            }
            LinkTargetType::MailMessage => self
                .get_message(tenant_id, caller, target_id)
                .await
                .map(|_| ())
                .map_err(|err| match err {
                    MailError::NotFound(_) => MailError::PermissionDenied,
                    other => other,
                }),
        }
    }

    /// Link a mail message to another RustShare object.
    ///
    /// Caller must be able to read both the mail message and the target object.
    /// If an active link already exists, it is returned idempotently.
    pub async fn link_message(
        &self,
        tenant_id: Uuid,
        caller: UserId,
        message_id: Uuid,
        target_type: LinkTargetType,
        target_id: Uuid,
    ) -> Result<MailLink, MailError> {
        // 1. Verify caller can read the source mail.
        self.get_message(tenant_id, caller, message_id).await?;

        // 2. Verify caller can read the link target.
        self.require_target_read(tenant_id, caller, &target_type, target_id)
            .await?;

        // 3. Return existing active link if present.
        if let Some(existing) = self
            .metadata_store
            .find_active_mail_link(message_id, target_type.as_str(), target_id, tenant_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
        {
            return Ok(existing);
        }

        // 4. Build the new link.
        let link = MailLink::new(tenant_id, message_id, caller, target_type, target_id);

        // 5. Persist the link and emit a MailLinked audit event atomically.
        //    ON CONFLICT handles races where another request created the same
        //    active link between the pre-check above and this insert.
        let mut tx = self
            .metadata_store
            .pool()
            .begin()
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        let inserted = self
            .metadata_store
            .create_mail_link_in_tx(&mut tx, &link)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        if !inserted {
            // A concurrent request won the race. Fetch the existing active link
            // and return it without emitting a duplicate event.
            let existing = self
                .metadata_store
                .find_active_mail_link(message_id, target_type.as_str(), target_id, tenant_id)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?
                .ok_or_else(|| {
                    MailError::Database(
                        "concurrent mail link disappeared after conflict".to_string(),
                    )
                })?;
            tx.commit()
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
            return Ok(existing);
        }

        let payload = MailLinkedPayload {
            message_id: link.message_id,
            link_id: link.id,
            target_type: link.target_type.clone(),
            target_id: link.target_id,
        };
        let event = Event::new(
            EventType::MailLinked,
            link.message_id,
            AggregateType::MailMessage,
            serde_json::to_value(payload).map_err(|e| MailError::Database(e.to_string()))?,
            caller,
        );
        self.event_store
            .append_in_tx(&mut tx, &event)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        Ok(link)
    }

    /// Remove a link between a mail message and another RustShare object.
    ///
    /// The link is soft-deleted and a `MailUnlinked` audit event is emitted only
    /// when the link was still active. Repeated calls after the link has already
    /// been soft-deleted are idempotent and do not emit additional events.
    pub async fn unlink_message(
        &self,
        tenant_id: Uuid,
        caller: UserId,
        link_id: Uuid,
    ) -> Result<(), MailError> {
        // 1. Load the link.
        let link = self
            .metadata_store
            .find_mail_link_by_id(link_id, tenant_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or(MailError::NotFound(link_id))?;

        // 2. Verify caller can read the source mail.
        self.get_message(tenant_id, caller, link.message_id).await?;

        // 3. Verify caller can read the link target.
        let target_type = link
            .target_type
            .parse::<LinkTargetType>()
            .map_err(MailError::Database)?;
        self.require_target_read(tenant_id, caller, &target_type, link.target_id)
            .await?;

        // 4. Soft-delete the link and emit a MailUnlinked audit event atomically.
        let mut tx = self
            .metadata_store
            .pool()
            .begin()
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        let updated = self
            .metadata_store
            .soft_delete_mail_link_in_tx(&mut tx, link_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        if updated {
            let payload = MailUnlinkedPayload {
                message_id: link.message_id,
                link_id: link.id,
                target_type: link.target_type.clone(),
                target_id: link.target_id,
            };
            let event = Event::new(
                EventType::MailUnlinked,
                link.message_id,
                AggregateType::MailMessage,
                serde_json::to_value(payload).map_err(|e| MailError::Database(e.to_string()))?,
                caller,
            );
            self.event_store
                .append_in_tx(&mut tx, &event)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        Ok(())
    }

    /// Load a mail link by id, including soft-deleted rows.
    ///
    /// Caller must be able to read the source mail. This is used by the DELETE
    /// handler to validate URL ownership for retries on already-deleted links.
    pub async fn find_mail_link_by_id(
        &self,
        tenant_id: Uuid,
        caller: UserId,
        link_id: Uuid,
    ) -> Result<MailLink, MailError> {
        let link = self
            .metadata_store
            .find_mail_link_by_id(link_id, tenant_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or(MailError::NotFound(link_id))?;

        // Verify caller can read the source mail; this also enforces tenant
        // scoping because get_message checks owner_id.
        self.get_message(tenant_id, caller, link.message_id).await?;

        Ok(link)
    }

    /// List active links for a mail message.
    ///
    /// Caller must be able to read the source mail. Links to targets the caller
    /// cannot read are silently omitted so the existence of inaccessible targets
    /// is not leaked.
    pub async fn list_message_links(
        &self,
        tenant_id: Uuid,
        caller: UserId,
        message_id: Uuid,
    ) -> Result<Vec<MailLink>, MailError> {
        // 1. Verify caller can read the source mail.
        self.get_message(tenant_id, caller, message_id).await?;

        // 2. Load active links.
        let links = self
            .metadata_store
            .list_mail_links_by_message(message_id, tenant_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        // 3. Filter out targets the caller cannot read.
        let mut visible = Vec::with_capacity(links.len());
        for link in links {
            let target_type = link
                .target_type
                .parse::<LinkTargetType>()
                .map_err(MailError::Database)?;
            match self
                .require_target_read(tenant_id, caller, &target_type, link.target_id)
                .await
            {
                Ok(()) => visible.push(link),
                Err(MailError::PermissionDenied) => {}
                Err(e) => return Err(e),
            }
        }

        Ok(visible)
    }

    /// List imported mail messages for a user.
    pub async fn list_messages(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
    ) -> Result<Vec<MailMessage>, MailError> {
        self.metadata_store
            .list_mail_messages(tenant_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))
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

    // ============================================================================
    // Mail accounts
    // ============================================================================

    #[allow(clippy::too_many_arguments)]
    /// Create a new IMAP mail account with an encrypted password.
    pub async fn create_account(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        name: String,
        host: String,
        port: i32,
        username: String,
        password: String,
        tls_mode: MailTlsMode,
    ) -> Result<MailAccount, MailError> {
        let password_enc = encrypt_secret(&password, &self.secret_key)
            .map_err(|e| MailError::Storage(format!("failed to encrypt password: {e}")))?;
        let account = MailAccount::new(
            tenant_id,
            owner_id,
            name,
            host,
            port,
            username,
            password_enc,
            tls_mode,
        );
        self.metadata_store
            .create_mail_account(&account)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        Ok(account)
    }

    /// List active mail accounts for a user.
    pub async fn list_accounts(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
    ) -> Result<Vec<MailAccount>, MailError> {
        self.metadata_store
            .list_mail_accounts_by_owner(tenant_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))
    }

    /// Get a single active mail account if owned by the user.
    pub async fn get_account(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
    ) -> Result<MailAccount, MailError> {
        let account = self
            .metadata_store
            .get_mail_account(account_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or(MailError::AccountNotFound(account_id))?;
        if account.tenant_id != tenant_id {
            return Err(MailError::PermissionDenied);
        }
        Ok(account)
    }

    #[allow(clippy::too_many_arguments)]
    /// Update a mail account's connection details and enabled state.
    pub async fn update_account(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        name: Option<String>,
        host: Option<String>,
        port: Option<i32>,
        username: Option<String>,
        password: Option<String>,
        tls_mode: Option<MailTlsMode>,
        is_enabled: Option<bool>,
    ) -> Result<MailAccount, MailError> {
        let mut account = self.get_account(tenant_id, owner_id, account_id).await?;
        if let Some(name) = name {
            account.name = name;
        }
        if let Some(host) = host {
            account.host = host;
        }
        if let Some(port) = port {
            account.port = port;
        }
        if let Some(username) = username {
            account.username = username;
        }
        if let Some(password) = password {
            account.password_enc = encrypt_secret(&password, &self.secret_key)
                .map_err(|e| MailError::Storage(format!("failed to encrypt password: {e}")))?;
        }
        if let Some(tls_mode) = tls_mode {
            account.tls_mode = tls_mode.to_string();
        }
        if let Some(is_enabled) = is_enabled {
            account.is_enabled = is_enabled;
        }
        self.metadata_store
            .update_mail_account(&account)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        Ok(account)
    }

    /// Soft-delete a mail account and cancel its pending import jobs.
    pub async fn delete_account(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
    ) -> Result<(), MailError> {
        // Ensure the account exists and belongs to the caller.
        self.get_account(tenant_id, owner_id, account_id).await?;
        self.metadata_store
            .soft_delete_mail_account(account_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        Ok(())
    }

    /// Test an account's IMAP connection and update its connection metadata.
    pub async fn test_account_connection(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
    ) -> Result<(), MailError> {
        let mut account = self.get_account(tenant_id, owner_id, account_id).await?;
        let password = rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key)
            .map_err(|e| MailError::Storage(format!("failed to decrypt password: {e}")))?;

        let result = self.connect_and_login(&account, &password).await;
        match result {
            Ok(mut session) => {
                session.list_folders().await.map_err(imap_to_mail_error)?;
                let _ = session.logout().await;
                account.last_connected_at = Some(Utc::now());
                account.last_error = None;
                self.metadata_store
                    .update_mail_account(&account)
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?;
                Ok(())
            }
            Err(e) => {
                account.last_error = Some(e.to_string());
                self.metadata_store
                    .update_mail_account(&account)
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?;
                Err(e)
            }
        }
    }

    // ============================================================================
    // IMAP browsing
    // ============================================================================

    /// List folders available on the account's IMAP server.
    pub async fn list_imap_folders(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
    ) -> Result<Vec<MailFolder>, MailError> {
        let account = self.get_account(tenant_id, owner_id, account_id).await?;
        let password = rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key)
            .map_err(|e| MailError::Storage(format!("failed to decrypt password: {e}")))?;
        let mut session = self.connect_and_login(&account, &password).await?;
        let folders = session.list_folders().await.map_err(imap_to_mail_error)?;
        let _ = session.logout().await;
        Ok(folders)
    }

    /// List message summaries in an IMAP folder.
    pub async fn list_imap_messages(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        folder: &str,
        limit: usize,
    ) -> Result<Vec<ImapMessageSummary>, MailError> {
        let account = self.get_account(tenant_id, owner_id, account_id).await?;
        let password = rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key)
            .map_err(|e| MailError::Storage(format!("failed to decrypt password: {e}")))?;
        let mut session = self.connect_and_login(&account, &password).await?;
        let summaries = session
            .fetch_message_summaries(folder, limit)
            .await
            .map_err(imap_to_mail_error)?;
        let _ = session.logout().await;
        Ok(summaries)
    }

    // ============================================================================
    // Import jobs
    // ============================================================================

    /// Create a job to import selected UIDs from an IMAP folder.
    pub async fn create_imap_import_job(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        folder_name: String,
        selected_uids: Vec<i64>,
    ) -> Result<MailImportJob, MailError> {
        if selected_uids.is_empty() {
            return Err(MailError::InvalidSource(
                "No message UIDs selected for import".to_string(),
            ));
        }
        // Ensure the account exists and belongs to the caller.
        self.get_account(tenant_id, owner_id, account_id).await?;
        let job = MailImportJob::new(tenant_id, owner_id, account_id, folder_name, selected_uids);
        self.metadata_store
            .create_mail_import_job(&job)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        Ok(job)
    }

    /// Get a single import job if owned by the user.
    pub async fn get_import_job(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        job_id: MailImportJobId,
    ) -> Result<MailImportJob, MailError> {
        let job = self
            .metadata_store
            .get_mail_import_job(job_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or(MailError::JobNotFound(job_id))?;
        if job.tenant_id != tenant_id {
            return Err(MailError::PermissionDenied);
        }
        Ok(job)
    }

    /// List active import jobs for a user, optionally filtered by account.
    pub async fn list_import_jobs(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: Option<MailAccountId>,
    ) -> Result<Vec<MailImportJob>, MailError> {
        self.metadata_store
            .list_mail_import_jobs_by_owner(tenant_id, owner_id, account_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))
    }

    /// Process an IMAP import job by fetching each selected UID and importing it.
    pub async fn process_import_job(&self, job: &MailImportJob) -> Result<(), MailError> {
        if !matches!(job.status.as_str(), "pending" | "running") {
            return Err(MailError::InvalidSource(format!(
                "Import job {} has status {} and cannot be re-processed",
                job.id, job.status
            )));
        }

        let account = self
            .metadata_store
            .get_mail_account(job.account_id, job.owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or(MailError::AccountNotFound(job.account_id))?;

        if account.deleted_at.is_some() || !account.is_enabled {
            return Err(MailError::AccountNotFound(job.account_id));
        }
        if account.tenant_id != job.tenant_id {
            return Err(MailError::PermissionDenied);
        }

        let password = rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key)
            .map_err(|e| MailError::Storage(format!("failed to decrypt password: {e}")))?;

        self.metadata_store
            .mark_mail_import_job_running(job.id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        let mut session = match self.connect_and_login(&account, &password).await {
            Ok(session) => session,
            Err(e) => {
                self.metadata_store
                    .mark_mail_import_job_failed(job.id, &format!("connection failed: {e}"))
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?;
                return Err(e);
            }
        };

        let uids = job.selected_uids.clone().unwrap_or_default();
        let mut processed = 0i32;
        let mut failed = 0i32;
        let mut last_error: Option<String> = None;

        for uid in uids {
            let uid_u32 = u32::try_from(uid)
                .map_err(|_| MailError::InvalidSource(format!("Invalid IMAP UID: {uid}")))?;
            match session
                .fetch_rfc822(&job.folder_name, uid_u32)
                .await
                .map_err(imap_to_mail_error)
            {
                Ok(raw_source) => {
                    match self
                        .import_raw_source(
                            job.tenant_id,
                            job.owner_id,
                            job.owner_id,
                            MailSourceMode::ImapSelected,
                            Some(&job.folder_name),
                            Some(uid),
                            raw_source,
                        )
                        .await
                    {
                        Ok(_) => {
                            processed += 1;
                        }
                        Err(e) => {
                            failed += 1;
                            last_error = Some(format!("uid {uid}: {e}"));
                        }
                    }
                }
                Err(e) => {
                    failed += 1;
                    last_error = Some(format!("uid {uid}: {e}"));
                }
            }
            self.metadata_store
                .update_mail_import_job_progress(job.id, processed, failed, last_error.as_deref())
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
        }

        let _ = session.logout().await;

        if failed > 0 {
            let error = last_error.unwrap_or_else(|| format!("{failed} message(s) failed"));
            self.metadata_store
                .mark_mail_import_job_failed(job.id, &error)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
            Err(MailError::Imap(error))
        } else {
            self.metadata_store
                .mark_mail_import_job_completed(job.id)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
            Ok(())
        }
    }

    // ============================================================================
    // Helpers
    // ============================================================================

    /// Connect to the IMAP server and log in using the account credentials.
    async fn connect_and_login(
        &self,
        account: &MailAccount,
        password: &str,
    ) -> Result<ImapSession, MailError> {
        if !(1..=65535).contains(&account.port) {
            return Err(MailError::InvalidSource(format!(
                "Invalid IMAP port: {}",
                account.port
            )));
        }
        let port = account.port as u16;
        let tls_mode = account
            .tls_mode
            .parse::<MailTlsMode>()
            .map_err(|e| MailError::Imap(e.to_string()))?;
        let client = ImapClient::connect(&account.host, port, tls_mode)
            .await
            .map_err(|e| MailError::Imap(e.to_string()))?;
        ImapSession::login(client, &account.username, password)
            .await
            .map_err(|e| MailError::Imap(e.to_string()))
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
        let short_uuid = &Uuid::new_v4().to_string()[..8];
        let folder_name = mail_message_folder_name(subject, short_uuid);

        self.folder_service
            .create_folder(folder_name, Some(mail_root_id), owner_id, tenant_id)
            .await
            .map_err(|e| MailError::Storage(e.to_string()))
    }
}

fn imap_to_mail_error(err: ImapError) -> MailError {
    MailError::Imap(err.to_string())
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

fn mail_message_folder_name(subject: Option<&str>, short_uuid: &str) -> String {
    let base_name = subject
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(slug::slugify)
        .map(|slug| truncate_chars(&slug, MAX_MAIL_FOLDER_SUBJECT_SLUG_LEN))
        .unwrap_or_else(|| "message".to_string());

    format!(
        "{}-{}-{}",
        Utc::now().format("%Y-%m-%d"),
        base_name,
        short_uuid
    )
}

fn unique_artifact_filename(
    filename: &str,
    counts: &mut HashMap<String, usize>,
    used: &mut HashSet<String>,
) -> String {
    let count = counts.entry(filename.to_string()).or_insert(0);
    loop {
        *count += 1;
        let candidate = if *count == 1 {
            filename.to_string()
        } else {
            match filename.rsplit_once('.') {
                Some((stem, ext)) if !stem.is_empty() => format!("{stem}-{}.{ext}", *count),
                _ => format!("{filename}-{}", *count),
            }
        };
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
}

fn safe_attachment_artifact_filename(filename: &str, idx: usize) -> String {
    let sanitized = filename
        .trim()
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | '\0' => '-',
            ch if ch.is_control() => '-',
            ch => ch,
        })
        .collect::<String>()
        .replace("..", ".")
        .trim_matches(|ch| ch == '.' || ch == ' ')
        .to_string();

    // FileService rejects reserved metadata filenames such as
    // `index.editor.json`; treat them like empty/unsafe names.
    let filename = if sanitized.is_empty() || is_reserved_file_name(&sanitized) {
        format!("attachment-{idx}")
    } else {
        sanitized
    };

    truncate_filename(&filename, MAX_MAIL_ARTIFACT_NAME_LEN)
}

fn is_reserved_file_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("index.editor.json")
}

fn truncate_filename(filename: &str, max_chars: usize) -> String {
    if filename.chars().count() <= max_chars {
        return filename.to_string();
    }

    match filename.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.chars().count() < max_chars => {
            let ext_len = ext.chars().count();
            let stem_len = max_chars - ext_len - 1;
            format!("{}.{}", truncate_chars(stem, stem_len), ext)
        }
        _ => truncate_chars(filename, max_chars),
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
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

    #[test]
    fn parse_attachment_preserves_long_content_disposition() {
        let raw = b"From: sender@example.com\r\nTo: recipient@example.com\r\nSubject: Long attachment header\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"boundary123\"\r\n\r\n--boundary123\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nSee attached file.\r\n\r\n--boundary123\r\nContent-Type: application/pdf; name=\"quarterly-report-2026-final.pdf\"\r\nContent-Disposition: attachment; filename=\"quarterly-report-2026-final.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\ncGRmIGNvbnRlbnQ=\r\n\r\n--boundary123--\r\n";
        let parsed = EmlParser::parse(raw.as_slice()).expect("parse .eml with long disposition");

        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(
            parsed.attachments[0].content_disposition,
            Some("attachment; filename=\"quarterly-report-2026-final.pdf\"".to_string())
        );
        assert!(parsed.attachments[0]
            .content_disposition
            .as_ref()
            .is_some_and(|value| value.len() > 50));
    }

    #[test]
    fn parse_name_only_mime_part_as_attachment() {
        let raw = b"From: sender@example.com\r\nTo: recipient@example.com\r\nSubject: Name attachment\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"boundary123\"\r\n\r\n--boundary123\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nSee attached file.\r\n\r\n--boundary123\r\nContent-Type: application/pdf; name=\"report.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\ncGRmIGNvbnRlbnQ=\r\n\r\n--boundary123--\r\n";
        let parsed = EmlParser::parse(raw.as_slice()).expect("parse .eml with name attachment");

        assert_eq!(parsed.attachments.len(), 1);
        let att = &parsed.attachments[0];
        assert_eq!(att.filename, Some("report.pdf".to_string()));
        assert_eq!(att.mime_type, "application/pdf");
        assert_eq!(att.data, b"pdf content");
        assert_eq!(att.content_disposition, None);
    }

    #[test]
    fn unique_artifact_filename_suffixes_duplicate_names() {
        let mut counts = HashMap::new();
        let mut used = HashSet::new();

        assert_eq!(
            unique_artifact_filename("report.pdf", &mut counts, &mut used),
            "report.pdf"
        );
        assert_eq!(
            unique_artifact_filename("report.pdf", &mut counts, &mut used),
            "report-2.pdf"
        );
        assert_eq!(
            unique_artifact_filename("report.pdf", &mut counts, &mut used),
            "report-3.pdf"
        );
        assert_eq!(
            unique_artifact_filename("README", &mut counts, &mut used),
            "README"
        );
        assert_eq!(
            unique_artifact_filename("README", &mut counts, &mut used),
            "README-2"
        );
    }

    #[test]
    fn unique_artifact_filename_skips_generated_collisions() {
        let mut counts = HashMap::new();
        let mut used = HashSet::new();

        assert_eq!(
            unique_artifact_filename("report-2.pdf", &mut counts, &mut used),
            "report-2.pdf"
        );
        assert_eq!(
            unique_artifact_filename("report.pdf", &mut counts, &mut used),
            "report.pdf"
        );
        assert_eq!(
            unique_artifact_filename("report.pdf", &mut counts, &mut used),
            "report-3.pdf"
        );
    }

    #[test]
    fn unique_artifact_filename_reserves_source_artifact() {
        let mut counts = HashMap::new();
        let mut used = HashSet::from(["source.eml".to_string()]);

        assert_eq!(
            unique_artifact_filename("source.eml", &mut counts, &mut used),
            "source-2.eml"
        );
    }

    #[test]
    fn mail_message_folder_name_truncates_long_subject_slug() {
        let subject = "Quarterly update ".repeat(30);
        let folder_name = mail_message_folder_name(Some(&subject), "12345678");

        assert!(folder_name.chars().count() <= 220);
        assert!(folder_name.ends_with("-12345678"));
    }

    #[test]
    fn safe_attachment_artifact_filename_removes_invalid_path_parts() {
        assert_eq!(
            safe_attachment_artifact_filename("../nested\\secret.txt", 0),
            "-nested-secret.txt"
        );
        assert_eq!(safe_attachment_artifact_filename("...", 3), "attachment-3");
        assert_eq!(
            safe_attachment_artifact_filename("report\0final.pdf", 0),
            "report-final.pdf"
        );
    }

    #[test]
    fn safe_attachment_artifact_filename_truncates_long_names() {
        let filename = format!("{}.pdf", "a".repeat(300));
        let sanitized = safe_attachment_artifact_filename(&filename, 0);

        assert!(sanitized.chars().count() <= MAX_MAIL_ARTIFACT_NAME_LEN);
        assert!(sanitized.ends_with(".pdf"));
    }

    #[test]
    fn safe_attachment_artifact_filename_rewrites_reserved_names() {
        assert_eq!(
            safe_attachment_artifact_filename("index.editor.json", 7),
            "attachment-7"
        );
        assert_eq!(
            safe_attachment_artifact_filename("Index.Editor.JSON", 2),
            "attachment-2"
        );
        // Similar but non-reserved names are preserved.
        assert_eq!(
            safe_attachment_artifact_filename("index.editor.json.backup", 0),
            "index.editor.json.backup"
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
            permission_resolver.clone(),
        ));

        let secret_key = Arc::new(SecretEncryptionKey::from_bytes([0x42; 32]));
        let mail_service = MailService::new(
            metadata_store.clone(),
            object_store.clone(),
            file_service,
            folder_service,
            permission_resolver,
            event_store.clone(),
            secret_key,
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
#[cfg(test)]
mod link_tests {
    use super::*;
    use rustshare_core::domain::User;
    use rustshare_core::events::EventBroadcaster;
    use rustshare_core::services::{FileService, FolderService, PermissionResolver};
    use rustshare_infrastructure::repositories::PermissionResolverRepository;

    #[test]
    fn link_target_type_parses_known_strings() {
        assert_eq!(
            "note".parse::<LinkTargetType>().unwrap(),
            LinkTargetType::Note
        );
        assert_eq!(
            "kanban_card".parse::<LinkTargetType>().unwrap(),
            LinkTargetType::KanbanCard
        );
        assert_eq!(
            "kanban_board".parse::<LinkTargetType>().unwrap(),
            LinkTargetType::KanbanBoard
        );
        assert_eq!(
            "meeting".parse::<LinkTargetType>().unwrap(),
            LinkTargetType::Meeting
        );
        assert_eq!(
            "file".parse::<LinkTargetType>().unwrap(),
            LinkTargetType::File
        );
        assert_eq!(
            "folder".parse::<LinkTargetType>().unwrap(),
            LinkTargetType::Folder
        );
        assert_eq!(
            "mail_message".parse::<LinkTargetType>().unwrap(),
            LinkTargetType::MailMessage
        );
    }

    #[test]
    fn link_target_type_rejects_unknown_strings() {
        assert!("unknown".parse::<LinkTargetType>().is_err());
        assert!("".parse::<LinkTargetType>().is_err());
    }

    #[test]
    fn mail_message_default_visibility_is_private() {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let imported_by = Uuid::new_v4();
        let msg = MailMessage::new(tenant_id, owner_id, imported_by, MailSourceMode::EmlUpload);
        assert_eq!(msg.visibility, "private");
    }

    #[test]
    fn link_message_round_trips_target_type_string() {
        let tenant_id = Uuid::new_v4();
        let message_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let link = MailLink::new(
            tenant_id,
            message_id,
            created_by,
            LinkTargetType::Note,
            target_id,
        );
        assert_eq!(link.target_type, "note");
        assert_eq!(
            link.target_type.parse::<LinkTargetType>().unwrap(),
            LinkTargetType::Note
        );
    }

    async fn setup_link_test() -> (
        sqlx::PgPool,
        MailService,
        Arc<MetadataStore>,
        Uuid,
        User,
        MailMessage,
        Folder,
    ) {
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for database tests");
        let pool = sqlx::postgres::PgPool::connect(&database_url)
            .await
            .expect("failed to connect to database");

        sqlx::migrate!("../migrations")
            .run(&pool)
            .await
            .expect("failed to run database migrations");

        let metadata_store = Arc::new(MetadataStore::new(pool.clone()));
        let event_store = Arc::new(EventStore::new(pool.clone()));
        let broadcaster = Arc::new(EventBroadcaster::new(100));
        let permission_resolver = Arc::new(PermissionResolver::new(Arc::new(
            PermissionResolverRepository::new(pool.clone()),
        )));

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
            permission_resolver.clone(),
        ));

        let tenant_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO tenants (id, name, created_at, updated_at)
            VALUES ($1, $2, NOW(), NOW())
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(tenant_id)
        .bind(format!("Mail Link Test Tenant {}", tenant_id))
        .execute(&pool)
        .await
        .expect("failed to create test tenant");

        let user = User::new(
            format!("mail_link_user_{}", Uuid::new_v4()),
            "Mail Link Test User".to_string(),
            "test_password_hash".to_string(),
            format!("mail_link_{}@test.local", Uuid::new_v4()),
            false,
            10_737_418_240,
            tenant_id,
        );
        metadata_store
            .create_user(&user)
            .await
            .expect("failed to create test user");

        let target_folder = folder_service
            .create_folder("Link Target Folder".to_string(), None, user.id, tenant_id)
            .await
            .expect("failed to create target folder");

        let secret_key = Arc::new(SecretEncryptionKey::from_bytes([0x42; 32]));
        let mail_service = MailService::new(
            metadata_store.clone(),
            object_store,
            file_service,
            folder_service,
            permission_resolver,
            event_store,
            secret_key,
        );

        let mut message = MailMessage::new(tenant_id, user.id, user.id, MailSourceMode::EmlUpload);
        message.subject = Some("Link Test".to_string());
        metadata_store
            .create_mail_message(&message)
            .await
            .expect("failed to create mail message");

        (
            pool,
            mail_service,
            metadata_store,
            tenant_id,
            user,
            message,
            target_folder,
        )
    }

    async fn cleanup_link_test(
        pool: &sqlx::PgPool,
        tenant_id: Uuid,
        user_id: Uuid,
        message_id: Uuid,
    ) {
        let _ = sqlx::query("DELETE FROM mail_links WHERE message_id = $1")
            .bind(message_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM events WHERE aggregate_id = $1")
            .bind(message_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM mail_messages WHERE id = $1")
            .bind(message_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM folders WHERE owner_id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .execute(pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL and S3-compatible object storage"]
    async fn link_and_unlink_message_happy_path() {
        let (pool, mail_service, metadata_store, tenant_id, user, message, target_folder) =
            setup_link_test().await;

        let link = mail_service
            .link_message(
                tenant_id,
                user.id,
                message.id,
                LinkTargetType::Folder,
                target_folder.id,
            )
            .await
            .expect("link_message should succeed");
        assert_eq!(link.message_id, message.id);
        assert_eq!(link.target_type, "folder");
        assert_eq!(link.target_id, target_folder.id);
        assert_eq!(link.created_by, user.id);
        assert_eq!(link.tenant_id, tenant_id);

        let links = mail_service
            .list_message_links(tenant_id, user.id, message.id)
            .await
            .expect("list_message_links should succeed");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].id, link.id);

        mail_service
            .unlink_message(tenant_id, user.id, link.id)
            .await
            .expect("unlink_message should succeed");

        let links_after_unlink = mail_service
            .list_message_links(tenant_id, user.id, message.id)
            .await
            .expect("list_message_links should succeed after unlink");
        assert!(links_after_unlink.is_empty());

        let deleted_link = metadata_store
            .find_mail_link_by_id(link.id, tenant_id)
            .await
            .expect("find_mail_link_by_id should not fail")
            .expect("link row should still exist");
        assert!(
            deleted_link.deleted_at.is_some(),
            "link should be soft-deleted"
        );

        // Idempotent: unlinking an already-soft-deleted link must succeed.
        mail_service
            .unlink_message(tenant_id, user.id, link.id)
            .await
            .expect("repeated unlink_message should succeed");

        cleanup_link_test(&pool, tenant_id, user.id, message.id).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL and S3-compatible object storage"]
    async fn unlink_already_deleted_link_is_idempotent() {
        let (pool, mail_service, metadata_store, tenant_id, user, message, target_folder) =
            setup_link_test().await;

        let link = mail_service
            .link_message(
                tenant_id,
                user.id,
                message.id,
                LinkTargetType::Folder,
                target_folder.id,
            )
            .await
            .expect("link_message should succeed");

        mail_service
            .unlink_message(tenant_id, user.id, link.id)
            .await
            .expect("first unlink_message should succeed");
        mail_service
            .unlink_message(tenant_id, user.id, link.id)
            .await
            .expect("second unlink_message should succeed");

        let row = metadata_store
            .find_mail_link_by_id(link.id, tenant_id)
            .await
            .expect("find_mail_link_by_id should not fail")
            .expect("link row should still exist");
        assert!(row.deleted_at.is_some(), "link should remain soft-deleted");

        cleanup_link_test(&pool, tenant_id, user.id, message.id).await;
    }
}
