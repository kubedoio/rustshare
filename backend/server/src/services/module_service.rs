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

use crate::services::icon_registry::is_approved_icon_key;
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
    #[error("Invalid data: {0}")]
    InvalidData(String),
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
    pub extra: Option<serde_json::Value>,
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
    pub root_path: Option<String>,
    pub renderer: Option<String>,
    pub default_template: Option<Option<String>>,
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
                "/Workspace/Notes",
                "notes",
                "template_default_note",
                "sticky-note",
                true,
                json!({
                    "sidebar": { "enabled": true, "order": 30, "icon": "sticky-note", "label": "Notes" },
                    "dashboard": { "enabled": true, "order": 10, "cardTitle": "Notes", "cardDescription": "Recent file-backed notes.", "summaryMode": "recent-items", "maxItems": 4, "primaryAction": { "label": "New note", "action": "create-from-template", "template": "template_default_note" } },
                    "modulePage": { "layout": "list-grid", "emptyStateTitle": "No notes yet", "emptyStateDescription": "Create your first file-backed note.", "emptyStateAction": "New note" }
                }),
            ),
            (
                "meetings",
                "Meeting Notes",
                "Record simple meeting notes, decisions, and follow-up items.",
                "/Workspace/Meetings",
                "meetings",
                "template_default_meeting",
                "calendar-days",
                false,
                json!({
                    "sidebar": { "enabled": true, "order": 40, "icon": "calendar-days", "label": "Meeting Notes" },
                    "dashboard": { "enabled": true, "order": 20, "cardTitle": "Meeting Notes", "cardDescription": "Recent meeting notes.", "summaryMode": "recent-items", "maxItems": 4, "primaryAction": { "label": "New meeting note", "action": "create-from-template", "template": "template_default_meeting" } },
                    "modulePage": { "layout": "list-grid", "emptyStateTitle": "No meeting notes yet", "emptyStateDescription": "Create a meeting note to capture agenda, discussion, decisions, and follow-up items.", "emptyStateAction": "New meeting note" }
                }),
            ),
            (
                "standups",
                "Standup Records",
                "Capture simple daily updates, blockers, and follow-up items.",
                "/Workspace/Standups",
                "standups",
                "template_default_standup",
                "clipboard-list",
                false,
                json!({
                    "sidebar": { "enabled": true, "order": 50, "icon": "clipboard-list", "label": "Standup Records" },
                    "dashboard": { "enabled": true, "order": 30, "cardTitle": "Standup Records", "cardDescription": "Recent standup records.", "summaryMode": "recent-items", "maxItems": 4, "primaryAction": { "label": "New standup", "action": "create-from-template", "template": "template_default_standup" } },
                    "modulePage": { "layout": "list-grid", "emptyStateTitle": "No standup records yet", "emptyStateDescription": "Create a daily update to capture progress, blockers, and follow-up items.", "emptyStateAction": "New standup" }
                }),
            ),
            (
                "kanban",
                "Kanban",
                "Organize lightweight work boards in your workspace.",
                "/Workspace/Kanban",
                "kanban",
                "template_default_kanban",
                "columns",
                false,
                json!({
                    "sidebar": { "enabled": true, "order": 50, "icon": "columns", "label": "Kanban" },
                    "dashboard": { "enabled": true, "order": 40, "widget": { "enabled": true, "type": "kanban-summary", "title": "Kanban", "description": "Recent boards.", "size": "large", "columns": { "desktop": 6, "tablet": 12, "mobile": 12 }, "maxItems": 4, "primaryAction": { "label": "New board", "action": "create-from-template", "template": "template_default_kanban" } } },
                    "page": { "enabled": true, "route": "/modules/kanban", "renderer": "kanban", "layout": "kanban-board", "emptyStateTitle": "No boards yet", "emptyStateDescription": "Create a lightweight board to organize work, ideas, or follow-up items.", "primaryAction": { "label": "New board", "action": "create-from-template", "template": "template_default_kanban" } }
                }),
            ),
            (
                "decisions",
                "Decisions",
                "Record important decisions with context and rationale.",
                "/Workspace/Decisions",
                "decisions",
                "template_default_decision",
                "git-branch",
                false,
                json!({
                    "sidebar": { "enabled": true, "order": 70, "icon": "git-branch", "label": "Decisions" },
                    "dashboard": { "enabled": true, "order": 50, "cardTitle": "Decisions", "cardDescription": "Recent decision records.", "summaryMode": "recent-items", "maxItems": 4, "primaryAction": { "label": "New decision", "action": "create-from-template", "template": "template_default_decision" } },
                    "modulePage": { "layout": "list-grid", "emptyStateTitle": "No decisions yet", "emptyStateDescription": "Create a decision record to preserve context, rationale, and follow-up.", "emptyStateAction": "New decision" }
                }),
            ),
            (
                "brainstorming",
                "Brainstorming",
                "Capture sketches, flows, and early ideas as visual workspace boards.",
                "/Workspace/Brainstorming",
                "brainstorming",
                "template_blank_brainstorm",
                "pen-tool",
                false,
                json!({
                    "sidebar": { "enabled": true, "order": 55, "icon": "pen-tool", "label": "Brainstorming" },
                    "dashboard": { "enabled": true, "order": 55, "widget": { "enabled": true, "type": "recent-brainstorm-boards", "title": "Brainstorming", "description": "Recent idea boards.", "size": "medium", "columns": { "desktop": 6, "tablet": 12, "mobile": 12 }, "maxItems": 4, "primaryAction": { "label": "New idea board", "action": "create-from-template", "template": "template_blank_brainstorm" } } },
                    "page": { "enabled": true, "route": "/modules/brainstorming", "renderer": "brainstorming", "layout": "gallery-grid", "emptyStateTitle": "No idea boards yet", "emptyStateDescription": "Create a simple visual board to capture sketches, flows, or early thinking.", "primaryAction": { "label": "New idea board", "action": "create-from-template", "template": "template_blank_brainstorm" } }
                }),
            ),
            (
                "shares",
                "Shares",
                "Manage items shared from your workspace.",
                "/Workspace/Shares",
                "shares",
                "template_default_share",
                "share-2",
                false,
                json!({
                    "sidebar": { "enabled": true, "order": 80, "icon": "share-2", "label": "Shares" },
                    "dashboard": { "enabled": true, "order": 60, "cardTitle": "Shares", "cardDescription": "Recent shares.", "summaryMode": "shares-overview", "maxItems": 4, "primaryAction": { "label": "New share", "action": "generic-create" } },
                    "modulePage": { "layout": "list-grid", "emptyStateTitle": "No active shares", "emptyStateDescription": "Share a file or folder when you are ready.", "emptyStateAction": "New share" }
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

        if let Some(admin_id) = self.find_admin_user_for_tenant(tenant_id).await? {
            let enabled_modules = self
                .list_modules(tenant_id)
                .await?
                .into_iter()
                .filter(|module| module.enabled)
                .collect::<Vec<_>>();

            for module in enabled_modules {
                self.ensure_module_root_folder(&module, admin_id, tenant_id)
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

        Ok(modules
            .into_iter()
            .map(|m| self.normalize_module(m))
            .collect())
    }

    /// List enabled modules (for dashboard).
    pub async fn list_enabled_modules(
        &self,
        tenant_id: Uuid,
        user_id: UserId,
    ) -> Result<Vec<Module>, ModuleError> {
        let is_admin = self.is_admin_user(user_id, tenant_id).await?;
        let modules: Vec<Module> = sqlx::query_as::<_, Module>(
            "SELECT * FROM modules WHERE enabled = true AND tenant_id = $1 ORDER BY display_name",
        )
        .bind(tenant_id)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(modules
            .into_iter()
            .map(|m| self.normalize_module(m))
            .filter(|module| user_can_access_module(module, is_admin))
            .collect())
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

        module
            .map(|m| self.normalize_module(m))
            .ok_or_else(|| ModuleError::NotFound(key.to_string()))
    }

    /// Resolve a module by key, returning the effective definition with defaults merged.
    fn normalize_module(&self, module: Module) -> Module {
        let ui_config = normalize_module_ui_config(
            &module.module_key,
            &module.display_name,
            &module.description,
            &module.icon,
            &module.root_path,
            &module.renderer,
            module.default_template.as_deref(),
            Some(module.ui_config),
        );

        Module {
            ui_config,
            ..module
        }
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
        validate_module_icon(&icon)?;
        let root_path = input.root_path.unwrap_or(module.root_path);
        validate_root_path(&root_path)?;
        let renderer = input.renderer.unwrap_or(module.renderer);
        let default_template = input.default_template.unwrap_or(module.default_template);
        let permissions = input.permissions.unwrap_or(module.permissions);
        let ai_indexing = input.ai_indexing.unwrap_or(module.ai_indexing);
        let audit = input.audit.unwrap_or(module.audit);

        // Validate UI config fields if provided
        if let Some(ref ui) = input.ui_config {
            if let Some(sidebar) = ui.get("sidebar").and_then(|v| v.as_object()) {
                if let Some(order) = sidebar.get("order").and_then(|v| v.as_i64()) {
                    if !(0..=1000).contains(&order) {
                        return Err(ModuleError::InvalidData(
                            "Sidebar order must be between 0 and 1000".to_string(),
                        ));
                    }
                }
            }
            if let Some(dashboard) = ui.get("dashboard").and_then(|v| v.as_object()) {
                if let Some(order) = dashboard.get("order").and_then(|v| v.as_i64()) {
                    if !(0..=1000).contains(&order) {
                        return Err(ModuleError::InvalidData(
                            "Dashboard order must be between 0 and 1000".to_string(),
                        ));
                    }
                }
                if let Some(max) = dashboard.get("maxItems").and_then(|v| v.as_i64()) {
                    if !(1..=50).contains(&max) {
                        return Err(ModuleError::InvalidData(
                            "Dashboard maxItems must be between 1 and 50".to_string(),
                        ));
                    }
                }
            }
        }

        let ui_config = normalize_module_ui_config(
            key,
            &display_name,
            &description,
            &icon,
            &root_path,
            &renderer,
            default_template.as_deref(),
            Some(input.ui_config.unwrap_or(module.ui_config)),
        );

        sqlx::query(
            r#"
            UPDATE modules
            SET display_name = $1, description = $2, icon = $3, root_path = $4,
                renderer = $5, default_template = $6, permissions = $7,
                ai_indexing = $8, audit = $9, ui_config = $10, updated_at = now()
            WHERE module_key = $11 AND tenant_id = $12
            "#,
        )
        .bind(display_name)
        .bind(description)
        .bind(icon)
        .bind(root_path)
        .bind(renderer)
        .bind(default_template)
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
        user_id: UserId,
    ) -> Result<ModuleSummary, ModuleError> {
        let module = self.get_module(key, tenant_id).await?;
        let is_admin = self.is_admin_user(user_id, tenant_id).await?;
        if !module.enabled {
            return Err(ModuleError::InvalidData("Module disabled".to_string()));
        }
        if !user_can_access_module(&module, is_admin) {
            return Err(ModuleError::PermissionDenied);
        }

        let ui_config = module
            .ui_config
            .as_object()
            .ok_or_else(|| ModuleError::InvalidData("ui_config is not an object".to_string()))?;

        let dashboard = ui_config.get("dashboard").and_then(|v| v.as_object());
        let summary_mode = dashboard
            .and_then(|d| d.get("summaryMode"))
            .and_then(|v| v.as_str())
            .unwrap_or("generic-file-summary");
        let max_items = dashboard
            .and_then(|d| d.get("maxItems"))
            .and_then(|v| v.as_i64())
            .unwrap_or(4) as i64;
        let root_path = module.root_path.trim_end_matches('/').to_string();
        let path_prefix = format!("{root_path}/%");

        let file_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM files WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND path LIKE $3",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(&path_prefix)
        .fetch_one(self.metadata_store.pool())
        .await
        .unwrap_or(0);

        let folder_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM folders WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND path LIKE $3 AND path <> $4",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(&path_prefix)
        .bind(&root_path)
        .fetch_one(self.metadata_store.pool())
        .await
        .unwrap_or(0);

        let total_items = file_count + folder_count;

        let (mode, recent_items, extra) = self
            .build_summary_for_mode(
                key,
                summary_mode,
                &root_path,
                &path_prefix,
                max_items,
                tenant_id,
                user_id,
            )
            .await?;

        Ok(ModuleSummary {
            module_key: key.to_string(),
            mode,
            total_items,
            recent_items,
            extra,
        })
    }

    /// Ensure the Workspace folder exists.
    async fn ensure_workspace_folder(
        &self,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<rustshare_core::domain::Folder, ModuleError> {
        let folders = self
            .metadata_store
            .list_folders(None, owner_id, tenant_id)
            .await
            .map_err(|e| ModuleError::Database(e.to_string()))?;

        if let Some(ws) = folders.into_iter().find(|f| f.name == "Workspace") {
            return Ok(ws);
        }

        self.folder_service
            .create_folder("Workspace".into(), None, owner_id, tenant_id)
            .await
            .map_err(|e| ModuleError::Storage(e.to_string()))
    }

    /// Ensure the module root folder exists under /Workspace.
    async fn ensure_module_root_folder(
        &self,
        module: &Module,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), ModuleError> {
        let root_name = module
            .root_path
            .trim_start_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&module.root_path)
            .to_string();

        if root_name.is_empty() {
            return Err(ModuleError::InvalidName(
                "Root path cannot be empty".to_string(),
            ));
        }

        let ws = self.ensure_workspace_folder(owner_id, tenant_id).await?;

        let folders = self
            .metadata_store
            .list_folders(Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| ModuleError::Database(e.to_string()))?;

        if folders.iter().any(|f| f.name == root_name) {
            return Ok(());
        }

        self.folder_service
            .create_folder(root_name, Some(ws.id), owner_id, tenant_id)
            .await?;

        Ok(())
    }

    async fn build_summary_for_mode(
        &self,
        key: &str,
        _summary_mode: &str,
        root_path: &str,
        path_prefix: &str,
        max_items: i64,
        tenant_id: Uuid,
        user_id: UserId,
    ) -> Result<(String, Vec<SummaryItem>, Option<serde_json::Value>), ModuleError> {
        match key {
            "notes" => {
                let items = self
                    .recent_files_under_path(path_prefix, max_items, tenant_id, user_id)
                    .await?;
                Ok(("recent-items".to_string(), items, None))
            }
            "meetings" => {
                let items = self
                    .recent_folders_under_path(path_prefix, max_items, tenant_id, user_id)
                    .await?;
                Ok(("recent-items".to_string(), items, None))
            }
            "standups" => {
                let items = self
                    .recent_files_under_path(path_prefix, max_items, tenant_id, user_id)
                    .await?;
                let today_token = Utc::now().format("%Y-%m-%d").to_string();
                let today_exists = items.iter().any(|item| item.name.contains(&today_token));
                Ok((
                    "today-status".to_string(),
                    items,
                    Some(json!({ "todayExists": today_exists })),
                ))
            }
            "kanban" => {
                let boards = self
                    .direct_child_folders(root_path, max_items, tenant_id, user_id)
                    .await?;
                let cards = self
                    .recent_folders_matching(path_prefix, "CARD-%", max_items, tenant_id, user_id)
                    .await?;
                Ok((
                    "kanban-overview".to_string(),
                    cards,
                    Some(json!({ "boards": boards })),
                ))
            }
            "decisions" => {
                let items = self
                    .recent_files_under_path(path_prefix, max_items, tenant_id, user_id)
                    .await?;
                Ok(("recent-items".to_string(), items, None))
            }
            "shares" => {
                let items = self
                    .recent_folders_under_path(path_prefix, max_items, tenant_id, user_id)
                    .await?;
                let public_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM folders WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND (path LIKE '/Shares/Public/%' OR path LIKE '/Workspace/Shares/Public/%')",
                )
                .bind(tenant_id)
                .bind(user_id)
                .fetch_one(self.metadata_store.pool())
                .await
                .unwrap_or(0);
                let internal_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM folders WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND (path LIKE '/Shares/Internal/%' OR path LIKE '/Workspace/Shares/Internal/%')",
                )
                .bind(tenant_id)
                .bind(user_id)
                .fetch_one(self.metadata_store.pool())
                .await
                .unwrap_or(0);
                Ok((
                    "shares-overview".to_string(),
                    items,
                    Some(json!({ "publicCount": public_count, "internalCount": internal_count })),
                ))
            }
            _ => {
                let items = self
                    .recent_mixed_items(path_prefix, max_items, tenant_id, user_id)
                    .await?;
                Ok(("generic-file-summary".to_string(), items, None))
            }
        }
    }

    async fn recent_files_under_path(
        &self,
        path_prefix: &str,
        max_items: i64,
        tenant_id: Uuid,
        owner_id: UserId,
    ) -> Result<Vec<SummaryItem>, ModuleError> {
        let rows = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>)>(
            "SELECT id, name, modified_at FROM files WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND path LIKE $3 ORDER BY modified_at DESC LIMIT $4",
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(path_prefix)
        .bind(max_items)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, name, updated_at)| SummaryItem {
                id: id.to_string(),
                name,
                item_type: "file".to_string(),
                updated_at,
            })
            .collect())
    }

    async fn recent_folders_under_path(
        &self,
        path_prefix: &str,
        max_items: i64,
        tenant_id: Uuid,
        owner_id: UserId,
    ) -> Result<Vec<SummaryItem>, ModuleError> {
        let rows = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>)>(
            "SELECT id, name, updated_at FROM folders WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND path LIKE $3 ORDER BY updated_at DESC LIMIT $4",
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(path_prefix)
        .bind(max_items)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, name, updated_at)| SummaryItem {
                id: id.to_string(),
                name,
                item_type: "folder".to_string(),
                updated_at,
            })
            .collect())
    }

    async fn recent_folders_matching(
        &self,
        path_prefix: &str,
        name_pattern: &str,
        max_items: i64,
        tenant_id: Uuid,
        owner_id: UserId,
    ) -> Result<Vec<SummaryItem>, ModuleError> {
        let rows = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>)>(
            "SELECT id, name, updated_at FROM folders WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND path LIKE $3 AND name LIKE $4 ORDER BY updated_at DESC LIMIT $5",
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(path_prefix)
        .bind(name_pattern)
        .bind(max_items)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, name, updated_at)| SummaryItem {
                id: id.to_string(),
                name,
                item_type: "folder".to_string(),
                updated_at,
            })
            .collect())
    }

    async fn direct_child_folders(
        &self,
        root_path: &str,
        max_items: i64,
        tenant_id: Uuid,
        owner_id: UserId,
    ) -> Result<Vec<SummaryItem>, ModuleError> {
        let rows = sqlx::query_as::<_, (Uuid, String, chrono::DateTime<chrono::Utc>)>(
            "SELECT id, name, updated_at FROM folders WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND parent_folder_id = (SELECT id FROM folders WHERE path = $3 AND tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL LIMIT 1) ORDER BY updated_at DESC LIMIT $4",
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(root_path)
        .bind(max_items)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, name, updated_at)| SummaryItem {
                id: id.to_string(),
                name,
                item_type: "folder".to_string(),
                updated_at,
            })
            .collect())
    }

    async fn recent_mixed_items(
        &self,
        path_prefix: &str,
        max_items: i64,
        tenant_id: Uuid,
        owner_id: UserId,
    ) -> Result<Vec<SummaryItem>, ModuleError> {
        let mut items = self
            .recent_files_under_path(path_prefix, max_items, tenant_id, owner_id)
            .await?;
        if (items.len() as i64) < max_items {
            let remaining = max_items - items.len() as i64;
            items.extend(
                self.recent_folders_under_path(path_prefix, remaining, tenant_id, owner_id)
                    .await?,
            );
        }
        items.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
        items.truncate(max_items as usize);
        Ok(items)
    }

    async fn is_admin_user(&self, user_id: UserId, tenant_id: Uuid) -> Result<bool, ModuleError> {
        let is_admin = sqlx::query_scalar::<_, bool>(
            "SELECT COALESCE(is_admin, false) FROM users WHERE id = $1 AND tenant_id = $2",
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_optional(self.metadata_store.pool())
        .await?
        .unwrap_or(false);
        Ok(is_admin)
    }

    async fn find_admin_user_for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<UserId>, ModuleError> {
        let admin_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM users WHERE tenant_id = $1 AND is_admin = true ORDER BY created_at ASC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(self.metadata_store.pool())
        .await?;
        Ok(admin_id)
    }
}

