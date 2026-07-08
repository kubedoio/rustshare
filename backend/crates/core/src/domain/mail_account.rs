use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{MailAccountId, MailImportJobId, UserId};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailTlsMode {
    #[default]
    Tls,
    StartTls,
    None,
}

impl std::fmt::Display for MailTlsMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MailTlsMode::Tls => f.write_str("tls"),
            MailTlsMode::StartTls => f.write_str("starttls"),
            MailTlsMode::None => f.write_str("none"),
        }
    }
}

impl std::str::FromStr for MailTlsMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tls" => Ok(MailTlsMode::Tls),
            "starttls" => Ok(MailTlsMode::StartTls),
            "none" => Ok(MailTlsMode::None),
            _ => Err(format!("Invalid mail TLS mode: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailImportJobStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl From<MailImportJobStatus> for String {
    fn from(status: MailImportJobStatus) -> Self {
        match status {
            MailImportJobStatus::Pending => "pending".to_string(),
            MailImportJobStatus::Running => "running".to_string(),
            MailImportJobStatus::Completed => "completed".to_string(),
            MailImportJobStatus::Failed => "failed".to_string(),
            MailImportJobStatus::Cancelled => "cancelled".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MailAccount {
    #[schema(value_type = Uuid)]
    pub id: MailAccountId,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
    pub name: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_enc: String,
    pub tls_mode: String,
    pub is_enabled: bool,
    pub last_error: Option<String>,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MailAccount {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id: Uuid,
        owner_id: UserId,
        name: String,
        host: String,
        port: i32,
        username: String,
        password_enc: String,
        tls_mode: MailTlsMode,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            owner_id,
            name,
            host,
            port,
            username,
            password_enc,
            tls_mode: tls_mode.to_string(),
            is_enabled: true,
            last_error: None,
            last_connected_at: None,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MailImportJob {
    #[schema(value_type = Uuid)]
    pub id: MailImportJobId,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
    #[schema(value_type = Uuid)]
    pub account_id: MailAccountId,
    pub source_mode: String,
    pub folder_name: String,
    pub selected_uids: Option<Vec<i64>>,
    pub status: String,
    pub total_messages: i32,
    pub processed_messages: i32,
    pub failed_messages: i32,
    pub last_error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MailImportJob {
    pub fn new(
        tenant_id: Uuid,
        owner_id: UserId,
        account_id: MailAccountId,
        folder_name: String,
        selected_uids: Vec<i64>,
    ) -> Self {
        let now = Utc::now();
        let total_messages = selected_uids.len() as i32;
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            owner_id,
            account_id,
            source_mode: "imap_selected".to_string(),
            folder_name,
            selected_uids: Some(selected_uids),
            status: MailImportJobStatus::Pending.into(),
            total_messages,
            processed_messages: 0,
            failed_messages: 0,
            last_error: None,
            started_at: None,
            completed_at: None,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}
