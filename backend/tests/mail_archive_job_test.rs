//! Integration test for the archive job lifecycle.

use rustshare_core::domain::MailTlsMode;

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
