//! ApplicationConfig service for workspace Application configuration.

use chrono::Utc;
use rustshare_core::{
    domain::{ApplicationConfig, ApplicationRegistry, ApplicationShellEntry, UserId},
    services::FolderService,
};
use rustshare_storage::MetadataStore;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::services::icon_registry::is_approved_icon_key;
use rustshare_infrastructure::repositories::PermissionResolverRepository;
use sqlx::Row;

/// Errors that can occur in Application operations.
#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    #[error("Application not found: {0}")]
    NotFound(String),
    #[error("Application already exists: {0}")]
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

impl From<rustshare_core::services::FolderError> for ApplicationError {
    fn from(e: rustshare_core::services::FolderError) -> Self {
        match e {
            rustshare_core::services::FolderError::NotFound(id) => {
                ApplicationError::NotFound(id.to_string())
            }
            rustshare_core::services::FolderError::PermissionDenied { .. } => {
                ApplicationError::PermissionDenied
            }
            rustshare_core::services::FolderError::InvalidName(s) => {
                ApplicationError::InvalidName(s)
            }
            rustshare_core::services::FolderError::DuplicateName { .. } => {
                ApplicationError::AlreadyExists("folder".to_string())
            }
            rustshare_core::services::FolderError::Database(e) => ApplicationError::Database(e),
            _ => ApplicationError::Storage(e.to_string()),
        }
    }
}

impl From<rustshare_core::services::FileError> for ApplicationError {
    fn from(e: rustshare_core::services::FileError) -> Self {
        match e {
            rustshare_core::services::FileError::NotFound(id) => {
                ApplicationError::NotFound(id.to_string())
            }
            rustshare_core::services::FileError::PermissionDenied { .. } => {
                ApplicationError::PermissionDenied
            }
            rustshare_core::services::FileError::InvalidName(s) => ApplicationError::InvalidName(s),
            rustshare_core::services::FileError::Database(e) => ApplicationError::Database(e),
            _ => ApplicationError::Storage(e.to_string()),
        }
    }
}

impl From<sqlx::Error> for ApplicationError {
    fn from(e: sqlx::Error) -> Self {
        ApplicationError::Database(e.to_string())
    }
}

