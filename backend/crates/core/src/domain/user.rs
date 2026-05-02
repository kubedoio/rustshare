use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::UserId;

/// User theme preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "text")]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Theme::Light => write!(f, "light"),
            Theme::Dark => write!(f, "dark"),
            Theme::System => write!(f, "system"),
        }
    }
}

impl std::str::FromStr for Theme {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "light" => Ok(Theme::Light),
            "dark" => Ok(Theme::Dark),
            "system" => Ok(Theme::System),
            _ => Err(format!("Invalid theme: {}", s)),
        }
    }
}

/// User's dashboard configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DashboardConfig {
    pub enabled_modules: Vec<String>,
    pub module_order: Vec<String>,
    pub sections: Vec<serde_json::Value>,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            enabled_modules: vec![
                "notes".to_string(),
                "kanban".to_string(),
                "brainstorming".to_string(),
            ],
            module_order: vec![
                "notes".to_string(),
                "kanban".to_string(),
                "brainstorming".to_string(),
            ],
            sections: vec![],
        }
    }
}


/// User account information.
///
/// Note: The `username` field is used for login and is distinct from `email`.
/// This allows users to have a stable login identifier separate from their email address.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: UserId,
    /// Login identifier (enhancement beyond spec)
    pub username: String,
    /// Display name shown in UI
    pub display_name: String,
    pub password_hash: String,
    pub email: String,
    pub is_admin: bool,
    /// Storage quota in bytes
    pub storage_quota: i64,
    /// Theme preference (light/dark/system)
    #[serde(default)]
    pub theme: Theme,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub disabled_at: Option<DateTime<Utc>>,
    /// User's first name
    pub name: Option<String>,
    /// User's last name
    pub surname: Option<String>,
    /// Path to avatar image
    pub avatar_path: Option<String>,
    /// Whether email can be shared with other users
    pub email_sharing_enabled: bool,
    /// Trash retention period in days (None = never auto-delete)
    pub trash_retention_days: Option<i32>,
    /// Tenant this user belongs to
    pub tenant_id: Uuid,
    /// User's dashboard configuration
    pub dashboard_config: sqlx::types::Json<DashboardConfig>,
}

impl User {
    /// Creates a new user account with the given credentials and settings.
    pub fn new(
        username: String,
        display_name: String,
        password_hash: String,
        email: String,
        is_admin: bool,
        storage_quota: i64,
        tenant_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            username,
            display_name,
            password_hash,
            email,
            is_admin,
            storage_quota,
            theme: Theme::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
            name: None,
            surname: None,
            avatar_path: None,
            email_sharing_enabled: true,
            trash_retention_days: Some(30),
            tenant_id,
            dashboard_config: sqlx::types::Json(DashboardConfig::default()),
        }
    }

    /// Updates the user's theme preference.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let tenant_id = Uuid::new_v4();
        let user = User::new(
            "alice".to_string(),
            "Alice Smith".to_string(),
            "hashed_password".to_string(),
            "alice@example.com".to_string(),
            false,
            10_737_418_240, // 10 GB
            tenant_id,
        );

        assert_eq!(user.username, "alice");
        assert_eq!(user.display_name, "Alice Smith");
        assert_eq!(user.email, "alice@example.com");
        assert!(!user.is_admin);
        assert_eq!(user.storage_quota, 10_737_418_240);
        assert_eq!(user.tenant_id, tenant_id);
        assert!(!user.id.is_nil());
    }

    #[test]
    fn test_admin_user_creation() {
        let tenant_id = Uuid::new_v4();
        let admin = User::new(
            "admin".to_string(),
            "Administrator".to_string(),
            "hashed_password".to_string(),
            "admin@example.com".to_string(),
            true,
            107_374_182_400, // 100 GB
            tenant_id,
        );

        assert!(admin.is_admin);
        assert_eq!(admin.tenant_id, tenant_id);
    }
}
