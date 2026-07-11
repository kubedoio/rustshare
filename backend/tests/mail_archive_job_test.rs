//! Integration test for the archive job lifecycle.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, NaiveDate, Utc};
use rustshare_core::domain::MailTlsMode;
use rustshare_server::services::imap_client::{ImapArchiveSession, ImapError};
use uuid::Uuid;

mod contracts;
use contracts::common::{cleanup_tenant, cleanup_user, setup_test_env};

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn archive_job_lifecycle_create_cancel_delete() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(
            ctx.tenant_id,
            user.id,
            "Test".to_string(),
            "imap.example.com".to_string(),
            993,
            "user".to_string(),
            "pass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    let job = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            Some(90),
            None,
        )
        .await
        .unwrap();

    assert_eq!(job.source_mode, "imap_archive");
    assert_eq!(job.retention_days, Some(90));
    assert_eq!(job.status, "pending");

    // List
    let jobs = ctx
        .mail_service()
        .list_archive_jobs(ctx.tenant_id, user.id, account.id)
        .await
        .unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, job.id);

    // Get
    let fetched = ctx
        .mail_service()
        .get_archive_job(ctx.tenant_id, user.id, job.id)
        .await
        .unwrap();
    assert_eq!(fetched.id, job.id);

    // Cancel
    let cancelled = ctx
        .mail_service()
        .cancel_archive_job(ctx.tenant_id, user.id, job.id)
        .await
        .unwrap();
    assert_eq!(cancelled.status, "cancelled");

    // Delete
    ctx.mail_service()
        .delete_archive_job(ctx.tenant_id, user.id, job.id)
        .await
        .unwrap();

    // Verify deleted
    let result = ctx
        .mail_service()
        .get_archive_job(ctx.tenant_id, user.id, job.id)
        .await;
    assert!(result.is_err());

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

fn sample_email_bytes(subject: &str, message_id: &str, date: &str, body: &str) -> Vec<u8> {
    format!(
        "From: sender@example.com\r\n\
         To: recipient@example.com\r\n\
         Subject: {}\r\n\
         Message-Id: <{}>\r\n\
         Date: {}\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\n\
         {}\r\n",
        subject, message_id, date, body
    )
    .into_bytes()
}

struct MockImapArchiveSession {
    uidvalidity: Option<u32>,
    uids: Vec<u32>,
    messages: HashMap<u32, Vec<u8>>,
    fetch_count: Arc<AtomicUsize>,
    fetch_uids_error: Option<String>,
    fetch_rfc822_error: Option<String>,
    fetch_rfc822_errors: HashMap<u32, String>,
}

impl MockImapArchiveSession {
    fn new(
        uidvalidity: Option<u32>,
        uids: Vec<u32>,
        messages: HashMap<u32, Vec<u8>>,
    ) -> (Self, Arc<AtomicUsize>) {
        let fetch_count = Arc::new(AtomicUsize::new(0));
        (
            Self {
                uidvalidity,
                uids,
                messages,
                fetch_count: fetch_count.clone(),
                fetch_uids_error: None,
                fetch_rfc822_error: None,
                fetch_rfc822_errors: HashMap::new(),
            },
            fetch_count,
        )
    }

    fn with_rfc822_error(mut self, uid: u32, error: impl Into<String>) -> Self {
        self.fetch_rfc822_errors.insert(uid, error.into());
        self
    }
}

#[async_trait]
impl ImapArchiveSession for MockImapArchiveSession {
    async fn fetch_uids_by_date_range(
        &mut self,
        _folder: &str,
        _since: Option<chrono::NaiveDate>,
        _before: Option<chrono::NaiveDate>,
    ) -> Result<(Option<u32>, Vec<u32>), ImapError> {
        if let Some(err) = &self.fetch_uids_error {
            return Err(ImapError::CommandFailed(err.clone()));
        }
        Ok((self.uidvalidity, self.uids.clone()))
    }

