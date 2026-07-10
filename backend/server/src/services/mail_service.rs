use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rustshare_core::domain::{
    Folder, LinkTargetType, MailAccount, MailAccountId, MailAttachment, MailImportJob,
    MailImportJobId, MailImportJobStatus, MailLink, MailMessage, MailMessagePart, MailSourceMode,
    MailTlsMode, MailVisibility, SharePermissions, UserId,
};
use rustshare_core::events::{
    AggregateType, Event, EventType, MailAccountCreatedPayload, MailAccountDeletedPayload,
    MailArchiveJobCancelledPayload, MailArchiveJobCompletedPayload, MailArchiveJobCreatedPayload,
    MailArchiveJobDeletedPayload, MailArchiveJobStartedPayload, MailImportedPayload,
    MailLinkedPayload, MailUnlinkedPayload,
};
use rustshare_core::services::eml_parser::EmlParser;
use rustshare_core::services::{FileService, FolderService, PermissionResolver};
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
    #[error("Mail account name already exists: {0}")]
    DuplicateAccountName(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("IMAP error: {0}")]
    Imap(String),
    #[error("Import cancelled")]
    Cancelled,
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
    broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
    secret_key: Arc<SecretEncryptionKey>,
}

impl MailService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        metadata_store: Arc<MetadataStore>,
        object_store: Arc<ObjectStore>,
        file_service: Arc<
            FileService<EventStore, MetadataStore, ObjectStore, PermissionResolverRepository>,
        >,
        folder_service: Arc<FolderService<EventStore, MetadataStore, PermissionResolverRepository>>,
        permission_resolver: Arc<PermissionResolver<PermissionResolverRepository>>,
        event_store: Arc<EventStore>,
        broadcaster: Arc<rustshare_core::events::EventBroadcaster>,
        secret_key: Arc<SecretEncryptionKey>,
    ) -> Self {
        Self {
            metadata_store,
            object_store,
            file_service,
            folder_service,
            permission_resolver,
            event_store,
            broadcaster,
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
            None,
            MailSourceMode::EmlUpload,
            None,
            None,
            None,
            raw_source,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn import_raw_source(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        imported_by: Uuid,
        account_id: Option<MailAccountId>,
        source_mode: MailSourceMode,
        source_folder: Option<&str>,
        source_uid: Option<i64>,
        source_uidvalidity: Option<i64>,
        raw_source: Vec<u8>,
        job_id: Option<Uuid>,
    ) -> Result<MailMessage, MailError> {
        if raw_source.is_empty() {
            return Err(MailError::InvalidSource("Empty mail source".to_string()));
        }

        let parsed =
            EmlParser::parse(&raw_source).map_err(|e| MailError::InvalidSource(e.to_string()))?;

        let source_hash = hex::encode(Sha256::digest(&raw_source));
        let source_key = format!("blobs/{source_hash}");
        let source_size = raw_source.len() as i64;

        // Persist the raw source blob first (content-addressed and safe to write
        // even if another worker wins the race).
        self.object_store
            .put(&source_key, bytes::Bytes::copy_from_slice(&raw_source))
            .await
            .map_err(|e| MailError::Storage(e.to_string()))?;

        let mut msg = MailMessage::new(tenant_id, owner_id, imported_by, source_mode);
        msg.account_id = account_id;
        msg.source_folder = source_folder.map(|s| s.to_string());
        msg.source_uid = source_uid;
        msg.source_uidvalidity = source_uidvalidity;
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

        // Insert the mail row before creating any visible artifacts so a
        // concurrent import of the same source UID is detected first.
        let inserted = self
            .metadata_store
            .create_mail_message_if_not_exists(&msg)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        if !inserted {
            // Another worker imported this UID concurrently; fetch the existing
            // message so the caller can treat it as already imported.
            if let (Some(account_id), Some(uid), Some(folder)) =
                (account_id, source_uid, source_folder)
            {
                if let Some(existing) = self
                    .metadata_store
                    .find_mail_message_by_source(
                        owner_id,
                        account_id,
                        source_mode.as_str(),
                        folder,
                        uid,
                        source_uidvalidity,
                    )
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?
                {
                    // Only treat the existing row as imported if it is complete.
                    // A concurrent worker may have inserted the deduplication row
                    // but not yet finished persisting artifacts.
                    if existing.folder_id.is_some() {
                        return Ok(existing);
                    }
                    return Err(MailError::Database(
                        "concurrent import of same UID still in progress".to_string(),
                    ));
                }
            }
            return Err(MailError::Database(
                "mail message source conflict but existing row not found".to_string(),
            ));
        }

        // Create the mail artifact folder and source file only after we won the
        // unique-source insert race. If any later step fails, remove the row so
        // a retry does not treat the UID as already imported while artifacts are
        // missing.
        // Create the mail artifact folder and source file only after we won the
        // unique-source insert race. Keep the folder_id local until all artifacts
        // are durable; only then mark the mail_messages row complete. If any step
        // fails, remove the folder and the deduplication row so retries import
        // the UID again.
        let mut message_folder_id: Option<Uuid> = None;
        let artifact_result: Result<(), MailError> = async {
            let mail_root = self.ensure_mail_root_folder(owner_id, tenant_id).await?;
            let message_folder = self
                .create_message_folder(mail_root.id, owner_id, tenant_id, parsed.subject.as_deref())
                .await?;
            message_folder_id = Some(message_folder.id);

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

            // If this import is tied to a job, re-check cancellation before
            // finalizing the row so a cancelled job does not leave visible mail.
            if let Some(jid) = job_id {
                if let Some(status) = self
                    .metadata_store
                    .get_mail_import_job_status(jid, owner_id)
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?
                {
                    if status != "running" {
                        return Err(MailError::Cancelled);
                    }
                }
            }

            // Mark the row complete only after every artifact is durable.
            msg.folder_id = Some(message_folder.id);
            self.metadata_store
                .update_mail_message_folder_id(msg.id, message_folder.id)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;

            Ok(())
        }
        .await;

        if let Err(e) = artifact_result {
            // Remove any visible artifacts (the message folder and its files)
            // before deleting the deduplication row, so failed imports do not
            // leave orphaned folders behind.
            if let Some(folder_id) = message_folder_id {
                if let Err(cleanup_err) =
                    self.folder_service.delete_folder(folder_id, owner_id).await
                {
                    tracing::error!(
                        message_id = %msg.id,
                        folder_id = %folder_id,
                        error = %cleanup_err,
                        "Failed to clean up partial message folder after artifact error"
                    );
                }
            }
            if let Err(cleanup_err) = self
                .metadata_store
                .delete_mail_message(msg.id, owner_id)
                .await
            {
                tracing::error!(
                    message_id = %msg.id,
                    error = %cleanup_err,
                    "Failed to clean up partial mail message row after artifact error"
                );
            }
            return Err(e);
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

    /// Reject TLS modes that are not supported or allowed in this phase.
    fn validate_tls_mode(tls_mode: MailTlsMode) -> Result<MailTlsMode, MailError> {
        match tls_mode {
            MailTlsMode::Tls => Ok(tls_mode),
            MailTlsMode::StartTls => Err(MailError::InvalidSource(
                "STARTTLS is not supported in this phase; use tls".to_string(),
            )),
            MailTlsMode::None => Err(MailError::InvalidSource(
                "plaintext IMAP (tls_mode: none) is not allowed; use tls".to_string(),
            )),
        }
    }

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
        let tls_mode = Self::validate_tls_mode(tls_mode)?;
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

        let mut tx = self
            .metadata_store
            .pool()
            .begin()
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        self.metadata_store
            .create_mail_account_in_tx(&mut tx, &account)
            .await
            .map_err(|e| mail_account_db_error(&account.name, e))?;

        let event = Event::new(
            EventType::MailAccountCreated,
            account.id,
            AggregateType::MailAccount,
            serde_json::to_value(MailAccountCreatedPayload {
                account_id: account.id,
                host: account.host.clone(),
                username: account.username.clone(),
                owner_id: account.owner_id,
            })
            .map_err(|e| MailError::Database(e.to_string()))?,
            account.owner_id,
        );
        self.event_store
            .append_in_tx(&mut tx, &event)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        tx.commit()
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
            account.tls_mode = Self::validate_tls_mode(tls_mode)?.to_string();
        }
        if let Some(is_enabled) = is_enabled {
            account.is_enabled = is_enabled;
        }
        self.metadata_store
            .update_mail_account(&account)
            .await
            .map_err(|e| mail_account_db_error(&account.name, e))?;
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

        let mut tx = self
            .metadata_store
            .pool()
            .begin()
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        let updated = self
            .metadata_store
            .soft_delete_mail_account_in_tx(&mut tx, account_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        if updated {
            let event = Event::new(
                EventType::MailAccountDeleted,
                account_id,
                AggregateType::MailAccount,
                serde_json::to_value(MailAccountDeletedPayload {
                    account_id,
                    owner_id,
                })
                .map_err(|e| MailError::Database(e.to_string()))?,
                owner_id,
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

    /// List message summaries in an IMAP folder, along with the folder's
    /// UIDVALIDITY so callers can submit stable UID selections.
    pub async fn list_imap_messages(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        folder: &str,
        limit: usize,
    ) -> Result<(Option<u32>, Vec<ImapMessageSummary>), MailError> {
        let account = self.get_account(tenant_id, owner_id, account_id).await?;
        let password = rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key)
            .map_err(|e| MailError::Storage(format!("failed to decrypt password: {e}")))?;
        let mut session = self.connect_and_login(&account, &password).await?;
        let result = session
            .fetch_message_summaries(folder, limit)
            .await
            .map_err(imap_to_mail_error)?;
        let _ = session.logout().await;
        Ok(result)
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
        source_uidvalidity: Option<i64>,
        selected_uids: Vec<i64>,
    ) -> Result<MailImportJob, MailError> {
        if selected_uids.is_empty() {
            return Err(MailError::InvalidSource(
                "No message UIDs selected for import".to_string(),
            ));
        }
        let mut selected_uids = selected_uids;
        selected_uids.sort_unstable();
        selected_uids.dedup();
        // Ensure the account exists, belongs to the caller, and is enabled.
        let account = self.get_account(tenant_id, owner_id, account_id).await?;
        if account.deleted_at.is_some() || !account.is_enabled {
            return Err(MailError::AccountNotFound(account_id));
        }
        let job = MailImportJob::new(
            tenant_id,
            owner_id,
            account_id,
            folder_name,
            selected_uids,
            source_uidvalidity,
        );
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

        let account = match self
            .metadata_store
            .get_mail_account(job.account_id, job.owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
        {
            Some(a) => a,
            None => {
                let marked = self
                    .metadata_store
                    .mark_mail_import_job_failed(job.id, "account not found")
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?;
                if !marked {
                    return Ok(());
                }
                return Err(MailError::AccountNotFound(job.account_id));
            }
        };

        if account.deleted_at.is_some() || !account.is_enabled {
            let marked = self
                .metadata_store
                .mark_mail_import_job_failed(job.id, "account disabled or deleted")
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
            if !marked {
                return Ok(());
            }
            return Err(MailError::AccountNotFound(job.account_id));
        }
        if account.tenant_id != job.tenant_id {
            return Err(MailError::PermissionDenied);
        }

        let password =
            match rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key) {
                Ok(p) => p,
                Err(e) => {
                    let error = format!("failed to decrypt password: {e}");
                    let marked = self
                        .metadata_store
                        .mark_mail_import_job_failed(job.id, &error)
                        .await
                        .map_err(|e| MailError::Database(e.to_string()))?;
                    if marked {
                        return Err(MailError::Storage(error));
                    }
                    return Ok(());
                }
            };

        let running = self
            .metadata_store
            .mark_mail_import_job_running(job.id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        if !running {
            tracing::info!(
                job_id = %job.id,
                "Stopping import job because it is no longer pending or running"
            );
            return Ok(());
        }

        let mut session = match self.connect_and_login(&account, &password).await {
            Ok(session) => session,
            Err(e) => {
                let marked = self
                    .metadata_store
                    .mark_mail_import_job_failed(job.id, &format!("connection failed: {e}"))
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?;
                if !marked {
                    return Ok(());
                }
                return Err(e);
            }
        };

        let source_uidvalidity = match session.select_folder(&job.folder_name).await {
            Ok(uidvalidity) => uidvalidity.map(i64::from),
            Err(e) => {
                let err = imap_to_mail_error(e);
                let marked = self
                    .metadata_store
                    .mark_mail_import_job_failed(job.id, &format!("folder selection failed: {err}"))
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?;
                if !marked {
                    return Ok(());
                }
                return Err(err);
            }
        };

        if source_uidvalidity != job.source_uidvalidity {
            let error = format!(
                "UIDVALIDITY changed from {:?} to {:?}; selected UIDs are stale",
                job.source_uidvalidity, source_uidvalidity
            );
            let marked = self
                .metadata_store
                .mark_mail_import_job_failed(job.id, &error)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
            if marked {
                return Err(MailError::Imap(error));
            }
            return Ok(());
        }

        let mut processed = 0i32;
        let mut failed = 0i32;
        let mut skipped_inflight = 0i32;
        let mut last_error: Option<String> = None;

        for &uid in job.selected_uids.as_deref().unwrap_or(&[]) {
            // Stop early if the job was cancelled (e.g. because the account was
            // deleted while this worker was running).
            if let Some(status) = self
                .metadata_store
                .get_mail_import_job_status(job.id, job.owner_id)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?
            {
                if status != "running" {
                    tracing::info!(
                        job_id = %job.id,
                        status = %status,
                        "Stopping import job because status is no longer running"
                    );
                    return Ok(());
                }
            }

            let uid_u32 = u32::try_from(uid)
                .map_err(|_| MailError::InvalidSource(format!("Invalid IMAP UID: {uid}")))?;

            // Skip UIDs already imported for this account/folder/uidvalidity so retries are
            // idempotent after a worker crash or stale-heartbeat reset. A row without a
            // folder_id is a partial import (worker died after dedup insert but before
            // artifacts were created). Only reclaim it when no other running job is
            // actively importing the same UID, so we do not corrupt an in-flight import.
            if let Some(existing) = self
                .metadata_store
                .find_mail_message_by_source(
                    job.owner_id,
                    job.account_id,
                    MailSourceMode::ImapSelected.as_str(),
                    &job.folder_name,
                    uid,
                    source_uidvalidity,
                )
                .await
                .map_err(|e| MailError::Database(e.to_string()))?
            {
                if existing.folder_id.is_some() {
                    processed += 1;
                    self.metadata_store
                        .update_mail_import_job_progress(
                            job.id,
                            processed,
                            failed,
                            last_error.as_deref(),
                        )
                        .await
                        .map_err(|e| MailError::Database(e.to_string()))?;
                    continue;
                }

                let in_flight = self
                    .metadata_store
                    .has_other_running_import_job_for_uid(
                        job.owner_id,
                        job.account_id,
                        &job.folder_name,
                        uid,
                        job.id,
                    )
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?;
                if in_flight {
                    tracing::info!(
                        job_id = %job.id,
                        message_id = %existing.id,
                        uid = %uid,
                        "Partial mail row belongs to another running job; deferring"
                    );
                    // Don't count this UID as processed: the other job may still
                    // fail and delete the partial row. Leave the current job
                    // non-terminal so the stale-job reset will retry it.
                    skipped_inflight += 1;
                    continue;
                }

                tracing::info!(
                    job_id = %job.id,
                    message_id = %existing.id,
                    uid = %uid,
                    "Deleting abandoned partial mail_message row and re-importing"
                );
                self.metadata_store
                    .delete_mail_message(existing.id, job.owner_id)
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?;
            }

            match session
                .fetch_rfc822(&job.folder_name, uid_u32, source_uidvalidity)
                .await
                .map_err(imap_to_mail_error)
            {
                Ok(raw_source) => {
                    // Re-check cancellation after the fetch; the account (and job)
                    // may have been deleted while the message body was downloading.
                    if let Some(status) = self
                        .metadata_store
                        .get_mail_import_job_status(job.id, job.owner_id)
                        .await
                        .map_err(|e| MailError::Database(e.to_string()))?
                    {
                        if status != "running" {
                            tracing::info!(
                                job_id = %job.id,
                                uid = %uid,
                                status = %status,
                                "Stopping import because job is no longer running after fetch"
                            );
                            return Ok(());
                        }
                    }

                    match self
                        .import_raw_source(
                            job.tenant_id,
                            job.owner_id,
                            job.owner_id,
                            Some(job.account_id),
                            MailSourceMode::ImapSelected,
                            Some(&job.folder_name),
                            Some(uid),
                            source_uidvalidity,
                            raw_source,
                            Some(job.id),
                        )
                        .await
                    {
                        Ok(msg) => {
                            let event = Event::new(
                                EventType::MailImported,
                                msg.id,
                                AggregateType::MailMessage,
                                serde_json::to_value(MailImportedPayload {
                                    message_id: msg.id,
                                    account_id: job.account_id,
                                    folder_name: job.folder_name.clone(),
                                    source_uid: uid,
                                    owner_id: job.owner_id,
                                })
                                .map_err(|e| MailError::Database(e.to_string()))?,
                                job.owner_id,
                            );
                            if let Err(e) = self.event_store.append(&event, &self.broadcaster).await
                            {
                                tracing::error!(
                                    message_id = %msg.id,
                                    job_id = %job.id,
                                    source_uid = uid,
                                    error = %e,
                                    "failed to append MailImported event"
                                );
                                failed += 1;
                                last_error = Some(format!(
                                    "uid {uid}: failed to append MailImported event: {e}"
                                ));
                            } else {
                                processed += 1;
                            }
                        }
                        Err(MailError::Cancelled) => {
                            tracing::info!(
                                job_id = %job.id,
                                uid = %uid,
                                "Stopping import because job was cancelled during import"
                            );
                            self.metadata_store
                                .update_mail_import_job_progress(
                                    job.id,
                                    processed,
                                    failed,
                                    last_error.as_deref(),
                                )
                                .await
                                .map_err(|e| MailError::Database(e.to_string()))?;
                            return Ok(());
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
            let marked = self
                .metadata_store
                .mark_mail_import_job_failed(job.id, &error)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
            if marked {
                Err(MailError::Imap(error))
            } else {
                Ok(())
            }
        } else if skipped_inflight > 0 {
            tracing::info!(
                job_id = %job.id,
                skipped = %skipped_inflight,
                "Import job has UIDs in-flight in other jobs; leaving non-terminal for retry"
            );
            Ok(())
        } else {
            let _ = self
                .metadata_store
                .mark_mail_import_job_completed(job.id)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
            Ok(())
        }
    }

    // ============================================================================
    // Archive jobs
    // ============================================================================

    /// Create a recurring IMAP archive job for a folder and optional date range.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_archive_job(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        folder_name: String,
        archive_since: Option<DateTime<Utc>>,
        archive_before: Option<DateTime<Utc>>,
        retention_days: Option<i32>,
        max_retries: Option<i32>,
    ) -> Result<MailImportJob, MailError> {
        let account = self
            .metadata_store
            .get_mail_account(account_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or(MailError::AccountNotFound(account_id))?;

        if account.tenant_id != tenant_id {
            return Err(MailError::PermissionDenied);
        }

        if let (Some(since), Some(before)) = (archive_since, archive_before) {
            if since >= before {
                return Err(MailError::InvalidSource(
                    "archive_since must be before archive_before".to_string(),
                ));
            }
        }

        let max_retries = max_retries.unwrap_or(3).max(0);
        let job = MailImportJob::new_archive(
            tenant_id,
            owner_id,
            account_id,
            folder_name,
            archive_since,
            archive_before,
            retention_days,
            max_retries,
        );

        self.metadata_store
            .create_mail_import_job(&job)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        let event = Event::new(
            EventType::MailArchiveJobCreated,
            job.id,
            AggregateType::MailImportJob,
            serde_json::to_value(MailArchiveJobCreatedPayload {
                job_id: job.id,
                account_id: job.account_id,
                folder_name: job.folder_name.clone(),
                owner_id: job.owner_id,
            })
            .map_err(|e| MailError::Database(e.to_string()))?,
            owner_id,
        );
        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        Ok(job)
    }

    /// List active archive jobs for a user, filtered by account.
    pub async fn list_archive_jobs(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
    ) -> Result<Vec<MailImportJob>, MailError> {
        let account = self
            .metadata_store
            .get_mail_account(account_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or(MailError::AccountNotFound(account_id))?;

        if account.tenant_id != tenant_id {
            return Err(MailError::PermissionDenied);
        }

        let jobs = self
            .metadata_store
            .list_mail_import_jobs_by_owner(tenant_id, owner_id, Some(account_id))
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        Ok(jobs
            .into_iter()
            .filter(|j| j.source_mode == "imap_archive")
            .collect())
    }

    /// Get a single archive job if owned by the user.
    pub async fn get_archive_job(
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

        if job.tenant_id != tenant_id || job.source_mode != "imap_archive" {
            return Err(MailError::JobNotFound(job_id));
        }

        Ok(job)
    }

    /// Cancel a pending or running archive job.
    pub async fn cancel_archive_job(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        job_id: MailImportJobId,
    ) -> Result<MailImportJob, MailError> {
        let job = self.get_archive_job(tenant_id, owner_id, job_id).await?;

        let updated = self
            .metadata_store
            .update_mail_import_job_status(job_id, "cancelled", &["pending", "running"])
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        if !updated {
            return Err(MailError::InvalidSource(
                "Job cannot be cancelled in its current state".to_string(),
            ));
        }

        let event = Event::new(
            EventType::MailArchiveJobCancelled,
            job.id,
            AggregateType::MailImportJob,
            serde_json::to_value(MailArchiveJobCancelledPayload {
                job_id: job.id,
                account_id: job.account_id,
            })
            .map_err(|e| MailError::Database(e.to_string()))?,
            owner_id,
        );
        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        self.get_archive_job(tenant_id, owner_id, job_id).await
    }

    /// Soft-delete an archive job.
    pub async fn delete_archive_job(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        job_id: MailImportJobId,
    ) -> Result<(), MailError> {
        let job = self.get_archive_job(tenant_id, owner_id, job_id).await?;

        let deleted = self
            .metadata_store
            .soft_delete_mail_archive_job(job_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        if !deleted {
            return Err(MailError::JobNotFound(job_id));
        }

        let event = Event::new(
            EventType::MailArchiveJobDeleted,
            job.id,
            AggregateType::MailImportJob,
            serde_json::to_value(MailArchiveJobDeletedPayload {
                job_id: job.id,
                account_id: job.account_id,
            })
            .map_err(|e| MailError::Database(e.to_string()))?,
            owner_id,
        );
        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        Ok(())
    }

    /// Process an IMAP archive job by fetching messages in the date range and
    /// importing each one.
    pub async fn process_archive_job(&self, job: &MailImportJob) -> Result<(), MailError> {
        // Only run pending/running jobs.
        if !matches!(
            job.status.parse::<MailImportJobStatus>().ok(),
            Some(MailImportJobStatus::Pending) | Some(MailImportJobStatus::Running)
        ) {
            return Ok(());
        }

        // Load account.
        let account = match self
            .metadata_store
            .get_mail_account(job.account_id, job.owner_id)
            .await
        {
            Ok(Some(a))
                if a.tenant_id == job.tenant_id && a.deleted_at.is_none() && a.is_enabled =>
            {
                a
            }
            Ok(Some(_)) => {
                self.metadata_store
                    .mark_archive_job_failed_with_retry(job.id, "Account unavailable")
                    .await
                    .ok();
                return Ok(());
            }
            Ok(None) => {
                self.metadata_store
                    .mark_archive_job_failed_with_retry(job.id, "Account not found")
                    .await
                    .ok();
                return Ok(());
            }
            Err(e) => {
                self.metadata_store
                    .mark_archive_job_failed_with_retry(job.id, &format!("Database error: {e}"))
                    .await
                    .ok();
                return Ok(());
            }
        };

        // Decrypt password.
        let password =
            match rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key) {
                Ok(p) => p,
                Err(e) => {
                    self.metadata_store
                        .mark_archive_job_failed_with_retry(
                            job.id,
                            &format!("Decryption error: {e}"),
                        )
                        .await
                        .ok();
                    return Ok(());
                }
            };

        // Mark running.
        if !self
            .metadata_store
            .mark_mail_import_job_running(job.id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
        {
            return Ok(());
        }

        // Emit started event.
        let event = Event::new(
            EventType::MailArchiveJobStarted,
            job.id,
            AggregateType::MailImportJob,
            serde_json::to_value(MailArchiveJobStartedPayload {
                job_id: job.id,
                account_id: job.account_id,
            })
            .map_err(|e| MailError::Database(e.to_string()))?,
            job.owner_id,
        );
        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .ok();

        // Connect.
        let mut session = match self.connect_and_login(&account, &password).await {
            Ok(s) => s,
            Err(e) => {
                self.metadata_store
                    .mark_archive_job_failed_with_retry(job.id, &e.to_string())
                    .await
                    .ok();
                return Ok(());
            }
        };

        // Fetch UID range.
        let (uid_validity, uids) = match session
            .fetch_uids_by_date_range(&job.folder_name, job.archive_since, job.archive_before)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.metadata_store
                    .mark_archive_job_failed_with_retry(job.id, &e.to_string())
                    .await
                    .ok();
                return Ok(());
            }
        };

        let uid_validity = uid_validity.map(i64::from);

        // Reset last_imported_uid if UIDVALIDITY changed.
        let last_imported_uid = if job.last_uid_validity != uid_validity {
            None
        } else {
            job.last_imported_uid
        };

        let mut processed = 0i32;
        let mut failed = 0i32;
        let mut last_uid: Option<i64> = last_imported_uid;

        // Update progress watermark before the loop.
        self.metadata_store
            .update_mail_archive_job_progress(
                job.id,
                processed,
                failed,
                uid_validity,
                last_imported_uid,
                None,
            )
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        for uid in uids {
            let uid_i64 = i64::from(uid);

            // Skip already imported UIDs.
            if let Some(last) = last_imported_uid {
                if uid_i64 <= last {
                    continue;
                }
            }

            // Check cancellation.
            if let Ok(Some(status)) = self
                .metadata_store
                .get_mail_import_job_status(job.id, job.owner_id)
                .await
            {
                if status == "cancelled" {
                    break;
                }
            }

            // Fetch raw message.
            let raw = match session
                .fetch_rfc822(&job.folder_name, uid, uid_validity)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    failed += 1;
                    self.metadata_store
                        .update_mail_archive_job_progress(
                            job.id,
                            processed,
                            failed,
                            uid_validity,
                            last_uid,
                            Some(&e.to_string()),
                        )
                        .await
                        .ok();
                    continue;
                }
            };

            // Import via existing path.
            match self
                .import_raw_source(
                    job.tenant_id,
                    job.owner_id,
                    job.owner_id,
                    Some(job.account_id),
                    MailSourceMode::ImapArchive,
                    Some(&job.folder_name),
                    Some(uid_i64),
                    uid_validity,
                    raw,
                    Some(job.id),
                )
                .await
            {
                Ok(msg) => {
                    processed += 1;
                    last_uid = Some(uid_i64);
                    let event = Event::new(
                        EventType::MailImported,
                        msg.id,
                        AggregateType::MailMessage,
                        serde_json::to_value(MailImportedPayload {
                            message_id: msg.id,
                            account_id: job.account_id,
                            folder_name: job.folder_name.clone(),
                            source_uid: uid_i64,
                            owner_id: job.owner_id,
                        })
                        .map_err(|e| MailError::Database(e.to_string()))?,
                        job.owner_id,
                    );
                    self.event_store
                        .append(&event, &self.broadcaster)
                        .await
                        .ok();
                }
                Err(e) => {
                    failed += 1;
                    self.metadata_store
                        .update_mail_archive_job_progress(
                            job.id,
                            processed,
                            failed,
                            uid_validity,
                            last_uid,
                            Some(&e.to_string()),
                        )
                        .await
                        .ok();
                }
            }

            // Update progress periodically.
            self.metadata_store
                .update_mail_archive_job_progress(
                    job.id,
                    processed,
                    failed,
                    uid_validity,
                    last_uid,
                    None,
                )
                .await
                .ok();
        }

        // Apply retention.
        if let Some(retention_days) = job.retention_days {
            if retention_days > 0 {
                self.metadata_store
                    .apply_archive_retention(
                        job.owner_id,
                        job.account_id,
                        &job.folder_name,
                        retention_days,
                    )
                    .await
                    .ok();
            }
        }

        // Mark completed if not cancelled.
        if let Ok(Some(status)) = self
            .metadata_store
            .get_mail_import_job_status(job.id, job.owner_id)
            .await
        {
            if status == "cancelled" {
                return Ok(());
            }
        }

        self.metadata_store
            .mark_mail_import_job_completed(job.id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        let event = Event::new(
            EventType::MailArchiveJobCompleted,
            job.id,
            AggregateType::MailImportJob,
            serde_json::to_value(MailArchiveJobCompletedPayload {
                job_id: job.id,
                account_id: job.account_id,
                processed_messages: processed,
            })
            .map_err(|e| MailError::Database(e.to_string()))?,
            job.owner_id,
        );
        self.event_store
            .append(&event, &self.broadcaster)
            .await
            .ok();

        Ok(())
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
        let port = u16::try_from(account.port).map_err(|_| {
            MailError::InvalidSource(format!("Invalid IMAP port: {}", account.port))
        })?;
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

fn mail_account_db_error(name: &str, err: anyhow::Error) -> MailError {
    if err.downcast_ref::<sqlx::Error>().is_some_and(
        |e| matches!(e, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505")),
    ) {
        MailError::DuplicateAccountName(name.to_string())
    } else {
        MailError::Database(err.to_string())
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
            broadcaster.clone(),
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
            broadcaster,
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
