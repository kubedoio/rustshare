use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::UserId;

/// Per-user Application visibility preference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ApplicationUserPreference {
    #[schema(value_type = Uuid)]
    pub user_id: UserId,
    pub application_id: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ApplicationUserPreference {
    pub fn new(user_id: UserId, application_id: String, enabled: bool) -> Self {
        Self {
            user_id,
            application_id,
            enabled,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
