//! Tenant-scoped persistence for managed identity provisioning operations.
//!
//! Each operation binds a trusted subject to an upstream relay mapping and
//! carries the pre-allocated identity attribution through to the actual vault
//! row. Exactly one operation per subject stays in the one-active index
//! (`authorized`/`provisioning`/`failed`/`abandoning`) until it is ready or abandoned; the
//! table cannot hold key material (see the Task 1 migration constraints).

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedIdentityOperationStatus {
    Authorized,
    Provisioning,
    Ready,
    Failed,
    Abandoning,
    Abandoned,
}

impl ManagedIdentityOperationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Authorized => "authorized",
            Self::Provisioning => "provisioning",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Abandoning => "abandoning",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "authorized" => Some(Self::Authorized),
            "provisioning" => Some(Self::Provisioning),
            "ready" => Some(Self::Ready),
            "failed" => Some(Self::Failed),
            "abandoning" => Some(Self::Abandoning),
            "abandoned" => Some(Self::Abandoned),
            _ => None,
        }
    }
}

/// Bounded catalog from the migration CHECK constraint on error_code;
/// mark_failed must not persist free-form error text. The integration test
/// asserts the migration catalog and this catalog never drift apart.
pub const MANAGED_IDENTITY_OPERATION_ERROR_CODES: [&str; 12] = [
    "invalid_subject",
    "mapping_unavailable",
    "stale_mapping_generation",
    "kms_unavailable",
    "identity_conflict",
    "transport_unavailable",
    "admission_rejected",
    "authentication_rejected",
    "membership_unproven",
    "database_unavailable",
    "attempts_exhausted",
    "internal_error",
];

const COLUMNS: &str = "operation_id, tenant_id, principal_id, workspace_id, mapping_id, \
 mapping_generation, identity_id, identity_generation, idempotency_key, status, error_code, \
 attempt_count, max_attempts, admission_event_id, created_at, updated_at, admitted_at, \
 ready_at, last_attempt_at";

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ManagedIdentityOperationError {
    #[error("managed identity operation storage failed")]
    Database,
    #[error("another active managed identity operation already exists for this subject")]
    Conflict,
    #[error("managed identity operation was not found")]
    NotFound,
    #[error("managed identity operation transition is not allowed from its current status")]
    InvalidTransition,
    #[error("managed identity operation request is invalid")]
    InvalidRequest,
}

#[derive(Clone, Debug, FromRow)]
pub struct ManagedIdentityOperationRecord {
    pub operation_id: Uuid,
    pub tenant_id: Uuid,
    pub principal_id: Uuid,
    pub workspace_id: Uuid,
    pub mapping_id: Uuid,
    pub mapping_generation: i64,
    pub identity_id: Uuid,
    pub identity_generation: i64,
    pub idempotency_key: Uuid,
    pub status: String,
    pub error_code: Option<String>,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub admission_event_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub admitted_at: Option<DateTime<Utc>>,
    pub ready_at: Option<DateTime<Utc>>,
    pub last_attempt_at: Option<DateTime<Utc>>,
}

/// The mapping binding is supplied by the provisioner from the signed mapping
/// store after trusted subject resolution; the operation store never
/// discovers mappings itself.
#[derive(Clone, Debug)]
pub struct ProvisioningRequest {
    pub tenant_id: Uuid,
    pub principal_id: Uuid,
    pub workspace_id: Uuid,
    pub mapping_id: Uuid,
    pub mapping_generation: i64,
    pub idempotency_key: Uuid,
}

/// Database operations for the Phase 2B.2 provisioning operation lifecycle.
/// No runtime caller is wired.
#[derive(Clone)]
pub struct ManagedIdentityOperationStore(PgPool);

impl ManagedIdentityOperationStore {
    pub fn new(pool: PgPool) -> Self {
        Self(pool)
    }

