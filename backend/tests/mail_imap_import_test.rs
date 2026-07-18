//! Integration test for the IMAP selected import flow.

use rustshare_core::domain::{MailImportJobStatus, MailTlsMode};

mod contracts;
use contracts::common::{cleanup_tenant, cleanup_user, setup_test_env};

#[tokio::test]
#[ignore = "requires IMAP test server (e.g. GreenMail)"]
async fn imap_selected_import_creates_job_and_imports_messages() {
    // 1. Set up test context.
    let ctx = setup_test_env().await;
    let tenant_id = ctx.tenant_id;
    let user = ctx.create_test_user("imapuser").await;

    // 2. Skip if no IMAP test server is configured or reachable.
    //    The IMAP client rejects internal/loopback hosts for SSRF protection,
    //    so the test server must be a public or non-internal address.
    let imap_host = std::env::var("IMAP_TEST_HOST").unwrap_or_default();
    if imap_host.is_empty()
        || imap_host.eq_ignore_ascii_case("localhost")
        || imap_host.starts_with("127.")
    {
        eprintln!("Skipping imap_selected_import test: IMAP_TEST_HOST must be set to a public/non-internal IMAP server");
        cleanup_user(&ctx.pool, user.id).await;
        cleanup_tenant(&ctx.pool, tenant_id).await;
        return;
    }
    let imap_port: i32 = std::env::var("IMAP_TEST_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3143);
    if tokio::net::TcpStream::connect((imap_host.as_str(), imap_port as u16))
        .await
        .is_err()
    {
        eprintln!("Skipping imap_selected_import test: no IMAP server at {imap_host}:{imap_port}");
        cleanup_user(&ctx.pool, user.id).await;
        cleanup_tenant(&ctx.pool, tenant_id).await;
        return;
    }
    let imap_username =
        std::env::var("IMAP_TEST_USERNAME").unwrap_or_else(|_| "user@localhost".to_string());
    let imap_password =
        std::env::var("IMAP_TEST_PASSWORD").unwrap_or_else(|_| "password".to_string());

    let mail_service = ctx.mail_service();

    // 3. Create an IMAP account.
    let account = mail_service
        .create_account(
            tenant_id,
            user.id,
            "Test Account".to_string(),
            imap_host.clone(),
            imap_port,
            imap_username.clone(),
            imap_password,
            MailTlsMode::Tls,
        )
        .await
        .expect("create account should succeed");

    // 4. List folders and verify INBOX exists.
    let folders = mail_service
        .list_imap_folders(tenant_id, user.id, account.id)
        .await
        .expect("list folders should succeed");
    assert!(
        folders.iter().any(|f| f.name.eq_ignore_ascii_case("INBOX")),
        "INBOX should be present"
    );

    // 5. List messages in INBOX.
    let (uidvalidity, messages) = mail_service
        .list_imap_messages(tenant_id, user.id, account.id, "INBOX", 10, None, None)
        .await
        .expect("list messages should succeed");
    let uidvalidity = uidvalidity.expect("INBOX should report a UIDVALIDITY");

    // 6. Create an import job for the first message UID (if any).
    let uids: Vec<i64> = messages.iter().map(|m| m.uid as i64).collect();
    if uids.is_empty() {
        // Nothing to import; test passes vacuously.
        cleanup_user(&ctx.pool, user.id).await;
        cleanup_tenant(&ctx.pool, tenant_id).await;
        return;
    }
    let job = mail_service
        .create_imap_import_job(
            tenant_id,
            user.id,
            account.id,
            "INBOX".to_string(),
            Some(i64::from(uidvalidity)),
            uids.clone(),
        )
        .await
        .expect("create import job should succeed");
    assert_eq!(job.status, String::from(MailImportJobStatus::Pending));
    assert_eq!(job.total_messages as usize, uids.len());

    // 7. Process the job synchronously.
    mail_service
        .process_import_job(&job)
        .await
        .expect("process import job should succeed");

    // 8. Verify the job completed and messages were imported.
    let job = mail_service
        .get_import_job(tenant_id, user.id, job.id)
        .await
        .expect("get import job should succeed");
    assert_eq!(job.status, String::from(MailImportJobStatus::Completed));
    assert_eq!(job.processed_messages as usize, uids.len());
    assert_eq!(job.failed_messages, 0);

    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, tenant_id).await;
}
