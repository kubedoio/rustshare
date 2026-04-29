//! Module service for workspace module registry management.

use chrono::Utc;
use rustshare_core::{
    domain::{Module, UserId},
    services::FolderService,
};
use rustshare_storage::MetadataStore;
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
    folder_service: Arc<
        FolderService<rustshare_storage::EventStore, MetadataStore, PermissionResolverRepository>,
    >,
    metadata_store: Arc<MetadataStore>,
}

#[derive(Debug, serde::Serialize)]
pub struct ModuleSummary {
    pub module_key: String,
    pub mode: String,
    pub total_items: i64,
    pub recent_items: Vec<SummaryItem>,
}

#[derive(Debug, serde::Serialize)]
pub struct SummaryItem {
    pub id: String,
    pub name: String,
    pub item_type: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
pub struct UpdateModuleInput {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub permissions: Option<serde_json::Value>,
    pub ai_indexing: Option<serde_json::Value>,
    pub audit: Option<serde_json::Value>,
    pub ui_config: Option<serde_json::Value>,
}

impl ModuleService {
    pub fn new(
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
                json!({
                    "sidebar": { "enabled": true, "order": 30, "icon": "file-text", "label": "Notes" },
                    "dashboard": { "enabled": true, "order": 10, "cardTitle": "Notes", "cardDescription": "Recent file-backed notes.", "summaryMode": "recent-items", "maxItems": 4, "primaryAction": { "label": "New Note", "action": "create-from-template", "template": "template_default_note" } },
                    "modulePage": { "layout": "list-grid", "emptyStateTitle": "No notes yet", "emptyStateDescription": "Create your first file-backed note.", "emptyStateAction": "New Note" }
                }),
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
                json!({
                    "sidebar": { "enabled": true, "order": 40, "icon": "users", "label": "Meetings" },
                    "dashboard": { "enabled": true, "order": 20, "cardTitle": "Meeting Notes", "cardDescription": "Structured meeting records.", "summaryMode": "recent-items", "maxItems": 4, "primaryAction": { "label": "New Meeting", "action": "create-from-template", "template": "template_default_meeting" } },
                    "modulePage": { "layout": "list-grid", "emptyStateTitle": "No meetings yet", "emptyStateDescription": "Create your first meeting note.", "emptyStateAction": "New Meeting" }
                }),
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
                json!({
                    "sidebar": { "enabled": true, "order": 50, "icon": "activity", "label": "Standups" },
                    "dashboard": { "enabled": true, "order": 30, "cardTitle": "Standup Records", "cardDescription": "Daily team updates and blockers.", "summaryMode": "recent-items", "maxItems": 4, "primaryAction": { "label": "New Standup", "action": "create-from-template", "template": "template_default_standup" } },
                    "modulePage": { "layout": "list-grid", "emptyStateTitle": "No standups yet", "emptyStateDescription": "Create your first standup record.", "emptyStateAction": "New Standup" }
                }),
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
                json!({
                    "sidebar": { "enabled": true, "order": 60, "icon": "columns", "label": "Kanban" },
                    "dashboard": { "enabled": true, "order": 40, "cardTitle": "Kanban Dashboard", "cardDescription": "Board cards as folders and files.", "summaryMode": "recent-items", "maxItems": 4, "primaryAction": { "label": "New Board", "action": "create-from-template", "template": "template_default_kanban" } },
                    "modulePage": { "layout": "list-grid", "emptyStateTitle": "No boards yet", "emptyStateDescription": "Create your first kanban board.", "emptyStateAction": "New Board" }
                }),
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
                json!({
                    "sidebar": { "enabled": true, "order": 70, "icon": "git-branch", "label": "Decisions" },
                    "dashboard": { "enabled": true, "order": 50, "cardTitle": "Decisions", "cardDescription": "Architectural and business decisions.", "summaryMode": "recent-items", "maxItems": 4, "primaryAction": { "label": "New Decision", "action": "create-from-template", "template": "template_default_decision" } },
                    "modulePage": { "layout": "list-grid", "emptyStateTitle": "No decisions yet", "emptyStateDescription": "Record your first decision.", "emptyStateAction": "New Decision" }
                }),
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
                json!({
                    "sidebar": { "enabled": true, "order": 80, "icon": "share-2", "label": "Shares" },
                    "dashboard": { "enabled": true, "order": 60, "cardTitle": "Shares", "cardDescription": "Public and internal share packages.", "summaryMode": "recent-items", "maxItems": 4, "primaryAction": { "label": "New Share", "action": "create-from-template", "template": "template_default_share" } },
                    "modulePage": { "layout": "list-grid", "emptyStateTitle": "No shares yet", "emptyStateDescription": "Create your first share package.", "emptyStateAction": "New Share" }
                }),
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
            ui_config,
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
                    ui_config,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    tenant_id,
                };

