use axum::Json;
use serde::Serialize;

use crate::handlers::extractors::AuthenticatedUser;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSurfaceLayout {
    pub r#type: String,
    pub columns: i32,
    pub gap: i32,
    pub compact_overview: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WorkspaceSurfaceSection {
    pub key: String,
    pub r#type: String,
    pub enabled: bool,
    pub order: i32,
    pub title: Option<String>,
    pub renderer: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WorkspaceSurfaceDefinition {
    pub id: String,
    pub key: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub layout: WorkspaceSurfaceLayout,
    pub sections: Vec<WorkspaceSurfaceSection>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WorkspaceSurfaceResponse {
    pub surface: WorkspaceSurfaceDefinition,
}

#[utoipa::path(
    get,
    path = "/api/v1/workspace-surface",
    tag = "Modules",
    responses(
        (status = 200, description = "Success", body = WorkspaceSurfaceResponse),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_workspace_surface(_user: AuthenticatedUser) -> Json<WorkspaceSurfaceResponse> {
    Json(WorkspaceSurfaceResponse {
        surface: WorkspaceSurfaceDefinition {
            id: "workspace_dashboard_default".to_string(),
            key: "default-workspace-dashboard".to_string(),
            name: "Default Workspace Dashboard".to_string(),
            version: "1.0".to_string(),
            enabled: true,
            layout: WorkspaceSurfaceLayout {
                r#type: "responsive-grid".to_string(),
                columns: 12,
                gap: 24,
                compact_overview: true,
            },
            sections: vec![
                WorkspaceSurfaceSection {
                    key: "workspace-overview".to_string(),
                    r#type: "workspace-summary".to_string(),
                    enabled: true,
                    order: 10,
                    title: None,
                    renderer: "compact-workspace-overview".to_string(),
                },
                WorkspaceSurfaceSection {
                    key: "summary-insights".to_string(),
                    r#type: "dashboard-widgets".to_string(),
                    enabled: true,
                    order: 20,
                    title: Some("Workspace Summary & Insights".to_string()),
                    renderer: "workspace-widget-grid".to_string(),
                },
            ],
        },
    })
}
