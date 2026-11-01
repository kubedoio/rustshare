//! Tenant-scoped persistence for inert managed Nostr identity envelopes.

use rustshare_crypto::managed_identity_vault::{
    IdentityContext, ManagedIdentityEnvelope, WrappedDek, ENVELOPE_VERSION,
};
use sqlx::{FromRow, PgPool};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedIdentityStatus {
    Active,
    Disabled,
    Revoked,
}

impl ManagedIdentityStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ManagedIdentityStoreError {
    #[error("managed identity storage failed")]
    Database,
    #[error("an active managed identity already exists")]
    Conflict,
    #[error("managed identity was not found or is not active")]
    NotFound,
    #[error("managed identity envelope is invalid")]
    InvalidEnvelope,
}

#[derive(Clone, Debug, FromRow)]
pub struct ManagedIdentityRecord {
    pub identity_id: Uuid,
    pub tenant_id: Uuid,
    pub principal_id: Uuid,
    pub generation: i64,
    pub idempotency_key: Uuid,
    pub public_key: String,
    pub envelope_version: i16,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub wrapped_dek: Vec<u8>,
    pub kms_key_id: String,
    pub kms_key_version: String,
    pub status: String,
}

impl ManagedIdentityRecord {
    pub fn envelope(&self) -> Result<ManagedIdentityEnvelope, ManagedIdentityStoreError> {
        let version = u16::try_from(self.envelope_version)
            .map_err(|_| ManagedIdentityStoreError::InvalidEnvelope)?;
        let nonce = self
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_| ManagedIdentityStoreError::InvalidEnvelope)?;
        if version != ENVELOPE_VERSION || self.ciphertext.len() != 48 {
            return Err(ManagedIdentityStoreError::InvalidEnvelope);
        }
        Ok(ManagedIdentityEnvelope {
            context: IdentityContext {
                tenant_id: self.tenant_id,
                principal_id: self.principal_id,
                identity_id: self.identity_id,
                generation: self.generation,
            },
            public_key: self.public_key.clone(),
            version,
            nonce,
            ciphertext: self.ciphertext.clone(),
            wrapped_dek: WrappedDek {
                key_id: self.kms_key_id.clone(),
                key_version: self.kms_key_version.clone(),
                ciphertext: self.wrapped_dek.clone(),
            },
        })
    }
}

/// Database operations for the Phase 2A vault. No runtime caller is wired.
#[derive(Clone)]
pub struct ManagedIdentityVaultStore(PgPool);

impl ManagedIdentityVaultStore {
    pub fn new(pool: PgPool) -> Self {
        Self(pool)
    }

