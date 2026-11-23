//! Authenticated, resource-scoped Files snapshots for the Memory worker.

use axum::{
    extract::{OriginalUri, State},
    http::{HeaderMap, Method, StatusCode},
    response::IntoResponse,
    Json,
};
use rustshare_core::domain::{ApplicationId, PrincipalId, TenantId, WorkspaceId};
use rustshare_integration_events::{event_types::*, IntegrationEvent};
use rustshare_memory::files_source::{ConnectorSource, FilesProjectionSnapshot};
use rustshare_resource_auth::{
    AuthenticationContext, PrincipalContext, PrincipalKind, Representation, ResourceRef,
    SourceError, WorkloadIdentity,
};
use rustshare_storage::MemoryFilesCatalogStore;
use serde::Deserialize;
use uuid::Uuid;

use crate::{memory_worker_auth::MemoryWorkerRequestAuth, AppState};

const ROUTE_PATH: &str = "/internal/v1/memory/files/snapshot";
const SIGNATURE_HEADER: &str = "X-Elembra-Memory-Signature";
const MAX_REQUEST_BODY_BYTES: usize = 4 * 1024;
const MEMORY_WORKER_ID: Uuid = Uuid::from_u128(0x51b1_7a4e_83ba_4f29_9a41_4776_3e2d_0d1a);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotRequest {
    source: String,
    event_id: Uuid,
}

fn error(status: StatusCode) -> impl IntoResponse {
    (
        status,
        Json(serde_json::json!({ "error": status.canonical_reason() })),
    )
}

/// Resolve only the current state associated with a durable Files event.
pub async fn files_snapshot(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if body.len() > MAX_REQUEST_BODY_BYTES {
        return error(StatusCode::PAYLOAD_TOO_LARGE).into_response();
    }
    let path = uri.path_and_query().map_or_else(
        || uri.path().to_owned(),
        |path_and_query| path_and_query.as_str().to_owned(),
    );
    if method != Method::POST || path != ROUTE_PATH {
        return error(StatusCode::UNAUTHORIZED).into_response();
    }
    let Some(signature) = headers
        .get(SIGNATURE_HEADER)
        .and_then(|value| value.to_str().ok())
    else {
        return error(StatusCode::UNAUTHORIZED).into_response();
    };
    let Ok(auth) = MemoryWorkerRequestAuth::from_env() else {
        return error(StatusCode::SERVICE_UNAVAILABLE).into_response();
    };
    if auth
        .verify_request(signature, method.as_str(), &path, &body)
        .is_err()
    {
        return error(StatusCode::UNAUTHORIZED).into_response();
    }
    let Ok(request) = serde_json::from_slice::<SnapshotRequest>(&body) else {
        return error(StatusCode::BAD_REQUEST).into_response();
    };
    if !valid_snapshot_request(&request) {
        return error(StatusCode::BAD_REQUEST).into_response();
    }
    let observed_at = match sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&state.db_pool)
        .await
    {
        Ok(observed_at) => observed_at,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE).into_response(),
    };
    match resolve_snapshot(&state, &request.source, request.event_id, observed_at).await {
        Ok(snapshot) => (StatusCode::OK, Json(snapshot)).into_response(),
        Err(cause) => {
            tracing::warn!(event_id = %request.event_id, error = %cause, "Memory Files snapshot failed");
            error(StatusCode::CONFLICT).into_response()
        }
    }
}

