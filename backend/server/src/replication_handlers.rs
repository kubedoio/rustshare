use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    handlers::{file_error_response, AuthenticatedUser, ErrorResponse},
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

#[derive(Debug, Deserialize)]
pub struct AdminJobsQuery {
    pub limit: Option<i64>,
}

pub async fn get_file_replication_status(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Json<FileReplicationStatusResponse>, Response> {
    let file = state
        .file_service
        .get_file(file_id, auth.user_id)
        .await
        .map_err(file_error_response)?;

    let row = sqlx::query(
        r#"
        SELECT
            fv.replication_state,
            rj.status AS job_status,
            rj.attempt_count,
            rj.next_attempt_at,
            rj.last_attempt_at,
            rj.last_error
        FROM file_versions fv
        LEFT JOIN replication_jobs rj
            ON rj.file_version_id = fv.id
        WHERE fv.file_id = $1
          AND fv.version_number = $2
        ORDER BY rj.updated_at DESC NULLS LAST
        LIMIT 1
        "#,
    )
    .bind(file.id)
    .bind(file.current_version)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(internal_error)?;

    let (
        replication_state,
        job_status,
        attempt_count,
        next_attempt_at,
        last_attempt_at,
        last_error,
    ) = if let Some(row) = row {
        (
            row.try_get("replication_state").map_err(internal_error)?,
            row.try_get("job_status").map_err(internal_error)?,
            row.try_get("attempt_count").map_err(internal_error)?,
            row.try_get("next_attempt_at").map_err(internal_error)?,
            row.try_get("last_attempt_at").map_err(internal_error)?,
            row.try_get("last_error").map_err(internal_error)?,
        )
    } else {
        ("primary_written".to_string(), None, None, None, None, None)
    };

    Ok(Json(FileReplicationStatusResponse {
        file_id: file.id,
        file_name: file.name,
        current_version: file.current_version,
        replication_state,
        job_status,
        attempt_count,
        next_attempt_at: next_attempt_at.map(to_rfc3339),
        last_attempt_at: last_attempt_at.map(to_rfc3339),
        last_error,
    }))
}

pub async fn list_replication_jobs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<AdminJobsQuery>,
) -> Result<Json<Vec<ReplicationJobResponse>>, Response> {
    let user = state
        .metadata_store
        .find_user_by_id(auth.user_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| internal_error(anyhow::anyhow!("authenticated user not found")))?;

    if !user.is_admin {
        return Err(forbidden_error("Admin access required"));
    }

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let rows = sqlx::query(
        r#"
        SELECT
            id,
            file_id,
            file_version_id,
            storage_key,
            status,
            attempt_count,
            next_attempt_at,
            last_attempt_at,
            last_error,
            created_at,
            updated_at
        FROM replication_jobs
        ORDER BY updated_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(&state.db_pool)
    .await
    .map_err(internal_error)?;

    let jobs = rows
        .into_iter()
        .map(|row| {
            let last_attempt_at: Option<DateTime<Utc>> = row.try_get("last_attempt_at")?;

            Ok(ReplicationJobResponse {
                job_id: row.try_get("id")?,
                file_id: row.try_get("file_id")?,
                file_version_id: row.try_get("file_version_id")?,
                storage_key: row.try_get("storage_key")?,
                status: row.try_get("status")?,
                attempt_count: row.try_get("attempt_count")?,
                next_attempt_at: to_rfc3339(row.try_get("next_attempt_at")?),
                last_attempt_at: last_attempt_at.map(to_rfc3339),
                last_error: row.try_get("last_error")?,
                created_at: to_rfc3339(row.try_get("created_at")?),
                updated_at: to_rfc3339(row.try_get("updated_at")?),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(internal_error)?;

    Ok(Json(jobs))
}

fn to_rfc3339(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn internal_error(error: impl std::fmt::Display) -> Response {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new(format!(
            "Internal server error: {error}"
        ))),
    )
        .into_response()
}

fn forbidden_error(message: &str) -> Response {
    (
        axum::http::StatusCode::FORBIDDEN,
        Json(ErrorResponse::new(message)),
    )
        .into_response()
}
