use rustshare_core::domain::MailMessage;
use uuid::Uuid;

#[test]
fn mail_message_defaults_to_private_visibility_and_eml_upload() {
    let msg = MailMessage::new(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), "eml_upload");
    assert_eq!(msg.visibility, "private");
    assert_eq!(msg.source_mode, "eml_upload");
    assert!(!msg.has_attachments);
    assert!(msg.deleted_at.is_none());
}

#[test]
fn mail_message_accepts_source_mode_enum() {
    use rustshare_core::domain::MailSourceMode;

    let msg = MailMessage::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        MailSourceMode::ImapSelected,
    );
    assert_eq!(msg.source_mode, "imap_selected");
}
