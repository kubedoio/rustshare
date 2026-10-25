//! Durable Files-backed Notes reconciliation. Each claim processes one
//! source page so due tenants share capacity fairly.

use std::{sync::Arc, time::Duration};

use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row;
use tokio::{sync::broadcast, task::JoinSet};
use uuid::Uuid;

use crate::{
    handlers::{
        memory_reconcile::{reconcile_files_memory_page, FilesReconcileResponse},
        AdminUser, AuthenticatedUser, ErrorResponse,
    },
    services::note_service::NoteService,
    state::AppState,
};
use rustshare_storage::MetadataStore;

const MAX_RETRIES: i32 = 8;

#[derive(Clone, Copy)]
pub struct MemoryFilesReconcileConfig {
    pub interval: Duration,
    pub poll_interval: Duration,
    pub lease: Duration,
    pub max_concurrent: usize,
}

impl MemoryFilesReconcileConfig {
    pub fn from_config(config: &crate::config::AppConfig) -> anyhow::Result<Self> {
        anyhow::ensure!(
            (60..=86_400).contains(&config.memory_files_reconcile_interval_secs),
            "memory reconcile interval must be between 60 and 86400 seconds"
        );
        anyhow::ensure!(
            (1..=60).contains(&config.memory_files_reconcile_poll_secs),
            "memory reconcile poll must be between 1 and 60 seconds"
        );
        anyhow::ensure!(
            (30..=3_600).contains(&config.memory_files_reconcile_lease_secs),
            "memory reconcile lease must be between 30 and 3600 seconds"
        );
        anyhow::ensure!(
            (1..=4).contains(&config.memory_files_reconcile_max_concurrent),
            "memory reconcile concurrency must be between 1 and 4"
        );
        Ok(Self {
            interval: Duration::from_secs(config.memory_files_reconcile_interval_secs),
            poll_interval: Duration::from_secs(config.memory_files_reconcile_poll_secs),
            lease: Duration::from_secs(config.memory_files_reconcile_lease_secs),
            max_concurrent: config.memory_files_reconcile_max_concurrent,
        })
    }
}

pub fn spawn_memory_files_reconcile_worker(
    metadata: Arc<MetadataStore>,
    note_service: Arc<NoteService>,
    config: MemoryFilesReconcileConfig,
    mut shutdown: broadcast::Receiver<()>,
) {
    tokio::spawn(async move {
        let worker_id = Uuid::new_v4();
        let mut tasks = JoinSet::new();
        loop {
            while tasks.len() < config.max_concurrent {
                match claim(&metadata, worker_id, config.lease).await {
                    Ok(Some(claim)) => {
                        let metadata = Arc::clone(&metadata);
                        let note_service = Arc::clone(&note_service);
                        tasks.spawn(async move {
                            run_page(metadata, note_service, config, worker_id, claim).await;
                        });
                    }
                    Ok(None) => break,
                    Err(error) => {
                        tracing::error!(error = %error, "Memory reconciliation claim failed");
                        break;
                    }
                }
            }
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(config.poll_interval) => {},
                _ = tasks.join_next(), if !tasks.is_empty() => {},
            }
        }
        while !tasks.is_empty() {
            if tokio::time::timeout(Duration::from_secs(30), tasks.join_next())
                .await
                .is_err()
            {
                tasks.abort_all();
                break;
            }
        }
    });
}

#[doc(hidden)]
pub struct MemoryFilesLease {
    pub tenant_id: Uuid,
    pub generation: i64,
    pub cursor: Option<Uuid>,
}

#[doc(hidden)]
pub async fn claim(
    metadata: &MetadataStore,
    owner: Uuid,
    lease: Duration,
) -> anyhow::Result<Option<MemoryFilesLease>> {
    let mut tx = metadata.pool().begin().await?;
    // File rows are the authoritative tenant inventory. Hard-deleted rows are
    // absent by design; this sweep does not claim orphan-index cleanup.
    sqlx::query("INSERT INTO memory_files_reconcile_jobs (tenant_id) SELECT DISTINCT tenant_id FROM files ON CONFLICT (tenant_id) DO NOTHING")
        .execute(&mut *tx).await?;
    let row = sqlx::query(
        "UPDATE memory_files_reconcile_jobs SET status='running', lease_owner=$1, lease_until=NOW()+($2 * INTERVAL '1 second'), heartbeat_at=NOW(), lease_generation=lease_generation+1, last_claimed_at=NOW(), updated_at=NOW() WHERE tenant_id=(SELECT tenant_id FROM memory_files_reconcile_jobs WHERE status IN ('idle','running') AND next_run_at<=NOW() AND (lease_until IS NULL OR lease_until<NOW()) ORDER BY next_run_at, last_claimed_at NULLS FIRST, tenant_id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING tenant_id, lease_generation, cursor_file_id",
    ).bind(owner).bind(lease.as_secs() as i64).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    row.map(|row| {
        Ok(MemoryFilesLease {
            tenant_id: row.try_get("tenant_id")?,
            generation: row.try_get("lease_generation")?,
            cursor: row.try_get("cursor_file_id")?,
        })
    })
    .transpose()
}

