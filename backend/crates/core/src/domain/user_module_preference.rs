use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};


use super::UserId;

/// Per-user module preference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserModulePreference {
    pub user_id: UserId,
    pub module_key: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl UserModulePreference {
    pub fn new(user_id: UserId, module_key: String, enabled: bool) -> Self {
        Self {
            user_id,
            module_key,
            enabled,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
