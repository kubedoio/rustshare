//! Bounded, workload-authenticated source reconciliation for Memory.

use axum::{
    extract::{OriginalUri, State},
    http::{HeaderMap, Method, StatusCode},
    response::IntoResponse,
    Json,
};
use rustshare_core::domain::{ApplicationId, PrincipalId, TenantId, WorkspaceId};
use rustshare_memory::files_source::{
    ConnectorSource, FilesProjectionSnapshot, FilesReconcilePage,
};
use rustshare_resource_auth::{
    AuthenticationContext, PrincipalContext, PrincipalKind, Representation, ResourceRef,
    SourceError, WorkloadIdentity,
};
use rustshare_storage::MemoryFilesCatalogStore;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{memory_worker_auth::MemoryWorkerRequestAuth, AppState};

const TENANTS_PATH: &str = "/internal/v1/memory/files/reconcile/tenants";
const PAGE_PATH: &str = "/internal/v1/memory/files/reconcile/page";
const SIGNATURE_HEADER: &str = "X-Elembra-Memory-Signature";
const MAX_PAGE_SIZE: u32 = 100;
const MAX_RESPONSE_BYTES: usize = 1_000_000;
const MEMORY_WORKER_ID: Uuid = Uuid::from_u128(0x51b1_7a4e_83ba_4f29_9a41_4776_3e2d_0d1a);

#[derive(Deserialize)]
struct TenantPageRequest {
    after: Option<Uuid>,
    limit: u32,
}
#[derive(Deserialize)]
struct SourcePageRequest {
    tenant_id: Uuid,
    after_file_id: Option<Uuid>,
    limit: u32,
}
#[derive(Serialize)]
struct TenantPageResponse {
    tenant_ids: Vec<Uuid>,
    next_cursor: Option<Uuid>,
}

fn error(status: StatusCode) -> impl IntoResponse {
    (
        status,
        Json(serde_json::json!({ "error": status.canonical_reason() })),
    )
}

fn authenticate(
    method: &Method,
    expected_path: &str,
    uri: &axum::http::Uri,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), StatusCode> {
    let actual_path = uri
        .path_and_query()
        .map_or_else(|| uri.path().to_owned(), |value| value.as_str().to_owned());
    if method != Method::POST || actual_path != expected_path {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let signature = headers
        .get(SIGNATURE_HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let auth = MemoryWorkerRequestAuth::from_env().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    auth.verify_request(signature, method.as_str(), expected_path, body)
        .map_err(|_| StatusCode::UNAUTHORIZED)
}

fn valid_limit(limit: u32) -> bool {
    (1..=MAX_PAGE_SIZE).contains(&limit)
}

pub async fn tenant_page(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if let Err(status) = authenticate(&method, TENANTS_PATH, &uri, &headers, &body) {
        return error(status).into_response();
    }
    let Ok(request) = serde_json::from_slice::<TenantPageRequest>(&body) else {
        return error(StatusCode::BAD_REQUEST).into_response();
    };
    if !valid_limit(request.limit) {
        return error(StatusCode::BAD_REQUEST).into_response();
    }
    let result = async {
        let rows: Vec<Uuid> = sqlx::query_scalar("SELECT DISTINCT tenant_id FROM connector_external_mappings WHERE ($1::uuid IS NULL OR tenant_id > $1) ORDER BY tenant_id LIMIT $2")
            .bind(request.after).bind(i64::from(request.limit) + 1).fetch_all(&state.db_pool).await?;
        let more = rows.len() > request.limit as usize;
        let tenant_ids: Vec<_> = rows.into_iter().take(request.limit as usize).collect();
        let next_cursor = more.then(|| *tenant_ids.last().expect("bounded page is nonempty when more rows exist"));
        let bytes = serde_json::to_vec(&TenantPageResponse { tenant_ids, next_cursor })?;
        anyhow::ensure!(bytes.len() <= MAX_RESPONSE_BYTES, "tenant page exceeds response limit");
        anyhow::Ok(bytes)
    }.await;
    match result {
        Ok(bytes) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            bytes,
        )
            .into_response(),
        Err(err) => {
            tracing::warn!(error = %err, "Memory tenant reconciliation page failed");
            error(StatusCode::SERVICE_UNAVAILABLE).into_response()
        }
    }
}

pub async fn source_page(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if let Err(status) = authenticate(&method, PAGE_PATH, &uri, &headers, &body) {
        return error(status).into_response();
    }
    let Ok(request) = serde_json::from_slice::<SourcePageRequest>(&body) else {
        return error(StatusCode::BAD_REQUEST).into_response();
    };
    if !valid_limit(request.limit) {
        return error(StatusCode::BAD_REQUEST).into_response();
    }
    match resolve_source_page(&state, request).await {
        Ok(page) => match serde_json::to_vec(&page) {
            Ok(bytes) if bytes.len() <= MAX_RESPONSE_BYTES => (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                bytes,
            )
                .into_response(),
            _ => error(StatusCode::PAYLOAD_TOO_LARGE).into_response(),
        },
        Err(err) => {
            tracing::warn!(error = %err, "Memory source reconciliation page failed");
            error(StatusCode::SERVICE_UNAVAILABLE).into_response()
        }
    }
}

async fn resolve_source_page(
    state: &AppState,
    request: SourcePageRequest,
) -> anyhow::Result<FilesReconcilePage> {
    let observed_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&state.db_pool)
        .await?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "WITH mapped AS (SELECT CASE WHEN split_part(split_part(target_resource_ref, '/file/', 2), '?', 1) ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN split_part(split_part(target_resource_ref, '/file/', 2), '?', 1)::uuid END AS file_id FROM connector_external_mappings WHERE tenant_id=$1) SELECT DISTINCT file_id FROM mapped WHERE file_id IS NOT NULL AND ($2::uuid IS NULL OR file_id > $2) ORDER BY file_id LIMIT $3")
        .bind(request.tenant_id).bind(request.after_file_id).bind(i64::from(request.limit) + 1).fetch_all(&state.db_pool).await?;
    let more = ids.len() > request.limit as usize;
    let page_ids: Vec<_> = ids.into_iter().take(request.limit as usize).collect();
    let catalog = MemoryFilesCatalogStore::new(state.db_pool.clone());
    let mut snapshots = Vec::with_capacity(page_ids.len());
    for file_id in &page_ids {
        snapshots.push(
            resolve_current_file(state, &catalog, request.tenant_id, *file_id, observed_at).await?,
        );
    }
    while !snapshots.is_empty() {
        let n = snapshots.len();
        let next_cursor = (n < page_ids.len() || more).then(|| page_ids[n - 1]);
        let page = FilesReconcilePage {
            snapshots: snapshots.clone(),
            next_cursor,
        };
        if serde_json::to_vec(&page)?.len() <= MAX_RESPONSE_BYTES {
            return Ok(page);
        }
        snapshots.pop();
    }
    if page_ids.is_empty() {
        return Ok(FilesReconcilePage {
            snapshots,
            next_cursor: None,
        });
    }
    anyhow::bail!("single Memory source snapshot exceeds response limit")
}