async fn run_page(
    metadata: Arc<MetadataStore>,
    note_service: Arc<NoteService>,
    config: MemoryFilesReconcileConfig,
    owner: Uuid,
    lease: MemoryFilesLease,
) {
    let tenant_id = lease.tenant_id;
    let page = reconcile_files_memory_page(&metadata, &note_service, tenant_id, lease.cursor, 100);
    tokio::pin!(page);
    let result = loop {
        tokio::select! {
            result = &mut page => break result,
            _ = tokio::time::sleep(config.lease / 3) => {
                let heartbeat = sqlx::query("UPDATE memory_files_reconcile_jobs SET heartbeat_at=NOW(), lease_until=NOW()+($4 * INTERVAL '1 second'), updated_at=NOW() WHERE tenant_id=$1 AND lease_owner=$2 AND lease_generation=$3 AND lease_until>NOW()")
                    .bind(tenant_id)
                    .bind(owner)
                    .bind(lease.generation)
                    .bind(config.lease.as_secs() as i64)
                    .execute(metadata.pool())
                    .await;
                if !matches!(heartbeat, Ok(ref row) if row.rows_affected() == 1) {
                    return;
                }
            }
        }
    };
    match result {
        Ok(page) => {
            if let Err(error) = finish_page(&metadata, &lease, owner, page, config.interval).await {
                tracing::error!(tenant_id = %tenant_id, error = %error, "Memory reconciliation checkpoint failed");
            }
        }
        Err(_) => {
            if let Err(error) = fail_page(&metadata, &lease, owner).await {
                tracing::error!(tenant_id = %tenant_id, error = %error, "Memory reconciliation retry update failed");
            }
        }
    }
}

#[doc(hidden)]
pub async fn finish_page(
    metadata: &MetadataStore,
    lease: &MemoryFilesLease,
    owner: Uuid,
    page: FilesReconcileResponse,
    interval: Duration,
) -> anyhow::Result<()> {
    let next = page.next_cursor.as_deref().map(parse_cursor).transpose()?;
    let complete = next.is_none();
    let res = sqlx::query("UPDATE memory_files_reconcile_jobs SET cursor_file_id=$4, processed=processed+$5, indexed=indexed+$6, removed=removed+$7, unavailable=unavailable+$8, status='idle', retry_count=0, last_error=NULL, last_success_at=CASE WHEN $9 THEN NOW() ELSE last_success_at END, next_run_at=CASE WHEN $9 THEN NOW()+($10 * INTERVAL '1 second') ELSE NOW() END, lease_owner=NULL, lease_until=NULL, heartbeat_at=NULL, updated_at=NOW() WHERE tenant_id=$1 AND lease_owner=$2 AND lease_generation=$3 AND lease_until>NOW()")
        .bind(lease.tenant_id).bind(owner).bind(lease.generation).bind(next).bind(page.processed as i64).bind(page.indexed as i64).bind(page.removed as i64).bind(page.unavailable as i64).bind(complete).bind(interval.as_secs() as i64).execute(metadata.pool()).await?;
    anyhow::ensure!(res.rows_affected() == 1, "lease lost before checkpoint");
    metrics::counter!("memory_files_reconcile_pages_total", "outcome" => "success").increment(1);
    Ok(())
}

#[doc(hidden)]
pub async fn fail_page(
    metadata: &MetadataStore,
    lease: &MemoryFilesLease,
    owner: Uuid,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE memory_files_reconcile_jobs SET retry_count=retry_count+1, last_error='reconciliation page failed', status=CASE WHEN retry_count+1 >= $4 THEN 'failed' ELSE 'idle' END, next_run_at=NOW()+(LEAST(3600, (30 * (1 << LEAST(retry_count, 7)))::int) * INTERVAL '1 second'), lease_owner=NULL, lease_until=NULL, heartbeat_at=NULL, updated_at=NOW() WHERE tenant_id=$1 AND lease_owner=$2 AND lease_generation=$3 AND lease_until>NOW()")
        .bind(lease.tenant_id).bind(owner).bind(lease.generation).bind(MAX_RETRIES).execute(metadata.pool()).await?;
    metrics::counter!("memory_files_reconcile_pages_total", "outcome" => "failure").increment(1);
    Ok(())
}

