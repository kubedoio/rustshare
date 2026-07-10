//! Metadata store for querying projection tables.
//!
//! NOTE: File SELECT queries use compile-time checked `sqlx::query_as!()` macros.
//! Other queries continue to use `sqlx::query!()` where type safety is enforced inline.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rustshare_core::domain::{
    File, FileVersion, Folder, MailAccount, MailAccountId, MailAttachment, MailImportJob,
    MailImportJobId, MailLink, MailLinkId, MailMessage, MailMessageId, MailMessagePart,
    OidcLoginState, ReplicationJob, ReplicationJobStatus, ReplicationState, ReplicationTarget,
    Share, SharePermissions, User, UserId, UserSession, Vault, VaultDevice, VaultFile,
    VaultWritePolicy,
};
use rustshare_core::services::VaultSyncError;
use serde_json;
use sqlx::PgPool;
#[cfg(test)]
use sqlx::Row;
use std::time::Duration;
use uuid::Uuid;

/// Business-level errors for vault file operations.
#[derive(Debug, thiserror::Error)]
pub(crate) enum VaultFileStoreError {
    #[error("file not found")]
    NotFound,
    #[error("destination exists")]
    DestinationExists,
}

/// Metadata store for querying projection tables
#[derive(Clone)]
pub struct MetadataStore {
    pool: PgPool,
}

