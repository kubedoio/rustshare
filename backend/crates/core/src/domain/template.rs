use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A template configuration for creating module objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Template {
    pub id: Uuid,
    pub template_key: String,
    pub name: String,
    pub application_id: String,
    pub version: String,
    pub description: String,
    pub ui_config: serde_json::Value,
    pub folder_structure: serde_json::Value,
    pub default_files: serde_json::Value,
    pub metadata_schema: serde_json::Value,
    pub renderer: Option<String>,
    pub visibility_policy: String,
    pub ai_indexing_policy: serde_json::Value,
    pub audit_logging_policy: serde_json::Value,
    pub application_config: serde_json::Value,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub enabled: bool,
    pub system_template: bool,
    pub tenant_id: Uuid,
}

/// A default file entry within a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TemplateDefaultFile {
    pub path: String,
    pub content: Option<String>,
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
}

/// Request to create an object from a template.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateFromTemplateRequest {
    pub template_key: String,
    pub name: String,
    pub parent_folder_id: Option<Uuid>,
}

/// Result of creating an object from a template.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreatedObject {
    pub object_id: Uuid,
    pub object_type: String,
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_default_file_serialization() {
        let file = TemplateDefaultFile {
            path: "README.md".to_string(),
            content: Some("# Hello".to_string()),
            content_type: Some("text/markdown".to_string()),
        };
        let json = serde_json::to_string(&file).unwrap();
        assert!(json.contains("README.md"));
        assert!(json.contains("contentType"));
    }

    #[test]
    fn test_template_default_file_deserialization() {
        let json = r##"{"path":"index.md","content":"# Note","contentType":"text/markdown"}"##;
        let file: TemplateDefaultFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.path, "index.md");
        assert_eq!(file.content, Some("# Note".to_string()));
        assert_eq!(file.content_type, Some("text/markdown".to_string()));
    }

    #[test]
    fn test_template_serialization_contract() {
        // This test asserts the canonical JSON shape for templates.
        // If the backend drifts from the contract, this test will fail.
        let template = Template {
            id: Uuid::nil(),
            template_key: "template_default_okf_note".to_string(),
            name: "Default OKF Note".to_string(),
            application_id: "io.elembra.notes".to_string(),
            version: "1.0".to_string(),
            description: "Default OKF-native template for notes.".to_string(),
            ui_config: serde_json::json!({
                "createLabel": "New Note",
                "icon": "sticky-note"
            }),
            folder_structure: serde_json::json!([
                "attachments",
                "drawings",
                "exports",
                "_rustshare"
            ]),
            default_files: serde_json::json!([
                {"path": "note.md", "content": "---\ntype: Note\ntitle: \"{{title}}\"\n...", "contentType": "text/markdown"}
            ]),
            metadata_schema: serde_json::json!({
                "type": "rustshare.note",
                "okf": {
                    "conceptType": "Note",
                    "frontmatterRequired": true
                }
            }),
            renderer: Some("okf-note".to_string()),
            visibility_policy: "workspace".to_string(),
            ai_indexing_policy: serde_json::json!({
                "enabled": true,
                "source": "okf-frontmatter-and-markdown",
                "permission_aware": true
            }),
            audit_logging_policy: serde_json::json!({"enabled": true}),
            application_config: serde_json::json!({}),
            created_by: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            enabled: true,
            system_template: true,
            tenant_id: Uuid::nil(),
        };

        let json = serde_json::to_value(&template).unwrap();
        let obj = json.as_object().unwrap();

        // Canonical snake_case field names must be present
        assert!(obj.contains_key("id"), "missing id");
        assert!(obj.contains_key("template_key"), "missing template_key");
        assert!(obj.contains_key("application_id"), "missing application_id");
        assert!(
            obj.contains_key("folder_structure"),
            "missing folder_structure"
        );
        assert!(obj.contains_key("default_files"), "missing default_files");
        assert!(
            obj.contains_key("metadata_schema"),
            "missing metadata_schema"
        );
        assert!(
            obj.contains_key("visibility_policy"),
            "missing visibility_policy"
        );
        assert!(
            obj.contains_key("ai_indexing_policy"),
            "missing ai_indexing_policy"
        );
        assert!(
            obj.contains_key("audit_logging_policy"),
            "missing audit_logging_policy"
        );
        assert!(
            obj.contains_key("application_config"),
            "missing application_config"
        );
        assert!(obj.contains_key("created_by"), "missing created_by");
        assert!(
            obj.contains_key("system_template"),
            "missing system_template"
        );
        assert!(obj.contains_key("tenant_id"), "missing tenant_id");

        // TemplateDefaultFile must serialize with camelCase contentType
        let files = obj.get("default_files").unwrap().as_array().unwrap();
        let first_file = files.first().unwrap().as_object().unwrap();
        assert!(
            first_file.contains_key("contentType"),
            "missing contentType in default_files"
        );
        assert!(
            !first_file.contains_key("content_type"),
            "snake_case content_type leaked into default_files serialization"
        );

        // Notes template must be OKF-native
        assert_eq!(
            obj.get("template_key").unwrap().as_str().unwrap(),
            "template_default_okf_note"
        );
        assert_eq!(obj.get("renderer").unwrap().as_str().unwrap(), "okf-note");
        let ai_policy = obj.get("ai_indexing_policy").unwrap().as_object().unwrap();
        assert_eq!(
            ai_policy.get("source").unwrap().as_str().unwrap(),
            "okf-frontmatter-and-markdown"
        );
        assert!(ai_policy
            .get("permission_aware")
            .unwrap()
            .as_bool()
            .unwrap());
        let schema = obj.get("metadata_schema").unwrap().as_object().unwrap();
        assert_eq!(
            schema.get("type").unwrap().as_str().unwrap(),
            "rustshare.note"
        );
        assert_eq!(
            schema
                .get("okf")
                .unwrap()
                .get("conceptType")
                .unwrap()
                .as_str()
                .unwrap(),
            "Note"
        );
    }
}
