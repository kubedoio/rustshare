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
use sqlx::Row;

#[allow(clippy::type_complexity)]
fn default_modules() -> Vec<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static str,
    bool,
    serde_json::Value,
)> {
    vec![
        (
            "notes",
            "Notes",
            "Write OKF-compatible, file-backed notes for durable company memory.",
            "/Workspace/Notes",
            "okf-note",
            Some("template_default_okf_note"),
            "sticky-note",
            true,
            json!({
                "documentFormat": "okf-markdown",
                "okf": { "enabled": true, "conceptType": "Note", "frontmatterRequired": true, "preserveUnknownFields": true },
                "sidebar": { "enabled": true, "order": 30, "icon": "sticky-note", "label": "Notes" },
                "dashboard": { "enabled": true, "order": 10, "cardTitle": "Notes", "cardDescription": "Recent OKF notes.", "summaryMode": "latest-notes", "maxItems": 4, "primaryAction": { "label": "New note", "action": "create-from-template", "template": "template_default_okf_note" }, "widget": { "enabled": true, "type": "latest-notes", "title": "Notes", "description": "Recent OKF notes.", "size": "small", "columns": { "desktop": 3, "tablet": 6, "mobile": 12 }, "maxItems": 4, "primaryAction": { "label": "New note", "action": "create-from-template", "template": "template_default_okf_note" } } },
                "modulePage": { "layout": "list-grid", "emptyStateTitle": "No notes yet", "emptyStateDescription": "Create your first OKF note.", "emptyStateAction": "New note" },
                "page": { "enabled": true, "route": "/modules/notes", "renderer": "okf-note", "layout": "list-grid", "emptyStateTitle": "No notes yet", "emptyStateDescription": "Create your first OKF note.", "emptyStateAction": "New note", "primaryAction": { "label": "New note", "action": "create-from-template", "template": "template_default_okf_note" }, "searchPlaceholder": "Search notes...", "filterLabel": "All notes", "sortLabel": "Modified", "itemSingular": "note", "itemPlural": "notes" }
            }),
        ),
        (
            "meetings",
            "Meeting Notes",
            "Record simple meeting notes, decisions, and follow-up items.",
            "/Workspace/Meetings",
            "meetings",
            Some("template_default_meeting"),
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
            Some("template_default_standup"),
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
            Some("template_default_kanban"),
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
            Some("template_default_decision"),
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
            Some("template_blank_brainstorm"),
            "lightbulb",
            false,
            json!({
                "sidebar": { "enabled": true, "order": 55, "icon": "lightbulb", "label": "Brainstorming" },
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
            Some("template_default_share"),
            "share-2",
            false,
            json!({
                "sidebar": { "enabled": true, "order": 80, "icon": "share-2", "label": "Shares" },
                "dashboard": { "enabled": true, "order": 60, "cardTitle": "Shares", "cardDescription": "Recent shares.", "summaryMode": "shares-overview", "maxItems": 4, "primaryAction": { "label": "New share", "action": "generic-create" } },
                "modulePage": { "layout": "list-grid", "emptyStateTitle": "No active shares", "emptyStateDescription": "Share a file or folder when you are ready.", "emptyStateAction": "New share" }
            }),
        ),
        (
            "mail",
            "Mail",
            "Import, archive, and reference email inside RustShare workspaces.",
            "/Workspace/Mail",
            "mail-list",
            None,
            "mail",
            false,
            json!({
                "sidebar": {
                    "enabled": true,
                    "icon": "mail",
                    "order": 60
                },
                "dashboard": {
                    "enabled": true,
                    "primaryAction": { "label": "Import mail", "action": "generic-create" }
                }
            }),
        ),
    ]
}

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
            rustshare_core::services::FolderError::Database(e) => ModuleError::Database(e),
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
            rustshare_core::services::FileError::Database(e) => ModuleError::Database(e),
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

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ModuleSummary {
    pub module_key: String,
    pub mode: String,
    pub total_items: i64,
    pub recent_items: Vec<SummaryItem>,
    pub extra: Option<serde_json::Value>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
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
        let defaults = default_modules();

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
            let row = sqlx::query!(
                "SELECT EXISTS(SELECT 1 FROM modules WHERE module_key = $1 AND tenant_id = $2) as exists",
                key,
                tenant_id
            )
            .fetch_one(self.metadata_store.pool())
            .await?;
            let exists = row.exists.unwrap_or(false);

            let ai_indexing = if key == "notes" {
                json!({
                    "enabled": true,
                    "source": "okf-frontmatter-and-markdown",
                    "permission_aware": true
                })
            } else {
                json!({"enabled": true})
            };

            if !exists {
                let module = Module {
                    id: Uuid::new_v4(),
                    module_key: key.to_string(),
                    display_name: display_name.to_string(),
                    description: description.to_string(),
                    enabled,
                    root_path: root_path.to_string(),
                    renderer: renderer.to_string(),
                    default_template: default_template.map(str::to_string),
                    icon: icon.to_string(),
                    schema_version: "1.0".to_string(),
                    permissions: json!({
                        "admin_can_configure": true,
                        "workspace_members_can_use": true,
                        "allow_public_share": false,
                        "allow_internal_share": true
                    }),
                    ai_indexing,
                    audit: json!({"enabled": true}),
                    ui_config,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    tenant_id,
                };

                sqlx::query!(
                    r#"
                    INSERT INTO modules (
                        id, module_key, display_name, description, enabled, root_path, renderer,
                        default_template, icon, schema_version, permissions, ai_indexing, audit,
                        ui_config, created_at, updated_at, tenant_id
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
                    "#,
                    module.id,
                    &module.module_key,
                    &module.display_name,
                    &module.description,
                    module.enabled,
                    &module.root_path,
                    &module.renderer,
                    module.default_template.as_deref(),
                    &module.icon,
                    &module.schema_version,
                    &module.permissions,
                    &module.ai_indexing,
                    &module.audit,
                    &module.ui_config,
                    module.created_at,
                    module.updated_at,
                    module.tenant_id
                )
                .execute(self.metadata_store.pool())
                .await?;
            }
        }

        // Fix brainstorming icon for existing installations (pen-tool → lightbulb)
        sqlx::query!(
            r#"
            UPDATE modules
            SET icon = 'lightbulb',
                ui_config = jsonb_set(
                    COALESCE(ui_config, '{}'),
                    '{sidebar,icon}',
                    '"lightbulb"'
                )
            WHERE module_key = 'brainstorming'
              AND tenant_id = $1
              AND icon = 'pen-tool'
            "#,
            tenant_id
        )
        .execute(self.metadata_store.pool())
        .await?;

        // Migrate legacy Notes modules to the OKF-native defaults without overwriting
        // admin UI config changes for other fields.
        let notes_okf_config = json!({
            "enabled": true,
            "conceptType": "Note",
            "frontmatterRequired": true,
            "preserveUnknownFields": true
        });
        let notes_ai_indexing = json!({
            "enabled": true,
            "source": "okf-frontmatter-and-markdown",
            "permission_aware": true
        });
        sqlx::query(
            r#"
            UPDATE modules
            SET renderer = 'okf-note',
                default_template = 'template_default_okf_note',
                ai_indexing = $1,
                ui_config = jsonb_set(
                    jsonb_set(
                        COALESCE(ui_config, '{}'),
                        '{okf}',
                        $2,
                        true
                    ),
                    '{documentFormat}',
                    $3,
                    true
                )
            WHERE module_key = 'notes'
              AND tenant_id = $4
              AND (renderer = 'notes' OR default_template = 'template_default_note')
            "#,
        )
        .bind(&notes_ai_indexing)
        .bind(&notes_okf_config)
        .bind(json!("okf-markdown"))
        .bind(tenant_id)
        .execute(self.metadata_store.pool())
        .await?;

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
        sqlx::query!(
            "UPDATE modules SET enabled = true, updated_at = now() WHERE module_key = $1 AND tenant_id = $2",
            key,
            tenant_id
        )
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

        sqlx::query!(
            "UPDATE modules SET enabled = false, updated_at = now() WHERE module_key = $1 AND tenant_id = $2",
            key,
            tenant_id
        )
        .execute(self.metadata_store.pool())
        .await?;

        self.get_module(key, tenant_id).await
    }

    /// List all modules (for admin).
    pub async fn list_modules(&self, tenant_id: Uuid) -> Result<Vec<Module>, ModuleError> {
        let rows = sqlx::query!(
            "SELECT * FROM modules WHERE tenant_id = $1 ORDER BY display_name",
            tenant_id
        )
        .fetch_all(self.metadata_store.pool())
        .await?;

        let modules: Vec<Module> = rows
            .into_iter()
            .map(|row| Module {
                id: row.id,
                module_key: row.module_key,
                display_name: row.display_name,
                description: row.description.unwrap_or_default(),
                enabled: row.enabled,
                root_path: row.root_path,
                renderer: row.renderer,
                default_template: row.default_template,
                icon: row.icon.unwrap_or_default(),
                schema_version: row.schema_version.unwrap_or_default(),
                permissions: row.permissions,
                ai_indexing: row.ai_indexing,
                audit: row.audit,
                ui_config: row.ui_config,
                created_at: row.created_at,
                updated_at: row.updated_at,
                tenant_id: row.tenant_id,
            })
            .collect();

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
        let rows = sqlx::query!(
            "SELECT * FROM modules WHERE enabled = true AND tenant_id = $1 ORDER BY display_name",
            tenant_id
        )
        .fetch_all(self.metadata_store.pool())
        .await?;

        let modules: Vec<Module> = rows
            .into_iter()
            .map(|row| Module {
                id: row.id,
                module_key: row.module_key,
                display_name: row.display_name,
                description: row.description.unwrap_or_default(),
                enabled: row.enabled,
                root_path: row.root_path,
                renderer: row.renderer,
                default_template: row.default_template,
                icon: row.icon.unwrap_or_default(),
                schema_version: row.schema_version.unwrap_or_default(),
                permissions: row.permissions,
                ai_indexing: row.ai_indexing,
                audit: row.audit,
                ui_config: row.ui_config,
                created_at: row.created_at,
                updated_at: row.updated_at,
                tenant_id: row.tenant_id,
            })
            .collect();

        Ok(modules
            .into_iter()
            .map(|m| self.normalize_module(m))
            .filter(|module| user_can_access_module(module, is_admin))
            .collect())
    }

    /// Get a single module by key.
    pub async fn get_module(&self, key: &str, tenant_id: Uuid) -> Result<Module, ModuleError> {
        let row = sqlx::query!(
            "SELECT * FROM modules WHERE module_key = $1 AND tenant_id = $2",
            key,
            tenant_id
        )
        .fetch_optional(self.metadata_store.pool())
        .await?;

        let module: Option<Module> = row.map(|row| Module {
            id: row.id,
            module_key: row.module_key,
            display_name: row.display_name,
            description: row.description.unwrap_or_default(),
            enabled: row.enabled,
            root_path: row.root_path,
            renderer: row.renderer,
            default_template: row.default_template,
            icon: row.icon.unwrap_or_default(),
            schema_version: row.schema_version.unwrap_or_default(),
            permissions: row.permissions,
            ai_indexing: row.ai_indexing,
            audit: row.audit,
            ui_config: row.ui_config,
            created_at: row.created_at,
            updated_at: row.updated_at,
            tenant_id: row.tenant_id,
        });

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
        let root_path = input.root_path.clone().unwrap_or(module.root_path);
        // Only enforce canonical path when explicitly changing root_path.
        // Existing legacy modules may keep their root path for read compatibility.
        if input.root_path.is_some() {
            validate_root_path(&root_path)?;
        }
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

        sqlx::query!(
            r#"
            UPDATE modules
            SET display_name = $1, description = $2, icon = $3, root_path = $4,
                renderer = $5, default_template = $6, permissions = $7,
                ai_indexing = $8, audit = $9, ui_config = $10, updated_at = now()
            WHERE module_key = $11 AND tenant_id = $12
            "#,
            display_name,
            description,
            icon,
            root_path,
            renderer,
            default_template,
            permissions,
            ai_indexing,
            audit,
            ui_config,
            key,
            tenant_id
        )
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

        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM files f
            WHERE f.tenant_id = $1
              AND f.deleted_at IS NULL
              AND f.path LIKE $3
              AND (
                f.owner_id = $2
                OR EXISTS (
                  SELECT 1
                  FROM shares s
                  LEFT JOIN group_members gm
                    ON gm.group_id = s.recipient_group_id
                   AND gm.user_id = $2
                  LEFT JOIN folders shared_folder
                    ON shared_folder.id = s.folder_id
                   AND shared_folder.deleted_at IS NULL
                  WHERE s.revoked_at IS NULL
                    AND (s.expires_at IS NULL OR s.expires_at > NOW())
                    AND (s.recipient_user_id = $2 OR gm.user_id IS NOT NULL)
                    AND (
                      s.file_id = f.id
                      OR (
                        shared_folder.id IS NOT NULL
                        AND f.path LIKE shared_folder.path || '/%'
                      )
                    )
                )
              )
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(&path_prefix)
        .fetch_one(self.metadata_store.pool())
        .await;
        let file_count = row
            .map(|r| r.try_get::<i64, _>("count").unwrap_or(0))
            .unwrap_or(0);

        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM folders f
            WHERE f.tenant_id = $1
              AND f.deleted_at IS NULL
              AND f.path LIKE $3
              AND f.path <> $4
              AND (
                f.owner_id = $2
                OR EXISTS (
                  SELECT 1
                  FROM shares s
                  LEFT JOIN group_members gm
                    ON gm.group_id = s.recipient_group_id
                   AND gm.user_id = $2
                  LEFT JOIN folders shared_folder
                    ON shared_folder.id = s.folder_id
                   AND shared_folder.deleted_at IS NULL
                  WHERE s.revoked_at IS NULL
                    AND (s.expires_at IS NULL OR s.expires_at > NOW())
                    AND (s.recipient_user_id = $2 OR gm.user_id IS NOT NULL)
                    AND shared_folder.id IS NOT NULL
                    AND (f.id = shared_folder.id OR f.path LIKE shared_folder.path || '/%')
                )
              )
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(&path_prefix)
        .bind(&root_path)
        .fetch_one(self.metadata_store.pool())
        .await;
        let folder_count = row
            .map(|r| r.try_get::<i64, _>("count").unwrap_or(0))
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
            .create_folder_or_get("Workspace".into(), None, owner_id, tenant_id)
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
            .create_folder_or_get(root_name, Some(ws.id), owner_id, tenant_id)
            .await?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
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
                let row = sqlx::query!(
                    "SELECT COUNT(*) as count FROM folders WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND (path LIKE '/Shares/Public/%' OR path LIKE '/Workspace/Shares/Public/%')",
                    tenant_id,
                    user_id
                )
                .fetch_one(self.metadata_store.pool())
                .await;
                let public_count = row.map(|r| r.count.unwrap_or(0)).unwrap_or(0);
                let row = sqlx::query!(
                    "SELECT COUNT(*) as count FROM folders WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND (path LIKE '/Shares/Internal/%' OR path LIKE '/Workspace/Shares/Internal/%')",
                    tenant_id,
                    user_id
                )
                .fetch_one(self.metadata_store.pool())
                .await;
                let internal_count = row.map(|r| r.count.unwrap_or(0)).unwrap_or(0);
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
        let rows = sqlx::query(
            r#"
            SELECT f.id, f.name, f.modified_at, f.parent_folder_id, pf.name as parent_name
            FROM files f
            LEFT JOIN folders pf ON f.parent_folder_id = pf.id
            WHERE f.tenant_id = $1
              AND f.deleted_at IS NULL
              AND f.path LIKE $3
              AND (
                f.owner_id = $2
                OR EXISTS (
                  SELECT 1
                  FROM shares s
                  LEFT JOIN group_members gm
                    ON gm.group_id = s.recipient_group_id
                   AND gm.user_id = $2
                  LEFT JOIN folders shared_folder
                    ON shared_folder.id = s.folder_id
                   AND shared_folder.deleted_at IS NULL
                  WHERE s.revoked_at IS NULL
                    AND (s.expires_at IS NULL OR s.expires_at > NOW())
                    AND (s.recipient_user_id = $2 OR gm.user_id IS NOT NULL)
                    AND (
                      s.file_id = f.id
                      OR (
                        shared_folder.id IS NOT NULL
                        AND f.path LIKE shared_folder.path || '/%'
                      )
                    )
                )
              )
            ORDER BY f.modified_at DESC
            LIMIT $4
            "#,
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(path_prefix)
        .bind(max_items)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let name: String = row.get("name");
                let parent_name: Option<String> = row.try_get("parent_name").unwrap_or(None);
                let display_name = if name == "note.md" {
                    parent_name.unwrap_or(name)
                } else {
                    name
                };
                SummaryItem {
                    id: row.get::<Uuid, _>("id").to_string(),
                    name: display_name,
                    item_type: "file".to_string(),
                    updated_at: row.get("modified_at"),
                }
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
        let rows = sqlx::query(
            r#"
            SELECT f.id, f.name, f.updated_at
            FROM folders f
            WHERE f.tenant_id = $1
              AND f.deleted_at IS NULL
              AND f.path LIKE $3
              AND (
                f.owner_id = $2
                OR EXISTS (
                  SELECT 1
                  FROM shares s
                  LEFT JOIN group_members gm
                    ON gm.group_id = s.recipient_group_id
                   AND gm.user_id = $2
                  LEFT JOIN folders shared_folder
                    ON shared_folder.id = s.folder_id
                   AND shared_folder.deleted_at IS NULL
                  WHERE s.revoked_at IS NULL
                    AND (s.expires_at IS NULL OR s.expires_at > NOW())
                    AND (s.recipient_user_id = $2 OR gm.user_id IS NOT NULL)
                    AND shared_folder.id IS NOT NULL
                    AND (f.id = shared_folder.id OR f.path LIKE shared_folder.path || '/%')
                )
              )
            ORDER BY f.updated_at DESC
            LIMIT $4
            "#,
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(path_prefix)
        .bind(max_items)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| SummaryItem {
                id: row.get::<Uuid, _>("id").to_string(),
                name: row.get("name"),
                item_type: "folder".to_string(),
                updated_at: row.get("updated_at"),
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
        let rows = sqlx::query(
            r#"
            SELECT f.id, f.name, f.updated_at
            FROM folders f
            WHERE f.tenant_id = $1
              AND f.deleted_at IS NULL
              AND f.path LIKE $3
              AND f.name LIKE $4
              AND (
                f.owner_id = $2
                OR EXISTS (
                  SELECT 1
                  FROM shares s
                  LEFT JOIN group_members gm
                    ON gm.group_id = s.recipient_group_id
                   AND gm.user_id = $2
                  LEFT JOIN folders shared_folder
                    ON shared_folder.id = s.folder_id
                   AND shared_folder.deleted_at IS NULL
                  WHERE s.revoked_at IS NULL
                    AND (s.expires_at IS NULL OR s.expires_at > NOW())
                    AND (s.recipient_user_id = $2 OR gm.user_id IS NOT NULL)
                    AND shared_folder.id IS NOT NULL
                    AND (f.id = shared_folder.id OR f.path LIKE shared_folder.path || '/%')
                )
              )
            ORDER BY f.updated_at DESC
            LIMIT $5
            "#,
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
            .map(|row| SummaryItem {
                id: row.get::<Uuid, _>("id").to_string(),
                name: row.get("name"),
                item_type: "folder".to_string(),
                updated_at: row.get("updated_at"),
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
        let rows = sqlx::query(
            r#"
            SELECT f.id, f.name, f.updated_at
            FROM folders f
            JOIN folders root
              ON root.id = f.parent_folder_id
             AND root.path = $3
             AND root.tenant_id = $1
             AND root.deleted_at IS NULL
            WHERE f.tenant_id = $1
              AND f.deleted_at IS NULL
              AND (
                f.owner_id = $2
                OR EXISTS (
                  SELECT 1
                  FROM shares s
                  LEFT JOIN group_members gm
                    ON gm.group_id = s.recipient_group_id
                   AND gm.user_id = $2
                  LEFT JOIN folders shared_folder
                    ON shared_folder.id = s.folder_id
                   AND shared_folder.deleted_at IS NULL
                  WHERE s.revoked_at IS NULL
                    AND (s.expires_at IS NULL OR s.expires_at > NOW())
                    AND (s.recipient_user_id = $2 OR gm.user_id IS NOT NULL)
                    AND shared_folder.id IS NOT NULL
                    AND (f.id = shared_folder.id OR f.path LIKE shared_folder.path || '/%')
                )
              )
            ORDER BY f.updated_at DESC
            LIMIT $4
            "#,
        )
        .bind(tenant_id)
        .bind(owner_id)
        .bind(root_path)
        .bind(max_items)
        .fetch_all(self.metadata_store.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| SummaryItem {
                id: row.get::<Uuid, _>("id").to_string(),
                name: row.get("name"),
                item_type: "folder".to_string(),
                updated_at: row.get("updated_at"),
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
        let row = sqlx::query!(
            "SELECT COALESCE(is_admin, false) as is_admin FROM users WHERE id = $1 AND tenant_id = $2",
            user_id,
            tenant_id
        )
        .fetch_optional(self.metadata_store.pool())
        .await?;
        let is_admin = row.and_then(|r| r.is_admin).unwrap_or(false);
        Ok(is_admin)
    }

    async fn find_admin_user_for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<UserId>, ModuleError> {
        let row = sqlx::query!(
            "SELECT id FROM users WHERE tenant_id = $1 AND is_admin = true ORDER BY created_at ASC LIMIT 1",
            tenant_id
        )
        .fetch_optional(self.metadata_store.pool())
        .await?;
        let admin_id = row.map(|r| r.id);
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

    // Enforce canonical /Workspace prefix for new/changed module roots.
    // Legacy roots are read-only; new writes must be under /Workspace.
    if !root_path.starts_with("/Workspace/") {
        return Err(ModuleError::InvalidName(format!(
            "Root path must be under /Workspace: {root_path}"
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

    let mut result = existing.clone();
    result.insert(
        "sidebar".to_string(),
        json!({
            "enabled": sidebar.get("enabled").and_then(|value| value.as_bool()).unwrap_or(true),
            "order": sidebar.get("order").and_then(|value| value.as_i64()).unwrap_or(default_sidebar_order(module_key)),
            "icon": sidebar.get("icon").and_then(|value| value.as_str()).unwrap_or(icon),
            "label": sidebar.get("label").and_then(|value| value.as_str()).unwrap_or(display_name)
        }),
    );
    result.insert(
        "dashboard".to_string(),
        json!({
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
        }),
    );
    result.insert(
        "modulePage".to_string(),
        json!({
            "layout": page_layout,
            "emptyStateTitle": page_empty_title,
            "emptyStateDescription": page_empty_description,
            "emptyStateAction": page_empty_action
        }),
    );
    result.insert(
        "page".to_string(),
        json!({
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
        }),
    );

    serde_json::Value::Object(result)
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
        "mail" => "Import mail",
        _ => "New note",
    };

    json!({
        "label": label,
        "action": if matches!(module_key, "shares" | "mail") { "generic-create" } else { "create-from-template" },
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
        "kanban" => "kanban-board",
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
    fn test_normalize_module_ui_config_contract() {
        let ui = normalize_module_ui_config(
            "notes",
            "Notes",
            "Write OKF-compatible, file-backed notes for durable company memory.",
            "sticky-note",
            "/Workspace/Notes",
            "okf-note",
            Some("template_default_okf_note"),
            Some(serde_json::json!({
                "documentFormat": "okf-markdown",
                "okf": {"enabled": true, "conceptType": "Note", "frontmatterRequired": true, "preserveUnknownFields": true},
                "sidebar": {"enabled": true, "order": 30, "icon": "sticky-note", "label": "Notes"},
                "dashboard": {"enabled": true, "order": 10, "cardTitle": "Notes", "cardDescription": "Recent OKF notes.", "summaryMode": "latest-notes", "maxItems": 4, "primaryAction": {"label": "New note", "action": "create-from-template", "template": "template_default_okf_note"}, "widget": {"enabled": true, "type": "latest-notes", "title": "Notes", "description": "Recent OKF notes.", "size": "small", "columns": {"desktop": 3, "tablet": 6, "mobile": 12}, "maxItems": 4, "primaryAction": {"label": "New note", "action": "create-from-template", "template": "template_default_okf_note"}}},
                "modulePage": {"layout": "list-grid", "emptyStateTitle": "No notes yet", "emptyStateDescription": "Create your first OKF note.", "emptyStateAction": "New note"},
                "page": {"enabled": true, "route": "/modules/notes", "renderer": "okf-note", "layout": "list-grid", "emptyStateTitle": "No notes yet", "emptyStateDescription": "Create your first OKF note.", "emptyStateAction": "New note", "primaryAction": {"label": "New note", "action": "create-from-template", "template": "template_default_okf_note"}, "searchPlaceholder": "Search notes...", "filterLabel": "All notes", "sortLabel": "Modified", "itemSingular": "note", "itemPlural": "notes"}
            })),
        );

        let ui_obj = ui.as_object().unwrap();

        // Canonical page key must exist
        assert!(ui_obj.contains_key("page"), "canonical 'page' key missing");
        // Legacy alias must exist for backward compatibility
        assert!(
            ui_obj.contains_key("modulePage"),
            "legacy 'modulePage' alias missing"
        );
        // OKF metadata must be preserved
        assert!(ui_obj.contains_key("okf"), "okf config missing");
        assert_eq!(
            ui_obj.get("documentFormat").unwrap().as_str().unwrap(),
            "okf-markdown"
        );

        let page = ui_obj.get("page").unwrap().as_object().unwrap();
        assert_eq!(
            page.get("route").unwrap().as_str().unwrap(),
            "/modules/notes"
        );
        assert_eq!(page.get("renderer").unwrap().as_str().unwrap(), "okf-note");
        assert_eq!(page.get("layout").unwrap().as_str().unwrap(), "list-grid");
        assert!(page.get("enabled").unwrap().as_bool().unwrap());

        let dashboard = ui_obj.get("dashboard").unwrap().as_object().unwrap();
        let widget = dashboard.get("widget").unwrap().as_object().unwrap();
        assert_eq!(
            widget.get("type").unwrap().as_str().unwrap(),
            "latest-notes"
        );
        assert_eq!(widget.get("size").unwrap().as_str().unwrap(), "small");
        assert_eq!(widget.get("maxItems").unwrap().as_i64().unwrap(), 4);
        assert_eq!(
            widget
                .get("primaryAction")
                .unwrap()
                .get("template")
                .unwrap()
                .as_str()
                .unwrap(),
            "template_default_okf_note"
        );

        // Columns must be present
        let columns = widget.get("columns").unwrap().as_object().unwrap();
        assert!(columns.contains_key("desktop"));
        assert!(columns.contains_key("tablet"));
        assert!(columns.contains_key("mobile"));
    }

    #[test]
    fn test_normalize_module_ui_config_defaults_match_contract() {
        let ui = normalize_module_ui_config(
            "kanban",
            "Kanban",
            "Organize work.",
            "columns",
            "/Workspace/Kanban",
            "kanban",
            Some("template_default_kanban"),
            None,
        );

        let ui_obj = ui.as_object().unwrap();
        let dashboard = ui_obj.get("dashboard").unwrap().as_object().unwrap();
        let widget = dashboard.get("widget").unwrap().as_object().unwrap();

        // Default widget types must not drift from contract
        assert_eq!(
            widget.get("type").unwrap().as_str().unwrap(),
            "kanban-summary"
        );
        assert_eq!(widget.get("size").unwrap().as_str().unwrap(), "large");

        let page = ui_obj.get("page").unwrap().as_object().unwrap();
        assert_eq!(
            page.get("layout").unwrap().as_str().unwrap(),
            "kanban-board"
        );
        assert_eq!(
            page.get("route").unwrap().as_str().unwrap(),
            "/modules/kanban"
        );
    }

    #[test]
    fn notes_default_module_is_okf_native() {
        let defaults = default_modules();
        let notes = defaults
            .iter()
            .find(|(k, _, _, _, _, _, _, _, _)| *k == "notes")
            .expect("notes module must exist");
        let (
            _,
            display_name,
            description,
            root_path,
            renderer,
            default_template,
            icon,
            enabled,
            ui_config,
        ) = notes;

        assert_eq!(*display_name, "Notes");
        assert_eq!(
            *description,
            "Write OKF-compatible, file-backed notes for durable company memory."
        );
        assert_eq!(*root_path, "/Workspace/Notes");
        assert_eq!(*renderer, "okf-note");
        assert_eq!(*default_template, Some("template_default_okf_note"));
        assert_eq!(*icon, "sticky-note");
        assert!(*enabled);

        let ui = ui_config.as_object().expect("ui_config must be an object");
        assert_eq!(
            ui.get("documentFormat").unwrap().as_str().unwrap(),
            "okf-markdown"
        );
        let okf = ui.get("okf").unwrap().as_object().unwrap();
        assert!(okf.get("enabled").unwrap().as_bool().unwrap());
        assert_eq!(okf.get("conceptType").unwrap().as_str().unwrap(), "Note");
        assert!(okf.get("frontmatterRequired").unwrap().as_bool().unwrap());
        assert!(okf.get("preserveUnknownFields").unwrap().as_bool().unwrap());
    }

    #[test]
    fn non_notes_default_modules_are_unaffected_by_okf_change() {
        let defaults = default_modules();
        for (key, _, _, _, renderer, default_template, _, _, _) in &defaults {
            if *key == "notes" {
                continue;
            }
            assert_ne!(
                *renderer, "okf-note",
                "module {} should not use the notes renderer",
                key
            );
            assert_ne!(
                *default_template,
                Some("template_default_okf_note"),
                "module {} should not use the notes default template",
                key
            );
        }
    }

    #[test]
    fn mail_default_module_uses_registered_template_and_icon() {
        let defaults = default_modules();
        let mail = defaults
            .iter()
            .find(|(key, _, _, _, _, _, _, _, _)| *key == "mail")
            .expect("mail module must exist");
        let (_, _, _, _, renderer, default_template, icon, enabled, ui_config) = mail;

        assert_eq!(*renderer, "mail-list");
        assert!(default_template.is_none());
        assert_eq!(*icon, "mail");
        assert!(validate_module_icon(icon).is_ok());
        assert!(!enabled);

        let sidebar_icon = ui_config
            .get("sidebar")
            .and_then(|sidebar| sidebar.get("icon"))
            .and_then(|icon| icon.as_str())
            .expect("mail sidebar icon");
        assert_eq!(sidebar_icon, "mail");

        let primary_action = ui_config
            .get("dashboard")
            .and_then(|dashboard| dashboard.get("primaryAction"))
            .expect("mail primary action");
        assert_eq!(
            primary_action
                .get("action")
                .and_then(|value| value.as_str()),
            Some("generic-create")
        );
        assert!(primary_action.get("template").is_none());
    }

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
        assert!(validate_module_icon("invalid-random-icon").is_err());
        assert!(validate_module_icon("script<alert>1</alert>").is_err());
    }

    #[test]
    fn default_module_definitions_use_canonical_workspace_root_paths() {
        // This test protects the legacy module root policy at the registry level:
        // all predefined modules must use /Workspace/<Module> so that new writes
        // go to the canonical path.
        let expected_roots = [
            ("notes", "/Workspace/Notes"),
            ("meetings", "/Workspace/Meetings"),
            ("standups", "/Workspace/Standups"),
            ("kanban", "/Workspace/Kanban"),
            ("decisions", "/Workspace/Decisions"),
            ("brainstorming", "/Workspace/Brainstorming"),
            ("shares", "/Workspace/Shares"),
        ];

        let defaults = default_modules();
        for (key, expected) in expected_roots {
            let found = defaults
                .iter()
                .find(|(k, _, _, _, _, _, _, _, _)| *k == key);
            assert!(found.is_some(), "default module {} must exist", key);
            let (_, _, _, root_path, _, _, _, _, _) = found.unwrap();
            assert_eq!(
                *root_path, expected,
                "module {} must use canonical workspace root",
                key
            );
        }
    }

    #[test]
    fn validate_root_path_accepts_canonical_workspace_paths() {
        assert!(validate_root_path("/Workspace/Notes").is_ok());
        assert!(validate_root_path("/Workspace/Meetings").is_ok());
        assert!(validate_root_path("/Workspace/Decisions").is_ok());
    }

    #[test]
    fn validate_root_path_rejects_invalid_paths() {
        assert!(validate_root_path("Notes").is_err()); // missing leading slash
        assert!(validate_root_path("/").is_err()); // root only
        assert!(validate_root_path("/../Notes").is_err()); // path traversal
    }

    #[test]
    fn validate_root_path_rejects_legacy_roots() {
        // Legacy roots are read-only; new/changed module roots must be canonical.
        assert!(validate_root_path("/Notes").is_err());
        assert!(validate_root_path("/Meetings").is_err());
        assert!(validate_root_path("/Standups").is_err());
        assert!(validate_root_path("/Kanban").is_err());
        assert!(validate_root_path("/Decisions").is_err());
        assert!(validate_root_path("/Brainstorming").is_err());
        assert!(validate_root_path("/Shares").is_err());
    }

    #[test]
    fn validate_root_path_accepts_nested_workspace_paths() {
        assert!(validate_root_path("/Workspace/Notes/Archive").is_ok());
        assert!(validate_root_path("/Workspace/Meetings/2026").is_ok());
    }
}
