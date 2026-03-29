use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::UserId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserSession {
    pub id: uuid::Uuid,
    pub user_id: UserId,
    pub session_token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub tenant_id: Uuid,
}

impl UserSession {
    pub fn new(
        user_id: UserId,
        session_token_hash: String,
        ttl_seconds: i64,
        user_agent: Option<String>,
        ip_address: Option<String>,
        tenant_id: Uuid,
    ) -> Self {
        let now = Utc::now();

        Self {
            id: uuid::Uuid::new_v4(),
            user_id,
            session_token_hash,
            expires_at: now + Duration::seconds(ttl_seconds),
            created_at: now,
            last_seen_at: now,
            user_agent,
            ip_address,
            tenant_id,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
}