fn user_can_access_module(module: &Module, is_admin: bool) -> bool {
    if is_admin {
        return true;
    }

    module
        .permissions
        .get("workspace_members_can_use")
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
}

fn validate_module_icon(icon: &str) -> Result<(), ModuleError> {
    if is_approved_icon_key(icon) {
        Ok(())
    } else {
        Err(ModuleError::InvalidData(format!(
            "Unapproved module icon: {icon}"
        )))
    }
}

fn validate_root_path(root_path: &str) -> Result<(), ModuleError> {
    if !root_path.starts_with('/') || root_path.contains("..") || root_path.trim() == "/" {
        return Err(ModuleError::InvalidName(format!(
            "Invalid root path: {root_path}"
        )));
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn normalize_module_ui_config(
    module_key: &str,
    display_name: &str,
    description: &str,
    icon: &str,
    _root_path: &str,
    renderer: &str,
    default_template: Option<&str>,
    existing_ui_config: Option<serde_json::Value>,
) -> serde_json::Value {
    let existing = existing_ui_config
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    let sidebar = existing
        .get("sidebar")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let dashboard = existing
        .get("dashboard")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let widget = dashboard
        .get("widget")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let page = existing
        .get("page")
        .and_then(|value| value.as_object())
        .cloned()
        .or_else(|| {
            existing
                .get("modulePage")
                .and_then(|value| value.as_object())
                .cloned()
        })
        .unwrap_or_default();

    let widget_type = widget
        .get("type")
        .and_then(|value| value.as_str())
        .or_else(|| {
            dashboard
                .get("summaryMode")
                .and_then(|value| value.as_str())
        })
        .unwrap_or(default_widget_type(module_key));
    let widget_title = widget
        .get("title")
        .and_then(|value| value.as_str())
        .or_else(|| dashboard.get("cardTitle").and_then(|value| value.as_str()))
        .unwrap_or(display_name);
    let widget_description = widget
        .get("description")
        .and_then(|value| value.as_str())
        .or_else(|| {
            dashboard
                .get("cardDescription")
                .and_then(|value| value.as_str())
        })
        .unwrap_or(description);
    let widget_size = widget
        .get("size")
        .and_then(|value| value.as_str())
        .unwrap_or(default_widget_size(module_key));
    let columns = widget
        .get("columns")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_else(|| default_widget_columns(widget_size));
    let widget_primary_action = widget
        .get("primaryAction")
        .cloned()
        .or_else(|| dashboard.get("primaryAction").cloned())
        .unwrap_or_else(|| default_primary_action(module_key, default_template));
    let dashboard_enabled = dashboard
        .get("enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(default_dashboard_enabled(module_key));
    let dashboard_order = dashboard
        .get("order")
        .and_then(|value| value.as_i64())
        .unwrap_or(default_dashboard_order(module_key));
    let max_items = widget
        .get("maxItems")
        .and_then(|value| value.as_i64())
        .or_else(|| dashboard.get("maxItems").and_then(|value| value.as_i64()))
        .unwrap_or(4);

    let page_enabled = page
        .get("enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let page_route = page
        .get("route")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| format!("/modules/{module_key}"));
    let page_renderer = page
        .get("renderer")
        .and_then(|value| value.as_str())
        .unwrap_or(renderer)
        .to_string();
    let page_layout = page
        .get("layout")
        .and_then(|value| value.as_str())
        .unwrap_or(default_page_layout(module_key))
        .to_string();
    let page_empty_title = page
        .get("emptyStateTitle")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| default_empty_state_title(module_key, display_name));
    let page_empty_description = page
        .get("emptyStateDescription")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| default_empty_state_description(module_key, description));
    let page_empty_action = page
        .get("emptyStateAction")
        .and_then(|value| value.as_str())
        .or_else(|| {
            widget_primary_action
                .get("label")
                .and_then(|value| value.as_str())
        })
        .unwrap_or("Create")
        .to_string();
    let page_primary_action = page
        .get("primaryAction")
        .cloned()
        .unwrap_or_else(|| widget_primary_action.clone());
    let page_search_placeholder = page
        .get("searchPlaceholder")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| format!("Search {}...", display_name.to_lowercase()));
    let page_filter_label = page
        .get("filterLabel")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| format!("All {}", display_name.to_lowercase()));
    let page_sort_label = page
        .get("sortLabel")
        .and_then(|value| value.as_str())
        .unwrap_or("Modified")
        .to_string();
    let page_item_singular = page
        .get("itemSingular")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| display_name.to_lowercase());
    let page_item_plural = page
        .get("itemPlural")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| display_name.to_lowercase());

    json!({
        "sidebar": {
            "enabled": sidebar.get("enabled").and_then(|value| value.as_bool()).unwrap_or(true),
            "order": sidebar.get("order").and_then(|value| value.as_i64()).unwrap_or(default_sidebar_order(module_key)),
            "icon": sidebar.get("icon").and_then(|value| value.as_str()).unwrap_or(icon),
            "label": sidebar.get("label").and_then(|value| value.as_str()).unwrap_or(display_name)
        },
        "dashboard": {
            "enabled": dashboard_enabled,
            "order": dashboard_order,
            "cardTitle": widget_title,
            "cardDescription": widget_description,
            "summaryMode": widget_type,
            "maxItems": max_items,
            "primaryAction": widget_primary_action,
            "widget": {
                "enabled": widget.get("enabled").and_then(|value| value.as_bool()).unwrap_or(dashboard_enabled),
                "type": widget_type,
                "title": widget_title,
                "description": widget_description,
                "size": widget_size,
                "columns": columns,
                "maxItems": max_items,
                "primaryAction": widget_primary_action
            }
        },
        "modulePage": {
            "layout": page_layout,
            "emptyStateTitle": page_empty_title,
            "emptyStateDescription": page_empty_description,
            "emptyStateAction": page_empty_action
        },
        "page": {
            "enabled": page_enabled,
            "route": page_route,
            "renderer": page_renderer,
            "layout": page_layout,
            "emptyStateTitle": page_empty_title,
            "emptyStateDescription": page_empty_description,
            "emptyStateAction": page_empty_action,
            "primaryAction": page_primary_action,
            "searchPlaceholder": page_search_placeholder,
            "filterLabel": page_filter_label,
            "sortLabel": page_sort_label,
            "itemSingular": page_item_singular,
            "itemPlural": page_item_plural
        }
    })
}

