//! Test-called managed identity provisioning lifecycle.
//!
//! No route, worker, startup hook, or production caller is wired. Tests supply
//! an authenticated disposable Principal context, signed mapping, test KMS,
//! and disposable relay-admin key explicitly.

use std::time::Duration;

use chrono::Utc;
use nostr::{Keys, Timestamp};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_crypto::managed_identity_vault::{
    DekKeyManagement, IdentityContext, ManagedIdentityEnvelope, RecoveredIdentity, WrappedDek,
};
use rustshare_resource_auth::chat_identity::build_buzz_provisioning_admission_event_at;
use rustshare_storage::{
    ChatIdentityStore, ManagedIdentityOperationError, ManagedIdentityOperationRecord,
    ManagedIdentityOperationStatus, ManagedIdentityOperationStore, ManagedIdentityRecord,
    ManagedIdentityStoreError, ManagedIdentityVaultStore, UpstreamRelayMappingStore,
};
use serde_json::Value;
use sqlx::PgPool;
use thiserror::Error;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

use crate::upstream_buzz_transport::{
    connect, RelayTlsTrustConfig, TenantRelaySession, UpstreamBuzzTransportError,
};

/// Facts supplied only by a trusted, test-only authenticated session fixture.
#[derive(Clone, Debug)]
pub(crate) struct AuthenticatedTestPrincipal {
    user_id: Uuid,
    tenant_id: Uuid,
    workspace_id: Uuid,
}

impl AuthenticatedTestPrincipal {
    pub(crate) fn new(user_id: Uuid, tenant_id: Uuid, workspace_id: Uuid) -> Self {
        Self {
            user_id,
            tenant_id,
            workspace_id,
        }
    }
}

/// Authorized tenant/Principal/workspace after enabled-user and Chat access
/// checks. Raw request UUIDs never become a subject directly.
#[derive(Clone, Debug)]
pub(crate) struct ProvisioningSubject {
    pub(crate) tenant_id: TenantId,
    pub(crate) principal_id: PrincipalId,
    pub(crate) workspace_id: WorkspaceId,
}

#[derive(Clone)]
pub(crate) struct ProvisioningContextResolver {
    chat: ChatIdentityStore,
    pool: PgPool,
}

impl ProvisioningContextResolver {
    pub(crate) fn new(pool: PgPool) -> Self {
        Self {
            chat: ChatIdentityStore::new(pool.clone()),
            pool,
        }
    }

    pub(crate) async fn resolve(
        &self,
        authenticated: &AuthenticatedTestPrincipal,
    ) -> Result<ProvisioningSubject, ProvisioningError> {
        if authenticated.user_id.is_nil()
            || authenticated.tenant_id.is_nil()
            || authenticated.workspace_id.is_nil()
        {
            return Err(ProvisioningError::InvalidSubject);
        }
        let enabled: Option<bool> = sqlx::query_scalar(
            "SELECT disabled_at IS NULL AND tenant_id=$2 FROM users WHERE id=$1",
        )
        .bind(authenticated.user_id)
        .bind(authenticated.tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ProvisioningError::DatabaseUnavailable)?;
        if enabled != Some(true) {
            return Err(ProvisioningError::InvalidSubject);
        }
        let entitled = self
            .chat
            .chat_access(
                TenantId(authenticated.tenant_id),
                WorkspaceId(authenticated.workspace_id),
                PrincipalId(authenticated.user_id),
            )
            .await
            .map_err(|_| ProvisioningError::DatabaseUnavailable)?;
        if !entitled {
            return Err(ProvisioningError::InvalidSubject);
        }
        Ok(ProvisioningSubject {
            tenant_id: TenantId(authenticated.tenant_id),
            principal_id: PrincipalId(authenticated.user_id),
            workspace_id: WorkspaceId(authenticated.workspace_id),
        })
    }
}