/// Service for managing workspace Applications.
pub struct ApplicationService {
    folder_service: Arc<
        FolderService<rustshare_storage::EventStore, MetadataStore, PermissionResolverRepository>,
    >,
    metadata_store: Arc<MetadataStore>,
    registry: Arc<ApplicationRegistry>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ApplicationSummary {
    pub application_id: String,
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
pub struct UpdateApplicationInput {
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

impl ApplicationService {
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
        Self::with_registry(
            folder_service,
            metadata_store,
            Arc::new(ApplicationRegistry::first_party().expect("first-party manifests are valid")),
        )
    }

    pub fn with_registry(
        folder_service: Arc<
            FolderService<
                rustshare_storage::EventStore,
                MetadataStore,
                PermissionResolverRepository,
            >,
        >,
        metadata_store: Arc<MetadataStore>,
        registry: Arc<ApplicationRegistry>,
    ) -> Self {
        Self {
            folder_service,
            metadata_store,
            registry,
        }
    }

    pub fn registry(&self) -> &ApplicationRegistry {
        &self.registry
    }

    fn manifest(
        &self,
        key: &str,
    ) -> Result<&rustshare_core::domain::ApplicationManifest, ApplicationError> {
        self.registry
            .available()
            .find(|manifest| manifest.metadata.id.0 == key)
            .ok_or_else(|| ApplicationError::NotFound(key.to_string()))
    }

    fn application_id_for_route_slug(&self, route_slug: &str) -> Result<String, ApplicationError> {
        self.registry
            .available()
            .find(|manifest| {
                manifest
                    .contributions
                    .navigation
                    .iter()
                    .any(|contribution| {
                        contribution
                            .route
                            .as_deref()
                            .and_then(|route| route.strip_prefix("/apps/"))
                            == Some(route_slug)
                    })
            })
            .map(|manifest| manifest.metadata.id.0.clone())
            .ok_or_else(|| ApplicationError::NotFound(route_slug.to_string()))
    }

    pub async fn list_enabled_application_shell(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ApplicationShellEntry>, ApplicationError> {
        let rows = sqlx::query(
            "SELECT application_id, configuration, enabled, health
             FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $1 AND enabled = true",
        )
        .bind(tenant_id)
        .fetch_all(self.metadata_store.pool())
        .await?;

        let mut entries = Vec::new();
        for row in rows {
            let application_id: String = row.try_get("application_id")?;
            let Some(manifest) = self
                .registry
                .available()
                .find(|manifest| manifest.metadata.id.0 == application_id)
            else {
                continue;
            };
            let health = match row.try_get::<String, _>("health")?.as_str() {
                "degraded" => rustshare_core::domain::ApplicationHealth::Degraded,
                "unavailable" => rustshare_core::domain::ApplicationHealth::Unavailable,
                _ => rustshare_core::domain::ApplicationHealth::Healthy,
            };
            entries.push(ApplicationShellEntry {
                manifest: manifest.clone(),
                enabled: true,
                configuration: row.try_get("configuration")?,
                health,
            });
        }
        Ok(entries)
    }

    pub async fn get_application_shell(
        &self,
        route_slug: &str,
        tenant_id: Uuid,
    ) -> Result<ApplicationShellEntry, ApplicationError> {
        self.list_enabled_application_shell(tenant_id)
            .await?
            .into_iter()
            .find(|entry| {
                entry
                    .manifest
                    .contributions
                    .navigation
                    .iter()
                    .any(|contribution| {
                        contribution
                            .route
                            .as_deref()
                            .and_then(|route| route.strip_prefix("/apps/"))
                            == Some(route_slug)
                    })
            })
            .ok_or_else(|| ApplicationError::NotFound(route_slug.to_string()))
    }

    fn application_config_from_manifest(
        &self,
        manifest: &rustshare_core::domain::ApplicationManifest,
        enabled: bool,
        configuration: serde_json::Value,
        tenant_id: Uuid,
    ) -> ApplicationConfig {
        let key = manifest.metadata.id.0.as_str();
        let slug = manifest
            .contributions
            .navigation
            .iter()
            .find_map(|contribution| contribution.route.as_deref())
            .and_then(|route| route.strip_prefix("/apps/"))
            .unwrap_or_else(|| key.rsplit('.').next().unwrap_or(key));
        let route_renderer = manifest
            .contributions
            .routes
            .first()
            .and_then(|contribution| contribution.renderer.as_deref())
            .unwrap_or(slug);
        let mut persisted = configuration.as_object().cloned().unwrap_or_default();
        let display_name = persisted
            .get("displayName")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&manifest.metadata.name)
            .to_string();
        let description = persisted
            .get("description")
            .and_then(|value| value.as_str())
            .unwrap_or(&manifest.metadata.description)
            .to_string();
        let default_template = match key {
            "io.elembra.notes" => Some("template_default_okf_note"),
            "io.elembra.meetings" => Some("template_default_meeting"),
            "io.elembra.standups" => Some("template_default_standup"),
            "io.elembra.kanban" => Some("template_default_kanban"),
            "io.elembra.decisions" => Some("template_default_decision"),
            "io.elembra.brainstorming" => Some("template_blank_brainstorm"),
            "io.elembra.shares" => Some("template_default_share"),
            _ => None,
        };
        let mut ui_config = persisted.remove("ui").unwrap_or_else(|| {
            let navigation = manifest.contributions.navigation.first();
            let dashboard = manifest.contributions.dashboard.first();
            let route = manifest.contributions.routes.first();
            json!({
                "sidebar": {
                    "enabled": navigation.is_some(),
                    "order": navigation.and_then(|c| c.order).unwrap_or(99),
                    "icon": navigation.and_then(|c| c.icon.as_deref()).unwrap_or("layout-dashboard"),
                    "label": navigation.and_then(|c| c.label.as_deref()).unwrap_or(&display_name)
                },
                "dashboard": {
                    "enabled": dashboard.is_some(),
                    "order": dashboard.and_then(|c| c.order).unwrap_or(99),
                    "summaryMode": dashboard.and_then(|c| c.renderer.as_deref()).unwrap_or("application-summary"),
                    "widget": {
                        "enabled": dashboard.is_some(),
                        "type": dashboard.and_then(|c| c.renderer.as_deref()).unwrap_or("application-summary"),
                        "title": display_name,
                        "description": description,
                        "size": "medium",
                        "columns": { "desktop": 6, "tablet": 12, "mobile": 12 },
                        "maxItems": 4
                    }
                },
                "page": {
                    "enabled": route.is_some(),
                    "route": route.and_then(|c| c.route.as_deref()).unwrap_or("/apps/"),
                    "renderer": route_renderer,
                    "layout": "list-grid",
                    "emptyStateTitle": format!("No {} yet", display_name.to_lowercase()),
                    "emptyStateDescription": description,
                    "emptyStateAction": format!("Create {}", display_name.to_lowercase())
                }
            })
        });
        if key == "io.elembra.notes" {
            if let Some(ui) = ui_config.as_object_mut() {
                ui.insert("documentFormat".to_string(), json!("okf-markdown"));
                ui.insert(
                    "okf".to_string(),
                    json!({
                        "enabled": true,
                        "conceptType": "Note",
                        "frontmatterRequired": true,
                        "preserveUnknownFields": true
                    }),
                );
            }
        }
        let root_path = persisted
            .get("rootPath")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("/Workspace/{}", title_case(slug)));
        let renderer = persisted
            .get("renderer")
            .and_then(|value| value.as_str())
            .unwrap_or(route_renderer)
            .to_string();
        let icon = persisted
            .get("icon")
            .and_then(|value| value.as_str())
            .or_else(|| {
                manifest
                    .contributions
                    .navigation
                    .first()
                    .and_then(|c| c.icon.as_deref())
            })
            .unwrap_or("layout-dashboard")
            .to_string();
        let id = Uuid::new_v4();
        ApplicationConfig {
            id,
            application_id: key.to_string(),
            display_name,
            description,
            enabled,
            root_path,
            renderer,
            default_template: persisted
                .get("defaultTemplate")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| default_template.map(str::to_string)),
            icon,
            schema_version: manifest.api_version.clone(),
            permissions: persisted.remove("permissions").unwrap_or_else(|| {
                json!({
                    "admin_can_configure": true,
                    "workspace_members_can_use": true,
                    "allow_public_share": false,
                    "allow_internal_share": true
                })
            }),
            ai_indexing: persisted.remove("aiIndexing").unwrap_or_else(|| {
                if key == "io.elembra.notes" {
                    json!({
                        "enabled": true,
                        "source": "okf-frontmatter-and-markdown",
                        "permission_aware": true
                    })
                } else {
                    json!({"enabled": true})
                }
            }),
            audit: persisted
                .remove("audit")
                .unwrap_or_else(|| json!({"enabled": true})),
            ui_config,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            tenant_id,
        }
    }

