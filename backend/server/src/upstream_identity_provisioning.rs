//! Test-called managed identity provisioning lifecycle.
//!
//! No route, worker, startup hook, or production caller is wired. Tests supply
//! an authenticated disposable Principal context, signed mapping, test KMS,
//! and disposable relay-admin key explicitly.

use std::time::Duration;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::Utc;
use nostr::{Event, EventBuilder, Keys, Kind, RelayUrl, Tag, Timestamp};
use rustshare_core::domain::{PrincipalId, TenantId, WorkspaceId};
use rustshare_crypto::managed_identity_vault::{
    DekKeyManagement, IdentityContext, ManagedIdentityEnvelope, RecoveredIdentity, WrappedDek,
};
use rustshare_resource_auth::chat_identity::build_buzz_provisioning_admission_event_at;
use rustshare_storage::{
    ChatIdentityStore, ManagedIdentityOperationError, ManagedIdentityOperationRecord,
    ManagedIdentityOperationStatus, ManagedIdentityOperationStore, ManagedIdentityRecord,
    ManagedIdentityStatus, ManagedIdentityStoreError, ManagedIdentityVaultStore, MetadataStore,
    UpstreamRelayMappingStore, UserSecurityEventRecord,
};
use serde_json::Value;
use sqlx::PgPool;
use thiserror::Error;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

const HISTORY_SUBSCRIPTION_TIMEOUT: Duration = Duration::from_secs(30);

