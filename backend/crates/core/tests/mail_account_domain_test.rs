use rustshare_core::domain::{MailAccount, MailImportJob, MailTlsMode};
use uuid::Uuid;

#[test]
fn mail_account_defaults() {
    let account = MailAccount::new(
        Uuid::nil(),
        Uuid::new_v4(),
        "Work Gmail".to_string(),
        "imap.gmail.com".to_string(),
        993,
        "user@example.com".to_string(),
        "enc".to_string(),
        MailTlsMode::Tls,
    );
    assert!(account.is_enabled);
    assert_eq!(account.tls_mode, "tls");
}

#[test]
fn mail_tls_mode_roundtrip() {
    assert_eq!(MailTlsMode::StartTls.to_string(), "starttls");
    assert_eq!(
        "starttls".parse::<MailTlsMode>().unwrap(),
        MailTlsMode::StartTls
    );
}

#[test]
fn mail_import_job_defaults() {
    let job = MailImportJob::new(
        Uuid::nil(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "INBOX".to_string(),
        vec![1, 2, 3],
    );
    assert_eq!(job.status, "pending");
    assert_eq!(job.total_messages, 3);
}