    async fn fetch_rfc822(
        &mut self,
        _folder: &str,
        uid: u32,
        _expected_uidvalidity: Option<i64>,
    ) -> Result<Vec<u8>, ImapError> {
        self.fetch_count.fetch_add(1, Ordering::SeqCst);
        if let Some(err) = self.fetch_rfc822_errors.get(&uid) {
            return Err(ImapError::CommandFailed(err.clone()));
        }
        if let Some(err) = &self.fetch_rfc822_error {
            return Err(ImapError::CommandFailed(err.clone()));
        }
        self.messages
            .get(&uid)
            .cloned()
            .ok_or_else(|| ImapError::CommandFailed(format!("message {uid} not found")))
    }
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn archive_job_processes_with_mock_imap() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(
            ctx.tenant_id,
            user.id,
            "Test".to_string(),
            "imap.example.com".to_string(),
            993,
            "user".to_string(),
            "pass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    let job = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            Some(90),
            None,
        )
        .await
        .unwrap();

    let mut messages = HashMap::new();
    messages.insert(
        1,
        sample_email_bytes(
            "First",
            "msg1@example.com",
            "Mon, 15 Aug 2022 10:30:00 +0000",
            "body one",
        ),
    );
    messages.insert(
        2,
        sample_email_bytes(
            "Second",
            "msg2@example.com",
            "Tue, 16 Aug 2022 11:00:00 +0000",
            "body two",
        ),
    );
    let (mut session, fetch_count) = MockImapArchiveSession::new(Some(1000), vec![1, 2], messages);

    ctx.metadata_store
        .mark_mail_import_job_running(job.id)
        .await
        .unwrap();
    ctx.mail_service()
        .process_archive_session(&job, &mut session)
        .await
        .unwrap();

    let updated = ctx
        .mail_service()
        .get_archive_job(ctx.tenant_id, user.id, job.id)
        .await
        .unwrap();
    assert_eq!(updated.status, "pending");
    assert_eq!(updated.total_messages, 2);
    assert_eq!(updated.processed_messages, 2);
    assert_eq!(updated.failed_messages, 0);
    assert_eq!(updated.last_uid_validity, Some(1000));
    assert_eq!(updated.last_imported_uid, Some(2));

    let msgs = ctx
        .metadata_store
        .list_mail_messages(ctx.tenant_id, user.id)
        .await
        .unwrap();
    assert_eq!(msgs.len(), 2);
    let uids: Vec<_> = msgs.iter().map(|m| m.source_uid).collect();
    assert!(uids.contains(&Some(1)));
    assert!(uids.contains(&Some(2)));
    for m in &msgs {
        assert_eq!(m.source_mode, "imap_archive");
        assert_eq!(m.source_folder.as_deref(), Some("INBOX"));
        assert_eq!(m.source_uidvalidity, Some(1000));
        assert_eq!(m.account_id, Some(account.id));
    }
    assert_eq!(fetch_count.load(Ordering::SeqCst), 2);

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn archive_job_uidvalidity_reset_reimports() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(
            ctx.tenant_id,
            user.id,
            "Test".to_string(),
            "imap.example.com".to_string(),
            993,
            "user".to_string(),
            "pass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    let job = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let msg = sample_email_bytes(
        "Reimport",
        "msg@example.com",
        "Mon, 15 Aug 2022 10:30:00 +0000",
        "body",
    );

    let (mut session1, fetch_count) =
        MockImapArchiveSession::new(Some(1000), vec![1], [(1, msg.clone())].into());
    ctx.metadata_store
        .mark_mail_import_job_running(job.id)
        .await
        .unwrap();
    ctx.mail_service()
        .process_archive_session(&job, &mut session1)
        .await
        .unwrap();

    let msgs_after_first = ctx
        .metadata_store
        .list_mail_messages(ctx.tenant_id, user.id)
        .await
        .unwrap();
    assert_eq!(msgs_after_first.len(), 1);

    // Simulate a UIDVALIDITY reset on the server and run the same job again.
    sqlx::query("UPDATE mail_import_jobs SET status = 'running' WHERE id = $1")
        .bind(job.id)
        .execute(&ctx.pool)
        .await
        .unwrap();

