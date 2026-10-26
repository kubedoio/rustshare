use axum::{extract::State, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    handlers::{AdminUser, AuthenticatedUser, ErrorResponse},
    state::AppState,
};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MemoryOrphanCleanupStatus {
    pub enabled: bool,
    pub dry_run: bool,
    pub status: String,
    pub last_success_at: Option<DateTime<Utc>>,
    pub cursor_chunk_id: Option<Uuid>,
    pub scanned: i64,
    pub candidates: i64,
    pub removed: i64,
    pub unresolved: i64,
    pub last_error: Option<String>,
    pub retry_count: i32,
    pub next_run_at: Option<DateTime<Utc>>,
}

#[utoipa::path(get, path = "/api/v1/admin/applications/memory/files/orphans/status", tag = "Memory", responses((status = 200, body = MemoryOrphanCleanupStatus), (status = 401, description = "Unauthorized"), (status = 403, description = "Tenant scope mismatch")))]
pub async fn get_status(
    AdminUser { user_id }: AdminUser,
    auth: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<Json<MemoryOrphanCleanupStatus>, (StatusCode, Json<ErrorResponse>)> {
    verify_admin(&state, user_id, auth.tenant_id).await?;
    let enabled = state.memory_orphan_cleanup_worker.is_some();
    let row = rustshare_infrastructure::repositories::MemoryOrphanCleanupRepository::new(
        state.db_pool.clone(),
    )
    .status(auth.tenant_id)
    .await
    .map_err(|_| internal_error())?;
    let status = if let Some(row) = row {
        let status = if enabled && row.status == "running" && row.lease_expired.unwrap_or(true) {
            "paused".to_string()
        } else if enabled {
            row.status
        } else {
            "disabled".to_string()
        };
        MemoryOrphanCleanupStatus {
            enabled,
            dry_run: state.memory_orphan_cleanup_dry_run,
            status,
            last_success_at: row.last_success_at,
            cursor_chunk_id: row.cursor_chunk_id,
            scanned: row.scanned,
            candidates: row.candidates,
            removed: row.removed,
            unresolved: row.unresolved,
            last_error: row.last_error,
            retry_count: row.retry_count,
            next_run_at: Some(row.next_run_at),
        }
    } else {
        MemoryOrphanCleanupStatus {
            enabled,
            dry_run: state.memory_orphan_cleanup_dry_run,
            status: if enabled { "idle" } else { "disabled" }.into(),
            last_success_at: None,
            cursor_chunk_id: None,
            scanned: 0,
            candidates: 0,
            removed: 0,
            unresolved: 0,
            last_error: None,
            retry_count: 0,
            next_run_at: None,
        }
    };
    Ok(Json(status))
}

#[utoipa::path(post, path = "/api/v1/admin/applications/memory/files/orphans/retry", tag = "Memory", responses((status = 202, description = "Orphan cleanup scheduled"), (status = 401, description = "Unauthorized"), (status = 403, description = "Tenant scope mismatch"), (status = 409, description = "Cleanup already running"), (status = 503, description = "Cleanup worker disabled")))]
pub async fn retry(
    AdminUser { user_id }: AdminUser,
    auth: AuthenticatedUser,
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    verify_admin(&state, user_id, auth.tenant_id).await?;
    if state.memory_orphan_cleanup_worker.is_none() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new("Memory orphan cleanup is disabled")),
        ));
    }
    let scheduled = rustshare_infrastructure::repositories::MemoryOrphanCleanupRepository::new(
        state.db_pool.clone(),
    )
    .retry(auth.tenant_id)
    .await
    .map_err(|_| internal_error())?;
    if !scheduled {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::new("orphan cleanup is currently running")),
        ));
    }
    Ok(StatusCode::ACCEPTED)
}

async fn verify_admin(
    state: &AppState,
    user_id: Uuid,
    tenant_id: Uuid,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let admin = state
        .user_repository
        .get_by_id(user_id)
        .await
        .map_err(|_| internal_error())?
        .filter(|user| user.disabled_at.is_none())
        .ok_or_else(unauthorized)?;
    if admin.tenant_id != tenant_id {
        return Err(forbidden());
    }
    Ok(())
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
