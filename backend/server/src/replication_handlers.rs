//! Replication status handlers for admin visibility endpoints.
//!
//! TODO: This module needs to be rewritten to use the new JobRepository
//! for replication job tracking instead of PostgreSQL.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    handlers::{AuthenticatedUser, ErrorResponse},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct FileReplicationStatusResponse {
    pub file_id: Uuid,
    pub file_name: String,
    pub current_version: i32,
    pub replication_state: String,
    pub job_status: Option<String>,
    pub attempt_count: Option<i32>,
    pub next_attempt_at: Option<String>,
    pub last_attempt_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReplicationJobResponse {
    pub job_id: Uuid,
    pub file_id: Uuid,
    pub file_version_id: Uuid,
    pub storage_key: String,
    pub status: String,
    pub attempt_count: i32,
    pub next_attempt_at: String,
    pub last_attempt_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct ReplicationSummaryResponse {
    pub generated_at: String,
    pub version_states: ReplicationVersionStateCounts,
    pub job_states: ReplicationJobStateCounts,
    pub target_states: ReplicationTargetStateCounts,
    pub oldest_pending_job_age_seconds: Option<i64>,
    pub oldest_failed_job_age_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ReplicationVersionStateCounts {
    pub primary_written: i64,
    pub queued: i64,
    pub syncing: i64,
    pub fully_replicated: i64,
    pub degraded: i64,
    pub failed: i64,
}

#[derive(Debug, Serialize)]
pub struct ReplicationJobStateCounts {
    pub queued: i64,
    pub syncing: i64,
    pub retrying: i64,
    pub completed: i64,
    pub failed: i64,
}

#[derive(Debug, Serialize)]
pub struct ReplicationTargetStateCounts {
    pub enabled: i64,
    pub required: i64,
    pub healthy: i64,
    pub degraded: i64,
    pub failed: i64,
}

#[derive(Debug, Serialize)]
pub struct ReplicationTargetHealthResponse {
    pub id: Uuid,
    pub name: String,
    pub destination_type: String,
    pub endpoint: String,
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub base_path: Option<String>,
    pub is_required: bool,
    pub enabled: bool,
    pub health_status: String,
    pub last_healthy_at: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct AdminJobsQuery {
    pub limit: Option<i64>,
}

/// GET /api/v1/files/{id}/replication
/// Get replication status for a specific file
pub async fn get_file_replication_status(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Json<FileReplicationStatusResponse>, Response> {
    let file = state
        .file_service
        .get_file(file_id, auth.user_id)
        .await
        .map_err(|_error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to get file")),
            )
                .into_response()
        })?;

    // TODO: Use JobRepository to get replication status
    tracing::warn!("File replication status not yet implemented in zero-PostgreSQL mode");

    // Return placeholder response
    Ok(Json(FileReplicationStatusResponse {
        file_id: file.id,
        file_name: file.name,
        current_version: file.current_version,
        replication_state: "primary_written".to_string(),
        job_status: None,
        attempt_count: None,
        next_attempt_at: None,
        last_attempt_at: None,
        last_error: None,
    }))
}

/// GET /api/v1/admin/replication/summary
/// Get summary statistics for replication
pub async fn get_replication_summary(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<ReplicationSummaryResponse>, Response> {
    require_admin(&state, auth.user_id).await?;

    // TODO: Use JobRepository to get replication statistics
    tracing::warn!("Replication summary not yet implemented in zero-PostgreSQL mode");

    // Return placeholder response with zeros
    Ok(Json(ReplicationSummaryResponse {
        generated_at: to_rfc3339(Utc::now()),
        version_states: ReplicationVersionStateCounts {
            primary_written: 0,
            queued: 0,
            syncing: 0,
            fully_replicated: 0,
            degraded: 0,
            failed: 0,
        },
        job_states: ReplicationJobStateCounts {
            queued: 0,
            syncing: 0,
            retrying: 0,
            completed: 0,
            failed: 0,
        },
        target_states: ReplicationTargetStateCounts {
            enabled: 0,
            required: 0,
            healthy: 0,
            degraded: 0,
            failed: 0,
        },
        oldest_pending_job_age_seconds: None,
        oldest_failed_job_age_seconds: None,
    }))
}

/// GET /api/v1/admin/replication/jobs
/// List replication jobs
pub async fn list_replication_jobs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(_query): Query<AdminJobsQuery>,
) -> Result<Json<Vec<ReplicationJobResponse>>, Response> {
    require_admin(&state, auth.user_id).await?;

    // TODO: Use JobRepository to list replication jobs
    tracing::warn!("Replication job list not yet implemented in zero-PostgreSQL mode");

    // Return empty list for now
    Ok(Json(vec![]))
}

/// GET /api/v1/admin/replication/targets
/// List replication targets
pub async fn list_replication_targets(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<ReplicationTargetHealthResponse>>, Response> {
    require_admin(&state, auth.user_id).await?;

    // TODO: Use CoordinationStore or new TargetRepository
    tracing::warn!("Replication target list not yet implemented in zero-PostgreSQL mode");

    // Return empty list for now
    Ok(Json(vec![]))
}

fn to_rfc3339(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

async fn require_admin(state: &AppState, user_id: Uuid) -> Result<(), Response> {
    let user = state
        .metadata_store
        .find_user_by_id(user_id)
        .await
        .map_err(|error| *internal_error(error))?
        .ok_or_else(|| *internal_error(anyhow::anyhow!("authenticated user not found")))?;

    if !user.is_admin {
        return Err(forbidden_error("Admin access required"));
    }

    Ok(())
}

fn internal_error(error: impl std::fmt::Display) -> Box<Response> {
    Box::new(
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Internal server error: {error}"
            ))),
        )
            .into_response(),
    )
}

fn forbidden_error(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new(message)),
    )
        .into_response()
}