    let (mut session2, fetch_count2) =
        MockImapArchiveSession::new(Some(2000), vec![1], [(1, msg)].into());
    ctx.mail_service()
        .process_archive_session(&job, &mut session2)
        .await
        .unwrap();

    let updated = ctx
        .mail_service()
        .get_archive_job(ctx.tenant_id, user.id, job.id)
        .await
        .unwrap();
    assert_eq!(updated.last_uid_validity, Some(2000));

    let msgs = ctx
        .metadata_store
        .list_mail_messages(ctx.tenant_id, user.id)
        .await
        .unwrap();
    assert_eq!(msgs.len(), 2);
    let uidvalidities: Vec<_> = msgs
        .iter()
        .map(|m| (m.source_uid, m.source_uidvalidity))
        .collect();
    assert!(uidvalidities.contains(&(Some(1), Some(1000))));
    assert!(uidvalidities.contains(&(Some(1), Some(2000))));
    assert_eq!(
        fetch_count.load(Ordering::SeqCst) + fetch_count2.load(Ordering::SeqCst),
        2
    );

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn archive_job_retention_soft_deletes_old_messages() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(
            ctx.tenant_id,
            user.id,
            "Test".to_string(),
            "imap.example.com".to_string(),
            993,
            "user".to_string(),
            "pass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    let job = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            Some(1),
            None,
        )
        .await
        .unwrap();

    let msg = sample_email_bytes(
        "Old",
        "old@example.com",
        "Mon, 15 Aug 2022 10:30:00 +0000",
        "body",
    );

    let (mut session, _) = MockImapArchiveSession::new(Some(1000), vec![1], [(1, msg)].into());
    ctx.metadata_store
        .mark_mail_import_job_running(job.id)
        .await
        .unwrap();
    ctx.mail_service()
        .process_archive_session(&job, &mut session)
        .await
        .unwrap();