    /// Ensure every build-time manifest has tenant/workspace state without overwriting intent.
    pub async fn ensure_default_applications(
        &self,
        tenant_id: Uuid,
    ) -> Result<(), ApplicationError> {
        let manifests = self.registry.available().cloned().collect::<Vec<_>>();
        for manifest in manifests {
            let key = manifest.metadata.id.0.as_str();
            let enabled = key == "io.elembra.notes";
            let existing = sqlx::query(
                "SELECT enabled, configuration FROM application_enablements
                 WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
            )
            .bind(tenant_id)
            .bind(key)
            .fetch_optional(self.metadata_store.pool())
            .await?;
            let (enabled, configuration) = if let Some(row) = existing {
                (row.try_get("enabled")?, row.try_get("configuration")?)
            } else {
                (enabled, serde_json::json!({}))
            };
            let config =
                self.application_config_from_manifest(&manifest, enabled, configuration, tenant_id);

            sqlx::query(
                "INSERT INTO application_enablements
                    (tenant_id, workspace_id, application_id, enabled, configuration)
                 VALUES ($1, $1, $2, $3, $4)
                 ON CONFLICT (tenant_id, workspace_id, application_id) DO NOTHING",
            )
            .bind(tenant_id)
            .bind(key)
            .bind(enabled)
            .bind(json!({
                "displayName": config.display_name,
                "description": config.description,
                "rootPath": config.root_path,
                "renderer": config.renderer,
                "defaultTemplate": config.default_template,
                "icon": config.icon,
                "permissions": config.permissions,
                "aiIndexing": config.ai_indexing,
                "audit": config.audit,
                "ui": config.ui_config
            }))
            .execute(self.metadata_store.pool())
            .await?;
        }

        if let Some(admin_id) = self.find_admin_user_for_tenant(tenant_id).await? {
            let enabled_applications = self
                .list_applications(tenant_id)
                .await?
                .into_iter()
                .filter(|application| application.enabled)
                .collect::<Vec<_>>();

            for application in enabled_applications {
                self.ensure_application_root_folder(&application, admin_id, tenant_id)
                    .await?;
            }
        }

        Ok(())
    }

