//! Integration tests for user-based SMTP and outbound mail workflows.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

use rustshare_core::domain::MailTlsMode;
mod contracts;
use contracts::common::{cleanup_tenant, cleanup_user, setup_test_env};

/// The production mail stack rejects plaintext SMTP and internal/private
/// mail-server destinations unless environment overrides are set. These
/// integration tests spin up a plaintext mock SMTP server on a localhost port,
/// so both overrides must be enabled. Because `std::env::set_var`/`remove_var`
/// are not thread-safe, multiple tests that toggle the same variable
/// concurrently race and can unset it mid-send. We set them once per
/// integration-test binary and never remove them.
fn enable_internal_smtp_for_tests() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        std::env::set_var("RUSTSHARE_ALLOW_INTERNAL_SMTP_FOR_TESTS", "true");
        std::env::set_var("RUSTSHARE_ALLOW_INTERNAL_MAIL_SERVERS", "true");
    });
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn test_smtp_settings_crud_and_isolation() {
    let ctx = setup_test_env().await;
    let mail_service = ctx.mail_service();

    // 1. Create two users (isolation testing)
    let user_a = ctx.create_test_user("user_a").await;
    let user_b = ctx.create_test_user("user_b").await;

    // Create an IMAP account for User A to attach SMTP settings to
    let account_a = mail_service
        .create_account(
            ctx.tenant_id,
            user_a.id,
            "User A IMAP".to_string(),
            "imap.a.com".to_string(),
            993,
            "usera@a.com".to_string(),
            "passa".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    // Create an IMAP account for User B to attach SMTP settings to
    let _account_b = mail_service
        .create_account(
            ctx.tenant_id,
            user_b.id,
            "User B IMAP".to_string(),
            "imap.b.com".to_string(),
            993,
            "userb".to_string(),
            "passb".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    // 2. CRUD for User A
    let smtp_a = mail_service
        .create_or_update_smtp_settings(
            ctx.tenant_id,
            user_a.id,
            account_a.id,
            "smtp.a.com".to_string(),
            587,
            "smtp_user_a".to_string(),
            Some("smtp_pass_a".to_string()),
            MailTlsMode::StartTls,
            "usera@a.com".to_string(),
            Some("User A Name".to_string()),
            None,
            Some("Sent".to_string()),
            true,
        )
        .await
        .unwrap();

    let unauthorized_from = mail_service
        .create_or_update_smtp_settings(
            ctx.tenant_id,
            user_a.id,
            account_a.id,
            "smtp.a.com".to_string(),
            587,
            "smtp_user_a".to_string(),
            Some("smtp_pass_a".to_string()),
            MailTlsMode::StartTls,
            "spoofed@example.com".to_string(),
            None,
            None,
            None,
            true,
        )
        .await
        .expect_err("unverified From identity must be rejected");
    assert!(unauthorized_from
        .to_string()
        .contains("mail account identity"));

    assert_eq!(smtp_a.host, "smtp.a.com");
    assert_eq!(smtp_a.port, 587);
    assert_eq!(smtp_a.username, "smtp_user_a");
    assert_eq!(smtp_a.from_address, "usera@a.com");
    assert_eq!(smtp_a.sent_folder.as_deref(), Some("Sent"));

    // Get settings
    let fetched_a = mail_service
        .get_smtp_settings(ctx.tenant_id, user_a.id, account_a.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched_a.id, smtp_a.id);

    // 3. Isolation: User B cannot access User A's SMTP settings
    let isolation_get = mail_service
        .get_smtp_settings(ctx.tenant_id, user_b.id, account_a.id)
        .await;
    assert!(isolation_get.is_err()); // PermissionDenied because account_a belongs to User A

    // 4. Update Settings
    let updated_a = mail_service
        .create_or_update_smtp_settings(
            ctx.tenant_id,
            user_a.id,
            account_a.id,
            "smtp.a.com".to_string(),
            465,
            "smtp_user_a".to_string(),
            None, // Password omitted to preserve existing
            MailTlsMode::Tls,
            "usera@a.com".to_string(),
            Some("User A Updated".to_string()),
            None,
            Some("Sent".to_string()),
            true,
        )
        .await
        .unwrap();

    assert_eq!(updated_a.port, 465);
    assert_eq!(updated_a.from_name.as_deref(), Some("User A Updated"));
    assert_eq!(updated_a.password_enc, smtp_a.password_enc); // password should be preserved

    // 5. Delete Settings
    mail_service
        .delete_smtp_settings(ctx.tenant_id, user_a.id, account_a.id)
        .await
        .unwrap();

    let after_delete = mail_service
        .get_smtp_settings(ctx.tenant_id, user_a.id, account_a.id)
        .await
        .unwrap();
    assert!(after_delete.is_none());

    // Clean up
    cleanup_user(&ctx.pool, user_a.id).await;
    cleanup_user(&ctx.pool, user_b.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn test_outbound_mail_send_flow() {
    let ctx = setup_test_env().await;
    enable_internal_smtp_for_tests();
    let mail_service = ctx.mail_service();
    let user = ctx.create_test_user("smtp_sender").await;

    // 1. Setup mock SMTP server in background
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let smtp_port = local_addr.port() as i32;

    let server_task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0; 4096];

        // 220 Greeting
        stream.write_all(b"220 localhost ESMTP\r\n").await.unwrap();

        // EHLO
        let _ = stream.read(&mut buf).await.unwrap();
        stream
            .write_all(b"250-localhost\r\n250 AUTH PLAIN\r\n")
            .await
            .unwrap();

        // AUTH PLAIN
        let _ = stream.read(&mut buf).await.unwrap();
        stream
            .write_all(b"235 Authentication successful\r\n")
            .await
            .unwrap();

        // MAIL FROM
        let _ = stream.read(&mut buf).await.unwrap();
        stream.write_all(b"250 Ok\r\n").await.unwrap();

        // RCPT TO
        let _ = stream.read(&mut buf).await.unwrap();
        stream.write_all(b"250 Ok\r\n").await.unwrap();

        // DATA
        let _ = stream.read(&mut buf).await.unwrap();
        stream.write_all(b"354 Start mail input\r\n").await.unwrap();

        // Read EML content until CRLF.CRLF
        let n = stream.read(&mut buf).await.unwrap();
        let received_eml = String::from_utf8_lossy(&buf[..n]).to_string();

        stream
            .write_all(b"250 Ok: queued as 12345\r\n")
            .await
            .unwrap();

        // QUIT
        let _ = stream.read(&mut buf).await.unwrap();
        stream.write_all(b"221 Bye\r\n").await.unwrap();

        received_eml
    });

    // 2. Configure IMAP and SMTP settings for User
    let account = mail_service
        .create_account(
            ctx.tenant_id,
            user.id,
            "IMAP".to_string(),
            "127.0.0.1".to_string(),
            993,
            "sender@example.com".to_string(),
            "imappass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();

    mail_service
        .create_or_update_smtp_settings(
            ctx.tenant_id,
            user.id,
            account.id,
            "127.0.0.1".to_string(),
            smtp_port,
            "smtpuser".to_string(),
            Some("smtppass".to_string()),
            MailTlsMode::None,
            "sender@example.com".to_string(),
            Some("Sender Name".to_string()),
            None,
            None, // No sent append in this test case
            true,
        )
        .await
        .unwrap();

    // 3. Send outbound mail
    let idempotency_key = Uuid::new_v4();
    let preflight_result = mail_service
        .send_outbound_mail(
            ctx.tenant_id,
            user.id,
            account.id,
            vec!["recipient@example.com".to_string()],
            vec![],
            vec![],
            "Test Subject".to_string(),
            "Test body content".to_string(),
            None,
            vec![Uuid::new_v4()],
            None,
            false,
            Some(idempotency_key),
        )
        .await;
    let preflight_error = match preflight_result {
        Ok(_) => panic!("missing attachment should fail before claiming the send key"),
        Err(error) => error,
    };
    assert!(matches!(
        preflight_error,
        rustshare_server::services::mail_service::MailError::PermissionDenied
    ));

    let outbound_msg = mail_service
        .send_outbound_mail(
            ctx.tenant_id,
            user.id,
            account.id,
            vec!["recipient@example.com".to_string()],
            vec![],
            vec![],
            "Test Subject".to_string(),
            "Test body content".to_string(),
            None,
            vec![], // No attachments
            None,   // Not a reply
            false,
            Some(idempotency_key),
        )
        .await
        .unwrap();
    let repeated = mail_service
        .send_outbound_mail(
            ctx.tenant_id,
            user.id,
            account.id,
            vec!["recipient@example.com".to_string()],
            vec![],
            vec![],
            "Test Subject".to_string(),
            "Test body content".to_string(),
            None,
            vec![],
            None,
            false,
            Some(idempotency_key),
        )
        .await
        .unwrap();

    // 4. Verify DB mail message is saved as outbound
    let outbound_msg = outbound_msg
        .message
        .expect("sent artifact should be stored");
    assert_eq!(
        repeated.message.map(|message| message.id),
        Some(outbound_msg.id)
    );
    assert_eq!(outbound_msg.source_mode, "outbound");
    assert_eq!(outbound_msg.subject, Some("Test Subject".to_string()));

    // 5. Verify the received EML on the mock SMTP server
    let received_eml = server_task.await.unwrap();
    assert!(received_eml.contains("Subject: Test Subject"));
    assert!(received_eml.contains("Test body content"));
    assert!(received_eml.contains("sender@example.com"));
    assert!(received_eml.contains("recipient@example.com"));

    // Clean up
    cleanup_user(&ctx.pool, user.id).await;
    cleanup_tenant(&ctx.pool, ctx.tenant_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and S3-compatible object storage"]
async fn stale_pending_send_claim_is_reclaimed() {
    let ctx = setup_test_env().await;
    enable_internal_smtp_for_tests();
    let mail_service = ctx.mail_service();
    let user = ctx.create_test_user("smtp_stale_claim").await;

    // Minimal mock SMTP server that accepts one message.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let smtp_port = listener.local_addr().unwrap().port() as i32;
    let server_task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0; 4096];
        stream.write_all(b"220 localhost ESMTP\r\n").await.unwrap();
        let _ = stream.read(&mut buf).await.unwrap();
        stream
            .write_all(b"250-localhost\r\n250 AUTH PLAIN\r\n")
            .await
            .unwrap();
        let _ = stream.read(&mut buf).await.unwrap();
        stream
            .write_all(b"235 Authentication successful\r\n")
            .await
            .unwrap();
        let _ = stream.read(&mut buf).await.unwrap();
        stream.write_all(b"250 Ok\r\n").await.unwrap();
        let _ = stream.read(&mut buf).await.unwrap();
        stream.write_all(b"250 Ok\r\n").await.unwrap();
        let _ = stream.read(&mut buf).await.unwrap();
        stream.write_all(b"354 Start mail input\r\n").await.unwrap();
        let _ = stream.read(&mut buf).await.unwrap();
        stream
            .write_all(b"250 Ok: queued as 12345\r\n")
            .await
            .unwrap();
        let _ = stream.read(&mut buf).await.unwrap();
        stream.write_all(b"221 Bye\r\n").await.unwrap();
    });

    let account = mail_service
        .create_account(
            ctx.tenant_id,
            user.id,
            "IMAP".to_string(),
            "127.0.0.1".to_string(),
            993,
            "sender@example.com".to_string(),
            "imappass".to_string(),
            MailTlsMode::Tls,
        )
        .await
        .unwrap();
    mail_service
        .create_or_update_smtp_settings(
            ctx.tenant_id,
            user.id,
            account.id,
            "127.0.0.1".to_string(),
            smtp_port,
            "smtpuser".to_string(),
            Some("smtppass".to_string()),
            MailTlsMode::None,
            "sender@example.com".to_string(),
            Some("Sender Name".to_string()),
            None,
            None,
            true,
        )
        .await
        .unwrap();

    // Simulate a crashed send: a pending claim from an hour ago.
    let idempotency_key = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO mail_send_idempotency
         (tenant_id, owner_id, account_id, idempotency_key, status, created_at)
         VALUES ($1, $2, $3, $4, 'pending', NOW() - INTERVAL '1 hour')",
    )
    .bind(ctx.tenant_id)
    .bind(user.id)
    .bind(account.id)
    .bind(idempotency_key)
    .execute(&ctx.pool)
    .await
    .unwrap();

    let sent = mail_service
        .send_outbound_mail(
            ctx.tenant_id,
            user.id,
            account.id,
            vec!["recipient@example.com".to_string()],
            vec![],
            vec![],
            "Reclaimed Subject".to_string(),
            "Body".to_string(),
            None,
            vec![],
            None,
            false,
            Some(idempotency_key),
        )
        .await
        .expect("stale pending claim should be reclaimed and the send retried");

    assert!(sent.message.is_some());
    server_task.await.unwrap();

    ctx.cleanup().await;
}
