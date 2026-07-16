use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use rustshare_core::domain::{
    Folder, LinkTargetType, MailAccount, MailAccountId, MailAttachment, MailImportJob,
    MailImportJobId, MailImportJobStatus, MailLink, MailMessage, MailMessagePart, MailSmtpSettings,
    MailSourceMode, MailTlsMode, MailVisibility, SharePermissions, UserId,
};
use rustshare_core::events::{
    AggregateType, Event, EventType, MailAccountCreatedPayload, MailAccountDeletedPayload,
    MailArchiveJobCancelledPayload, MailArchiveJobCompletedPayload, MailArchiveJobCreatedPayload,
    MailArchiveJobDeletedPayload, MailArchiveJobFailedPayload, MailArchiveJobStartedPayload,
    MailImportedPayload, MailLinkedPayload, MailUnlinkedPayload,
};
use rustshare_core::services::eml_parser::EmlParser;
use rustshare_core::services::{
    EmailService, FileService, FolderService, OutboundMailMessage, PermissionResolver,
};
use rustshare_crypto::{encrypt_secret, SecretEncryptionKey};
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use rustshare_storage::{EventStore, MetadataStore, ObjectStore};
use sha2::{Digest, Sha256};
use std::str::FromStr;
use uuid::Uuid;

use super::imap_client::{
    ImapArchiveSession, ImapClient, ImapError, ImapMailboxSession, ImapMessageSummary, ImapSession,
    MailFolder,
};

const MAX_MAIL_ARTIFACT_NAME_LEN: usize = 200;
const MAX_MAIL_FOLDER_SUBJECT_SLUG_LEN: usize = 200;
const MAX_OUTBOUND_MAIL_ATTACHMENT_BYTES: i64 = 25 * 1024 * 1024;
const MAX_MAIL_SEND_RECIPIENTS: usize = 50;
const MAX_MAIL_SEND_BODY_BYTES: usize = 256 * 1024;
const MAX_MAIL_SEND_ATTACHMENTS: usize = 20;

fn validate_outbound_mail(
    to: &[String],
    cc: &[String],
    bcc: &[String],
    subject: &str,
    body: &str,
    attachment_count: usize,
) -> Result<(), MailError> {
    let recipient_count = to.len() + cc.len() + bcc.len();
    if recipient_count == 0 {
        return Err(MailError::InvalidSource(
            "At least one recipient is required".to_string(),
        ));
    }
    if recipient_count > MAX_MAIL_SEND_RECIPIENTS {
        return Err(MailError::InvalidSource(format!(
            "At most {MAX_MAIL_SEND_RECIPIENTS} recipients are allowed"
        )));
    }
    if subject.trim().is_empty() {
        return Err(MailError::InvalidSource("Subject is required".to_string()));
    }
    if subject.len() > 998 {
        return Err(MailError::InvalidSource("Subject is too long".to_string()));
    }
    if body.trim().is_empty() {
        return Err(MailError::InvalidSource("Body is required".to_string()));
    }
    if body.len() > MAX_MAIL_SEND_BODY_BYTES {
        return Err(MailError::InvalidSource(
            "Message body is too large".to_string(),
        ));
    }
    if attachment_count > MAX_MAIL_SEND_ATTACHMENTS {
        return Err(MailError::InvalidSource(format!(
            "At most {MAX_MAIL_SEND_ATTACHMENTS} attachments are allowed"
        )));
    }
    if to
        .iter()
        .chain(cc.iter())
        .chain(bcc.iter())
        .any(|address| address.trim().is_empty() || address.len() > 512)
    {
        return Err(MailError::InvalidSource(
            "Recipient addresses are invalid".to_string(),
        ));
    }
    Ok(())
}

fn draft_attachment_file_ids(attachments: Vec<MailAttachment>) -> Result<Vec<Uuid>, MailError> {
    attachments
        .into_iter()
        .map(|attachment| {
            attachment.file_id.ok_or_else(|| {
                MailError::InvalidSource(format!(
                    "Draft attachment '{}' is no longer available",
                    attachment.filename
                ))
            })
        })
        .collect()
}

fn visible_in_imported_mail_list(msg: &MailMessage) -> bool {
    msg.source_mode != "draft"
}

