//! Module service for workspace module registry management.

use anyhow::Result;
use chrono::Utc;
use rustshare_core::{
    domain::{Module, UserId},
    services::{FileService, FolderService},
};
use rustshare_storage::{MetadataStore, ObjectStore};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use rustshare_infrastructure::repositories::PermissionResolverRepository;

/// Errors that can occur in module operations.
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("Module not found: {0}")]
    NotFound(String),
    #[error("Module already exists: {0}")]
    AlreadyExists(String),
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Invalid name: {0}")]
    InvalidName(String),
}

impl From<rustshare_core::services::FolderError> for ModuleError {
    fn from(e: rustshare_core::services::FolderError) -> Self {
        match e {
            rustshare_core::services::FolderError::NotFound(id) => {
                ModuleError::NotFound(id.to_string())
            }
            rustshare_core::services::FolderError::PermissionDenied { .. } => {
                ModuleError::PermissionDenied
            }
            rustshare_core::services::FolderError::InvalidName(s) => ModuleError::InvalidName(s),
            rustshare_core::services::FolderError::DuplicateName { .. } => {
                ModuleError::AlreadyExists("folder".to_string())
            }
            rustshare_core::services::FolderError::Database(e) => {
                ModuleError::Database(e.to_string())
            }
            _ => ModuleError::Storage(e.to_string()),
        }
    }
}

impl From<rustshare_core::services::FileError> for ModuleError {
    fn from(e: rustshare_core::services::FileError) -> Self {
        match e {
            rustshare_core::services::FileError::NotFound(id) => {
                ModuleError::NotFound(id.to_string())
            }
            rustshare_core::services::FileError::PermissionDenied { .. } => {
                ModuleError::PermissionDenied
            }
            rustshare_core::services::FileError::InvalidName(s) => ModuleError::InvalidName(s),
            rustshare_core::services::FileError::Database(e) => {
                ModuleError::Database(e.to_string())
            }
            _ => ModuleError::Storage(e.to_string()),
        }
    }
}

impl From<sqlx::Error> for ModuleError {
    fn from(e: sqlx::Error) -> Self {
        ModuleError::Database(e.to_string())
    }
}

/// Service for managing workspace modules.
pub struct ModuleService {
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

impl ModuleService {
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

    /// Ensure default predefined modules exist. Does not overwrite existing.
    pub async fn ensure_default_modules(&self, tenant_id: Uuid) -> Result<(), ModuleError> {
        let defaults = vec![
            (
                "notes",
                "Notes",
                "Capture file-backed notes and reusable knowledge.",
                "/Notes",
                "notes",
                "template_default_note",
                "file-text",
                true,
            ),
            (
                "meetings",
                "Meeting Notes",
                "Create structured meeting records with agenda, attendees, decisions and action items.",
                "/Meetings",
                "meeting-notes",
                "template_default_meeting",
                "users",
                false,
            ),
            (
                "standups",
                "Standup Records",
                "Track daily team updates, blockers and follow-up items.",
                "/Standups",
                "standups",
                "template_default_standup",
                "activity",
                false,
            ),
            (
                "kanban",
                "Kanban Dashboard",
                "Manage board cards as folders and files.",
                "/Kanban",
                "kanban",
                "template_default_kanban",
                "columns",
                false,
            ),
            (
                "decisions",
                "Decisions",
                "Record architectural, product and business decisions.",
                "/Decisions",
                "decisions",
                "template_default_decision",
                "git-branch",
                false,
            ),
            (
                "shares",
                "Shares",
                "Manage public and internal share packages.",
                "/Shares",
                "shares",
                "template_default_share",
                "share-2",
                false,
            ),
        ];

        for (
            key,
            display_name,
            description,
            root_path,
            renderer,
            default_template,
            icon,
            enabled,
        ) in defaults
        {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM modules WHERE module_key = $1 AND tenant_id = $2)",
            )
            .bind(key)
            .bind(tenant_id)
            .fetch_one(self.metadata_store.pool())
            .await?;

            if !exists {
                let module = Module {
                    id: Uuid::new_v4(),
                    module_key: key.to_string(),
                    display_name: display_name.to_string(),
                    description: description.to_string(),
                    enabled,
                    root_path: root_path.to_string(),
                    renderer: renderer.to_string(),
                    default_template: Some(default_template.to_string()),
                    icon: icon.to_string(),
                    schema_version: "1.0".to_string(),
                    permissions: json!({
                        "admin_can_configure": true,
                        "workspace_members_can_use": true,
                        "allow_public_share": false,
                        "allow_internal_share": true
                    }),
                    ai_indexing: json!({"enabled": true}),
                    audit: json!({"enabled": true}),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    tenant_id,
                };

                sqlx::query(
                    r#"
                    INSERT INTO modules (
                        id, module_key, display_name, description, enabled, root_path, renderer,
                        default_template, icon, schema_version, permissions, ai_indexing, audit,
                        created_at, updated_at, tenant_id
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
                    "#,
                )
                .bind(module.id)
                .bind(&module.module_key)
                .bind(&module.display_name)
                .bind(&module.description)
                .bind(module.enabled)
                .bind(&module.root_path)
                .bind(&module.renderer)
                .bind(&module.default_template)
                .bind(&module.icon)
                .bind(&module.schema_version)
                .bind(&module.permissions)
                .bind(&module.ai_indexing)
                .bind(&module.audit)
                .bind(module.created_at)
                .bind(module.updated_at)
                .bind(module.tenant_id)
                .execute(self.metadata_store.pool())
                .await?;
            }
        }

