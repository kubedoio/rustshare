use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A workspace module configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Module {
    pub id: Uuid,
    pub module_key: String,
    pub display_name: String,
    pub description: String,
    pub enabled: bool,
    pub root_path: String,
    pub renderer: String,
    pub default_template: Option<String>,
    pub icon: String,
    pub schema_version: String,
    pub permissions: serde_json::Value,
    pub ai_indexing: serde_json::Value,
    pub audit: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tenant_id: Uuid,
}

/// Permission settings for a module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModulePermissions {
    #[serde(default)]
    pub admin_can_configure: bool,
    #[serde(default)]
    pub workspace_members_can_use: bool,
    #[serde(default)]
    pub allow_public_share: bool,
    #[serde(default)]
    pub allow_internal_share: bool,
}

/// AI indexing policy for a module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiIndexingPolicy {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Audit policy for a module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditPolicy {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_creation() {
        let module = Module {
            id: Uuid::new_v4(),
            module_key: "notes".to_string(),
            display_name: "Notes".to_string(),
            description: "Capture file-backed notes.".to_string(),
            enabled: true,
            root_path: "/Notes".to_string(),
            renderer: "notes".to_string(),
            default_template: Some("template_default_note".to_string()),
            icon: "file-text".to_string(),
            schema_version: "1.0".to_string(),
            permissions: serde_json::json!({"admin_can_configure": true}),
            ai_indexing: serde_json::json!({"enabled": true}),
            audit: serde_json::json!({"enabled": true}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tenant_id: Uuid::nil(),
        };

        assert_eq!(module.module_key, "notes");
        assert!(module.enabled);
    }

    #[test]
    fn test_module_permissions_defaults() {
        let perms: ModulePermissions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!perms.admin_can_configure);
        assert!(!perms.workspace_members_can_use);
        assert!(!perms.allow_public_share);
        assert!(!perms.allow_internal_share);
    }

    #[test]
    fn test_module_permissions_parsing() {
        let perms: ModulePermissions =
            serde_json::from_value(serde_json::json!({
                "admin_can_configure": true,
                "workspace_members_can_use": true,
                "allow_public_share": false,
                "allow_internal_share": true
            }))
            .unwrap();
        assert!(perms.admin_can_configure);
        assert!(perms.workspace_members_can_use);
        assert!(!perms.allow_public_share);
        assert!(perms.allow_internal_share);
    }
}