enum MailboxMutation<'a> {
    MarkSeen(bool),
    Move(&'a str),
    Delete,
}

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
    #[error("Invalid job state: {0}")]
    JobInvalidState(String),
    #[error("Mail account name already exists: {0}")]
    DuplicateAccountName(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("IMAP error: {0}")]
    Imap(String),
    #[error("SMTP send failed")]
    Smtp,
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
            None,
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
        archive_job_id: Option<MailImportJobId>,
        custom_message_id: Option<Uuid>,
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
        if let Some(cid) = custom_message_id {
            msg.id = cid;
        }
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
        msg.archive_job_id = archive_job_id;
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
                match self
                    .metadata_store
                    .get_mail_import_job_status(jid, owner_id)
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?
                {
                    Some(status) if status == "running" => {}
                    _ => return Err(MailError::Cancelled),
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

    /// Get a single imported mail message if owned by `owner_id` in `tenant_id`.
    pub async fn get_message(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        message_id: Uuid,
    ) -> Result<MailMessage, MailError> {
        let msg = self
            .metadata_store
            .find_mail_message_by_id(message_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or(MailError::NotFound(message_id))?;
        if msg.tenant_id != tenant_id || msg.owner_id != owner_id {
            return Err(MailError::PermissionDenied);
        }
        Ok(msg)
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
        Ok(self
            .metadata_store
            .list_mail_messages(tenant_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .into_iter()
            .filter(visible_in_imported_mail_list)
            .collect())
    }

    pub async fn list_drafts(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        account_id: MailAccountId,
    ) -> Result<Vec<MailMessage>, MailError> {
        self.get_account(tenant_id, owner_id, account_id).await?;
        Ok(self
            .metadata_store
            .list_mail_messages(tenant_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .into_iter()
            .filter(|msg| msg.account_id == Some(account_id) && msg.source_mode == "draft")
            .collect())
    }

    pub async fn get_draft(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        account_id: MailAccountId,
        draft_id: Uuid,
    ) -> Result<MailMessage, MailError> {
        let msg = self.get_message(tenant_id, owner_id, draft_id).await?;
        if msg.account_id != Some(account_id) || msg.source_mode != "draft" {
            return Err(MailError::NotFound(draft_id));
        }
        Ok(msg)
    }

    /// List body parts for a message, scoped to the owning user and tenant.
    pub async fn list_parts(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        message_id: Uuid,
    ) -> Result<Vec<MailMessagePart>, MailError> {
        // Verify the caller can access the message first so cross-tenant requests
        // are rejected with a permission error rather than an empty list.
        self.get_message(tenant_id, owner_id, message_id).await?;
        self.metadata_store
            .list_mail_message_parts_by_message_id(message_id, tenant_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))
    }

    /// Fetch a single message part and its blob bytes.
    pub async fn get_message_part(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        message_id: Uuid,
        part_id: Uuid,
    ) -> Result<(MailMessagePart, bytes::Bytes), MailError> {
        let part = self
            .metadata_store
            .find_mail_message_part_by_id(part_id, message_id, tenant_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or(MailError::NotFound(part_id))?;
        let blob_key = part
            .blob_key
            .clone()
            .ok_or_else(|| MailError::InvalidSource("part has no blob".to_string()))?;
        let bytes = self
            .object_store
            .get(&blob_key)
            .await
            .map_err(|e| MailError::Storage(e.to_string()))?;
        Ok((part, bytes))
    }

    /// Download the original raw `.eml` source for a message.
    pub async fn download_message_source(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        message_id: Uuid,
    ) -> Result<(String, bytes::Bytes), MailError> {
        let msg = self.get_message(tenant_id, owner_id, message_id).await?;
        let blob_key = msg
            .blob_key
            .ok_or_else(|| MailError::InvalidSource("message has no source blob".to_string()))?;
        let bytes = self
            .object_store
            .get(&blob_key)
            .await
            .map_err(|e| MailError::Storage(e.to_string()))?;
        let filename = format!("message-{message_id}.eml");
        Ok((filename, bytes))
    }

    /// List attachments for a message, scoped to the owning user and tenant.
    pub async fn list_attachments(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
        message_id: Uuid,
    ) -> Result<Vec<MailAttachment>, MailError> {
        // Verify the caller can access the message first so cross-tenant requests
        // are rejected with a permission error rather than an empty list.
        self.get_message(tenant_id, owner_id, message_id).await?;
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
        before_uid: Option<u32>,
    ) -> Result<(Option<u32>, Vec<ImapMessageSummary>), MailError> {
        let account = self.get_account(tenant_id, owner_id, account_id).await?;
        let password = rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key)
            .map_err(|e| MailError::Storage(format!("failed to decrypt password: {e}")))?;
        let mut session = self.connect_and_login(&account, &password).await?;
        let result = session
            .fetch_message_summaries(folder, limit, before_uid)
            .await
            .map_err(imap_to_mail_error)?;
        let _ = session.logout().await;
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn mark_imap_message_seen(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        folder: &str,
        expected_uidvalidity: Option<i64>,
        uid: u32,
        seen: bool,
    ) -> Result<(), MailError> {
        let account = self.get_account(tenant_id, owner_id, account_id).await?;
        let password = rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key)
            .map_err(|e| MailError::Storage(format!("failed to decrypt password: {e}")))?;
        let mut session = self.connect_and_login(&account, &password).await?;
        let result = run_imap_mailbox_mutation(
            &mut session,
            folder,
            expected_uidvalidity,
            uid,
            MailboxMutation::MarkSeen(seen),
        )
        .await;
        let _ = session.logout().await;
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn move_imap_message(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        folder: &str,
        expected_uidvalidity: Option<i64>,
        uid: u32,
        destination_folder: &str,
    ) -> Result<(), MailError> {
        let account = self.get_account(tenant_id, owner_id, account_id).await?;
        let password = rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key)
            .map_err(|e| MailError::Storage(format!("failed to decrypt password: {e}")))?;
        let mut session = self.connect_and_login(&account, &password).await?;
        let result = run_imap_mailbox_mutation(
            &mut session,
            folder,
            expected_uidvalidity,
            uid,
            MailboxMutation::Move(destination_folder),
        )
        .await;
        let _ = session.logout().await;
        result
    }

    pub async fn delete_imap_message(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        folder: &str,
        expected_uidvalidity: Option<i64>,
        uid: u32,
    ) -> Result<(), MailError> {
        let account = self.get_account(tenant_id, owner_id, account_id).await?;
        let password = rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key)
            .map_err(|e| MailError::Storage(format!("failed to decrypt password: {e}")))?;
        let mut session = self.connect_and_login(&account, &password).await?;
        let result = run_imap_mailbox_mutation(
            &mut session,
            folder,
            expected_uidvalidity,
            uid,
            MailboxMutation::Delete,
        )
        .await;
        let _ = session.logout().await;
        result
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
        if job.source_mode != MailSourceMode::ImapSelected.as_str() {
            return Err(MailError::JobNotFound(job_id));
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
                            None,
                            None,
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
    ///
    /// `max_retries` is the maximum total number of attempts for the job,
    /// including the initial attempt. For example, a value of `3` allows one
    /// initial run plus up to two retries.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_archive_job(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        folder_name: String,
        archive_since: Option<NaiveDate>,
        archive_before: Option<NaiveDate>,
        retention_days: Option<i32>,
        max_retries: Option<i32>,
    ) -> Result<MailImportJob, MailError> {
        let account = self
            .metadata_store
            .get_mail_account(account_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or(MailError::AccountNotFound(account_id))?;

        if !account.is_enabled {
            return Err(MailError::AccountNotFound(account_id));
        }

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
        if max_retries < 1 {
            return Err(MailError::InvalidSource(
                "max_retries must be at least 1".to_string(),
            ));
        }
        if let Some(retention_days) = retention_days {
            if retention_days <= 0 {
                return Err(MailError::InvalidSource(
                    "retention_days must be positive".to_string(),
                ));
            }
        }
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

        let mut tx = self
            .event_store
            .begin_transaction()
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        self.event_store
            .append_in_tx(&mut tx, &event)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        self.metadata_store
            .create_mail_import_job_in_tx(&mut tx, &job)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        self.broadcaster.publish(event);

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
            .list_mail_archive_jobs_by_owner(tenant_id, owner_id, account_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        Ok(jobs)
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
            return Err(MailError::JobInvalidState(
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
    ///
    /// The metadata store atomically sets the status to `cancelled` while
    /// soft-deleting, so a worker already inside `process_archive_session`
    /// sees the cancellation and exits cleanly instead of getting stuck in a
    /// `running` state against a deleted row.
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

    /// Record a failed archive job attempt with retry semantics and emit a
    /// `MailArchiveJobFailed` domain event when retries are exhausted.
    async fn emit_archive_job_failed_event(&self, job: &MailImportJob, error: &str) {
        match self
            .metadata_store
            .mark_archive_job_failed_with_retry(job.id, error)
            .await
        {
            Ok(Some(status)) if status == "failed" => {
                if let Ok(payload) = serde_json::to_value(MailArchiveJobFailedPayload {
                    job_id: job.id,
                    account_id: job.account_id,
                    error: error.to_string(),
                }) {
                    let event = Event::new(
                        EventType::MailArchiveJobFailed,
                        job.id,
                        AggregateType::MailImportJob,
                        payload,
                        job.owner_id,
                    );
                    if let Err(e) = self.event_store.append(&event, &self.broadcaster).await {
                        tracing::error!(
                            job_id = %job.id,
                            error = %e,
                            "Failed to append MailArchiveJobFailed event"
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// If `message` is owned by another archive job that is no longer active
    /// (deleted, cancelled, or failed), reassign it to `job` so the current
    /// job's retention policy applies. Active overlapping jobs keep their
    /// ownership to avoid a ping-pong race.
    async fn maybe_reassign_archive_message_ownership(
        &self,
        message: &MailMessage,
        job: &MailImportJob,
    ) -> Result<(), MailError> {
        let Some(existing_job_id) = message.archive_job_id else {
            return Ok(());
        };
        if existing_job_id == job.id {
            return Ok(());
        }
        let other_status = self
            .metadata_store
            .get_mail_import_job_status(existing_job_id, job.owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        let is_inactive = match other_status.as_deref() {
            None => true, // soft-deleted
            Some("cancelled") | Some("failed") => true,
            _ => false,
        };
        if is_inactive {
            self.metadata_store
                .update_mail_message_archive_job_id(
                    message.id,
                    job.owner_id,
                    message.archive_job_id,
                    job.id,
                )
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
        }
        Ok(())
    }

    /// Process an IMAP archive session for `job` without connecting or logging
    /// in. This is the testable core of [`process_archive_job`].
    pub async fn process_archive_session(
        &self,
        job: &MailImportJob,
        session: &mut dyn ImapArchiveSession,
    ) -> Result<(), MailError> {
        // Fetch UID range.
        let (uid_validity, uids) = match session
            .fetch_uids_by_date_range(&job.folder_name, job.archive_since, job.archive_before)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.emit_archive_job_failed_event(job, &e.to_string())
                    .await;
                return Ok(());
            }
        };

        let uid_validity = uid_validity.map(i64::from);

        // Update total message count now that we know the UID list size.
        let total = uids.len() as i32;
        self.metadata_store
            .update_mail_archive_job_total(job.id, total)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        // Reset last_imported_uid if UIDVALIDITY changed.
        let last_imported_uid = if job.last_uid_validity != uid_validity {
            None
        } else {
            job.last_imported_uid
        };

        let mut processed = last_imported_uid
            .map(|last| uids.iter().filter(|&&uid| i64::from(uid) <= last).count() as i32)
            .unwrap_or(0);
        let mut failed = 0i32;
        let mut failed_once = false;
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
            match self
                .metadata_store
                .get_mail_import_job_status(job.id, job.owner_id)
                .await
            {
                Ok(Some(status)) if status == "running" => {}
                Ok(_) => {
                    break;
                }
                Err(e) => return Err(MailError::Database(e.to_string())),
            }

            // Skip UIDs that were already imported by this archive job.
            match self
                .metadata_store
                .find_mail_message_by_source(
                    job.owner_id,
                    job.account_id,
                    "imap_archive",
                    &job.folder_name,
                    uid_i64,
                    uid_validity,
                )
                .await
            {
                Ok(Some(existing)) => {
                    if existing.folder_id.is_none() {
                        // A previous run crashed after inserting the
                        // deduplication row but before artifacts were durable.
                        // If another archive job still owns that partial row,
                        // leave it alone and retry this job later.
                        if let Some(archive_job_id) = existing.archive_job_id {
                            if archive_job_id != job.id {
                                let status = self
                                    .metadata_store
                                    .get_mail_import_job_status(archive_job_id, job.owner_id)
                                    .await
                                    .map_err(|e| MailError::Database(e.to_string()))?;
                                if status.as_deref() == Some("running") {
                                    failed += 1;
                                    failed_once = true;
                                    self.metadata_store
                                        .update_mail_archive_job_progress(
                                            job.id,
                                            processed,
                                            failed,
                                            uid_validity,
                                            last_uid,
                                            Some("Partial row is still owned by another running archive job"),
                                        )
                                        .await
                                        .ok();
                                    continue;
                                }
                            }
                        }
                        // Reclaim the abandoned partial row and re-import this UID.
                        if let Err(e) = self
                            .metadata_store
                            .delete_mail_message(existing.id, job.owner_id)
                            .await
                        {
                            failed += 1;
                            failed_once = true;
                            self.metadata_store
                                .update_mail_archive_job_progress(
                                    job.id,
                                    processed,
                                    failed,
                                    uid_validity,
                                    last_uid,
                                    Some(&format!(
                                        "Failed to reclaim incomplete message {}: {e}",
                                        existing.id
                                    )),
                                )
                                .await
                                .ok();
                            continue;
                        }
                    } else {
                        // Already imported under this UIDVALIDITY.
                        // If the row is owned by another archive job that has
                        // since been deleted, reassign it to the current job so
                        // the current job's retention policy applies. Active
                        // overlapping jobs keep their ownership to avoid a
                        // ping-pong race.
                        self.maybe_reassign_archive_message_ownership(&existing, job)
                            .await?;

                        // Advance the watermark to this UID as long as no
                        // earlier UID in this run failed. Gaps are permanently
                        // missing messages within the current UIDVALIDITY.
                        processed += 1;
                        if !failed_once {
                            last_uid = Some(uid_i64);
                        }
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
                            .map_err(|e| MailError::Database(e.to_string()))?;
                        continue;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    failed += 1;
                    failed_once = true;
                    self.metadata_store
                        .update_mail_archive_job_progress(
                            job.id,
                            processed,
                            failed,
                            uid_validity,
                            last_uid,
                            Some(&format!("Deduplication lookup failed: {e}")),
                        )
                        .await
                        .ok();
                    continue;
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
                    failed_once = true;
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
                    Some(job.id),
                    None,
                )
                .await
            {
                Ok(msg) => {
                    self.maybe_reassign_archive_message_ownership(&msg, job)
                        .await?;

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
                    match self.event_store.append(&event, &self.broadcaster).await {
                        Ok(()) => {
                            processed += 1;
                            if !failed_once {
                                last_uid = Some(uid_i64);
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                message_id = %msg.id,
                                error = %e,
                                "Failed to append MailImported event"
                            );
                            failed += 1;
                            failed_once = true;
                            let err = format!("Failed to append MailImported event: {e}");
                            self.metadata_store
                                .update_mail_archive_job_progress(
                                    job.id,
                                    processed,
                                    failed,
                                    uid_validity,
                                    last_uid,
                                    Some(&err),
                                )
                                .await
                                .ok();
                        }
                    }
                }
                Err(MailError::Cancelled) => {
                    // The row status is expected to already be `cancelled`
                    // because `cancel_archive_job` updates it before the
                    // import path observes the cancellation.
                    return Ok(());
                }
                Err(e) => {
                    failed += 1;
                    failed_once = true;
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
            if let Err(e) = self
                .metadata_store
                .update_mail_archive_job_progress(
                    job.id,
                    processed,
                    failed,
                    uid_validity,
                    last_uid,
                    None,
                )
                .await
            {
                tracing::error!(
                    job_id = %job.id,
                    error = %e,
                    "Failed to update archive job progress"
                );
            }
        }

        // Apply retention only if the job is still running. If it was cancelled
        // during the scan, skip retention so a user-requested stop does not
        // delete aged messages.
        let still_running = match self
            .metadata_store
            .get_mail_import_job_status(job.id, job.owner_id)
            .await
        {
            Ok(Some(status)) if status == "running" => true,
            Ok(_) => false,
            Err(e) => return Err(MailError::Database(e.to_string())),
        };
        if still_running {
            if let Some(retention_days) = job.retention_days {
                if retention_days > 0 {
                    self.metadata_store
                        .apply_archive_retention(
                            job.owner_id,
                            job.account_id,
                            &job.folder_name,
                            job.id,
                            retention_days,
                        )
                        .await
                        .map_err(|e| {
                            MailError::Database(format!("Retention cleanup failed: {e}"))
                        })?;
                }
            }
        }

        // Mark completed if not cancelled and no failures.
        match self
            .metadata_store
            .get_mail_import_job_status(job.id, job.owner_id)
            .await
        {
            Ok(Some(status)) if status == "running" => {}
            Ok(_) => {
                return Ok(());
            }
            Err(e) => return Err(MailError::Database(e.to_string())),
        }

        if failed > 0 {
            self.emit_archive_job_failed_event(job, &format!("{failed} messages failed to import"))
                .await;
            return Ok(());
        }

        let requeued = self
            .metadata_store
            .requeue_mail_archive_job(job.id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        if !requeued {
            return Ok(());
        }

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
        if let Err(e) = self.event_store.append(&event, &self.broadcaster).await {
            tracing::error!(
                job_id = %job.id,
                error = %e,
                "Failed to append MailArchiveJobCompleted event"
            );
        }

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

        // Mark running first so that any subsequent failure can be recorded
        // with retry/backoff semantics.
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
        if let Err(e) = self.event_store.append(&event, &self.broadcaster).await {
            tracing::error!(
                job_id = %job.id,
                error = %e,
                "Failed to append MailArchiveJobStarted event"
            );
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
                self.emit_archive_job_failed_event(job, "Account unavailable")
                    .await;
                return Ok(());
            }
            Ok(None) => {
                self.emit_archive_job_failed_event(job, "Account not found")
                    .await;
                return Ok(());
            }
            Err(e) => {
                self.emit_archive_job_failed_event(job, &format!("Database error: {e}"))
                    .await;
                return Ok(());
            }
        };

        // Decrypt password.
        let password =
            match rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key) {
                Ok(p) => p,
                Err(e) => {
                    self.emit_archive_job_failed_event(job, &format!("Decryption error: {e}"))
                        .await;
                    return Ok(());
                }
            };

        // Connect.
        let mut session = match self.connect_and_login(&account, &password).await {
            Ok(s) => s,
            Err(e) => {
                self.emit_archive_job_failed_event(job, &e.to_string())
                    .await;
                return Ok(());
            }
        };

        let process_result = self.process_archive_session(job, &mut session).await;

        let _ = session.logout().await;

        if let Err(ref e) = process_result {
            self.emit_archive_job_failed_event(job, &e.to_string())
                .await;
            return Ok(());
        }
        process_result
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
    pub async fn get_smtp_settings(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
    ) -> Result<Option<MailSmtpSettings>, MailError> {
        self.get_account(tenant_id, owner_id, account_id).await?;

        self.metadata_store
            .get_mail_smtp_settings(account_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_or_update_smtp_settings(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        host: String,
        port: i32,
        username: String,
        password: Option<String>,
        tls_mode: MailTlsMode,
        from_address: String,
        from_name: Option<String>,
        reply_to: Option<String>,
        sent_folder: Option<String>,
        is_enabled: bool,
    ) -> Result<MailSmtpSettings, MailError> {
        self.get_account(tenant_id, owner_id, account_id).await?;
        if tls_mode == MailTlsMode::None {
            return Err(MailError::InvalidSource(
                "plaintext SMTP (tls_mode: none) is not allowed; use tls or starttls".to_string(),
            ));
        }

        let existing = self
            .metadata_store
            .get_mail_smtp_settings(account_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        let password_enc = match password {
            Some(pass) => encrypt_secret(&pass, &self.secret_key)
                .map_err(|e| MailError::Storage(format!("failed to encrypt SMTP password: {e}")))?,
            None => {
                if let Some(ref ext) = existing {
                    ext.password_enc.clone()
                } else {
                    return Err(MailError::InvalidSource(
                        "SMTP password is required for initial configuration".to_string(),
                    ));
                }
            }
        };

        let settings = MailSmtpSettings {
            id: existing.as_ref().map(|e| e.id).unwrap_or_else(Uuid::new_v4),
            tenant_id,
            owner_id,
            mail_account_id: account_id,
            host,
            port,
            username,
            password_enc,
            tls_mode: tls_mode.to_string(),
            from_address,
            from_name,
            reply_to,
            sent_folder,
            is_enabled,
            created_at: existing
                .as_ref()
                .map(|e| e.created_at)
                .unwrap_or_else(Utc::now),
            updated_at: Utc::now(),
        };

        if existing.is_some() {
            self.metadata_store
                .update_mail_smtp_settings(&settings)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;

            let event = Event::new(
                EventType::MailSmtpSettingsUpdated,
                account_id,
                AggregateType::MailAccount,
                serde_json::json!({
                    "mail_account_id": account_id,
                    "host": settings.host.clone(),
                    "username": settings.username.clone(),
                }),
                owner_id,
            );
            let _ = self.event_store.append(&event, &self.broadcaster).await;
        } else {
            self.metadata_store
                .create_mail_smtp_settings(&settings)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;

            let event = Event::new(
                EventType::MailSmtpSettingsCreated,
                account_id,
                AggregateType::MailAccount,
                serde_json::json!({
                    "mail_account_id": account_id,
                    "host": settings.host.clone(),
                    "username": settings.username.clone(),
                }),
                owner_id,
            );
            let _ = self.event_store.append(&event, &self.broadcaster).await;
        }

        Ok(settings)
    }

    pub async fn delete_smtp_settings(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
    ) -> Result<(), MailError> {
        self.get_account(tenant_id, owner_id, account_id).await?;

        let deleted = self
            .metadata_store
            .delete_mail_smtp_settings(account_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        if deleted {
            let event = Event::new(
                EventType::MailSmtpSettingsDeleted,
                account_id,
                AggregateType::MailAccount,
                serde_json::json!({
                    "mail_account_id": account_id,
                }),
                owner_id,
            );
            let _ = self.event_store.append(&event, &self.broadcaster).await;
        }

        Ok(())
    }

    pub async fn test_smtp_connection(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
    ) -> Result<(), MailError> {
        let settings = self
            .get_smtp_settings(tenant_id, owner_id, account_id)
            .await?
            .ok_or_else(|| MailError::InvalidSource("SMTP settings not configured".to_string()))?;

        let email_service = EmailService::new(
            self.metadata_store.pool().clone(),
            (*self.secret_key).clone(),
        );
        email_service
            .send_user_email_via_smtp(
                &settings,
                OutboundMailMessage {
                    recipients: std::slice::from_ref(&settings.from_address),
                    cc: &[],
                    bcc: &[],
                    subject: "SMTP Connection Test",
                    body: "This is a test message to verify your SMTP settings.",
                    in_reply_to: None,
                    references: None,
                    attachments: vec![],
                },
            )
            .await
            .map_err(|e| MailError::InvalidSource(e.to_string()))?;

        let event = Event::new(
            EventType::MailSmtpConnectionTested,
            account_id,
            AggregateType::MailAccount,
            serde_json::json!({
                "mail_account_id": account_id,
                "status": "success",
            }),
            owner_id,
        );
        let _ = self.event_store.append(&event, &self.broadcaster).await;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_outbound_mail(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        to: Vec<String>,
        cc: Vec<String>,
        bcc: Vec<String>,
        subject: String,
        body: String,
        attachment_ids: Vec<Uuid>,
        in_reply_to_msg_id: Option<Uuid>,
        is_forward: bool,
    ) -> Result<MailMessage, MailError> {
        validate_outbound_mail(&to, &cc, &bcc, &subject, &body, attachment_ids.len())?;

        let account = self.get_account(tenant_id, owner_id, account_id).await?;

        let smtp = self
            .metadata_store
            .get_mail_smtp_settings(account_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .ok_or_else(|| {
                MailError::InvalidSource(
                    "SMTP settings not configured for this account".to_string(),
                )
            })?;

        if !smtp.is_enabled {
            return Err(MailError::InvalidSource(
                "SMTP settings are disabled".to_string(),
            ));
        }

        let mut smtp_attachments = Vec::new();
        let mut attachment_bytes = 0_i64;
        for file_id in &attachment_ids {
            let file = self
                .file_service
                .get_file(*file_id, owner_id)
                .await
                .map_err(|_| MailError::PermissionDenied)?;

            if file.tenant_id != tenant_id {
                return Err(MailError::PermissionDenied);
            }
            attachment_bytes = attachment_bytes.saturating_add(file.size);
            if attachment_bytes > MAX_OUTBOUND_MAIL_ATTACHMENT_BYTES {
                return Err(MailError::InvalidSource(
                    "Mail attachments are too large".to_string(),
                ));
            }

            let content = self
                .object_store
                .get(&file.storage_key())
                .await
                .map_err(|e| MailError::Storage(format!("failed to fetch file content: {e}")))?;

            smtp_attachments.push(rustshare_core::services::SmtpAttachment {
                filename: file.name.clone(),
                mime_type: file.mime_type.clone(),
                content: content.to_vec(),
            });
        }

        let mut in_reply_to = None;
        let mut references = None;
        if let Some(reply_to_id) = in_reply_to_msg_id {
            if let Some(orig_msg) = self
                .metadata_store
                .find_mail_message_by_id(reply_to_id)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?
            {
                if orig_msg.tenant_id != tenant_id || orig_msg.owner_id != owner_id {
                    return Err(MailError::PermissionDenied);
                }

                if let Some(ref msg_id) = orig_msg.message_id {
                    in_reply_to = Some(msg_id.clone());
                    let mut refs = Vec::new();
                    if let Some(ref orig_refs) = orig_msg.references {
                        refs.extend(orig_refs.clone());
                    }
                    refs.push(msg_id.clone());
                    references = Some(refs.join(" "));
                }
            } else {
                return Err(MailError::NotFound(reply_to_id));
            }
        }

        let email_service = EmailService::new(
            self.metadata_store.pool().clone(),
            (*self.secret_key).clone(),
        );
        let email = OutboundMailMessage {
            recipients: &to,
            cc: &cc,
            bcc: &bcc,
            subject: &subject,
            body: &body,
            in_reply_to,
            references,
            attachments: smtp_attachments,
        };

        let raw_eml = email_service
            .send_user_email_via_smtp(&smtp, email)
            .await
            .map_err(|e| {
                let event = Event::new(
                    EventType::MailSendFailed,
                    account_id,
                    AggregateType::MailAccount,
                    serde_json::json!({
                        "mail_account_id": account_id,
                        "error": e.to_string(),
                    }),
                    owner_id,
                );
                drop(tokio::spawn({
                    let event_store = self.event_store.clone();
                    let broadcaster = self.broadcaster.clone();
                    async move {
                        let _ = event_store.append(&event, &broadcaster).await;
                    }
                }));
                MailError::Smtp
            })?;

        let mail_message = match self
            .import_raw_source(
                tenant_id,
                owner_id,
                owner_id,
                Some(account_id),
                MailSourceMode::Outbound,
                None,
                None,
                None,
                raw_eml.clone(),
                None,
                None,
                None,
            )
            .await
        {
            Ok(msg) => msg,
            Err(e) => {
                tracing::error!(
                    "Outbound mail sent via SMTP, but local import/storage failed: {:?}",
                    e
                );
                // Fallback: construct in-memory MailMessage to prevent API retry
                let parsed = EmlParser::parse(&raw_eml).map_err(|pe| {
                    MailError::InvalidSource(format!("Failed to parse raw sent EML fallback: {pe}"))
                })?;
                let mut fallback_msg =
                    MailMessage::new(tenant_id, owner_id, owner_id, MailSourceMode::Outbound);
                fallback_msg.account_id = Some(account_id);
                fallback_msg.subject = parsed.subject;
                fallback_msg.from_address = parsed.from.as_ref().map(|a| a.address.clone());
                fallback_msg.from_name = parsed.from.as_ref().and_then(|a| a.name.clone());
                fallback_msg.to_addresses = addresses_to_json(&parsed.to);
                fallback_msg.cc_addresses = addresses_to_json(&parsed.cc);
                fallback_msg.bcc_addresses = addresses_to_json(&parsed.bcc);
                fallback_msg.sent_at = parsed.sent_at;
                fallback_msg.has_attachments = !parsed.attachments.is_empty();
                fallback_msg.visibility = MailVisibility::Private.into();
                fallback_msg
            }
        };

        let mut append_failed = false;
        if let Some(ref sent_folder) = smtp.sent_folder {
            if !sent_folder.trim().is_empty() {
                let decrypted_imap_password =
                    rustshare_crypto::decrypt_secret(&account.password_enc, &self.secret_key)
                        .map_err(|e| {
                            MailError::Storage(format!("failed to decrypt IMAP password: {e}"))
                        })?;

                let imap_client = ImapClient::connect(
                    &account.host,
                    account.port as u16,
                    MailTlsMode::from_str(&account.tls_mode).unwrap_or(MailTlsMode::Tls),
                )
                .await;
                match imap_client {
                    Ok(client) => {
                        let session_res =
                            ImapSession::login(client, &account.username, &decrypted_imap_password)
                                .await;
                        match session_res {
                            Ok(mut session) => {
                                if let Err(e) = session.append_message(sent_folder, &raw_eml).await
                                {
                                    append_failed = true;
                                    tracing::warn!(
                                        "Failed to append sent mail to IMAP Sent folder: {:?}",
                                        e
                                    );
                                    let event = Event::new(
                                        EventType::MailSentFolderAppendFailed,
                                        mail_message.id,
                                        AggregateType::MailMessage,
                                        serde_json::json!({
                                            "mail_message_id": mail_message.id,
                                            "error": e.to_string(),
                                        }),
                                        owner_id,
                                    );
                                    let _ =
                                        self.event_store.append(&event, &self.broadcaster).await;
                                }
                                let _ = session.logout().await;
                            }
                            Err(e) => {
                                append_failed = true;
                                tracing::warn!(
                                    "Failed to login to IMAP for Sent folder append: {:?}",
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        append_failed = true;
                        tracing::warn!("Failed to connect to IMAP for Sent folder append: {:?}", e);
                    }
                }
            }
        }

        let event_type = if is_forward {
            EventType::MailForwardSent
        } else if in_reply_to_msg_id.is_some() {
            EventType::MailReplySent
        } else {
            EventType::MailMessageSent
        };

        let event = Event::new(
            event_type,
            mail_message.id,
            AggregateType::MailMessage,
            serde_json::json!({
                "mail_message_id": mail_message.id,
                "sent_by": owner_id,
                "subject": subject,
                "to_count": to.len(),
                "cc_count": cc.len(),
                "bcc_count": bcc.len(),
                "append_failed": append_failed,
            }),
            owner_id,
        );
        let _ = self.event_store.append(&event, &self.broadcaster).await;

        Ok(mail_message)
    }
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

    #[allow(clippy::too_many_arguments)]
    pub async fn save_draft(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        draft_id: Option<Uuid>,
        to: Vec<String>,
        cc: Vec<String>,
        bcc: Vec<String>,
        subject: String,
        body: String,
        attachment_ids: Vec<Uuid>,
        in_reply_to_msg_id: Option<Uuid>,
    ) -> Result<MailMessage, MailError> {
        let account = self.get_account(tenant_id, owner_id, account_id).await?;

        for file_id in &attachment_ids {
            let file = self
                .file_service
                .get_file(*file_id, owner_id)
                .await
                .map_err(|_| MailError::PermissionDenied)?;
            if file.tenant_id != tenant_id {
                return Err(MailError::PermissionDenied);
            }
        }

        let smtp = self
            .metadata_store
            .get_mail_smtp_settings(account_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?
            .unwrap_or_else(|| MailSmtpSettings {
                id: Uuid::new_v4(),
                tenant_id,
                owner_id,
                mail_account_id: account_id,
                host: "localhost".to_string(),
                port: 25,
                username: "draft".to_string(),
                password_enc: "".to_string(),
                tls_mode: "none".to_string(),
                from_address: account.username.clone(),
                from_name: None,
                reply_to: None,
                sent_folder: None,
                is_enabled: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });

        let email = OutboundMailMessage {
            recipients: &to,
            cc: &cc,
            bcc: &bcc,
            subject: &subject,
            body: &body,
            in_reply_to: None,
            references: None,
            attachments: Vec::new(),
        };

        let email_service = EmailService::new(
            self.metadata_store.pool().clone(),
            (*self.secret_key).clone(),
        );
        let raw_eml = email_service
            .build_raw_draft_eml(&smtp, email)
            .map_err(|e| MailError::InvalidSource(e.to_string()))?;

        let target_id = if let Some(did) = draft_id {
            if let Some(existing) = self
                .metadata_store
                .find_mail_message_by_id(did)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?
            {
                if existing.tenant_id != tenant_id
                    || existing.owner_id != owner_id
                    || existing.account_id != Some(account_id)
                    || existing.source_mode != "draft"
                {
                    return Err(MailError::PermissionDenied);
                }
                self.metadata_store
                    .delete_mail_message(did, owner_id)
                    .await
                    .map_err(|e| MailError::Database(e.to_string()))?;
            }
            did
        } else {
            Uuid::new_v4()
        };

        let mut msg = self
            .import_raw_source(
                tenant_id,
                owner_id,
                owner_id,
                Some(account_id),
                MailSourceMode::Draft,
                None,
                None,
                None,
                raw_eml,
                None,
                None,
                Some(target_id),
            )
            .await?;
        msg.in_reply_to = in_reply_to_msg_id.map(|id| id.to_string());
        self.metadata_store
            .update_mail_message_in_reply_to(target_id, owner_id, msg.in_reply_to.as_deref())
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        for file_id in &attachment_ids {
            let file = self
                .file_service
                .get_file(*file_id, owner_id)
                .await
                .map_err(|_| MailError::PermissionDenied)?;
            let attachment = MailAttachment {
                id: Uuid::new_v4(),
                tenant_id,
                message_id: target_id,
                file_id: Some(*file_id),
                filename: file.name.clone(),
                mime_type: Some(file.mime_type.clone()),
                size_bytes: Some(file.size),
                part_index: None,
                content_disposition: None,
                blob_key: None,
                created_at: Utc::now(),
            };
            self.metadata_store
                .create_mail_attachment(&attachment)
                .await
                .map_err(|e| MailError::Database(e.to_string()))?;
        }
        msg.has_attachments = !attachment_ids.is_empty();

        let event = Event::new(
            EventType::MailMessageDraftCreated,
            target_id,
            AggregateType::MailAccount,
            serde_json::json!({
                "mail_message_id": target_id,
                "subject": subject,
            }),
            owner_id,
        );
        let _ = self.event_store.append(&event, &self.broadcaster).await;

        Ok(msg)
    }

    pub async fn discard_draft(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        draft_id: Uuid,
    ) -> Result<(), MailError> {
        let draft = self
            .get_draft(tenant_id, owner_id, account_id, draft_id)
            .await?;

        if let Some(folder_id) = draft.folder_id {
            self.folder_service
                .delete_folder(folder_id, owner_id)
                .await
                .map_err(|e| MailError::Storage(e.to_string()))?;
        }

        self.metadata_store
            .delete_mail_message(draft_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        let event = Event::new(
            EventType::MailMessageDraftDeleted,
            draft_id,
            AggregateType::MailAccount,
            serde_json::json!({
                "mail_message_id": draft_id,
            }),
            owner_id,
        );
        let _ = self.event_store.append(&event, &self.broadcaster).await;

        Ok(())
    }

    pub async fn send_draft(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        draft_id: Uuid,
    ) -> Result<MailMessage, MailError> {
        let draft = self
            .get_draft(tenant_id, owner_id, account_id, draft_id)
            .await?;

        let parts = self.list_parts(tenant_id, owner_id, draft_id).await?;
        let body_part = parts
            .iter()
            .find(|p| p.content_type.starts_with("text/plain"))
            .ok_or_else(|| {
                MailError::InvalidSource("Draft has no plain text body part".to_string())
            })?;
        let (_, body_bytes) = self
            .get_message_part(tenant_id, owner_id, draft_id, body_part.id)
            .await?;
        let body = String::from_utf8(body_bytes.to_vec())
            .map_err(|e| MailError::InvalidSource(format!("Invalid draft body encoding: {e}")))?;

        let attachments = self
            .metadata_store
            .list_mail_attachments_by_message_id(draft_id, tenant_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;
        let attachment_ids = draft_attachment_file_ids(attachments)?;

        let to = mail_address_strings(&draft.to_addresses);
        let cc = mail_address_strings(&draft.cc_addresses);
        let bcc = mail_address_strings(&draft.bcc_addresses);
        let subject = draft.subject.unwrap_or_default();

        let sent_msg = self
            .send_outbound_mail(
                tenant_id,
                owner_id,
                account_id,
                to,
                cc,
                bcc,
                subject,
                body,
                attachment_ids,
                draft
                    .in_reply_to
                    .and_then(|id_str| Uuid::parse_str(&id_str).ok()),
                false,
            )
            .await?;

        self.metadata_store
            .delete_mail_message(draft_id, owner_id)
            .await
            .map_err(|e| MailError::Database(e.to_string()))?;

        Ok(sent_msg)
    }
}

fn imap_to_mail_error(err: ImapError) -> MailError {
    MailError::Imap(err.to_string())
}

async fn run_imap_mailbox_mutation<S: ImapMailboxSession + ?Sized>(
    session: &mut S,
    folder: &str,
    expected_uidvalidity: Option<i64>,
    uid: u32,
    mutation: MailboxMutation<'_>,
) -> Result<(), MailError> {
    let actual_uidvalidity = session
        .select_folder(folder)
        .await
        .map_err(imap_to_mail_error)?;

    if let Some(expected_uidvalidity) = expected_uidvalidity {
        let expected_uidvalidity = u32::try_from(expected_uidvalidity).map_err(|_| {
            MailError::InvalidSource(format!(
                "Folder UIDVALIDITY is out of range for {folder}: {expected_uidvalidity}"
            ))
        })?;
        if actual_uidvalidity != Some(expected_uidvalidity) {
            return Err(MailError::InvalidSource(format!(
                "Folder UIDVALIDITY changed for {folder}: expected {expected_uidvalidity}, got {:?}",
                actual_uidvalidity
            )));
        }
    }

    match mutation {
        MailboxMutation::MarkSeen(seen) => session
            .mark_seen(folder, uid, seen)
            .await
            .map_err(imap_to_mail_error),
        MailboxMutation::Move(destination_folder) => {
            if !session.supports_move().await.map_err(imap_to_mail_error)? {
                return Err(MailError::InvalidSource(
                    "Server does not support MOVE; refusing unsafe mailbox move".to_string(),
                ));
            }
            session
                .move_message(folder, uid, destination_folder)
                .await
                .map_err(imap_to_mail_error)
        }
        MailboxMutation::Delete => {
            if !session
                .supports_uidplus()
                .await
                .map_err(imap_to_mail_error)?
            {
                return Err(MailError::InvalidSource(
                    "Server does not support UIDPLUS; refusing unsafe mailbox-wide EXPUNGE"
                        .to_string(),
                ));
            }
            session
                .delete_message(folder, uid)
                .await
                .map_err(imap_to_mail_error)
        }
    }
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

fn mail_address_strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.as_str()
                .or_else(|| item.get("address").and_then(serde_json::Value::as_str))
        })
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect()
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

    #[derive(Default)]
    struct MockMailboxSession {
        uidvalidity: Option<u32>,
        supports_move: bool,
        supports_uidplus: bool,
        calls: Vec<String>,
    }

    #[async_trait::async_trait]
    impl ImapMailboxSession for MockMailboxSession {
        async fn select_folder(&mut self, folder: &str) -> Result<Option<u32>, ImapError> {
            self.calls.push(format!("select:{folder}"));
            Ok(self.uidvalidity)
        }

        async fn mark_seen(&mut self, folder: &str, uid: u32, seen: bool) -> Result<(), ImapError> {
            self.calls.push(format!("mark_seen:{folder}:{uid}:{seen}"));
            Ok(())
        }

        async fn supports_move(&mut self) -> Result<bool, ImapError> {
            self.calls.push("supports_move".to_string());
            Ok(self.supports_move)
        }

        async fn move_message(
            &mut self,
            folder: &str,
            uid: u32,
            destination_folder: &str,
        ) -> Result<(), ImapError> {
            self.calls
                .push(format!("move:{folder}:{uid}:{destination_folder}"));
            Ok(())
        }

        async fn supports_uidplus(&mut self) -> Result<bool, ImapError> {
            self.calls.push("supports_uidplus".to_string());
            Ok(self.supports_uidplus)
        }

        async fn delete_message(&mut self, folder: &str, uid: u32) -> Result<(), ImapError> {
            self.calls.push(format!("delete:{folder}:{uid}"));
            Ok(())
        }
    }

    #[tokio::test]
    async fn mailbox_mutation_rejects_uidvalidity_mismatch_before_mutating() {
        let mut session = MockMailboxSession {
            uidvalidity: Some(9),
            supports_move: true,
            supports_uidplus: true,
            calls: Vec::new(),
        };

        let err = run_imap_mailbox_mutation(
            &mut session,
            "Inbox",
            Some(8),
            42,
            MailboxMutation::MarkSeen(true),
        )
        .await
        .expect_err("stale UIDVALIDITY should fail");

        assert!(err.to_string().contains("UIDVALIDITY changed"));
        assert_eq!(session.calls, vec!["select:Inbox"]);
    }

    #[tokio::test]
    async fn mailbox_mutation_marks_read_and_unread() {
        let mut session = MockMailboxSession {
            uidvalidity: Some(9),
            supports_move: true,
            supports_uidplus: true,
            calls: Vec::new(),
        };

        run_imap_mailbox_mutation(
            &mut session,
            "Inbox",
            Some(9),
            42,
            MailboxMutation::MarkSeen(true),
        )
        .await
        .expect("mark read should succeed");
        run_imap_mailbox_mutation(
            &mut session,
            "Inbox",
            Some(9),
            42,
            MailboxMutation::MarkSeen(false),
        )
        .await
        .expect("mark unread should succeed");

        assert_eq!(
            session.calls,
            vec![
                "select:Inbox",
                "mark_seen:Inbox:42:true",
                "select:Inbox",
                "mark_seen:Inbox:42:false",
            ]
        );
    }

    #[tokio::test]
    async fn mailbox_mutation_rejects_move_without_move_capability() {
        let mut session = MockMailboxSession {
            uidvalidity: Some(9),
            supports_move: false,
            supports_uidplus: true,
            calls: Vec::new(),
        };

        let err = run_imap_mailbox_mutation(
            &mut session,
            "Inbox",
            Some(9),
            42,
            MailboxMutation::Move("Archive"),
        )
        .await
        .expect_err("MOVE-less server should fail");

        assert!(err.to_string().contains("does not support MOVE"));
        assert_eq!(session.calls, vec!["select:Inbox", "supports_move"]);
    }

    #[tokio::test]
    async fn mailbox_mutation_moves_to_archive_or_trash_destination() {
        let mut session = MockMailboxSession {
            uidvalidity: Some(9),
            supports_move: true,
            supports_uidplus: true,
            calls: Vec::new(),
        };

        run_imap_mailbox_mutation(
            &mut session,
            "Inbox",
            Some(9),
            42,
            MailboxMutation::Move("Archive"),
        )
        .await
        .expect("archive move should succeed");
        run_imap_mailbox_mutation(
            &mut session,
            "Inbox",
            Some(9),
            43,
            MailboxMutation::Move("Trash"),
        )
        .await
        .expect("trash move should succeed");

        assert_eq!(
            session.calls,
            vec![
                "select:Inbox",
                "supports_move",
                "move:Inbox:42:Archive",
                "select:Inbox",
                "supports_move",
                "move:Inbox:43:Trash",
            ]
        );
    }

    #[tokio::test]
    async fn mailbox_mutation_rejects_delete_without_uidplus() {
        let mut session = MockMailboxSession {
            uidvalidity: Some(9),
            supports_move: true,
            supports_uidplus: false,
            calls: Vec::new(),
        };

        let err =
            run_imap_mailbox_mutation(&mut session, "Trash", Some(9), 42, MailboxMutation::Delete)
                .await
                .expect_err("UIDPLUS-less delete should fail");

        assert!(err.to_string().contains("does not support UIDPLUS"));
        assert_eq!(session.calls, vec!["select:Trash", "supports_uidplus"]);
    }

    #[tokio::test]
    async fn mailbox_mutation_deletes_with_uidplus() {
        let mut session = MockMailboxSession {
            uidvalidity: Some(9),
            supports_move: true,
            supports_uidplus: true,
            calls: Vec::new(),
        };

        run_imap_mailbox_mutation(&mut session, "Trash", Some(9), 42, MailboxMutation::Delete)
            .await
            .expect("UIDPLUS delete should succeed");

        assert_eq!(
            session.calls,
            vec!["select:Trash", "supports_uidplus", "delete:Trash:42"]
        );
    }

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

    #[test]
    fn outbound_mail_validation_accepts_normal_message() {
        let to = vec!["to@example.com".to_string()];
        let cc = Vec::new();
        let bcc = vec!["blind@example.com".to_string()];

        validate_outbound_mail(&to, &cc, &bcc, "Subject", "Body", 0)
            .expect("normal outbound mail should validate");
    }

    #[test]
    fn outbound_mail_validation_rejects_invalid_draft_send() {
        let empty = Vec::new();
        let err = validate_outbound_mail(&empty, &empty, &empty, "", "", 0)
            .expect_err("draft send without recipients should fail before SMTP");

        assert!(err.to_string().contains("At least one recipient"));
    }

    #[test]
    fn draft_attachment_file_ids_preserves_backing_files() {
        let file_id = Uuid::new_v4();
        let attachments = vec![MailAttachment {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            message_id: Uuid::new_v4(),
            file_id: Some(file_id),
            filename: "report.pdf".to_string(),
            mime_type: Some("application/pdf".to_string()),
            size_bytes: Some(10),
            part_index: None,
            content_disposition: None,
            blob_key: None,
            created_at: Utc::now(),
        }];

        assert_eq!(
            draft_attachment_file_ids(attachments).unwrap(),
            vec![file_id]
        );
    }

    #[test]
    fn draft_attachment_file_ids_rejects_missing_backing_file() {
        let attachments = vec![MailAttachment {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            message_id: Uuid::new_v4(),
            file_id: None,
            filename: "lost.pdf".to_string(),
            mime_type: Some("application/pdf".to_string()),
            size_bytes: Some(10),
            part_index: None,
            content_disposition: None,
            blob_key: None,
            created_at: Utc::now(),
        }];

        let err = draft_attachment_file_ids(attachments)
            .expect_err("missing draft attachment file id should fail before SMTP");

        assert!(err.to_string().contains("lost.pdf"));
    }

    #[test]
    fn imported_mail_list_excludes_drafts() {
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let draft = MailMessage::new(tenant_id, owner_id, owner_id, MailSourceMode::Draft);
        let imported = MailMessage::new(tenant_id, owner_id, owner_id, MailSourceMode::EmlUpload);

        assert!(!visible_in_imported_mail_list(&draft));
        assert!(visible_in_imported_mail_list(&imported));
    }
}