impl MetadataStore {
    /// Get access to the underlying database pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[derive(Debug, Clone)]
pub struct OwnedPublicShare {
    pub share: Share,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub resource_name: String,
}

/// Folder with share information
#[derive(Debug, Clone)]
pub struct FolderWithShares {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub parent_folder_id: Option<Uuid>,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tenant_id: Uuid,
    pub starred_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub ancestor_ids: Option<Vec<Uuid>>,
    pub is_shared: bool,
    pub share_count: i64,
    pub share_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct PublicShareAccessLogEntry {
    pub accessed_at: DateTime<Utc>,
    pub action: String,
    pub success: bool,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub actor_type: Option<String>,
    pub actor_label: Option<String>,
    pub share_session_id: Option<Uuid>,
    pub share_session_subject: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReplicationAttemptRecord<'a> {
    pub job_id: Uuid,
    pub target_id: Uuid,
    pub attempt_number: i32,
    pub status: &'a str,
    pub error_message: Option<&'a str>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ShareAccessLogEntry {
    pub share_id: Uuid,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub action: String,
    pub success: bool,
    pub actor_type: Option<String>,
    pub actor_label: Option<String>,
    pub share_session_id: Option<Uuid>,
    pub share_session_subject: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UserSecurityEventRecord<'a> {
    pub user_id: Uuid,
    pub event_type: &'a str,
    pub description: &'a str,
    pub ip_address: Option<&'a str>,
    pub user_agent: Option<&'a str>,
    pub session_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct UserSecurityEvent {
    pub id: Uuid,
    pub event_type: String,
    pub description: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub session_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SecurityConfig {
    pub login_protection_enabled: bool,
    pub max_login_attempts: i32,
    pub login_block_duration_minutes: i32,
    pub updated_at: DateTime<Utc>,
}

impl MetadataStore {
    fn permission_to_db_value(permission: SharePermissions) -> &'static str {
        match permission {
            SharePermissions::View => "View",
            SharePermissions::Edit => "Edit",
            SharePermissions::Admin => "Admin",
        }
    }

    fn permission_from_db_value(value: &str) -> SharePermissions {
        match value {
            "Edit" | "edit" => SharePermissions::Edit,
            "Admin" | "admin" => SharePermissions::Admin,
            _ => SharePermissions::View,
        }
    }

    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_mail_message(&self, msg: &MailMessage) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO mail_messages (
                id, tenant_id, owner_id, account_id, source_mode, source_folder, source_uid, source_uidvalidity,
                message_id, in_reply_to, reference_ids, subject, from_address, from_name,
                to_addresses, cc_addresses, bcc_addresses, sent_at, imported_at, imported_by,
                visibility, folder_id, object_key, blob_key, blob_sha256, size_bytes, has_attachments,
                deleted_at, created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8,
                $9, $10, $11, $12, $13, $14,
                $15, $16, $17, $18, $19, $20,
                $21, $22, $23, $24, $25, $26, $27,
                $28, $29, $30
            )
            "#,
            msg.id,
            msg.tenant_id,
            msg.owner_id,
            msg.account_id,
            msg.source_mode,
            msg.source_folder,
            msg.source_uid,
            msg.source_uidvalidity,
            msg.message_id,
            msg.in_reply_to,
            msg.references.as_deref(),
            msg.subject,
            msg.from_address,
            msg.from_name,
            msg.to_addresses,
            msg.cc_addresses,
            msg.bcc_addresses,
            msg.sent_at,
            msg.imported_at,
            msg.imported_by,
            msg.visibility,
            msg.folder_id,
            msg.object_key,
            msg.blob_key,
            msg.blob_sha256,
            msg.size_bytes,
            msg.has_attachments,
            msg.deleted_at,
            msg.created_at,
            msg.updated_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Insert a mail message if one does not already exist for the same source.
    ///
    /// Returns `true` if a new row was inserted, `false` if the unique source
    /// key already existed.
    pub async fn create_mail_message_if_not_exists(&self, msg: &MailMessage) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            INSERT INTO mail_messages (
                id, tenant_id, owner_id, account_id, source_mode, source_folder, source_uid, source_uidvalidity,
                message_id, in_reply_to, reference_ids, subject, from_address, from_name,
                to_addresses, cc_addresses, bcc_addresses, sent_at, imported_at, imported_by,
                visibility, folder_id, object_key, blob_key, blob_sha256, size_bytes, has_attachments,
                deleted_at, created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8,
                $9, $10, $11, $12, $13, $14,
                $15, $16, $17, $18, $19, $20,
                $21, $22, $23, $24, $25, $26, $27,
                $28, $29, $30
            )
            ON CONFLICT (owner_id, account_id, source_mode, source_folder, source_uid, source_uidvalidity)
            WHERE deleted_at IS NULL AND source_mode IN ('imap_selected', 'imap_archive')
            DO NOTHING
            "#,
            msg.id,
            msg.tenant_id,
            msg.owner_id,
            msg.account_id,
            msg.source_mode,
            msg.source_folder,
            msg.source_uid,
            msg.source_uidvalidity,
            msg.message_id,
            msg.in_reply_to,
            msg.references.as_deref(),
            msg.subject,
            msg.from_address,
            msg.from_name,
            msg.to_addresses,
            msg.cc_addresses,
            msg.bcc_addresses,
            msg.sent_at,
            msg.imported_at,
            msg.imported_by,
            msg.visibility,
            msg.folder_id,
            msg.object_key,
            msg.blob_key,
            msg.blob_sha256,
            msg.size_bytes,
            msg.has_attachments,
            msg.deleted_at,
            msg.created_at,
            msg.updated_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Update the folder_id of an existing mail message.
    pub async fn update_mail_message_folder_id(
        &self,
        id: uuid::Uuid,
        folder_id: uuid::Uuid,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE mail_messages
            SET folder_id = $2, updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
            id,
            folder_id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Hard-delete a mail message row. Intended for cleaning up a partially
    /// imported message when artifact creation fails after the row was inserted
    /// for deduplication.
    pub async fn delete_mail_message(&self, id: uuid::Uuid, owner_id: Uuid) -> Result<u64> {
        let result = sqlx::query!(
            r#"
            DELETE FROM mail_messages
            WHERE id = $1 AND owner_id = $2
            "#,
            id,
            owner_id
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn create_mail_message_part(&self, part: &MailMessagePart) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO mail_message_parts (
                id, tenant_id, message_id, part_index, content_type, charset,
                blob_key, blob_sha256, size_bytes, is_body, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            part.id,
            part.tenant_id,
            part.message_id,
            part.part_index,
            part.content_type,
            part.charset,
            part.blob_key,
            part.blob_sha256,
            part.size_bytes,
            part.is_body,
            part.created_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_mail_attachment(&self, attachment: &MailAttachment) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO mail_attachments (
                id, tenant_id, message_id, file_id, filename, mime_type,
                size_bytes, part_index, content_disposition, blob_key, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            attachment.id,
            attachment.tenant_id,
            attachment.message_id,
            attachment.file_id,
            attachment.filename,
            attachment.mime_type,
            attachment.size_bytes,
            attachment.part_index,
            attachment.content_disposition,
            attachment.blob_key,
            attachment.created_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// List all attachments for a mail message, scoped to the owning user and tenant.
    pub async fn list_mail_attachments_by_message_id(
        &self,
        message_id: Uuid,
        tenant_id: Uuid,
        owner_id: Uuid,
    ) -> Result<Vec<MailAttachment>> {
        let rows = sqlx::query_as!(
            MailAttachment,
            r#"
            SELECT
                a.id, a.tenant_id, a.message_id, a.file_id, a.filename, a.mime_type,
                a.size_bytes, a.part_index, a.content_disposition, a.blob_key, a.created_at
            FROM mail_attachments a
            JOIN mail_messages m ON m.id = a.message_id
            WHERE a.message_id = $1 AND m.tenant_id = $2 AND m.owner_id = $3
            ORDER BY a.filename
            "#,
            message_id,
            tenant_id,
            owner_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn find_mail_message_by_id(
        &self,
        id: Uuid,
        owner_id: Uuid,
    ) -> Result<Option<MailMessage>> {
        let row = sqlx::query_as!(
            MailMessage,
            r#"
            SELECT
                id, tenant_id, owner_id, account_id, source_mode, source_folder, source_uid, source_uidvalidity,
                message_id, in_reply_to, reference_ids AS references, subject, from_address, from_name,
                to_addresses, cc_addresses, bcc_addresses, sent_at, imported_at, imported_by,
                visibility, folder_id, object_key, blob_key, blob_sha256, size_bytes, has_attachments,
                deleted_at, created_at, updated_at
            FROM mail_messages
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            id,
            owner_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Find an existing mail message imported from the same IMAP account/folder/UIDVALIDITY.
    pub async fn find_mail_message_by_source(
        &self,
        owner_id: UserId,
        account_id: MailAccountId,
        source_mode: &str,
        source_folder: &str,
        source_uid: i64,
        source_uidvalidity: Option<i64>,
    ) -> Result<Option<MailMessage>> {
        let row = sqlx::query_as!(
            MailMessage,
            r#"
            SELECT
                id, tenant_id, owner_id, account_id, source_mode, source_folder, source_uid, source_uidvalidity,
                message_id, in_reply_to, reference_ids AS references, subject, from_address, from_name,
                to_addresses, cc_addresses, bcc_addresses, sent_at, imported_at, imported_by,
                visibility, folder_id, object_key, blob_key, blob_sha256, size_bytes, has_attachments,
                deleted_at, created_at, updated_at
            FROM mail_messages
            WHERE owner_id = $1
              AND account_id = $2
              AND source_mode = $3
              AND source_folder = $4
              AND source_uid = $5
              AND source_uidvalidity IS NOT DISTINCT FROM $6
              AND deleted_at IS NULL
            "#,
            owner_id,
            account_id,
            source_mode,
            source_folder,
            source_uid,
            source_uidvalidity
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Returns true if another running import job (not `exclude_job_id`) covers
    /// the same owner/account/folder/UID. Used to avoid deleting a partial
    /// `mail_messages` row that belongs to an active concurrent import.
    pub async fn has_other_running_import_job_for_uid(
        &self,
        owner_id: UserId,
        account_id: MailAccountId,
        folder_name: &str,
        uid: i64,
        exclude_job_id: MailImportJobId,
    ) -> Result<bool> {
        let exists = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM mail_import_jobs
                WHERE owner_id = $1
                  AND account_id = $2
                  AND folder_name = $3
                  AND $4 = ANY(selected_uids)
                  AND status = 'running'
                  AND id != $5
                  AND deleted_at IS NULL
            ) AS "exists!"
            "#,
            owner_id,
            account_id,
            folder_name,
            uid,
            exclude_job_id
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    /// Persist a new Mail link inside an existing transaction.
    ///
    /// Returns `true` if a new row was inserted, or `false` if an active link
    /// for the same message/target already exists (unique conflict).
    pub async fn create_mail_link_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        link: &MailLink,
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            INSERT INTO mail_links (
                id, tenant_id, message_id, target_type, target_id, created_by, created_at, deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (message_id, target_type, target_id) WHERE deleted_at IS NULL
            DO NOTHING
            "#,
            link.id,
            link.tenant_id,
            link.message_id,
            link.target_type,
            link.target_id,
            link.created_by,
            link.created_at,
            link.deleted_at,
        )
        .execute(&mut **tx)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Persist a new Mail link.
    ///
    /// Returns `true` if a new row was inserted, or `false` if an active link
    /// for the same message/target already exists.
    pub async fn create_mail_link(&self, link: &MailLink) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let inserted = self.create_mail_link_in_tx(&mut tx, link).await?;
        tx.commit().await?;
        Ok(inserted)
    }

    /// Soft-delete a Mail link inside an existing transaction and return true if a row was updated.
    pub async fn soft_delete_mail_link_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        link_id: MailLinkId,
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE mail_links
            SET deleted_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
            link_id
        )
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Soft-delete a Mail link and return true if a row was updated.
    pub async fn soft_delete_mail_link(&self, link_id: MailLinkId) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let updated = self.soft_delete_mail_link_in_tx(&mut tx, link_id).await?;
        tx.commit().await?;
        Ok(updated)
    }

    /// List active links for a Mail message, scoped to tenant.
    pub async fn list_mail_links_by_message(
        &self,
        message_id: MailMessageId,
        tenant_id: Uuid,
    ) -> Result<Vec<MailLink>> {
        let rows = sqlx::query_as!(
            MailLink,
            r#"
            SELECT
                id, tenant_id, message_id, target_type, target_id, created_by, created_at, deleted_at
            FROM mail_links
            WHERE message_id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            ORDER BY created_at
            "#,
            message_id,
            tenant_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list_mail_messages(
        &self,
        tenant_id: Uuid,
        owner_id: Uuid,
    ) -> Result<Vec<MailMessage>> {
        let rows = sqlx::query_as!(
            MailMessage,
            r#"
            SELECT
                id, tenant_id, owner_id, account_id, source_mode, source_folder, source_uid, source_uidvalidity,
                message_id, in_reply_to, reference_ids AS references, subject, from_address, from_name,
                to_addresses, cc_addresses, bcc_addresses, sent_at, imported_at, imported_by,
                visibility, folder_id, object_key, blob_key, blob_sha256, size_bytes, has_attachments,
                deleted_at, created_at, updated_at
            FROM mail_messages
            WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL
            ORDER BY imported_at DESC, created_at DESC
            "#,
            tenant_id,
            owner_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Find a Mail link by ID, scoped to tenant.
    pub async fn find_mail_link_by_id(
        &self,
        link_id: MailLinkId,
        tenant_id: Uuid,
    ) -> Result<Option<MailLink>> {
        let row = sqlx::query_as!(
            MailLink,
            r#"
            SELECT
                id, tenant_id, message_id, target_type, target_id, created_by, created_at, deleted_at
            FROM mail_links
            WHERE id = $1 AND tenant_id = $2
            "#,
            link_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Find an active Mail link for a given message and target, scoped to tenant.
    pub async fn find_active_mail_link(
        &self,
        message_id: MailMessageId,
        target_type: impl Into<String>,
        target_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<MailLink>> {
        let target_type = target_type.into();
        let row = sqlx::query_as!(
            MailLink,
            r#"
            SELECT
                id, tenant_id, message_id, target_type, target_id, created_by, created_at, deleted_at
            FROM mail_links
            WHERE message_id = $1 AND target_type = $2 AND target_id = $3
              AND tenant_id = $4 AND deleted_at IS NULL
            "#,
            message_id,
            target_type,
            target_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Create a new mail account inside an existing transaction.
    pub async fn create_mail_account_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        account: &MailAccount,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO mail_accounts (
                id, tenant_id, owner_id, name, host, port, username, password_enc,
                tls_mode, is_enabled, last_error, last_connected_at, deleted_at,
                created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            "#,
            account.id,
            account.tenant_id,
            account.owner_id,
            account.name,
            account.host,
            account.port,
            account.username,
            account.password_enc,
            account.tls_mode,
            account.is_enabled,
            account.last_error.as_deref(),
            account.last_connected_at,
            account.deleted_at,
            account.created_at,
            account.updated_at,
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Create a new mail account.
    pub async fn create_mail_account(&self, account: &MailAccount) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.create_mail_account_in_tx(&mut tx, account).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Find a mail account by ID, scoped to the owning user.
    pub async fn get_mail_account(
        &self,
        id: MailAccountId,
        owner_id: UserId,
    ) -> Result<Option<MailAccount>> {
        let row = sqlx::query_as!(
            MailAccount,
            r#"
            SELECT
                id, tenant_id, owner_id, name, host, port, username, password_enc,
                tls_mode, is_enabled, last_error, last_connected_at, deleted_at,
                created_at, updated_at
            FROM mail_accounts
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            id,
            owner_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// List active mail accounts for a user, ordered by creation time (newest first).
    pub async fn list_mail_accounts_by_owner(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
    ) -> Result<Vec<MailAccount>> {
        let rows = sqlx::query_as!(
            MailAccount,
            r#"
            SELECT
                id, tenant_id, owner_id, name, host, port, username, password_enc,
                tls_mode, is_enabled, last_error, last_connected_at, deleted_at,
                created_at, updated_at
            FROM mail_accounts
            WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL
            ORDER BY created_at DESC
            "#,
            tenant_id,
            owner_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Update a mail account's connection details, enabled state, and
    /// connection status fields.
    pub async fn update_mail_account(&self, account: &MailAccount) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE mail_accounts
            SET
                name = $3,
                host = $4,
                port = $5,
                username = $6,
                password_enc = $7,
                tls_mode = $8,
                is_enabled = $9,
                last_error = $10,
                last_connected_at = $11,
                updated_at = NOW()
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            account.id,
            account.owner_id,
            account.name,
            account.host,
            account.port,
            account.username,
            account.password_enc,
            account.tls_mode,
            account.is_enabled,
            account.last_error,
            account.last_connected_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Soft-delete a mail account inside an existing transaction and return
    /// whether a row was updated.
    ///
    /// Any pending or running import jobs belonging to the account are
    /// cancelled as part of the same transaction.
    pub async fn soft_delete_mail_account_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        id: MailAccountId,
        owner_id: UserId,
    ) -> Result<bool> {
        sqlx::query!(
            r#"
            UPDATE mail_import_jobs
            SET status = 'cancelled', updated_at = NOW()
            WHERE account_id = $1
              AND owner_id = $2
              AND status IN ('pending', 'running')
              AND deleted_at IS NULL
            "#,
            id,
            owner_id
        )
        .execute(&mut **tx)
        .await?;

        let result = sqlx::query!(
            r#"
            UPDATE mail_accounts
            SET deleted_at = NOW()
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            id,
            owner_id
        )
        .execute(&mut **tx)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Soft-delete a mail account and return whether a row was updated.
    ///
    /// Any pending or running import jobs belonging to the account are
    /// cancelled as part of the same transaction.
    pub async fn soft_delete_mail_account(
        &self,
        id: MailAccountId,
        owner_id: UserId,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let updated = self
            .soft_delete_mail_account_in_tx(&mut tx, id, owner_id)
            .await?;
        tx.commit().await?;
        Ok(updated)
    }

    /// Create a new mail import job.
    pub async fn create_mail_import_job(&self, job: &MailImportJob) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO mail_import_jobs (
                id, tenant_id, owner_id, account_id, source_mode, folder_name,
                selected_uids, source_uidvalidity, archive_since, archive_before,
                last_uid_validity, last_imported_uid, retention_days, retry_count, max_retries,
                status, total_messages, processed_messages, failed_messages, last_error,
                started_at, completed_at, deleted_at, created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                $16, $17, $18, $19, $20, $21, $22, $23, $24, $25
            )
            "#,
            job.id,
            job.tenant_id,
            job.owner_id,
            job.account_id,
            job.source_mode,
            job.folder_name,
            job.selected_uids.as_deref(),
            job.source_uidvalidity,
            job.archive_since,
            job.archive_before,
            job.last_uid_validity,
            job.last_imported_uid,
            job.retention_days,
            job.retry_count,
            job.max_retries,
            job.status,
            job.total_messages,
            job.processed_messages,
            job.failed_messages,
            job.last_error.as_deref(),
            job.started_at,
            job.completed_at,
            job.deleted_at,
            job.created_at,
            job.updated_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Find a mail import job by ID, scoped to the owning user.
    pub async fn get_mail_import_job(
        &self,
        id: MailImportJobId,
        owner_id: UserId,
    ) -> Result<Option<MailImportJob>> {
        let row = sqlx::query_as!(
            MailImportJob,
            r#"
            SELECT
                id, tenant_id, owner_id, account_id, source_mode, folder_name,
                selected_uids AS "selected_uids: _",
                source_uidvalidity,
                archive_since, archive_before, last_uid_validity, last_imported_uid,
                retention_days, retry_count, max_retries,
                status, total_messages, processed_messages, failed_messages,
                last_error, started_at, completed_at, deleted_at, created_at, updated_at
            FROM mail_import_jobs
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            id,
            owner_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Return the current status string of a mail import job, if it exists.
    pub async fn get_mail_import_job_status(
        &self,
        id: MailImportJobId,
        owner_id: UserId,
    ) -> Result<Option<String>> {
        let row = sqlx::query_scalar!(
            r#"
            SELECT status
            FROM mail_import_jobs
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            id,
            owner_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// List active mail import jobs for a user, optionally filtered by account.
    pub async fn list_mail_import_jobs_by_owner(
        &self,
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: Option<MailAccountId>,
    ) -> Result<Vec<MailImportJob>> {
        if let Some(account_id) = account_id {
            let rows = sqlx::query_as!(
                MailImportJob,
                r#"
                SELECT
                    id, tenant_id, owner_id, account_id, source_mode, folder_name,
                    selected_uids AS "selected_uids: _",
                    source_uidvalidity,
                    archive_since, archive_before, last_uid_validity, last_imported_uid,
                    retention_days, retry_count, max_retries,
                    status, total_messages, processed_messages, failed_messages,
                    last_error, started_at, completed_at, deleted_at, created_at, updated_at
                FROM mail_import_jobs
                WHERE tenant_id = $1 AND owner_id = $2 AND account_id = $3 AND deleted_at IS NULL
                ORDER BY created_at DESC
                "#,
                tenant_id,
                owner_id,
                account_id
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        } else {
            let rows = sqlx::query_as!(
                MailImportJob,
                r#"
                SELECT
                    id, tenant_id, owner_id, account_id, source_mode, folder_name,
                    selected_uids AS "selected_uids: _",
                    source_uidvalidity,
                    archive_since, archive_before, last_uid_validity, last_imported_uid,
                    retention_days, retry_count, max_retries,
                    status, total_messages, processed_messages, failed_messages,
                    last_error, started_at, completed_at, deleted_at, created_at, updated_at
                FROM mail_import_jobs
                WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL
                ORDER BY created_at DESC
                "#,
                tenant_id,
                owner_id
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        }
    }

    /// Atomically claim the oldest pending mail import job for processing.
    ///
    /// Only claims jobs whose associated mail account is enabled, whose tenant
    /// has the mail module enabled, and whose job has not been soft-deleted.
    /// The SELECT, UPDATE, and RETURN are performed in a single CTE statement
    /// so the row transitions to `running` before the lock is released.
    pub async fn claim_next_pending_mail_import_job(&self) -> Result<Option<MailImportJob>> {
        let job = sqlx::query_as!(
            MailImportJob,
            r#"
            WITH target AS (
                SELECT j.id
                FROM mail_import_jobs j
                JOIN mail_accounts a ON a.id = j.account_id
                JOIN modules m ON m.tenant_id = j.tenant_id
                WHERE j.status = 'pending'
                  AND j.deleted_at IS NULL
                  AND a.deleted_at IS NULL
                  AND a.is_enabled = true
                  AND m.module_key = 'mail'
                  AND m.enabled = true
                  AND (
                      j.source_mode != 'imap_archive'
                      OR (
                          j.retry_count < j.max_retries
                          AND j.updated_at <= NOW() - (interval '1 second' * (2 ^ GREATEST(j.retry_count, 0)))
                      )
                  )
                ORDER BY j.created_at ASC
                FOR UPDATE OF j SKIP LOCKED
                LIMIT 1
            ),
            updated AS (
                UPDATE mail_import_jobs
                SET status = 'running', started_at = NOW(), updated_at = NOW()
                FROM target
                WHERE mail_import_jobs.id = target.id
                RETURNING mail_import_jobs.*
            )
            SELECT
                id, tenant_id, owner_id, account_id, source_mode, folder_name,
                selected_uids AS "selected_uids: _",
                source_uidvalidity,
                archive_since, archive_before, last_uid_validity, last_imported_uid,
                retention_days, retry_count, max_retries,
                status, total_messages, processed_messages, failed_messages,
                last_error, started_at, completed_at, deleted_at, created_at, updated_at
            FROM updated
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(job)
    }

    /// Update the progress counters for a mail import job.
    pub async fn update_mail_import_job_progress(
        &self,
        id: MailImportJobId,
        processed: i32,
        failed: i32,
        last_error: Option<&str>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE mail_import_jobs
            SET
                processed_messages = $2,
                failed_messages = $3,
                last_error = $4,
                updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
            id,
            processed,
            failed,
            last_error
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update the progress counters and UID watermark for an IMAP archive job.
    pub async fn update_mail_archive_job_progress(
        &self,
        id: MailImportJobId,
        processed: i32,
        failed: i32,
        last_uid_validity: Option<i64>,
        last_imported_uid: Option<i64>,
        last_error: Option<&str>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE mail_import_jobs
            SET processed_messages = $1,
                failed_messages = $2,
                last_uid_validity = $3,
                last_imported_uid = $4,
                last_error = $5,
                updated_at = NOW()
            WHERE id = $6 AND deleted_at IS NULL
            "#,
            processed,
            failed,
            last_uid_validity,
            last_imported_uid,
            last_error,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Soft-delete a mail import job, but only if it is an `imap_archive` job.
    ///
    /// Returns `true` if a row was updated.
    pub async fn soft_delete_mail_archive_job(
        &self,
        id: MailImportJobId,
        owner_id: UserId,
    ) -> Result<bool> {
        let rows = sqlx::query!(
            r#"
            UPDATE mail_import_jobs
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
              AND source_mode = 'imap_archive'
            "#,
            id,
            owner_id
        )
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(rows > 0)
    }

    /// Soft-delete archived mail messages that exceed the configured retention period.
    ///
    /// Returns the number of rows soft-deleted.
    pub async fn apply_archive_retention(
        &self,
        owner_id: UserId,
        account_id: MailAccountId,
        folder_name: &str,
        retention_days: i32,
    ) -> Result<u64> {
        if retention_days <= 0 {
            return Err(anyhow::anyhow!("retention_days must be positive"));
        }
        let rows = sqlx::query!(
            r#"
            UPDATE mail_messages
            SET deleted_at = NOW(), updated_at = NOW()
            WHERE owner_id = $1
              AND account_id = $2
              AND source_folder = $3
              AND source_mode = 'imap_archive'
              AND imported_at < NOW() - (interval '1 day' * $4)
              AND deleted_at IS NULL
            "#,
            owner_id,
            account_id,
            folder_name,
            retention_days as f64
        )
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(rows)
    }

    /// Mark a pending or already-running mail import job as running and record
    /// its start time.
    ///
    /// Returns `true` if the job was in the `pending` or `running` state and
    /// updated, `false` if it was already in a terminal state (e.g. cancelled).
    pub async fn mark_mail_import_job_running(&self, id: MailImportJobId) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE mail_import_jobs
            SET status = 'running', started_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL AND status IN ('pending', 'running')
            "#,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Reset mail import jobs stuck in the `running` state back to `pending`.
    ///
    /// Uses `updated_at` as a heartbeat: [`update_mail_import_job_progress`]
    /// refreshes it after each UID, so a live worker is not reset.
    pub async fn reset_stale_running_mail_import_jobs(
        &self,
        stale_threshold: Duration,
        exclude_ids: &[MailImportJobId],
    ) -> Result<u64> {
        let seconds = stale_threshold.as_secs_f64();
        let result = sqlx::query!(
            r#"
            UPDATE mail_import_jobs
            SET status = 'pending', started_at = NULL, last_error = 'stale running job reset by worker', updated_at = NOW()
            WHERE status = 'running'
              AND deleted_at IS NULL
              AND updated_at < NOW() - interval '1 second' * $1
              AND id != ALL($2)
            "#,
            seconds,
            exclude_ids as &[MailImportJobId],
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Mark a running mail import job as completed.
    ///
    /// Returns `true` if the job was in the `running` state and updated,
    /// `false` if it was already in a terminal state (e.g. cancelled).
    pub async fn mark_mail_import_job_completed(&self, id: MailImportJobId) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE mail_import_jobs
            SET status = 'completed', completed_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL AND status = 'running'
            "#,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Mark a running mail import job as failed.
    ///
    /// Returns `true` if the job was in the `running` state and updated,
    /// `false` if it was already in a terminal state (e.g. cancelled).
    pub async fn mark_mail_import_job_failed(
        &self,
        id: MailImportJobId,
        error: &str,
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE mail_import_jobs
            SET status = 'failed',
                last_error = $2,
                completed_at = NOW(),
                updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL AND status = 'running'
            "#,
            id,
            error
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Mark a running IMAP archive job as failed, with retry/backoff semantics.
    ///
    /// If the job has not exhausted `max_retries`, it is returned to the
    /// `pending` state with `started_at` cleared so it can be claimed again
    /// after the backoff delay. Once retries are exhausted, the job moves to
    /// `failed` and `completed_at` is recorded.
    ///
    /// Returns `true` if the job was in the `running` state and updated.
    pub async fn mark_archive_job_failed_with_retry(
        &self,
        id: MailImportJobId,
        error: &str,
    ) -> Result<bool> {
        let rows = sqlx::query!(
            r#"
            UPDATE mail_import_jobs
            SET status = CASE
                    WHEN retry_count < max_retries THEN 'pending'
                    ELSE 'failed'
                 END,
                last_error = $2,
                retry_count = retry_count + 1,
                started_at = CASE
                    WHEN retry_count < max_retries THEN NULL
                    ELSE started_at
                 END,
                completed_at = CASE
                    WHEN retry_count < max_retries THEN NULL
                    ELSE NOW()
                 END,
                updated_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL AND source_mode = 'imap_archive' AND status = 'running'
            "#,
            id,
            error
        )
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(rows > 0)
    }

    fn parse_replication_state(value: &str) -> Result<ReplicationState> {
        value.parse().map_err(|error: String| {
            anyhow::anyhow!("invalid replication state `{value}`: {error}")
        })
    }

    fn parse_replication_job_status(value: &str) -> Result<ReplicationJobStatus> {
        value.parse().map_err(|error: String| {
            anyhow::anyhow!("invalid replication job status `{value}`: {error}")
        })
    }

    /// Create a new user in the projection table
    pub async fn create_user(&self, user: &User) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO users (id, username, email, password_hash, display_name, is_admin, storage_quota, theme, created_at, updated_at, name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            "#,
            user.id,
            user.username,
            user.email,
            user.password_hash,
            user.display_name,
            user.is_admin,
            user.storage_quota,
            user.theme.to_string(),
            user.created_at,
            user.updated_at,
            user.name.as_deref(),
            user.surname.as_deref(),
            user.avatar_path.as_deref(),
            user.email_sharing_enabled,
            user.trash_retention_days,
            user.tenant_id,
            serde_json::to_value(&*user.dashboard_config).unwrap(),
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Create a new opaque browser session.
    pub async fn create_user_session(&self, session: &UserSession) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO user_sessions (
                id,
                user_id,
                session_token_hash,
                expires_at,
                created_at,
                last_seen_at,
                user_agent,
                ip_address,
                tenant_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            session.id,
            session.user_id,
            session.session_token_hash,
            session.expires_at,
            session.created_at,
            session.last_seen_at,
            session.user_agent.as_deref(),
            session.ip_address.as_deref(),
            session.tenant_id,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find a browser session by hashed token.
    pub async fn find_user_session_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<UserSession>> {
        let row = sqlx::query!(
            r#"
            SELECT
                id,
                user_id,
                session_token_hash,
                expires_at,
                created_at,
                last_seen_at,
                user_agent,
                ip_address,
                tenant_id
            FROM user_sessions
            WHERE session_token_hash = $1
            "#,
            token_hash
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(UserSession {
                id: row.id,
                user_id: row.user_id,
                session_token_hash: row.session_token_hash,
                expires_at: row.expires_at,
                created_at: row.created_at,
                last_seen_at: row.last_seen_at,
                user_agent: row.user_agent,
                ip_address: row.ip_address,
                tenant_id: row.tenant_id,
            }))
        } else {
            Ok(None)
        }
    }

    /// Touch session activity for active browser sessions.
    pub async fn touch_user_session(&self, session_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE user_sessions
            SET last_seen_at = NOW()
            WHERE id = $1
            "#,
            session_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete a browser session by hashed token.
    pub async fn delete_user_session_by_token_hash(&self, token_hash: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM user_sessions WHERE session_token_hash = $1",
            token_hash
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List active browser sessions for a user.
    pub async fn list_user_sessions(&self, user_id: Uuid) -> Result<Vec<UserSession>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                id,
                user_id,
                session_token_hash,
                expires_at,
                created_at,
                last_seen_at,
                user_agent,
                ip_address,
                tenant_id
            FROM user_sessions
            WHERE user_id = $1
            ORDER BY last_seen_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(UserSession {
                    id: row.id,
                    user_id: row.user_id,
                    session_token_hash: row.session_token_hash,
                    expires_at: row.expires_at,
                    created_at: row.created_at,
                    last_seen_at: row.last_seen_at,
                    user_agent: row.user_agent,
                    ip_address: row.ip_address.map(|ip| ip.to_string()),
                    tenant_id: row.tenant_id,
                })
            })
            .collect()
    }

    /// Delete a browser session by session id, scoped to the owning user.
    pub async fn delete_user_session_by_id(&self, user_id: Uuid, session_id: Uuid) -> Result<()> {
        sqlx::query!(
            "DELETE FROM user_sessions WHERE user_id = $1 AND id = $2",
            user_id,
            session_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Create a security event entry for a user account.
    pub async fn create_user_security_event(
        &self,
        event: UserSecurityEventRecord<'_>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO user_security_events (
                id,
                user_id,
                event_type,
                description,
                ip_address,
                user_agent,
                session_id,
                occurred_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
            "#,
            Uuid::new_v4(),
            event.user_id,
            event.event_type,
            event.description,
            event.ip_address,
            event.user_agent,
            event.session_id,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List recent security events for a user account.
    pub async fn list_user_security_events(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<UserSecurityEvent>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                id,
                event_type,
                description,
                ip_address,
                user_agent,
                session_id,
                occurred_at
            FROM user_security_events
            WHERE user_id = $1
            ORDER BY occurred_at DESC
            LIMIT $2
            "#,
            user_id,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(UserSecurityEvent {
                    id: row.id,
                    event_type: row.event_type,
                    description: row.description,
                    ip_address: row.ip_address,
                    user_agent: row.user_agent,
                    session_id: row.session_id,
                    occurred_at: row.occurred_at,
                })
            })
            .collect()
    }

    /// Persist a short-lived OIDC login state.
    pub async fn create_oidc_login_state(&self, login_state: &OidcLoginState) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO oidc_login_states (
                state,
                pkce_verifier,
                nonce,
                redirect_to,
                expires_at,
                created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            login_state.state,
            login_state.pkce_verifier,
            login_state.nonce,
            login_state.redirect_to,
            login_state.expires_at,
            login_state.created_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Load an OIDC login state by the opaque state token.
    pub async fn find_oidc_login_state(&self, state: &str) -> Result<Option<OidcLoginState>> {
        let row = sqlx::query!(
            r#"
            SELECT
                state,
                pkce_verifier,
                nonce,
                redirect_to,
                expires_at,
                created_at
            FROM oidc_login_states
            WHERE state = $1
            "#,
            state
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(OidcLoginState {
                state: row.state,
                pkce_verifier: row.pkce_verifier,
                nonce: row.nonce,
                redirect_to: row.redirect_to,
                expires_at: row.expires_at,
                created_at: row.created_at,
            }))
        } else {
            Ok(None)
        }
    }

    /// Delete a consumed or expired OIDC login state.
    pub async fn delete_oidc_login_state(&self, state: &str) -> Result<()> {
        sqlx::query!("DELETE FROM oidc_login_states WHERE state = $1", state)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Find user by email using the same case-insensitive semantics as the
    /// tenant-scoped email uniqueness index.
    ///
    /// This lookup is intentionally unscoped for legacy password-login
    /// fallback only. Callers must reject ambiguous cross-tenant matches before
    /// authenticating the returned user.
    pub async fn find_user_by_email(&self, email: &str) -> Result<Option<User>> {
        let user = sqlx::query_as::<_, User>(
            r#"SELECT id, username, email, password_hash, display_name, is_admin, storage_quota, theme, created_at, updated_at, disabled_at, name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config FROM users WHERE LOWER(email) = LOWER($1) LIMIT 1"#,
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(user)
    }

    /// Count users matching the given email using case-insensitive semantics.
    pub async fn count_users_by_email(&self, email: &str) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM users WHERE LOWER(email) = LOWER($1)"#,
        )
        .bind(email)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    /// Find user by email scoped to a tenant (case-insensitive).
    pub async fn find_user_by_email_and_tenant(
        &self,
        email: &str,
        tenant_id: Uuid,
    ) -> Result<Option<User>> {
        let user = sqlx::query_as!(
            User,
            r#"
            SELECT id, username, email, password_hash, display_name, is_admin, storage_quota, theme as "theme: _", created_at, updated_at, disabled_at, name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config as "dashboard_config: _"
            FROM users
            WHERE LOWER(email) = LOWER($1) AND tenant_id = $2
            "#,
            email,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(user)
    }

    /// Find user by username.
    pub async fn find_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let user = sqlx::query_as!(
            User,
            r#"SELECT id, username, email, password_hash, display_name, is_admin, storage_quota, theme as "theme: _", created_at, updated_at, disabled_at, name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config as "dashboard_config: _" FROM users WHERE username = $1"#,
            username
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(user)
    }

    /// Find user by ID
    pub async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>> {
        let user = sqlx::query_as!(
            User,
            r#"SELECT id, username, email, password_hash, display_name, is_admin, storage_quota, theme as "theme: _", created_at, updated_at, disabled_at, name, surname, avatar_path, email_sharing_enabled, trash_retention_days, tenant_id, dashboard_config as "dashboard_config: _" FROM users WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(user)
    }

    /// Update a user's password hash and bump the updated timestamp.
    pub async fn update_user_password_hash(&self, id: Uuid, password_hash: &str) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE users
            SET password_hash = $2, updated_at = NOW()
            WHERE id = $1
            "#,
            id,
            password_hash
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Check if any users exist (for admin bootstrapping)
    pub async fn has_users(&self) -> Result<bool> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) as \"count!\" FROM users")
            .fetch_one(&self.pool)
            .await?;

        Ok(count > 0)
    }

    /// Update user's theme preference
    pub async fn update_user_theme(&self, user_id: Uuid, theme: &str) -> Result<()> {
        sqlx::query!(
            r#"UPDATE users SET theme = $1, updated_at = NOW() WHERE id = $2"#,
            theme,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update user profile fields
    pub async fn update_user_profile(
        &self,
        user_id: Uuid,
        name: Option<&str>,
        surname: Option<&str>,
        display_name: Option<&str>,
        email_sharing_enabled: Option<bool>,
        theme: Option<String>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE users SET
                name = COALESCE($1, name),
                surname = COALESCE($2, surname),
                display_name = COALESCE($3, display_name),
                email_sharing_enabled = COALESCE($4, email_sharing_enabled),
                theme = COALESCE($5, theme),
                updated_at = NOW()
            WHERE id = $6
            "#,
            name,
            surname,
            display_name,
            email_sharing_enabled,
            theme,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update user's trash retention setting.
    pub async fn update_user_trash_retention(
        &self,
        user_id: Uuid,
        days: Option<i32>,
    ) -> Result<()> {
        sqlx::query!(
            r#"UPDATE users SET trash_retention_days = $1, updated_at = NOW() WHERE id = $2"#,
            days,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update user's avatar path
    pub async fn update_user_avatar(&self, user_id: Uuid, avatar_path: Option<&str>) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE users
            SET avatar_path = $1,
                updated_at = NOW()
            WHERE id = $2
            "#,
            avatar_path,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update user's dashboard configuration
    pub async fn update_user_dashboard_config(
        &self,
        user_id: Uuid,
        config: &rustshare_core::domain::DashboardConfig,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE users
            SET dashboard_config = $1,
                updated_at = NOW()
            WHERE id = $2
            "#,
            serde_json::to_value(config).unwrap(),
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List all users that have trash auto-clean enabled (trash_retention_days IS NOT NULL).
    pub async fn list_users_with_trash_retention(&self) -> Result<Vec<(Uuid, Uuid, i32)>> {
        let rows = sqlx::query!(
            r#"SELECT id, tenant_id, trash_retention_days as "trash_retention_days!" FROM users WHERE trash_retention_days IS NOT NULL"#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| (r.id, r.tenant_id, r.trash_retention_days))
            .collect())
    }

    // -----------------------------------------------------------------
    // Login protection
    // -----------------------------------------------------------------

    /// Check if an IP address is currently blocked from logging in.
    pub async fn is_ip_blocked(&self, ip_address: &str) -> Result<bool> {
        Ok(sqlx::query!(
            r#"
            SELECT blocked_until
            FROM login_attempts
            WHERE ip_address = $1
            "#,
            ip_address
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|r| r.blocked_until.map(|t| t > Utc::now()).unwrap_or(false))
        .unwrap_or(false))
    }

    /// Record a failed login attempt for an IP address.
    /// If failed_count reaches max_login_attempts, blocks the IP for login_block_duration_minutes.
    pub async fn record_login_failure(&self, ip_address: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        let config = sqlx::query!(
            r#"
            SELECT login_protection_enabled, max_login_attempts, login_block_duration_minutes
            FROM security_config
            WHERE id = 1
            "#,
        )
        .fetch_one(&mut *tx)
        .await?;

        let enabled = config.login_protection_enabled;
        if !enabled {
            tx.commit().await?;
            return Ok(());
        }

        let max_attempts = config.max_login_attempts;
        let block_duration = config.login_block_duration_minutes;

        // Check if an existing block has expired — if so, reset the count
        let existing = sqlx::query!(
            "SELECT blocked_until FROM login_attempts WHERE ip_address = $1",
            ip_address
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(row) = existing {
            let blocked_until = row.blocked_until;
            if let Some(until) = blocked_until {
                if until <= Utc::now() {
                    // Block expired — reset count so user gets a fresh start
                    sqlx::query!(
                        "UPDATE login_attempts SET failed_count = 0, blocked_until = NULL WHERE ip_address = $1",
                        ip_address
                    )
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }

        let row = sqlx::query!(
            r#"
            INSERT INTO login_attempts (ip_address, failed_count, last_attempt_at)
            VALUES ($1, 1, NOW())
            ON CONFLICT (ip_address) DO UPDATE SET
                failed_count = login_attempts.failed_count + 1,
                last_attempt_at = NOW()
            RETURNING failed_count
            "#,
            ip_address
        )
        .fetch_one(&mut *tx)
        .await?;

        let failed_count = row.failed_count;

        if failed_count >= max_attempts {
            let block_until = Utc::now() + chrono::Duration::minutes(block_duration as i64);
            sqlx::query!(
                r#"
                UPDATE login_attempts
                SET blocked_until = $2
                WHERE ip_address = $1
                "#,
                ip_address,
                block_until
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Clear login attempts for an IP address after a successful login.
    pub async fn clear_login_attempts(&self, ip_address: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM login_attempts WHERE ip_address = $1",
            ip_address
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get the current security configuration.
    pub async fn get_security_config(&self) -> Result<Option<SecurityConfig>> {
        let row = sqlx::query!(
            r#"
            SELECT login_protection_enabled, max_login_attempts, login_block_duration_minutes, updated_at
            FROM security_config
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| SecurityConfig {
            login_protection_enabled: r.login_protection_enabled,
            max_login_attempts: r.max_login_attempts,
            login_block_duration_minutes: r.login_block_duration_minutes,
            updated_at: r.updated_at,
        }))
    }

    /// Update the security configuration.
    pub async fn update_security_config(
        &self,
        login_protection_enabled: Option<bool>,
        max_login_attempts: Option<i32>,
        login_block_duration_minutes: Option<i32>,
    ) -> Result<SecurityConfig> {
        let row = sqlx::query!(
            r#"
            UPDATE security_config
            SET
                login_protection_enabled = COALESCE($1, login_protection_enabled),
                max_login_attempts = COALESCE($2, max_login_attempts),
                login_block_duration_minutes = COALESCE($3, login_block_duration_minutes),
                updated_at = NOW()
            WHERE id = 1
            RETURNING login_protection_enabled, max_login_attempts, login_block_duration_minutes, updated_at
            "#,
            login_protection_enabled,
            max_login_attempts,
            login_block_duration_minutes
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(SecurityConfig {
            login_protection_enabled: row.login_protection_enabled,
            max_login_attempts: row.max_login_attempts,
            login_block_duration_minutes: row.login_block_duration_minutes,
            updated_at: row.updated_at,
        })
    }

    /// Create a new file in the projection table inside a transaction.
    pub async fn create_file_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        file: &File,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO files (id, name, path, size, mime_type, content_hash, storage_key, owner_id, parent_folder_id, current_version, created_at, modified_at, tenant_id, starred_at, deleted_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NULL, NULL)
            "#,
            file.id,
            &file.name,
            &file.path,
            file.size,
            &file.mime_type,
            &file.content_hash,
            file.storage_key(),
            file.owner_id,
            file.parent_folder_id,
            file.current_version,
            file.created_at,
            file.modified_at,
            file.tenant_id,
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Create a new file in the projection table
    pub async fn create_file(&self, file: &File) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.create_file_in_tx(&mut tx, file).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Find file by ID (owner-filtered)
    pub async fn find_file_by_id(&self, id: Uuid, owner_id: Uuid) -> Result<Option<File>> {
        let file = sqlx::query_as!(
            File,
            r#"SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id FROM files WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL"#,
            id,
            owner_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(file)
    }

    /// Find file by ID without owner check.
    ///
    /// ⚠️ WARNING: This bypasses ownership filtering. Only use for public-share
    /// endpoints or other cases where the caller has already verified access.
    pub async fn find_file_by_id_unchecked(&self, id: Uuid) -> Result<Option<File>> {
        let file = sqlx::query_as!(
            File,
            r#"SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id FROM files WHERE id = $1 AND deleted_at IS NULL"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(file)
    }

    /// Find a file by its canonical path for a specific owner.
    pub async fn find_file_by_path(&self, path: &str, owner_id: Uuid) -> Result<Option<File>> {
        let file = sqlx::query_as!(
            File,
            r#"
            SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id
            FROM files
            WHERE path = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            path,
            owner_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(file)
    }

    /// Update a file in the projection table inside a transaction.
    pub async fn update_file_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        file: &File,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE files
            SET name = $2, path = $3, size = $4, mime_type = $5, content_hash = $6,
                storage_key = $7, parent_folder_id = $8, current_version = $9, modified_at = $10, tenant_id = $11
            WHERE id = $1 AND owner_id = $12
            "#,
            file.id,
            &file.name,
            &file.path,
            file.size,
            &file.mime_type,
            &file.content_hash,
            file.storage_key(),
            file.parent_folder_id,
            file.current_version,
            file.modified_at,
            file.tenant_id,
            file.owner_id,
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Update a file in the projection table
    pub async fn update_file(&self, file: &File) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.update_file_in_tx(&mut tx, file).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Delete a file from the projection table inside a transaction.
    pub async fn delete_file_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        id: Uuid,
        owner_id: Uuid,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE files
            SET deleted_at = NOW(), starred_at = NULL
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            id,
            owner_id,
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Delete a file from the projection table
    pub async fn delete_file(&self, id: Uuid, owner_id: Uuid) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.delete_file_in_tx(&mut tx, id, owner_id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// List files with optional filters
    ///
    /// Returns files owned by the specified user, optionally filtered by parent folder.
    /// Pass `None` for parent_id to get files in the root directory (no parent).
    pub async fn list_files(
        &self,
        parent_id: Option<Uuid>,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<File>> {
        let files = sqlx::query_as!(
            File,
            r#"
            SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id
            FROM files
            WHERE owner_id = $1
              AND tenant_id = $2
              AND deleted_at IS NULL
              AND (parent_folder_id = $3 OR ($3 IS NULL AND parent_folder_id IS NULL))
            ORDER BY name ASC
            "#,
            owner_id,
            tenant_id,
            parent_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(files)
    }

    /// List files by parent folder regardless of owner.
    ///
    /// This is used for collaborative folders where children may be created by
    /// different users but still belong to the same parent folder.
    pub async fn list_files_by_parent(
        &self,
        parent_id: Option<Uuid>,
        tenant_id: Uuid,
    ) -> Result<Vec<File>> {
        let files = sqlx::query_as!(
            File,
            r#"
            SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id
            FROM files
            WHERE tenant_id = $1
              AND deleted_at IS NULL
              AND (parent_folder_id = $2 OR ($2 IS NULL AND parent_folder_id IS NULL))
            ORDER BY name ASC
            "#,
            tenant_id,
            parent_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(files)
    }

    /// Find all non-deleted files that belong to any of the given folders.
    pub async fn find_files_in_folders(
        &self,
        folder_ids: &[Uuid],
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<File>> {
        let files = sqlx::query_as::<_, File>(
            r#"
            SELECT
                id, name, path, size, mime_type, content_hash,
                owner_id, parent_folder_id, current_version,
                created_at, modified_at, starred_at, deleted_at, tenant_id
            FROM files
            WHERE tenant_id = $1
              AND owner_id = $2
              AND deleted_at IS NULL
              AND parent_folder_id = ANY($3)
            ORDER BY path ASC
            "#,
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(folder_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(files)
    }

    pub async fn set_file_starred(&self, id: Uuid, owner_id: Uuid, starred: bool) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE files
            SET starred_at = CASE WHEN $3 THEN NOW() ELSE NULL END
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            id,
            owner_id,
            starred,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn restore_file(&self, id: Uuid, owner_id: Uuid, tenant_id: Uuid) -> Result<bool> {
        let row = sqlx::query!(
            r#"
            SELECT id, name, parent_folder_id
            FROM files
            WHERE id = $1 AND owner_id = $2 AND tenant_id = $3 AND deleted_at IS NOT NULL
            "#,
            id,
            owner_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(false);
        };

        let name: String = row.name;
        let parent_folder_id: Option<Uuid> = row.parent_folder_id;

        let parent_path = if let Some(parent_id) = parent_folder_id {
            sqlx::query_scalar::<_, String>(
                r#"
                SELECT path
                FROM folders
                WHERE id = $1 AND owner_id = $2 AND tenant_id = $3 AND deleted_at IS NULL
                "#,
            )
            .bind(parent_id)
            .bind(owner_id)
            .bind(tenant_id)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let restored_parent_id = if parent_path.is_some() {
            parent_folder_id
        } else {
            None
        };
        let restored_path = if let Some(parent_path) = parent_path {
            if parent_path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", parent_path.trim_end_matches('/'), name)
            }
        } else {
            format!("/{}", name)
        };

        let result = sqlx::query!(
            r#"
            UPDATE files
            SET deleted_at = NULL, parent_folder_id = $2, path = $3
            WHERE id = $1 AND owner_id = $4 AND tenant_id = $5 AND deleted_at IS NOT NULL
            "#,
            id,
            restored_parent_id,
            restored_path,
            owner_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn permanently_delete_file(&self, id: Uuid, owner_id: Uuid) -> Result<bool> {
        let result = sqlx::query!(
            "DELETE FROM files WHERE id = $1 AND owner_id = $2 AND deleted_at IS NOT NULL",
            id,
            owner_id,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Create a new file version in the projection table inside a transaction.
    pub async fn create_file_version_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        version: &FileVersion,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO file_versions (
                id,
                file_id,
                version_number,
                content_hash,
                storage_key,
                size,
                replication_state,
                created_by,
                created_at,
                change_description,
                tenant_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (file_id, version_number) DO UPDATE SET
                content_hash = EXCLUDED.content_hash,
                storage_key = EXCLUDED.storage_key,
                size = EXCLUDED.size,
                replication_state = EXCLUDED.replication_state,
                created_at = EXCLUDED.created_at,
                change_description = EXCLUDED.change_description
            "#,
            version.id,
            version.file_id,
            version.version_number,
            &version.content_hash,
            version.storage_key(),
            version.size,
            version.replication_state.as_str(),
            version.created_by,
            version.created_at,
            version.change_description.as_deref(),
            version.tenant_id,
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Create a new file version in the projection table
    pub async fn create_file_version(&self, version: &FileVersion) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.create_file_version_in_tx(&mut tx, version).await?;
        tx.commit().await?;
        Ok(())
    }

    /// List all versions for a file, ordered by version number descending (newest first)
    pub async fn list_file_versions(
        &self,
        file_id: Uuid,
        owner_id: Uuid,
    ) -> Result<Vec<FileVersion>> {
        let rows = sqlx::query!(
            r#"
            SELECT v.id, v.file_id, v.version_number, v.content_hash, v.size, v.replication_state, v.created_by, v.created_at, v.change_description, v.tenant_id
            FROM file_versions v
            JOIN files f ON v.file_id = f.id
            WHERE v.file_id = $1 AND f.owner_id = $2
            ORDER BY v.version_number DESC
            "#,
            file_id,
            owner_id
        )
        .fetch_all(&self.pool)
        .await?;

        let mut versions = Vec::new();
        for row in rows {
            let version = FileVersion {
                id: row.id,
                file_id: row.file_id,
                version_number: row.version_number,
                content_hash: row.content_hash,
                size: row.size,
                replication_state: Self::parse_replication_state(&row.replication_state)?,
                created_by: row.created_by,
                created_at: row.created_at,
                change_description: row.change_description,
                tenant_id: row.tenant_id,
            };
            versions.push(version);
        }

        Ok(versions)
    }

    /// Find a specific version of a file
    pub async fn find_file_version(
        &self,
        file_id: Uuid,
        version: i32,
        owner_id: Uuid,
    ) -> Result<Option<FileVersion>> {
        let row = sqlx::query!(
            r#"
            SELECT v.id, v.file_id, v.version_number, v.content_hash, v.size, v.replication_state, v.created_by, v.created_at, v.change_description, v.tenant_id
            FROM file_versions v
            JOIN files f ON v.file_id = f.id
            WHERE v.file_id = $1 AND v.version_number = $2 AND f.owner_id = $3
            "#,
            file_id,
            version,
            owner_id
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let version = FileVersion {
                id: row.id,
                file_id: row.file_id,
                version_number: row.version_number,
                content_hash: row.content_hash,
                size: row.size,
                replication_state: Self::parse_replication_state(&row.replication_state)?,
                created_by: row.created_by,
                created_at: row.created_at,
                change_description: row.change_description,
                tenant_id: row.tenant_id,
            };
            Ok(Some(version))
        } else {
            Ok(None)
        }
    }

    /// Count enabled replication targets.
    pub async fn count_enabled_replication_targets(&self) -> Result<i64> {
        sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!" FROM replication_targets WHERE enabled = TRUE"#
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Create a durable replication job for asynchronous workers.
    pub async fn create_replication_job(&self, job: &ReplicationJob) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO replication_jobs (
                id,
                file_id,
                file_version_id,
                storage_key,
                status,
                attempt_count,
                next_attempt_at,
                last_attempt_at,
                leased_at,
                lease_token,
                last_error,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
            job.id,
            job.file_id,
            job.file_version_id,
            &job.storage_key,
            job.status.as_str(),
            job.attempt_count,
            job.next_attempt_at,
            job.last_attempt_at,
            job.leased_at,
            job.lease_token,
            job.last_error.as_deref(),
            job.created_at,
            job.updated_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update replication state after queueing or worker progress.
    pub async fn update_file_version_replication_state(
        &self,
        version_id: Uuid,
        state: ReplicationState,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE file_versions
            SET replication_state = $2
            WHERE id = $1
            "#,
            version_id,
            state.as_str(),
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List enabled replication targets that workers should copy into.
    pub async fn list_enabled_replication_targets(&self) -> Result<Vec<ReplicationTarget>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                id,
                name,
                destination_type,
                endpoint,
                bucket,
                region,
                base_path,
                is_required,
                enabled,
                auth_config,
                health_status,
                last_healthy_at,
                last_error,
                created_at,
                updated_at
            FROM replication_targets
            WHERE enabled = TRUE
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut targets = Vec::with_capacity(rows.len());
        for row in rows {
            targets.push(ReplicationTarget {
                id: row.id,
                name: row.name,
                destination_type: row.destination_type,
                endpoint: row.endpoint,
                bucket: row.bucket,
                region: row.region,
                base_path: row.base_path,
                is_required: row.is_required,
                enabled: row.enabled,
                auth_config: row.auth_config,
                health_status: row.health_status,
                last_healthy_at: row.last_healthy_at,
                last_error: row.last_error,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(targets)
    }

    /// Lease due replication jobs for background processing.
    pub async fn lease_replication_jobs(
        &self,
        limit: i64,
        lease_timeout_secs: i64,
        lease_token: Uuid,
    ) -> Result<Vec<ReplicationJob>> {
        let rows = sqlx::query!(
            r#"
            WITH candidates AS (
                SELECT id
                FROM replication_jobs
                WHERE status IN ('queued', 'retrying')
                  AND next_attempt_at <= NOW()
                  AND (
                    leased_at IS NULL
                    OR leased_at < NOW() - make_interval(secs => $2)
                  )
                ORDER BY next_attempt_at ASC, created_at ASC
                LIMIT $1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE replication_jobs
            SET
                status = 'syncing',
                leased_at = NOW(),
                lease_token = $3,
                last_attempt_at = NOW(),
                attempt_count = attempt_count + 1,
                updated_at = NOW()
            WHERE id IN (SELECT id FROM candidates)
            RETURNING
                id,
                file_id,
                file_version_id,
                storage_key,
                status as "status!",
                attempt_count,
                next_attempt_at,
                last_attempt_at,
                leased_at,
                lease_token,
                last_error,
                created_at,
                updated_at
            "#,
            limit,
            lease_timeout_secs as f64,
            lease_token,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut jobs = Vec::with_capacity(rows.len());
        for row in rows {
            let status: String = row.status;
            jobs.push(ReplicationJob {
                id: row.id,
                file_id: row.file_id,
                file_version_id: row.file_version_id,
                storage_key: row.storage_key,
                status: Self::parse_replication_job_status(&status)?,
                attempt_count: row.attempt_count,
                next_attempt_at: row.next_attempt_at,
                last_attempt_at: row.last_attempt_at,
                leased_at: row.leased_at,
                lease_token: row.lease_token,
                last_error: row.last_error,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(jobs)
    }

    /// Mark a replication job as completed and release its lease.
    pub async fn mark_replication_job_completed(&self, job_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE replication_jobs
            SET
                status = 'completed',
                leased_at = NULL,
                lease_token = NULL,
                last_error = NULL,
                updated_at = NOW()
            WHERE id = $1
            "#,
            job_id,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark a replication job for retry after a transient failure.
    pub async fn mark_replication_job_retrying(
        &self,
        job_id: Uuid,
        last_error: &str,
        next_attempt_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE replication_jobs
            SET
                status = 'retrying',
                leased_at = NULL,
                lease_token = NULL,
                last_error = $2,
                next_attempt_at = $3,
                updated_at = NOW()
            WHERE id = $1
            "#,
            job_id,
            last_error,
            next_attempt_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark a replication job as terminally failed.
    pub async fn mark_replication_job_failed(&self, job_id: Uuid, last_error: &str) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE replication_jobs
            SET
                status = 'failed',
                leased_at = NULL,
                lease_token = NULL,
                last_error = $2,
                updated_at = NOW()
            WHERE id = $1
            "#,
            job_id,
            last_error,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Record the result of a single target replication attempt.
    pub async fn create_replication_attempt(
        &self,
        attempt: ReplicationAttemptRecord<'_>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO replication_attempts (
                id,
                job_id,
                target_id,
                attempt_number,
                status,
                error_message,
                started_at,
                completed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            Uuid::new_v4(),
            attempt.job_id,
            attempt.target_id,
            attempt.attempt_number,
            attempt.status,
            attempt.error_message,
            attempt.started_at,
            attempt.completed_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update target health after a replication attempt.
    pub async fn update_replication_target_health(
        &self,
        target_id: Uuid,
        health_status: &str,
        last_error: Option<&str>,
        last_healthy_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE replication_targets
            SET
                health_status = $2,
                last_error = $3,
                last_healthy_at = COALESCE($4, last_healthy_at),
                updated_at = NOW()
            WHERE id = $1
            "#,
            target_id,
            health_status,
            last_error,
            last_healthy_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Create a new folder in the projection table inside a transaction.
    pub async fn create_folder_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        folder: &Folder,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO folders (id, name, path, parent_folder_id, owner_id, created_at, updated_at, tenant_id, starred_at, deleted_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, NULL)
            "#,
            folder.id,
            folder.name,
            folder.path,
            folder.parent_folder_id,
            folder.owner_id,
            folder.created_at,
            folder.updated_at,
            folder.tenant_id,
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Create a new folder in the projection table
    pub async fn create_folder(&self, folder: &Folder) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.create_folder_in_tx(&mut tx, folder).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Find folder by ID
    pub async fn find_folder_by_id(&self, id: Uuid, owner_id: Uuid) -> Result<Option<Folder>> {
        let folder = sqlx::query_as!(
            Folder,
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id,
                   NULL::uuid[] as "ancestor_ids: _"
            FROM folders
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            id,
            owner_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(folder)
    }

    /// Find a folder by ID without owner filtering.
    ///
    /// ⚠️ WARNING: This bypasses ownership filtering. Only use for permission
    /// resolution or other cases where the caller has already verified access.
    pub async fn find_folder_by_id_unchecked(&self, id: Uuid) -> Result<Option<Folder>> {
        let folder = sqlx::query_as!(
            Folder,
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id,
                   NULL::uuid[] as "ancestor_ids: _"
            FROM folders
            WHERE id = $1 AND deleted_at IS NULL
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(folder)
    }

    /// Find a folder by its canonical path for a specific owner.
    pub async fn find_folder_by_path(&self, path: &str, owner_id: Uuid) -> Result<Option<Folder>> {
        let folder = sqlx::query_as!(
            Folder,
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id,
                   NULL::uuid[] as "ancestor_ids: _"
            FROM folders
            WHERE path = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            path,
            owner_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(folder)
    }

    /// Update a folder in the projection table inside a transaction.
    pub async fn update_folder_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        folder: &Folder,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE folders
            SET name = $2, path = $3, parent_folder_id = $4, updated_at = $5, tenant_id = $6
            WHERE id = $1 AND owner_id = $7
            "#,
            folder.id,
            folder.name,
            folder.path,
            folder.parent_folder_id,
            folder.updated_at,
            folder.tenant_id,
            folder.owner_id,
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Update a folder in the projection table
    pub async fn update_folder(&self, folder: &Folder) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.update_folder_in_tx(&mut tx, folder).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Delete a folder from the projection table inside a transaction.
    pub async fn delete_folder_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'static, sqlx::Postgres>,
        id: Uuid,
        owner_id: Uuid,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE folders
            SET deleted_at = NOW(), starred_at = NULL
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            id,
            owner_id,
        )
        .execute(&mut **tx)
        .await?;

        sqlx::query!(
            r#"
            UPDATE files
            SET deleted_at = COALESCE(deleted_at, NOW()), starred_at = NULL
            WHERE parent_folder_id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            id,
            owner_id,
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    /// Delete a folder from the projection table
    pub async fn delete_folder(&self, id: Uuid, owner_id: Uuid) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        self.delete_folder_in_tx(&mut tx, id, owner_id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// List folders with optional filters
    ///
    /// Returns folders owned by the specified user, optionally filtered by parent folder.
    /// Pass `None` for parent_id to get folders in the root directory (no parent).
    pub async fn list_folders(
        &self,
        parent_id: Option<Uuid>,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<Folder>> {
        let folders = sqlx::query_as!(
            Folder,
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id,
                   NULL::uuid[] as "ancestor_ids: _"
            FROM folders
            WHERE owner_id = $1
              AND tenant_id = $2
              AND deleted_at IS NULL
              AND (parent_folder_id = $3 OR ($3 IS NULL AND parent_folder_id IS NULL))
            ORDER BY name ASC
            "#,
            owner_id,
            tenant_id,
            parent_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(folders)
    }

    /// List folders by parent folder regardless of owner.
    ///
    /// This preserves collaborative folder structure when shared folders contain
    /// children created by multiple users.
    pub async fn list_folders_by_parent(
        &self,
        parent_id: Option<Uuid>,
        tenant_id: Uuid,
    ) -> Result<Vec<Folder>> {
        let folders = sqlx::query_as!(
            Folder,
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id,
                   NULL::uuid[] as "ancestor_ids: _"
            FROM folders
            WHERE tenant_id = $1
              AND deleted_at IS NULL
              AND (parent_folder_id = $2 OR ($2 IS NULL AND parent_folder_id IS NULL))
            ORDER BY name ASC
            "#,
            tenant_id,
            parent_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(folders)
    }

    /// List folders with share counts
    ///
    /// Returns folders owned by the specified user with share information.
    pub async fn list_folders_with_shares(
        &self,
        parent_id: Option<Uuid>,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<FolderWithShares>> {
        let rows = sqlx::query!(
            r#"
            SELECT 
                f.id, f.name, f.path, f.parent_folder_id, f.owner_id, 
                f.created_at, f.updated_at, f.tenant_id,
                f.starred_at, f.deleted_at,
                EXISTS (
                    SELECT 1 FROM shares 
                    WHERE folder_id = f.id 
                    AND revoked_at IS NULL
                ) as "is_shared!",
                (
                    SELECT COUNT(*) FROM shares
                    WHERE folder_id = f.id
                    AND revoked_at IS NULL
                ) as "share_count!",
                (
                    SELECT MIN(expires_at) FROM shares
                    WHERE folder_id = f.id
                    AND revoked_at IS NULL
                ) as share_expires_at
            FROM folders f
            WHERE f.owner_id = $1
              AND f.tenant_id = $2
              AND f.deleted_at IS NULL
              AND (f.parent_folder_id = $3 OR ($3 IS NULL AND f.parent_folder_id IS NULL))
            ORDER BY f.name ASC
            "#,
            owner_id,
            tenant_id,
            parent_id
        )
        .fetch_all(&self.pool)
        .await?;

        let mut folders = Vec::new();
        for row in rows {
            let folder = FolderWithShares {
                id: row.id,
                name: row.name,
                path: row.path,
                parent_folder_id: row.parent_folder_id,
                owner_id: row.owner_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
                starred_at: row.starred_at,
                deleted_at: row.deleted_at,
                tenant_id: row.tenant_id,
                ancestor_ids: None,
                is_shared: row.is_shared,
                share_count: row.share_count,
                share_expires_at: row.share_expires_at,
            };
            folders.push(folder);
        }

        Ok(folders)
    }

    pub async fn set_folder_starred(
        &self,
        id: Uuid,
        owner_id: Uuid,
        starred: bool,
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE folders
            SET starred_at = CASE WHEN $3 THEN NOW() ELSE NULL END
            WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL
            "#,
            id,
            owner_id,
            starred,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn restore_folder(&self, id: Uuid, owner_id: Uuid, tenant_id: Uuid) -> Result<bool> {
        let row = sqlx::query!(
            r#"
            SELECT id, name, path, parent_folder_id
            FROM folders
            WHERE id = $1 AND owner_id = $2 AND tenant_id = $3 AND deleted_at IS NOT NULL
            "#,
            id,
            owner_id,
            tenant_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(false);
        };

        let name: String = row.name;
        let old_path: String = row.path;
        let parent_folder_id: Option<Uuid> = row.parent_folder_id;

        let parent_row = if let Some(parent_id) = parent_folder_id {
            sqlx::query!(
                r#"
                SELECT id, path
                FROM folders
                WHERE id = $1 AND owner_id = $2 AND tenant_id = $3 AND deleted_at IS NULL
                "#,
                parent_id,
                owner_id,
                tenant_id,
            )
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let restored_parent_id: Option<Uuid> = parent_row.as_ref().map(|value| value.id);
        let restored_path = if let Some(parent_row) = &parent_row {
            let parent_path: String = parent_row.path.clone();
            if parent_path == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", parent_path.trim_end_matches('/'), name)
            }
        } else {
            format!("/{}", name)
        };

        let duplicate = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM folders
            WHERE owner_id = $1
              AND tenant_id = $2
              AND deleted_at IS NULL
              AND parent_folder_id IS NOT DISTINCT FROM $3
              AND name = $4
              AND id <> $5
            "#,
        )
        .bind(owner_id)
        .bind(tenant_id)
        .bind(restored_parent_id)
        .bind(&name)
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        if duplicate > 0 {
            anyhow::bail!(
                "A folder named `{}` already exists in the restore destination",
                name
            );
        }

        sqlx::query!(
            r#"
            UPDATE folders
            SET deleted_at = NULL, parent_folder_id = $2, path = $3
            WHERE id = $1 AND owner_id = $4 AND tenant_id = $5 AND deleted_at IS NOT NULL
            "#,
            id,
            restored_parent_id,
            &restored_path,
            owner_id,
            tenant_id,
        )
        .execute(&self.pool)
        .await?;

        let old_prefix = format!("{}/%", old_path.trim_end_matches('/'));
        let new_prefix = format!("{}/", restored_path.trim_end_matches('/'));

        sqlx::query!(
            r#"
            UPDATE folders
            SET deleted_at = NULL,
                path = $2 || substr(path, length($3) + 1)
            WHERE owner_id = $1
              AND tenant_id = $4
              AND path LIKE $5
            "#,
            owner_id,
            &new_prefix,
            &old_path,
            tenant_id,
            &old_prefix,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query!(
            r#"
            UPDATE files
            SET deleted_at = NULL,
                path = $2 || substr(path, length($3) + 1)
            WHERE owner_id = $1
              AND tenant_id = $4
              AND path LIKE $5
            "#,
            owner_id,
            &new_prefix,
            &old_path,
            tenant_id,
            &old_prefix,
        )
        .execute(&self.pool)
        .await?;

        Ok(true)
    }

    pub async fn permanently_delete_folder(&self, id: Uuid, owner_id: Uuid) -> Result<bool> {
        let result = sqlx::query!(
            "DELETE FROM folders WHERE id = $1 AND owner_id = $2 AND deleted_at IS NOT NULL",
            id,
            owner_id,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get a summary of trashed items for a user.
    pub async fn get_trash_summary(
        &self,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<(i64, i64, i64)> {
        let file_row = sqlx::query!(
            r#"
            SELECT COUNT(*) as count, COALESCE(SUM(size), 0)::bigint as total_size
            FROM files
            WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL
            "#,
            owner_id,
            tenant_id,
        )
        .fetch_one(&self.pool)
        .await?;

        let file_count: i64 = file_row.count.unwrap_or(0);
        let total_size: i64 = file_row.total_size.unwrap_or(0);

        let folder_row = sqlx::query!(
            "SELECT COUNT(*) as count FROM folders WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL",
            owner_id,
            tenant_id,
        )
        .fetch_one(&self.pool)
        .await?;

        let folder_count: i64 = folder_row.count.unwrap_or(0);

        Ok((file_count, folder_count, total_size))
    }

    /// Permanently delete all trashed items for a user.
    pub async fn empty_trash(&self, owner_id: Uuid, tenant_id: Uuid) -> Result<()> {
        // Delete trashed files first (to avoid FK violations when deleting folders)
        sqlx::query!(
            "DELETE FROM files WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL",
            owner_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        // Delete trashed folders (cascade will handle any remaining child records)
        sqlx::query!(
            "DELETE FROM folders WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL",
            owner_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Permanently delete trashed items older than the given number of days for a user.
    pub async fn clean_old_trash(&self, owner_id: Uuid, tenant_id: Uuid, days: i32) -> Result<u64> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days.into());

        // Delete old trashed files first
        let file_result = sqlx::query!(
            "DELETE FROM files WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL AND deleted_at < $3",
            owner_id,
            tenant_id,
            cutoff
        )
        .execute(&self.pool)
        .await?;

        // Delete old trashed folders
        let folder_result = sqlx::query!(
            "DELETE FROM folders WHERE owner_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL AND deleted_at < $3",
            owner_id,
            tenant_id,
            cutoff
        )
        .execute(&self.pool)
        .await?;

        Ok(file_result.rows_affected() + folder_result.rows_affected())
    }

    /// Delete audit logs (admin_actions) older than the given number of days.
    pub async fn clean_audit_logs_older_than(&self, days: i64) -> Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM admin_actions WHERE performed_at < NOW() - INTERVAL '1 day' * $1",
            days as f64
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Delete expired browser sessions older than the given number of days.
    pub async fn clean_expired_sessions_older_than(&self, days: i64) -> Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM user_sessions WHERE expires_at < NOW() - INTERVAL '1 day' * $1",
            days as f64
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Delete expired shares older than the given number of days.
    pub async fn clean_expired_shares_older_than(&self, days: i64) -> Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM shares WHERE expires_at IS NOT NULL AND expires_at < NOW() - INTERVAL '1 day' * $1",
            days as f64
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Delete old file versions older than the given number of days,
    /// keeping at least 3 versions per file.
    pub async fn clean_old_file_versions(&self, days: i64) -> Result<u64> {
        let result = sqlx::query!(
            r#"
            DELETE FROM file_versions
            WHERE id IN (
                SELECT fv.id
                FROM file_versions fv
                JOIN files f ON f.id = fv.file_id
                WHERE fv.created_at < NOW() - INTERVAL '1 day' * $1
                AND fv.version_number <= f.current_version - 3
            )
            "#,
            days as f64
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Delete replication attempt history older than the given number of days.
    pub async fn clean_replication_history_older_than(&self, days: i64) -> Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM replication_attempts WHERE completed_at < NOW() - INTERVAL '1 day' * $1",
            days as f64
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Delete OIDC login states older than the given number of days.
    pub async fn clean_oidc_states_older_than(&self, days: i64) -> Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM oidc_login_states WHERE created_at < NOW() - INTERVAL '1 day' * $1",
            days as f64
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Delete device pair requests older than the given number of days.
    pub async fn clean_device_pairs_older_than(&self, days: i64) -> Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM device_pair_requests WHERE expires_at < NOW() - INTERVAL '1 day' * $1",
            days as f64
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Delete webhook delivery logs older than the given number of days.
    pub async fn clean_webhook_logs_older_than(&self, days: i64) -> Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM webhook_logs WHERE created_at < NOW() - INTERVAL '1 day' * $1",
            days as f64
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Find all descendant folders of a given folder using recursive CTE
    ///
    /// Returns all folders in the subtree rooted at the specified folder,
    /// including the folder itself and all its direct and indirect children.
    /// Find all descendant folders of a given folder, filtered by owner.
    ///
    /// Use this for operations where the caller has already verified the user
    /// owns the root folder (e.g., delete, move).
    pub async fn find_descendant_folders(
        &self,
        folder_id: Uuid,
        owner_id: Uuid,
    ) -> Result<Vec<Folder>> {
        let rows = sqlx::query!(
            r#"
            WITH RECURSIVE folder_tree AS (
                -- Base case: start with the specified folder
                SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id
                FROM folders
                WHERE id = $1 AND owner_id = $2 AND deleted_at IS NULL

                UNION ALL

                -- Recursive case: get all direct children
                SELECT f.id, f.name, f.path, f.parent_folder_id, f.owner_id, f.created_at, f.updated_at, f.starred_at, f.deleted_at, f.tenant_id
                FROM folders f
                INNER JOIN folder_tree ft ON f.parent_folder_id = ft.id
                WHERE f.owner_id = $2 AND f.deleted_at IS NULL
            )
            SELECT id as "id!", name as "name!", path as "path!", parent_folder_id as "parent_folder_id", owner_id as "owner_id!", created_at as "created_at!", updated_at as "updated_at!", starred_at as "starred_at", deleted_at as "deleted_at", tenant_id as "tenant_id!",
                   NULL::uuid[] as "ancestor_ids: Vec<Uuid>"
            FROM folder_tree
            ORDER BY path ASC
            "#,
            folder_id,
            owner_id
        )
        .fetch_all(&self.pool)
        .await?;

        let mut folders = Vec::new();
        for row in rows {
            let folder = Folder {
                id: row.id,
                name: row.name,
                path: row.path,
                parent_folder_id: row.parent_folder_id,
                owner_id: row.owner_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
                starred_at: row.starred_at,
                deleted_at: row.deleted_at,
                tenant_id: row.tenant_id,
                ancestor_ids: row.ancestor_ids,
            };
            folders.push(folder);
        }

        Ok(folders)
    }

    /// Find all descendant folders of a given folder without owner filtering.
    ///
    /// ⚠️ WARNING: This bypasses ownership filtering. Only use for public-share
    /// endpoints or other cases where the caller has already verified access.
    pub async fn find_descendant_folders_unchecked(&self, folder_id: Uuid) -> Result<Vec<Folder>> {
        let rows = sqlx::query!(
            r#"
            WITH RECURSIVE folder_tree AS (
                -- Base case: start with the specified folder
                SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id
                FROM folders
                WHERE id = $1 AND deleted_at IS NULL

                UNION ALL

                -- Recursive case: get all direct children
                SELECT f.id, f.name, f.path, f.parent_folder_id, f.owner_id, f.created_at, f.updated_at, f.starred_at, f.deleted_at, f.tenant_id
                FROM folders f
                INNER JOIN folder_tree ft ON f.parent_folder_id = ft.id
                WHERE f.deleted_at IS NULL
            )
            SELECT id as "id!", name as "name!", path as "path!", parent_folder_id as "parent_folder_id", owner_id as "owner_id!", created_at as "created_at!", updated_at as "updated_at!", starred_at as "starred_at", deleted_at as "deleted_at", tenant_id as "tenant_id!",
                   NULL::uuid[] as "ancestor_ids: Vec<Uuid>"
            FROM folder_tree
            ORDER BY path ASC
            "#,
            folder_id
        )
        .fetch_all(&self.pool)
        .await?;

        let mut folders = Vec::new();
        for row in rows {
            let folder = Folder {
                id: row.id,
                name: row.name,
                path: row.path,
                parent_folder_id: row.parent_folder_id,
                owner_id: row.owner_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
                starred_at: row.starred_at,
                deleted_at: row.deleted_at,
                tenant_id: row.tenant_id,
                ancestor_ids: row.ancestor_ids,
            };
            folders.push(folder);
        }

        Ok(folders)
    }

    /// Create a new share link for a file
    pub async fn create_share(&self, share: &Share) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO shares (id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions, password_hash, expires_at, upload_only, access_count, created_at, tenant_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            "#,
            share.id,
            share.file_id,
            share.folder_id,
            share.share_token,
            share.recipient_user_id,
            share.recipient_group_id,
            share.created_by,
            Self::permission_to_db_value(share.permissions),
            share.password_hash,
            share.expires_at,
            share.upload_only,
            share.access_count,
            share.created_at,
            share.tenant_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Find a share by its token, scoped to a tenant.
    pub async fn get_share_by_token(&self, token: &str, tenant_id: Uuid) -> Result<Option<Share>> {
        let share = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions as "permissions: SharePermissions", password_hash, expires_at, upload_only, access_count, created_at, revoked_at, tenant_id
            FROM shares
            WHERE share_token = $1 AND tenant_id = $2
            "#,
            token,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(share)
    }

    /// Find a share by its token without tenant scoping.
    ///
    /// Public share tokens are globally unique, so this is safe to use for
    /// public-share endpoints that perform their own tenant verification.
    pub async fn get_share_by_token_unscoped(&self, token: &str) -> Result<Option<Share>> {
        let share = sqlx::query_as::<_, Share>(
            r#"
            SELECT id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions::text AS permissions, password_hash, expires_at, upload_only, access_count, created_at, revoked_at, tenant_id
            FROM shares
            WHERE share_token = $1
            "#,
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;
        Ok(share)
    }

    /// Find a share by ID
    pub async fn get_share(&self, share_id: Uuid, actor_id: Uuid) -> Result<Option<Share>> {
        let share = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions as "permissions: SharePermissions", password_hash, expires_at, upload_only, access_count, created_at, revoked_at, tenant_id
            FROM shares
            WHERE id = $1 AND (recipient_user_id = $2 OR created_by = $2)
            "#,
            share_id,
            actor_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(share)
    }

    /// Find a share by ID without actor filtering.
    pub async fn get_share_unchecked(&self, share_id: Uuid) -> Result<Option<Share>> {
        let share = sqlx::query_as::<_, Share>(
            r#"
            SELECT id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions::text AS permissions, password_hash, expires_at, upload_only, access_count, created_at, revoked_at, tenant_id
            FROM shares
            WHERE id = $1
            "#
        )
        .bind(share_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(share)
    }

    /// Get all active (non-revoked) shares for a file
    pub async fn get_file_shares(&self, file_id: Uuid) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions as "permissions: SharePermissions", password_hash, expires_at, upload_only, access_count, created_at, revoked_at, tenant_id
            FROM shares
            WHERE file_id = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
            file_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(shares)
    }

    /// Get all active (non-revoked) shares for a folder.
    pub async fn get_folder_shares(&self, folder_id: Uuid) -> Result<Vec<Share>> {
        let shares = sqlx::query_as!(
            Share,
            r#"
            SELECT id, file_id, folder_id, share_token, recipient_user_id, recipient_group_id, created_by, permissions as "permissions: SharePermissions", password_hash, expires_at, upload_only, access_count, created_at, revoked_at, tenant_id
            FROM shares
            WHERE folder_id = $1 AND revoked_at IS NULL
            ORDER BY created_at DESC
            "#,
            folder_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(shares)
    }

    /// Get all active public shares created by a specific user, with file names.
    pub async fn get_user_public_shares(&self, user_id: Uuid) -> Result<Vec<OwnedPublicShare>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                s.id,
                s.file_id,
                s.folder_id,
                s.share_token,
                s.recipient_user_id,
                s.recipient_group_id,
                s.created_by,
                s.permissions,
                s.password_hash,
                s.expires_at,
                s.upload_only,
                s.access_count,
                s.created_at,
                s.revoked_at,
                s.tenant_id,
                COALESCE(s.file_id, s.folder_id) AS "resource_id!",
                CASE
                    WHEN s.file_id IS NOT NULL THEN 'file'
                    ELSE 'folder'
                END AS "resource_type!",
                COALESCE(f.name, fo.name) AS "resource_name!"
            FROM shares s
            LEFT JOIN files f ON f.id = s.file_id
            LEFT JOIN folders fo ON fo.id = s.folder_id
            WHERE s.created_by = $1
              AND s.recipient_user_id IS NULL
              AND s.recipient_group_id IS NULL
              AND s.revoked_at IS NULL
            ORDER BY s.created_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await?;

        let mut shares = Vec::with_capacity(rows.len());
        for row in rows {
            let permissions = Self::permission_from_db_value(&row.permissions);

            shares.push(OwnedPublicShare {
                share: Share {
                    id: row.id,
                    file_id: row.file_id,
                    folder_id: row.folder_id,
                    share_token: row.share_token,
                    recipient_user_id: row.recipient_user_id,
                    recipient_group_id: row.recipient_group_id,
                    created_by: row.created_by,
                    permissions,
                    password_hash: row.password_hash,
                    expires_at: row.expires_at,
                    upload_only: row.upload_only,
                    access_count: row.access_count,
                    created_at: row.created_at,
                    revoked_at: row.revoked_at,
                    tenant_id: row.tenant_id,
                },
                resource_id: row.resource_id,
                resource_type: row.resource_type,
                resource_name: row.resource_name,
            });
        }

        Ok(shares)
    }

    /// Get all active shares created by a specific user (public, user, and group shares).
    pub async fn get_user_all_shares(
        &self,
        user_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<OwnedPublicShare>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                s.id,
                s.file_id,
                s.folder_id,
                s.share_token,
                s.recipient_user_id,
                s.recipient_group_id,
                s.created_by,
                s.permissions,
                s.password_hash,
                s.expires_at,
                s.upload_only,
                s.access_count,
                s.created_at,
                s.revoked_at,
                s.tenant_id,
                COALESCE(s.file_id, s.folder_id) AS "resource_id!",
                CASE
                    WHEN s.file_id IS NOT NULL THEN 'file'
                    ELSE 'folder'
                END AS "resource_type!",
                COALESCE(f.name, fo.name) AS "resource_name!"
            FROM shares s
            LEFT JOIN files f ON f.id = s.file_id
            LEFT JOIN folders fo ON fo.id = s.folder_id
            WHERE s.created_by = $1
              AND s.revoked_at IS NULL
            ORDER BY s.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            user_id,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        let mut shares = Vec::with_capacity(rows.len());
        for row in rows {
            let permissions = Self::permission_from_db_value(&row.permissions);

            shares.push(OwnedPublicShare {
                share: Share {
                    id: row.id,
                    file_id: row.file_id,
                    folder_id: row.folder_id,
                    share_token: row.share_token,
                    recipient_user_id: row.recipient_user_id,
                    recipient_group_id: row.recipient_group_id,
                    created_by: row.created_by,
                    permissions,
                    password_hash: row.password_hash,
                    expires_at: row.expires_at,
                    upload_only: row.upload_only,
                    access_count: row.access_count,
                    created_at: row.created_at,
                    revoked_at: row.revoked_at,
                    tenant_id: row.tenant_id,
                },
                resource_id: row.resource_id,
                resource_type: row.resource_type,
                resource_name: row.resource_name,
            });
        }

        Ok(shares)
    }

    /// Get access-log entries for a public share owned by a specific user.
    pub async fn get_public_share_access_log(
        &self,
        share_id: Uuid,
        owner_id: Uuid,
        limit: i64,
    ) -> Result<Vec<PublicShareAccessLogEntry>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                sal.accessed_at,
                sal.action,
                sal.success,
                sal.ip_address,
                sal.user_agent,
                sal.actor_type,
                sal.actor_label,
                sal.share_session_id,
                sal.share_session_subject
            FROM share_access_log sal
            INNER JOIN shares s ON s.id = sal.share_id
            WHERE sal.share_id = $1
              AND s.created_by = $2
              AND s.recipient_user_id IS NULL
            ORDER BY sal.accessed_at DESC
            LIMIT $3
            "#,
            share_id,
            owner_id,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(PublicShareAccessLogEntry {
                    accessed_at: row.accessed_at,
                    action: row.action,
                    success: row.success,
                    ip_address: row.ip_address.map(|ip| ip.to_string()),
                    user_agent: row.user_agent,
                    actor_type: row.actor_type,
                    actor_label: row.actor_label,
                    share_session_id: row.share_session_id,
                    share_session_subject: row.share_session_subject,
                })
            })
            .collect()
    }

    /// Update a share's password, expiration, and permissions
    pub async fn update_share(&self, share: &Share) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE shares
            SET password_hash = $2, expires_at = $3, tenant_id = $4, permissions = $5
            WHERE id = $1 AND created_by = $6
            "#,
            share.id,
            share.password_hash,
            share.expires_at,
            share.tenant_id,
            Self::permission_to_db_value(share.permissions),
            share.created_by
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Revoke a share link (soft delete)
    pub async fn revoke_share(&self, share_id: Uuid, actor_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE shares
            SET revoked_at = NOW()
            WHERE id = $1 AND created_by = $2
            "#,
            share_id,
            actor_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Increment share access count and update last_accessed_at
    pub async fn increment_share_access(&self, share_id: Uuid) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE shares
            SET access_count = access_count + 1, last_accessed_at = NOW()
            WHERE id = $1
            "#,
            share_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Log a share access attempt
    pub async fn log_share_access(&self, entry: ShareAccessLogEntry) -> Result<()> {
        // Validate IP address format before storage
        let validated_ip = entry
            .ip_address
            .and_then(|ip| ip.parse::<sqlx::types::ipnetwork::IpNetwork>().ok());

        sqlx::query!(
            r#"
            INSERT INTO share_access_log (
                share_id, ip_address, user_agent, action, success,
                actor_type, actor_label, share_session_id, share_session_subject
            )
            VALUES ($1, $2::inet, $3, $4, $5, $6, $7, $8, $9)
            "#,
            entry.share_id,
            validated_ip,
            entry.user_agent,
            entry.action,
            entry.success,
            entry.actor_type,
            entry.actor_label,
            entry.share_session_id,
            entry.share_session_subject
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List all markdown files for a user across their entire library.
    pub async fn list_all_folders(&self, owner_id: Uuid, tenant_id: Uuid) -> Result<Vec<Folder>> {
        let folders = sqlx::query_as!(
            Folder,
            r#"
            SELECT id, name, path, parent_folder_id, owner_id, created_at, updated_at, starred_at, deleted_at, tenant_id,
                NULL::uuid[] as "ancestor_ids: _"
            FROM folders
            WHERE owner_id = $1
              AND tenant_id = $2
              AND deleted_at IS NULL
            ORDER BY path ASC
            "#,
            owner_id,
            tenant_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(folders)
    }

    pub async fn list_all_markdown_files(
        &self,
        owner_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Vec<File>> {
        let files = sqlx::query_as!(
            File,
            r#"
            SELECT id, name, path, size, mime_type, content_hash, owner_id, parent_folder_id, current_version, created_at, modified_at, starred_at, deleted_at, tenant_id
            FROM files
            WHERE owner_id = $1
              AND tenant_id = $2
              AND deleted_at IS NULL
              AND (mime_type = 'text/markdown' OR name ILIKE '%.md')
            ORDER BY modified_at DESC
            "#,
            owner_id,
            tenant_id,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(files)
    }

    /// Check if a user is a member of a group.
    pub async fn is_user_in_group(&self, user_id: Uuid, group_id: Uuid) -> Result<bool> {
        let exists = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM group_members
                WHERE group_id = $1 AND user_id = $2
            ) as "exists!"
            "#,
            group_id,
            user_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }

    // -----------------------------------------------------------------
    // Vault sync
    // -----------------------------------------------------------------

    /// Obsidian is a trademark of Dynalist Inc. RustShare is not affiliated with,
    /// endorsed by, or sponsored by Obsidian.
    /// Create a new vault.
    pub async fn create_vault(&self, vault: &Vault) -> sqlx::Result<Vault> {
        sqlx::query!(
            r#"
            INSERT INTO vaults (id, tenant_id, owner_user_id, name, adapter, root_path, write_policy, server_rev, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            vault.id,
            vault.tenant_id,
            vault.owner_user_id,
            vault.name,
            vault.adapter.to_string(),
            vault.root_path,
            vault.write_policy.to_string(),
            vault.server_rev,
            vault.created_at,
            vault.updated_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(vault.clone())
    }

    /// Update an existing vault row.
    pub async fn update_vault(&self, vault: &Vault) -> sqlx::Result<Vault> {
        sqlx::query!(
            r#"
            UPDATE vaults
            SET name = $1, adapter = $2, root_path = $3, write_policy = $4, server_rev = $5, updated_at = $6
            WHERE id = $7 AND tenant_id = $8
            "#,
            vault.name,
            vault.adapter.to_string(),
            vault.root_path,
            vault.write_policy.to_string(),
            vault.server_rev,
            vault.updated_at,
            vault.id,
            vault.tenant_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(vault.clone())
    }

    /// Update only the write policy for a vault, leaving server_rev untouched.
    pub async fn update_vault_write_policy(
        &self,
        vault_id: Uuid,
        tenant_id: Uuid,
        write_policy: &VaultWritePolicy,
        updated_at: DateTime<Utc>,
    ) -> sqlx::Result<Vault> {
        sqlx::query_as!(
            Vault,
            r#"
            UPDATE vaults
            SET write_policy = $1, updated_at = $2
            WHERE id = $3 AND tenant_id = $4
            RETURNING id, tenant_id, owner_user_id, name, adapter as "adapter: _", root_path, write_policy as "write_policy: _", server_rev, created_at, updated_at
            "#,
            write_policy.to_string(),
            updated_at,
            vault_id,
            tenant_id,
        )
        .fetch_one(&self.pool)
        .await
    }

    /// Find an existing WebUI device for a user/vault pair.
    pub async fn get_webui_device(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        vault_id: Uuid,
    ) -> sqlx::Result<Option<VaultDevice>> {
        sqlx::query_as!(
            VaultDevice,
            r#"
            SELECT id, tenant_id, user_id, vault_id, device_name, client_type, client_version, last_sync_rev, revoked_at, created_at, last_seen_at
            FROM vault_devices
            WHERE tenant_id = $1 AND user_id = $2 AND vault_id = $3 AND client_type = 'web_ui' AND revoked_at IS NULL
            "#,
            tenant_id,
            user_id,
            vault_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    /// Create a WebUI device row for a vault.
    pub async fn create_webui_device(&self, device: &VaultDevice) -> sqlx::Result<VaultDevice> {
        sqlx::query!(
            r#"
            INSERT INTO vault_devices (id, tenant_id, user_id, vault_id, device_name, client_type, client_version, last_sync_rev, revoked_at, created_at, last_seen_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT DO NOTHING
            "#,
            device.id,
            device.tenant_id,
            device.user_id,
            device.vault_id,
            device.device_name,
            device.client_type,
            device.client_version,
            device.last_sync_rev,
            device.revoked_at,
            device.created_at,
            device.last_seen_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(device.clone())
    }

    /// Get a vault by ID.
    pub async fn get_vault(&self, vault_id: Uuid, tenant_id: Uuid) -> sqlx::Result<Option<Vault>> {
        let vault = sqlx::query_as!(
            Vault,
            r#"
            SELECT id, tenant_id, owner_user_id, name, adapter as "adapter: _", root_path, write_policy as "write_policy: _", server_rev, created_at, updated_at
            FROM vaults
            WHERE id = $1 AND tenant_id = $2
            "#,
            vault_id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(vault)
    }

    /// List vaults for an owner within a tenant.
    pub async fn list_vaults(&self, tenant_id: Uuid, owner_id: Uuid) -> sqlx::Result<Vec<Vault>> {
        let vaults = sqlx::query_as!(
            Vault,
            r#"
            SELECT id, tenant_id, owner_user_id, name, adapter as "adapter: _", root_path, write_policy as "write_policy: _", server_rev, created_at, updated_at
            FROM vaults
            WHERE tenant_id = $1 AND owner_user_id = $2
            ORDER BY name ASC
            "#,
            tenant_id,
            owner_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(vaults)
    }

    /// Atomically increment the server revision of a vault.
    pub async fn increment_vault_rev(&self, vault_id: Uuid, tenant_id: Uuid) -> sqlx::Result<i64> {
        let row = sqlx::query!(
            r#"
            UPDATE vaults
            SET server_rev = server_rev + 1, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING server_rev
            "#,
            vault_id,
            tenant_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.server_rev)
    }

    /// Get a file by vault ID and relative path.
    pub async fn get_vault_file(
        &self,
        vault_id: Uuid,
        relative_path: &str,
        tenant_id: Uuid,
    ) -> sqlx::Result<Option<VaultFile>> {
        let file = sqlx::query_as!(
            VaultFile,
            r#"
            SELECT id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            FROM vault_files
            WHERE vault_id = $1 AND relative_path = $2 AND tenant_id = $3 AND deleted = false
            "#,
            vault_id,
            relative_path,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(file)
    }

    /// Get a file by vault ID and relative path, including tombstones.
    pub async fn get_vault_file_including_deleted(
        &self,
        vault_id: Uuid,
        relative_path: &str,
        tenant_id: Uuid,
    ) -> sqlx::Result<Option<VaultFile>> {
        let file = sqlx::query_as::<_, VaultFile>(
            r#"
            SELECT id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            FROM vault_files
            WHERE vault_id = $1 AND relative_path = $2 AND tenant_id = $3
            ORDER BY deleted ASC, server_rev DESC
            LIMIT 1
            "#,
        )
        .bind(vault_id)
        .bind(relative_path)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(file)
    }

    /// List all files in a vault.
    pub async fn list_vault_files(
        &self,
        vault_id: Uuid,
        tenant_id: Uuid,
        limit: Option<i64>,
    ) -> sqlx::Result<Vec<VaultFile>> {
        let limit = limit.unwrap_or(i64::MAX);
        let files = sqlx::query_as!(
            VaultFile,
            r#"
            SELECT id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            FROM vault_files
            WHERE vault_id = $1 AND tenant_id = $2
            ORDER BY relative_path ASC
            LIMIT $3
            "#,
            vault_id,
            tenant_id,
            limit
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(files)
    }

    /// Insert a new file in a vault.
    pub async fn insert_vault_file(&self, file: &VaultFile) -> sqlx::Result<VaultFile> {
        let row = sqlx::query!(
            r#"
            INSERT INTO vault_files (id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            "#,
            file.id,
            file.tenant_id,
            file.vault_id,
            file.relative_path,
            file.content_type,
            file.sha256,
            file.size,
            file.server_rev,
            file.mtime_client,
            file.mtime_server,
            file.deleted,
            file.deleted_at,
            file.last_writer_device_id,
            file.created_at,
            file.updated_at,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(VaultFile {
            id: row.id,
            tenant_id: row.tenant_id,
            vault_id: row.vault_id,
            relative_path: row.relative_path,
            content_type: row.content_type,
            sha256: row.sha256,
            size: row.size,
            server_rev: row.server_rev,
            mtime_client: row.mtime_client,
            mtime_server: row.mtime_server,
            deleted: row.deleted,
            deleted_at: row.deleted_at,
            last_writer_device_id: row.last_writer_device_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    /// Insert a new file atomically, incrementing the vault revision in the
    /// same transaction so the revision cannot leak on conflict.
    pub async fn insert_vault_file_atomic(&self, file: &VaultFile) -> sqlx::Result<VaultFile> {
        let mut tx = self.pool.begin().await?;

        let rev_row = sqlx::query!(
            r#"
            UPDATE vaults SET server_rev = server_rev + 1, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING server_rev
            "#,
            file.vault_id,
            file.tenant_id,
        )
        .fetch_one(&mut *tx)
        .await?;

        let row = sqlx::query!(
            r#"
            INSERT INTO vault_files (id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            "#,
            file.id,
            file.tenant_id,
            file.vault_id,
            file.relative_path,
            file.content_type,
            file.sha256,
            file.size,
            rev_row.server_rev,
            file.mtime_client,
            file.mtime_server,
            file.deleted,
            file.deleted_at,
            file.last_writer_device_id,
            file.created_at,
            file.updated_at,
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(VaultFile {
            id: row.id,
            tenant_id: row.tenant_id,
            vault_id: row.vault_id,
            relative_path: row.relative_path,
            content_type: row.content_type,
            sha256: row.sha256,
            size: row.size,
            server_rev: row.server_rev,
            mtime_client: row.mtime_client,
            mtime_server: row.mtime_server,
            deleted: row.deleted,
            deleted_at: row.deleted_at,
            last_writer_device_id: row.last_writer_device_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    /// Upsert a file in a vault.
    pub async fn upsert_vault_file(&self, file: &VaultFile) -> sqlx::Result<VaultFile> {
        let row = sqlx::query!(
            r#"
            INSERT INTO vault_files (id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT (vault_id, relative_path) WHERE deleted_at IS NULL DO UPDATE SET
                content_type = EXCLUDED.content_type,
                sha256 = EXCLUDED.sha256,
                size = EXCLUDED.size,
                server_rev = EXCLUDED.server_rev,
                mtime_client = EXCLUDED.mtime_client,
                mtime_server = EXCLUDED.mtime_server,
                deleted = EXCLUDED.deleted,
                deleted_at = EXCLUDED.deleted_at,
                last_writer_device_id = EXCLUDED.last_writer_device_id,
                updated_at = EXCLUDED.updated_at
            RETURNING id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            "#,
            file.id,
            file.tenant_id,
            file.vault_id,
            file.relative_path,
            file.content_type,
            file.sha256,
            file.size,
            file.server_rev,
            file.mtime_client,
            file.mtime_server,
            file.deleted,
            file.deleted_at,
            file.last_writer_device_id,
            file.created_at,
            file.updated_at,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(VaultFile {
            id: row.id,
            tenant_id: row.tenant_id,
            vault_id: row.vault_id,
            relative_path: row.relative_path,
            content_type: row.content_type,
            sha256: row.sha256,
            size: row.size,
            server_rev: row.server_rev,
            mtime_client: row.mtime_client,
            mtime_server: row.mtime_server,
            deleted: row.deleted,
            deleted_at: row.deleted_at,
            last_writer_device_id: row.last_writer_device_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    /// Update an existing vault file ONLY if its current server_rev matches base_rev.
    /// Returns true if updated, false if the revision didn't match (conflict).
    pub async fn update_vault_file_conditional(
        &self,
        file: &VaultFile,
        base_server_rev: i64,
    ) -> sqlx::Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE vault_files
            SET sha256 = $1, size = $2, server_rev = $3, mtime_server = NOW(),
                updated_at = NOW(), last_writer_device_id = $4, content_type = $5
            WHERE vault_id = $6 AND relative_path = $7 AND tenant_id = $8
              AND server_rev = $9 AND deleted = false
            "#,
            file.sha256,
            file.size,
            file.server_rev,
            file.last_writer_device_id,
            file.content_type,
            file.vault_id,
            file.relative_path,
            file.tenant_id,
            base_server_rev,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Atomically increment vault revision and update an existing file ONLY if
    /// its current server_rev matches base_server_rev.  Returns the updated
    /// file on success, or `None` if the revision did not match.
    pub async fn update_vault_file_conditional_atomic(
        &self,
        file: &VaultFile,
        base_server_rev: i64,
    ) -> sqlx::Result<Option<VaultFile>> {
        let mut tx = self.pool.begin().await?;

        let rev_row = sqlx::query!(
            r#"
            UPDATE vaults SET server_rev = server_rev + 1, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING server_rev
            "#,
            file.vault_id,
            file.tenant_id,
        )
        .fetch_one(&mut *tx)
        .await?;

        let row = sqlx::query!(
            r#"
            UPDATE vault_files
            SET sha256 = $1, size = $2, server_rev = $3, mtime_server = NOW(),
                updated_at = NOW(), last_writer_device_id = $4, content_type = $5
            WHERE vault_id = $6 AND relative_path = $7 AND tenant_id = $8
              AND server_rev = $9 AND deleted = false
            RETURNING id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            "#,
            file.sha256,
            file.size,
            rev_row.server_rev,
            file.last_writer_device_id,
            file.content_type,
            file.vault_id,
            file.relative_path,
            file.tenant_id,
            base_server_rev,
        )
        .fetch_optional(&mut *tx)
        .await?;

        match row {
            Some(r) => {
                tx.commit().await?;
                Ok(Some(VaultFile {
                    id: r.id,
                    tenant_id: r.tenant_id,
                    vault_id: r.vault_id,
                    relative_path: r.relative_path,
                    content_type: r.content_type,
                    sha256: r.sha256,
                    size: r.size,
                    server_rev: r.server_rev,
                    mtime_client: r.mtime_client,
                    mtime_server: r.mtime_server,
                    deleted: r.deleted,
                    deleted_at: r.deleted_at,
                    last_writer_device_id: r.last_writer_device_id,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                }))
            }
            None => {
                tx.rollback().await?;
                Ok(None)
            }
        }
    }

    /// Atomically increment vault revision and update an existing file ONLY if
    /// the vault's write_policy is `web_editing_enabled` and the file's current
    /// server_rev matches base_server_rev.
    pub async fn update_vault_file_conditional_atomic_for_webui(
        &self,
        file: &VaultFile,
        base_server_rev: i64,
    ) -> Result<Option<VaultFile>, VaultSyncError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| VaultSyncError::Database(e.to_string()))?;

        let rev_row = sqlx::query!(
            r#"
            UPDATE vaults SET server_rev = server_rev + 1, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND write_policy = 'web_editing_enabled'
            RETURNING server_rev
            "#,
            file.vault_id,
            file.tenant_id,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| VaultSyncError::Database(e.to_string()))?;

        let Some(rev_row) = rev_row else {
            tx.rollback()
                .await
                .map_err(|e| VaultSyncError::Database(e.to_string()))?;
            return Err(VaultSyncError::WritePolicyDenied {
                policy: "web_editing_enabled".to_string(),
            });
        };

        let row = sqlx::query!(
            r#"
            UPDATE vault_files
            SET sha256 = $1, size = $2, server_rev = $3, mtime_server = NOW(),
                updated_at = NOW(), last_writer_device_id = $4, content_type = $5
            WHERE vault_id = $6 AND relative_path = $7 AND tenant_id = $8
              AND server_rev = $9 AND deleted = false
            RETURNING id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            "#,
            file.sha256,
            file.size,
            rev_row.server_rev,
            file.last_writer_device_id,
            file.content_type,
            file.vault_id,
            file.relative_path,
            file.tenant_id,
            base_server_rev,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| VaultSyncError::Database(e.to_string()))?;

        match row {
            Some(r) => {
                tx.commit()
                    .await
                    .map_err(|e| VaultSyncError::Database(e.to_string()))?;
                Ok(Some(VaultFile {
                    id: r.id,
                    tenant_id: r.tenant_id,
                    vault_id: r.vault_id,
                    relative_path: r.relative_path,
                    content_type: r.content_type,
                    sha256: r.sha256,
                    size: r.size,
                    server_rev: r.server_rev,
                    mtime_client: r.mtime_client,
                    mtime_server: r.mtime_server,
                    deleted: r.deleted,
                    deleted_at: r.deleted_at,
                    last_writer_device_id: r.last_writer_device_id,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                }))
            }
            None => {
                tx.rollback()
                    .await
                    .map_err(|e| VaultSyncError::Database(e.to_string()))?;
                Ok(None)
            }
        }
    }

    /// Tombstone (soft-delete) a file in a vault.
    pub async fn tombstone_vault_file(
        &self,
        vault_id: Uuid,
        relative_path: &str,
        tenant_id: Uuid,
        new_rev: i64,
        device_id: &str,
    ) -> Result<VaultFile> {
        let row = sqlx::query!(
            r#"
            UPDATE vault_files
            SET deleted = true, deleted_at = NOW(), server_rev = $4, last_writer_device_id = $5, updated_at = NOW()
            WHERE vault_id = $1 AND relative_path = $2 AND tenant_id = $3
            RETURNING id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            "#,
            vault_id,
            relative_path,
            tenant_id,
            new_rev,
            device_id,
        )
        .fetch_optional(&self.pool)
        .await?;

        let row = match row {
            Some(r) => r,
            None => return Err(VaultFileStoreError::NotFound.into()),
        };

        Ok(VaultFile {
            id: row.id,
            tenant_id: row.tenant_id,
            vault_id: row.vault_id,
            relative_path: row.relative_path,
            content_type: row.content_type,
            sha256: row.sha256,
            size: row.size,
            server_rev: row.server_rev,
            mtime_client: row.mtime_client,
            mtime_server: row.mtime_server,
            deleted: row.deleted,
            deleted_at: row.deleted_at,
            last_writer_device_id: row.last_writer_device_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    /// Tombstone a vault file ONLY if its current server_rev matches base_rev.
    /// Returns true if tombstoned, false if revision didn't match.
    pub async fn tombstone_vault_file_conditional(
        &self,
        vault_id: Uuid,
        relative_path: &str,
        tenant_id: Uuid,
        base_server_rev: i64,
        new_rev: i64,
        device_id: &str,
    ) -> sqlx::Result<bool> {
        let result = sqlx::query!(
            r#"
            UPDATE vault_files
            SET deleted = true, deleted_at = NOW(), server_rev = $1,
                last_writer_device_id = $2, updated_at = NOW()
            WHERE vault_id = $3 AND relative_path = $4 AND tenant_id = $5
              AND server_rev = $6 AND deleted = false
            "#,
            new_rev,
            device_id,
            vault_id,
            relative_path,
            tenant_id,
            base_server_rev,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Atomically increment vault revision and tombstone a file ONLY if its
    /// current server_rev matches base_server_rev.  Returns the updated file
    /// on success, or `None` if the revision did not match.
    pub async fn tombstone_vault_file_conditional_atomic(
        &self,
        vault_id: Uuid,
        relative_path: &str,
        tenant_id: Uuid,
        base_server_rev: i64,
        device_id: &str,
    ) -> sqlx::Result<Option<VaultFile>> {
        let mut tx = self.pool.begin().await?;

        let rev_row = sqlx::query!(
            r#"
            UPDATE vaults SET server_rev = server_rev + 1, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING server_rev
            "#,
            vault_id,
            tenant_id,
        )
        .fetch_one(&mut *tx)
        .await?;

        let row = sqlx::query!(
            r#"
            UPDATE vault_files
            SET deleted = true, deleted_at = NOW(), server_rev = $1,
                last_writer_device_id = $2, updated_at = NOW()
            WHERE vault_id = $3 AND relative_path = $4 AND tenant_id = $5
              AND server_rev = $6 AND deleted = false
            RETURNING id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            "#,
            rev_row.server_rev,
            device_id,
            vault_id,
            relative_path,
            tenant_id,
            base_server_rev,
        )
        .fetch_optional(&mut *tx)
        .await?;

        match row {
            Some(r) => {
                tx.commit().await?;
                Ok(Some(VaultFile {
                    id: r.id,
                    tenant_id: r.tenant_id,
                    vault_id: r.vault_id,
                    relative_path: r.relative_path,
                    content_type: r.content_type,
                    sha256: r.sha256,
                    size: r.size,
                    server_rev: r.server_rev,
                    mtime_client: r.mtime_client,
                    mtime_server: r.mtime_server,
                    deleted: r.deleted,
                    deleted_at: r.deleted_at,
                    last_writer_device_id: r.last_writer_device_id,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                }))
            }
            None => {
                tx.rollback().await?;
                Ok(None)
            }
        }
    }

    /// Rename a file within a vault.
    pub async fn rename_vault_file(
        &self,
        vault_id: Uuid,
        old_path: &str,
        new_path: &str,
        tenant_id: Uuid,
        new_rev: i64,
        device_id: &str,
    ) -> Result<VaultFile> {
        let mut tx = self.pool.begin().await?;

        // Check if an active file already exists at the destination path
        let existing = sqlx::query!(
            r#"SELECT id FROM vault_files WHERE vault_id = $1 AND relative_path = $2 AND tenant_id = $3 AND deleted = false"#,
            vault_id,
            new_path,
            tenant_id
        )
        .fetch_optional(&mut *tx)
        .await?;

        if existing.is_some() {
            tx.rollback().await?;
            return Err(VaultFileStoreError::DestinationExists.into());
        }

        // Update the old row to the new path
        let row = sqlx::query!(
            r#"
            UPDATE vault_files
            SET relative_path = $4, server_rev = $5, last_writer_device_id = $6, updated_at = NOW()
            WHERE vault_id = $1 AND relative_path = $2 AND tenant_id = $3
            RETURNING id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            "#,
            vault_id,
            old_path,
            tenant_id,
            new_path,
            new_rev,
            device_id,
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(VaultFile {
            id: row.id,
            tenant_id: row.tenant_id,
            vault_id: row.vault_id,
            relative_path: row.relative_path,
            content_type: row.content_type,
            sha256: row.sha256,
            size: row.size,
            server_rev: row.server_rev,
            mtime_client: row.mtime_client,
            mtime_server: row.mtime_server,
            deleted: row.deleted,
            deleted_at: row.deleted_at,
            last_writer_device_id: row.last_writer_device_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    /// Rename a vault file ONLY if its current server_rev matches base_rev.
    /// Returns `true` if renamed, `false` if revision didn't match.
    /// Returns `Err(VaultFileStoreError::DestinationExists)` if destination already exists.
    #[allow(clippy::too_many_arguments)]
    pub async fn rename_vault_file_conditional(
        &self,
        vault_id: Uuid,
        old_path: &str,
        new_path: &str,
        tenant_id: Uuid,
        base_server_rev: i64,
        new_rev: i64,
        device_id: &str,
    ) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;

        // First check destination does NOT exist (active file)
        let existing = sqlx::query!(
            r#"SELECT 1 as one FROM vault_files WHERE vault_id = $1 AND relative_path = $2 AND tenant_id = $3 AND deleted = false"#,
            vault_id,
            new_path,
            tenant_id,
        )
        .fetch_optional(&mut *tx)
        .await?;
        if existing.is_some() {
            tx.rollback().await?;
            return Err(VaultFileStoreError::DestinationExists.into());
        }

        let result = sqlx::query!(
            r#"
            UPDATE vault_files
            SET relative_path = $1, server_rev = $2, last_writer_device_id = $3, updated_at = NOW()
            WHERE vault_id = $4 AND relative_path = $5 AND tenant_id = $6
              AND server_rev = $7 AND deleted = false
            "#,
            new_path,
            new_rev,
            device_id,
            vault_id,
            old_path,
            tenant_id,
            base_server_rev,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    /// Atomically increment vault revision and rename a file ONLY if its
    /// current server_rev matches base_server_rev.  Returns the updated file
    /// on success, or `None` if the revision did not match.
    /// Returns `Err(VaultFileStoreError::DestinationExists)` if destination
    /// already occupied by an active file.
    pub async fn rename_vault_file_conditional_atomic(
        &self,
        vault_id: Uuid,
        old_path: &str,
        new_path: &str,
        tenant_id: Uuid,
        base_server_rev: i64,
        device_id: &str,
    ) -> anyhow::Result<Option<VaultFile>> {
        let mut tx = self.pool.begin().await?;

        let existing = sqlx::query!(
            r#"SELECT 1 as one FROM vault_files WHERE vault_id = $1 AND relative_path = $2 AND tenant_id = $3 AND deleted = false"#,
            vault_id,
            new_path,
            tenant_id,
        )
        .fetch_optional(&mut *tx)
        .await?;
        if existing.is_some() {
            tx.rollback().await?;
            return Err(VaultFileStoreError::DestinationExists.into());
        }

        let rev_row = sqlx::query!(
            r#"
            UPDATE vaults SET server_rev = server_rev + 1, updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING server_rev
            "#,
            vault_id,
            tenant_id,
        )
        .fetch_one(&mut *tx)
        .await?;

        let row = sqlx::query!(
            r#"
            UPDATE vault_files
            SET relative_path = $1, server_rev = $2, last_writer_device_id = $3, updated_at = NOW()
            WHERE vault_id = $4 AND relative_path = $5 AND tenant_id = $6
              AND server_rev = $7 AND deleted = false
            RETURNING id, tenant_id, vault_id, relative_path, content_type, sha256, size, server_rev, mtime_client, mtime_server, deleted, deleted_at, last_writer_device_id, created_at, updated_at
            "#,
            new_path,
            rev_row.server_rev,
            device_id,
            vault_id,
            old_path,
            tenant_id,
            base_server_rev,
        )
        .fetch_optional(&mut *tx)
        .await?;

        match row {
            Some(r) => {
                tx.commit().await?;
                Ok(Some(VaultFile {
                    id: r.id,
                    tenant_id: r.tenant_id,
                    vault_id: r.vault_id,
                    relative_path: r.relative_path,
                    content_type: r.content_type,
                    sha256: r.sha256,
                    size: r.size,
                    server_rev: r.server_rev,
                    mtime_client: r.mtime_client,
                    mtime_server: r.mtime_server,
                    deleted: r.deleted,
                    deleted_at: r.deleted_at,
                    last_writer_device_id: r.last_writer_device_id,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                }))
            }
            None => {
                tx.rollback().await?;
                Ok(None)
            }
        }
    }

    /// Register a new device for vault sync.
    pub async fn create_vault_device(&self, device: &VaultDevice) -> sqlx::Result<VaultDevice> {
        sqlx::query!(
            r#"
            INSERT INTO vault_devices (id, tenant_id, user_id, vault_id, device_name, client_type, client_version, last_sync_rev, revoked_at, created_at, last_seen_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            device.id,
            device.tenant_id,
            device.user_id,
            device.vault_id,
            device.device_name,
            device.client_type,
            device.client_version,
            device.last_sync_rev,
            device.revoked_at,
            device.created_at,
            device.last_seen_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(device.clone())
    }

    /// Get a device by its ID string.
    pub async fn get_vault_device(
        &self,
        device_id: &str,
        tenant_id: Uuid,
    ) -> sqlx::Result<Option<VaultDevice>> {
        let id = Uuid::parse_str(device_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let device = sqlx::query_as!(
            VaultDevice,
            r#"
            SELECT id, tenant_id, user_id, vault_id, device_name, client_type, client_version, last_sync_rev, revoked_at, created_at, last_seen_at
            FROM vault_devices
            WHERE id = $1 AND tenant_id = $2
            "#,
            id,
            tenant_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(device)
    }

    /// Bind a device to a vault.
    pub async fn bind_vault_device_to_vault(
        &self,
        device_id: &str,
        tenant_id: Uuid,
        vault_id: Uuid,
    ) -> sqlx::Result<VaultDevice> {
        let id = Uuid::parse_str(device_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let device = sqlx::query_as::<_, VaultDevice>(
            r#"
            UPDATE vault_devices
            SET vault_id = $3, last_seen_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND revoked_at IS NULL AND (vault_id IS NULL OR vault_id = $3)
            RETURNING id, tenant_id, user_id, vault_id, device_name, client_type, client_version, last_sync_rev, revoked_at, created_at, last_seen_at
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(vault_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(device)
    }

    /// Revoke a device.
    pub async fn revoke_vault_device(&self, device_id: Uuid, tenant_id: Uuid) -> sqlx::Result<()> {
        let result = sqlx::query!(
            r#"
            UPDATE vault_devices
            SET revoked_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            "#,
            device_id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    /// Update the last_seen_at timestamp for a device.
    pub async fn update_vault_device_last_seen(
        &self,
        device_id: &str,
        tenant_id: Uuid,
    ) -> sqlx::Result<()> {
        let id = Uuid::parse_str(device_id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let result = sqlx::query!(
            r#"
            UPDATE vault_devices
            SET last_seen_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND revoked_at IS NULL
            "#,
            id,
            tenant_id
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    /// Update the last_seen_at timestamp of a vault device to an explicit value.
    pub async fn update_vault_device_last_seen_at(
        &self,
        device_id: Uuid,
        last_seen_at: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE vault_devices
            SET last_seen_at = $1
            WHERE id = $2
            "#,
            last_seen_at,
            device_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustshare_core::domain::{File, FileVersion, Folder, Share, SharePermissions, User};

    const TEST_DATABASE_URL: &str = "postgres://rustshare:changeme@localhost:5432/rustshare";

    async fn setup_test_db() -> PgPool {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| TEST_DATABASE_URL.to_string());
        PgPool::connect(&database_url).await.unwrap()
    }

    async fn setup_metadata_store() -> (MetadataStore, PgPool) {
        let pool = setup_test_db().await;
        let store = MetadataStore::new(pool.clone());
        (store, pool)
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_create_and_find_user() {
        let pool = setup_test_db().await;
        let store = MetadataStore::new(pool.clone());
        let tenant_id = Uuid::new_v4();

        let user = User::new(
            "testuser".to_string(),
            "Test User".to_string(),
            "hash123".to_string(),
            "test@example.com".to_string(),
            false,
            10_737_418_240, // 10GB
            tenant_id,
        );

        store.create_user(&user).await.unwrap();

        let found = store.find_user_by_email("test@example.com").await.unwrap();
        assert!(found.is_some());
        let found_user = found.unwrap();
        assert_eq!(found_user.email, "test@example.com");
        assert_eq!(found_user.username, "testuser");

        // Cleanup
        sqlx::query("DELETE FROM users WHERE email = $1")
            .bind("test@example.com")
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_file_crud() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();

        // First create a user to own the file
        let owner = User::new(
            "fileowner".to_string(),
            "File Owner".to_string(),
            "hash456".to_string(),
            "fileowner@example.com".to_string(),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&owner).await.unwrap();

        // Create a file
        let file = File::new(
            "test-document.pdf".to_string(),
            "/Documents/test-document.pdf".to_string(),
            "abc123def456hash".to_string(),
            2048,
            "application/pdf".to_string(),
            None, // No parent folder
            owner.id,
            tenant_id,
        );

        // Test: create_file
        store.create_file(&file).await.unwrap();

        // Test: find_file_by_id
        let found = store.find_file_by_id(file.id, owner.id).await.unwrap();
        assert!(found.is_some());
        let found_file = found.unwrap();
        assert_eq!(found_file.id, file.id);
        assert_eq!(found_file.name, "test-document.pdf");
        assert_eq!(found_file.path, "/Documents/test-document.pdf");
        assert_eq!(found_file.content_hash, "abc123def456hash");
        assert_eq!(found_file.size, 2048);
        assert_eq!(found_file.mime_type, "application/pdf");
        assert_eq!(found_file.owner_id, owner.id);
        assert_eq!(found_file.current_version, 1);

        // Test: update_file (modify name and size)
        let mut updated_file = found_file.clone();
        updated_file.name = "renamed-document.pdf".to_string();
        updated_file.size = 4096;
        store.update_file(&updated_file).await.unwrap();

        let found_updated = store
            .find_file_by_id(file.id, owner.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_updated.name, "renamed-document.pdf");
        assert_eq!(found_updated.size, 4096);

        // Test: list_files (with no parent_id filter)
        let files = store.list_files(None, owner.id, tenant_id).await.unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().any(|f| f.id == file.id));

        // Test: delete_file
        store.delete_file(file.id, owner.id).await.unwrap();
        let not_found = store.find_file_by_id(file.id, owner.id).await.unwrap();
        assert!(not_found.is_none());

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_file_versions() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();

        // First create a user to own the file
        let user = User::new(
            "versionuser".to_string(),
            "Version User".to_string(),
            "hash789".to_string(),
            "versionuser@example.com".to_string(),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&user).await.unwrap();

        // Create a file
        let file = File::new(
            "versioned-doc.txt".to_string(),
            "/Documents/versioned-doc.txt".to_string(),
            "hash1".to_string(),
            100,
            "text/plain".to_string(),
            None,
            user.id,
            tenant_id,
        );
        store.create_file(&file).await.unwrap();

        // Create file version 1
        let version1 = FileVersion::new(
            file.id,
            1,
            "hash1".to_string(),
            100,
            user.id,
            Some("Initial version".to_string()),
            tenant_id,
        );
        store.create_file_version(&version1).await.unwrap();

        // Create file version 2
        let version2 = FileVersion::new(
            file.id,
            2,
            "hash2".to_string(),
            200,
            user.id,
            Some("Second version".to_string()),
            tenant_id,
        );
        store.create_file_version(&version2).await.unwrap();

        // Create file version 3
        let version3 = FileVersion::new(
            file.id,
            3,
            "hash3".to_string(),
            300,
            user.id,
            None,
            tenant_id,
        );
        store.create_file_version(&version3).await.unwrap();

        // Test: list_file_versions (should be in DESC order: 3, 2, 1)
        let versions = store.list_file_versions(file.id, user.id).await.unwrap();
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version_number, 3);
        assert_eq!(versions[1].version_number, 2);
        assert_eq!(versions[2].version_number, 1);
        assert_eq!(versions[0].content_hash, "hash3");
        assert_eq!(versions[1].content_hash, "hash2");
        assert_eq!(versions[2].content_hash, "hash1");

        // Test: find_file_version (find version 2)
        let found_version = store.find_file_version(file.id, 2, user.id).await.unwrap();
        assert!(found_version.is_some());
        let found = found_version.unwrap();
        assert_eq!(found.version_number, 2);
        assert_eq!(found.content_hash, "hash2");
        assert_eq!(found.size, 200);
        assert_eq!(found.created_by, user.id);
        assert_eq!(found.change_description, Some("Second version".to_string()));

        // Test: find_file_version (non-existent version)
        let not_found = store.find_file_version(file.id, 99, user.id).await.unwrap();
        assert!(not_found.is_none());

        // Cleanup (file_versions will cascade delete with file)
        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_folder_crud() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();

        // First create a user to own the folders
        let owner = User::new(
            "folderowner".to_string(),
            "Folder Owner".to_string(),
            "hashabc".to_string(),
            "folderowner@example.com".to_string(),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&owner).await.unwrap();

        // Test: create_folder (root folder)
        let root_folder = Folder::new_root(owner.id, tenant_id);
        store.create_folder(&root_folder).await.unwrap();

        // Test: find_folder_by_id
        let found = store
            .find_folder_by_id(root_folder.id, owner.id)
            .await
            .unwrap();
        assert!(found.is_some());
        let found_folder = found.unwrap();
        assert_eq!(found_folder.id, root_folder.id);
        assert_eq!(found_folder.name, "Root");
        assert_eq!(found_folder.path, "/Root");
        assert_eq!(found_folder.parent_folder_id, None);
        assert_eq!(found_folder.owner_id, owner.id);

        // Test: create_folder (child folder - Documents)
        let docs_folder = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            root_folder.id,
            owner.id,
            tenant_id,
        );
        store.create_folder(&docs_folder).await.unwrap();

        // Test: create_folder (child folder - Photos)
        let photos_folder = Folder::new_child(
            "Photos".to_string(),
            "/Photos".to_string(),
            root_folder.id,
            owner.id,
            tenant_id,
        );
        store.create_folder(&photos_folder).await.unwrap();

        // Test: create_folder (nested folder - Documents/Work)
        let work_folder = Folder::new_child(
            "Work".to_string(),
            "/Documents/Work".to_string(),
            docs_folder.id,
            owner.id,
            tenant_id,
        );
        store.create_folder(&work_folder).await.unwrap();

        // Test: create_folder (deeply nested folder - Documents/Work/Projects)
        let projects_folder = Folder::new_child(
            "Projects".to_string(),
            "/Documents/Work/Projects".to_string(),
            work_folder.id,
            owner.id,
            tenant_id,
        );
        store.create_folder(&projects_folder).await.unwrap();

        // Test: list_folders (root level - should return Documents and Photos)
        let root_children = store
            .list_folders(Some(root_folder.id), owner.id, tenant_id)
            .await
            .unwrap();
        assert_eq!(root_children.len(), 2);
        assert!(root_children.iter().any(|f| f.name == "Documents"));
        assert!(root_children.iter().any(|f| f.name == "Photos"));

        // Test: list_folders (Documents children - should return Work)
        let docs_children = store
            .list_folders(Some(docs_folder.id), owner.id, tenant_id)
            .await
            .unwrap();
        assert_eq!(docs_children.len(), 1);
        assert_eq!(docs_children[0].name, "Work");

        // Test: list_folders (no parent - should return root folder)
        let root_folders = store.list_folders(None, owner.id, tenant_id).await.unwrap();
        assert_eq!(root_folders.len(), 1);
        assert_eq!(root_folders[0].name, "Root");

        // Test: find_descendant_folders (should find all descendants of Documents)
        let descendants = store
            .find_descendant_folders(docs_folder.id, owner.id)
            .await
            .unwrap();
        // Should include: Documents, Work, Projects (3 folders)
        assert_eq!(descendants.len(), 3);
        assert!(descendants.iter().any(|f| f.name == "Documents"));
        assert!(descendants.iter().any(|f| f.name == "Work"));
        assert!(descendants.iter().any(|f| f.name == "Projects"));

        // Test: find_descendant_folders (leaf folder should only return itself)
        let leaf_descendants = store
            .find_descendant_folders(projects_folder.id, owner.id)
            .await
            .unwrap();
        assert_eq!(leaf_descendants.len(), 1);
        assert_eq!(leaf_descendants[0].name, "Projects");

        // Test: update_folder (rename Photos to Pictures)
        let mut updated_photos = photos_folder.clone();
        updated_photos.name = "Pictures".to_string();
        updated_photos.path = "/Pictures".to_string();
        updated_photos.updated_at = chrono::Utc::now();
        store.update_folder(&updated_photos).await.unwrap();

        let found_updated = store
            .find_folder_by_id(photos_folder.id, owner.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found_updated.name, "Pictures");
        assert_eq!(found_updated.path, "/Pictures");

        // Test: delete_folder (delete leaf folder first)
        store
            .delete_folder(projects_folder.id, owner.id)
            .await
            .unwrap();
        let not_found = store
            .find_folder_by_id(projects_folder.id, owner.id)
            .await
            .unwrap();
        assert!(not_found.is_none());

        // Verify descendants updated after deletion
        let updated_descendants = store
            .find_descendant_folders(docs_folder.id, owner.id)
            .await
            .unwrap();
        assert_eq!(updated_descendants.len(), 2); // Only Documents and Work remain
        assert!(!updated_descendants.iter().any(|f| f.name == "Projects"));

        // Cleanup: Delete folders (cascade will handle children)
        // Delete in order: leaf -> parent
        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(work_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(docs_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(photos_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM folders WHERE id = $1")
            .bind(root_folder.id)
            .execute(&pool)
            .await
            .unwrap();

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_share_crud() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();

        // First create a user to own the file
        let owner = User::new(
            "shareowner".to_string(),
            "Share Owner".to_string(),
            "hashxyz".to_string(),
            "shareowner@example.com".to_string(),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&owner).await.unwrap();

        // Create a file to share
        let file = File::new(
            "shareable-document.pdf".to_string(),
            "/Documents/shareable-document.pdf".to_string(),
            "abcdef123456hash".to_string(),
            3072,
            "application/pdf".to_string(),
            None,
            owner.id,
            tenant_id,
        );
        store.create_file(&file).await.unwrap();

        // Test: create_share
        let share_token = Uuid::new_v4().to_string();
        let share = Share::new(
            file.id,
            share_token.clone(),
            owner.id,
            SharePermissions::View,
            Some("hashed_password".to_string()),
            None,
            tenant_id,
        );
        store.create_share(&share).await.unwrap();

        // Test: get_share_by_token
        let found_by_token = store
            .get_share_by_token(&share_token, tenant_id)
            .await
            .unwrap();
        assert!(found_by_token.is_some());
        let found_share = found_by_token.unwrap();
        assert_eq!(found_share.id, share.id);
        assert_eq!(found_share.share_token, Some(share_token.clone()));
        assert_eq!(found_share.file_id, Some(file.id));
        assert_eq!(found_share.permissions, SharePermissions::View);
        assert_eq!(
            found_share.password_hash,
            Some("hashed_password".to_string())
        );
        assert_eq!(found_share.access_count, 0);

        // Test: get_share
        let found_by_id = store.get_share(share.id, owner.id).await.unwrap();
        assert!(found_by_id.is_some());
        let found_share_by_id = found_by_id.unwrap();
        assert_eq!(found_share_by_id.id, share.id);
        assert_eq!(found_share_by_id.share_token, Some(share_token.clone()));

        // Create a second share for the same file
        let share_token_2 = Uuid::new_v4().to_string();
        let share2 = Share::new(
            file.id,
            share_token_2.clone(),
            owner.id,
            SharePermissions::Edit,
            None,
            None,
            tenant_id,
        );
        store.create_share(&share2).await.unwrap();

        // Test: get_file_shares
        let file_shares = store.get_file_shares(file.id).await.unwrap();
        assert_eq!(file_shares.len(), 2);
        assert!(file_shares
            .iter()
            .any(|s| s.share_token == Some(share_token.clone())));
        assert!(file_shares
            .iter()
            .any(|s| s.share_token == Some(share_token_2.clone())));

        // Test: increment_share_access
        store.increment_share_access(share.id).await.unwrap();
        let updated = store.get_share(share.id, owner.id).await.unwrap().unwrap();
        assert_eq!(updated.access_count, 1);

        // Test: log_share_access
        store
            .log_share_access(ShareAccessLogEntry {
                share_id: share.id,
                ip_address: Some("192.168.1.1".to_string()),
                user_agent: Some("Mozilla/5.0".to_string()),
                action: "access".to_string(),
                success: true,
                actor_type: Some("public_share_session".to_string()),
                actor_label: Some("Uploader".to_string()),
                share_session_id: Some(Uuid::new_v4()),
                share_session_subject: Some("share:test".to_string()),
            })
            .await
            .unwrap();

        // Test: update_share
        let mut updated_share = found_share.clone();
        updated_share.password_hash = Some("new_hashed_password".to_string());
        store.update_share(&updated_share).await.unwrap();

        let after_update = store.get_share(share.id, owner.id).await.unwrap().unwrap();
        assert_eq!(
            after_update.password_hash,
            Some("new_hashed_password".to_string())
        );

        // Test: revoke_share
        store.revoke_share(share.id, owner.id).await.unwrap();

        // After revoke, share should not appear in get_file_shares (only active shares)
        let active_shares = store.get_file_shares(file.id).await.unwrap();
        assert_eq!(active_shares.len(), 1);
        assert!(active_shares
            .iter()
            .all(|s| s.share_token == Some(share_token_2.clone())));

        // But should still be retrievable by ID
        let revoked_share = store.get_share(share.id, owner.id).await.unwrap();
        assert!(revoked_share.is_some());

        // Cleanup
        sqlx::query("DELETE FROM shares WHERE file_id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();

        // Cleanup user
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires database
    async fn test_get_public_shares_excludes_group_shares() {
        let (store, pool) = setup_metadata_store().await;

        // Create test user and group
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();

        // Create user
        let owner = User::new(
            format!("testowner_{}", user_id),
            "Test Owner".to_string(),
            "hash123".to_string(),
            format!("testowner_{}@example.com", user_id),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&owner).await.unwrap();

        // Create file
        let file = File::new(
            "test-document.pdf".to_string(),
            "/Documents/test-document.pdf".to_string(),
            "content_hash".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            owner.id,
            tenant_id,
        );
        store.create_file(&file).await.unwrap();

        // Create public share
        let public_share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file.id),
            folder_id: None,
            share_token: Some(Uuid::new_v4().to_string()),
            permissions: SharePermissions::View,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: None,
            created_by: owner.id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id,
        };
        store.create_share(&public_share).await.unwrap();

        // Create backing group row for the FK used by recipient_group_id.
        let group_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO user_groups (id, name, description, created_by)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(group_id)
        .bind(format!("test-group-{}", group_id))
        .bind(Some("Test group".to_string()))
        .bind(owner.id)
        .execute(&pool)
        .await
        .unwrap();

        // Create group share (same file)
        let group_share = Share {
            id: Uuid::new_v4(),
            file_id: Some(file.id),
            folder_id: None,
            share_token: None,
            permissions: SharePermissions::Edit,
            password_hash: None,
            expires_at: None,
            upload_only: false,
            access_count: 0,
            recipient_user_id: None,
            recipient_group_id: Some(group_id),
            created_by: owner.id,
            created_at: Utc::now(),
            revoked_at: None,
            tenant_id,
        };
        store.create_share(&group_share).await.unwrap();

        // Query public shares
        let public_shares = store.get_user_public_shares(owner.id).await.unwrap();

        // Should only return 1 (the public share), not 2
        assert_eq!(public_shares.len(), 1);
        assert_eq!(public_shares[0].share.id, public_share.id);

        // Cleanup
        sqlx::query("DELETE FROM shares WHERE file_id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM user_groups WHERE id = $1")
            .bind(group_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_cross_user_file_isolation() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();
        let suffix = Uuid::new_v4().to_string();

        let user_a = User::new(
            format!("user_a_{}", suffix),
            "User A".to_string(),
            "hash_a".to_string(),
            format!("user_a_{}@example.com", suffix),
            false,
            10_737_418_240,
            tenant_id,
        );
        let user_b = User::new(
            format!("user_b_{}", suffix),
            "User B".to_string(),
            "hash_b".to_string(),
            format!("user_b_{}@example.com", suffix),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&user_a).await.unwrap();
        store.create_user(&user_b).await.unwrap();

        let file = File::new(
            "secret.pdf".to_string(),
            "/secret.pdf".to_string(),
            "hash".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            user_a.id,
            tenant_id,
        );
        store.create_file(&file).await.unwrap();

        // User B should NOT be able to find User A's file
        let found = store.find_file_by_id(file.id, user_b.id).await.unwrap();
        assert!(found.is_none(), "User B should not find User A's file");

        // User B should NOT be able to delete User A's file
        let result = store.delete_file(file.id, user_b.id).await;
        assert!(
            result.is_ok(),
            "delete_file should not error for non-existent (to B) file"
        );
        // File should still exist for User A
        let still_exists = store.find_file_by_id(file.id, user_a.id).await.unwrap();
        assert!(still_exists.is_some(), "User A's file should still exist");

        // Cleanup
        store.delete_file(file.id, user_a.id).await.unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
            .bind(user_a.id)
            .bind(user_b.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_cross_user_folder_isolation() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();
        let suffix = Uuid::new_v4().to_string();

        let user_a = User::new(
            format!("user_a_{}", suffix),
            "User A".to_string(),
            "hash_a".to_string(),
            format!("user_a_{}@example.com", suffix),
            false,
            10_737_418_240,
            tenant_id,
        );
        let user_b = User::new(
            format!("user_b_{}", suffix),
            "User B".to_string(),
            "hash_b".to_string(),
            format!("user_b_{}@example.com", suffix),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&user_a).await.unwrap();
        store.create_user(&user_b).await.unwrap();

        let folder = Folder::new_root(user_a.id, tenant_id);
        store.create_folder(&folder).await.unwrap();

        // User B should NOT be able to find User A's folder
        let found = store.find_folder_by_id(folder.id, user_b.id).await.unwrap();
        assert!(found.is_none(), "User B should not find User A's folder");

        // User B should NOT be able to delete User A's folder
        let result = store.delete_folder(folder.id, user_b.id).await;
        assert!(
            result.is_ok(),
            "delete_folder should not error for non-existent (to B) folder"
        );
        // Folder should still exist for User A
        let still_exists = store.find_folder_by_id(folder.id, user_a.id).await.unwrap();
        assert!(still_exists.is_some(), "User A's folder should still exist");

        // Cleanup
        store.delete_folder(folder.id, user_a.id).await.unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
            .bind(user_a.id)
            .bind(user_b.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_cross_user_share_isolation() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();
        let suffix = Uuid::new_v4().to_string();

        let user_a = User::new(
            format!("user_a_{}", suffix),
            "User A".to_string(),
            "hash_a".to_string(),
            format!("user_a_{}@example.com", suffix),
            false,
            10_737_418_240,
            tenant_id,
        );
        let user_b = User::new(
            format!("user_b_{}", suffix),
            "User B".to_string(),
            "hash_b".to_string(),
            format!("user_b_{}@example.com", suffix),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&user_a).await.unwrap();
        store.create_user(&user_b).await.unwrap();

        let file = File::new(
            "shared.pdf".to_string(),
            "/shared.pdf".to_string(),
            "hash".to_string(),
            1024,
            "application/pdf".to_string(),
            None,
            user_a.id,
            tenant_id,
        );
        store.create_file(&file).await.unwrap();

        // Create a public link share (no recipient) so User B has no access
        let share = Share::new(
            file.id,
            "token".to_string(),
            user_a.id,
            rustshare_core::domain::SharePermissions::View,
            None,
            None,
            tenant_id,
        );
        store.create_share(&share).await.unwrap();

        // User B should NOT be able to get User A's share by ID
        let found = store.get_share(share.id, user_b.id).await.unwrap();
        assert!(
            found.is_none(),
            "User B should not get User A's share by ID"
        );

        // User B should NOT be able to revoke User A's share
        let result = store.revoke_share(share.id, user_b.id).await;
        assert!(
            result.is_ok(),
            "revoke_share should not error for non-existent (to B) share"
        );
        // Share should still exist
        let still_exists = store.get_share(share.id, user_a.id).await.unwrap();
        assert!(still_exists.is_some(), "User A's share should still exist");

        // Cleanup
        sqlx::query("DELETE FROM shares WHERE id = $1")
            .bind(share.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM files WHERE id = $1")
            .bind(file.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
            .bind(user_a.id)
            .bind(user_b.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_find_files_in_folders() {
        let (store, pool) = setup_metadata_store().await;
        let tenant_id = Uuid::new_v4();

        let owner = User::new(
            "filefinder".to_string(),
            "File Finder".to_string(),
            "hash".to_string(),
            "filefinder@example.com".to_string(),
            false,
            10_737_418_240,
            tenant_id,
        );
        store.create_user(&owner).await.unwrap();

        // Create folder hierarchy: Root > Documents > Work
        let root = Folder::new_root(owner.id, tenant_id);
        store.create_folder(&root).await.unwrap();

        let docs = Folder::new_child(
            "Documents".to_string(),
            "/Documents".to_string(),
            root.id,
            owner.id,
            tenant_id,
        );
        store.create_folder(&docs).await.unwrap();

        let work = Folder::new_child(
            "Work".to_string(),
            "/Documents/Work".to_string(),
            docs.id,
            owner.id,
            tenant_id,
        );
        store.create_folder(&work).await.unwrap();

        // Create files in different folders
        let file_in_docs = File::new(
            "report.pdf".to_string(),
            "/Documents/report.pdf".to_string(),
            "hash1".to_string(),
            1024,
            "application/pdf".to_string(),
            Some(docs.id),
            owner.id,
            tenant_id,
        );
        store.create_file(&file_in_docs).await.unwrap();

        let file_in_work = File::new(
            "notes.txt".to_string(),
            "/Documents/Work/notes.txt".to_string(),
            "hash2".to_string(),
            512,
            "text/plain".to_string(),
            Some(work.id),
            owner.id,
            tenant_id,
        );
        store.create_file(&file_in_work).await.unwrap();

        let file_in_root = File::new(
            "rootfile.txt".to_string(),
            "/rootfile.txt".to_string(),
            "hash3".to_string(),
            256,
            "text/plain".to_string(),
            Some(root.id),
            owner.id,
            tenant_id,
        );
        store.create_file(&file_in_root).await.unwrap();

        // Query files in Documents and Work
        let folder_ids = vec![docs.id, work.id];
        let files = store
            .find_files_in_folders(&folder_ids, owner.id, tenant_id)
            .await
            .unwrap();

        assert_eq!(files.len(), 2, "Should find 2 files in Documents + Work");
        assert!(files.iter().any(|f| f.name == "report.pdf"));
        assert!(files.iter().any(|f| f.name == "notes.txt"));

        // Query files in root only
        let root_files = store
            .find_files_in_folders(&[root.id], owner.id, tenant_id)
            .await
            .unwrap();
        assert_eq!(root_files.len(), 1);
        assert_eq!(root_files[0].name, "rootfile.txt");

        // Query empty folder list
        let empty = store
            .find_files_in_folders(&[], owner.id, tenant_id)
            .await
            .unwrap();
        assert!(empty.is_empty());

        // Cleanup
        sqlx::query("DELETE FROM files WHERE owner_id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM folders WHERE owner_id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL
    async fn test_record_login_failure_concurrent() {
        let (store, pool) = setup_metadata_store().await;
        let ip = "192.168.1.1";

        // Ensure login protection is enabled and max attempts is high enough
        // so the concurrent tasks don't trigger a block before reaching 10.
        store
            .update_security_config(Some(true), Some(100), None)
            .await
            .unwrap();

        // Clean up any existing attempts for this IP
        sqlx::query("DELETE FROM login_attempts WHERE ip_address = $1")
            .bind(ip)
            .execute(&pool)
            .await
            .unwrap();

        // Spawn 10 concurrent tasks, each recording a login failure
        let mut handles = Vec::new();
        for _ in 0..10 {
            let store = store.clone();
            let ip = ip.to_string();
            handles.push(tokio::spawn(async move {
                store.record_login_failure(&ip).await.unwrap();
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Verify the final failed_count is exactly 10
        let row = sqlx::query("SELECT failed_count FROM login_attempts WHERE ip_address = $1")
            .bind(ip)
            .fetch_one(&pool)
            .await
            .unwrap();

        let failed_count: i32 = row.try_get("failed_count").unwrap();
        assert_eq!(
            failed_count, 10,
            "Concurrent login failures should sum to exactly 10"
        );

        // Clean up
        sqlx::query("DELETE FROM login_attempts WHERE ip_address = $1")
            .bind(ip)
            .execute(&pool)
            .await
            .unwrap();

        // Restore default security config
        store
            .update_security_config(Some(true), Some(5), Some(15))
            .await
            .unwrap();
    }
}

// Service-layer metadata-store trait bridges.
// These live next to the concrete type so the storage crate root stays small.
#[allow(async_fn_in_trait)]
impl rustshare_core::services::FileMetadataStoreOps for MetadataStore {
    type Tx = sqlx::Transaction<'static, sqlx::Postgres>;

    async fn create_file(&self, file: &rustshare_core::domain::File) -> anyhow::Result<()> {
        self.create_file(file).await
    }

    async fn create_file_in_tx(
        &self,
        tx: &mut Self::Tx,
        file: &rustshare_core::domain::File,
    ) -> anyhow::Result<()> {
        self.create_file_in_tx(tx, file).await
    }

    async fn find_file_by_path(
        &self,
        path: &str,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::File>> {
        self.find_file_by_path(path, owner_id).await
    }

    async fn create_file_version(
        &self,
        version: &rustshare_core::domain::FileVersion,
    ) -> anyhow::Result<()> {
        self.create_file_version(version).await
    }

    async fn create_file_version_in_tx(
        &self,
        tx: &mut Self::Tx,
        version: &rustshare_core::domain::FileVersion,
    ) -> anyhow::Result<()> {
        self.create_file_version_in_tx(tx, version).await
    }

    async fn find_folder_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id(id, owner_id).await
    }

    async fn find_folder_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id_unchecked(id).await
    }

    async fn find_file_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::File>> {
        self.find_file_by_id(id, owner_id).await
    }

    async fn find_file_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::File>> {
        self.find_file_by_id_unchecked(id).await
    }

    async fn update_file(&self, file: &rustshare_core::domain::File) -> anyhow::Result<()> {
        self.update_file(file).await
    }

    async fn update_file_in_tx(
        &self,
        tx: &mut Self::Tx,
        file: &rustshare_core::domain::File,
    ) -> anyhow::Result<()> {
        self.update_file_in_tx(tx, file).await
    }

    async fn delete_file(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> anyhow::Result<()> {
        self.delete_file(id, owner_id).await
    }

    async fn delete_file_in_tx(
        &self,
        tx: &mut Self::Tx,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<()> {
        self.delete_file_in_tx(tx, id, owner_id).await
    }

    async fn list_file_versions(
        &self,
        file_id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::FileVersion>> {
        self.list_file_versions(file_id, owner_id).await
    }

    async fn find_file_version(
        &self,
        file_id: uuid::Uuid,
        version: i32,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::FileVersion>> {
        self.find_file_version(file_id, version, owner_id).await
    }

    async fn count_enabled_replication_targets(&self) -> anyhow::Result<i64> {
        self.count_enabled_replication_targets().await
    }

    async fn create_replication_job(
        &self,
        job: &rustshare_core::domain::ReplicationJob,
    ) -> anyhow::Result<()> {
        self.create_replication_job(job).await
    }

    async fn update_file_version_replication_state(
        &self,
        version_id: uuid::Uuid,
        state: rustshare_core::domain::ReplicationState,
    ) -> anyhow::Result<()> {
        self.update_file_version_replication_state(version_id, state)
            .await
    }
}

#[allow(async_fn_in_trait)]
impl rustshare_core::services::FolderMetadataStoreOps for MetadataStore {
    type Tx = sqlx::Transaction<'static, sqlx::Postgres>;

    async fn create_folder(&self, folder: &rustshare_core::domain::Folder) -> anyhow::Result<()> {
        self.create_folder(folder).await
    }

    async fn create_folder_in_tx(
        &self,
        tx: &mut Self::Tx,
        folder: &rustshare_core::domain::Folder,
    ) -> anyhow::Result<()> {
        self.create_folder_in_tx(tx, folder).await
    }

    async fn find_folder_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id(id, owner_id).await
    }

    async fn find_folder_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id_unchecked(id).await
    }

    async fn update_folder(&self, folder: &rustshare_core::domain::Folder) -> anyhow::Result<()> {
        self.update_folder(folder).await
    }

    async fn update_folder_in_tx(
        &self,
        tx: &mut Self::Tx,
        folder: &rustshare_core::domain::Folder,
    ) -> anyhow::Result<()> {
        self.update_folder_in_tx(tx, folder).await
    }

    async fn delete_folder(&self, id: uuid::Uuid, owner_id: uuid::Uuid) -> anyhow::Result<()> {
        self.delete_folder(id, owner_id).await
    }

    async fn delete_folder_in_tx(
        &self,
        tx: &mut Self::Tx,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<()> {
        self.delete_folder_in_tx(tx, id, owner_id).await
    }

    async fn list_folders(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Folder>> {
        self.list_folders(parent_id, owner_id, tenant_id).await
    }

    async fn list_folders_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Folder>> {
        self.list_folders_by_parent(parent_id, tenant_id).await
    }

    async fn find_descendant_folders(
        &self,
        folder_id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Folder>> {
        self.find_descendant_folders(folder_id, owner_id).await
    }

    async fn list_files(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::File>> {
        self.list_files(parent_id, owner_id, tenant_id).await
    }

    async fn list_files_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::File>> {
        self.list_files_by_parent(parent_id, tenant_id).await
    }
}

#[allow(async_fn_in_trait)]
impl rustshare_core::services::ShareMetadataStoreOps for MetadataStore {
    async fn find_user_by_id(
        &self,
        id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::User>> {
        self.find_user_by_id(id).await
    }

    async fn find_file_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::File>> {
        self.find_file_by_id(id, owner_id).await
    }

    async fn find_file_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::File>> {
        self.find_file_by_id_unchecked(id).await
    }

    async fn find_folder_by_id(
        &self,
        id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id(id, owner_id).await
    }

    async fn find_folder_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Folder>> {
        self.find_folder_by_id_unchecked(id).await
    }

    async fn create_share(&self, share: &rustshare_core::domain::Share) -> anyhow::Result<()> {
        self.create_share(share).await
    }

    async fn get_share_by_id(
        &self,
        id: uuid::Uuid,
        actor_id: rustshare_core::domain::UserId,
    ) -> anyhow::Result<Option<rustshare_core::domain::Share>> {
        self.get_share(id, actor_id).await
    }

    async fn get_share_by_id_unchecked(
        &self,
        id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Share>> {
        self.get_share_unchecked(id).await
    }

    async fn get_share_by_token(
        &self,
        token: &str,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Option<rustshare_core::domain::Share>> {
        self.get_share_by_token(token, tenant_id).await
    }

    async fn get_share_by_token_unscoped(
        &self,
        token: &str,
    ) -> anyhow::Result<Option<rustshare_core::domain::Share>> {
        self.get_share_by_token_unscoped(token).await
    }

    async fn get_file_shares(
        &self,
        file_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Share>> {
        self.get_file_shares(file_id).await
    }

    async fn get_folder_shares(
        &self,
        folder_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Share>> {
        self.get_folder_shares(folder_id).await
    }

    async fn list_files(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::File>> {
        self.list_files(parent_id, owner_id, tenant_id).await
    }

    async fn list_files_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::File>> {
        self.list_files_by_parent(parent_id, tenant_id).await
    }

    async fn list_folders(
        &self,
        parent_id: Option<uuid::Uuid>,
        owner_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Folder>> {
        self.list_folders(parent_id, owner_id, tenant_id).await
    }

    async fn list_folders_by_parent(
        &self,
        parent_id: Option<uuid::Uuid>,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Folder>> {
        self.list_folders_by_parent(parent_id, tenant_id).await
    }

    async fn find_descendant_folders(
        &self,
        folder_id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Folder>> {
        self.find_descendant_folders(folder_id, owner_id).await
    }

    async fn find_descendant_folders_unchecked(
        &self,
        folder_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Folder>> {
        self.find_descendant_folders_unchecked(folder_id).await
    }

    async fn revoke_share(
        &self,
        share_id: uuid::Uuid,
        actor_id: rustshare_core::domain::UserId,
    ) -> anyhow::Result<()> {
        self.revoke_share(share_id, actor_id).await
    }

    async fn update_share(&self, share: &rustshare_core::domain::Share) -> anyhow::Result<()> {
        self.update_share(share).await
    }

    async fn is_user_in_group(
        &self,
        user_id: rustshare_core::domain::UserId,
        group_id: uuid::Uuid,
    ) -> anyhow::Result<bool> {
        self.is_user_in_group(user_id, group_id).await
    }
}

#[allow(async_fn_in_trait, clippy::too_many_arguments)]
impl rustshare_core::services::VaultStore for MetadataStore {
    async fn create_vault(
        &self,
        vault: &rustshare_core::domain::Vault,
    ) -> anyhow::Result<rustshare_core::domain::Vault, rustshare_core::services::VaultSyncError>
    {
        self.create_vault(vault)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))
    }

    async fn get_vault(
        &self,
        vault_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<rustshare_core::domain::Vault, rustshare_core::services::VaultSyncError>
    {
        self.get_vault(vault_id, tenant_id)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))?
            .ok_or(rustshare_core::services::VaultSyncError::VaultNotFound(
                vault_id,
            ))
    }

    async fn list_vaults(
        &self,
        tenant_id: uuid::Uuid,
        owner_id: uuid::Uuid,
    ) -> anyhow::Result<Vec<rustshare_core::domain::Vault>, rustshare_core::services::VaultSyncError>
    {
        self.list_vaults(tenant_id, owner_id)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))
    }

    async fn get_file(
        &self,
        vault_id: uuid::Uuid,
        relative_path: &str,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<rustshare_core::domain::VaultFile, rustshare_core::services::VaultSyncError>
    {
        self.get_vault_file(vault_id, relative_path, tenant_id)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))?
            .ok_or_else(|| {
                rustshare_core::services::VaultSyncError::FileNotFound(relative_path.to_string())
            })
    }

    async fn get_file_including_deleted(
        &self,
        vault_id: uuid::Uuid,
        relative_path: &str,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<rustshare_core::domain::VaultFile, rustshare_core::services::VaultSyncError>
    {
        self.get_vault_file_including_deleted(vault_id, relative_path, tenant_id)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))?
            .ok_or_else(|| {
                rustshare_core::services::VaultSyncError::FileNotFound(relative_path.to_string())
            })
    }

    async fn list_files(
        &self,
        vault_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
        limit: Option<i64>,
    ) -> anyhow::Result<
        Vec<rustshare_core::domain::VaultFile>,
        rustshare_core::services::VaultSyncError,
    > {
        self.list_vault_files(vault_id, tenant_id, limit)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))
    }

    async fn insert_file_atomic(
        &self,
        file: &rustshare_core::domain::VaultFile,
    ) -> anyhow::Result<rustshare_core::domain::VaultFile, rustshare_core::services::VaultSyncError>
    {
        self.insert_vault_file_atomic(file)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => {
                    rustshare_core::services::VaultSyncError::VaultNotFound(file.vault_id)
                }
                _ => rustshare_core::services::VaultSyncError::Database(e.to_string()),
            })
    }

    async fn update_file_conditional_atomic(
        &self,
        file: &rustshare_core::domain::VaultFile,
        base_server_rev: i64,
    ) -> anyhow::Result<
        Option<rustshare_core::domain::VaultFile>,
        rustshare_core::services::VaultSyncError,
    > {
        self.update_vault_file_conditional_atomic(file, base_server_rev)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => {
                    rustshare_core::services::VaultSyncError::VaultNotFound(file.vault_id)
                }
                _ => rustshare_core::services::VaultSyncError::Database(e.to_string()),
            })
    }

    async fn update_file_conditional_atomic_for_webui(
        &self,
        file: &rustshare_core::domain::VaultFile,
        base_server_rev: i64,
    ) -> anyhow::Result<
        Option<rustshare_core::domain::VaultFile>,
        rustshare_core::services::VaultSyncError,
    > {
        self.update_vault_file_conditional_atomic_for_webui(file, base_server_rev)
            .await
    }

    async fn tombstone_file_conditional_atomic(
        &self,
        vault_id: uuid::Uuid,
        relative_path: &str,
        tenant_id: uuid::Uuid,
        base_server_rev: i64,
        device_id: &str,
    ) -> anyhow::Result<
        Option<rustshare_core::domain::VaultFile>,
        rustshare_core::services::VaultSyncError,
    > {
        self.tombstone_vault_file_conditional_atomic(
            vault_id,
            relative_path,
            tenant_id,
            base_server_rev,
            device_id,
        )
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => {
                rustshare_core::services::VaultSyncError::VaultNotFound(vault_id)
            }
            _ => rustshare_core::services::VaultSyncError::Database(e.to_string()),
        })
    }

    async fn rename_file_conditional_atomic(
        &self,
        vault_id: uuid::Uuid,
        old_path: &str,
        new_path: &str,
        tenant_id: uuid::Uuid,
        base_server_rev: i64,
        device_id: &str,
    ) -> anyhow::Result<
        Option<rustshare_core::domain::VaultFile>,
        rustshare_core::services::VaultSyncError,
    > {
        self.rename_vault_file_conditional_atomic(
            vault_id,
            old_path,
            new_path,
            tenant_id,
            base_server_rev,
            device_id,
        )
        .await
        .map_err(|e| {
            if let Some(err) = e.downcast_ref::<VaultFileStoreError>() {
                match err {
                    VaultFileStoreError::NotFound => {
                        rustshare_core::services::VaultSyncError::FileNotFound(old_path.to_string())
                    }
                    VaultFileStoreError::DestinationExists => {
                        rustshare_core::services::VaultSyncError::FileAlreadyExists(
                            new_path.to_string(),
                        )
                    }
                }
            } else if e
                .downcast_ref::<sqlx::Error>()
                .map(|se| matches!(se, sqlx::Error::RowNotFound))
                .unwrap_or(false)
            {
                rustshare_core::services::VaultSyncError::VaultNotFound(vault_id)
            } else {
                rustshare_core::services::VaultSyncError::Database(e.to_string())
            }
        })
    }

    async fn register_device(
        &self,
        device: &rustshare_core::domain::VaultDevice,
    ) -> anyhow::Result<rustshare_core::domain::VaultDevice, rustshare_core::services::VaultSyncError>
    {
        self.create_vault_device(device)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))
    }

    async fn get_device(
        &self,
        device_id: &str,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<rustshare_core::domain::VaultDevice, rustshare_core::services::VaultSyncError>
    {
        self.get_vault_device(device_id, tenant_id)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))?
            .ok_or_else(|| {
                rustshare_core::services::VaultSyncError::DeviceNotFound(device_id.to_string())
            })
    }

    async fn bind_device_to_vault(
        &self,
        device_id: &str,
        tenant_id: uuid::Uuid,
        vault_id: uuid::Uuid,
    ) -> anyhow::Result<rustshare_core::domain::VaultDevice, rustshare_core::services::VaultSyncError>
    {
        match self
            .bind_vault_device_to_vault(device_id, tenant_id, vault_id)
            .await
        {
            Ok(device) => Ok(device),
            Err(sqlx::Error::RowNotFound) => {
                match self.get_vault_device(device_id, tenant_id).await {
                    Ok(Some(device)) if device.revoked_at.is_some() => {
                        Err(rustshare_core::services::VaultSyncError::DeviceRevoked)
                    }
                    Ok(Some(_)) => Err(rustshare_core::services::VaultSyncError::Unauthorized),
                    Ok(None) => Err(rustshare_core::services::VaultSyncError::DeviceNotFound(
                        device_id.to_string(),
                    )),
                    Err(e) => Err(rustshare_core::services::VaultSyncError::Database(
                        e.to_string(),
                    )),
                }
            }
            Err(e) => Err(rustshare_core::services::VaultSyncError::Database(
                e.to_string(),
            )),
        }
    }

    async fn update_vault(
        &self,
        vault: &rustshare_core::domain::Vault,
    ) -> anyhow::Result<rustshare_core::domain::Vault, rustshare_core::services::VaultSyncError>
    {
        self.update_vault(vault)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))
    }

    async fn update_vault_write_policy(
        &self,
        vault_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
        write_policy: &rustshare_core::domain::VaultWritePolicy,
        updated_at: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<rustshare_core::domain::Vault, rustshare_core::services::VaultSyncError>
    {
        self.update_vault_write_policy(vault_id, tenant_id, write_policy, updated_at)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))
    }

    async fn get_webui_device(
        &self,
        tenant_id: uuid::Uuid,
        user_id: uuid::Uuid,
        vault_id: uuid::Uuid,
    ) -> anyhow::Result<
        Option<rustshare_core::domain::VaultDevice>,
        rustshare_core::services::VaultSyncError,
    > {
        self.get_webui_device(tenant_id, user_id, vault_id)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))
    }

    async fn create_webui_device(
        &self,
        device: &rustshare_core::domain::VaultDevice,
    ) -> anyhow::Result<rustshare_core::domain::VaultDevice, rustshare_core::services::VaultSyncError>
    {
        self.create_webui_device(device)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))
    }

    async fn revoke_device(
        &self,
        device_id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<(), rustshare_core::services::VaultSyncError> {
        self.revoke_vault_device(device_id, tenant_id)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => {
                    rustshare_core::services::VaultSyncError::DeviceNotFound(device_id.to_string())
                }
                _ => rustshare_core::services::VaultSyncError::Database(e.to_string()),
            })
    }

    async fn update_device_last_seen(
        &self,
        device_id: &str,
        tenant_id: uuid::Uuid,
    ) -> anyhow::Result<(), rustshare_core::services::VaultSyncError> {
        self.update_vault_device_last_seen(device_id, tenant_id)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => rustshare_core::services::VaultSyncError::DeviceRevoked,
                _ => rustshare_core::services::VaultSyncError::Database(e.to_string()),
            })
    }

    async fn update_vault_device_last_seen_at(
        &self,
        device_id: uuid::Uuid,
        last_seen_at: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<(), rustshare_core::services::VaultSyncError> {
        self.update_vault_device_last_seen_at(device_id, last_seen_at)
            .await
            .map_err(|e| rustshare_core::services::VaultSyncError::Database(e.to_string()))
    }
}