async fn resolve_current_file(
    state: &AppState,
    catalog: &MemoryFilesCatalogStore,
    tenant_id: Uuid,
    file_id: Uuid,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<FilesProjectionSnapshot> {
    let Some(file) = state
        .metadata_store
        .find_file_by_id_for_tenant(file_id, tenant_id)
        .await?
    else {
        return Ok(FilesProjectionSnapshot::Deleted {
            tenant_id,
            file_id,
            observed_at,
        });
    };
    let version = format!("sha256:{}", file.content_hash);
    let Some(record) = catalog
        .connector_record_for_current_file(
            tenant_id,
            file_id,
            version.clone(),
            i64::from(file.current_version),
        )
        .await?
    else {
        return Ok(FilesProjectionSnapshot::Ignored {
            tenant_id,
            file_id,
            observed_at,
        });
    };
    let file_ref = ResourceRef::new(
        ApplicationId::new("io.elembra.files"),
        "file",
        file_id.to_string(),
    )
    .with_version(version.clone());
    let audit_id = reconciliation_audit_id(tenant_id, file_id, &version);
    let context = memory_worker_context(tenant_id);
    let fetched = state
        .source_authorizer
        .fetch(&context, &file_ref, Representation::Text)
        .await;
    catalog
        .record_source_access(
            audit_id,
            tenant_id,
            file_id,
            MEMORY_WORKER_ID,
            &version,
            fetched.is_ok(),
        )
        .await?;
    let content = match fetched {
        Ok(result) => String::from_utf8(result.data.to_vec())?,
        Err(SourceError::Unauthorized) => {
            return Ok(FilesProjectionSnapshot::Ignored {
                tenant_id,
                file_id,
                observed_at,
            });
        }
        Err(SourceError::NotFound) => {
            return Ok(FilesProjectionSnapshot::Deleted {
                tenant_id,
                file_id,
                observed_at,
            });
        }
        Err(err) => return Err(err.into()),
    };
    let connector = ConnectorSource {
        instance_id: record.connector_instance_id,
        resource_ref: record.source_resource_ref,
        revision: record.source_revision,
        relative_path: record.relative_path,
        observed_modified_at: record.observed_modified_at,
    };
    let snapshot = state
        .note_service
        .prepare_connector_index_snapshot(&file, observed_at, connector, content)
        .await?;
    match snapshot {
        Some(snapshot) => Ok(FilesProjectionSnapshot::Available(Box::new(snapshot))),
        None => Ok(FilesProjectionSnapshot::Ignored {
            tenant_id,
            file_id,
            observed_at,
        }),
    }
}

fn memory_worker_context(tenant_id: Uuid) -> PrincipalContext {
    // Memory indexes Connector-owned Files under current owner read authority; search reauthorizes each caller.
    let workload = PrincipalId(MEMORY_WORKER_ID);
    PrincipalContext {
        principal_id: workload,
        principal_kind: PrincipalKind::Service,
        tenant_id: TenantId(tenant_id),
        workspace_id: WorkspaceId(tenant_id),
        group_ids: Vec::new(),
        grants: Vec::new(),
        authentication: Some(AuthenticationContext {
            method: "workload_hmac".into(),
            issuer: None,
            strength: Some("service".into()),
        }),
        delegation: None,
        workload_identity: Some(WorkloadIdentity {
            application_id: Some(ApplicationId::new("io.elembra.memory")),
            subject: Some("memory-files-projector".into()),
        }),
        correlation_id: None,
    }
}

fn reconciliation_audit_id(tenant_id: Uuid, file_id: Uuid, version: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("elembra:memory:files:reconcile:{tenant_id}:{file_id}:{version}").as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_size_is_bounded() {
        assert!(!valid_limit(0));
        assert!(valid_limit(1));
        assert!(valid_limit(100));
        assert!(!valid_limit(101));
    }
    #[test]
    fn audit_id_is_stable_and_versioned() {
        let tenant = Uuid::new_v4();
        let file = Uuid::new_v4();
        let first = reconciliation_audit_id(tenant, file, "sha256:a");
        assert_eq!(first, reconciliation_audit_id(tenant, file, "sha256:a"));
        assert_ne!(first, reconciliation_audit_id(tenant, file, "sha256:b"));
        assert_ne!(
            first,
            reconciliation_audit_id(Uuid::new_v4(), file, "sha256:a")
        );
    }
}