#[derive(Clone)]
pub(crate) struct ManagedIdentityProvisioner {
    vault: ManagedIdentityVaultStore,
    operations: ManagedIdentityOperationStore,
    mappings: UpstreamRelayMappingStore,
    resolver: ProvisioningContextResolver,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IdentityOutcome {
    pub(crate) identity_id: Uuid,
    pub(crate) generation: i64,
    pub(crate) public_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DriveOutcome {
    Ready(IdentityOutcome),
    NotReady { status: String },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ReconcileSummary {
    pub(crate) attempted: usize,
    pub(crate) advanced: usize,
    pub(crate) failed: usize,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum ProvisioningError {
    #[error("provisioning subject is not authorized")]
    InvalidSubject,
    #[error("signed relay mapping is unavailable")]
    MappingUnavailable,
    #[error("signed relay mapping generation changed")]
    StaleMappingGeneration,
    #[error("test key provider is unavailable")]
    KmsUnavailable,
    #[error("managed identity conflicts with an existing identity")]
    IdentityConflict,
    #[error("trusted relay transport is unavailable")]
    TransportUnavailable,
    #[error("relay admission failed")]
    AdmissionRejected,
    #[error("managed identity authentication was rejected")]
    AuthenticationRejected,
    #[error("relay membership could not be proven")]
    MembershipUnproven,
    #[error("provisioning database is unavailable")]
    DatabaseUnavailable,
    #[error("provisioning attempts are exhausted")]
    AttemptsExhausted,
    #[error("provisioning state is internally inconsistent")]
    Internal,
}

impl ProvisioningError {
    fn code(self) -> &'static str {
        match self {
            Self::InvalidSubject => "invalid_subject",
            Self::MappingUnavailable => "mapping_unavailable",
            Self::StaleMappingGeneration => "stale_mapping_generation",
            Self::KmsUnavailable => "kms_unavailable",
            Self::IdentityConflict => "identity_conflict",
            Self::TransportUnavailable => "transport_unavailable",
            Self::AdmissionRejected => "admission_rejected",
            Self::AuthenticationRejected => "authentication_rejected",
            Self::MembershipUnproven => "membership_unproven",
            Self::DatabaseUnavailable => "database_unavailable",
            Self::AttemptsExhausted => "attempts_exhausted",
            Self::Internal => "internal_error",
        }
    }
}

impl From<ManagedIdentityOperationError> for ProvisioningError {
    fn from(error: ManagedIdentityOperationError) -> Self {
        match error {
            ManagedIdentityOperationError::Database => Self::DatabaseUnavailable,
            ManagedIdentityOperationError::Conflict => Self::IdentityConflict,
            ManagedIdentityOperationError::NotFound
            | ManagedIdentityOperationError::InvalidTransition
            | ManagedIdentityOperationError::InvalidRequest => Self::Internal,
        }
    }
}

impl From<UpstreamBuzzTransportError> for ProvisioningError {
    fn from(error: UpstreamBuzzTransportError) -> Self {
        match error {
            UpstreamBuzzTransportError::MappingStore(_) => Self::MappingUnavailable,
            UpstreamBuzzTransportError::Mapping(_) => Self::MappingUnavailable,
            UpstreamBuzzTransportError::Connection | UpstreamBuzzTransportError::SessionFenced => {
                Self::TransportUnavailable
            }
        }
    }
}

impl From<ManagedIdentityStoreError> for ProvisioningError {
    fn from(error: ManagedIdentityStoreError) -> Self {
        match error {
            ManagedIdentityStoreError::Conflict => Self::IdentityConflict,
            ManagedIdentityStoreError::Database
            | ManagedIdentityStoreError::NotFound
            | ManagedIdentityStoreError::InvalidEnvelope => Self::DatabaseUnavailable,
        }
    }
}

impl ManagedIdentityProvisioner {
    pub(crate) fn new(pool: PgPool, mappings: UpstreamRelayMappingStore) -> Self {
        Self {
            vault: ManagedIdentityVaultStore::new(pool.clone()),
            operations: ManagedIdentityOperationStore::new(pool.clone()),
            mappings,
            resolver: ProvisioningContextResolver::new(pool),
        }
    }

    pub(crate) fn operations(&self) -> &ManagedIdentityOperationStore {
        &self.operations
    }

    pub(crate) async fn authorize(
        &self,
        authenticated: AuthenticatedTestPrincipal,
        idempotency_key: Uuid,
    ) -> Result<ManagedIdentityOperationRecord, ProvisioningError> {
        if idempotency_key.is_nil() {
            return Err(ProvisioningError::InvalidSubject);
        }
        let subject = self.resolver.resolve(&authenticated).await?;
        let mapping = self
            .mappings
            .load(subject.tenant_id, subject.workspace_id)
            .await
            .map_err(|_| ProvisioningError::MappingUnavailable)?;
        let record = self
            .operations
            .authorize(rustshare_storage::ProvisioningRequest {
                tenant_id: subject.tenant_id.0,
                principal_id: subject.principal_id.0,
                workspace_id: subject.workspace_id.0,
                mapping_id: mapping.record().mapping_id,
                mapping_generation: mapping.record().generation as i64,
                idempotency_key,
            })
            .await?;
        Ok(record)
    }

    /// Reuse an existing active identity or create one exactly once at the
    /// operation's durable identity generation.
    pub(crate) async fn ensure_identity<K: DekKeyManagement>(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
        kms: &K,
    ) -> Result<IdentityOutcome, ProvisioningError> {
        let operation = self.operations.load(tenant_id, operation_id).await?;
        if operation.attempt_count >= operation.max_attempts {
            return Err(self
                .fail(
                    tenant_id,
                    operation_id,
                    ProvisioningError::AttemptsExhausted,
                )
                .await);
        }
        if operation.status == ManagedIdentityOperationStatus::Ready.as_str() {
            return self.active_identity_for_operation(&operation).await;
        }
        if operation.status == ManagedIdentityOperationStatus::Abandoned.as_str() {
            return Err(ProvisioningError::Internal);
        }
        let mapping = match self
            .mappings
            .load(
                TenantId(operation.tenant_id),
                WorkspaceId(operation.workspace_id),
            )
            .await
        {
            Ok(mapping) => mapping,
            Err(_) => {
                return Err(self
                    .fail(
                        tenant_id,
                        operation_id,
                        ProvisioningError::MappingUnavailable,
                    )
                    .await)
            }
        };
        if mapping.record().mapping_id != operation.mapping_id
            || mapping.record().generation as i64 != operation.mapping_generation
        {
            return Err(self
                .fail(
                    tenant_id,
                    operation_id,
                    ProvisioningError::StaleMappingGeneration,
                )
                .await);
        }

        let active = match self
            .vault
            .load_active_by_principal(tenant_id, operation.principal_id)
            .await
        {
            Ok(active) => active,
            Err(_) => {
                return Err(self
                    .fail(
                        tenant_id,
                        operation_id,
                        ProvisioningError::DatabaseUnavailable,
                    )
                    .await)
            }
        };

        if let Some(active) = active {
            if operation.status == ManagedIdentityOperationStatus::Provisioning.as_str() {
                if active.identity_id != operation.identity_id
                    || active.generation != operation.identity_generation
                {
                    return Err(self
                        .fail(tenant_id, operation_id, ProvisioningError::IdentityConflict)
                        .await);
                }
                return Ok(IdentityOutcome {
                    identity_id: active.identity_id,
                    generation: active.generation,
                    public_key: active.public_key,
                });
            }
            if self
                .operations
                .attach_identity(
                    tenant_id,
                    operation_id,
                    active.identity_id,
                    active.generation,
                )
                .await
                .is_err()
                && !self
                    .identity_is_attached(
                        tenant_id,
                        operation_id,
                        active.identity_id,
                        active.generation,
                    )
                    .await
            {
                return Err(self
                    .fail(
                        tenant_id,
                        operation_id,
                        ProvisioningError::DatabaseUnavailable,
                    )
                    .await);
            }
            return Ok(IdentityOutcome {
                identity_id: active.identity_id,
                generation: active.generation,
                public_key: active.public_key,
            });
        }

        if operation.status == ManagedIdentityOperationStatus::Provisioning.as_str() {
            return Err(self
                .fail(tenant_id, operation_id, ProvisioningError::IdentityConflict)
                .await);
        }

        let envelope = match ManagedIdentityEnvelope::generate(
            IdentityContext {
                tenant_id: operation.tenant_id,
                principal_id: operation.principal_id,
                identity_id: operation.identity_id,
                generation: operation.identity_generation,
            },
            kms,
        )
        .await
        {
            Ok(envelope) => envelope,
            Err(_) => {
                return Err(self
                    .fail(tenant_id, operation_id, ProvisioningError::KmsUnavailable)
                    .await)
            }
        };
        let stored = match self.vault.insert_or_load(operation_id, &envelope).await {
            Ok(stored) => stored,
            Err(ManagedIdentityStoreError::Conflict) => {
                return Err(self
                    .fail(tenant_id, operation_id, ProvisioningError::IdentityConflict)
                    .await)
            }
            Err(_) => {
                return Err(self
                    .fail(
                        tenant_id,
                        operation_id,
                        ProvisioningError::DatabaseUnavailable,
                    )
                    .await)
            }
        };
        if self
            .operations
            .attach_identity(
                tenant_id,
                operation_id,
                stored.identity_id,
                stored.generation,
            )
            .await
            .is_err()
            && !self
                .identity_is_attached(
                    tenant_id,
                    operation_id,
                    stored.identity_id,
                    stored.generation,
                )
                .await
        {
            return Err(self
                .fail(
                    tenant_id,
                    operation_id,
                    ProvisioningError::DatabaseUnavailable,
                )
                .await);
        }
        Ok(IdentityOutcome {
            identity_id: stored.identity_id,
            generation: stored.generation,
            public_key: stored.public_key,
        })
    }

    /// Resume retryable operations sequentially. It never invents an
    /// operation and is called only by tests in this phase.
    pub(crate) async fn reconcile<K: DekKeyManagement>(
        &self,
        tenant_id: Uuid,
        limit: i64,
        kms: &K,
    ) -> Result<ReconcileSummary, ProvisioningError> {
        let operations = self.operations.list_retryable(tenant_id, limit).await?;
        let mut summary = ReconcileSummary::default();
        for operation in operations {
            summary.attempted += 1;
            match self
                .ensure_identity(tenant_id, operation.operation_id, kms)
                .await
            {
                Ok(_) => summary.advanced += 1,
                Err(_) => summary.failed += 1,
            }
        }
        Ok(summary)
    }

    /// Drive identity storage, NIP-43 admission, fresh NIP-42 authentication,
    /// and one member-only NIP-29 read before marking the operation ready.
    pub(crate) async fn drive<K: DekKeyManagement>(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
        kms: &K,
        relay_admin: &Keys,
        trust: &RelayTlsTrustConfig,
        membership_channel: &str,
    ) -> Result<DriveOutcome, ProvisioningError> {
        let identity = self.ensure_identity(tenant_id, operation_id, kms).await?;
        let operation = self.operations.load(tenant_id, operation_id).await?;
        if operation.status == ManagedIdentityOperationStatus::Ready.as_str() {
            return Ok(DriveOutcome::Ready(identity));
        }
        let mapping = match self
            .mappings
            .load(
                TenantId(operation.tenant_id),
                WorkspaceId(operation.workspace_id),
            )
            .await
        {
            Ok(mapping) => mapping,
            Err(_) => {
                return Err(self
                    .fail(
                        tenant_id,
                        operation_id,
                        ProvisioningError::MappingUnavailable,
                    )
                    .await)
            }
        };
        let record = match self
            .vault
            .load_active_by_principal(tenant_id, operation.principal_id)
            .await
        {
            Ok(Some(record)) => record,
            _ => {
                return Err(self
                    .fail(tenant_id, operation_id, ProvisioningError::IdentityConflict)
                    .await)
            }
        };
        let envelope = match envelope_from_record(&record) {
            Ok(envelope) => envelope,
            Err(error) => return Err(self.fail(tenant_id, operation_id, error).await),
        };
        let recovered = match envelope
            .recover(
                IdentityContext {
                    tenant_id: record.tenant_id,
                    principal_id: record.principal_id,
                    identity_id: record.identity_id,
                    generation: record.generation,
                },
                kms,
            )
            .await
        {
            Ok(recovered) => recovered,
            Err(_) => {
                return Err(self
                    .fail(tenant_id, operation_id, ProvisioningError::KmsUnavailable)
                    .await)
            }
        };
        let mut session = match connect(
            &self.mappings,
            TenantId(operation.tenant_id),
            WorkspaceId(operation.workspace_id),
            trust,
        )
        .await
        {
            Ok(session) => session,
            Err(_) => {
                return Err(self
                    .fail(
                        tenant_id,
                        operation_id,
                        ProvisioningError::TransportUnavailable,
                    )
                    .await)
            }
        };

        if operation.admission_event_id.is_none() {
            if let Err(error) = self
                .admit(&mut session, &operation, &identity, relay_admin)
                .await
            {
                return Err(self.fail(tenant_id, operation_id, error).await);
            }
        }
        if let Err(error) = prove_membership(
            &mut session,
            &recovered,
            mapping.relay_url(),
            membership_channel,
        )
        .await
        {
            return Err(self.fail(tenant_id, operation_id, error).await);
        }
        if self
            .operations
            .mark_ready(tenant_id, operation_id)
            .await
            .is_err()
        {
            return Err(self
                .fail(
                    tenant_id,
                    operation_id,
                    ProvisioningError::DatabaseUnavailable,
                )
                .await);
        }
        Ok(DriveOutcome::Ready(identity))
    }

    async fn admit(
        &self,
        session: &mut TenantRelaySession,
        operation: &ManagedIdentityOperationRecord,
        identity: &IdentityOutcome,
        relay_admin: &Keys,
    ) -> Result<(), ProvisioningError> {
        let event = build_buzz_provisioning_admission_event_at(
            &identity.public_key,
            relay_admin,
            Timestamp::from(operation.created_at.timestamp() as u64),
        )
        .map_err(|_| ProvisioningError::AdmissionRejected)?;
        let event_id = event.id.to_hex();
        session
            .send_text(&serde_json::json!(["EVENT", event]).to_string())
            .await
            .map_err(|_| ProvisioningError::TransportUnavailable)?;
        if !await_ok(session, &event_id).await? {
            return Err(ProvisioningError::AdmissionRejected);
        }
        self.operations
            .record_admission_sent(operation.tenant_id, operation.operation_id, &event_id)
            .await
            .map_err(|_| ProvisioningError::DatabaseUnavailable)?;
        Ok(())
    }

    async fn identity_is_attached(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
        identity_id: Uuid,
        generation: i64,
    ) -> bool {
        matches!(
            self.operations.load(tenant_id, operation_id).await,
            Ok(operation)
                if operation.status == ManagedIdentityOperationStatus::Provisioning.as_str()
                    && operation.identity_id == identity_id
                    && operation.identity_generation == generation
        )
    }

    async fn active_identity_for_operation(
        &self,
        operation: &ManagedIdentityOperationRecord,
    ) -> Result<IdentityOutcome, ProvisioningError> {
        let active = self
            .vault
            .load_active_by_principal(operation.tenant_id, operation.principal_id)
            .await
            .map_err(|_| ProvisioningError::DatabaseUnavailable)?
            .ok_or(ProvisioningError::Internal)?;
        if active.identity_id != operation.identity_id
            || active.generation != operation.identity_generation
        {
            return Err(ProvisioningError::IdentityConflict);
        }
        Ok(IdentityOutcome {
            identity_id: active.identity_id,
            generation: active.generation,
            public_key: active.public_key,
        })
    }

    async fn fail(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
        error: ProvisioningError,
    ) -> ProvisioningError {
        let _ = self
            .operations
            .mark_failed(tenant_id, operation_id, error.code())
            .await;
        error
    }
}

fn envelope_from_record(
    record: &ManagedIdentityRecord,
) -> Result<ManagedIdentityEnvelope, ProvisioningError> {
    let nonce: [u8; 12] = record
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| ProvisioningError::IdentityConflict)?;
    Ok(ManagedIdentityEnvelope {
        context: IdentityContext {
            tenant_id: record.tenant_id,
            principal_id: record.principal_id,
            identity_id: record.identity_id,
            generation: record.generation,
        },
        public_key: record.public_key.clone(),
        version: u16::try_from(record.envelope_version)
            .map_err(|_| ProvisioningError::IdentityConflict)?,
        nonce,
        ciphertext: record.ciphertext.clone(),
        wrapped_dek: WrappedDek {
            key_id: record.kms_key_id.clone(),
            key_version: record.kms_key_version.clone(),
            ciphertext: record.wrapped_dek.clone(),
        },
    })
}

async fn receive_json(
    session: &mut TenantRelaySession,
) -> Result<Option<Value>, ProvisioningError> {
    let frame = tokio::time::timeout(Duration::from_secs(15), session.receive())
        .await
        .map_err(|_| ProvisioningError::TransportUnavailable)??
        .ok_or(ProvisioningError::TransportUnavailable)?;
    let Message::Text(frame) = frame else {
        return Ok(None);
    };
    serde_json::from_str(&frame)
        .map(Some)
        .map_err(|_| ProvisioningError::TransportUnavailable)
}

async fn await_ok(
    session: &mut TenantRelaySession,
    event_id: &str,
) -> Result<bool, ProvisioningError> {
    loop {
        let Some(value) = receive_json(session).await? else {
            continue;
        };
        if value[0] == "OK" && value[1] == event_id {
            return Ok(value[2].as_bool().unwrap_or(false));
        }
        if value[0] == "CLOSED" {
            return Ok(false);
        }
    }
}

async fn prove_membership(
    session: &mut TenantRelaySession,
    recovered: &RecoveredIdentity,
    relay_url: &str,
    membership_channel: &str,
) -> Result<(), ProvisioningError> {
    if membership_channel.is_empty() {
        return Err(ProvisioningError::MembershipUnproven);
    }
    let challenge = loop {
        let Some(value) = receive_json(session).await? else {
            continue;
        };
        if value[0] == "AUTH" {
            break value[1]
                .as_str()
                .ok_or(ProvisioningError::AuthenticationRejected)?
                .to_owned();
        }
        if value[0] == "CLOSED" {
            return Err(ProvisioningError::AuthenticationRejected);
        }
    };
    let event = recovered
        .sign_nip42_authentication(
            &challenge,
            relay_url,
            Timestamp::from(Utc::now().timestamp() as u64),
        )
        .map_err(|_| ProvisioningError::AuthenticationRejected)?;
    let event_id = event.id.to_hex();
    session
        .send_text(&serde_json::json!(["AUTH", event]).to_string())
        .await
        .map_err(|_| ProvisioningError::TransportUnavailable)?;
    if !await_ok(session, &event_id).await? {
        return Err(ProvisioningError::AuthenticationRejected);
    }

    let subscription = format!("provision-{}", Uuid::new_v4());
    session
        .send_text(
            &serde_json::json!([
                "REQ",
                subscription,
                {"kinds": [9], "#h": [membership_channel], "limit": 50}
            ])
            .to_string(),
        )
        .await
        .map_err(|_| ProvisioningError::TransportUnavailable)?;
    let mut saw_event = false;
    loop {
        let Some(value) = receive_json(session).await? else {
            continue;
        };
        if value[0] == "EVENT" && value[1] == subscription {
            saw_event = true;
        } else if value[0] == "EOSE" && value[1] == subscription {
            break;
        } else if value[0] == "CLOSED" && value[1] == subscription {
            return Err(ProvisioningError::MembershipUnproven);
        }
    }
    if !saw_event {
        return Err(ProvisioningError::MembershipUnproven);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::{Duration, TimeZone, Timelike, Utc};
    use ed25519_dalek::{Signer, SigningKey};
    use nostr::Keys;
    use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
    use rustshare_crypto::managed_identity_vault::{
        DekKeyManagement, IdentityContext, KeyEncryptionContext, ManagedIdentityEnvelope,
        VaultError, WrappedDek,
    };
    use rustshare_resource_auth::upstream_relay_mapping::{
        RelayEndpointPolicy, RelayMappingRecord,
    };
    use rustshare_resource_auth::upstream_relay_provisioning::{
        MappingProvisioningAction, MappingProvisioningStatement, MappingProvisioningVerifier,
    };
    use rustshare_storage::{ManagedIdentityVaultStore, UpstreamRelayMappingStore};
    use sqlx::PgPool;
    use tokio::sync::Barrier;
    use tokio_rustls::rustls::pki_types::CertificateDer;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    use super::{
        AuthenticatedTestPrincipal, DriveOutcome, ManagedIdentityProvisioner,
        ProvisioningContextResolver, ProvisioningError,
    };
    use crate::upstream_buzz_transport::RelayTlsTrustConfig;

    fn required(name: &str) -> String {
        std::env::var(name)
            .unwrap_or_else(|_| panic!("{name} must be set for the disposable relay test"))
    }

    #[derive(Clone)]
    struct TestKms {
        unavailable: Arc<std::sync::atomic::AtomicBool>,
    }

    impl TestKms {
        fn new() -> Self {
            Self {
                unavailable: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }

        fn set_unavailable(&self, value: bool) {
            self.unavailable
                .store(value, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[async_trait::async_trait]
    impl DekKeyManagement for TestKms {
        async fn wrap_dek(
            &self,
            dek: &[u8; 32],
            _context: &KeyEncryptionContext,
        ) -> Result<WrappedDek, VaultError> {
            if self.unavailable.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(VaultError::KeyManagement);
            }
            Ok(WrappedDek {
                key_id: "test-kek".into(),
                key_version: "v1".into(),
                ciphertext: dek.to_vec(),
            })
        }

        async fn unwrap_dek(
            &self,
            wrapped: &WrappedDek,
            _context: &KeyEncryptionContext,
        ) -> Result<Zeroizing<[u8; 32]>, VaultError> {
            if self.unavailable.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(VaultError::KeyManagement);
            }
            let bytes: [u8; 32] = wrapped
                .ciphertext
                .as_slice()
                .try_into()
                .map_err(|_| VaultError::Recovery)?;
            Ok(Zeroizing::new(bytes))
        }
    }

    fn mapping_record(tenant: Uuid, workspace: Uuid) -> RelayMappingRecord {
        RelayMappingRecord {
            mapping_id: Uuid::new_v4(),
            tenant_id: TenantId(tenant),
            workspace_id: WorkspaceId(workspace),
            community_id: Uuid::new_v4().to_string(),
            endpoint_policy: RelayEndpointPolicy::StrictPublic,
            relay_deployment_id: Uuid::new_v4(),
            endpoint_service_identity: format!(
                "k8s://{}/test-ns/buzz/{}",
                Uuid::new_v4(),
                Uuid::new_v4()
            ),
            tls_peer_spki_sha256: format!("sha256:{}", "b2".repeat(32)),
            relay_url: format!("wss://relay-{tenant}.example.test"),
            relay_host: format!("relay-{tenant}.example.test"),
            relay_pubkey: nostr::Keys::generate().public_key().to_hex(),
            deployment_digest: format!("sha256:{}", "a1".repeat(32)),
            approved_ips: vec!["8.8.8.8".parse().unwrap()],
            generation: 1,
            active: true,
            valid_until: Utc::now() + Duration::days(1),
            updated_by: Uuid::new_v4(),
            audit_entry_id: Uuid::new_v4(),
        }
    }

    async fn mapping_store(
        pool: &PgPool,
        tenant: Uuid,
        workspace: Uuid,
    ) -> UpstreamRelayMappingStore {
        let signing_key = SigningKey::from_bytes(&[21; 32]);
        let operator_service_id = Uuid::new_v4();
        let now = Utc::now();
        let now = now
            .with_nanosecond(now.timestamp_subsec_micros() * 1_000)
            .unwrap();
        let mut record = mapping_record(tenant, workspace);
        record.updated_by = operator_service_id;
        record.valid_until = now + Duration::days(1);
        let mut statement = MappingProvisioningStatement {
            operator_service_id,
            operator_key_id: "deployment-test-key-v1".into(),
            operation_id: Uuid::new_v4(),
            change_reference_id: Uuid::new_v4(),
            action: MappingProvisioningAction::Create,
            expected_generation: 0,
            issued_at: now,
            expires_at: now + Duration::minutes(2),
            record,
            signature: Vec::new(),
        };
        statement.signature = signing_key
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        let verifier = Arc::new(
            MappingProvisioningVerifier::new(
                operator_service_id,
                HashMap::from([(
                    statement.operator_key_id.clone(),
                    signing_key.verifying_key(),
                )]),
            )
            .unwrap(),
        );
        let store = UpstreamRelayMappingStore::with_provisioning_verifier(pool.clone(), verifier);
        store.apply(&statement).await.unwrap();
        store
    }

    async fn tenant_user_workspace(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
        let tenant = Uuid::new_v4();
        let user = Uuid::new_v4();
        let workspace = Uuid::new_v4();
        sqlx::query("INSERT INTO tenants (id,name) VALUES ($1,$2)")
            .bind(tenant)
            .bind(format!("Provisioning test {tenant}"))
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO users (id,username,email,password_hash,display_name,storage_quota,tenant_id) \
             VALUES ($1,$2,$3,'test-only',$2,1024,$4)",
        )
        .bind(user)
        .bind(format!("provisioning-{user}"))
        .bind(format!("{user}@example.test"))
        .bind(tenant)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO application_enablements (tenant_id,workspace_id,application_id,enabled) \
             VALUES ($1,$2,'io.elembra.chat',true)",
        )
        .bind(tenant)
        .bind(workspace)
        .execute(pool)
        .await
        .unwrap();
        (tenant, user, workspace)
    }

    async fn pool() -> PgPool {
        let pool = PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be disposable PostgreSQL"),
        )
        .await
        .unwrap();
        sqlx::migrate!("../migrations").run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn context_resolver_accepts_authenticated_entitled_principal() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let resolver = ProvisioningContextResolver::new(pool.clone());
        let authenticated = AuthenticatedTestPrincipal::new(user, tenant, workspace);
        let subject = resolver.resolve(&authenticated).await.unwrap();
        assert_eq!(subject.tenant_id, TenantId(tenant));
        assert_eq!(subject.principal_id, PrincipalId(user));
        assert_eq!(subject.workspace_id, WorkspaceId(workspace));
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn wrong_tenant_or_unentitled_context_is_rejected_without_operation() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), mapping);
        let wrong_tenant = AuthenticatedTestPrincipal::new(user, Uuid::new_v4(), workspace);
        assert_eq!(
            provisioner
                .authorize(wrong_tenant, Uuid::new_v4())
                .await
                .unwrap_err(),
            ProvisioningError::InvalidSubject
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM managed_chat_identity_operations WHERE tenant_id=$1",
        )
        .bind(tenant)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
        sqlx::query("UPDATE users SET disabled_at=now() WHERE id=$1")
            .bind(user)
            .execute(&pool)
            .await
            .unwrap();
        let disabled = AuthenticatedTestPrincipal::new(user, tenant, workspace);
        assert_eq!(
            provisioner
                .authorize(disabled, Uuid::new_v4())
                .await
                .unwrap_err(),
            ProvisioningError::InvalidSubject
        );
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn authorize_binds_signed_mapping_and_retry_preserves_identity() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let expected = mapping
            .load(TenantId(tenant), WorkspaceId(workspace))
            .await
            .unwrap()
            .record()
            .clone();
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), mapping);
        let authenticated = AuthenticatedTestPrincipal::new(user, tenant, workspace);
        let key = Uuid::new_v4();
        let operation = provisioner
            .authorize(authenticated.clone(), key)
            .await
            .unwrap();
        assert_eq!(operation.mapping_id, expected.mapping_id);
        assert_eq!(operation.mapping_generation, expected.generation as i64);
        let kms = TestKms::new();
        let first = provisioner
            .ensure_identity(tenant, operation.operation_id, &kms)
            .await
            .unwrap();
        let second = provisioner
            .ensure_identity(tenant, operation.operation_id, &kms)
            .await
            .unwrap();
        assert_eq!(first.public_key, second.public_key);
        assert_eq!(first.identity_id, second.identity_id);
        assert_eq!(first.generation, second.generation);
        let stored = provisioner
            .operations()
            .load(tenant, operation.operation_id)
            .await
            .unwrap();
        assert_eq!(stored.identity_id, first.identity_id);
        assert_eq!(stored.identity_generation, first.generation);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn concurrent_ensure_identity_yields_one_active_identity() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let provisioner = Arc::new(ManagedIdentityProvisioner::new(pool.clone(), mapping));
        let authenticated = AuthenticatedTestPrincipal::new(user, tenant, workspace);
        let operation = provisioner
            .authorize(authenticated, Uuid::new_v4())
            .await
            .unwrap();
        let kms = TestKms::new();
        let barrier = Arc::new(Barrier::new(8));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let provisioner = provisioner.clone();
            let kms = kms.clone();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                provisioner
                    .ensure_identity(tenant, operation.operation_id, &kms)
                    .await
                    .unwrap()
            }));
        }
        let mut outcomes = Vec::new();
        for task in tasks {
            outcomes.push(task.await.unwrap());
        }
        let first = &outcomes[0];
        assert!(outcomes
            .iter()
            .all(|outcome| outcome.public_key == first.public_key));
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM managed_chat_identities WHERE tenant_id=$1 AND principal_id=$2 AND status='active'",
        )
        .bind(tenant)
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn kms_outage_fails_closed_then_reconcile_recovers() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), mapping);
        let operation = provisioner
            .authorize(
                AuthenticatedTestPrincipal::new(user, tenant, workspace),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        let kms = TestKms::new();
        kms.set_unavailable(true);
        assert_eq!(
            provisioner
                .ensure_identity(tenant, operation.operation_id, &kms)
                .await
                .unwrap_err(),
            ProvisioningError::KmsUnavailable
        );
        let failed = provisioner
            .operations()
            .load(tenant, operation.operation_id)
            .await
            .unwrap();
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.error_code.as_deref(), Some("kms_unavailable"));
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM managed_chat_identities WHERE tenant_id=$1 AND principal_id=$2",
        )
        .bind(tenant)
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);

        kms.set_unavailable(false);
        let summary = provisioner.reconcile(tenant, 10, &kms).await.unwrap();
        assert_eq!(summary.attempted, 1);
        assert_eq!(summary.advanced, 1);
        let outcome = provisioner
            .ensure_identity(tenant, operation.operation_id, &kms)
            .await
            .unwrap();
        assert_eq!(outcome.identity_id, operation.identity_id);
        assert_eq!(outcome.generation, operation.identity_generation);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn crash_after_encrypted_storage_reuses_the_stored_identity() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), mapping);
        let operation = provisioner
            .authorize(
                AuthenticatedTestPrincipal::new(user, tenant, workspace),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        let kms = TestKms::new();
        let envelope = ManagedIdentityEnvelope::generate(
            IdentityContext {
                tenant_id: operation.tenant_id,
                principal_id: operation.principal_id,
                identity_id: operation.identity_id,
                generation: operation.identity_generation,
            },
            &kms,
        )
        .await
        .unwrap();
        let stored = ManagedIdentityVaultStore::new(pool.clone())
            .insert_or_load(operation.operation_id, &envelope)
            .await
            .unwrap();
        let outcome = provisioner
            .ensure_identity(tenant, operation.operation_id, &kms)
            .await
            .unwrap();
        assert_eq!(outcome.identity_id, stored.identity_id);
        assert_eq!(outcome.public_key, stored.public_key);
        assert_eq!(outcome.generation, stored.generation);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn database_outage_is_retryable_and_unauthorized_then_recovers() {
        let database_url = std::env::var("DATABASE_URL").unwrap();
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), mapping);
        let operation = provisioner
            .authorize(
                AuthenticatedTestPrincipal::new(user, tenant, workspace),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        let kms = TestKms::new();
        pool.close().await;
        assert_eq!(
            provisioner
                .ensure_identity(tenant, operation.operation_id, &kms)
                .await
                .unwrap_err(),
            ProvisioningError::DatabaseUnavailable
        );

        let restored = PgPool::connect(&database_url).await.unwrap();
        let recovered = ManagedIdentityProvisioner::new(
            restored.clone(),
            UpstreamRelayMappingStore::new(restored.clone()),
        );
        let summary = recovered.reconcile(tenant, 10, &kms).await.unwrap();
        assert_eq!(summary.attempted, 1);
        assert_eq!(summary.advanced, 1);
        let status: String = sqlx::query_scalar(
            "SELECT status FROM managed_chat_identity_operations WHERE tenant_id=$1 AND operation_id=$2",
        )
        .bind(tenant)
        .bind(operation.operation_id)
        .fetch_one(&restored)
        .await
        .unwrap();
        assert_ne!(status, "ready");
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn attempt_exhaustion_blocks_reauthorize() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), mapping);
        let operation = provisioner
            .authorize(
                AuthenticatedTestPrincipal::new(user, tenant, workspace),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        sqlx::query(
            "UPDATE managed_chat_identity_operations SET max_attempts=1 \
             WHERE tenant_id=$1 AND operation_id=$2",
        )
        .bind(tenant)
        .bind(operation.operation_id)
        .execute(&pool)
        .await
        .unwrap();
        let kms = TestKms::new();
        kms.set_unavailable(true);
        assert_eq!(
            provisioner
                .ensure_identity(tenant, operation.operation_id, &kms)
                .await
                .unwrap_err(),
            ProvisioningError::KmsUnavailable
        );
        assert_eq!(
            provisioner
                .ensure_identity(tenant, operation.operation_id, &kms)
                .await
                .unwrap_err(),
            ProvisioningError::AttemptsExhausted
        );
        assert_eq!(
            provisioner
                .authorize(
                    AuthenticatedTestPrincipal::new(user, tenant, workspace),
                    Uuid::new_v4(),
                )
                .await
                .unwrap_err(),
            ProvisioningError::IdentityConflict
        );
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL, relay-admin key, and private Buzz service"]
    async fn live_provisioning_reaches_ready_after_nip42_and_member_only_read() {
        assert_eq!(required("BUZZ_TEST_DISPOSABLE_CONFIRM"), "DISPOSABLE_ONLY");
        let database_url =
            crate::test_guard::disposable_database_url_from_env().unwrap_or_else(|message| {
                panic!("provisioning test requires disposable PostgreSQL: {message}")
            });
        let relay_host = required("BUZZ_TEST_RELAY_HOST");
        let relay_url = required("BUZZ_TEST_RELAY_URL");
        let community_id = required("BUZZ_TEST_COMMUNITY_ID");
        let relay_pubkey = required("BUZZ_TEST_RELAY_PUBKEY");
        let deployment_digest = required("BUZZ_TEST_DEPLOYMENT_DIGEST");
        let service_identity = required("BUZZ_TEST_SERVICE_IDENTITY");
        let spki_pin = required("BUZZ_TEST_TLS_SPKI_PIN");
        let approved_ip = required("BUZZ_TEST_APPROVED_IP").parse().unwrap();
        let root_der = std::fs::read(required("BUZZ_TEST_TLS_ROOT_DER")).unwrap();
        let membership_channel = required("BUZZ_TEST_DM_CHANNEL_ID");
        let relay_admin = Keys::parse(&required("BUZZ_TEST_RELAY_ADMIN_NSEC")).unwrap();

        let pool = PgPool::connect(&database_url).await.unwrap();
        sqlx::migrate!("../migrations").run(&pool).await.unwrap();
        let tenant = Uuid::new_v4();
        let workspace = Uuid::new_v4();
        let user = Uuid::new_v4();
        sqlx::query("INSERT INTO tenants (id,name) VALUES ($1,$2)")
            .bind(tenant)
            .bind(format!("disposable provisioning {tenant}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO users (id,username,email,password_hash,display_name,storage_quota,tenant_id) \
             VALUES ($1,$2,$3,'test-only',$2,1024,$4)",
        )
        .bind(user)
        .bind(format!("provisioning-live-{user}"))
        .bind(format!("{user}@example.test"))
        .bind(tenant)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO application_enablements (tenant_id,workspace_id,application_id,enabled) \
             VALUES ($1,$2,'io.elembra.chat',true)",
        )
        .bind(tenant)
        .bind(workspace)
        .execute(&pool)
        .await
        .unwrap();

        let operator_service_id = Uuid::new_v4();
        let operator_key_id = "disposable-provisioning-authority-v1".to_owned();
        let operator_key = SigningKey::from_bytes(&rand::random());
        let now = Utc::now();
        let now = Utc.timestamp_micros(now.timestamp_micros()).unwrap();
        let mut statement = MappingProvisioningStatement {
            operator_service_id,
            operator_key_id: operator_key_id.clone(),
            operation_id: Uuid::new_v4(),
            change_reference_id: Uuid::new_v4(),
            action: MappingProvisioningAction::Create,
            expected_generation: 0,
            issued_at: now,
            expires_at: now + Duration::minutes(2),
            record: RelayMappingRecord {
                mapping_id: Uuid::new_v4(),
                tenant_id: TenantId(tenant),
                workspace_id: WorkspaceId(workspace),
                community_id,
                endpoint_policy: RelayEndpointPolicy::TrustedService,
                relay_deployment_id: Uuid::new_v4(),
                endpoint_service_identity: service_identity,
                tls_peer_spki_sha256: spki_pin,
                relay_url,
                relay_host,
                relay_pubkey,
                deployment_digest,
                approved_ips: vec![approved_ip],
                generation: 1,
                active: true,
                valid_until: now + Duration::hours(1),
                updated_by: operator_service_id,
                audit_entry_id: Uuid::new_v4(),
            },
            signature: Vec::new(),
        };
        statement.signature = operator_key
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        let verifier = Arc::new(
            MappingProvisioningVerifier::new(
                operator_service_id,
                HashMap::from([(operator_key_id, operator_key.verifying_key())]),
            )
            .unwrap(),
        );
        let store = UpstreamRelayMappingStore::with_provisioning_verifier(pool.clone(), verifier);
        store.apply(&statement).await.unwrap();
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), store);
        let operation = provisioner
            .authorize(
                AuthenticatedTestPrincipal::new(user, tenant, workspace),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        let trust = RelayTlsTrustConfig::TrustedService {
            endpoint_service_identity: statement.record.endpoint_service_identity.clone(),
            roots: vec![CertificateDer::from(root_der)],
        };
        let outcome = provisioner
            .drive(
                tenant,
                operation.operation_id,
                &TestKms::new(),
                &relay_admin,
                &trust,
                &membership_channel,
            )
            .await
            .unwrap();
        let DriveOutcome::Ready(identity) = outcome else {
            panic!("provisioning did not reach ready")
        };
        assert_eq!(identity.public_key.len(), 64);
        assert_ne!(identity.public_key, relay_admin.public_key().to_hex());
        let ready = provisioner
            .operations()
            .load(tenant, operation.operation_id)
            .await
            .unwrap();
        assert_eq!(ready.status, "ready");
        assert!(ready.admission_event_id.is_some());
        assert!(ready.ready_at.is_some());
    }
}
