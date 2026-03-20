use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, str::FromStr};

use super::{FileId, VersionId};

pub type ReplicationJobId = uuid::Uuid;
pub type ReplicationTargetId = uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicationTarget {
    pub id: ReplicationTargetId,
    pub name: String,
    pub destination_type: String,
    pub endpoint: String,
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub base_path: Option<String>,
    pub is_required: bool,
    pub enabled: bool,
    pub auth_config: Option<Value>,
    pub health_status: String,
    pub last_healthy_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationJobStatus {
    #[default]
    Queued,
    Syncing,
    Retrying,
    Completed,
    Failed,
}

impl ReplicationJobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Syncing => "syncing",
            Self::Retrying => "retrying",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl fmt::Display for ReplicationJobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReplicationJobStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "syncing" => Ok(Self::Syncing),
            "retrying" => Ok(Self::Retrying),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("Unknown replication job status: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicationJob {
    pub id: ReplicationJobId,
    pub file_id: FileId,
    pub file_version_id: VersionId,
    pub storage_key: String,
    pub status: ReplicationJobStatus,
    pub attempt_count: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub leased_at: Option<DateTime<Utc>>,
    pub lease_token: Option<uuid::Uuid>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ReplicationJob {
    pub fn new(file_id: FileId, file_version_id: VersionId, storage_key: String) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4(),
            file_id,
            file_version_id,
            storage_key,
            status: ReplicationJobStatus::Queued,
            attempt_count: 0,
            next_attempt_at: now,
            last_attempt_at: None,
            leased_at: None,
            lease_token: None,
            last_error: None,
            created_at: now,
            updated_at: now,
        }
    }
}