    /// Enable an Application: mark enabled + ensure root folder exists.
    pub async fn enable_application(
        &self,
        key: &str,
        actor_id: UserId,
        tenant_id: Uuid,
    ) -> Result<ApplicationConfig, ApplicationError> {
        let application = self.get_application(key, tenant_id).await?;

        if application.enabled {
            sqlx::query(
                "UPDATE application_enablements SET enabled = true, updated_at = now()
                 WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
            )
            .bind(tenant_id)
            .bind(key)
            .execute(self.metadata_store.pool())
            .await?;
            return Ok(application);
        }

        // Ensure root folder exists
        self.ensure_application_root_folder(&application, actor_id, tenant_id)
            .await?;

        sqlx::query(
            "UPDATE application_enablements SET enabled = true, updated_at = now()
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(tenant_id)
        .bind(key)
        .execute(self.metadata_store.pool())
        .await?;

        self.get_application(key, tenant_id).await
    }

    /// Disable an Application: mark disabled. Does NOT delete files.
    pub async fn disable_application(
        &self,
        key: &str,
        _actor_id: UserId,
        tenant_id: Uuid,
    ) -> Result<ApplicationConfig, ApplicationError> {
        let application = self.get_application(key, tenant_id).await?;

        if !application.enabled {
            sqlx::query(
                "UPDATE application_enablements SET enabled = false, updated_at = now()
                 WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
            )
            .bind(tenant_id)
            .bind(key)
            .execute(self.metadata_store.pool())
            .await?;
            return Ok(application);
        }

        sqlx::query(
            "UPDATE application_enablements SET enabled = false, updated_at = now()
             WHERE tenant_id = $1 AND workspace_id = $1 AND application_id = $2",
        )
        .bind(tenant_id)
        .bind(key)
        .execute(self.metadata_store.pool())
        .await?;

        self.get_application(key, tenant_id).await
    }

    /// List all configured Applications for the admin shell.
    pub async fn list_applications(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ApplicationConfig>, ApplicationError> {
        let rows = sqlx::query(
            "SELECT application_id, enabled, configuration
             FROM application_enablements
             WHERE tenant_id = $1 AND workspace_id = $1",
        )
        .bind(tenant_id)
        .fetch_all(self.metadata_store.pool())
        .await?;

        let mut applications = Vec::new();
        for row in rows {
            let key: String = row.try_get("application_id")?;
            let manifest = self.manifest(&key)?;
            applications.push(self.application_config_from_manifest(
                manifest,
                row.try_get("enabled")?,
                row.try_get("configuration")?,
                tenant_id,
            ));
        }
        applications.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        Ok(applications)
    }

    /// List enabled Applications for the Shell.
    pub async fn list_enabled_applications(
        &self,
        tenant_id: Uuid,
        user_id: UserId,
    ) -> Result<Vec<ApplicationConfig>, ApplicationError> {
        let is_admin = self.is_admin_user(user_id, tenant_id).await?;
        Ok(self
            .list_applications(tenant_id)
            .await?
            .into_iter()
            .filter(|application| application.enabled)
            .filter(|application| user_can_access_application(application, is_admin))
            .collect())
    }

    /// Get a single Application by canonical ID.
    pub async fn get_application(
        &self,
        key: &str,
        tenant_id: Uuid,
    ) -> Result<ApplicationConfig, ApplicationError> {
        let row = sqlx::query(
            "SELECT enabled, configuration FROM application_enablements
             WHERE application_id = $1 AND tenant_id = $2 AND workspace_id = $2",
        )
        .bind(key)
        .bind(tenant_id)
        .fetch_optional(self.metadata_store.pool())
        .await?;

        let manifest = self.manifest(key)?;
        let row = row.ok_or_else(|| ApplicationError::NotFound(key.to_string()))?;
        let enabled = row.try_get("enabled")?;
        let configuration = row.try_get("configuration")?;
        Ok(self.application_config_from_manifest(manifest, enabled, configuration, tenant_id))
    }

    /// Update Application configuration (admin only). Only certain fields are mutable.
    pub async fn update_application(
        &self,
        key: &str,
        input: UpdateApplicationInput,
        tenant_id: Uuid,
    ) -> Result<ApplicationConfig, ApplicationError> {
        let application = self.get_application(key, tenant_id).await?;

        let display_name = input.display_name.unwrap_or(application.display_name);
        let description = input.description.unwrap_or(application.description);
        let icon = input.icon.unwrap_or(application.icon);
        validate_application_icon(&icon)?;
        let root_path = input.root_path.clone().unwrap_or(application.root_path);
        // Only enforce canonical path when explicitly changing root_path.
        // Existing roots remain readable while changed roots use the canonical layout.
        if input.root_path.is_some() {
            validate_root_path(&root_path)?;
        }
        let renderer = input.renderer.unwrap_or(application.renderer);
        let default_template = input
            .default_template
            .unwrap_or(application.default_template);
        let permissions = input.permissions.unwrap_or(application.permissions);
        let ai_indexing = input.ai_indexing.unwrap_or(application.ai_indexing);
        let audit = input.audit.unwrap_or(application.audit);

        // Validate UI config fields if provided
        if let Some(ref ui) = input.ui_config {
            if let Some(sidebar) = ui.get("sidebar").and_then(|v| v.as_object()) {
                if let Some(order) = sidebar.get("order").and_then(|v| v.as_i64()) {
                    if !(0..=1000).contains(&order) {
                        return Err(ApplicationError::InvalidData(
                            "Sidebar order must be between 0 and 1000".to_string(),
                        ));
                    }
                }
            }
            if let Some(dashboard) = ui.get("dashboard").and_then(|v| v.as_object()) {
                if let Some(order) = dashboard.get("order").and_then(|v| v.as_i64()) {
                    if !(0..=1000).contains(&order) {
                        return Err(ApplicationError::InvalidData(
                            "Dashboard order must be between 0 and 1000".to_string(),
                        ));
                    }
                }
                if let Some(max) = dashboard.get("maxItems").and_then(|v| v.as_i64()) {
                    if !(1..=50).contains(&max) {
                        return Err(ApplicationError::InvalidData(
                            "Dashboard maxItems must be between 1 and 50".to_string(),
                        ));
                    }
                }
            }
        }

        let ui_config = normalize_application_ui_config(
            key,
            &display_name,
            &description,
            &icon,
            &root_path,
            &renderer,
            default_template.as_deref(),
            Some(input.ui_config.unwrap_or(application.ui_config)),
        );

        let configuration = json!({
            "displayName": display_name,
            "description": description,
            "rootPath": root_path,
            "renderer": renderer,
            "defaultTemplate": default_template,
            "icon": icon,
            "permissions": permissions,
            "aiIndexing": ai_indexing,
            "audit": audit,
            "ui": ui_config
        });
        sqlx::query(
            "UPDATE application_enablements
             SET configuration = $1, updated_at = now()
             WHERE application_id = $2 AND tenant_id = $3 AND workspace_id = $3",
        )
        .bind(configuration)
        .bind(key)
        .bind(tenant_id)
        .execute(self.metadata_store.pool())
        .await?;

        self.get_application(key, tenant_id).await
    }

    /// Get a summary of Application contents for a declared route slug.
    pub async fn get_application_summary(
        &self,
        route_slug: &str,
        tenant_id: Uuid,
        user_id: UserId,
    ) -> Result<ApplicationSummary, ApplicationError> {
        let application_id = self.application_id_for_route_slug(route_slug)?;
        let application = self.get_application(&application_id, tenant_id).await?;
        let is_admin = self.is_admin_user(user_id, tenant_id).await?;
        if !application.enabled {
            return Err(ApplicationError::InvalidData(
                "ApplicationConfig disabled".to_string(),
            ));
        }
        if !user_can_access_application(&application, is_admin) {
            return Err(ApplicationError::PermissionDenied);
        }

        let ui_config = application.ui_config.as_object().ok_or_else(|| {
            ApplicationError::InvalidData("ui_config is not an object".to_string())
        })?;

        let dashboard = ui_config.get("dashboard").and_then(|v| v.as_object());
        let summary_mode = dashboard
            .and_then(|d| d.get("summaryMode"))
            .and_then(|v| v.as_str())
            .unwrap_or("generic-file-summary");
        let max_items = dashboard
            .and_then(|d| d.get("maxItems"))
            .and_then(|v| v.as_i64())
            .unwrap_or(4) as i64;
        let root_path = application.root_path.trim_end_matches('/').to_string();
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
        .await?;
        let file_count = row.try_get::<i64, _>("count")?;

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
        .await?;
        let folder_count = row.try_get::<i64, _>("count")?;

        let total_items = file_count + folder_count;

        let (mode, recent_items, extra) = self
            .build_summary_for_mode(
                &application_id,
                summary_mode,
                &root_path,
                &path_prefix,
                max_items,
                tenant_id,
                user_id,
            )
            .await?;

        Ok(ApplicationSummary {
            application_id,
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
    ) -> Result<rustshare_core::domain::Folder, ApplicationError> {
        let folders = self
            .metadata_store
            .list_folders(None, owner_id, tenant_id)
            .await
            .map_err(|e| ApplicationError::Database(e.to_string()))?;

        if let Some(ws) = folders.into_iter().find(|f| f.name == "Workspace") {
            return Ok(ws);
        }

        self.folder_service
            .create_folder_or_get("Workspace".into(), None, owner_id, tenant_id)
            .await
            .map_err(|e| ApplicationError::Storage(e.to_string()))
    }

    /// Ensure the Application root folder exists under /Workspace.
    async fn ensure_application_root_folder(
        &self,
        application: &ApplicationConfig,
        owner_id: UserId,
        tenant_id: Uuid,
    ) -> Result<(), ApplicationError> {
        let root_name = application
            .root_path
            .trim_start_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&application.root_path)
            .to_string();

        if root_name.is_empty() {
            return Err(ApplicationError::InvalidName(
                "Root path cannot be empty".to_string(),
            ));
        }

        let ws = self.ensure_workspace_folder(owner_id, tenant_id).await?;

        let folders = self
            .metadata_store
            .list_folders(Some(ws.id), owner_id, tenant_id)
            .await
            .map_err(|e| ApplicationError::Database(e.to_string()))?;

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
    ) -> Result<(String, Vec<SummaryItem>, Option<serde_json::Value>), ApplicationError> {
        match key {
            "io.elembra.notes" => {
                let items = self
                    .recent_files_under_path(path_prefix, max_items, tenant_id, user_id)
                    .await?;
                Ok(("recent-items".to_string(), items, None))
            }
            "io.elembra.meetings" => {
                let items = self
                    .recent_folders_under_path(path_prefix, max_items, tenant_id, user_id)
                    .await?;
                Ok(("recent-items".to_string(), items, None))
            }
            "io.elembra.standups" => {
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
            "io.elembra.kanban" => {
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
            "io.elembra.decisions" => {
                let items = self
                    .recent_files_under_path(path_prefix, max_items, tenant_id, user_id)
                    .await?;
                Ok(("recent-items".to_string(), items, None))
            }
            "io.elembra.shares" => {
                let items = self
                    .recent_folders_under_path(path_prefix, max_items, tenant_id, user_id)
                    .await?;
                let row = sqlx::query!(
                    "SELECT COUNT(*) as count FROM folders WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND (path LIKE '/Shares/Public/%' OR path LIKE '/Workspace/Shares/Public/%')",
                    tenant_id,
                    user_id
                )
                .fetch_one(self.metadata_store.pool())
                .await?;
                let public_count = row.count.unwrap_or(0);
                let row = sqlx::query!(
                    "SELECT COUNT(*) as count FROM folders WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL AND (path LIKE '/Shares/Internal/%' OR path LIKE '/Workspace/Shares/Internal/%')",
                    tenant_id,
                    user_id
                )
                .fetch_one(self.metadata_store.pool())
                .await?;
                let internal_count = row.count.unwrap_or(0);
                Ok((
                    "shares-overview".to_string(),
                    items,
                    Some(json!({ "publicCount": public_count, "internalCount": internal_count })),
                ))
            }
            "io.elembra.mail" => {
                let row = sqlx::query!(
                    "SELECT COUNT(*) as count FROM mail_messages WHERE tenant_id = $1 AND owner_id = $2 AND deleted_at IS NULL",
                    tenant_id,
                    user_id
                )
                .fetch_one(self.metadata_store.pool())
                .await?;
                let count = row.count.unwrap_or(0);
                Ok((
                    "mail-summary".to_string(),
                    vec![],
                    Some(json!({ "count": count })),
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
    ) -> Result<Vec<SummaryItem>, ApplicationError> {
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
    ) -> Result<Vec<SummaryItem>, ApplicationError> {
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
    ) -> Result<Vec<SummaryItem>, ApplicationError> {
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
    ) -> Result<Vec<SummaryItem>, ApplicationError> {
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
    ) -> Result<Vec<SummaryItem>, ApplicationError> {
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

    async fn is_admin_user(
        &self,
        user_id: UserId,
        tenant_id: Uuid,
    ) -> Result<bool, ApplicationError> {
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
    ) -> Result<Option<UserId>, ApplicationError> {
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

fn user_can_access_application(application: &ApplicationConfig, is_admin: bool) -> bool {
    if is_admin {
        return true;
    }

    application
        .permissions
        .get("workspace_members_can_use")
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
}

fn validate_application_icon(icon: &str) -> Result<(), ApplicationError> {
    if is_approved_icon_key(icon) {
        Ok(())
    } else {
        Err(ApplicationError::InvalidData(format!(
            "Unapproved Application icon: {icon}"
        )))
    }
}

fn validate_root_path(root_path: &str) -> Result<(), ApplicationError> {
    if !root_path.starts_with('/') || root_path.contains("..") || root_path.trim() == "/" {
        return Err(ApplicationError::InvalidName(format!(
            "Invalid root path: {root_path}"
        )));
    }

    // Enforce canonical /Workspace prefix for new/changed Application roots.
    // Legacy roots are read-only; new writes must be under /Workspace.
    if !root_path.starts_with("/Workspace/") {
        return Err(ApplicationError::InvalidName(format!(
            "Root path must be under /Workspace: {root_path}"
        )));
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn normalize_application_ui_config(
    application_id: &str,
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
        .unwrap_or_default();

    let widget_type = widget
        .get("type")
        .and_then(|value| value.as_str())
        .or_else(|| {
            dashboard
                .get("summaryMode")
                .and_then(|value| value.as_str())
        })
        .unwrap_or(default_widget_type(application_id));
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
        .unwrap_or(default_widget_size(application_id));
    let columns = widget
        .get("columns")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_else(|| default_widget_columns(widget_size));
    let widget_primary_action = widget
        .get("primaryAction")
        .cloned()
        .or_else(|| dashboard.get("primaryAction").cloned())
        .unwrap_or_else(|| default_primary_action(application_id, default_template));
    let dashboard_enabled = dashboard
        .get("enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(default_dashboard_enabled(application_id));
    let dashboard_order = dashboard
        .get("order")
        .and_then(|value| value.as_i64())
        .unwrap_or(default_dashboard_order(application_id));
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
        .unwrap_or_else(|| {
            format!(
                "/apps/{}",
                application_id.rsplit('.').next().unwrap_or(application_id)
            )
        });
    let page_renderer = page
        .get("renderer")
        .and_then(|value| value.as_str())
        .unwrap_or(renderer)
        .to_string();
    let page_layout = page
        .get("layout")
        .and_then(|value| value.as_str())
        .unwrap_or(default_page_layout(application_id))
        .to_string();
    let page_empty_title = page
        .get("emptyStateTitle")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| default_empty_state_title(application_id, display_name));
    let page_empty_description = page
        .get("emptyStateDescription")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| default_empty_state_description(application_id, description));
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
            "order": sidebar.get("order").and_then(|value| value.as_i64()).unwrap_or(default_sidebar_order(application_id)),
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

fn title_case(value: &str) -> String {
    value
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn default_widget_type(application_id: &str) -> &str {
    match application_id {
        "io.elembra.kanban" => "kanban-summary",
        "io.elembra.meetings" => "decisions-meetings-summary",
        "io.elembra.notes" => "latest-notes",
        "io.elembra.shares" => "active-shares",
        _ => "generic-application-summary",
    }
}

fn default_widget_size(application_id: &str) -> &str {
    match application_id {
        "io.elembra.kanban" => "large",
        "io.elembra.meetings" => "medium",
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

fn default_primary_action(
    application_id: &str,
    default_template: Option<&str>,
) -> serde_json::Value {
    let label = match application_id {
        "io.elembra.kanban" => "New board",
        "io.elembra.brainstorming" => "New idea board",
        "io.elembra.meetings" => "New meeting note",
        "io.elembra.standups" => "New standup",
        "io.elembra.decisions" => "New decision",
        "io.elembra.shares" => "New share",
        "io.elembra.mail" => "Import mail",
        _ => "New note",
    };

    json!({
        "label": label,
        "action": if matches!(application_id, "io.elembra.shares" | "io.elembra.mail") { "generic-create" } else { "create-from-template" },
        "template": default_template
    })
}

fn default_dashboard_enabled(application_id: &str) -> bool {
    !matches!(
        application_id,
        "io.elembra.decisions" | "io.elembra.standups"
    )
}

fn default_dashboard_order(application_id: &str) -> i64 {
    match application_id {
        "io.elembra.kanban" => 10,
        "io.elembra.meetings" => 20,
        "io.elembra.notes" => 30,
        "io.elembra.shares" => 40,
        "io.elembra.standups" => 50,
        "io.elembra.decisions" => 60,
        _ => 99,
    }
}

fn default_sidebar_order(application_id: &str) -> i64 {
    match application_id {
        "io.elembra.notes" => 30,
        "io.elembra.meetings" => 40,
        "io.elembra.standups" => 50,
        "io.elembra.kanban" => 60,
        "io.elembra.decisions" => 70,
        "io.elembra.shares" => 80,
        _ => 99,
    }
}

fn default_page_layout(application_id: &str) -> &str {
    match application_id {
        "io.elembra.kanban" => "kanban-board",
        _ => "list-grid",
    }
}

fn default_empty_state_title(application_id: &str, display_name: &str) -> String {
    match application_id {
        "io.elembra.notes" => "No notes yet".to_string(),
        "io.elembra.meetings" => "No meeting notes yet".to_string(),
        "io.elembra.standups" => "No standups yet".to_string(),
        "io.elembra.kanban" => "No boards yet".to_string(),
        "io.elembra.decisions" => "No decisions yet".to_string(),
        "io.elembra.shares" => "No active shares".to_string(),
        _ => format!("No {} yet", display_name.to_lowercase()),
    }
}

fn default_empty_state_description(application_id: &str, description: &str) -> String {
    match application_id {
        "io.elembra.notes" => "Create your first file-backed note.".to_string(),
        "io.elembra.meetings" => "Create your first meeting note.".to_string(),
        "io.elembra.standups" => "Create your first standup record.".to_string(),
        "io.elembra.kanban" => "No boards yet. Create your first file-backed board.".to_string(),
        "io.elembra.decisions" => "No decisions recorded yet.".to_string(),
        "io.elembra.shares" => "No active shares.".to_string(),
        _ => description.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_application_ui_config_contract() {
        let ui = normalize_application_ui_config(
            "io.elembra.notes",
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
                "page": {"enabled": true, "route": "/apps/notes", "renderer": "okf-note", "layout": "list-grid", "emptyStateTitle": "No notes yet", "emptyStateDescription": "Create your first OKF note.", "emptyStateAction": "New note", "primaryAction": {"label": "New note", "action": "create-from-template", "template": "template_default_okf_note"}, "searchPlaceholder": "Search notes...", "filterLabel": "All notes", "sortLabel": "Modified", "itemSingular": "note", "itemPlural": "notes"}
            })),
        );

        let ui_obj = ui.as_object().unwrap();

        // Canonical page key must exist
        assert!(ui_obj.contains_key("page"), "canonical 'page' key missing");
        // OKF metadata must be preserved
        assert!(ui_obj.contains_key("okf"), "okf config missing");
        assert_eq!(
            ui_obj.get("documentFormat").unwrap().as_str().unwrap(),
            "okf-markdown"
        );

        let page = ui_obj.get("page").unwrap().as_object().unwrap();
        assert_eq!(page.get("route").unwrap().as_str().unwrap(), "/apps/notes");
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
    fn test_normalize_application_ui_config_defaults_match_contract() {
        let ui = normalize_application_ui_config(
            "io.elembra.kanban",
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
        assert_eq!(page.get("route").unwrap().as_str().unwrap(), "/apps/kanban");
    }

    #[test]
    fn test_application_error_display() {
        let err = ApplicationError::NotFound("notes".to_string());
        assert_eq!(err.to_string(), "Application not found: notes");
    }

    #[test]
    fn test_application_error_display_already_exists() {
        let err = ApplicationError::AlreadyExists("meetings".to_string());
        assert_eq!(err.to_string(), "Application already exists: meetings");
    }

    #[test]
    fn test_application_error_display_permission_denied() {
        let err = ApplicationError::PermissionDenied;
        assert_eq!(err.to_string(), "Permission denied");
    }

    #[test]
    fn test_application_error_display_database() {
        let err = ApplicationError::Database("connection failed".to_string());
        assert_eq!(err.to_string(), "Database error: connection failed");
    }

    #[test]
    fn test_update_application_input_debug() {
        let input = UpdateApplicationInput {
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
    fn test_application_summary_serialize() {
        let summary = ApplicationSummary {
            application_id: "io.elembra.notes".to_string(),
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
    fn accepts_approved_application_icons() {
        assert!(validate_application_icon("sticky-note").is_ok());
        assert!(validate_application_icon("calendar-days").is_ok());
    }

    #[test]
    fn rejects_unapproved_application_icons() {
        assert!(validate_application_icon("users").is_err());
        assert!(validate_application_icon("invalid-random-icon").is_err());
        assert!(validate_application_icon("script<alert>1</alert>").is_err());
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
        // Existing roots are read-only; new/changed Application roots must be canonical.
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
