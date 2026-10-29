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
    pub page_timeout: Duration,
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
        anyhow::ensure!(
            (1..=3_600).contains(&config.memory_files_reconcile_page_timeout_secs),
            "memory reconcile page timeout must be between 1 and 3600 seconds"
        );
        Ok(Self {
            interval: Duration::from_secs(config.memory_files_reconcile_interval_secs),
            poll_interval: Duration::from_secs(config.memory_files_reconcile_poll_secs),
            lease: Duration::from_secs(config.memory_files_reconcile_lease_secs),
            max_concurrent: config.memory_files_reconcile_max_concurrent,
            page_timeout: Duration::from_secs(config.memory_files_reconcile_page_timeout_secs),
        })
    }
}

pub fn spawn_memory_files_reconcile_worker(
    metadata: Arc<MetadataStore>,
    note_service: Arc<NoteService>,
    config: MemoryFilesReconcileConfig,
    mut shutdown: broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let worker_id = Uuid::new_v4();
        let mut tasks = JoinSet::new();
        'worker: loop {
            if !matches!(
                tokio::time::timeout(config.page_timeout, discover_tenant_page(&metadata)).await,
                Ok(Ok(()))
            ) {
                tracing::error!("Memory reconciliation tenant discovery failed or timed out");
            }
            while tasks.len() < config.max_concurrent {
                let claimed = tokio::select! {
                    biased;
                    _ = shutdown.recv() => break 'worker,
                    result = tokio::time::timeout(config.page_timeout, claim(&metadata, worker_id, config.lease)) => result,
                };
                match claimed {
                    Ok(Ok(Some(claim))) => {
                        let metadata = Arc::clone(&metadata);
                        let note_service = Arc::clone(&note_service);
                        tasks.spawn(async move {
                            run_page(metadata, note_service, config, worker_id, claim).await;
                        });
                    }
                    Ok(Ok(None)) => break,
                    _ => {
                        tracing::error!("Memory reconciliation claim failed or timed out");
                        break;
                    }
                }
            }
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(config.poll_interval) => {},
                result = tasks.join_next(), if !tasks.is_empty() => {
                    if matches!(result, Some(Err(_))) {
                        tracing::error!("Memory reconciliation task stopped unexpectedly; lease will recover");
                    }
                },
            }
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    })
}

#[doc(hidden)]
pub struct MemoryFilesLease {
    pub tenant_id: Uuid,
    pub generation: i64,
    pub cursor: Option<Uuid>,
}

async fn lease_transaction(
    pool: &sqlx::PgPool,
) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

#[doc(hidden)]
pub async fn heartbeat(
    metadata: &MetadataStore,
    lease: &MemoryFilesLease,
    owner: Uuid,
    duration: Duration,
) -> anyhow::Result<()> {
    let mut tx = lease_transaction(metadata.pool()).await?;
    let result = sqlx::query("UPDATE memory_files_reconcile_jobs SET heartbeat_at=clock_timestamp(), lease_until=clock_timestamp()+($4 * INTERVAL '1 second'), updated_at=clock_timestamp() WHERE tenant_id=$1 AND lease_owner=$2 AND lease_generation=$3 AND lease_until>clock_timestamp()")
        .bind(lease.tenant_id).bind(owner).bind(lease.generation).bind(duration.as_secs() as i64)
        .execute(&mut *tx).await?;
    anyhow::ensure!(result.rows_affected() == 1, "lease lost before heartbeat");
    tx.commit().await?;
    Ok(())
}