async fn resolve_snapshot(
    state: &AppState,
    source: &str,
    event_id: Uuid,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<FilesProjectionSnapshot> {
    let event_json: serde_json::Value = sqlx::query_scalar(
        "SELECT event_json FROM integration_outbox WHERE source = $1 AND event_id = $2",
    )
    .bind(source)
    .bind(event_id)
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("unknown integration event"))?;
    let event: IntegrationEvent = serde_json::from_value(event_json)?;
    anyhow::ensure!(
        event.source == source,
        "event source does not match outbox key"
    );
    anyhow::ensure!(
        matches!(
            event.r#type.as_str(),
            FILES_FILE_CREATED_V1 | FILES_FILE_UPDATED_V1 | FILES_FILE_DELETED_V1
        ),
        "unsupported event type"
    );
    let resource = event
        .resource
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Files event has no resource"))?;
    anyhow::ensure!(
        resource.application == ApplicationId::new("io.elembra.files")
            && resource.resource_type == "file",
        "event is not a Files resource"
    );
    let file_id = Uuid::parse_str(&resource.resource_id)?;
    let tenant_id = event.tenant_id.0;
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
    let current_version = format!("sha256:{}", file.content_hash);
    let catalog = MemoryFilesCatalogStore::new(state.db_pool.clone());
    // Resolve provenance from the current Connector mapping, not the event
    // payload: an older create/update may be delivered after a newer import.
    let connector_event = event.data.get("sourceProvenance").is_some();
    let Some(record) = catalog
        .connector_record_for_current_file(
            tenant_id,
            file_id,
            current_version.clone(),
            i64::from(file.current_version),
        )
        .await?
    else {
        return missing_connector_mapping_snapshot(
            tenant_id,
            file_id,
            observed_at,
            connector_event,
        );
    };

    let file_ref = ResourceRef::new(
        ApplicationId::new("io.elembra.files"),
        "file",
        file_id.to_string(),
    )
    .with_version(current_version.clone());
    let access_id = event_access_audit_id(source, event_id);
    let context = memory_worker_context(tenant_id);
    let fetched = state
        .source_authorizer
        .fetch(&context, &file_ref, Representation::Text)
        .await;
    catalog
        .record_source_access(
            access_id,
            tenant_id,
            file_id,
            MEMORY_WORKER_ID,
            &current_version,
            fetched.is_ok(),
        )
        .await?;
    let content = match fetched {
        Ok(fetched) => String::from_utf8(fetched.data.to_vec())?,
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
        Err(error) => return Err(error.into()),
    };
    let connector = ConnectorSource {
        instance_id: record.connector_instance_id,
        resource_ref: record.source_resource_ref.clone(),
        revision: record.source_revision.clone(),
        relative_path: record.relative_path.clone(),
        observed_modified_at: record.observed_modified_at,
    };
    let Some(snapshot) = state
        .note_service
        .prepare_connector_index_snapshot(&file, observed_at, connector, content)
        .await?
    else {
        return Ok(FilesProjectionSnapshot::Ignored {
            tenant_id,
            file_id,
            observed_at,
        });
    };
    Ok(FilesProjectionSnapshot::Available(Box::new(snapshot)))
}

fn event_access_audit_id(source: &str, event_id: Uuid) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("elembra:memory:files:event:{source}:{event_id}").as_bytes(),
    )
}

fn memory_worker_context(tenant_id: Uuid) -> PrincipalContext {
    // Files applies its explicit Memory indexing policy and resolves the live owner permission.
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

fn valid_snapshot_request(request: &SnapshotRequest) -> bool {
    !request.source.is_empty() && request.source.len() <= 255 && !request.event_id.is_nil()
}

fn missing_connector_mapping_snapshot(
    tenant_id: Uuid,
    file_id: Uuid,
    observed_at: chrono::DateTime<chrono::Utc>,
    connector_event: bool,
) -> anyhow::Result<FilesProjectionSnapshot> {
    anyhow::ensure!(
        !connector_event,
        "Connector provenance mapping is not committed yet"
    );
    Ok(FilesProjectionSnapshot::Ignored {
        tenant_id,
        file_id,
        observed_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexing_context_is_an_authenticated_service_without_user_delegation() {
        let tenant = Uuid::new_v4();
        let context = memory_worker_context(tenant);
        assert_eq!(context.principal_kind, PrincipalKind::Service);
        assert_eq!(context.principal_id, PrincipalId(MEMORY_WORKER_ID));
        assert_eq!(
            context.authentication.as_ref().unwrap().method,
            "workload_hmac"
        );
        assert!(context.delegation.is_none());
    }

    #[test]
    fn source_access_audit_id_uses_composite_outbox_identity() {
        let event_id = Uuid::new_v4();
        assert_eq!(
            event_access_audit_id("connector-a", event_id),
            event_access_audit_id("connector-a", event_id)
        );
        assert_ne!(
            event_access_audit_id("connector-a", event_id),
            event_access_audit_id("connector-b", event_id)
        );
    }

    #[test]
    fn snapshot_request_requires_bounded_source_and_non_nil_event() {
        assert!(valid_snapshot_request(&SnapshotRequest {
            source: "connector-worker".into(),
            event_id: Uuid::new_v4(),
        }));
        assert!(!valid_snapshot_request(&SnapshotRequest {
            source: String::new(),
            event_id: Uuid::new_v4(),
        }));
        assert!(!valid_snapshot_request(&SnapshotRequest {
            source: "x".repeat(256),
            event_id: Uuid::new_v4(),
        }));
        assert!(!valid_snapshot_request(&SnapshotRequest {
            source: "connector-worker".into(),
            event_id: Uuid::nil(),
        }));
    }

    #[test]
    fn missing_mapping_retries_connector_events_but_ignores_ordinary_files_events() {
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let observed_at = chrono::Utc::now();

        assert!(
            missing_connector_mapping_snapshot(tenant_id, file_id, observed_at, true,).is_err()
        );
        assert!(matches!(
            missing_connector_mapping_snapshot(tenant_id, file_id, observed_at, false),
            Ok(FilesProjectionSnapshot::Ignored { .. })
        ));
    }
}