                sqlx::query(
                    r#"
                    INSERT INTO modules (
                        id, module_key, display_name, description, enabled, root_path, renderer,
                        default_template, icon, schema_version, permissions, ai_indexing, audit,
                        ui_config, created_at, updated_at, tenant_id
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
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
                .bind(&module.ui_config)
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
        self.ensure_module_root_folder(&module, actor_id, tenant_id)
            .await?;

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
    pub async fn list_enabled_modules(&self, tenant_id: Uuid) -> Result<Vec<Module>, ModuleError> {
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
        input: UpdateModuleInput,
        tenant_id: Uuid,
    ) -> Result<Module, ModuleError> {
        let module = self.get_module(key, tenant_id).await?;

        let display_name = input.display_name.unwrap_or(module.display_name);
        let description = input.description.unwrap_or(module.description);
        let icon = input.icon.unwrap_or(module.icon);
        let permissions = input.permissions.unwrap_or(module.permissions);
        let ai_indexing = input.ai_indexing.unwrap_or(module.ai_indexing);
        let audit = input.audit.unwrap_or(module.audit);
        let ui_config = input.ui_config.unwrap_or(module.ui_config);

        sqlx::query(
            r#"
            UPDATE modules
            SET display_name = $1, description = $2, icon = $3,
                permissions = $4, ai_indexing = $5, audit = $6, ui_config = $7, updated_at = now()
            WHERE module_key = $8 AND tenant_id = $9
            "#,
        )
        .bind(display_name)
        .bind(description)
        .bind(icon)
        .bind(permissions)
        .bind(ai_indexing)
        .bind(audit)
        .bind(ui_config)
        .bind(key)
        .bind(tenant_id)
        .execute(self.metadata_store.pool())
        .await?;

        self.get_module(key, tenant_id).await
    }

    /// Get a summary of module contents for dashboard cards.
    pub async fn get_module_summary(
        &self,
        key: &str,
        tenant_id: Uuid,
    ) -> Result<ModuleSummary, ModuleError> {
        let module = self.get_module(key, tenant_id).await?;

        let ui_config = module.ui_config.as_object().ok_or_else(|| {
            ModuleError::InvalidData("ui_config is not an object".to_string())
        })?;

        let dashboard = ui_config.get("dashboard").and_then(|v| v.as_object());
        let summary_mode = dashboard
            .and_then(|d| d.get("summaryMode"))
            .and_then(|v| v.as_str())
            .unwrap_or("none");
        let max_items = dashboard
            .and_then(|d| d.get("maxItems"))
            .and_then(|v| v.as_i64())
            .unwrap_or(4) as i64;

        let root_name = module.root_path.trim_start_matches('/');

        // Find root folder
        let folder_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM folders WHERE name = $1 AND parent_id IS NULL AND tenant_id = $2 LIMIT 1"
        )
        .bind(root_name)
        .bind(tenant_id)
        .fetch_optional(self.metadata_store.pool())
        .await?;

        let mut recent_items = Vec::new();
        let mut total_items = 0i64;

        if let Some(fid) = folder_id {
            // Count files and subfolders
            let file_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM files WHERE folder_id = $1 AND tenant_id = $2"
            )
            .bind(fid)
            .bind(tenant_id)
            .fetch_one(self.metadata_store.pool())
            .await?;

            let folder_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM folders WHERE parent_id = $1 AND tenant_id = $2"
            )
            .bind(fid)
            .bind(tenant_id)
            .fetch_one(self.metadata_store.pool())
            .await?;

            total_items = file_count + folder_count;

            if summary_mode == "recent-items" {
                // Recent files
                let files = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>)>(
                    "SELECT id, name, updated_at FROM files WHERE folder_id = $1 AND tenant_id = $2 ORDER BY updated_at DESC LIMIT $3"
                )
                .bind(fid)
                .bind(tenant_id)
                .bind(max_items)
                .fetch_all(self.metadata_store.pool())
                .await?;

                for (id, name, updated_at) in files {
                    recent_items.push(SummaryItem {
                        id: id.to_string(),
                        name,
                        item_type: "file".to_string(),
                        updated_at,
                    });
                }

                // Recent subfolders (fill remaining slots)
                let remaining = max_items - recent_items.len() as i64;
                if remaining > 0 {
                    let folders = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>)>(
                        "SELECT id, name, updated_at FROM folders WHERE parent_id = $1 AND tenant_id = $2 ORDER BY updated_at DESC LIMIT $3"
                    )
                    .bind(fid)
                    .bind(tenant_id)
                    .bind(remaining)
                    .fetch_all(self.metadata_store.pool())
                    .await?;

                    for (id, name, updated_at) in folders {
                        recent_items.push(SummaryItem {
                            id: id.to_string(),
                            name,
                            item_type: "folder".to_string(),
                            updated_at,
                        });
                    }
                }
            }
        }

        Ok(ModuleSummary {
            module_key: key.to_string(),
            mode: summary_mode.to_string(),
            total_items,
            recent_items,
        })
    }

    /// Ensure the module root folder exists.
    async fn ensure_module_root_folder(
        &self,
        module: &Module,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), ModuleError> {
        let root_name = module.root_path.trim_start_matches('/').to_string();

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

    #[test]
    fn test_module_error_display_already_exists() {
        let err = ModuleError::AlreadyExists("meetings".to_string());
        assert_eq!(err.to_string(), "Module already exists: meetings");
    }

    #[test]
    fn test_module_error_display_permission_denied() {
        let err = ModuleError::PermissionDenied;
        assert_eq!(err.to_string(), "Permission denied");
    }

    #[test]
    fn test_module_error_display_database() {
        let err = ModuleError::Database("connection failed".to_string());
        assert_eq!(err.to_string(), "Database error: connection failed");
    }

    #[test]
    fn test_update_module_input_debug() {
        let input = UpdateModuleInput {
            display_name: Some("Test".to_string()),
            description: None,
            icon: Some("file-text".to_string()),
            permissions: None,
            ai_indexing: None,
            audit: None,
            ui_config: Some(json!({"sidebar": {"enabled": true}})),
        };
        let debug = format!("{:?}", input);
        assert!(debug.contains("Test"));
        assert!(debug.contains("sidebar"));
    }

    #[test]
    fn test_module_summary_serialize() {
        let summary = ModuleSummary {
            module_key: "notes".to_string(),
            mode: "recent-items".to_string(),
            total_items: 5,
            recent_items: vec![
                SummaryItem {
                    id: "uuid-1".to_string(),
                    name: "Note 1".to_string(),
                    item_type: "file".to_string(),
                    updated_at: Utc::now(),
                },
            ],
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("notes"));
        assert!(json.contains("recent-items"));
        assert!(json.contains("Note 1"));
    }

    #[test]
    fn test_summary_item_serialize() {
        let item = SummaryItem {
            id: "uuid-1".to_string(),
            name: "Folder A".to_string(),
            item_type: "folder".to_string(),
            updated_at: Utc::now(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("Folder A"));
        assert!(json.contains("folder"));
    }
}
