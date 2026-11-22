use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    handlers::{AppError, AuthenticatedUser},
    services::connector_service::{
        AcknowledgeConnectorExecution, AuthenticatedConnectorWorker, ConnectorError,
        ConnectorExecutionLeaseView, ConnectorView, FailConnectorExecution, IssuedWorkerCredential,
        RegisterConnector, UpdateConnector,
    },
    state::DatabaseState,
};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateConnectorRequest {
    pub expected_version: i64,
    pub source_config: Option<serde_json::Value>,
    #[serde(
        default,
        deserialize_with = "crate::handlers::organization_admin::deserialize_present_nullable_string"
    )]
    pub credential_ref: Option<Option<String>>,
    pub lifecycle_state: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerLeaseFenceRequest {
    pub fence: i64,
}

#[utoipa::path(post, path = "/api/v1/connectors/{id}/worker-credentials", params(("id" = Uuid, Path)), responses((status = 201, body = IssuedWorkerCredential)), security(("bearer_auth" = [])))]
pub async fn rotate_worker_credential(
    State(state): State<DatabaseState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<IssuedWorkerCredential>), AppError> {
    let credential = state
        .connector_service
        .rotate_worker_credential(auth.tenant_id, auth.user_id, id)
        .await
        .map_err(map_error)?;
    Ok((StatusCode::CREATED, Json(credential)))
}

/// Connector credentials authenticate only to this Connector's worker API;
/// they are not accepted by user or Files routes.
#[utoipa::path(get, path = "/api/v1/connectors/worker/session", params(("Authorization" = String, Header, description = "Connector ecw1 credential")), responses((status = 200, body = ConnectorView), (status = 401, description = "Invalid, expired, revoked, paused, or disconnected worker credential")))]
pub async fn worker_session(
    State(state): State<DatabaseState>,
    headers: HeaderMap,
) -> Result<Json<ConnectorView>, Response> {
    let worker = authenticate_worker(&state, &headers).await?;
    Ok(Json(worker.instance))
}

#[utoipa::path(post, path = "/api/v1/connectors/worker/lease", params(("Authorization" = String, Header, description = "Connector ecw1 credential")), responses((status = 200, body = Option<ConnectorExecutionLeaseView>)))]
pub async fn claim_worker_lease(
    State(state): State<DatabaseState>,
    headers: HeaderMap,
) -> Result<Json<Option<ConnectorExecutionLeaseView>>, Response> {
    let worker = authenticate_worker(&state, &headers).await?;
    state
        .connector_service
        .claim_execution(&worker)
        .await
        .map(Json)
        .map_err(|error| map_error(error).into_response())
}

#[utoipa::path(post, path = "/api/v1/connectors/worker/lease/{lease_id}/renew", params(("Authorization" = String, Header, description = "Connector ecw1 credential"), ("lease_id" = Uuid, Path)), request_body = WorkerLeaseFenceRequest, responses((status = 200, body = ConnectorExecutionLeaseView), (status = 409, description = "Lease expired or fenced")))]
pub async fn renew_worker_lease(
    State(state): State<DatabaseState>,
    headers: HeaderMap,
    Path(lease_id): Path<Uuid>,
    Json(request): Json<WorkerLeaseFenceRequest>,
) -> Result<Json<ConnectorExecutionLeaseView>, Response> {
    let worker = authenticate_worker(&state, &headers).await?;
    let lease = state
        .connector_service
        .renew_execution(&worker, lease_id, request.fence)
        .await
        .map_err(|error| map_error(error).into_response())?
        .ok_or_else(|| {
            AppError::conflict("Connector execution lease expired or was fenced").into_response()
        })?;
    Ok(Json(lease))
}

#[utoipa::path(post, path = "/api/v1/connectors/worker/lease/{lease_id}/ack", params(("Authorization" = String, Header, description = "Connector ecw1 credential"), ("lease_id" = Uuid, Path)), request_body = AcknowledgeConnectorExecution, responses((status = 204, description = "Checkpoint acknowledged"), (status = 409, description = "Lease expired, fenced, or checkpoint is stale")))]
pub async fn acknowledge_worker_lease(
    State(state): State<DatabaseState>,
    headers: HeaderMap,
    Path(lease_id): Path<Uuid>,
    Json(request): Json<AcknowledgeConnectorExecution>,
) -> Result<StatusCode, Response> {
    let worker = authenticate_worker(&state, &headers).await?;
    let acknowledged = state
        .connector_service
        .acknowledge_execution(&worker, lease_id, request)
        .await
        .map_err(|error| map_error(error).into_response())?;
    if acknowledged {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::conflict("Connector execution lease expired or was fenced").into_response())
    }
}

#[utoipa::path(post, path = "/api/v1/connectors/worker/lease/{lease_id}/fail", params(("Authorization" = String, Header, description = "Connector ecw1 credential"), ("lease_id" = Uuid, Path)), request_body = FailConnectorExecution, responses((status = 204, description = "Failure status recorded"), (status = 409, description = "Lease expired or fenced")))]
pub async fn fail_worker_lease(
    State(state): State<DatabaseState>,
    headers: HeaderMap,
    Path(lease_id): Path<Uuid>,
    Json(request): Json<FailConnectorExecution>,
) -> Result<StatusCode, Response> {
    let worker = authenticate_worker(&state, &headers).await?;
    let recorded = state
        .connector_service
        .fail_execution(&worker, lease_id, request)
        .await
        .map_err(|error| map_error(error).into_response())?;
    if recorded {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::conflict("Connector execution lease expired or was fenced").into_response())
    }
}

