use rustshare_core::domain::{LinkTargetType, MailLink};
use uuid::Uuid;

#[test]
fn link_target_type_round_trips_strings() {
    let cases = vec![
        (LinkTargetType::Note, "note"),
        (LinkTargetType::KanbanCard, "kanban_card"),
        (LinkTargetType::KanbanBoard, "kanban_board"),
        (LinkTargetType::Meeting, "meeting"),
        (LinkTargetType::File, "file"),
        (LinkTargetType::Folder, "folder"),
        (LinkTargetType::MailMessage, "mail_message"),
    ];

    for (variant, expected) in cases {
        assert_eq!(variant.as_str(), expected);
        assert_eq!(expected.parse::<LinkTargetType>().unwrap(), variant);
    }
}

#[test]
fn mail_link_defaults_to_no_deleted_at() {
    let link = MailLink::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        LinkTargetType::Note,
        Uuid::new_v4(),
    );

    assert!(link.deleted_at.is_none());
    assert_eq!(link.target_type, "note");
}
