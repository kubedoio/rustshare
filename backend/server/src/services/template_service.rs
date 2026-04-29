//! Template service for creating and managing module templates.

use bytes::Bytes;
use chrono::Utc;
use rustshare_core::{
    domain::{Template, TemplateDefaultFile, CreatedObject, UserId},
    services::{FileService, FolderService},
};
use rustshare_storage::{MetadataStore, ObjectStore};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use rustshare_infrastructure::repositories::PermissionResolverRepository;

/// Errors that can occur in template operations.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Template not found: {0}")]
    NotFound(String),
    #[error("Template already exists: {0}")]
    AlreadyExists(String),
    #[error("Module not found or disabled: {0}")]
    ModuleNotFound(String),
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

impl From<rustshare_core::services::FolderError> for TemplateError {
    fn from(e: rustshare_core::services::FolderError) -> Self {
        match e {
            rustshare_core::services::FolderError::NotFound(id) => {
                TemplateError::ModuleNotFound(id.to_string())
            }
            rustshare_core::services::FolderError::PermissionDenied { .. } => {
                TemplateError::PermissionDenied
            }
            rustshare_core::services::FolderError::InvalidName(s) => TemplateError::InvalidData(s),
            rustshare_core::services::FolderError::DuplicateName { .. } => {
                TemplateError::AlreadyExists("folder".to_string())
            }
            rustshare_core::services::FolderError::Database(e) => {
                TemplateError::Database(e.to_string())
            }
            _ => TemplateError::Storage(e.to_string()),
        }
    }
}

impl From<rustshare_core::services::FileError> for TemplateError {
    fn from(e: rustshare_core::services::FileError) -> Self {
        match e {
            rustshare_core::services::FileError::NotFound(id) => {
                TemplateError::NotFound(id.to_string())
            }
            rustshare_core::services::FileError::PermissionDenied { .. } => {
                TemplateError::PermissionDenied
            }
            rustshare_core::services::FileError::InvalidName(s) => TemplateError::InvalidData(s),
            rustshare_core::services::FileError::Database(e) => {
                TemplateError::Database(e.to_string())
            }
            _ => TemplateError::Storage(e.to_string()),
        }
    }
}

impl From<sqlx::Error> for TemplateError {
    fn from(e: sqlx::Error) -> Self {
        TemplateError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for TemplateError {
    fn from(e: serde_json::Error) -> Self {
        TemplateError::InvalidData(e.to_string())
    }
}

/// Request to create a new template.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateTemplateRequest {
    pub template_key: String,
    pub name: String,
    pub module_key: String,
    pub description: String,
    pub folder_structure: Vec<String>,
    pub default_files: Vec<TemplateDefaultFile>,
    pub metadata_schema: serde_json::Value,
    pub renderer: Option<String>,
    pub visibility_policy: String,
}

/// Request to update a template.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub folder_structure: Option<Vec<String>>,
    pub default_files: Option<Vec<TemplateDefaultFile>>,
    pub metadata_schema: Option<serde_json::Value>,
    pub renderer: Option<String>,
    pub visibility_policy: Option<String>,
    pub enabled: Option<bool>,
}

/// Service for managing templates and instantiating objects from them.
pub struct TemplateService {
    file_service: Arc<
        FileService<
            rustshare_storage::EventStore,
            MetadataStore,
            ObjectStore,
            PermissionResolverRepository,
        >,
    >,
    folder_service: Arc<
        FolderService<rustshare_storage::EventStore, MetadataStore, PermissionResolverRepository>,
    >,
    metadata_store: Arc<MetadataStore>,
}

impl TemplateService {
    pub fn new(
        file_service: Arc<
            FileService<
                rustshare_storage::EventStore,
                MetadataStore,
                ObjectStore,
                PermissionResolverRepository,
            >,
        >,
        folder_service: Arc<
            FolderService<
                rustshare_storage::EventStore,
                MetadataStore,
                PermissionResolverRepository,
            >,
        >,
        metadata_store: Arc<MetadataStore>,
    ) -> Self {
        Self {
            file_service,
            folder_service,
            metadata_store,
        }
    }

