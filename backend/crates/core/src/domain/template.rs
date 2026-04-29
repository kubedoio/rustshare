use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A template configuration for creating module objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Template {
    pub id: Uuid,
    pub template_key: String,
    pub name: String,
    pub module_key: String,
    pub version: String,
    pub description: String,
    pub folder_structure: serde_json::Value,
    pub default_files: serde_json::Value,
    pub metadata_schema: serde_json::Value,
    pub renderer: Option<String>,
    pub visibility_policy: String,
    pub ai_indexing_policy: serde_json::Value,
    pub audit_logging_policy: serde_json::Value,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub enabled: bool,
    pub tenant_id: Uuid,
}

/// A default file entry within a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateDefaultFile {
    pub path: String,
    pub content: Option<String>,
    #[serde(rename = "contentType")]
    pub content_type: Option<String>,
}

/// Request to create an object from a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFromTemplateRequest {
    pub template_key: String,
    pub name: String,
    pub parent_folder_id: Option<Uuid>,
}

/// Result of creating an object from a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}
