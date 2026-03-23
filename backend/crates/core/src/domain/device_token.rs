use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct DevicePairRequest {
    pub id: Uuid,
    pub device_code: String,
    pub user_code: String,
    pub user_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeviceToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub device_name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}
