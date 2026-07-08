use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::{MailMessageId, UserId};

/// Unique identifier for a mail link.
pub type MailLinkId = Uuid;

/// Discriminator for the type of RustShare object a mail message is linked to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkTargetType {
    Note,
    KanbanCard,
    KanbanBoard,
    Meeting,
    File,
    Folder,
    MailMessage,
}

impl LinkTargetType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LinkTargetType::Note => "note",
            LinkTargetType::KanbanCard => "kanban_card",
            LinkTargetType::KanbanBoard => "kanban_board",
            LinkTargetType::Meeting => "meeting",
            LinkTargetType::File => "file",
            LinkTargetType::Folder => "folder",
            LinkTargetType::MailMessage => "mail_message",
        }
    }
}

impl std::fmt::Display for LinkTargetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for LinkTargetType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "note" => Ok(LinkTargetType::Note),
            "kanban_card" => Ok(LinkTargetType::KanbanCard),
            "kanban_board" => Ok(LinkTargetType::KanbanBoard),
            "meeting" => Ok(LinkTargetType::Meeting),
            "file" => Ok(LinkTargetType::File),
            "folder" => Ok(LinkTargetType::Folder),
            "mail_message" => Ok(LinkTargetType::MailMessage),
            _ => Err(format!("unknown link target type: {s}")),
        }
    }
}

impl From<LinkTargetType> for String {
    fn from(target_type: LinkTargetType) -> Self {
        target_type.to_string()
    }
}

/// Join row representing a link between a Mail message and another RustShare object.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MailLink {
    pub id: MailLinkId,
    pub tenant_id: Uuid,
    pub message_id: MailMessageId,
    pub target_type: String,
    pub target_id: Uuid,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl MailLink {
    pub fn new(
        tenant_id: Uuid,
        message_id: MailMessageId,
        created_by: UserId,
        target_type: impl Into<String>,
        target_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            message_id,
            target_type: target_type.into(),
            target_id,
            created_by,
            created_at: Utc::now(),
            deleted_at: None,
        }
    }
}
