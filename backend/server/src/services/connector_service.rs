use std::sync::Arc;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use rustshare_core::domain::{
    ActionCapability, ConnectorDescriptor, ConnectorHealth, ConnectorInstanceId,
    ConnectorInstanceStatus, ConnectorStatusError, PrincipalId, TenantId, WorkspaceId,
};
use rustshare_resource_auth::{Delegation, PrincipalContext, PrincipalKind, WorkloadIdentity};
use rustshare_storage::{
    ConnectorExecutionAcknowledgment, ConnectorInstanceRecord, ConnectorMutation,
    ConnectorRegistration, ConnectorStore, ConnectorWorkerCredentialIdentity,
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

const WORKER_CREDENTIAL_LIFETIME: chrono::Duration = chrono::Duration::days(30);
const EXECUTION_LEASE_SECONDS: i32 = 60;
const MAX_CURSOR_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssuedWorkerCredential {
    /// Returned only at issuance; the server stores only its SHA-256 hash.
    pub credential: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedConnectorWorker {
    pub context: PrincipalContext,
    pub instance: ConnectorView,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorExecutionLeaseView {
    pub lease_id: Uuid,
    pub fence: i64,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub checkpoint_version: i64,
    pub cursor: Option<Value>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcknowledgeConnectorExecution {
    pub fence: i64,
    pub expected_checkpoint_version: i64,
    pub cursor: Value,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FailConnectorExecution {
    pub fence: i64,
    pub error: ConnectorStatusError,
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

    pub async fn rotate_worker_credential(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
        instance_id: Uuid,
    ) -> Result<IssuedWorkerCredential, ConnectorError> {
        let mut secret = [0_u8; 32];
        rand::rng().fill_bytes(&mut secret);
        let credential = format!(
            "ecw1_{}.{}",
            Uuid::new_v4().simple(),
            URL_SAFE_NO_PAD.encode(secret)
        );
        let token_hash: [u8; 32] = Sha256::digest(credential.as_bytes()).into();
        let expires_at = chrono::Utc::now() + WORKER_CREDENTIAL_LIFETIME;
        self.store
            .rotate_worker_credential(
                tenant_id,
                principal_id,
                instance_id,
                Uuid::new_v4(),
                &token_hash,
                expires_at,
            )
            .await
            .map_err(|error| match error {
                sqlx::Error::RowNotFound => ConnectorError::NotFound,
                other => ConnectorError::Storage(other),
            })?;
        Ok(IssuedWorkerCredential {
            credential,
            expires_at,
        })
    }

    pub async fn authenticate_worker(
        &self,
        credential: &str,
    ) -> Result<AuthenticatedConnectorWorker, ConnectorError> {
        if !credential.starts_with("ecw1_") || credential.len() > 128 {
            return Err(ConnectorError::NotFound);
        }
        let token_hash: [u8; 32] = Sha256::digest(credential.as_bytes()).into();
        let authenticated = self
            .store
            .authenticate_worker_credential(&token_hash)
            .await?
            .ok_or(ConnectorError::NotFound)?;
        Ok(AuthenticatedConnectorWorker {
            context: worker_principal_context(&authenticated.identity),
            instance: authenticated.instance.into(),
        })
    }

    pub async fn claim_execution(
        &self,
        worker: &AuthenticatedConnectorWorker,
    ) -> Result<Option<ConnectorExecutionLeaseView>, ConnectorError> {
        let (tenant_id, principal_id, instance_id) = worker_scope(worker)?;
        self.store
            .claim_execution(
                tenant_id,
                principal_id,
                instance_id,
                Uuid::new_v4(),
                EXECUTION_LEASE_SECONDS,
            )
            .await
            .map(|lease| lease.map(Into::into))
            .map_err(Into::into)
    }

    pub async fn renew_execution(
        &self,
        worker: &AuthenticatedConnectorWorker,
        lease_id: Uuid,
        fence: i64,
    ) -> Result<Option<ConnectorExecutionLeaseView>, ConnectorError> {
        let (tenant_id, principal_id, instance_id) = worker_scope(worker)?;
        self.store
            .renew_execution(
                tenant_id,
                principal_id,
                instance_id,
                lease_id,
                fence,
                EXECUTION_LEASE_SECONDS,
            )
            .await
            .map(|lease| lease.map(Into::into))
            .map_err(Into::into)
    }

    pub async fn acknowledge_execution(
        &self,
        worker: &AuthenticatedConnectorWorker,
        lease_id: Uuid,
        acknowledgment: AcknowledgeConnectorExecution,
    ) -> Result<bool, ConnectorError> {
        if acknowledgment.fence < 1 || acknowledgment.expected_checkpoint_version < 0 {
            return Err(ConnectorError::Invalid(
                "lease fence must be positive and checkpoint version non-negative".into(),
            ));
        }
        validate_checkpoint_cursor(
            acknowledgment.expected_checkpoint_version,
            &acknowledgment.cursor,
        )?;
        let (tenant_id, principal_id, instance_id) = worker_scope(worker)?;
        self.store
            .acknowledge_execution(ConnectorExecutionAcknowledgment {
                tenant_id,
                principal_id,
                instance_id,
                lease_id,
                lease_fence: acknowledgment.fence,
                expected_checkpoint_version: acknowledgment.expected_checkpoint_version,
                cursor: &acknowledgment.cursor,
            })
            .await
            .map_err(Into::into)
    }

    pub async fn fail_execution(
        &self,
        worker: &AuthenticatedConnectorWorker,
        lease_id: Uuid,
        failure: FailConnectorExecution,
    ) -> Result<bool, ConnectorError> {
        if failure.fence < 1 {
            return Err(ConnectorError::Invalid(
                "lease fence must be positive".into(),
            ));
        }
        let category = match failure.error {
            ConnectorStatusError::Authentication => "authentication",
            ConnectorStatusError::RateLimit => "rate_limit",
            ConnectorStatusError::SourceUnavailable => "source_unavailable",
            ConnectorStatusError::InvalidContent => "invalid_content",
            ConnectorStatusError::PolicyDenied => "policy_denied",
            ConnectorStatusError::Unknown => "unknown",
        };
        let (tenant_id, principal_id, instance_id) = worker_scope(worker)?;
        self.store
            .fail_execution(
                tenant_id,
                principal_id,
                instance_id,
                lease_id,
                failure.fence,
                category,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn mapped_target(
        &self,
        worker: &AuthenticatedConnectorWorker,
        source_resource_ref: &str,
        source_revision: &str,
    ) -> Result<Option<String>, ConnectorError> {
        let (tenant_id, _, instance_id) = worker_scope(worker)?;
        self.store
            .mapped_target(tenant_id, instance_id, source_resource_ref, source_revision)
            .await
            .map_err(Into::into)
    }

    pub async fn record_mapping(
        &self,
        worker: &AuthenticatedConnectorWorker,
        source_resource_ref: &str,
        source_revision: &str,
        target_resource_ref: &str,
    ) -> Result<bool, ConnectorError> {
        let (tenant_id, _, instance_id) = worker_scope(worker)?;
        self.store
            .record_mapping(
                tenant_id,
                instance_id,
                source_resource_ref,
                source_revision,
                target_resource_ref,
            )
            .await
            .map_err(|error| match error {
                sqlx::Error::RowNotFound => ConnectorError::Conflict,
                other => ConnectorError::Storage(other),
            })
    }
}

fn validate_checkpoint_cursor(expected_version: i64, cursor: &Value) -> Result<(), ConnectorError> {
    let next_version = expected_version
        .checked_add(1)
        .ok_or_else(|| ConnectorError::Invalid("checkpoint version is exhausted".into()))?;
    let size = serde_json::to_vec(cursor)
        .map_err(|error| ConnectorError::Invalid(error.to_string()))?
        .len();
    if !cursor.is_object()
        || size > MAX_CURSOR_BYTES
        || cursor.get("revision").and_then(Value::as_i64) != Some(next_version)
    {
        return Err(ConnectorError::Invalid(format!(
            "cursor must be an object no larger than 64 KiB with revision {next_version}"
        )));
    }
    Ok(())
}

impl From<rustshare_storage::ConnectorExecutionLease> for ConnectorExecutionLeaseView {
    fn from(lease: rustshare_storage::ConnectorExecutionLease) -> Self {
        Self {
            lease_id: lease.lease_id,
            fence: lease.lease_fence,
            expires_at: lease.lease_expires_at,
            checkpoint_version: lease.checkpoint_version,
            cursor: lease.cursor,
        }
    }
}

fn worker_scope(
    worker: &AuthenticatedConnectorWorker,
) -> Result<(Uuid, Uuid, Uuid), ConnectorError> {
    let action = ActionCapability::new("files.write");
    let authority = worker
        .context
        .effective_user_authority(&action, None)
        .map_err(|_| ConnectorError::NotFound)?;
    Ok((
        worker.context.tenant_id.0,
        authority.user_authority_id.0,
        Uuid::from(worker.instance.id),
    ))
}

fn worker_principal_context(identity: &ConnectorWorkerCredentialIdentity) -> PrincipalContext {
    PrincipalContext {
        principal_id: PrincipalId(identity.instance_id),
        principal_kind: PrincipalKind::Service,
        tenant_id: TenantId(identity.tenant_id),
        workspace_id: WorkspaceId(identity.tenant_id),
        group_ids: Vec::new(),
        grants: Vec::new(),
        authentication: Some(rustshare_resource_auth::AuthenticationContext {
            method: "connector_worker_credential".into(),
            ..Default::default()
        }),
        delegation: Some(Delegation {
            issuer_principal_id: PrincipalId(identity.principal_id),
            delegate_principal_id: PrincipalId(identity.instance_id),
            actions: vec![ActionCapability::new("files.write")],
            workspace_id: Some(WorkspaceId(identity.tenant_id)),
            resource_scope: None,
            expires_at: Some(identity.expires_at),
            grant_id: Some(identity.credential_id.to_string()),
        }),
        workload_identity: Some(WorkloadIdentity {
            application_id: None,
            subject: Some(format!("connector-instance:{}", identity.instance_id)),
        }),
        correlation_id: None,
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

    #[test]
    fn worker_context_keeps_owner_principal_and_separate_workload_identity() {
        let identity = ConnectorWorkerCredentialIdentity {
            credential_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            principal_id: Uuid::new_v4(),
            instance_id: Uuid::new_v4(),
            expires_at: chrono::Utc::now() + chrono::Duration::days(1),
        };
        let context = worker_principal_context(&identity);
        assert_eq!(context.principal_id, PrincipalId(identity.instance_id));
        assert_eq!(context.principal_kind, PrincipalKind::Service);
        assert_eq!(context.tenant_id, TenantId(identity.tenant_id));
        assert_eq!(context.workspace_id, WorkspaceId(identity.tenant_id));
        assert_eq!(
            context
                .authentication
                .as_ref()
                .map(|auth| auth.method.as_str()),
            Some("connector_worker_credential")
        );
        assert_eq!(
            context
                .workload_identity
                .as_ref()
                .and_then(|workload| workload.subject.as_deref()),
            Some(format!("connector-instance:{}", identity.instance_id).as_str())
        );
        let delegated_write = context
            .effective_user_authority(&ActionCapability::new("files.write"), None)
            .unwrap();
        assert_eq!(
            delegated_write.user_authority_id,
            PrincipalId(identity.principal_id)
        );
        for action in ["files.read", "files.delete", "memory.read", "memory.write"] {
            assert!(context
                .effective_user_authority(&ActionCapability::new(action), None)
                .is_err());
        }
    }

    #[test]
    fn checkpoint_cursor_revision_must_advance_exactly_once() {
        assert!(validate_checkpoint_cursor(0, &serde_json::json!({"revision": 1})).is_ok());
        assert!(validate_checkpoint_cursor(0, &serde_json::json!({"revision": 0})).is_err());
        assert!(validate_checkpoint_cursor(0, &serde_json::json!({"revision": 2})).is_err());
        assert!(validate_checkpoint_cursor(0, &serde_json::json!([])).is_err());
        assert!(validate_checkpoint_cursor(i64::MAX, &serde_json::json!({"revision": 0})).is_err());
    }
}
