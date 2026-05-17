use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    handlers::{AuthenticatedUser, AppError},
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

pub async fn get_file_replication_status(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Path(file_id): Path<Uuid>,
) -> Result<Json<FileReplicationStatusResponse>, AppError> {
    let file = state
        .file_service
        .get_file(file_id, auth.user_id)
        .await?;

    let row = sqlx::query!(
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
        file.id,
        file.current_version
    )
    .fetch_optional(&state.db_pool)
    .await?;

    let (
        replication_state,
        job_status,
        attempt_count,
        next_attempt_at,
        last_attempt_at,
        last_error,
    ) = if let Some(row) = row {
        (
            row.replication_state,
            Some(row.job_status),
            Some(row.attempt_count),
            Some(row.next_attempt_at),
            row.last_attempt_at,
            row.last_error,
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

pub async fn get_replication_summary(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<ReplicationSummaryResponse>, AppError> {
    require_admin(&state, auth.user_id).await?;

    let version_counts_row = sqlx::query!(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE replication_state = 'primary_written') AS primary_written,
            COUNT(*) FILTER (WHERE replication_state = 'queued') AS queued,
            COUNT(*) FILTER (WHERE replication_state = 'syncing') AS syncing,
            COUNT(*) FILTER (WHERE replication_state = 'fully_replicated') AS fully_replicated,
            COUNT(*) FILTER (WHERE replication_state = 'degraded') AS degraded,
            COUNT(*) FILTER (WHERE replication_state = 'failed') AS failed
        FROM file_versions
        "#
    )
    .fetch_one(&state.db_pool)
    .await?;

    let job_counts_row = sqlx::query!(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status = 'queued') AS queued,
            COUNT(*) FILTER (WHERE status = 'syncing') AS syncing,
            COUNT(*) FILTER (WHERE status = 'retrying') AS retrying,
            COUNT(*) FILTER (WHERE status = 'completed') AS completed,
            COUNT(*) FILTER (WHERE status = 'failed') AS failed,
            CAST(EXTRACT(EPOCH FROM (NOW() - (MIN(created_at) FILTER (WHERE status IN ('queued', 'retrying', 'syncing'))))) AS BIGINT) AS oldest_pending_job_age_seconds,
            CAST(EXTRACT(EPOCH FROM (NOW() - (MIN(updated_at) FILTER (WHERE status = 'failed')))) AS BIGINT) AS oldest_failed_job_age_seconds
        FROM replication_jobs
        "#
    )
    .fetch_one(&state.db_pool)
    .await?;

    let target_counts_row = sqlx::query!(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE enabled = TRUE) AS enabled,
            COUNT(*) FILTER (WHERE enabled = TRUE AND is_required = TRUE) AS required,
            COUNT(*) FILTER (WHERE enabled = TRUE AND health_status = 'healthy') AS healthy,
            COUNT(*) FILTER (WHERE enabled = TRUE AND health_status = 'degraded') AS degraded,
            COUNT(*) FILTER (WHERE enabled = TRUE AND health_status = 'failed') AS failed
        FROM replication_targets
        "#
    )
    .fetch_one(&state.db_pool)
    .await?;

    Ok(Json(ReplicationSummaryResponse {
        generated_at: to_rfc3339(Utc::now()),
        version_states: ReplicationVersionStateCounts {
            primary_written: version_counts_row.primary_written.unwrap_or(0),
            queued: version_counts_row.queued.unwrap_or(0),
            syncing: version_counts_row.syncing.unwrap_or(0),
            fully_replicated: version_counts_row.fully_replicated.unwrap_or(0),
            degraded: version_counts_row.degraded.unwrap_or(0),
            failed: version_counts_row.failed.unwrap_or(0),
        },
        job_states: ReplicationJobStateCounts {
            queued: job_counts_row.queued.unwrap_or(0),
            syncing: job_counts_row.syncing.unwrap_or(0),
            retrying: job_counts_row.retrying.unwrap_or(0),
            completed: job_counts_row.completed.unwrap_or(0),
            failed: job_counts_row.failed.unwrap_or(0),
        },
        target_states: ReplicationTargetStateCounts {
            enabled: target_counts_row.enabled.unwrap_or(0),
            required: target_counts_row.required.unwrap_or(0),
            healthy: target_counts_row.healthy.unwrap_or(0),
            degraded: target_counts_row.degraded.unwrap_or(0),
            failed: target_counts_row.failed.unwrap_or(0),
        },
        oldest_pending_job_age_seconds: job_counts_row.oldest_pending_job_age_seconds,
        oldest_failed_job_age_seconds: job_counts_row.oldest_failed_job_age_seconds,
    }))
}

pub async fn list_replication_jobs(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
    Query(query): Query<AdminJobsQuery>,
) -> Result<Json<Vec<ReplicationJobResponse>>, AppError> {
    require_admin(&state, auth.user_id).await?;

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let rows = sqlx::query!(
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
        limit
    )
    .fetch_all(&state.db_pool)
    .await?;

    let jobs = rows
        .into_iter()
        .map(|row| {
            let last_attempt_at = row.last_attempt_at;

            Ok(ReplicationJobResponse {
                job_id: row.id,
                file_id: row.file_id,
                file_version_id: row.file_version_id,
                storage_key: row.storage_key,
                status: row.status,
                attempt_count: row.attempt_count,
                next_attempt_at: to_rfc3339(row.next_attempt_at),
                last_attempt_at: last_attempt_at.map(to_rfc3339),
                last_error: row.last_error,
                created_at: to_rfc3339(row.created_at),
                updated_at: to_rfc3339(row.updated_at),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        ?;

    Ok(Json(jobs))
}

pub async fn list_replication_targets(
    State(state): State<AppState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<ReplicationTargetHealthResponse>>, AppError> {
    require_admin(&state, auth.user_id).await?;

    let rows = sqlx::query!(
        r#"
        SELECT
            id,
            name,
            destination_type,
            endpoint,
            bucket,
            region,
            base_path,
            is_required,
            enabled,
            health_status,
            last_healthy_at,
            last_error,
            updated_at
        FROM replication_targets
        ORDER BY is_required DESC, name ASC
        "#
    )
    .fetch_all(&state.db_pool)
    .await?;

    let targets = rows
        .into_iter()
        .map(|row| {
            let last_healthy_at = row.last_healthy_at;

            Ok(ReplicationTargetHealthResponse {
                id: row.id,
                name: row.name,
                destination_type: row.destination_type,
                endpoint: row.endpoint,
                bucket: row.bucket,
                region: row.region,
                base_path: row.base_path,
                is_required: row.is_required,
                enabled: row.enabled,
                health_status: row.health_status,
                last_healthy_at: last_healthy_at.map(to_rfc3339),
                last_error: row.last_error,
                updated_at: to_rfc3339(row.updated_at),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        ?;

    Ok(Json(targets))
}

fn to_rfc3339(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

async fn require_admin(state: &AppState, user_id: Uuid) -> Result<(), AppError> {
    let user = state
        .metadata_store
        .find_user_by_id(user_id)
        .await?
        .ok_or_else(|| AppError::internal("authenticated user not found"))?;

    if !user.is_admin {
        return Err(AppError::forbidden("Admin access required"));
    }

    Ok(())
}
