use axum::{
    extract::{Multipart, Path, State},
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
    state::{DatabaseState, ServiceState},
};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectorFileImportMetadata {
    pub lease_id: Uuid,
    pub fence: i64,
    pub source_resource_ref: String,
    pub source_revision: String,
    pub relative_path: String,
    pub observed_modified_at: chrono::DateTime<chrono::Utc>,
    pub parent_folder_id: Option<Uuid>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorFileImportResponse {
    pub source_resource_ref: String,
    pub source_revision: String,
    pub target_resource_ref: String,
}

#[derive(Debug, utoipa::ToSchema)]
pub struct ConnectorFileImportMultipart {
    #[schema(value_type = String)]
    pub metadata: String,
    #[schema(value_type = String, format = Binary)]
    pub file: String,
}

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

#[utoipa::path(post, path = "/api/v1/connectors/worker/files/import", request_body(content = ConnectorFileImportMultipart, content_type = "multipart/form-data"), params(("Authorization" = String, Header, description = "Connector ecw1 credential")), responses((status = 200, body = ConnectorFileImportResponse), (status = 400, description = "Invalid or unsafe source observation"), (status = 401, description = "Invalid Connector credential"), (status = 409, description = "Source revision conflicts with an existing mapping")))]
pub async fn import_connector_file(
    State(state): State<DatabaseState>,
    State(services): State<ServiceState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<ConnectorFileImportResponse>, Response> {
    let worker = authenticate_worker(&state, &headers).await?;
    let mut metadata: Option<ConnectorFileImportMetadata> = None;
    let mut file_temp: Option<tempfile::NamedTempFile> = None;
    while let Some(mut field) = multipart.next_field().await.map_err(|_| {
        AppError::bad_request("Malformed Connector import multipart body").into_response()
    })? {
        match field.name().unwrap_or("") {
            "metadata" if metadata.is_none() => {
                let bytes = field.bytes().await.map_err(|_| {
                    AppError::bad_request("Invalid Connector import metadata").into_response()
                })?;
                if bytes.len() > 16 * 1024 {
                    return Err(
                        AppError::bad_request("Connector import metadata is too large")
                            .into_response(),
                    );
                }
                metadata = Some(serde_json::from_slice(&bytes).map_err(|_| {
                    AppError::bad_request("Invalid Connector import metadata").into_response()
                })?);
            }
            "file" if file_temp.is_none() => {
                file_temp = Some(
                    super::stream_multipart_field_to_temp_file(&mut field, 4 * 1024 * 1024)
                        .await
                        .map_err(IntoResponse::into_response)?
                        .0,
                );
            }
            _ => {
                return Err(
                    AppError::bad_request("Unexpected or duplicate Connector import field")
                        .into_response(),
                )
            }
        }
    }
    let metadata = metadata.ok_or_else(|| {
        AppError::bad_request("Missing Connector import metadata").into_response()
    })?;
    let file_temp = file_temp
        .ok_or_else(|| AppError::bad_request("Missing Connector import file").into_response())?;
    if !approved_destination_matches(&worker.instance.source_config, metadata.parent_folder_id) {
        return Err(
            AppError::forbidden("Connector destination folder is not approved").into_response(),
        );
    }
    if !valid_source_revision(&metadata.source_revision) {
        return Err(AppError::bad_request("Invalid source revision").into_response());
    }
    if state
        .connector_service
        .renew_execution(&worker, metadata.lease_id, metadata.fence)
        .await
        .map_err(|error| map_error(error).into_response())?
        .is_none()
    {
        return Err(
            AppError::conflict("Connector execution lease expired or was fenced").into_response(),
        );
    }
    let source_ref = rustshare_resource_auth::ResourceRef::from_uri(&metadata.source_resource_ref)
        .map_err(|_| AppError::bad_request("Invalid source ResourceRef").into_response())?;
    if source_ref.application.0 != worker.instance.connector_id
        || source_ref.resource_type != "file"
        || source_ref.version.is_some()
    {
        return Err(
            AppError::bad_request("Source ResourceRef does not match this Connector")
                .into_response(),
        );
    }
    let source_resource_ref = source_ref.to_uri();
    if let Some(target) = state
        .connector_service
        .mapped_target(&worker, &source_resource_ref, &metadata.source_revision)
        .await
        .map_err(|error| map_error(error).into_response())?
    {
        let target_ref = rustshare_resource_auth::ResourceRef::from_uri(&target).map_err(|_| {
            AppError::internal("Stored Connector mapping is invalid").into_response()
        })?;
        if target_ref.application.0 != "io.elembra.files"
            || target_ref.resource_type != "file"
            || target_ref.version.as_deref() != Some(metadata.source_revision.as_str())
        {
            return Err(AppError::internal("Stored Connector mapping is invalid").into_response());
        }
        let target_id = Uuid::parse_str(&target_ref.resource_id).map_err(|_| {
            AppError::internal("Stored Connector mapping is invalid").into_response()
        })?;
        let authority = worker
            .context
            .effective_user_authority(
                &rustshare_core::domain::ActionCapability::new(
                    rustshare_resource_auth::FILES_WRITE,
                ),
                None,
            )
            .map_err(|_| worker_auth_rejection())?;
        services
            .file_service
            .get_file(target_id, authority.user_authority_id.0)
            .await
            .map_err(|error| AppError::from(error).into_response())?;
        return Ok(Json(ConnectorFileImportResponse {
            source_resource_ref,
            source_revision: metadata.source_revision,
            target_resource_ref: target,
        }));
    }

    let authority = worker
        .context
        .effective_user_authority(
            &rustshare_core::domain::ActionCapability::new(rustshare_resource_auth::FILES_WRITE),
            None,
        )
        .map_err(|_| worker_auth_rejection())?;
    let file_name = std::path::Path::new(&metadata.relative_path)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::bad_request("Invalid relative source path").into_response())?
        .to_owned();
    let mime_type = mime_guess::from_path(&file_name)
        .first_or_octet_stream()
        .to_string();
    let file = services
        .file_service
        .import_connector_file_from_path(rustshare_core::services::ConnectorFileImport {
            owner_id: authority.user_authority_id.0,
            connector_instance_id: Uuid::from(worker.instance.id),
            source_resource_ref: source_resource_ref.clone(),
            source_revision: metadata.source_revision.clone(),
            relative_path: metadata.relative_path,
            observed_modified_at: metadata.observed_modified_at,
            name: file_name,
            parent_folder_id: metadata.parent_folder_id,
            file_path: file_temp.path().to_path_buf(),
            mime_type,
            tenant_id: worker.context.tenant_id.0,
        })
        .await
        .map_err(|error| AppError::from(error).into_response())?;
    let target_ref = rustshare_resource_auth::ResourceRef::new(
        rustshare_core::domain::ApplicationId::new("io.elembra.files"),
        "file",
        file.id.to_string(),
    )
    .with_version(metadata.source_revision.clone())
    .to_uri();
    state
        .connector_service
        .record_mapping(
            &worker,
            &source_resource_ref,
            &metadata.source_revision,
            &target_ref,
        )
        .await
        .map_err(|error| map_error(error).into_response())?;
    Ok(Json(ConnectorFileImportResponse {
        source_resource_ref,
        source_revision: metadata.source_revision,
        target_resource_ref: target_ref,
    }))
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

fn valid_source_revision(revision: &str) -> bool {
    revision
        .strip_prefix("sha256:")
        .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn approved_destination_matches(
    source_config: &serde_json::Value,
    requested: Option<Uuid>,
) -> bool {
    match source_config.get("destinationFolderId") {
        None | Some(serde_json::Value::Null) => requested.is_none(),
        Some(serde_json::Value::String(folder_id)) => Uuid::parse_str(folder_id).ok() == requested,
        _ => false,
    }
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

#[cfg(test)]
mod connector_import_tests {
    use super::approved_destination_matches;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn import_destination_must_match_owner_configuration() {
        let folder_id = Uuid::new_v4();
        assert!(approved_destination_matches(&json!({}), None));
        assert!(!approved_destination_matches(&json!({}), Some(folder_id)));
        assert!(approved_destination_matches(
            &json!({"destinationFolderId": folder_id.to_string()}),
            Some(folder_id)
        ));
        assert!(!approved_destination_matches(
            &json!({"destinationFolderId": Uuid::new_v4().to_string()}),
            Some(folder_id)
        ));
    }
}
