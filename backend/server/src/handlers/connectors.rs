use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    handlers::{AppError, AuthenticatedUser},
    services::connector_service::{
        ConnectorError, ConnectorView, RegisterConnector, UpdateConnector,
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

#[cfg(test)]
mod tests {
    use super::UpdateConnectorRequest;

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
