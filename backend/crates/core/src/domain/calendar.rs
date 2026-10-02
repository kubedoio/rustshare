use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

use super::UserId;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CalendarSourceKind {
    #[default]
    Internal,
    IcalImport,
    Google,
    Outlook,
}

impl CalendarSourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CalendarSourceKind::Internal => "internal",
            CalendarSourceKind::IcalImport => "ical_import",
            CalendarSourceKind::Google => "google",
            CalendarSourceKind::Outlook => "outlook",
        }
    }
}

impl From<CalendarSourceKind> for String {
    fn from(kind: CalendarSourceKind) -> Self {
        kind.as_str().to_string()
    }
}

impl std::str::FromStr for CalendarSourceKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "internal" => Ok(CalendarSourceKind::Internal),
            "ical_import" => Ok(CalendarSourceKind::IcalImport),
            "google" => Ok(CalendarSourceKind::Google),
            "outlook" => Ok(CalendarSourceKind::Outlook),
            _ => Err(format!("Invalid calendar source kind: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CalendarSourceStatus {
    #[default]
    Healthy,
    Degraded,
    AuthRequired,
    RateLimited,
    Paused,
    Failed,
}

impl CalendarSourceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CalendarSourceStatus::Healthy => "healthy",
            CalendarSourceStatus::Degraded => "degraded",
            CalendarSourceStatus::AuthRequired => "auth_required",
            CalendarSourceStatus::RateLimited => "rate_limited",
            CalendarSourceStatus::Paused => "paused",
            CalendarSourceStatus::Failed => "failed",
        }
    }
}

impl From<CalendarSourceStatus> for String {
    fn from(status: CalendarSourceStatus) -> Self {
        status.as_str().to_string()
    }
}

impl std::str::FromStr for CalendarSourceStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "healthy" => Ok(CalendarSourceStatus::Healthy),
            "degraded" => Ok(CalendarSourceStatus::Degraded),
            "auth_required" => Ok(CalendarSourceStatus::AuthRequired),
            "rate_limited" => Ok(CalendarSourceStatus::RateLimited),
            "paused" => Ok(CalendarSourceStatus::Paused),
            "failed" => Ok(CalendarSourceStatus::Failed),
            _ => Err(format!("Invalid calendar source status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CalendarEventStatus {
    #[default]
    Confirmed,
    Tentative,
    Cancelled,
}

impl CalendarEventStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CalendarEventStatus::Confirmed => "confirmed",
            CalendarEventStatus::Tentative => "tentative",
            CalendarEventStatus::Cancelled => "cancelled",
        }
    }
}

impl From<CalendarEventStatus> for String {
    fn from(status: CalendarEventStatus) -> Self {
        status.as_str().to_string()
    }
}

impl std::str::FromStr for CalendarEventStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "confirmed" => Ok(CalendarEventStatus::Confirmed),
            "tentative" => Ok(CalendarEventStatus::Tentative),
            "cancelled" => Ok(CalendarEventStatus::Cancelled),
            _ => Err(format!("Invalid calendar event status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CalendarImportJobStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl CalendarImportJobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CalendarImportJobStatus::Pending => "pending",
            CalendarImportJobStatus::Running => "running",
            CalendarImportJobStatus::Completed => "completed",
            CalendarImportJobStatus::Failed => "failed",
            CalendarImportJobStatus::Cancelled => "cancelled",
        }
    }
}

impl From<CalendarImportJobStatus> for String {
    fn from(status: CalendarImportJobStatus) -> Self {
        status.as_str().to_string()
    }
}

impl std::str::FromStr for CalendarImportJobStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(CalendarImportJobStatus::Pending),
            "running" => Ok(CalendarImportJobStatus::Running),
            "completed" => Ok(CalendarImportJobStatus::Completed),
            "failed" => Ok(CalendarImportJobStatus::Failed),
            "cancelled" => Ok(CalendarImportJobStatus::Cancelled),
            _ => Err(format!("Invalid calendar import job status: {s}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct CalendarSource {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
    pub kind: String,
    pub display_name: String,
    pub external_account: Option<String>,
    pub external_calendar_id: Option<String>,
    #[serde(skip_serializing)]
    pub refresh_token_enc: Option<String>,
    #[serde(skip_serializing)]
    pub access_token_enc: Option<String>,
    pub access_token_expires_at: Option<DateTime<Utc>>,
    pub scopes: Option<String>,
    pub is_enabled: bool,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub status: String,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct CalendarEvent {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
    #[schema(value_type = Uuid)]
    pub source_id: Uuid,
    pub external_uid: Option<String>,
    pub external_etag: Option<String>,
    pub recurrence_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub location: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub all_day: bool,
    pub original_date: Option<NaiveDate>,
    pub timezone: String,
    pub rrule: Option<String>,
    pub status: String,
    pub read_only: bool,
    #[serde(skip_serializing)]
    pub raw: Option<serde_json::Value>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct CalendarSyncState {
    #[schema(value_type = Uuid)]
    pub source_id: Uuid,
    pub next_sync_at: DateTime<Utc>,
    pub locked_at: Option<DateTime<Utc>>,
    pub locked_by: Option<String>,
    pub cursor_kind: Option<String>,
    pub cursor_value: Option<String>,
    pub cursor_expires_at: Option<DateTime<Utc>>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct CalendarOauthState {
    pub state: String,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
    pub kind: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow, ToSchema)]
pub struct CalendarImportJob {
    #[schema(value_type = Uuid)]
    pub id: Uuid,
    pub tenant_id: Uuid,
    #[schema(value_type = Uuid)]
    pub owner_id: UserId,
    #[schema(value_type = Uuid)]
    pub source_id: Uuid,
    pub status: String,
    pub filename: String,
    pub size_bytes: i64,
    pub total_events: i32,
    pub processed_events: i32,
    pub failed_events: i32,
    pub last_error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