    /// Insert once per (tenant, principal, idempotency key), pre-allocating
    /// the next vault generation for the subject. Concurrent retries return
    /// the committed winner; a different key while one operation is active
    /// conflicts with the one-active index.
    pub async fn authorize(
        &self,
        request: ProvisioningRequest,
    ) -> Result<ManagedIdentityOperationRecord, ManagedIdentityOperationError> {
        if request.mapping_generation <= 0 {
            return Err(ManagedIdentityOperationError::InvalidRequest);
        }
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, request.tenant_id).await?;
        let next_generation: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(generation),0)+1 FROM managed_chat_identities \
             WHERE tenant_id=$1 AND principal_id=$2",
        )
        .bind(request.tenant_id)
        .bind(request.principal_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_database_error)?;
        let inserted = sqlx::query_as::<_, ManagedIdentityOperationRecord>(&format!(
            "INSERT INTO managed_chat_identity_operations \
             (operation_id, tenant_id, principal_id, workspace_id, mapping_id, mapping_generation, \
              identity_id, identity_generation, idempotency_key) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) \
             ON CONFLICT (tenant_id, principal_id, idempotency_key) DO NOTHING \
             RETURNING {COLUMNS}"
        ))
        .bind(Uuid::new_v4())
        .bind(request.tenant_id)
        .bind(request.principal_id)
        .bind(request.workspace_id)
        .bind(request.mapping_id)
        .bind(request.mapping_generation)
        .bind(Uuid::new_v4())
        .bind(next_generation)
        .bind(request.idempotency_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;

        let record = match inserted {
            Some(record) => record,
            None => sqlx::query_as::<_, ManagedIdentityOperationRecord>(&format!(
                "SELECT {COLUMNS} FROM managed_chat_identity_operations \
                 WHERE tenant_id=$1 AND principal_id=$2 AND idempotency_key=$3"
            ))
            .bind(request.tenant_id)
            .bind(request.principal_id)
            .bind(request.idempotency_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_database_error)?
            .ok_or(ManagedIdentityOperationError::Conflict)?,
        };
        tx.commit().await.map_err(map_database_error)?;
        Ok(record)
    }

    pub async fn load(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
    ) -> Result<ManagedIdentityOperationRecord, ManagedIdentityOperationError> {
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let record = sqlx::query_as::<_, ManagedIdentityOperationRecord>(&format!(
            "SELECT {COLUMNS} FROM managed_chat_identity_operations \
             WHERE tenant_id=$1 AND operation_id=$2"
        ))
        .bind(tenant_id)
        .bind(operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?
        .ok_or(ManagedIdentityOperationError::NotFound)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(record)
    }

    /// Whether a ready operation already relies on this active identity.
    pub async fn identity_is_ready(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
        identity_id: Uuid,
        identity_generation: i64,
    ) -> Result<bool, ManagedIdentityOperationError> {
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let ready = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM managed_chat_identity_operations \
             WHERE tenant_id=$1 AND principal_id=$2 AND identity_id=$3 \
               AND identity_generation=$4 AND status='ready')",
        )
        .bind(tenant_id)
        .bind(principal_id)
        .bind(identity_id)
        .bind(identity_generation)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(ready)
    }

    /// CAS `authorized`/`failed` -> `provisioning`, binding a reused active
    /// identity or reserving the preallocated identity before KMS work.
    pub async fn attach_identity(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
        identity_id: Uuid,
        identity_generation: i64,
    ) -> Result<ManagedIdentityOperationRecord, ManagedIdentityOperationError> {
        if identity_generation <= 0 {
            return Err(ManagedIdentityOperationError::InvalidRequest);
        }
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let updated = sqlx::query_as::<_, ManagedIdentityOperationRecord>(&format!(
            "UPDATE managed_chat_identity_operations \
             SET identity_id=$3, identity_generation=$4, status='provisioning', updated_at=now() \
             WHERE tenant_id=$1 AND operation_id=$2 AND status IN ('authorized','failed') \
             RETURNING {COLUMNS}"
        ))
        .bind(tenant_id)
        .bind(operation_id)
        .bind(identity_id)
        .bind(identity_generation)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        match updated {
            Some(record) => {
                tx.commit().await.map_err(map_database_error)?;
                Ok(record)
            }
            None => Err(self
                .classify_failed_transition(tenant_id, operation_id, tx)
                .await),
        }
    }

    /// CAS set-if-null of the admission event id while `provisioning`,
    /// stamping admitted_at alongside it; returns whether this call won. A
    /// fixed-format 64-hex event id keeps key material out of the table by
    /// construction.
    pub async fn record_admission_sent(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
        event_id: &str,
    ) -> Result<bool, ManagedIdentityOperationError> {
        if !event_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            || event_id.len() != 64
        {
            return Err(ManagedIdentityOperationError::InvalidRequest);
        }
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let updated = sqlx::query_as::<_, ManagedIdentityOperationRecord>(&format!(
            "UPDATE managed_chat_identity_operations \
             SET admission_event_id=$3, admitted_at=now(), updated_at=now() \
             WHERE tenant_id=$1 AND operation_id=$2 AND status='provisioning' \
               AND admission_event_id IS NULL \
             RETURNING {COLUMNS}"
        ))
        .bind(tenant_id)
        .bind(operation_id)
        .bind(event_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        match updated {
            Some(_) => {
                tx.commit().await.map_err(map_database_error)?;
                Ok(true)
            }
            None => {
                let record = sqlx::query_as::<_, ManagedIdentityOperationRecord>(&format!(
                    "SELECT {COLUMNS} FROM managed_chat_identity_operations \
                     WHERE tenant_id=$1 AND operation_id=$2"
                ))
                .bind(tenant_id)
                .bind(operation_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(map_database_error)?;
                tx.rollback().await.map_err(map_database_error)?;
                match record {
                    None => Err(ManagedIdentityOperationError::NotFound),
                    Some(record) if record.admission_event_id.is_some() => Ok(false),
                    Some(_) => Err(ManagedIdentityOperationError::InvalidTransition),
                }
            }
        }
    }

    /// CAS `provisioning` -> `ready`; the only transition out of the one-active
    /// index by success.
    pub async fn mark_ready(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
    ) -> Result<ManagedIdentityOperationRecord, ManagedIdentityOperationError> {
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let updated = sqlx::query_as::<_, ManagedIdentityOperationRecord>(&format!(
            "UPDATE managed_chat_identity_operations \
             SET status='ready', ready_at=now(), updated_at=now() \
             WHERE tenant_id=$1 AND operation_id=$2 AND status='provisioning' \
             RETURNING {COLUMNS}"
        ))
        .bind(tenant_id)
        .bind(operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        match updated {
            Some(record) => {
                tx.commit().await.map_err(map_database_error)?;
                Ok(record)
            }
            None => Err(self
                .classify_failed_transition(tenant_id, operation_id, tx)
                .await),
        }
    }

    /// CAS any nonterminal status -> `failed`, incrementing attempts and
    /// setting a bounded catalog error code. Never reopens `ready` or
    /// `abandoned` rows; the row stays in the one-active index and resumable.
    pub async fn mark_failed(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
        error_code: &str,
    ) -> Result<ManagedIdentityOperationRecord, ManagedIdentityOperationError> {
        if !MANAGED_IDENTITY_OPERATION_ERROR_CODES.contains(&error_code) {
            return Err(ManagedIdentityOperationError::InvalidRequest);
        }
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let updated = sqlx::query_as::<_, ManagedIdentityOperationRecord>(&format!(
            "UPDATE managed_chat_identity_operations \
             SET status='failed', error_code=$3, attempt_count=attempt_count+1, \
                 last_attempt_at=now(), updated_at=now() \
             WHERE tenant_id=$1 AND operation_id=$2 \
               AND status IN ('authorized','provisioning','failed') \
             RETURNING {COLUMNS}"
        ))
        .bind(tenant_id)
        .bind(operation_id)
        .bind(error_code)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        match updated {
            Some(record) => {
                tx.commit().await.map_err(map_database_error)?;
                Ok(record)
            }
            None => Err(self
                .classify_failed_transition(tenant_id, operation_id, tx)
                .await),
        }
    }

    /// Claim a failed operation for abandonment without releasing its slot.
    pub async fn begin_abandon(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
    ) -> Result<ManagedIdentityOperationRecord, ManagedIdentityOperationError> {
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let updated = sqlx::query_as::<_, ManagedIdentityOperationRecord>(&format!(
            "UPDATE managed_chat_identity_operations \
             SET status='abandoning', updated_at=now() \
             WHERE tenant_id=$1 AND operation_id=$2 AND status='failed' \
             RETURNING {COLUMNS}"
        ))
        .bind(tenant_id)
        .bind(operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        match updated {
            Some(record) => {
                tx.commit().await.map_err(map_database_error)?;
                Ok(record)
            }
            None => Err(self
                .classify_failed_transition(tenant_id, operation_id, tx)
                .await),
        }
    }

    /// CAS `abandoning` -> `abandoned`, releasing the one-active index only
    /// after the provisioner disables any attached never-ready identity.
    pub async fn abandon(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
    ) -> Result<ManagedIdentityOperationRecord, ManagedIdentityOperationError> {
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let updated = sqlx::query_as::<_, ManagedIdentityOperationRecord>(&format!(
            "UPDATE managed_chat_identity_operations \
             SET status='abandoned', updated_at=now() \
             WHERE tenant_id=$1 AND operation_id=$2 AND status='abandoning' \
               AND NOT EXISTS ( \
                   SELECT 1 FROM managed_chat_identities AS identity \
                   WHERE identity.tenant_id=managed_chat_identity_operations.tenant_id \
                     AND identity.principal_id=managed_chat_identity_operations.principal_id \
                     AND identity.identity_id=managed_chat_identity_operations.identity_id \
                     AND identity.generation=managed_chat_identity_operations.identity_generation \
                     AND identity.status='active' \
                     AND NOT EXISTS ( \
                         SELECT 1 FROM managed_chat_identity_operations AS ready_operation \
                         WHERE ready_operation.tenant_id=identity.tenant_id \
                           AND ready_operation.principal_id=identity.principal_id \
                           AND ready_operation.identity_id=identity.identity_id \
                           AND ready_operation.identity_generation=identity.generation \
                           AND ready_operation.status='ready' \
                     ) \
               ) \
             RETURNING {COLUMNS}"
        ))
        .bind(tenant_id)
        .bind(operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        match updated {
            Some(record) => {
                tx.commit().await.map_err(map_database_error)?;
                Ok(record)
            }
            None => Err(self
                .classify_failed_transition(tenant_id, operation_id, tx)
                .await),
        }
    }

    /// In-index operations below their attempt bound, oldest first.
    pub async fn list_retryable(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ManagedIdentityOperationRecord>, ManagedIdentityOperationError> {
        if limit <= 0 {
            return Err(ManagedIdentityOperationError::InvalidRequest);
        }
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let records = sqlx::query_as::<_, ManagedIdentityOperationRecord>(&format!(
            "SELECT {COLUMNS} FROM managed_chat_identity_operations \
             WHERE tenant_id=$1 AND status IN ('authorized','provisioning','failed') \
               AND attempt_count < max_attempts \
             ORDER BY created_at, operation_id LIMIT $2"
        ))
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(records)
    }

    /// Classify a CAS UPDATE that matched no row: the operation is gone
    /// (NotFound) or in a status that rejects the transition (InvalidTransition).
    async fn classify_failed_transition(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
        mut tx: sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> ManagedIdentityOperationError {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM managed_chat_identity_operations \
             WHERE tenant_id=$1 AND operation_id=$2)",
        )
        .bind(tenant_id)
        .bind(operation_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_database_error);
        let _ = tx.rollback().await;
        match exists {
            Ok(true) => ManagedIdentityOperationError::InvalidTransition,
            Ok(false) => ManagedIdentityOperationError::NotFound,
            Err(error) => error,
        }
    }
}

async fn set_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
) -> Result<(), ManagedIdentityOperationError> {
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(map_database_error)?;
    Ok(())
}

fn map_database_error(error: sqlx::Error) -> ManagedIdentityOperationError {
    if matches!(&error, sqlx::Error::Database(database) if database.code().as_deref() == Some("23505"))
    {
        ManagedIdentityOperationError::Conflict
    } else {
        ManagedIdentityOperationError::Database
    }
}