async fn authenticate_worker(
    state: &DatabaseState,
    headers: &HeaderMap,
) -> Result<AuthenticatedConnectorWorker, Response> {
    let credential = connector_credential(headers).ok_or_else(worker_auth_rejection)?;
    match state
        .connector_service
        .authenticate_worker(credential)
        .await
    {
        Ok(worker) => Ok(worker),
        Err(ConnectorError::NotFound) => Err(worker_auth_rejection()),
        Err(error) => Err(map_error(error).into_response()),
    }
}

fn connector_credential(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, token)| scheme.eq_ignore_ascii_case("Connector") && !token.is_empty())
        .map(|(_, token)| token)
}

fn worker_auth_rejection() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(crate::handlers::ErrorResponse::new(
            "Invalid Connector credential",
        )),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{connector_credential, UpdateConnectorRequest};
    use axum::http::{header::AUTHORIZATION, HeaderMap, HeaderValue};

    #[test]
    fn worker_credential_uses_a_separate_authorization_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Connector ecw1_secret"),
        );
        assert_eq!(connector_credential(&headers), Some("ecw1_secret"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer ecw1_secret"),
        );
        assert_eq!(connector_credential(&headers), None);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("connector ecw1_secret"),
        );
        assert_eq!(connector_credential(&headers), Some("ecw1_secret"));
    }

    #[test]
    fn credential_reference_patch_distinguishes_omitted_null_and_value() {
        let omitted: UpdateConnectorRequest =
            serde_json::from_value(serde_json::json!({"expectedVersion": 1})).unwrap();
        assert_eq!(omitted.credential_ref, None);

        let cleared: UpdateConnectorRequest = serde_json::from_value(
            serde_json::json!({"expectedVersion": 1, "credentialRef": null}),
        )
        .unwrap();
        assert_eq!(cleared.credential_ref, Some(None));

        let set: UpdateConnectorRequest = serde_json::from_value(serde_json::json!({
            "expectedVersion": 1,
            "credentialRef": "secret://connector/key"
        }))
        .unwrap();
        assert_eq!(
            set.credential_ref,
            Some(Some("secret://connector/key".to_string()))
        );
    }
}

fn map_error(error: ConnectorError) -> AppError {
    match error {
        ConnectorError::NotFound => AppError::not_found("Connector instance not found"),
        ConnectorError::Conflict => {
            AppError::conflict("Connector instance changed or idempotency key reused")
        }
        ConnectorError::Invalid(message) => AppError::bad_request(message),
        ConnectorError::Storage(error) => {
            tracing::error!(error = %error, "connector persistence failed");
            AppError::internal("Connector operation failed")
        }
    }
}

#[utoipa::path(post, path = "/api/v1/connectors", request_body = RegisterConnector, params(("Idempotency-Key" = String, Header, description = "Stable retry key")), responses((status = 201, body = ConnectorView), (status = 200, body = ConnectorView)), security(("bearer_auth" = [])))]
pub async fn register_connector(
    State(state): State<DatabaseState>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Json(request): Json<RegisterConnector>,
) -> Result<(StatusCode, Json<ConnectorView>), AppError> {
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::bad_request("Idempotency-Key header is required"))?;
    let (record, created) = state
        .connector_service
        .register(auth.tenant_id, auth.user_id, key, request)
        .await
        .map_err(map_error)?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(record),
    ))
}

#[utoipa::path(get, path = "/api/v1/connectors", responses((status = 200, body = [ConnectorView])), security(("bearer_auth" = [])))]
pub async fn list_connectors(
    State(state): State<DatabaseState>,
    auth: AuthenticatedUser,
) -> Result<Json<Vec<ConnectorView>>, AppError> {
    state
        .connector_service
        .list(auth.tenant_id, auth.user_id)
        .await
        .map(Json)
        .map_err(map_error)
}

#[utoipa::path(get, path = "/api/v1/connectors/{id}", params(("id" = Uuid, Path)), responses((status = 200, body = ConnectorView)), security(("bearer_auth" = [])))]
pub async fn get_connector(
    State(state): State<DatabaseState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ConnectorView>, AppError> {
    state
        .connector_service
        .get(auth.tenant_id, auth.user_id, id)
        .await
        .map(Json)
        .map_err(map_error)
}

#[utoipa::path(patch, path = "/api/v1/connectors/{id}", request_body = UpdateConnectorRequest, params(("id" = Uuid, Path)), responses((status = 200, body = ConnectorView), (status = 409, description = "Stale version or disconnected instance")), security(("bearer_auth" = [])))]
pub async fn update_connector(
    State(state): State<DatabaseState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateConnectorRequest>,
) -> Result<Json<ConnectorView>, AppError> {
    state
        .connector_service
        .update(
            auth.tenant_id,
            auth.user_id,
            id,
            request.expected_version,
            UpdateConnector {
                source_config: request.source_config,
                credential_ref: request.credential_ref,
                lifecycle_state: request.lifecycle_state,
            },
        )
        .await
        .map(Json)
        .map_err(map_error)
}
