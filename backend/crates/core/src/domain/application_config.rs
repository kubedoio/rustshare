use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Persisted configuration for an enabled Application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ApplicationConfig {
    pub id: Uuid,
    pub application_id: String,
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
    pub ui_config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tenant_id: Uuid,
}

impl ApplicationConfig {
    /// Extract the OKF module config from `ui_config.okf`, if present.
    pub fn okf_config(&self) -> ApplicationOkfConfig {
        self.ui_config
            .get("okf")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default()
    }
}

/// Permission settings for a module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApplicationPermissions {
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

/// OKF module configuration stored inside `ui_config.okf`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApplicationOkfConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, rename = "conceptType")]
    pub concept_type: Option<String>,
    #[serde(default, rename = "frontmatterRequired")]
    pub frontmatter_required: bool,
    #[serde(default, rename = "preserveUnknownFields")]
    pub preserve_unknown_fields: bool,
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
        let module = ApplicationConfig {
            id: Uuid::new_v4(),
            application_id: "io.elembra.notes".to_string(),
            display_name: "Notes".to_string(),
            description: "Write OKF-compatible, file-backed notes for durable company memory."
                .to_string(),
            enabled: true,
            root_path: "/Workspace/Notes".to_string(),
            renderer: "okf-note".to_string(),
            default_template: Some("template_default_okf_note".to_string()),
            icon: "sticky-note".to_string(),
            schema_version: "1.0".to_string(),
            permissions: serde_json::json!({"admin_can_configure": true}),
            ai_indexing: serde_json::json!({"enabled": true, "source": "okf-frontmatter-and-markdown", "permission_aware": true}),
            audit: serde_json::json!({"enabled": true}),
            ui_config: serde_json::json!({
                "documentFormat": "okf-markdown",
                "okf": {"enabled": true, "conceptType": "Note", "frontmatterRequired": true, "preserveUnknownFields": true}
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tenant_id: Uuid::nil(),
        };

        assert_eq!(module.application_id, "io.elembra.notes");
        assert!(module.enabled);
        assert_eq!(module.root_path, "/Workspace/Notes");
        assert_eq!(module.renderer, "okf-note");
        assert_eq!(
            module.default_template,
            Some("template_default_okf_note".to_string())
        );

        let okf = module.okf_config();
        assert!(okf.enabled);
        assert_eq!(okf.concept_type, Some("Note".to_string()));
        assert!(okf.frontmatter_required);
        assert!(okf.preserve_unknown_fields);
    }

    #[test]
    fn test_application_permissions_defaults() {
        let perms: ApplicationPermissions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!perms.admin_can_configure);
        assert!(!perms.workspace_members_can_use);
        assert!(!perms.allow_public_share);
        assert!(!perms.allow_internal_share);
    }

    #[test]
    fn test_application_permissions_parsing() {
        let perms: ApplicationPermissions = serde_json::from_value(serde_json::json!({
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
    fn test_module_serialization_contract() {
        // This test asserts the canonical JSON shape that the API produces.
        // If the backend drifts from the contract (e.g. changes field names
        // or omits required keys), this test will fail.
        let module = ApplicationConfig {
            id: Uuid::nil(),
            application_id: "io.elembra.notes".to_string(),
            display_name: "Notes".to_string(),
            description: "Write OKF-compatible, file-backed notes for durable company memory."
                .to_string(),
            enabled: true,
            root_path: "/Workspace/Notes".to_string(),
            renderer: "okf-note".to_string(),
            default_template: Some("template_default_okf_note".to_string()),
            icon: "sticky-note".to_string(),
            schema_version: "1.0".to_string(),
            permissions: serde_json::json!({
                "admin_can_configure": true,
                "workspace_members_can_use": true,
                "allow_public_share": false,
                "allow_internal_share": true
            }),
            ai_indexing: serde_json::json!({
                "enabled": true,
                "source": "okf-frontmatter-and-markdown",
                "permission_aware": true
            }),
            audit: serde_json::json!({"enabled": true}),
            ui_config: serde_json::json!({
                "documentFormat": "okf-markdown",
                "okf": {
                    "enabled": true,
                    "conceptType": "Note",
                    "frontmatterRequired": true,
                    "preserveUnknownFields": true
                },
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
                    "cardDescription": "Recent OKF notes.",
                    "summaryMode": "latest-notes",
                    "maxItems": 4,
                    "primaryAction": {
                        "label": "New note",
                        "action": "create-from-template",
                        "template": "template_default_okf_note"
                    },
                    "widget": {
                        "enabled": true,
                        "type": "latest-notes",
                        "title": "Notes",
                        "description": "Recent OKF notes.",
                        "size": "small",
                        "columns": {"desktop": 3, "tablet": 6, "mobile": 12},
                        "maxItems": 4,
                        "primaryAction": {
                            "label": "New note",
                            "action": "create-from-template",
                            "template": "template_default_okf_note"
                        }
                    }
                },
                "page": {
                    "enabled": true,
                    "route": "/apps/notes",
                    "renderer": "okf-note",
                    "layout": "list-grid",
                    "emptyStateTitle": "No notes yet",
                    "emptyStateDescription": "Create your first OKF note.",
                    "emptyStateAction": "New note",
                    "primaryAction": {
                        "label": "New note",
                        "action": "create-from-template",
                        "template": "template_default_okf_note"
                    },
                    "searchPlaceholder": "Search notes...",
                    "filterLabel": "All notes",
                    "sortLabel": "Modified",
                    "itemSingular": "note",
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
        assert!(obj.contains_key("application_id"), "missing application_id");
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

        // Notes module must expose OKF-native renderer and template
        assert_eq!(obj.get("renderer").unwrap().as_str().unwrap(), "okf-note");
        assert_eq!(
            obj.get("default_template").unwrap().as_str().unwrap(),
            "template_default_okf_note"
        );
        let ai_indexing = obj.get("ai_indexing").unwrap().as_object().unwrap();
        assert_eq!(
            ai_indexing.get("source").unwrap().as_str().unwrap(),
            "okf-frontmatter-and-markdown"
        );
        assert!(ai_indexing
            .get("permission_aware")
            .unwrap()
            .as_bool()
            .unwrap());
        let ui_config = obj.get("ui_config").unwrap().as_object().unwrap();
        assert_eq!(
            ui_config.get("documentFormat").unwrap().as_str().unwrap(),
            "okf-markdown"
        );
        assert!(ui_config.contains_key("okf"));
        let okf = ui_config.get("okf").unwrap().as_object().unwrap();
        assert_eq!(okf.get("conceptType").unwrap().as_str().unwrap(), "Note");

        // OKF config helper must match the embedded block
        let parsed_okf = module.okf_config();
        assert!(parsed_okf.enabled);
        assert_eq!(parsed_okf.concept_type, Some("Note".to_string()));
        assert!(parsed_okf.frontmatter_required);
    }
}