#[doc(hidden)]
pub async fn claim(
    metadata: &MetadataStore,
    owner: Uuid,
    lease: Duration,
) -> anyhow::Result<Option<MemoryFilesLease>> {
    let mut tx = lease_transaction(metadata.pool()).await?;
    let row = sqlx::query(
        "UPDATE memory_files_reconcile_jobs SET status='running', lease_owner=$1, lease_until=clock_timestamp()+($2 * INTERVAL '1 second'), heartbeat_at=clock_timestamp(), lease_generation=lease_generation+1, last_claimed_at=NOW(), updated_at=NOW() WHERE tenant_id=(SELECT tenant_id FROM memory_files_reconcile_jobs WHERE status IN ('idle','running') AND next_run_at<=NOW() AND (lease_until IS NULL OR lease_until<=clock_timestamp()) ORDER BY next_run_at, last_claimed_at NULLS FIRST, tenant_id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING tenant_id, lease_generation, cursor_file_id",
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

#[doc(hidden)]
pub async fn discover_tenant_page(metadata: &MetadataStore) -> anyhow::Result<()> {
    let mut tx = lease_transaction(metadata.pool()).await?;
    let cursor: Option<Uuid> = sqlx::query_scalar(
        "SELECT cursor_tenant_id FROM memory_files_reconcile_discovery WHERE singleton=true FOR UPDATE",
    )
    .fetch_one(&mut *tx)
    .await?;
    let tenants = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM tenants WHERE ($1::uuid IS NULL OR id > $1) ORDER BY id LIMIT 100",
    )
    .bind(cursor)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO memory_files_reconcile_jobs (tenant_id) SELECT tenant_id FROM unnest($1::uuid[]) AS discovered(tenant_id) WHERE EXISTS (SELECT 1 FROM files f WHERE f.tenant_id=discovered.tenant_id) ON CONFLICT (tenant_id) DO NOTHING")
        .bind(&tenants).execute(&mut *tx).await?;
    let next = (tenants.len() == 100).then(|| tenants[99]);
    sqlx::query(
        "UPDATE memory_files_reconcile_discovery SET cursor_tenant_id=$1 WHERE singleton=true",
    )
    .bind(next)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn run_page(
    metadata: Arc<MetadataStore>,
    note_service: Arc<NoteService>,
    config: MemoryFilesReconcileConfig,
    owner: Uuid,
    lease: MemoryFilesLease,
) {
    let tenant_id = lease.tenant_id;
    // Drop an interrupted page to release source locks before retry bookkeeping.
    let result = {
        let page =
            reconcile_files_memory_page(&metadata, &note_service, tenant_id, lease.cursor, 100);
        tokio::pin!(page);
        let deadline = tokio::time::sleep(config.page_timeout);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                biased;
                _ = &mut deadline => break None,
                result = &mut page => break Some(result),
                _ = tokio::time::sleep(config.lease / 3) => {
                    let heartbeat = tokio::select! {
                        _ = &mut deadline => break None,
                        result = tokio::time::timeout(config.lease / 3, heartbeat(&metadata, &lease, owner, config.lease))
                        => result,
                    };
                    if !matches!(heartbeat, Ok(Ok(()))) {
                        tracing::warn!(tenant_id = %tenant_id, "Memory reconciliation lease lost; page will replay");
                        metrics::counter!("memory_files_reconcile_pages_total", "outcome" => "lease_lost").increment(1);
                        return;
                    }
                }
            }
        }
    };
    match result {
        Some(Ok(page)) => {
            if !matches!(
                tokio::time::timeout(
                    config.lease / 3,
                    finish_page(&metadata, &lease, owner, page, config.interval)
                )
                .await,
                Ok(Ok(()))
            ) {
                tracing::error!(tenant_id = %tenant_id, "Memory reconciliation checkpoint failed; page will replay");
            }
        }
        _ => {
            tracing::warn!(tenant_id = %tenant_id, "Memory reconciliation page failed or timed out");
            if !matches!(
                tokio::time::timeout(config.lease / 3, fail_page(&metadata, &lease, owner)).await,
                Ok(Ok(()))
            ) {
                tracing::error!(tenant_id = %tenant_id, "Memory reconciliation retry update failed; page will replay");
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
    let mut tx = lease_transaction(metadata.pool()).await?;
    let res = sqlx::query("UPDATE memory_files_reconcile_jobs SET cursor_file_id=$4, processed=processed+$5, indexed=indexed+$6, removed=removed+$7, unavailable=unavailable+$8, status='idle', retry_count=0, last_error=NULL, last_success_at=CASE WHEN $9 THEN NOW() ELSE last_success_at END, next_run_at=CASE WHEN $9 THEN NOW()+($10 * INTERVAL '1 second') ELSE NOW() END, lease_owner=NULL, lease_until=NULL, heartbeat_at=NULL, updated_at=NOW() WHERE tenant_id=$1 AND lease_owner=$2 AND lease_generation=$3 AND lease_until>clock_timestamp()")
        .bind(lease.tenant_id).bind(owner).bind(lease.generation).bind(next).bind(page.processed as i64).bind(page.indexed as i64).bind(page.removed as i64).bind(page.unavailable as i64).bind(complete).bind(interval.as_secs() as i64).execute(&mut *tx).await?;
    anyhow::ensure!(res.rows_affected() == 1, "lease lost before checkpoint");
    tx.commit().await?;
    metrics::counter!("memory_files_reconcile_pages_total", "outcome" => "success").increment(1);
    Ok(())
}

#[doc(hidden)]
pub async fn fail_page(
    metadata: &MetadataStore,
    lease: &MemoryFilesLease,
    owner: Uuid,
) -> anyhow::Result<()> {
    let mut tx = lease_transaction(metadata.pool()).await?;
    let res = sqlx::query("UPDATE memory_files_reconcile_jobs SET retry_count=retry_count+1, last_error='reconciliation page failed', status=CASE WHEN retry_count+1 >= $4 THEN 'failed' ELSE 'idle' END, next_run_at=NOW()+(LEAST(3600, (30 * (1 << LEAST(retry_count, 7)))::int) * INTERVAL '1 second'), lease_owner=NULL, lease_until=NULL, heartbeat_at=NULL, updated_at=NOW() WHERE tenant_id=$1 AND lease_owner=$2 AND lease_generation=$3 AND lease_until>clock_timestamp()")
        .bind(lease.tenant_id).bind(owner).bind(lease.generation).bind(MAX_RETRIES).execute(&mut *tx).await?;
    anyhow::ensure!(res.rows_affected() == 1, "lease lost before retry update");
    tx.commit().await?;
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
    let row = sqlx::query("SELECT status, lease_until<=clock_timestamp() AS lease_expired, last_success_at, cursor_file_id, processed, indexed, removed, unavailable, last_error, retry_count, next_run_at FROM memory_files_reconcile_jobs WHERE tenant_id=$1")
        .bind(auth.tenant_id).fetch_optional(&state.db_pool).await.map_err(|_| internal_error())?;
    let Some(row) = row else {
        return Ok(Json(MemoryFilesReconcileStatus {
            enabled: state.memory_files_reconcile_worker.is_some(),
            status: if state.memory_files_reconcile_worker.is_some() {
                "idle"
            } else {
                "disabled"
            }
            .into(),
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
    let status: String = row.try_get("status").map_err(|_| internal_error())?;
    let lease_expired: Option<bool> = row.try_get("lease_expired").map_err(|_| internal_error())?;
    Ok(Json(MemoryFilesReconcileStatus {
        enabled: state.memory_files_reconcile_worker.is_some(),
        status: if state.memory_files_reconcile_worker.is_some() {
            if status == "running" && lease_expired.unwrap_or(true) {
                "paused".into()
            } else {
                status
            }
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
    let mut tx = lease_transaction(&state.db_pool)
        .await
        .map_err(|_| internal_error())?;
    let result = sqlx::query("INSERT INTO memory_files_reconcile_jobs (tenant_id, status, next_run_at) VALUES ($1,'idle',NOW()) ON CONFLICT (tenant_id) DO UPDATE SET status='idle', retry_count=0, last_error=NULL, next_run_at=NOW(), lease_owner=NULL, lease_until=NULL, heartbeat_at=NULL, lease_generation=memory_files_reconcile_jobs.lease_generation+1, updated_at=NOW() WHERE memory_files_reconcile_jobs.status <> 'running' OR memory_files_reconcile_jobs.lease_until<=clock_timestamp()")
        .bind(auth.tenant_id).execute(&mut *tx).await.map_err(|_| internal_error())?;
    if result.rows_affected() == 0 {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::new("reconciliation is currently running")),
        ));
    }
    tx.commit().await.map_err(|_| internal_error())?;
    Ok(StatusCode::ACCEPTED)
}

fn unauthorized() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse::new("Unauthorized")),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_defaults_off_and_enforces_resource_bounds() {
        let mut config: crate::config::AppConfig = serde_json::from_value(serde_json::json!({
            "database_url": "postgres://localhost/rustshare_test",
            "jwt_secret": "unused-in-this-configuration-test",
            "rustfs_endpoint": "http://localhost:9000",
            "rustfs_region": "us-east-1",
            "rustfs_bucket": "rustshare-test",
            "rustshare_chat_webhook_secret": "unused-in-this-configuration-test"
        }))
        .unwrap();
        assert!(!config.memory_files_reconcile_enabled);
        assert!(!config.memory_files_orphan_cleanup_enabled);
        assert!(config.memory_files_orphan_cleanup_dry_run);
        assert!(MemoryFilesReconcileConfig::from_config(&config).is_ok());
        assert!(
            crate::memory_orphan_cleanup_worker::MemoryOrphanCleanupConfig::from_config(&config)
                .is_ok()
        );
        config.memory_files_reconcile_page_timeout_secs = 0;
        assert!(MemoryFilesReconcileConfig::from_config(&config).is_err());
        config.memory_files_reconcile_page_timeout_secs = 3_601;
        assert!(MemoryFilesReconcileConfig::from_config(&config).is_err());
        config.memory_files_reconcile_page_timeout_secs = 300;
        config.memory_files_reconcile_max_concurrent = 0;
        assert!(MemoryFilesReconcileConfig::from_config(&config).is_err());
        config.memory_files_reconcile_max_concurrent = 5;
        assert!(MemoryFilesReconcileConfig::from_config(&config).is_err());
    }
}