    /// Ensure default templates exist for predefined modules. Idempotent.
    pub async fn ensure_default_templates(&self, tenant_id: Uuid) -> Result<(), TemplateError> {
        let defaults = vec![
            (
                "template_default_note",
                "Default Note",
                "notes",
                "1.0",
                "Default template for notes.",
                Vec::<String>::new(),
                vec![TemplateDefaultFile {
                    path: ".rustshare-module.json".to_string(),
                    content: Some(r#"{"type":"rustshare.module","module_key":"notes"}"#.to_string()),
                    content_type: Some("application/json".to_string()),
                }],
                json!({}),
                Some("notes"),
            ),
            (
                "template_default_meeting",
                "Default Meeting Note",
                "meetings",
                "1.0",
                "Default template for meeting notes.",
                vec![],
                vec![
                    TemplateDefaultFile {
                        path: "index.md".to_string(),
                        content: Some("# Meeting\n\n## Agenda\n\n## Attendees\n\n## Notes\n\n## Decisions\n\n## Action Items\n".to_string()),
                        content_type: Some("text/markdown".to_string()),
                    },
                    TemplateDefaultFile {
                        path: ".rustshare.json".to_string(),
                        content: Some(r#"{"type":"rustshare.module","module_key":"meetings"}"#.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                    TemplateDefaultFile {
                        path: "events.jsonl".to_string(),
                        content: Some("".to_string()),
                        content_type: Some("application/jsonlines".to_string()),
                    },
                ],
                json!({
                    "type": "meeting",
                    "fields": {
                        "title": "string",
                        "date": "string",
                        "attendees": "array"
                    }
                }),
                Some("meeting-notes"),
            ),
            (
                "template_default_standup",
                "Default Standup Record",
                "standups",
                "1.0",
                "Default template for standup records.",
                vec![],
                vec![
                    TemplateDefaultFile {
                        path: ".rustshare-module.json".to_string(),
                        content: Some(r#"{"type":"rustshare.module","module_key":"standups"}"#.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                ],
                json!({}),
                Some("standups"),
            ),
            (
                "template_default_kanban",
                "Default Kanban Board",
                "kanban",
                "1.0",
                "Creates a standard Kanban board folder structure.",
                vec![
                    "00-Backlog".to_string(),
                    "01-Ready".to_string(),
                    "02-In-Progress".to_string(),
                    "03-Review".to_string(),
                    "04-Done".to_string(),
                ],
                vec![
                    TemplateDefaultFile {
                        path: "README.md".to_string(),
                        content: Some("# Kanban Board\n\nThis board is file-backed.\n".to_string()),
                        content_type: Some("text/markdown".to_string()),
                    },
                    TemplateDefaultFile {
                        path: ".rustshare-module.json".to_string(),
                        content: Some(r#"{"type":"rustshare.module","module_key":"kanban"}"#.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                ],
                json!({
                    "type": "kanban.board",
                    "fields": {
                        "title": "string",
                        "owner": "string",
                        "statusColumns": "array"
                    }
                }),
                Some("kanban"),
            ),
            (
                "template_default_decision",
                "Default Decision Record",
                "decisions",
                "1.0",
                "Default template for decision records.",
                vec![],
                vec![
                    TemplateDefaultFile {
                        path: ".rustshare-module.json".to_string(),
                        content: Some(r#"{"type":"rustshare.module","module_key":"decisions"}"#.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                ],
                json!({
                    "type": "decision",
                    "fields": {
                        "title": "string",
                        "status": "string",
                        "date": "string"
                    }
                }),
                Some("decisions"),
            ),
            (
                "template_default_share",
                "Default Share Package",
                "shares",
                "1.0",
                "Default template for share packages.",
                vec!["files".to_string()],
                vec![
                    TemplateDefaultFile {
                        path: "README.md".to_string(),
                        content: Some("# Share Package\n\nShared files and resources.\n".to_string()),
                        content_type: Some("text/markdown".to_string()),
                    },
                    TemplateDefaultFile {
                        path: ".rustshare-share.json".to_string(),
                        content: Some(r#"{"type":"rustshare.share","module_key":"shares"}"#.to_string()),
                        content_type: Some("application/json".to_string()),
                    },
                ],
                json!({}),
                Some("shares"),
            ),
        ];

        for (
            key,
            name,
            module_key,
            version,
            description,
            folder_structure,
            default_files,
            metadata_schema,
            renderer,
        ) in defaults
        {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM templates WHERE template_key = $1 AND tenant_id = $2)",
            )
            .bind(key)
            .bind(tenant_id)
            .fetch_one(self.metadata_store.pool())
            .await?;

            if !exists {
                let template = Template {
                    id: Uuid::new_v4(),
                    template_key: key.to_string(),
                    name: name.to_string(),
                    module_key: module_key.to_string(),
                    version: version.to_string(),
                    description: description.to_string(),
                    folder_structure: serde_json::to_value(&folder_structure)?,
                    default_files: serde_json::to_value(&default_files)?,
                    metadata_schema: metadata_schema.clone(),
                    renderer: renderer.map(|s| s.to_string()),
                    visibility_policy: "workspace".to_string(),
                    ai_indexing_policy: json!({"enabled": true}),
                    audit_logging_policy: json!({"enabled": true}),
                    created_by: None,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    enabled: true,
                    tenant_id,
                };

                sqlx::query(
                    r#"
                    INSERT INTO templates (
                        id, template_key, name, module_key, version, description,
                        folder_structure, default_files, metadata_schema, renderer,
                        visibility_policy, ai_indexing_policy, audit_logging_policy,
                        created_by, created_at, updated_at, enabled, tenant_id
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
                    "#,
                )
                .bind(template.id)
                .bind(&template.template_key)
                .bind(&template.name)
                .bind(&template.module_key)
                .bind(&template.version)
                .bind(&template.description)
                .bind(&template.folder_structure)
                .bind(&template.default_files)
                .bind(&template.metadata_schema)
                .bind(&template.renderer)
                .bind(&template.visibility_policy)
                .bind(&template.ai_indexing_policy)
                .bind(&template.audit_logging_policy)
                .bind(template.created_by)
                .bind(template.created_at)
                .bind(template.updated_at)
                .bind(template.enabled)
                .bind(template.tenant_id)
                .execute(self.metadata_store.pool())
                .await?;
            }
        }

        Ok(())
    }

    /// Create a custom template (admin only).
    pub async fn create_template(
        &self,
        request: CreateTemplateRequest,
        created_by: Uuid,
        tenant_id: Uuid,
    ) -> Result<Template, TemplateError> {
        // Validate uniqueness
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM templates WHERE template_key = $1 AND tenant_id = $2)",
        )
        .bind(&request.template_key)
        .bind(tenant_id)
        .fetch_one(self.metadata_store.pool())
        .await?;

        if exists {
            return Err(TemplateError::AlreadyExists(request.template_key));
        }

        // Validate module exists
        let module_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM modules WHERE module_key = $1 AND tenant_id = $2)",
        )
        .bind(&request.module_key)
        .bind(tenant_id)
        .fetch_one(self.metadata_store.pool())
        .await?;

        if !module_exists {
            return Err(TemplateError::ModuleNotFound(request.module_key));
        }

        // Validate folder structure
        for folder in &request.folder_structure {
            if folder.contains('/') || folder.contains('\\') || folder.is_empty() {
                return Err(TemplateError::InvalidData(format!(
                    "Invalid folder name: {}",
                    folder
                )));
            }
        }

        // Validate default files
        for file in &request.default_files {
            if file.path.is_empty()
                || file.path.contains('/')
                || file.path.contains('\\')
                || file.path == "."
                || file.path == ".."
            {
                return Err(TemplateError::InvalidData(format!(
                    "Invalid file path: {}",
                    file.path
                )));
            }
        }

        let template = Template {
            id: Uuid::new_v4(),
            template_key: request.template_key,
            name: request.name,
            module_key: request.module_key,
            version: "1.0".to_string(),
            description: request.description,
            folder_structure: serde_json::to_value(&request.folder_structure)?,
            default_files: serde_json::to_value(&request.default_files)?,
            metadata_schema: request.metadata_schema,
            renderer: request.renderer,
            visibility_policy: request.visibility_policy,
            ai_indexing_policy: json!({"enabled": true}),
            audit_logging_policy: json!({"enabled": true}),
            created_by: Some(created_by),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            enabled: true,
            tenant_id,
        };

        sqlx::query(
            r#"
            INSERT INTO templates (
                id, template_key, name, module_key, version, description,
                folder_structure, default_files, metadata_schema, renderer,
                visibility_policy, ai_indexing_policy, audit_logging_policy,
                created_by, created_at, updated_at, enabled, tenant_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
            "#,
        )
        .bind(template.id)
        .bind(&template.template_key)
        .bind(&template.name)
        .bind(&template.module_key)
        .bind(&template.version)
        .bind(&template.description)
        .bind(&template.folder_structure)
        .bind(&template.default_files)
        .bind(&template.metadata_schema)
        .bind(&template.renderer)
        .bind(&template.visibility_policy)
        .bind(&template.ai_indexing_policy)
        .bind(&template.audit_logging_policy)
        .bind(template.created_by)
        .bind(template.created_at)
        .bind(template.updated_at)
        .bind(template.enabled)
        .bind(template.tenant_id)
        .execute(self.metadata_store.pool())
        .await?;

        Ok(template)
    }

    /// List all templates.
    pub async fn list_templates(&self, tenant_id: Uuid) -> Result<Vec<Template>, TemplateError> {
        let templates: Vec<Template> = sqlx::query_as::<_, Template>(
            "SELECT * FROM templates WHERE tenant_id = $1 ORDER BY name",
        )
        .bind(tenant_id)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(templates)
    }

    /// List templates for a specific module.
    pub async fn list_templates_by_module(
        &self,
        module_key: &str,
        tenant_id: Uuid,
    ) -> Result<Vec<Template>, TemplateError> {
        let templates: Vec<Template> = sqlx::query_as::<_, Template>(
            "SELECT * FROM templates WHERE module_key = $1 AND tenant_id = $2 ORDER BY name",
        )
        .bind(module_key)
        .bind(tenant_id)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(templates)
    }

    /// Get template by key.
    pub async fn get_template(
        &self,
        key: &str,
        tenant_id: Uuid,
    ) -> Result<Template, TemplateError> {
        let template: Option<Template> = sqlx::query_as::<_, Template>(
            "SELECT * FROM templates WHERE template_key = $1 AND tenant_id = $2",
        )
        .bind(key)
        .bind(tenant_id)
        .fetch_optional(self.metadata_store.pool())
        .await?;

        template.ok_or_else(|| TemplateError::NotFound(key.to_string()))
    }

    /// Update template.
    pub async fn update_template(
        &self,
        key: &str,
        request: UpdateTemplateRequest,
        tenant_id: Uuid,
    ) -> Result<Template, TemplateError> {
        let template = self.get_template(key, tenant_id).await?;

        let name = request.name.unwrap_or(template.name);
        let description = request.description.unwrap_or(template.description);
        let folder_structure = request
            .folder_structure
            .map(|v| serde_json::to_value(&v))
            .transpose()?
            .unwrap_or(template.folder_structure);
        let default_files = request
            .default_files
            .map(|v| serde_json::to_value(&v))
            .transpose()?
            .unwrap_or(template.default_files);
        let metadata_schema = request.metadata_schema.unwrap_or(template.metadata_schema);
        let renderer = request.renderer.or(template.renderer);
        let visibility_policy = request
            .visibility_policy
            .unwrap_or(template.visibility_policy);
        let enabled = request.enabled.unwrap_or(template.enabled);

        sqlx::query(
            r#"
            UPDATE templates
            SET name = $1, description = $2, folder_structure = $3,
                default_files = $4, metadata_schema = $5, renderer = $6,
                visibility_policy = $7, enabled = $8, updated_at = now()
            WHERE template_key = $9 AND tenant_id = $10
            "#,
        )
        .bind(name)
        .bind(description)
        .bind(folder_structure)
        .bind(default_files)
        .bind(metadata_schema)
        .bind(renderer)
        .bind(visibility_policy)
        .bind(enabled)
        .bind(key)
        .bind(tenant_id)
        .execute(self.metadata_store.pool())
        .await?;

        self.get_template(key, tenant_id).await
    }

    /// Delete a template. Predefined templates cannot be deleted; they are disabled.
    pub async fn delete_template(
        &self,
        key: &str,
        tenant_id: Uuid,
    ) -> Result<(), TemplateError> {
        let template = self.get_template(key, tenant_id).await?;

        // Prevent deletion of predefined templates
        if template.template_key.starts_with("template_default_") {
            return Err(TemplateError::InvalidData(
                "Cannot delete predefined templates".to_string(),
            ));
        }

        sqlx::query(
            "DELETE FROM templates WHERE template_key = $1 AND tenant_id = $2",
        )
        .bind(key)
        .bind(tenant_id)
        .execute(self.metadata_store.pool())
        .await?;

        Ok(())
    }

    /// Instantiate an object from a template.
    pub async fn create_from_template(
        &self,
        template_key: &str,
        owner_id: UserId,
        tenant_id: Uuid,
        name: String,
        parent_folder_id: Option<Uuid>,
    ) -> Result<CreatedObject, TemplateError> {
        let template = self.get_template(template_key, tenant_id).await?;

        if !template.enabled {
            return Err(TemplateError::NotFound(template_key.to_string()));
        }

        // Verify module is enabled
        let module_enabled: bool = sqlx::query_scalar(
            "SELECT enabled FROM modules WHERE module_key = $1 AND tenant_id = $2",
        )
        .bind(&template.module_key)
        .bind(tenant_id)
        .fetch_optional(self.metadata_store.pool())
        .await?
        .unwrap_or(false);

        if !module_enabled {
            return Err(TemplateError::ModuleNotFound(template.module_key));
        }

        // Determine parent folder
        let parent_id = if let Some(id) = parent_folder_id {
            Some(id)
        } else {
            // Fetch module root_path from modules table
            let root_path: String = sqlx::query_scalar(
                "SELECT root_path FROM modules WHERE module_key = $1 AND tenant_id = $2",
            )
            .bind(&template.module_key)
            .bind(tenant_id)
            .fetch_one(self.metadata_store.pool())
            .await
            .map_err(|e| TemplateError::Database(format!("Failed to resolve module root: {}", e)))?;

            let root_name = root_path.trim_start_matches('/');

            let folders = self
                .metadata_store
                .list_folders(None, owner_id, tenant_id)
                .await
                .map_err(|e| TemplateError::Database(e.to_string()))?;

            if let Some(existing) = folders.into_iter().find(|f| f.name == root_name) {
                Some(existing.id)
            } else {
                let folder = self
                    .folder_service
                    .create_folder(root_name.to_string(), None, owner_id, tenant_id)
                    .await?;
                Some(folder.id)
            }
        };

        // Create the main object folder
        let object_folder = self
            .folder_service
            .create_folder(name.clone(), parent_id, owner_id, tenant_id)
            .await?;

        // Create subfolders from template
        let folder_structure: Vec<String> =
            serde_json::from_value(template.folder_structure.clone())?;
        for subfolder_name in folder_structure {
            self.folder_service
                .create_folder(subfolder_name, Some(object_folder.id), owner_id, tenant_id)
                .await?;
        }

        // Create default files
        let default_files: Vec<TemplateDefaultFile> =
            serde_json::from_value(template.default_files.clone())?;
        for file in default_files {
            let content = file.content.unwrap_or_default();
            let mime_type = file.content_type.unwrap_or_else(|| "text/plain".to_string());

            self.file_service
                .upload_file(
                    owner_id,
                    file.path,
                    Some(object_folder.id),
                    Bytes::from(content),
                    mime_type,
                    tenant_id,
                )
                .await?;
        }

        Ok(CreatedObject {
            object_id: object_folder.id,
            object_type: "folder".to_string(),
            path: object_folder.path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_error_display() {
        let err = TemplateError::NotFound("my-template".to_string());
        assert_eq!(err.to_string(), "Template not found: my-template");
    }
}
