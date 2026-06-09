use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A workspace module configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Module {
    pub id: Uuid,
    #[serde(alias = "key")]
    pub module_key: String,
    #[serde(alias = "displayName")]
    pub display_name: String,
    pub description: String,
    pub enabled: bool,
    #[serde(alias = "rootPath")]
    pub root_path: String,
    pub renderer: String,
    #[serde(alias = "defaultTemplate")]
    pub default_template: Option<String>,
    pub icon: String,
    #[serde(alias = "schemaVersion")]
    pub schema_version: String,
    pub permissions: serde_json::Value,
    #[serde(alias = "aiIndexing")]
    pub ai_indexing: serde_json::Value,
    pub audit: serde_json::Value,
    #[serde(alias = "ui")]
    pub ui_config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tenant_id: Uuid,
}

/// Permission settings for a module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AiIndexingPolicy {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Audit policy for a module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
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
            root_path: "/Workspace/Notes".to_string(),
            renderer: "notes".to_string(),
            default_template: Some("template_default_note".to_string()),
            icon: "sticky-note".to_string(),
            schema_version: "1.0".to_string(),
            permissions: serde_json::json!({"admin_can_configure": true}),
            ai_indexing: serde_json::json!({"enabled": true}),
            audit: serde_json::json!({"enabled": true}),
            ui_config: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tenant_id: Uuid::nil(),
        };

        assert_eq!(module.module_key, "notes");
        assert!(module.enabled);
        assert_eq!(module.root_path, "/Workspace/Notes");
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
        let perms: ModulePermissions = serde_json::from_value(serde_json::json!({
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

    #[test]
    fn test_module_deserialization_legacy_aliases() {
        let json = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000000",
            "key": "notes",
            "displayName": "Notes",
            "description": "Test",
            "enabled": true,
            "rootPath": "/Workspace/Notes",
            "renderer": "notes",
            "defaultTemplate": "template_default_note",
            "icon": "sticky-note",
            "schemaVersion": "1.0",
            "permissions": {},
            "aiIndexing": {"enabled": true},
            "audit": {"enabled": true},
            "ui": {},
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "tenant_id": "00000000-0000-0000-0000-000000000000"
        });
        let module: Module = serde_json::from_value(json).unwrap();
        assert_eq!(module.module_key, "notes");
        assert_eq!(module.display_name, "Notes");
        assert_eq!(module.root_path, "/Workspace/Notes");
        assert_eq!(
            module.default_template,
            Some("template_default_note".to_string())
        );
        assert_eq!(module.schema_version, "1.0");
        assert_eq!(module.ai_indexing, serde_json::json!({"enabled": true}));
        assert_eq!(module.ui_config, serde_json::json!({}));
    }

    #[test]
    fn test_module_serialization_contract() {
        // This test asserts the canonical JSON shape that the API produces.
        // If the backend drifts from the contract (e.g. changes field names
        // or omits required keys), this test will fail.
        let module = Module {
            id: Uuid::nil(),
            module_key: "notes".to_string(),
            display_name: "Notes".to_string(),
            description: "Capture file-backed notes.".to_string(),
            enabled: true,
            root_path: "/Workspace/Notes".to_string(),
            renderer: "notes".to_string(),
            default_template: Some("template_default_note".to_string()),
            icon: "sticky-note".to_string(),
            schema_version: "1.0".to_string(),
            permissions: serde_json::json!({
                "admin_can_configure": true,
                "workspace_members_can_use": true,
                "allow_public_share": false,
                "allow_internal_share": true
            }),
            ai_indexing: serde_json::json!({"enabled": true}),
            audit: serde_json::json!({"enabled": true}),
            ui_config: serde_json::json!({
                "sidebar": {
                    "enabled": true,
                    "order": 30,
                    "icon": "sticky-note",
                    "label": "Notes"
                },
                "dashboard": {
                    "enabled": true,
                    "order": 10,
                    "cardTitle": "Notes",
                    "cardDescription": "Recent file-backed notes.",
                    "summaryMode": "recent-items",
                    "maxItems": 4,
                    "primaryAction": {
                        "label": "New note",
                        "action": "create-from-template",
                        "template": "template_default_note"
                    },
                    "widget": {
                        "enabled": true,
                        "type": "latest-notes",
                        "title": "Notes",
                        "description": "Recent file-backed notes.",
                        "size": "small",
                        "columns": {"desktop": 3, "tablet": 6, "mobile": 12},
                        "maxItems": 4,
                        "primaryAction": {
                            "label": "New note",
                            "action": "create-from-template",
                            "template": "template_default_note"
                        }
                    }
                },
                "modulePage": {
                    "layout": "list-grid",
                    "emptyStateTitle": "No notes yet",
                    "emptyStateDescription": "Create your first file-backed note.",
                    "emptyStateAction": "New note"
                },
                "page": {
                    "enabled": true,
                    "route": "/modules/notes",
                    "renderer": "notes",
                    "layout": "list-grid",
                    "emptyStateTitle": "No notes yet",
                    "emptyStateDescription": "Create your first file-backed note.",
                    "emptyStateAction": "New note",
                    "primaryAction": {
                        "label": "New note",
                        "action": "create-from-template",
                        "template": "template_default_note"
                    },
                    "searchPlaceholder": "Search notes...",
                    "filterLabel": "All notes",
                    "sortLabel": "Modified",
                    "itemSingular": "notes",
                    "itemPlural": "notes"
                }
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tenant_id: Uuid::nil(),
        };

        let json = serde_json::to_value(&module).unwrap();
        let obj = json.as_object().unwrap();

        // Canonical snake_case field names must be present
        assert!(obj.contains_key("id"), "missing id");
        assert!(obj.contains_key("module_key"), "missing module_key");
        assert!(obj.contains_key("display_name"), "missing display_name");
        assert!(obj.contains_key("root_path"), "missing root_path");
        assert!(
            obj.contains_key("default_template"),
            "missing default_template"
        );
        assert!(obj.contains_key("schema_version"), "missing schema_version");
        assert!(obj.contains_key("ai_indexing"), "missing ai_indexing");
        assert!(obj.contains_key("ui_config"), "missing ui_config");
        assert!(obj.contains_key("created_at"), "missing created_at");
        assert!(obj.contains_key("updated_at"), "missing updated_at");
        assert!(obj.contains_key("tenant_id"), "missing tenant_id");

        // Canonical root path must not drift to legacy
        assert_eq!(
            obj.get("root_path").unwrap().as_str().unwrap(),
            "/Workspace/Notes"
        );

        // Permissions must serialize with snake_case keys
        let perms = obj.get("permissions").unwrap().as_object().unwrap();
        assert!(perms.contains_key("admin_can_configure"));
        assert!(perms.contains_key("workspace_members_can_use"));
        assert!(perms.contains_key("allow_public_share"));
        assert!(perms.contains_key("allow_internal_share"));
    }
}
