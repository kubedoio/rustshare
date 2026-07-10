use chrono::Utc;
use rustshare_core::domain::MailImportJob;
use uuid::Uuid;

#[test]
fn archive_job_defaults_to_imap_archive_source_mode() {
    let job = MailImportJob::new_archive(
        Uuid::nil(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "INBOX".to_string(),
        None,
        None,
        Some(365),
        3,
    );
    assert_eq!(job.source_mode, "imap_archive");
    assert_eq!(job.retry_count, 0);
    assert_eq!(job.max_retries, 3);
    assert_eq!(job.retention_days, Some(365));
    assert_eq!(job.selected_uids, None);
}

#[test]
fn archive_job_stores_date_range_and_defaults() {
    let since = Utc::now();
    let before = since + chrono::Duration::days(1);
    let job = MailImportJob::new_archive(
        Uuid::nil(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        "Archive".to_string(),
        Some(since),
        Some(before),
        Some(90),
        5,
    );
    assert_eq!(job.folder_name, "Archive");
    assert_eq!(job.archive_since, Some(since));
    assert_eq!(job.archive_before, Some(before));
    assert_eq!(job.last_uid_validity, None);
    assert_eq!(job.last_imported_uid, None);
}