    // Age the imported message so that a 1-day retention policy deletes it.
    let old_imported_at = Utc::now() - Duration::days(10);
    sqlx::query(
        "UPDATE mail_messages SET imported_at = $1 WHERE owner_id = $2 AND source_uid = $3 AND source_uidvalidity = $4",
    )
    .bind(old_imported_at)
    .bind(user.id)
    .bind(1_i64)
    .bind(1000_i64)
    .execute(&ctx.pool)
    .await
    .unwrap();
    let folder_id: Uuid = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT folder_id FROM mail_messages WHERE owner_id = $1 AND source_uid = $2 AND source_uidvalidity = $3",
    )
    .bind(user.id)
    .bind(1_i64)
    .bind(1000_i64)
    .fetch_one(&ctx.pool)
    .await
    .unwrap()
    .expect("archive import should set folder_id");

    // Re-run the job with the same UID; the message is deduplicated, but the
    // retention pass still applies.
    ctx.mail_service()
        .process_archive_session(&job, &mut session)
        .await
        .unwrap();

    let deleted_count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM mail_messages WHERE owner_id = $1 AND deleted_at IS NOT NULL",
    )
    .bind(user.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(deleted_count, 1);

    let visible = ctx
        .metadata_store
        .list_mail_messages(ctx.tenant_id, user.id)
        .await
        .unwrap();
    assert!(visible.is_empty());

    let visible_artifacts: i64 = sqlx::query_scalar(
        r#"
        SELECT
          (SELECT COUNT(*) FROM folders WHERE id = $1 AND deleted_at IS NULL)
        + (SELECT COUNT(*) FROM files WHERE parent_folder_id = $1 AND deleted_at IS NULL)
        "#,
    )
    .bind(folder_id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(visible_artifacts, 0);

    // Internal parts/attachments rows must also be purged.
    let remaining_parts: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM mail_message_parts WHERE message_id IN (SELECT id FROM mail_messages WHERE owner_id = $1)"
    )
    .bind(user.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(remaining_parts, 0);

    let remaining_attachments: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM mail_attachments WHERE message_id IN (SELECT id FROM mail_messages WHERE owner_id = $1)"
    )
    .bind(user.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(remaining_attachments, 0);

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn archive_job_watermark_stops_at_first_failure() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(
            ctx.tenant_id,
            user.id,
            "Test".to_string(),
            "imap.example.com".to_string(),
            993,
            "user".to_string(),
            "pass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    let job = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let mut messages = HashMap::new();
    messages.insert(
        1,
        sample_email_bytes(
            "First",
            "msg1@example.com",
            "Mon, 15 Aug 2022 10:30:00 +0000",
            "body one",
        ),
    );
    messages.insert(
        3,
        sample_email_bytes(
            "Third",
            "msg3@example.com",
            "Tue, 16 Aug 2022 11:00:00 +0000",
            "body three",
        ),
    );
    let (mut session, _) = MockImapArchiveSession::new(Some(1000), vec![1, 2, 3], messages);
    session = session.with_rfc822_error(2, "simulate fetch failure");

    ctx.metadata_store
        .mark_mail_import_job_running(job.id)
        .await
        .unwrap();
    ctx.mail_service()
        .process_archive_session(&job, &mut session)
        .await
        .unwrap();

    let updated = ctx
        .mail_service()
        .get_archive_job(ctx.tenant_id, user.id, job.id)
        .await
        .unwrap();
    // UID 2 failed, so the watermark must not advance past UID 1 even though
    // UID 3 imported successfully.
    assert_eq!(updated.last_uid_validity, Some(1000));
    assert_eq!(updated.last_imported_uid, Some(1));
    assert_eq!(updated.processed_messages, 2);
    assert_eq!(updated.failed_messages, 1);

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn archive_job_reclaims_incomplete_row() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(
            ctx.tenant_id,
            user.id,
            "Test".to_string(),
            "imap.example.com".to_string(),
            993,
            "user".to_string(),
            "pass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    let job = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // Insert an incomplete mail_messages row (folder_id is NULL) as if a
    // previous run crashed after deduplication but before artifacts were ready.
    let old_id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO mail_messages (
            id, tenant_id, owner_id, account_id, source_mode, source_folder,
            source_uid, source_uidvalidity, imported_at, imported_by, visibility
        ) VALUES ($1, $2, $3, $4, 'imap_archive', 'INBOX', 1, 1000, NOW(), $3, 'private')
        "#,
    )
    .bind(old_id)
    .bind(ctx.tenant_id)
    .bind(user.id)
    .bind(account.id)
    .execute(&ctx.pool)
    .await
    .unwrap();

    let msg = sample_email_bytes(
        "Complete",
        "complete@example.com",
        "Mon, 15 Aug 2022 10:30:00 +0000",
        "body",
    );
    let (mut session, _) = MockImapArchiveSession::new(Some(1000), vec![1], [(1, msg)].into());

    ctx.metadata_store
        .mark_mail_import_job_running(job.id)
        .await
        .unwrap();
    ctx.mail_service()
        .process_archive_session(&job, &mut session)
        .await
        .unwrap();

    let updated = ctx
        .mail_service()
        .get_archive_job(ctx.tenant_id, user.id, job.id)
        .await
        .unwrap();
    assert_eq!(updated.status, "pending");
    assert_eq!(updated.last_imported_uid, Some(1));

    let msgs = ctx
        .metadata_store
        .list_mail_messages(ctx.tenant_id, user.id)
        .await
        .unwrap();
    assert_eq!(msgs.len(), 1);
    let m = &msgs[0];
    // The incomplete row must have been replaced, not reused.
    assert_ne!(m.id, old_id);
    assert!(m.folder_id.is_some());
    assert_eq!(m.source_uid, Some(1));

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn archive_job_watermark_advances_across_gaps() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(
            ctx.tenant_id,
            user.id,
            "Test".to_string(),
            "imap.example.com".to_string(),
            993,
            "user".to_string(),
            "pass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    let job = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let mut messages = HashMap::new();
    messages.insert(
        1,
        sample_email_bytes(
            "First",
            "msg1@example.com",
            "Mon, 15 Aug 2022 10:30:00 +0000",
            "body one",
        ),
    );
    messages.insert(
        3,
        sample_email_bytes(
            "Third",
            "msg3@example.com",
            "Mon, 15 Aug 2022 11:00:00 +0000",
            "body three",
        ),
    );
    messages.insert(
        5,
        sample_email_bytes(
            "Fifth",
            "msg5@example.com",
            "Mon, 15 Aug 2022 12:00:00 +0000",
            "body five",
        ),
    );
    let (mut session, _) = MockImapArchiveSession::new(Some(1000), vec![1, 3, 5], messages);

    ctx.metadata_store
        .mark_mail_import_job_running(job.id)
        .await
        .unwrap();
    ctx.mail_service()
        .process_archive_session(&job, &mut session)
        .await
        .unwrap();

    let updated = ctx
        .mail_service()
        .get_archive_job(ctx.tenant_id, user.id, job.id)
        .await
        .unwrap();
    // IMAP UIDs are monotonic but not contiguous; the watermark must reach the
    // highest successfully imported UID even when gaps represent deleted messages.
    assert_eq!(updated.status, "pending");
    assert_eq!(updated.last_uid_validity, Some(1000));
    assert_eq!(updated.last_imported_uid, Some(5));
    assert_eq!(updated.processed_messages, 3);
    assert_eq!(updated.failed_messages, 0);

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn archive_job_retention_skipped_when_cancelled() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(
            ctx.tenant_id,
            user.id,
            "Test".to_string(),
            "imap.example.com".to_string(),
            993,
            "user".to_string(),
            "pass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    let job = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            Some(1),
            None,
        )
        .await
        .unwrap();

    let msg = sample_email_bytes(
        "Old",
        "old@example.com",
        "Mon, 15 Aug 2022 10:30:00 +0000",
        "body",
    );
    let (mut session, _) = MockImapArchiveSession::new(Some(1000), vec![1], [(1, msg)].into());

    ctx.metadata_store
        .mark_mail_import_job_running(job.id)
        .await
        .unwrap();
    ctx.mail_service()
        .process_archive_session(&job, &mut session)
        .await
        .unwrap();

    // Age the imported message so it would be deleted by retention.
    let old_imported_at = Utc::now() - Duration::days(10);
    sqlx::query(
        "UPDATE mail_messages SET imported_at = $1 WHERE owner_id = $2 AND source_uid = $3 AND source_uidvalidity = $4",
    )
    .bind(old_imported_at)
    .bind(user.id)
    .bind(1_i64)
    .bind(1000_i64)
    .execute(&ctx.pool)
    .await
    .unwrap();

    // Cancel the job, then run the session again. The deduplication loop
    // breaks immediately and retention must be skipped.
    ctx.metadata_store
        .update_mail_import_job_status(job.id, "cancelled", &["pending", "running"])
        .await
        .unwrap();

    ctx.mail_service()
        .process_archive_session(&job, &mut session)
        .await
        .unwrap();

    let deleted_count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM mail_messages WHERE owner_id = $1 AND deleted_at IS NOT NULL",
    )
    .bind(user.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(deleted_count, 0);

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn archive_job_reassigns_ownership_from_deleted_job() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(
            ctx.tenant_id,
            user.id,
            "Test".to_string(),
            "imap.example.com".to_string(),
            993,
            "user".to_string(),
            "pass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    let job_a = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let msg = sample_email_bytes(
        "Shared",
        "shared@example.com",
        "Mon, 15 Aug 2022 10:30:00 +0000",
        "body",
    );
    let (mut session_a, _) =
        MockImapArchiveSession::new(Some(1000), vec![1], [(1, msg.clone())].into());

    ctx.metadata_store
        .mark_mail_import_job_running(job_a.id)
        .await
        .unwrap();
    ctx.mail_service()
        .process_archive_session(&job_a, &mut session_a)
        .await
        .unwrap();

    // Delete job A so its archived messages are orphaned.
    ctx.mail_service()
        .delete_archive_job(ctx.tenant_id, user.id, job_a.id)
        .await
        .unwrap();

    // Create job B for the same folder and run it over the same UID.
    let job_b = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let (mut session_b, _) = MockImapArchiveSession::new(Some(1000), vec![1], [(1, msg)].into());

    ctx.metadata_store
        .mark_mail_import_job_running(job_b.id)
        .await
        .unwrap();
    ctx.mail_service()
        .process_archive_session(&job_b, &mut session_b)
        .await
        .unwrap();

    let updated_b = ctx
        .mail_service()
        .get_archive_job(ctx.tenant_id, user.id, job_b.id)
        .await
        .unwrap();
    assert_eq!(updated_b.last_imported_uid, Some(1));
    assert_eq!(updated_b.processed_messages, 1);

    let archive_job_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT archive_job_id FROM mail_messages WHERE owner_id = $1 AND source_uid = $2 AND source_uidvalidity = $3 AND deleted_at IS NULL",
    )
    .bind(user.id)
    .bind(1_i64)
    .bind(1000_i64)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(archive_job_id, Some(job_b.id));

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn archive_job_delete_cancels_running_job() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(
            ctx.tenant_id,
            user.id,
            "Test".to_string(),
            "imap.example.com".to_string(),
            993,
            "user".to_string(),
            "pass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    let job = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    ctx.metadata_store
        .mark_mail_import_job_running(job.id)
        .await
        .unwrap();

    ctx.mail_service()
        .delete_archive_job(ctx.tenant_id, user.id, job.id)
        .await
        .unwrap();

    let result = ctx
        .mail_service()
        .get_archive_job(ctx.tenant_id, user.id, job.id)
        .await;
    assert!(result.is_err());

    let (status, deleted_at): (String, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
        "SELECT status, deleted_at FROM mail_import_jobs WHERE id = $1 AND owner_id = $2",
    )
    .bind(job.id)
    .bind(user.id)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(status, "cancelled");
    assert!(deleted_at.is_some());

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn archive_job_reassigns_ownership_from_cancelled_job() {
    let ctx = setup_test_env().await;
    let user = ctx.create_test_user("archiveowner").await;
    let account = ctx
        .mail_service()
        .create_account(
            ctx.tenant_id,
            user.id,
            "Test".to_string(),
            "imap.example.com".to_string(),
            993,
            "user".to_string(),
            "pass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    let job_a = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let msg = sample_email_bytes(
        "Shared",
        "shared@example.com",
        "Mon, 15 Aug 2022 10:30:00 +0000",
        "body",
    );
    let (mut session_a, _) =
        MockImapArchiveSession::new(Some(1000), vec![1], [(1, msg.clone())].into());

    ctx.metadata_store
        .mark_mail_import_job_running(job_a.id)
        .await
        .unwrap();
    ctx.mail_service()
        .process_archive_session(&job_a, &mut session_a)
        .await
        .unwrap();

    // Cancel job A so its archived messages are orphaned.
    ctx.mail_service()
        .cancel_archive_job(ctx.tenant_id, user.id, job_a.id)
        .await
        .unwrap();

    // Create job B for the same folder and run it over the same UID.
    let job_b = ctx
        .mail_service()
        .create_archive_job(
            ctx.tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let (mut session_b, _) = MockImapArchiveSession::new(Some(1000), vec![1], [(1, msg)].into());

    ctx.metadata_store
        .mark_mail_import_job_running(job_b.id)
        .await
        .unwrap();
    ctx.mail_service()
        .process_archive_session(&job_b, &mut session_b)
        .await
        .unwrap();

    let archive_job_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT archive_job_id FROM mail_messages WHERE owner_id = $1 AND source_uid = $2 AND source_uidvalidity = $3 AND deleted_at IS NULL",
    )
    .bind(user.id)
    .bind(1_i64)
    .bind(1000_i64)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(archive_job_id, Some(job_b.id));

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}