#[cfg(test)]
static LIVE_HISTORY_DATABASE_READS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
async fn wait_for_live_history_database_outage() {
    let Some(ack_file) = std::env::var_os("BUZZ_TEST_HISTORY_DB_OUTAGE_ACK_FILE") else {
        return;
    };
    if LIVE_HISTORY_DATABASE_READS.fetch_add(1, Ordering::SeqCst) != 0 {
        return;
    }
    println!("BUZZ_TEST_HISTORY_DB_OUTAGE_READY");
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
    let ack_file = std::path::PathBuf::from(ack_file);
    tokio::time::timeout(Duration::from_secs(120), async {
        while !ack_file.exists() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("history database outage acknowledgement timed out");
}

// shortcut: this foundation has no runtime caller, so a process-wide lock
// prevents overlapping test drives; replace it with a durable lease before
// wiring a multi-process worker.
static DRIVE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    #[cfg(test)]
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
            || authenticated.workspace_id != authenticated.tenant_id
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
    #[error("upstream channel history is not authorized")]
    ReadDenied,
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
            Self::ReadDenied => "read_denied",
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
            ManagedIdentityStoreError::OperationNotWritable => Self::Internal,
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

    /// One-shot history read using the managed identity and the relay's own
    /// NIP-42 authorization. Results are withheld unless the complete response
    /// reaches EOSE and the local subject, identity, operation and mapping are
    /// still current. This intentionally exposes no persistent subscription.
    pub(crate) async fn read_channel_history<K: DekKeyManagement>(
        &self,
        authenticated: &AuthenticatedTestPrincipal,
        channel_id: &str,
        kms: &K,
        trust: &RelayTlsTrustConfig,
    ) -> Result<Vec<Event>, ProvisioningError> {
        let result = self
            .read_channel_history_inner(authenticated, channel_id, kms, trust)
            .await;
        let channel = Uuid::parse_str(channel_id)
            .ok()
            .filter(|id| id.to_string() == channel_id)
            .map(|id| id.to_string())
            .unwrap_or_else(|| "invalid".to_owned());
        let description = match &result {
            Ok(history) => format!(
                "allowed tenant={} workspace={} channel={} operation={} identity={}:{} mapping={}:{} events={}",
                authenticated.tenant_id,
                authenticated.workspace_id,
                channel,
                history.operation_id,
                history.identity_id,
                history.identity_generation,
                history.mapping_id,
                history.mapping_generation,
                history.events.len()
            ),
            Err(error) => format!(
                "denied tenant={} workspace={} channel={} reason={}",
                authenticated.tenant_id,
                authenticated.workspace_id,
                channel,
                error.code()
            ),
        };
        MetadataStore::new(self.resolver.pool.clone())
            .create_user_security_event(UserSecurityEventRecord {
                user_id: authenticated.user_id,
                event_type: "managed_chat_history_access",
                description: &description,
                ip_address: None,
                user_agent: None,
                session_id: None,
            })
            .await
            .map_err(|_| ProvisioningError::DatabaseUnavailable)?;
        result.map(|history| history.events)
    }

    async fn read_channel_history_inner<K: DekKeyManagement>(
        &self,
        authenticated: &AuthenticatedTestPrincipal,
        channel_id: &str,
        kms: &K,
        trust: &RelayTlsTrustConfig,
    ) -> Result<AuthorizedHistoryRead, ProvisioningError> {
        const MAX_HISTORY_EVENTS: usize = 100;
        const MAX_EVENT_CONTENT_BYTES: usize = 64 * 1024;

        let channel = Uuid::parse_str(channel_id)
            .ok()
            .filter(|id| id.to_string() == channel_id)
            .ok_or(ProvisioningError::ReadDenied)?;
        let subject = self.resolver.resolve(authenticated).await?;
        let identity_record = self
            .vault
            .load_active_by_principal(subject.tenant_id.0, subject.principal_id.0)
            .await
            .map_err(|_| ProvisioningError::DatabaseUnavailable)?
            .ok_or(ProvisioningError::ReadDenied)?;
        let identity = IdentityOutcome {
            identity_id: identity_record.identity_id,
            generation: identity_record.generation,
            public_key: identity_record.public_key.clone(),
        };
        let operation = self
            .operations
            .ready_for_identity(
                subject.tenant_id.0,
                subject.principal_id.0,
                subject.workspace_id.0,
                identity.identity_id,
                identity.generation,
            )
            .await
            .map_err(|_| ProvisioningError::ReadDenied)?;
        if !identity_binding_matches(
            operation.identity_id,
            operation.identity_generation,
            &identity,
            &identity_record,
        ) {
            return Err(ProvisioningError::IdentityConflict);
        }
        let mapping = self
            .mappings
            .load(subject.tenant_id, subject.workspace_id)
            .await
            .map_err(|_| ProvisioningError::MappingUnavailable)?;
        let mapping_id = mapping.record().mapping_id;
        let mapping_generation = mapping.record().generation as i64;
        if !mapping_binding_matches(
            operation.mapping_id,
            operation.mapping_generation,
            mapping_id,
            mapping_generation,
        ) {
            return Err(ProvisioningError::StaleMappingGeneration);
        }
        let context = IdentityContext {
            tenant_id: subject.tenant_id.0,
            principal_id: subject.principal_id.0,
            identity_id: identity.identity_id,
            generation: identity.generation,
        };
        let envelope = identity_record
            .envelope()
            .map_err(|_| ProvisioningError::IdentityConflict)?;
        let recovered = envelope
            .recover(context, kms)
            .await
            .map_err(|_| ProvisioningError::KmsUnavailable)?;
        let mut session = connect(
            &self.mappings,
            subject.tenant_id,
            subject.workspace_id,
            trust,
        )
        .await?;
        if session.scope() != &mapping.scope() {
            return Err(ProvisioningError::StaleMappingGeneration);
        }
        authenticate_managed_identity(&mut session, recovered, mapping.relay_url()).await?;
        let discovery = read_discovery_event(&mut session, &channel.to_string()).await?;
        validate_nip29_discovery(
            &discovery,
            &channel.to_string(),
            &mapping.record().relay_pubkey,
        )?;
        let events = read_nip29_messages(
            &mut session,
            &channel.to_string(),
            MAX_HISTORY_EVENTS,
            MAX_EVENT_CONTENT_BYTES,
        )
        .await?;

        #[cfg(test)]
        wait_for_live_history_database_outage().await;

        let current_subject = self.resolver.resolve(authenticated).await?;
        let current_identity = self
            .vault
            .load_active_by_principal(current_subject.tenant_id.0, current_subject.principal_id.0)
            .await
            .map_err(|_| ProvisioningError::DatabaseUnavailable)?
            .ok_or(ProvisioningError::ReadDenied)?;
        let current_operation = self
            .operations
            .ready_for_identity(
                current_subject.tenant_id.0,
                current_subject.principal_id.0,
                current_subject.workspace_id.0,
                current_identity.identity_id,
                current_identity.generation,
            )
            .await
            .map_err(|_| ProvisioningError::ReadDenied)?;
        let current_mapping = self
            .mappings
            .load(current_subject.tenant_id, current_subject.workspace_id)
            .await
            .map_err(|_| ProvisioningError::MappingUnavailable)?;
        if current_subject.tenant_id != subject.tenant_id
            || current_subject.principal_id != subject.principal_id
            || current_subject.workspace_id != subject.workspace_id
            || current_operation.operation_id != operation.operation_id
            || !identity_binding_matches(
                operation.identity_id,
                operation.identity_generation,
                &identity,
                &current_identity,
            )
            || !mapping_binding_matches(
                operation.mapping_id,
                operation.mapping_generation,
                current_mapping.record().mapping_id,
                current_mapping.record().generation as i64,
            )
        {
            return Err(ProvisioningError::ReadDenied);
        }
        Ok(AuthorizedHistoryRead {
            events,
            operation_id: operation.operation_id,
            identity_id: identity.identity_id,
            identity_generation: identity.generation,
            mapping_id,
            mapping_generation,
        })
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
        if operation.status == ManagedIdentityOperationStatus::Abandoning.as_str() {
            return Err(ProvisioningError::Internal);
        }
        if operation.attempt_count >= operation.max_attempts {
            return Err(ProvisioningError::AttemptsExhausted);
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

        if operation.admission_event_id.is_some() && active.is_none() {
            return Err(self
                .fail(tenant_id, operation_id, ProvisioningError::IdentityConflict)
                .await);
        }

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
            if operation.admission_event_id.is_some()
                && (active.identity_id != operation.identity_id
                    || active.generation != operation.identity_generation)
            {
                return Err(self
                    .fail(tenant_id, operation_id, ProvisioningError::IdentityConflict)
                    .await);
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

        if operation.status != ManagedIdentityOperationStatus::Provisioning.as_str()
            && self
                .operations
                .attach_identity(
                    tenant_id,
                    operation_id,
                    operation.identity_id,
                    operation.identity_generation,
                )
                .await
                .is_err()
            && !self
                .identity_is_attached(
                    tenant_id,
                    operation_id,
                    operation.identity_id,
                    operation.identity_generation,
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
        let stored = match self
            .vault
            .insert_or_load_for_operation(operation_id, &envelope)
            .await
        {
            Ok(stored) => stored,
            Err(ManagedIdentityStoreError::OperationNotWritable) => {
                return Err(ProvisioningError::Internal)
            }
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
        if stored.status != "active"
            || stored.identity_id != operation.identity_id
            || stored.generation != operation.identity_generation
        {
            return Err(self
                .fail(tenant_id, operation_id, ProvisioningError::IdentityConflict)
                .await);
        }
        Ok(IdentityOutcome {
            identity_id: stored.identity_id,
            generation: stored.generation,
            public_key: stored.public_key,
        })
    }

    /// Resume retryable operations sequentially through relay admission and
    /// membership proof. It never invents an operation and is test-called only.
    pub(crate) async fn reconcile<K: DekKeyManagement>(
        &self,
        tenant_id: Uuid,
        limit: i64,
        kms: &K,
        relay_admin: &Keys,
        trust: &RelayTlsTrustConfig,
        membership_channel: &str,
    ) -> Result<ReconcileSummary, ProvisioningError> {
        let operations = self.operations.list_retryable(tenant_id, limit).await?;
        let mut summary = ReconcileSummary::default();
        for operation in operations {
            summary.attempted += 1;
            match self
                .drive(
                    tenant_id,
                    operation.operation_id,
                    kms,
                    relay_admin,
                    trust,
                    membership_channel,
                )
                .await
            {
                Ok(DriveOutcome::Ready(_)) => summary.advanced += 1,
                Ok(DriveOutcome::NotReady { .. }) => summary.failed += 1,
                Err(_) => summary.failed += 1,
            }
        }
        Ok(summary)
    }

    /// Disable this failed operation's attached identity before releasing its
    /// one-active slot. Repeating an already completed abandon is harmless.
    pub(crate) async fn abandon(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
    ) -> Result<(), ProvisioningError> {
        let operation = self.operations.load(tenant_id, operation_id).await?;
        if operation.status == ManagedIdentityOperationStatus::Abandoned.as_str() {
            return Ok(());
        }
        if operation.status == ManagedIdentityOperationStatus::Failed.as_str() {
            if let Err(error) = self.operations.begin_abandon(tenant_id, operation_id).await {
                let current = self.operations.load(tenant_id, operation_id).await?;
                if current.status == ManagedIdentityOperationStatus::Abandoned.as_str() {
                    return Ok(());
                }
                if current.status != ManagedIdentityOperationStatus::Abandoning.as_str() {
                    return Err(error.into());
                }
            }
        } else if operation.status != ManagedIdentityOperationStatus::Abandoning.as_str() {
            return Err(ProvisioningError::Internal);
        }

        if let Some(active) = self
            .vault
            .load_active_by_principal(tenant_id, operation.principal_id)
            .await?
        {
            if active.identity_id == operation.identity_id
                && active.generation == operation.identity_generation
                && !self
                    .operations
                    .identity_is_ready(
                        tenant_id,
                        operation.principal_id,
                        active.identity_id,
                        active.generation,
                    )
                    .await?
            {
                match self
                    .vault
                    .disable_or_revoke(
                        tenant_id,
                        active.identity_id,
                        ManagedIdentityStatus::Disabled,
                    )
                    .await
                {
                    Ok(()) | Err(ManagedIdentityStoreError::NotFound) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }

        match self.operations.abandon(tenant_id, operation_id).await {
            Ok(_) => Ok(()),
            Err(error) => {
                if self
                    .operations
                    .load(tenant_id, operation_id)
                    .await
                    .is_ok_and(|current| {
                        current.status == ManagedIdentityOperationStatus::Abandoned.as_str()
                    })
                {
                    Ok(())
                } else {
                    Err(error.into())
                }
            }
        }
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
        let _drive_guard = DRIVE_LOCK.lock().await;
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
        if !mapping_binding_matches(
            operation.mapping_id,
            operation.mapping_generation,
            mapping.record().mapping_id,
            mapping.record().generation as i64,
        ) {
            return Err(self
                .fail(
                    tenant_id,
                    operation_id,
                    ProvisioningError::StaleMappingGeneration,
                )
                .await);
        }
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
        if !identity_binding_matches(
            operation.identity_id,
            operation.identity_generation,
            &identity,
            &record,
        ) {
            return Err(self
                .fail(tenant_id, operation_id, ProvisioningError::IdentityConflict)
                .await);
        }
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
        if session.scope() != &mapping.scope() {
            return Err(self
                .fail(
                    tenant_id,
                    operation_id,
                    ProvisioningError::StaleMappingGeneration,
                )
                .await);
        }

        if operation.admission_event_id.is_none() {
            if let Err(error) =
                authenticate_relay_admin(&mut session, relay_admin, mapping.relay_url()).await
            {
                return Err(self.fail(tenant_id, operation_id, error).await);
            }
            if let Err(error) = self
                .admit(&mut session, &operation, &identity, relay_admin)
                .await
            {
                return Err(self.fail(tenant_id, operation_id, error).await);
            }
            println!("upstream provisioning stage: relay admission accepted");
            // Buzz closes the socket after the membership event changes relay state.
            drop(session);
            session = match connect(
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
            if session.scope() != &mapping.scope() {
                return Err(self
                    .fail(
                        tenant_id,
                        operation_id,
                        ProvisioningError::StaleMappingGeneration,
                    )
                    .await);
            }
        }
        // Replaying this deterministic event repairs a prior failure after NIP-43 admission.
        if let Err(error) =
            authenticate_relay_admin(&mut session, relay_admin, mapping.relay_url()).await
        {
            return Err(self.fail(tenant_id, operation_id, error).await);
        }
        if let Err(error) = add_nip29_member(
            &mut session,
            &operation,
            &identity.public_key,
            relay_admin,
            membership_channel,
        )
        .await
        {
            return Err(self.fail(tenant_id, operation_id, error).await);
        }
        println!("upstream provisioning stage: private channel membership accepted");
        drop(session);
        session = match connect(
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
        if session.scope() != &mapping.scope() {
            return Err(self
                .fail(
                    tenant_id,
                    operation_id,
                    ProvisioningError::StaleMappingGeneration,
                )
                .await);
        }
        if let Err(error) = prove_membership(
            &mut session,
            recovered,
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
            if self
                .operations
                .load(tenant_id, operation_id)
                .await
                .is_ok_and(|current| {
                    current.status == ManagedIdentityOperationStatus::Ready.as_str()
                })
            {
                return Ok(DriveOutcome::Ready(identity));
            }
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

struct AuthorizedHistoryRead {
    events: Vec<Event>,
    operation_id: Uuid,
    identity_id: Uuid,
    identity_generation: i64,
    mapping_id: Uuid,
    mapping_generation: i64,
}

fn mapping_binding_matches(
    operation_mapping_id: Uuid,
    operation_generation: i64,
    mapping_id: Uuid,
    mapping_generation: i64,
) -> bool {
    operation_mapping_id == mapping_id && operation_generation == mapping_generation
}

fn identity_binding_matches(
    operation_identity_id: Uuid,
    operation_generation: i64,
    identity: &IdentityOutcome,
    record: &ManagedIdentityRecord,
) -> bool {
    operation_identity_id == identity.identity_id
        && operation_generation == identity.generation
        && record.identity_id == identity.identity_id
        && record.generation == identity.generation
        && record.public_key == identity.public_key
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
    if frame.len() > 2 * 1024 * 1024 {
        return Err(ProvisioningError::ReadDenied);
    }
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
            let accepted = value[2].as_bool().unwrap_or(false);
            if !accepted {
                println!("upstream relay rejected event: {value}");
            }
            return Ok(accepted);
        }
        if value[0] == "CLOSED" {
            println!("upstream relay closed subscription: {value}");
            return Ok(false);
        }
    }
}

async fn authenticate_relay_admin(
    session: &mut TenantRelaySession,
    relay_admin: &Keys,
    relay_url: &str,
) -> Result<(), ProvisioningError> {
    let challenge = loop {
        let Some(value) = receive_json(session).await? else {
            continue;
        };
        if value[0] == "AUTH" {
            break value
                .get(1)
                .and_then(Value::as_str)
                .ok_or(ProvisioningError::AuthenticationRejected)?
                .to_owned();
        }
        if value[0] == "CLOSED" {
            return Err(ProvisioningError::AuthenticationRejected);
        }
    };
    println!("upstream provisioning stage: relay-admin AUTH challenge received");
    let relay =
        RelayUrl::parse(relay_url).map_err(|_| ProvisioningError::AuthenticationRejected)?;
    let event = EventBuilder::auth(&challenge, relay)
        .sign_with_keys(relay_admin)
        .map_err(|_| ProvisioningError::AuthenticationRejected)?;
    let event_id = event.id.to_hex();
    session
        .send_text(&serde_json::json!(["AUTH", event]).to_string())
        .await
        .map_err(|_| ProvisioningError::TransportUnavailable)?;
    if !await_ok(session, &event_id).await? {
        return Err(ProvisioningError::AuthenticationRejected);
    }
    println!("upstream provisioning stage: relay-admin authenticated");
    Ok(())
}

async fn add_nip29_member(
    session: &mut TenantRelaySession,
    operation: &ManagedIdentityOperationRecord,
    identity_pubkey: &str,
    relay_admin: &Keys,
    membership_channel: &str,
) -> Result<(), ProvisioningError> {
    let event = EventBuilder::new(Kind::Custom(9000), "")
        .tags([
            Tag::parse(["h", membership_channel])
                .map_err(|_| ProvisioningError::MembershipUnproven)?,
            Tag::parse(["p", identity_pubkey])
                .map_err(|_| ProvisioningError::MembershipUnproven)?,
        ])
        .custom_created_at(Timestamp::from(operation.created_at.timestamp() as u64))
        .sign_with_keys(relay_admin)
        .map_err(|_| ProvisioningError::MembershipUnproven)?;
    let event_id = event.id.to_hex();
    session
        .send_text(&serde_json::json!(["EVENT", event]).to_string())
        .await
        .map_err(|_| ProvisioningError::TransportUnavailable)?;
    if !await_ok(session, &event_id).await? {
        return Err(ProvisioningError::MembershipUnproven);
    }
    Ok(())
}

async fn prove_membership(
    session: &mut TenantRelaySession,
    recovered: RecoveredIdentity,
    relay_url: &str,
    membership_channel: &str,
) -> Result<(), ProvisioningError> {
    if membership_channel.is_empty() {
        return Err(ProvisioningError::MembershipUnproven);
    }
    authenticate_managed_identity(session, recovered, relay_url).await?;

    let subscription = format!("provision-{}", Uuid::new_v4());
    let saw_event = match tokio::time::timeout(HISTORY_SUBSCRIPTION_TIMEOUT, async {
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
        close_history_subscription(session, &subscription).await?;
        Ok::<bool, ProvisioningError>(saw_event)
    })
    .await
    {
        Ok(result) => result?,
        Err(_) => {
            session.abort();
            return Err(ProvisioningError::TransportUnavailable);
        }
    };
    if !saw_event {
        return Err(ProvisioningError::MembershipUnproven);
    }
    Ok(())
}

async fn authenticate_managed_identity(
    session: &mut TenantRelaySession,
    recovered: RecoveredIdentity,
    relay_url: &str,
) -> Result<(), ProvisioningError> {
    match tokio::time::timeout(
        HISTORY_SUBSCRIPTION_TIMEOUT,
        authenticate_managed_identity_inner(session, recovered, relay_url),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            session.abort();
            Err(ProvisioningError::TransportUnavailable)
        }
    }
}

async fn authenticate_managed_identity_inner(
    session: &mut TenantRelaySession,
    recovered: RecoveredIdentity,
    relay_url: &str,
) -> Result<(), ProvisioningError> {
    let challenge = loop {
        let Some(value) = receive_json(session).await? else {
            continue;
        };
        if value.get(0).and_then(Value::as_str) == Some("AUTH") {
            break value[1]
                .as_str()
                .ok_or(ProvisioningError::AuthenticationRejected)?
                .to_owned();
        }
        if value.get(0).and_then(Value::as_str) == Some("CLOSED") {
            return Err(ProvisioningError::AuthenticationRejected);
        }
    };
    if challenge.is_empty() || challenge.len() > 1024 {
        return Err(ProvisioningError::AuthenticationRejected);
    }
    let event = recovered
        .sign_nip42_authentication(
            &challenge,
            relay_url,
            Timestamp::from(Utc::now().timestamp() as u64),
        )
        .map_err(|_| ProvisioningError::AuthenticationRejected)?;
    if event.pubkey.to_hex() != recovered.public_key() || event.verify().is_err() {
        return Err(ProvisioningError::AuthenticationRejected);
    }
    let event_id = event.id.to_hex();
    drop(recovered);
    session
        .send_text(&serde_json::json!(["AUTH", event]).to_string())
        .await
        .map_err(|_| ProvisioningError::TransportUnavailable)?;
    if !await_ok(session, &event_id).await? {
        return Err(ProvisioningError::AuthenticationRejected);
    }
    Ok(())
}

async fn read_discovery_event(
    session: &mut TenantRelaySession,
    channel_id: &str,
) -> Result<Event, ProvisioningError> {
    match tokio::time::timeout(
        HISTORY_SUBSCRIPTION_TIMEOUT,
        read_discovery_event_inner(session, channel_id),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            session.abort();
            Err(ProvisioningError::TransportUnavailable)
        }
    }
}

async fn read_discovery_event_inner(
    session: &mut TenantRelaySession,
    channel_id: &str,
) -> Result<Event, ProvisioningError> {
    let subscription = format!("history-discovery-{}", Uuid::new_v4());
    session
        .send_text(
            &serde_json::json!([
                "REQ",
                subscription,
                {"kinds": [39000], "#d": [channel_id], "limit": 2}
            ])
            .to_string(),
        )
        .await
        .map_err(|_| ProvisioningError::TransportUnavailable)?;
    let mut events = Vec::new();
    loop {
        let Some(value) = receive_json(session).await? else {
            continue;
        };
        match value.get(0).and_then(Value::as_str) {
            Some("EVENT") if value.get(1).and_then(Value::as_str) == Some(&subscription) => {
                let raw = value.get(2).cloned().ok_or(ProvisioningError::ReadDenied)?;
                let event: Event =
                    serde_json::from_value(raw).map_err(|_| ProvisioningError::ReadDenied)?;
                if events.len() >= 2 {
                    return Err(ProvisioningError::ReadDenied);
                }
                events.push(event);
            }
            Some("EOSE") if value.get(1).and_then(Value::as_str) == Some(&subscription) => break,
            Some("CLOSED") if value.get(1).and_then(Value::as_str) == Some(&subscription) => {
                return Err(ProvisioningError::ReadDenied)
            }
            _ => {}
        }
    }
    close_history_subscription(session, &subscription).await?;
    match events.as_slice() {
        [event] => Ok(event.clone()),
        _ => Err(ProvisioningError::ReadDenied),
    }
}

async fn read_nip29_messages(
    session: &mut TenantRelaySession,
    channel_id: &str,
    max_events: usize,
    max_content_bytes: usize,
) -> Result<Vec<Event>, ProvisioningError> {
    match tokio::time::timeout(
        HISTORY_SUBSCRIPTION_TIMEOUT,
        read_nip29_messages_inner(session, channel_id, max_events, max_content_bytes),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            session.abort();
            Err(ProvisioningError::TransportUnavailable)
        }
    }
}

async fn read_nip29_messages_inner(
    session: &mut TenantRelaySession,
    channel_id: &str,
    max_events: usize,
    max_content_bytes: usize,
) -> Result<Vec<Event>, ProvisioningError> {
    let subscription = format!("history-{}", Uuid::new_v4());
    session
        .send_text(
            &serde_json::json!([
                "REQ",
                subscription,
                {"kinds": [9], "#h": [channel_id], "limit": max_events}
            ])
            .to_string(),
        )
        .await
        .map_err(|_| ProvisioningError::TransportUnavailable)?;
    let mut events = Vec::new();
    let mut total_bytes = 0usize;
    loop {
        let Some(value) = receive_json(session).await? else {
            continue;
        };
        if append_nip29_history_frame(
            &value,
            &subscription,
            channel_id,
            max_events,
            max_content_bytes,
            &mut events,
            &mut total_bytes,
        )? {
            break;
        }
    }
    close_history_subscription(session, &subscription).await?;
    Ok(events)
}

fn append_nip29_history_frame(
    value: &Value,
    subscription: &str,
    channel_id: &str,
    max_events: usize,
    max_content_bytes: usize,
    events: &mut Vec<Event>,
    total_bytes: &mut usize,
) -> Result<bool, ProvisioningError> {
    const MAX_EVENT_FRAME_BYTES: usize = 128 * 1024;
    const MAX_HISTORY_BYTES: usize = 8 * 1024 * 1024;

    match value.get(0).and_then(Value::as_str) {
        Some("EVENT") if value.get(1).and_then(Value::as_str) == Some(subscription) => {
            if events.len() >= max_events {
                return Err(ProvisioningError::ReadDenied);
            }
            let raw = value.get(2).cloned().ok_or(ProvisioningError::ReadDenied)?;
            let event_bytes = serde_json::to_vec(&raw)
                .map_err(|_| ProvisioningError::ReadDenied)?
                .len();
            *total_bytes = total_bytes
                .checked_add(event_bytes)
                .filter(|size| event_bytes <= MAX_EVENT_FRAME_BYTES && *size <= MAX_HISTORY_BYTES)
                .ok_or(ProvisioningError::ReadDenied)?;
            let event: Event =
                serde_json::from_value(raw).map_err(|_| ProvisioningError::ReadDenied)?;
            validate_nip29_message(&event, channel_id, max_content_bytes)?;
            events.push(event);
            Ok(false)
        }
        Some("EOSE") if value.get(1).and_then(Value::as_str) == Some(subscription) => Ok(true),
        Some("CLOSED") if value.get(1).and_then(Value::as_str) == Some(subscription) => {
            Err(ProvisioningError::ReadDenied)
        }
        _ => Ok(false),
    }
}

async fn close_history_subscription(
    session: &mut TenantRelaySession,
    subscription: &str,
) -> Result<(), ProvisioningError> {
    session
        .send_text(&serde_json::json!(["CLOSE", subscription]).to_string())
        .await
        .map_err(|_| ProvisioningError::TransportUnavailable)
}

fn validate_nip29_discovery(
    event: &Event,
    channel_id: &str,
    relay_pubkey: &str,
) -> Result<(), ProvisioningError> {
    let d_tags: Vec<_> = event
        .tags
        .iter()
        .filter_map(|tag| {
            let fields = tag.as_slice();
            (fields.first().map(String::as_str) == Some("d"))
                .then(|| fields.get(1).map(String::as_str))
                .flatten()
        })
        .collect();
    if event.kind != Kind::Custom(39000)
        || event.pubkey.to_hex() != relay_pubkey
        || event.content.len() > 1024 * 1024
        || d_tags.as_slice() != [channel_id]
        || event.verify().is_err()
    {
        return Err(ProvisioningError::ReadDenied);
    }
    Ok(())
}

fn validate_nip29_message(
    event: &Event,
    channel_id: &str,
    max_content_bytes: usize,
) -> Result<(), ProvisioningError> {
    let h_tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().map(String::as_str) == Some("h"))
        .collect();
    if event.kind != Kind::ChatMessage
        || event.content.len() > max_content_bytes
        || h_tags.len() != 1
        || h_tags[0].as_slice() != ["h", channel_id]
        || event.verify().is_err()
    {
        return Err(ProvisioningError::ReadDenied);
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
    use rustshare_storage::{
        ManagedIdentityOperationError, ManagedIdentityRecord, ManagedIdentityStatus,
        ManagedIdentityVaultStore, UpstreamRelayMappingStore,
    };
    use sqlx::PgPool;
    use tokio::sync::{Barrier, Notify};
    use tokio_rustls::rustls::pki_types::CertificateDer;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    use super::{
        append_nip29_history_frame, identity_binding_matches, mapping_binding_matches,
        validate_nip29_discovery, validate_nip29_message, AuthenticatedTestPrincipal, DriveOutcome,
        IdentityOutcome, ManagedIdentityProvisioner, ProvisioningContextResolver,
        ProvisioningError,
    };
    use crate::upstream_buzz_transport::RelayTlsTrustConfig;

    #[test]
    fn upstream_history_validates_relay_signed_discovery_and_kind9_scope() {
        let relay = Keys::generate();
        let author = Keys::generate();
        let channel = Uuid::new_v4().to_string();
        let discovery = nostr::EventBuilder::new(nostr::Kind::Custom(39000), "{}")
            .tags([nostr::Tag::parse(["d", channel.as_str()]).unwrap()])
            .sign_with_keys(&relay)
            .unwrap();
        assert!(
            validate_nip29_discovery(&discovery, &channel, &relay.public_key().to_hex()).is_ok()
        );
        assert!(
            validate_nip29_discovery(&discovery, &channel, &author.public_key().to_hex()).is_err()
        );

        let message = nostr::EventBuilder::new(nostr::Kind::ChatMessage, "test message")
            .tags([nostr::Tag::parse(["h", channel.as_str()]).unwrap()])
            .sign_with_keys(&author)
            .unwrap();
        assert!(validate_nip29_message(&message, &channel, 64).is_ok());
        let text_note = nostr::EventBuilder::new(nostr::Kind::TextNote, "not a NIP-29 message")
            .tags([nostr::Tag::parse(["h", channel.as_str()]).unwrap()])
            .sign_with_keys(&author)
            .unwrap();
        assert!(validate_nip29_message(&text_note, &channel, 64).is_err());
        assert!(validate_nip29_message(&message, &Uuid::new_v4().to_string(), 64).is_err());
        assert!(validate_nip29_message(&message, &channel, 4).is_err());
        let malformed_channel_tag =
            nostr::EventBuilder::new(nostr::Kind::ChatMessage, "test message")
                .tags([nostr::Tag::parse(["h", channel.as_str(), "extra"]).unwrap()])
                .sign_with_keys(&author)
                .unwrap();
        assert!(validate_nip29_message(&malformed_channel_tag, &channel, 64).is_err());
        let duplicate_channel_tags =
            nostr::EventBuilder::new(nostr::Kind::ChatMessage, "test message")
                .tags([
                    nostr::Tag::parse(["h", channel.as_str()]).unwrap(),
                    nostr::Tag::parse(["h", channel.as_str()]).unwrap(),
                ])
                .sign_with_keys(&author)
                .unwrap();
        assert!(validate_nip29_message(&duplicate_channel_tags, &channel, 64).is_err());
        let mut tampered = message;
        tampered.content.push('!');
        assert!(validate_nip29_message(&tampered, &channel, 64).is_err());
    }

    #[test]
    fn closed_history_subscription_denies_after_buffered_event() {
        let author = Keys::generate();
        let channel = Uuid::new_v4().to_string();
        let subscription = "history-test";
        let event = nostr::EventBuilder::new(nostr::Kind::ChatMessage, "private message")
            .tags([nostr::Tag::parse(["h", channel.as_str()]).unwrap()])
            .sign_with_keys(&author)
            .unwrap();
        let event_frame = serde_json::json!(["EVENT", subscription, event]);
        let closed_frame = serde_json::json!(["CLOSED", subscription, "not authorized"]);
        let result = (|| {
            let mut events = Vec::new();
            let mut total_bytes = 0;
            assert!(!append_nip29_history_frame(
                &event_frame,
                subscription,
                &channel,
                100,
                64 * 1024,
                &mut events,
                &mut total_bytes,
            )?);
            assert_eq!(events.len(), 1);
            if append_nip29_history_frame(
                &closed_frame,
                subscription,
                &channel,
                100,
                64 * 1024,
                &mut events,
                &mut total_bytes,
            )? {
                Ok(events)
            } else {
                Err(ProvisioningError::ReadDenied)
            }
        })();
        assert!(matches!(result, Err(ProvisioningError::ReadDenied)));
    }

    #[test]
    fn drive_rejects_changed_mapping_binding() {
        let mapping_id = Uuid::new_v4();
        assert!(mapping_binding_matches(mapping_id, 4, mapping_id, 4));
        assert!(!mapping_binding_matches(mapping_id, 4, Uuid::new_v4(), 4));
        assert!(!mapping_binding_matches(mapping_id, 4, mapping_id, 5));
    }

    #[test]
    fn drive_rejects_replaced_active_identity() {
        let identity_id = Uuid::new_v4();
        let principal_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let identity = IdentityOutcome {
            identity_id,
            generation: 3,
            public_key: "a".repeat(64),
        };
        let mut record = ManagedIdentityRecord {
            identity_id,
            tenant_id,
            principal_id,
            generation: 3,
            idempotency_key: Uuid::new_v4(),
            public_key: identity.public_key.clone(),
            envelope_version: 1,
            nonce: vec![0; 12],
            ciphertext: vec![0; 48],
            wrapped_dek: vec![0; 32],
            kms_key_id: "test".into(),
            kms_key_version: "v1".into(),
            status: "active".into(),
        };
        assert!(identity_binding_matches(identity_id, 3, &identity, &record));
        assert!(!identity_binding_matches(
            Uuid::new_v4(),
            3,
            &identity,
            &record
        ));
        assert!(!identity_binding_matches(
            identity_id,
            4,
            &identity,
            &record
        ));

        record.identity_id = Uuid::new_v4();
        assert!(!identity_binding_matches(
            identity_id,
            3,
            &identity,
            &record
        ));
        record.identity_id = identity_id;
        record.generation = 4;
        assert!(!identity_binding_matches(
            identity_id,
            3,
            &identity,
            &record
        ));
    }

    fn required(name: &str) -> String {
        std::env::var(name)
            .unwrap_or_else(|_| panic!("{name} must be set for the disposable relay test"))
    }

    async fn pause_for_relay_harness(marker: &str, ack_env: &str) {
        let ack = std::path::PathBuf::from(required(ack_env));
        assert!(
            !ack.exists(),
            "relay outage acknowledgement must start absent"
        );
        println!("{marker}");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(120), async {
            while !ack.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("relay outage acknowledgement timed out");
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

    struct BlockingKms {
        started: Notify,
        release: Notify,
    }

    #[async_trait::async_trait]
    impl DekKeyManagement for BlockingKms {
        async fn wrap_dek(
            &self,
            dek: &[u8; 32],
            _context: &KeyEncryptionContext,
        ) -> Result<WrappedDek, VaultError> {
            self.started.notify_one();
            self.release.notified().await;
            Ok(WrappedDek {
                key_id: "test-kek".into(),
                key_version: "v1".into(),
                ciphertext: dek.to_vec(),
            })
        }

        async fn unwrap_dek(
            &self,
            _wrapped: &WrappedDek,
            _context: &KeyEncryptionContext,
        ) -> Result<Zeroizing<[u8; 32]>, VaultError> {
            Err(VaultError::KeyManagement)
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
        let workspace = tenant;
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

        let mismatched_workspace = AuthenticatedTestPrincipal::new(user, tenant, Uuid::new_v4());
        assert_eq!(
            resolver.resolve(&mismatched_workspace).await.unwrap_err(),
            ProvisioningError::InvalidSubject
        );
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
    async fn identity_claim_precedes_kms_and_resumes_after_interruption() {
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
        provisioner
            .operations()
            .mark_failed(tenant, operation.operation_id, "kms_unavailable")
            .await
            .unwrap();

        let kms = Arc::new(BlockingKms {
            started: Notify::new(),
            release: Notify::new(),
        });
        let task_provisioner = provisioner.clone();
        let task_kms = kms.clone();
        let operation_id = operation.operation_id;
        let task = tokio::spawn(async move {
            task_provisioner
                .ensure_identity(tenant, operation_id, task_kms.as_ref())
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), kms.started.notified())
            .await
            .expect("KMS wrapping was not reached");

        let claimed = provisioner
            .operations()
            .load(tenant, operation.operation_id)
            .await
            .unwrap();
        assert_eq!(claimed.status, "provisioning");
        assert_eq!(claimed.identity_id, operation.identity_id);
        assert_eq!(claimed.identity_generation, operation.identity_generation);
        assert!(provisioner
            .vault
            .load_active_by_principal(tenant, user)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            provisioner
                .abandon(tenant, operation.operation_id)
                .await
                .unwrap_err(),
            ProvisioningError::Internal
        );

        task.abort();
        let _ = task.await;
        let recovered = provisioner
            .ensure_identity(tenant, operation.operation_id, &TestKms::new())
            .await
            .unwrap();
        assert_eq!(recovered.identity_id, operation.identity_id);
        assert_eq!(recovered.generation, operation.identity_generation);
        let active = provisioner
            .vault
            .load_active_by_principal(tenant, user)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(active.identity_id, operation.identity_id);
        assert_eq!(active.generation, operation.identity_generation);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
    async fn late_kms_result_cannot_insert_identity_after_abandonment() {
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
        provisioner
            .operations()
            .mark_failed(tenant, operation.operation_id, "kms_unavailable")
            .await
            .unwrap();

        let kms = Arc::new(BlockingKms {
            started: Notify::new(),
            release: Notify::new(),
        });
        let task_provisioner = provisioner.clone();
        let task_kms = kms.clone();
        let operation_id = operation.operation_id;
        let task = tokio::spawn(async move {
            task_provisioner
                .ensure_identity(tenant, operation_id, task_kms.as_ref())
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), kms.started.notified())
            .await
            .expect("KMS wrapping was not reached");

        // Model a competing attempt failing while this KMS request is pending.
        provisioner
            .operations()
            .mark_failed(tenant, operation.operation_id, "kms_unavailable")
            .await
            .unwrap();
        provisioner
            .abandon(tenant, operation.operation_id)
            .await
            .unwrap();
        kms.release.notify_one();
        assert_eq!(
            task.await.unwrap().unwrap_err(),
            ProvisioningError::Internal
        );
        assert!(provisioner
            .vault
            .load_active_by_principal(tenant, user)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            provisioner
                .operations()
                .load(tenant, operation.operation_id)
                .await
                .unwrap()
                .status,
            "abandoned"
        );
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
    async fn admitted_operation_cannot_rebind_to_a_replacement_identity() {
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
        let admitted = provisioner
            .ensure_identity(tenant, operation.operation_id, &kms)
            .await
            .unwrap();
        let admission_event_id = "d".repeat(64);
        provisioner
            .operations()
            .record_admission_sent(tenant, operation.operation_id, &admission_event_id)
            .await
            .unwrap();
        provisioner
            .operations()
            .mark_failed(tenant, operation.operation_id, "authentication_rejected")
            .await
            .unwrap();
        provisioner
            .vault
            .disable_or_revoke(
                tenant,
                admitted.identity_id,
                ManagedIdentityStatus::Disabled,
            )
            .await
            .unwrap();

        let replacement_id = Uuid::new_v4();
        let replacement = ManagedIdentityEnvelope::generate(
            IdentityContext {
                tenant_id: tenant,
                principal_id: user,
                identity_id: replacement_id,
                generation: admitted.generation + 1,
            },
            &kms,
        )
        .await
        .unwrap();
        provisioner
            .vault
            .insert_or_load(Uuid::new_v4(), &replacement)
            .await
            .unwrap();

        assert_eq!(
            provisioner
                .ensure_identity(tenant, operation.operation_id, &kms)
                .await
                .unwrap_err(),
            ProvisioningError::IdentityConflict
        );
        let unchanged = provisioner
            .operations()
            .load(tenant, operation.operation_id)
            .await
            .unwrap();
        assert_eq!(unchanged.identity_id, admitted.identity_id);
        assert_eq!(unchanged.identity_generation, admitted.generation);
        assert_eq!(
            unchanged.admission_event_id.as_deref(),
            Some(admission_event_id.as_str())
        );
        let active = provisioner
            .vault
            .load_active_by_principal(tenant, user)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(active.identity_id, replacement_id);
        provisioner
            .vault
            .disable_or_revoke(tenant, replacement_id, ManagedIdentityStatus::Disabled)
            .await
            .unwrap();
        assert_eq!(
            provisioner
                .ensure_identity(tenant, operation.operation_id, &kms)
                .await
                .unwrap_err(),
            ProvisioningError::IdentityConflict
        );
        assert!(provisioner
            .vault
            .load_active_by_principal(tenant, user)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the integration-tests workflow"]
    async fn failed_admitted_operation_rejects_a_replacement_identity() {
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
        let admitted = provisioner
            .ensure_identity(tenant, operation.operation_id, &kms)
            .await
            .unwrap();
        let admission_event_id = "c".repeat(64);
        provisioner
            .operations()
            .record_admission_sent(tenant, operation.operation_id, &admission_event_id)
            .await
            .unwrap();
        provisioner
            .operations()
            .mark_failed(tenant, operation.operation_id, "authentication_rejected")
            .await
            .unwrap();
        provisioner
            .vault
            .disable_or_revoke(
                tenant,
                admitted.identity_id,
                ManagedIdentityStatus::Disabled,
            )
            .await
            .unwrap();

        let replacement_id = Uuid::new_v4();
        let replacement = ManagedIdentityEnvelope::generate(
            IdentityContext {
                tenant_id: tenant,
                principal_id: user,
                identity_id: replacement_id,
                generation: admitted.generation + 1,
            },
            &kms,
        )
        .await
        .unwrap();
        provisioner
            .vault
            .insert_or_load(Uuid::new_v4(), &replacement)
            .await
            .unwrap();

        assert_eq!(
            provisioner
                .ensure_identity(tenant, operation.operation_id, &kms)
                .await
                .unwrap_err(),
            ProvisioningError::IdentityConflict
        );
        let unchanged = provisioner
            .operations()
            .load(tenant, operation.operation_id)
            .await
            .unwrap();
        assert_eq!(unchanged.identity_id, admitted.identity_id);
        assert_eq!(unchanged.identity_generation, admitted.generation);
        assert_eq!(
            unchanged.admission_event_id.as_deref(),
            Some(admission_event_id.as_str())
        );
        let active = provisioner
            .vault
            .load_active_by_principal(tenant, user)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(active.identity_id, replacement_id);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn drive_rejects_mapping_generation_changed_after_authorize() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let original = mapping
            .load(TenantId(tenant), WorkspaceId(workspace))
            .await
            .unwrap()
            .record()
            .clone();
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), mapping.clone());
        let operation = provisioner
            .authorize(
                AuthenticatedTestPrincipal::new(user, tenant, workspace),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        assert_eq!(operation.mapping_generation, 1);

        let operator_service_id = original.updated_by;
        let signing_key = SigningKey::from_bytes(&[21; 32]);
        let now = Utc::now();
        let issued_at = now
            .with_nanosecond(now.timestamp_subsec_micros() * 1_000)
            .unwrap();
        let mut statement = MappingProvisioningStatement {
            operator_service_id,
            operator_key_id: "deployment-test-key-v1".into(),
            operation_id: Uuid::new_v4(),
            change_reference_id: Uuid::new_v4(),
            action: MappingProvisioningAction::Update,
            expected_generation: 1,
            issued_at,
            expires_at: issued_at + Duration::minutes(2),
            record: original,
            signature: Vec::new(),
        };
        statement.record.generation = 2;
        statement.record.audit_entry_id = Uuid::new_v4();
        statement.signature = signing_key
            .sign(&statement.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        mapping.apply(&statement).await.unwrap();

        assert_eq!(
            provisioner
                .drive(
                    tenant,
                    operation.operation_id,
                    &TestKms::new(),
                    &Keys::generate(),
                    &RelayTlsTrustConfig::StrictPublic,
                    "membership",
                )
                .await
                .unwrap_err(),
            ProvisioningError::StaleMappingGeneration
        );
        assert_eq!(
            mapping
                .load(TenantId(tenant), WorkspaceId(workspace))
                .await
                .unwrap()
                .record()
                .generation,
            2
        );
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
    async fn kms_outage_fails_closed_then_identity_retry_recovers() {
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
        recovered
            .ensure_identity(tenant, operation.operation_id, &kms)
            .await
            .unwrap();
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
                .operations()
                .load(tenant, operation.operation_id)
                .await
                .unwrap()
                .attempt_count,
            1
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
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn abandon_stale_mapping_failure_disables_identity_and_releases_slot() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), mapping);
        let authenticated = AuthenticatedTestPrincipal::new(user, tenant, workspace);
        let operation = provisioner
            .authorize(authenticated.clone(), Uuid::new_v4())
            .await
            .unwrap();
        let identity = provisioner
            .ensure_identity(tenant, operation.operation_id, &TestKms::new())
            .await
            .unwrap();
        provisioner
            .operations()
            .mark_failed(tenant, operation.operation_id, "stale_mapping_generation")
            .await
            .unwrap();

        provisioner
            .abandon(tenant, operation.operation_id)
            .await
            .unwrap();
        provisioner
            .abandon(tenant, operation.operation_id)
            .await
            .unwrap();
        let identity_status: String = sqlx::query_scalar(
            "SELECT status FROM managed_chat_identities WHERE tenant_id=$1 AND identity_id=$2",
        )
        .bind(tenant)
        .bind(identity.identity_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(identity_status, "disabled");
        assert_eq!(
            provisioner
                .operations()
                .load(tenant, operation.operation_id)
                .await
                .unwrap()
                .status,
            "abandoned"
        );
        provisioner
            .authorize(authenticated, Uuid::new_v4())
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn abandon_exhausted_operation_disables_identity_and_releases_slot() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), mapping);
        let authenticated = AuthenticatedTestPrincipal::new(user, tenant, workspace);
        let operation = provisioner
            .authorize(authenticated.clone(), Uuid::new_v4())
            .await
            .unwrap();
        let identity = provisioner
            .ensure_identity(tenant, operation.operation_id, &TestKms::new())
            .await
            .unwrap();
        sqlx::query(
            "UPDATE managed_chat_identity_operations \
             SET max_attempts=1, attempt_count=1, status='failed', error_code='attempts_exhausted' \
             WHERE tenant_id=$1 AND operation_id=$2",
        )
        .bind(tenant)
        .bind(operation.operation_id)
        .execute(&pool)
        .await
        .unwrap();

        provisioner
            .abandon(tenant, operation.operation_id)
            .await
            .unwrap();
        let identity_status: String = sqlx::query_scalar(
            "SELECT status FROM managed_chat_identities WHERE tenant_id=$1 AND identity_id=$2",
        )
        .bind(tenant)
        .bind(identity.identity_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(identity_status, "disabled");
        provisioner
            .authorize(authenticated, Uuid::new_v4())
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn abandon_keeps_slot_claimed_when_identity_disable_fails() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), mapping);
        let authenticated = AuthenticatedTestPrincipal::new(user, tenant, workspace);
        let operation = provisioner
            .authorize(authenticated.clone(), Uuid::new_v4())
            .await
            .unwrap();
        let identity = provisioner
            .ensure_identity(tenant, operation.operation_id, &TestKms::new())
            .await
            .unwrap();
        provisioner
            .operations()
            .mark_failed(tenant, operation.operation_id, "stale_mapping_generation")
            .await
            .unwrap();

        let trigger = format!("test_abandon_{}", Uuid::new_v4().simple());
        let function = format!("{trigger}_fn");
        sqlx::query(&format!(
            "CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN IF OLD.identity_id = '{identity_id}' AND NEW.status = 'disabled' \
             THEN RAISE EXCEPTION 'test disable failure'; END IF; RETURN NEW; END $$",
            identity_id = identity.identity_id,
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(&format!(
            "CREATE TRIGGER {trigger} BEFORE UPDATE OF status ON managed_chat_identities \
             FOR EACH ROW EXECUTE FUNCTION {function}()"
        ))
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            provisioner
                .abandon(tenant, operation.operation_id)
                .await
                .unwrap_err(),
            ProvisioningError::DatabaseUnavailable
        );
        assert_eq!(
            provisioner
                .operations()
                .load(tenant, operation.operation_id)
                .await
                .unwrap()
                .status,
            "abandoning"
        );
        assert_eq!(
            provisioner
                .authorize(authenticated.clone(), Uuid::new_v4())
                .await
                .unwrap_err(),
            ProvisioningError::IdentityConflict
        );
        assert_eq!(
            provisioner
                .ensure_identity(tenant, operation.operation_id, &TestKms::new())
                .await
                .unwrap_err(),
            ProvisioningError::Internal
        );

        sqlx::query(&format!(
            "DROP TRIGGER {trigger} ON managed_chat_identities"
        ))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(&format!("DROP FUNCTION {function}()"))
            .execute(&pool)
            .await
            .unwrap();
        provisioner
            .abandon(tenant, operation.operation_id)
            .await
            .unwrap();
        provisioner
            .authorize(authenticated, Uuid::new_v4())
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL; exercised by the dedicated live provisioning run"]
    async fn abandon_does_not_disable_a_ready_identity() {
        let pool = pool().await;
        let (tenant, user, workspace) = tenant_user_workspace(&pool).await;
        let mapping = mapping_store(&pool, tenant, workspace).await;
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), mapping);
        let authenticated = AuthenticatedTestPrincipal::new(user, tenant, workspace);
        let operation = provisioner
            .authorize(authenticated.clone(), Uuid::new_v4())
            .await
            .unwrap();
        let identity = provisioner
            .ensure_identity(tenant, operation.operation_id, &TestKms::new())
            .await
            .unwrap();
        provisioner
            .operations()
            .record_admission_sent(tenant, operation.operation_id, &"a".repeat(64))
            .await
            .unwrap();
        provisioner
            .operations()
            .mark_ready(tenant, operation.operation_id)
            .await
            .unwrap();

        assert_eq!(
            provisioner
                .abandon(tenant, operation.operation_id)
                .await
                .unwrap_err(),
            ProvisioningError::Internal
        );
        let status: String = sqlx::query_scalar(
            "SELECT status FROM managed_chat_identities WHERE tenant_id=$1 AND identity_id=$2",
        )
        .bind(tenant)
        .bind(identity.identity_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "active");

        let retry = provisioner
            .authorize(authenticated.clone(), Uuid::new_v4())
            .await
            .unwrap();
        let reused = provisioner
            .ensure_identity(tenant, retry.operation_id, &TestKms::new())
            .await
            .unwrap();
        assert_eq!(reused.identity_id, identity.identity_id);
        provisioner
            .operations()
            .mark_failed(tenant, retry.operation_id, "stale_mapping_generation")
            .await
            .unwrap();
        provisioner
            .abandon(tenant, retry.operation_id)
            .await
            .unwrap();

        let status: String = sqlx::query_scalar(
            "SELECT status FROM managed_chat_identities WHERE tenant_id=$1 AND identity_id=$2",
        )
        .bind(tenant)
        .bind(identity.identity_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "active");
        assert_eq!(
            provisioner
                .operations()
                .load(tenant, operation.operation_id)
                .await
                .unwrap()
                .status,
            "ready"
        );
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL, relay-admin key, and private Buzz service"]
    async fn live_provisioning_reaches_ready_after_nip42_and_member_only_read() {
        run_live_provisioning(false).await;
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL, relay-admin key, and private Buzz service"]
    async fn live_provisioning_recovers_after_relay_outage() {
        run_live_provisioning(true).await;
    }

    async fn run_live_provisioning(with_relay_outage: bool) {
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
        let membership_channel = required("BUZZ_TEST_PROVISIONING_CHANNEL_ID");
        let relay_admin = Keys::parse(&required("BUZZ_TEST_RELAY_ADMIN_NSEC")).unwrap();

        let pool = PgPool::connect(&database_url).await.unwrap();
        sqlx::migrate!("../migrations").run(&pool).await.unwrap();
        let tenant = Uuid::new_v4();
        let workspace = tenant;
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
        let provisioner = ManagedIdentityProvisioner::new(pool.clone(), store.clone());
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
        let kms = TestKms::new();
        let original_mapping = store
            .load(TenantId(tenant), WorkspaceId(workspace))
            .await
            .unwrap()
            .record()
            .clone();
        assert_eq!(operation.mapping_id, original_mapping.mapping_id);
        assert_eq!(
            operation.mapping_generation,
            original_mapping.generation as i64
        );
        let identity = provisioner
            .ensure_identity(tenant, operation.operation_id, &kms)
            .await
            .unwrap();

        // A second entitled tenant may use its own mapping and operation, but
        // tenant-scoped stores must hide the first tenant's operation/mapping/key.
        let (other_tenant, other_user, other_workspace) = tenant_user_workspace(&pool).await;
        let other_mappings = mapping_store(&pool, other_tenant, other_workspace).await;
        let other_provisioner =
            ManagedIdentityProvisioner::new(pool.clone(), other_mappings.clone());
        let other_operation = other_provisioner
            .authorize(
                AuthenticatedTestPrincipal::new(other_user, other_tenant, other_workspace),
                Uuid::new_v4(),
            )
            .await
            .unwrap();
        assert_eq!(
            other_provisioner
                .operations()
                .load(other_tenant, operation.operation_id)
                .await
                .unwrap_err(),
            ManagedIdentityOperationError::NotFound
        );
        assert!(other_mappings
            .load(TenantId(other_tenant), WorkspaceId(workspace))
            .await
            .is_err());
        assert!(other_provisioner
            .vault
            .load_active_by_principal(other_tenant, user)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            other_provisioner
                .drive(
                    other_tenant,
                    operation.operation_id,
                    &kms,
                    &relay_admin,
                    &trust,
                    &membership_channel,
                )
                .await
                .unwrap_err(),
            ProvisioningError::Internal
        );
        let untouched_other_operation = other_provisioner
            .operations()
            .load(other_tenant, other_operation.operation_id)
            .await
            .unwrap();
        assert_eq!(untouched_other_operation.status, "authorized");
        assert!(untouched_other_operation.admission_event_id.is_none());
        assert!(other_provisioner
            .vault
            .load_active_by_principal(other_tenant, other_user)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            provisioner
                .operations()
                .load(tenant, operation.operation_id)
                .await
                .unwrap()
                .status,
            "provisioning"
        );
        assert_eq!(
            provisioner
                .vault
                .load_active_by_principal(tenant, user)
                .await
                .unwrap()
                .unwrap()
                .identity_id,
            identity.identity_id
        );
        let initial_identity = if with_relay_outage {
            let identity = provisioner
                .ensure_identity(tenant, operation.operation_id, &kms)
                .await
                .unwrap();
            pause_for_relay_harness(
                "BUZZ_TEST_RELAY_OUTAGE_READY",
                "BUZZ_TEST_RELAY_OUTAGE_ACK_FILE",
            )
            .await;
            assert_eq!(
                provisioner
                    .drive(
                        tenant,
                        operation.operation_id,
                        &kms,
                        &relay_admin,
                        &trust,
                        &membership_channel,
                    )
                    .await
                    .unwrap_err(),
                ProvisioningError::TransportUnavailable
            );
            let failed = provisioner
                .operations()
                .load(tenant, operation.operation_id)
                .await
                .unwrap();
            assert_eq!(failed.status, "failed");
            assert_eq!(failed.error_code.as_deref(), Some("transport_unavailable"));
            assert_eq!(failed.attempt_count, 1);
            assert!(failed.attempt_count < failed.max_attempts);
            assert_eq!(failed.identity_id, identity.identity_id);
            assert_eq!(failed.identity_generation, identity.generation);
            assert!(failed.admission_event_id.is_none());
            assert!(provisioner
                .operations()
                .list_retryable(tenant, 10)
                .await
                .unwrap()
                .iter()
                .any(|retryable| retryable.operation_id == operation.operation_id));
            let still_active = provisioner
                .vault
                .load_active_by_principal(tenant, user)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(still_active.identity_id, identity.identity_id);
            assert_eq!(still_active.generation, identity.generation);
            let during_outage_mapping = store
                .load(TenantId(tenant), WorkspaceId(workspace))
                .await
                .unwrap()
                .record()
                .clone();
            assert_eq!(
                during_outage_mapping.mapping_id,
                original_mapping.mapping_id
            );
            assert_eq!(
                during_outage_mapping.generation,
                original_mapping.generation
            );
            pause_for_relay_harness(
                "BUZZ_TEST_RELAY_RESTORE_READY",
                "BUZZ_TEST_RELAY_RESTORE_ACK_FILE",
            )
            .await;
            let summary = provisioner
                .reconcile(tenant, 10, &kms, &relay_admin, &trust, &membership_channel)
                .await
                .unwrap();
            assert_eq!(summary.attempted, 1);
            assert_eq!(summary.advanced, 1);
            assert_eq!(summary.failed, 0);
            let ready_after_reconcile = provisioner
                .operations()
                .load(tenant, operation.operation_id)
                .await
                .unwrap();
            assert_eq!(ready_after_reconcile.status, "ready");
            let recovered = provisioner
                .ensure_identity(tenant, operation.operation_id, &kms)
                .await
                .unwrap();
            assert_eq!(recovered, identity);
            Some(identity)
        } else {
            None
        };
        let (outcome, concurrent_outcome) = tokio::join!(
            provisioner.drive(
                tenant,
                operation.operation_id,
                &kms,
                &relay_admin,
                &trust,
                &membership_channel,
            ),
            provisioner.drive(
                tenant,
                operation.operation_id,
                &kms,
                &relay_admin,
                &trust,
                &membership_channel,
            ),
        );
        let outcome = outcome.unwrap();
        assert_eq!(concurrent_outcome.unwrap(), outcome);
        let DriveOutcome::Ready(identity) = outcome else {
            panic!("provisioning did not reach ready")
        };
        if let Some(initial_identity) = initial_identity {
            assert_eq!(identity, initial_identity);
        }
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
        let authenticated = AuthenticatedTestPrincipal::new(user, tenant, workspace);
        let history = provisioner
            .read_channel_history(&authenticated, &membership_channel, &kms, &trust)
            .await
            .unwrap();
        let expected_event_id = required("BUZZ_TEST_PROVISIONING_EVENT_ID");
        assert!(history.iter().any(|event| {
            event.id.to_hex() == expected_event_id
                && event.pubkey.to_hex() == relay_admin.public_key().to_hex()
        }));
        let denied = provisioner
            .read_channel_history(&authenticated, &Uuid::new_v4().to_string(), &kms, &trust)
            .await;
        assert!(matches!(denied, Err(ProvisioningError::ReadDenied)));
        kms.set_unavailable(true);
        let kms_denied = provisioner
            .read_channel_history(&authenticated, &membership_channel, &kms, &trust)
            .await;
        assert!(matches!(kms_denied, Err(ProvisioningError::KmsUnavailable)));
        kms.set_unavailable(false);
        let audits: Vec<String> = sqlx::query_scalar(
            "SELECT description FROM user_security_events \
             WHERE user_id=$1 AND event_type='managed_chat_history_access'",
        )
        .bind(user)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(audits.len(), 3);
        assert!(audits.iter().any(|entry| entry.starts_with("allowed ")));
        assert!(audits.iter().any(|entry| entry.starts_with("denied ")));
        assert!(audits
            .iter()
            .any(|entry| entry.contains("reason=kms_unavailable")));
        assert!(audits
            .iter()
            .all(|entry| !entry.contains("provisioning-channel-seed")));
        let final_mapping = store
            .load(TenantId(tenant), WorkspaceId(workspace))
            .await
            .unwrap();
        assert_eq!(
            final_mapping.record().mapping_id,
            original_mapping.mapping_id
        );
        assert_eq!(
            final_mapping.record().generation,
            original_mapping.generation
        );
        let database_denied = provisioner
            .read_channel_history(&authenticated, &membership_channel, &kms, &trust)
            .await;
        assert!(matches!(
            database_denied,
            Err(ProvisioningError::DatabaseUnavailable)
        ));
        println!("history_reader_db_outage=DATABASE_UNAVAILABLE");
        let restore_ack_path =
            std::path::PathBuf::from(required("BUZZ_TEST_HISTORY_DB_RESTORE_ACK_FILE"));
        println!("BUZZ_TEST_HISTORY_DB_RESTORE_READY");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(120), async {
            while !restore_ack_path.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("history database restore acknowledgement timed out");
    }
}