        Ok(())
    }

    /// Enable a module: mark enabled + ensure root folder exists.
    pub async fn enable_module(
        &self,
        key: &str,
        actor_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Module, ModuleError> {
        let module = self.get_module(key, tenant_id).await?;

        if module.enabled {
            return Ok(module);
        }

        // Ensure root folder exists
        self.ensure_module_root_folder(&module, actor_id, tenant_id).await?;

        // Mark enabled
        sqlx::query(
            "UPDATE modules SET enabled = true, updated_at = now() WHERE module_key = $1 AND tenant_id = $2",
        )
        .bind(key)
        .bind(tenant_id)
        .execute(self.metadata_store.pool())
        .await?;

        self.get_module(key, tenant_id).await
    }

    /// Disable a module: mark disabled. Does NOT delete files.
    pub async fn disable_module(
        &self,
        key: &str,
        _actor_id: UserId,
        tenant_id: Uuid,
    ) -> Result<Module, ModuleError> {
        let module = self.get_module(key, tenant_id).await?;

        if !module.enabled {
            return Ok(module);
        }

        sqlx::query(
            "UPDATE modules SET enabled = false, updated_at = now() WHERE module_key = $1 AND tenant_id = $2",
        )
        .bind(key)
        .bind(tenant_id)
        .execute(self.metadata_store.pool())
        .await?;

        self.get_module(key, tenant_id).await
    }

    /// List all modules (for admin).
    pub async fn list_modules(&self, tenant_id: Uuid) -> Result<Vec<Module>, ModuleError> {
        let modules: Vec<Module> = sqlx::query_as::<_, Module>(
            "SELECT * FROM modules WHERE tenant_id = $1 ORDER BY display_name",
        )
        .bind(tenant_id)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(modules)
    }

    /// List enabled modules (for dashboard).
    pub async fn list_enabled_modules(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<Module>, ModuleError> {
        let modules: Vec<Module> = sqlx::query_as::<_, Module>(
            "SELECT * FROM modules WHERE enabled = true AND tenant_id = $1 ORDER BY display_name",
        )
        .bind(tenant_id)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(modules)
    }

    /// Get a single module by key.
    pub async fn get_module(&self, key: &str, tenant_id: Uuid) -> Result<Module, ModuleError> {
        let module: Option<Module> = sqlx::query_as::<_, Module>(
            "SELECT * FROM modules WHERE module_key = $1 AND tenant_id = $2",
        )
        .bind(key)
        .bind(tenant_id)
        .fetch_optional(self.metadata_store.pool())
        .await?;

        module.ok_or_else(|| ModuleError::NotFound(key.to_string()))
    }

    /// Update module config (admin only). Only certain fields are mutable.
    pub async fn update_module(
        &self,
        key: &str,
        display_name: Option<String>,
        description: Option<String>,
        icon: Option<String>,
        permissions: Option<serde_json::Value>,
        ai_indexing: Option<serde_json::Value>,
        audit: Option<serde_json::Value>,
        tenant_id: Uuid,
    ) -> Result<Module, ModuleError> {
        let module = self.get_module(key, tenant_id).await?;

        let display_name = display_name.unwrap_or(module.display_name);
        let description = description.unwrap_or(module.description);
        let icon = icon.unwrap_or(module.icon);
        let permissions = permissions.unwrap_or(module.permissions);
        let ai_indexing = ai_indexing.unwrap_or(module.ai_indexing);
        let audit = audit.unwrap_or(module.audit);

        sqlx::query(
            r#"
            UPDATE modules
            SET display_name = $1, description = $2, icon = $3,
                permissions = $4, ai_indexing = $5, audit = $6, updated_at = now()
            WHERE module_key = $7 AND tenant_id = $8
            "#,
        )
        .bind(display_name)
        .bind(description)
        .bind(icon)
        .bind(permissions)
        .bind(ai_indexing)
        .bind(audit)
        .bind(key)
        .bind(tenant_id)
        .execute(self.metadata_store.pool())
        .await?;

        self.get_module(key, tenant_id).await
    }

    /// Ensure the module root folder exists.
    async fn ensure_module_root_folder(
        &self,
        module: &Module,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), ModuleError> {
        let root_name = module
            .root_path
            .trim_start_matches('/')
            .to_string();

        if root_name.is_empty() {
            return Err(ModuleError::InvalidName(
                "Root path cannot be empty".to_string(),
            ));
        }

        // Look for an existing folder with this name at root level for this user.
        let folders = self
            .metadata_store
            .list_folders(None, owner_id, tenant_id)
            .await
            .map_err(|e| ModuleError::Database(e.to_string()))?;

        if folders.iter().any(|f| f.name == root_name) {
            return Ok(());
        }

        // Create the root folder using the actor as owner
        self.folder_service
            .create_folder(root_name, None, owner_id, tenant_id)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_error_display() {
        let err = ModuleError::NotFound("notes".to_string());
        assert_eq!(err.to_string(), "Module not found: notes");
    }
}
