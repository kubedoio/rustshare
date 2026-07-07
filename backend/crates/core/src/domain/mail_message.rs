use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{MailAttachmentId, MailMessageId, MailMessagePartId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailSourceMode {
    EmlUpload,
    ImapSelected,
    ImapArchive,
    InboundAddress,
}

impl Default for MailSourceMode {
    fn default() -> Self {
        Self::EmlUpload
    }
}

impl From<MailSourceMode> for String {
    fn from(mode: MailSourceMode) -> Self {
        match mode {
            MailSourceMode::EmlUpload => "eml_upload".to_string(),
            MailSourceMode::ImapSelected => "imap_selected".to_string(),
            MailSourceMode::ImapArchive => "imap_archive".to_string(),
            MailSourceMode::InboundAddress => "inbound_address".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailVisibility {
    Private,
    Workspace,
    Project,
    AdminArchive,
}

impl Default for MailVisibility {
    fn default() -> Self {
        Self::Private
    }
}

impl From<MailVisibility> for String {
    fn from(visibility: MailVisibility) -> Self {
        match visibility {
            MailVisibility::Private => "private".to_string(),
            MailVisibility::Workspace => "workspace".to_string(),
            MailVisibility::Project => "project".to_string(),
            MailVisibility::AdminArchive => "admin_archive".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MailMessage {
    #[schema(value_type = Uuid)]
    pub id: MailMessageId,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
    pub source_mode: String,
    pub source_folder: Option<String>,
    pub source_uid: Option<i64>,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    #[sqlx(rename = "reference_ids")]
    pub references: Option<Vec<String>>,
    pub subject: Option<String>,
    pub from_address: Option<String>,
    pub from_name: Option<String>,
    pub to_addresses: serde_json::Value,
    pub cc_addresses: serde_json::Value,
    pub bcc_addresses: serde_json::Value,
    pub sent_at: Option<DateTime<Utc>>,
    pub imported_at: DateTime<Utc>,
    #[schema(value_type = Uuid)]
    pub imported_by: UserId,
    pub visibility: String,
    pub object_key: Option<String>,
    pub blob_key: Option<String>,
    pub blob_sha256: Option<String>,
    pub size_bytes: Option<i64>,
    pub has_attachments: bool,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MailMessage {
    pub fn new(
        tenant_id: Uuid,
        owner_id: UserId,
        imported_by: UserId,
        source_mode: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            owner_id,
            source_mode: source_mode.into(),
            source_folder: None,
            source_uid: None,
            message_id: None,
            in_reply_to: None,
            references: None,
            subject: None,
            from_address: None,
            from_name: None,
            to_addresses: serde_json::Value::Array(vec![]),
            cc_addresses: serde_json::Value::Array(vec![]),
            bcc_addresses: serde_json::Value::Array(vec![]),
            sent_at: None,
            imported_at: now,
            imported_by,
            visibility: "private".to_string(),
            object_key: None,
            blob_key: None,
            blob_sha256: None,
            size_bytes: None,
            has_attachments: false,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MailMessagePart {
    #[schema(value_type = Uuid)]
    pub id: MailMessagePartId,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub message_id: MailMessageId,
    pub part_index: i32,
    pub content_type: String,
    pub charset: Option<String>,
    pub blob_key: Option<String>,
    pub blob_sha256: Option<String>,
    pub size_bytes: Option<i64>,
    pub is_body: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MailAttachment {
    #[schema(value_type = Uuid)]
    pub id: MailAttachmentId,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub message_id: MailMessageId,
    #[schema(value_type = Option<Uuid>)]
    pub file_id: Option<Uuid>,
    pub filename: String,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub part_index: Option<i32>,
    pub content_disposition: Option<String>,
    pub blob_key: Option<String>,
    pub created_at: DateTime<Utc>,
}