fn default_widget_type(module_key: &str) -> &str {
    match module_key {
        "kanban" => "kanban-summary",
        "meetings" => "decisions-meetings-summary",
        "notes" => "latest-notes",
        "shares" => "active-shares",
        _ => "generic-module-summary",
    }
}

fn default_widget_size(module_key: &str) -> &str {
    match module_key {
        "kanban" => "large",
        "meetings" => "medium",
        _ => "small",
    }
}

fn default_widget_columns(widget_size: &str) -> serde_json::Map<String, serde_json::Value> {
    let (desktop, tablet, mobile) = match widget_size {
        "large" => (5, 12, 12),
        "medium" => (4, 6, 12),
        _ => (3, 6, 12),
    };

    serde_json::Map::from_iter([
        ("desktop".to_string(), json!(desktop)),
        ("tablet".to_string(), json!(tablet)),
        ("mobile".to_string(), json!(mobile)),
    ])
}

fn default_primary_action(module_key: &str, default_template: Option<&str>) -> serde_json::Value {
    let label = match module_key {
        "kanban" => "New board",
        "brainstorming" => "New idea board",
        "meetings" => "New meeting note",
        "standups" => "New standup",
        "decisions" => "New decision",
        "shares" => "New share",
        _ => "New note",
    };

    json!({
        "label": label,
        "action": if module_key == "shares" { "generic-create" } else { "create-from-template" },
        "template": default_template
    })
}