fn parse_cursor(value: &str) -> anyhow::Result<Uuid> {
    let parsed = rustshare_resource_auth::ResourceRef::from_uri(value)?;
    Ok(Uuid::parse_str(&parsed.resource_id)?)
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct MemoryFilesReconcileStatus {
    pub enabled: bool,
    pub status: String,
    pub last_success_at: Option<DateTime<Utc>>,
    pub cursor: Option<String>,
    pub processed: i64,
    pub indexed: i64,
    pub removed: i64,
    pub unavailable: i64,
    pub last_error: Option<String>,
    pub retry_count: i32,
    pub next_run_at: Option<DateTime<Utc>>,
}

#[utoipa::path(get, path = "/api/v1/admin/applications/memory/files/reconcile/status", tag = "Memory", responses((status = 200, body = MemoryFilesReconcileStatus), (status = 401, description = "Unauthorized"), (status = 403, description = "Tenant scope mismatch")))]
pub async fn get_status(
    AdminUser { user_id }: AdminUser,
    auth: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<Json<MemoryFilesReconcileStatus>, (StatusCode, Json<ErrorResponse>)> {
    let admin = state
        .user_repository
        .get_by_id(user_id)
        .await
        .map_err(|_| internal_error())?
        .filter(|u| u.disabled_at.is_none())
        .ok_or_else(unauthorized)?;
    if admin.tenant_id != auth.tenant_id {
        return Err(forbidden());
    }
    let row = sqlx::query("SELECT status, last_success_at, cursor_file_id, processed, indexed, removed, unavailable, last_error, retry_count, next_run_at FROM memory_files_reconcile_jobs WHERE tenant_id=$1")
        .bind(auth.tenant_id).fetch_optional(&state.db_pool).await.map_err(|_| internal_error())?;
    let Some(row) = row else {
        return Ok(Json(MemoryFilesReconcileStatus {
            enabled: enabled(),
            status: if enabled() { "idle" } else { "disabled" }.into(),
            last_success_at: None,
            cursor: None,
            processed: 0,
            indexed: 0,
            removed: 0,
            unavailable: 0,
            last_error: None,
            retry_count: 0,
            next_run_at: None,
        }));
    };
    let cursor: Option<Uuid> = row
        .try_get("cursor_file_id")
        .map_err(|_| internal_error())?;
    Ok(Json(MemoryFilesReconcileStatus {
        enabled: enabled(),
        status: if enabled() {
            row.try_get("status").map_err(|_| internal_error())?
        } else {
            "disabled".into()
        },
        last_success_at: row
            .try_get("last_success_at")
            .map_err(|_| internal_error())?,
        cursor: cursor.map(|id| {
            rustshare_resource_auth::ResourceRef::new(
                rustshare_core::domain::ApplicationId::new("io.elembra.files"),
                "file",
                id.to_string(),
            )
            .to_uri()
        }),
        processed: row.try_get("processed").map_err(|_| internal_error())?,
        indexed: row.try_get("indexed").map_err(|_| internal_error())?,
        removed: row.try_get("removed").map_err(|_| internal_error())?,
        unavailable: row.try_get("unavailable").map_err(|_| internal_error())?,
        last_error: row.try_get("last_error").map_err(|_| internal_error())?,
        retry_count: row.try_get("retry_count").map_err(|_| internal_error())?,
        next_run_at: row.try_get("next_run_at").map_err(|_| internal_error())?,
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/applications/memory/files/reconcile/retry", tag = "Memory", responses((status = 202, description = "Reconciliation scheduled"), (status = 401, description = "Unauthorized"), (status = 403, description = "Tenant scope mismatch"), (status = 409, description = "Reconciliation already running")))]
pub async fn retry(
    AdminUser { user_id }: AdminUser,
    auth: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let admin = state
        .user_repository
        .get_by_id(user_id)
        .await
        .map_err(|_| internal_error())?
        .filter(|u| u.disabled_at.is_none())
        .ok_or_else(unauthorized)?;
    if admin.tenant_id != auth.tenant_id {
        return Err(forbidden());
    }
    let result = sqlx::query("INSERT INTO memory_files_reconcile_jobs (tenant_id, status, next_run_at) VALUES ($1,'idle',NOW()) ON CONFLICT (tenant_id) DO UPDATE SET status='idle', retry_count=0, last_error=NULL, next_run_at=NOW(), lease_owner=NULL, lease_until=NULL, updated_at=NOW() WHERE memory_files_reconcile_jobs.status <> 'running'")
        .bind(auth.tenant_id).execute(&state.db_pool).await.map_err(|_| internal_error())?;
    if result.rows_affected() == 0 {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::new("reconciliation is currently running")),
        ));
    }
    Ok(StatusCode::ACCEPTED)
}

fn unauthorized() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse::new("Unauthorized")),
    )
}
fn enabled() -> bool {
    std::env::var("RUSTSHARE_MEMORY_FILES_RECONCILE_ENABLED")
        .is_ok_and(|value| value.eq_ignore_ascii_case("true"))
}
fn forbidden() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new("tenant scope mismatch")),
    )
}
fn internal_error() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse::new("internal server error")),
    )
}