    /// Insert once per tenant/Principal/idempotency key. Concurrent retries
    /// return the committed winner; a second active identity conflicts.
    pub async fn insert_or_load(
        &self,
        idempotency_key: Uuid,
        envelope: &ManagedIdentityEnvelope,
    ) -> Result<ManagedIdentityRecord, ManagedIdentityStoreError> {
        if envelope.context.generation <= 0
            || envelope.version != ENVELOPE_VERSION
            || envelope.ciphertext.len() != 48
            || envelope.public_key.len() != 64
            || !envelope
                .public_key
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(ManagedIdentityStoreError::InvalidEnvelope);
        }
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, envelope.context.tenant_id).await?;
        let inserted = sqlx::query_as::<_, ManagedIdentityRecord>(
            "INSERT INTO managed_chat_identities \
             (identity_id, tenant_id, principal_id, generation, idempotency_key, public_key, \
              envelope_version, nonce, ciphertext, wrapped_dek, kms_key_id, kms_key_version) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) \
             ON CONFLICT (tenant_id, principal_id, idempotency_key) DO NOTHING \
             RETURNING identity_id, tenant_id, principal_id, generation, idempotency_key, \
              public_key, envelope_version, nonce, ciphertext, wrapped_dek, kms_key_id, \
              kms_key_version, status",
        )
        .bind(envelope.context.identity_id)
        .bind(envelope.context.tenant_id)
        .bind(envelope.context.principal_id)
        .bind(envelope.context.generation)
        .bind(idempotency_key)
        .bind(&envelope.public_key)
        .bind(
            i16::try_from(envelope.version)
                .map_err(|_| ManagedIdentityStoreError::InvalidEnvelope)?,
        )
        .bind(envelope.nonce.as_slice())
        .bind(&envelope.ciphertext)
        .bind(&envelope.wrapped_dek.ciphertext)
        .bind(&envelope.wrapped_dek.key_id)
        .bind(&envelope.wrapped_dek.key_version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;

        let record = match inserted {
            Some(record) => record,
            None => sqlx::query_as::<_, ManagedIdentityRecord>(
                "SELECT identity_id, tenant_id, principal_id, generation, idempotency_key, \
                 public_key, envelope_version, nonce, ciphertext, wrapped_dek, kms_key_id, \
                 kms_key_version, status FROM managed_chat_identities \
                 WHERE tenant_id=$1 AND principal_id=$2 AND idempotency_key=$3",
            )
            .bind(envelope.context.tenant_id)
            .bind(envelope.context.principal_id)
            .bind(idempotency_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(map_database_error)?
            .ok_or(ManagedIdentityStoreError::Conflict)?,
        };
        tx.commit().await.map_err(map_database_error)?;
        Ok(record)
    }

    pub async fn get(
        &self,
        tenant_id: Uuid,
        identity_id: Uuid,
    ) -> Result<ManagedIdentityRecord, ManagedIdentityStoreError> {
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let record = sqlx::query_as::<_, ManagedIdentityRecord>(
            "SELECT identity_id, tenant_id, principal_id, generation, idempotency_key, \
             public_key, envelope_version, nonce, ciphertext, wrapped_dek, kms_key_id, \
             kms_key_version, status FROM managed_chat_identities \
             WHERE tenant_id=$1 AND identity_id=$2",
        )
        .bind(tenant_id)
        .bind(identity_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?
        .ok_or(ManagedIdentityStoreError::NotFound)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(record)
    }

    /// Persist KMS rewrapping while matching the immutable identity envelope.
    pub async fn persist_rewrapped_dek(
        &self,
        envelope: &ManagedIdentityEnvelope,
    ) -> Result<(), ManagedIdentityStoreError> {
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, envelope.context.tenant_id).await?;
        let result = sqlx::query(
            "UPDATE managed_chat_identities SET wrapped_dek=$4, kms_key_id=$5, \
             kms_key_version=$6, updated_at=now() \
             WHERE tenant_id=$1 AND identity_id=$2 AND public_key=$3 \
               AND envelope_version=$7 AND nonce=$8 AND ciphertext=$9 \
               AND principal_id=$10 AND generation=$11",
        )
        .bind(envelope.context.tenant_id)
        .bind(envelope.context.identity_id)
        .bind(&envelope.public_key)
        .bind(&envelope.wrapped_dek.ciphertext)
        .bind(&envelope.wrapped_dek.key_id)
        .bind(&envelope.wrapped_dek.key_version)
        .bind(
            i16::try_from(envelope.version)
                .map_err(|_| ManagedIdentityStoreError::InvalidEnvelope)?,
        )
        .bind(envelope.nonce.as_slice())
        .bind(&envelope.ciphertext)
        .bind(envelope.context.principal_id)
        .bind(envelope.context.generation)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        if result.rows_affected() != 1 {
            return Err(ManagedIdentityStoreError::NotFound);
        }
        tx.commit().await.map_err(map_database_error)
    }

    pub async fn disable_or_revoke(
        &self,
        tenant_id: Uuid,
        identity_id: Uuid,
        status: ManagedIdentityStatus,
    ) -> Result<(), ManagedIdentityStoreError> {
        if status == ManagedIdentityStatus::Active {
            return Err(ManagedIdentityStoreError::InvalidEnvelope);
        }
        let mut tx = self.0.begin().await.map_err(map_database_error)?;
        set_tenant(&mut tx, tenant_id).await?;
        let result = sqlx::query(
            "UPDATE managed_chat_identities SET status=$3, updated_at=now() \
             WHERE tenant_id=$1 AND identity_id=$2 AND status='active'",
        )
        .bind(tenant_id)
        .bind(identity_id)
        .bind(status.as_str())
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        if result.rows_affected() != 1 {
            return Err(ManagedIdentityStoreError::NotFound);
        }
        tx.commit().await.map_err(map_database_error)
    }
}

async fn set_tenant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
) -> Result<(), ManagedIdentityStoreError> {
    sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
        .bind(tenant_id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(map_database_error)?;
    Ok(())
}

fn map_database_error(error: sqlx::Error) -> ManagedIdentityStoreError {
    if matches!(&error, sqlx::Error::Database(database) if database.code().as_deref() == Some("23505"))
    {
        ManagedIdentityStoreError::Conflict
    } else {
        ManagedIdentityStoreError::Database
    }
}