fn default_dashboard_enabled(module_key: &str) -> bool {
    !matches!(module_key, "decisions" | "standups")
}

fn default_dashboard_order(module_key: &str) -> i64 {
    match module_key {
        "kanban" => 10,
        "meetings" => 20,
        "notes" => 30,
        "shares" => 40,
        "standups" => 50,
        "decisions" => 60,
        _ => 99,
    }
}

fn default_sidebar_order(module_key: &str) -> i64 {
    match module_key {
        "notes" => 30,
        "meetings" => 40,
        "standups" => 50,
        "kanban" => 60,
        "decisions" => 70,
        "shares" => 80,
        _ => 99,
    }
}

fn default_page_layout(module_key: &str) -> &str {
    match module_key {
        "kanban" => "board",
        _ => "list-grid",
    }
}

fn default_empty_state_title(module_key: &str, display_name: &str) -> String {
    match module_key {
        "notes" => "No notes yet".to_string(),
        "meetings" => "No meeting notes yet".to_string(),
        "standups" => "No standups yet".to_string(),
        "kanban" => "No boards yet".to_string(),
        "decisions" => "No decisions yet".to_string(),
        "shares" => "No active shares".to_string(),
        _ => format!("No {} yet", display_name.to_lowercase()),
    }
}

fn default_empty_state_description(module_key: &str, description: &str) -> String {
    match module_key {
        "notes" => "Create your first file-backed note.".to_string(),
        "meetings" => "Create your first meeting note.".to_string(),
        "standups" => "Create your first standup record.".to_string(),
        "kanban" => "No boards yet. Create your first file-backed board.".to_string(),
        "decisions" => "No decisions recorded yet.".to_string(),
        "shares" => "No active shares.".to_string(),
        _ => description.to_string(),
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
            root_path: None,
            renderer: None,
            default_template: None,
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
            recent_items: vec![SummaryItem {
                id: "uuid-1".to_string(),
                name: "Note 1".to_string(),
                item_type: "file".to_string(),
                updated_at: Utc::now(),
            }],
            extra: None,
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

    #[test]
    fn accepts_approved_module_icons() {
        assert!(validate_module_icon("sticky-note").is_ok());
        assert!(validate_module_icon("calendar-days").is_ok());
    }

    #[test]
    fn rejects_unapproved_module_icons() {
        assert!(validate_module_icon("users").is_err());
        assert!(validate_module_icon("activity").is_err());
    }
}
