use std::sync::Arc;

use rustshare_core::domain::{
    ConnectorDescriptor, ConnectorHealth, ConnectorInstanceId, ConnectorInstanceStatus,
    ConnectorStatusError,
};
use rustshare_storage::{
    ConnectorInstanceRecord, ConnectorMutation, ConnectorRegistration, ConnectorStore,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("connector instance not found")]
    NotFound,
    #[error("connector instance changed; reload before retrying")]
    Conflict,
    #[error("invalid connector request: {0}")]
    Invalid(String),
    #[error("connector persistence failed")]
    Storage(#[from] sqlx::Error),
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterConnector {
    pub descriptor: ConnectorDescriptor,
    #[serde(default = "empty_config")]
    pub source_config: Value,
    pub credential_ref: Option<String>,
    pub mode: String,
}

fn empty_config() -> Value {
    serde_json::json!({})
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateConnector {
    pub source_config: Option<Value>,
    pub credential_ref: Option<Option<String>>,
    pub lifecycle_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorView {
    pub id: ConnectorInstanceId,
    pub connector_id: String,
    pub descriptor: Value,
    pub source_config: Value,
    pub credential_configured: bool,
    pub mode: String,
    pub lifecycle_state: String,
    pub status: ConnectorInstanceStatus,
    pub version: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<ConnectorInstanceRecord> for ConnectorView {
    fn from(row: ConnectorInstanceRecord) -> Self {
        let health = match row.health.as_str() {
            "healthy" => ConnectorHealth::Healthy,
            "degraded" => ConnectorHealth::Degraded,
            "auth_required" => ConnectorHealth::AuthRequired,
            "rate_limited" => ConnectorHealth::RateLimited,
            "paused" => ConnectorHealth::Paused,
            _ => ConnectorHealth::Failed,
        };
        let last_error = row.last_error.as_deref().and_then(|error| match error {
            "authentication" => Some(ConnectorStatusError::Authentication),
            "rate_limit" => Some(ConnectorStatusError::RateLimit),
            "source_unavailable" => Some(ConnectorStatusError::SourceUnavailable),
            "invalid_content" => Some(ConnectorStatusError::InvalidContent),
            "policy_denied" => Some(ConnectorStatusError::PolicyDenied),
            "unknown" => Some(ConnectorStatusError::Unknown),
            _ => None,
        });
        Self {
            id: row.id.into(),
            connector_id: row.connector_id,
            descriptor: row.descriptor,
            source_config: row.source_config,
            credential_configured: row.credential_ref.is_some(),
            mode: row.mode,
            lifecycle_state: row.lifecycle_state,
            status: ConnectorInstanceStatus {
                health,
                last_success_at: row.last_success_at,
                queued_count: row.queued_count.max(0) as u64,
                retry_count: row.retry_count.max(0) as u64,
                last_error,
            },
            version: row.version,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct ConnectorService {
    store: Arc<ConnectorStore>,
}

impl ConnectorService {
    pub fn new(store: Arc<ConnectorStore>) -> Self {
        Self { store }
    }

    pub async fn register(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
        idempotency_key: &str,
        request: RegisterConnector,
    ) -> Result<(ConnectorView, bool), ConnectorError> {
        if idempotency_key.trim().is_empty() || idempotency_key.len() > 128 {
            return Err(ConnectorError::Invalid(
                "Idempotency-Key must be 1 to 128 bytes".into(),
            ));
        }
        request
            .descriptor
            .validate()
            .map_err(|error| ConnectorError::Invalid(error.to_string()))?;
        if !mode_enabled(&request.descriptor, &request.mode) {
            return Err(ConnectorError::Invalid(
                "mode is not declared by the Connector descriptor".into(),
            ));
        }
        validate_config(&request.source_config)?;
        validate_credential_ref(request.credential_ref.as_deref())?;

        let descriptor = serde_json::to_value(&request.descriptor)
            .map_err(|error| ConnectorError::Invalid(error.to_string()))?;
        let digest = Sha256::digest(
            serde_json::to_vec(&(
                &descriptor,
                &request.source_config,
                &request.credential_ref,
                &request.mode,
            ))
            .map_err(|error| ConnectorError::Invalid(error.to_string()))?,
        );
        let digest: [u8; 32] = digest.into();
        let (record, created) = self
            .store
            .register(ConnectorRegistration {
                tenant_id,
                principal_id,
                connector_id: &request.descriptor.metadata.id,
                descriptor: &descriptor,
                source_config: &request.source_config,
                credential_ref: request.credential_ref.as_deref(),
                mode: &request.mode,
                idempotency_key,
                request_digest: &digest,
            })
            .await
            .map_err(|error| match error {
                sqlx::Error::RowNotFound => ConnectorError::Conflict,
                other => ConnectorError::Storage(other),
            })?;
        Ok((record.into(), created))
    }

    pub async fn get(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
        id: Uuid,
    ) -> Result<ConnectorView, ConnectorError> {
        self.store
            .get(tenant_id, principal_id, id)
            .await?
            .map(Into::into)
            .ok_or(ConnectorError::NotFound)
    }

    pub async fn list(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
    ) -> Result<Vec<ConnectorView>, ConnectorError> {
        Ok(self
            .store
            .list(tenant_id, principal_id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    pub async fn update(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
        id: Uuid,
        expected_version: i64,
        request: UpdateConnector,
    ) -> Result<ConnectorView, ConnectorError> {
        if expected_version < 1 {
            return Err(ConnectorError::Invalid("version must be positive".into()));
        }
        if let Some(config) = &request.source_config {
            validate_config(config)?;
        }
        if let Some(credential_ref) = &request.credential_ref {
            validate_credential_ref(credential_ref.as_deref())?;
        }
        if request.source_config.is_none()
            && request.credential_ref.is_none()
            && request.lifecycle_state.is_none()
        {
            return Err(ConnectorError::Invalid(
                "at least one Connector field must be updated".into(),
            ));
        }
        if request
            .lifecycle_state
            .as_deref()
            .is_some_and(|state| !matches!(state, "active" | "paused" | "disconnected"))
        {
            return Err(ConnectorError::Invalid("invalid lifecycle state".into()));
        }
        self.store
            .mutate(ConnectorMutation {
                tenant_id,
                principal_id,
                id,
                expected_version,
                source_config: request.source_config.as_ref(),
                credential_ref: request
                    .credential_ref
                    .as_ref()
                    .map(|value| value.as_deref()),
                lifecycle_state: request.lifecycle_state.as_deref(),
            })
            .await?
            .map(Into::into)
            .ok_or(ConnectorError::Conflict)
    }
}

fn mode_enabled(descriptor: &ConnectorDescriptor, mode: &str) -> bool {
    match mode {
        "reference" => descriptor.supports.modes.reference,
        "mirror" => descriptor.supports.modes.mirror,
        "import" => descriptor.supports.modes.import,
        "archive" => descriptor.supports.modes.archive,
        "export" => descriptor.supports.modes.export,
        "bidirectionalSync" => descriptor.supports.modes.bidirectional_sync,
        _ => false,
    }
}

fn validate_credential_ref(value: Option<&str>) -> Result<(), ConnectorError> {
    if let Some(value) = value {
        if value.len() > 512
            || !(value
                .strip_prefix("secret://")
                .or_else(|| value.strip_prefix("keyring://"))
                .is_some_and(|reference| !reference.is_empty()))
            || value.chars().any(char::is_control)
        {
            return Err(ConnectorError::Invalid(
                "credentialRef must be an opaque secret:// or keyring:// reference".into(),
            ));
        }
    }
    Ok(())
}

fn validate_config(value: &Value) -> Result<(), ConnectorError> {
    fn check(value: &Value) -> bool {
        match value {
            Value::Object(map) => map.iter().all(|(key, value)| {
                let key = key.to_ascii_lowercase().replace(['-', '_'], "");
                ![
                    "token",
                    "password",
                    "secret",
                    "privatekey",
                    "credential",
                    "apikey",
                    "accesskey",
                    "auth",
                ]
                .iter()
                .any(|part| key.contains(part))
                    && check(value)
            }),
            Value::Array(values) => values.iter().all(check),
            _ => true,
        }
    }

    if !value.is_object() {
        return Err(ConnectorError::Invalid(
            "sourceConfig must be an object".into(),
        ));
    }
    if serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len()) > 32 * 1024 {
        return Err(ConnectorError::Invalid(
            "sourceConfig exceeds 32 KiB".into(),
        ));
    }
    if !check(value) {
        return Err(ConnectorError::Invalid(
            "sourceConfig must not contain credentials".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_config_rejects_credentials_recursively() {
        let config = serde_json::json!({"paths": {"api_token": "do-not-store"}});
        assert!(validate_config(&config).is_err());
        assert!(validate_config(&serde_json::json!({"api_key":"do-not-store"})).is_err());
        assert!(validate_config(&serde_json::json!({"authorization":"Bearer x"})).is_err());
    }

    #[test]
    fn source_config_accepts_non_secret_settings() {
        let config = serde_json::json!({"roots": ["/notes"], "include_hidden": false});
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn credential_reference_rejects_credential_material() {
        assert!(validate_credential_ref(Some("bearer-token-value")).is_err());
        assert!(validate_credential_ref(Some("secret://")).is_err());
        assert!(validate_credential_ref(Some("secret://connectors/local-text/key-1")).is_ok());
    }
}
