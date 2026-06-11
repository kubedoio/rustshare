use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    handlers::{AdminUser, AppError},
    AppState,
};

use super::log_admin_action;

#[derive(sqlx::FromRow, Serialize, utoipa::ToSchema)]
pub struct WorkflowResponse {
    pub id: String,
    pub key: String,
    pub name: String,
    pub trigger_type: String,
    pub status: String,
    pub subject: Option<String>,
    pub body: Option<String>,
    pub terms_enabled: bool,
    pub terms_text: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub updated_by: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateWorkflowRequest {
    pub subject: Option<String>,
    pub body: Option<String>,
    pub terms_enabled: Option<bool>,
    pub terms_text: Option<String>,
    pub status: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/workflows",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn list_workflows(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
) -> Result<Json<Vec<WorkflowResponse>>, AppError> {
    let rows = sqlx::query_as::<_, WorkflowRow>(
        "SELECT id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                created_at, updated_at, updated_by
         FROM workflows
         ORDER BY created_at ASC",
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;

    Ok(Json(rows.into_iter().map(WorkflowResponse::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/workflows/{id}",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn get_workflow(
    State(state): State<AppState>,
    AdminUser { .. }: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowResponse>, AppError> {
    let row = sqlx::query_as::<_, WorkflowRow>(
        "SELECT id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                created_at, updated_at, updated_by
         FROM workflows
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("Workflow not found"))?;

    Ok(Json(WorkflowResponse::from(row)))
}

#[utoipa::path(
    put,
    path = "/api/v1/admin/workflows/{id}",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn update_workflow(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateWorkflowRequest>,
) -> Result<Json<WorkflowResponse>, AppError> {
    let row = sqlx::query_as::<_, WorkflowRow>(
        "UPDATE workflows
         SET subject = COALESCE($2, subject),
             body = COALESCE($3, body),
             terms_enabled = COALESCE($4, terms_enabled),
             terms_text = COALESCE($5, terms_text),
             status = COALESCE($6, status),
             updated_by = $7,
             updated_at = NOW()
         WHERE id = $1
         RETURNING id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                   created_at, updated_at, updated_by",
    )
    .bind(id)
    .bind(req.subject)
    .bind(req.body)
    .bind(req.terms_enabled)
    .bind(req.terms_text)
    .bind(req.status)
    .bind(actor_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("Workflow not found"))?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "workflow.updated",
        Some("workflow"),
        Some(id),
        json!({}),
    )
    .await;

    Ok(Json(WorkflowResponse::from(row)))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/workflows/{id}/enable",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn enable_workflow(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowResponse>, AppError> {
    let wf = sqlx::query_as::<_, WorkflowRow>(
        "SELECT id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                created_at, updated_at, updated_by
         FROM workflows
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("Workflow not found"))?;

    if wf.key != "invite_email" {
        return Err(AppError::bad_request(
            "Only invite_email workflow can be enabled currently",
        ));
    }

    let smtp_ok: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM smtp_config
            WHERE id = '00000000-0000-0000-0000-000000000002'
              AND enabled = true
              AND host IS NOT NULL
              AND port IS NOT NULL
              AND from_address IS NOT NULL
        )",
    )
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;

    if !smtp_ok {
        return Err(AppError::bad_request(
            "SMTP must be configured and enabled before this workflow can be activated",
        ));
    }

    let row = sqlx::query_as::<_, WorkflowRow>(
        "UPDATE workflows
         SET status = 'active', updated_by = $2, updated_at = NOW()
         WHERE id = $1
         RETURNING id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                   created_at, updated_at, updated_by",
    )
    .bind(id)
    .bind(actor_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "workflow.enabled",
        Some("workflow"),
        Some(id),
        json!({}),
    )
    .await;

    Ok(Json(WorkflowResponse::from(row)))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/workflows/{id}/disable",
    tag = "Admin",
    responses(
        (status = 200, description = "Success"),
        (status = 401, description = "Unauthorized", body = crate::handlers::ErrorResponse),
    ),
)]
pub async fn disable_workflow(
    State(state): State<AppState>,
    AdminUser { user_id: actor_id }: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkflowResponse>, AppError> {
    let row = sqlx::query_as::<_, WorkflowRow>(
        "UPDATE workflows
         SET status = 'draft', updated_by = $2, updated_at = NOW()
         WHERE id = $1
         RETURNING id, key, name, trigger_type, status, subject, body, terms_enabled, terms_text,
                   created_at, updated_at, updated_by",
    )
    .bind(id)
    .bind(actor_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| AppError::internal(e.to_string()))?
    .ok_or_else(|| AppError::not_found("Workflow not found"))?;

    log_admin_action(
        &state.db_pool,
        actor_id,
        "workflow.disabled",
        Some("workflow"),
        Some(id),
        json!({}),
    )
    .await;

    Ok(Json(WorkflowResponse::from(row)))
}

#[derive(sqlx::FromRow)]
struct WorkflowRow {
    id: Uuid,
    key: String,
    name: String,
    trigger_type: String,
    status: String,
    subject: Option<String>,
    body: Option<String>,
    terms_enabled: bool,
    terms_text: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    updated_by: Option<Uuid>,
}

impl From<WorkflowRow> for WorkflowResponse {
    fn from(row: WorkflowRow) -> Self {
        WorkflowResponse {
            id: row.id.to_string(),
            key: row.key,
            name: row.name,
            trigger_type: row.trigger_type,
            status: row.status,
            subject: row.subject,
            body: row.body,
            terms_enabled: row.terms_enabled,
            terms_text: row.terms_text,
            created_at: row.created_at,
            updated_at: row.updated_at,
            updated_by: row.updated_by.map(|u| u.to_string()),
        }
    }
}
