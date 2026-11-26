use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Acquire, FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ConnectorInstanceRecord {
    pub id: Uuid,
    pub connector_id: String,
    pub descriptor: Value,
    pub source_config: Value,
    pub credential_ref: Option<String>,
    pub cursor: Option<Value>,
    pub mode: String,
    pub lifecycle_state: String,
    pub health: String,
    pub last_success_at: Option<DateTime<Utc>>,
    pub retry_count: i64,
    pub queued_count: i64,
    pub last_error: Option<String>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct ConnectorRegistration<'a> {
    pub tenant_id: Uuid,
    pub principal_id: Uuid,
    pub connector_id: &'a str,
    pub descriptor: &'a Value,
    pub source_config: &'a Value,
    pub credential_ref: Option<&'a str>,
    pub mode: &'a str,
    pub idempotency_key: &'a str,
    pub request_digest: &'a [u8; 32],
}

pub struct ConnectorMutation<'a> {
    pub tenant_id: Uuid,
    pub principal_id: Uuid,
    pub id: Uuid,
    pub expected_version: i64,
    pub source_config: Option<&'a Value>,
    pub credential_ref: Option<Option<&'a str>>,
    pub lifecycle_state: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConnectorMappingProvenance<'a> {
    pub relative_path: Option<&'a str>,
    pub source_observed_modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ConnectorWorkerCredentialIdentity {
    pub credential_id: Uuid,
    pub tenant_id: Uuid,
    pub principal_id: Uuid,
    pub instance_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct AuthenticatedConnectorWorkerRecord {
    #[sqlx(flatten)]
    pub identity: ConnectorWorkerCredentialIdentity,
    #[sqlx(flatten)]
    pub instance: ConnectorInstanceRecord,
}

#[derive(Debug, Clone, FromRow)]
pub struct ConnectorExecutionLease {
    pub lease_id: Uuid,
    pub lease_fence: i64,
    pub lease_expires_at: DateTime<Utc>,
    pub checkpoint_version: i64,
    pub cursor: Option<Value>,
}

pub struct ConnectorExecutionAcknowledgment<'a> {
    pub tenant_id: Uuid,
    pub principal_id: Uuid,
    pub instance_id: Uuid,
    pub lease_id: Uuid,
    pub lease_fence: i64,
    pub expected_checkpoint_version: i64,
    pub cursor: &'a Value,
}

#[derive(Clone)]
pub struct ConnectorStore {
    pool: PgPool,
}

impl ConnectorStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Serialize import and forget for one Connector source revision.
    pub async fn lock_mapping_scope(
        &self,
        tenant_id: Uuid,
        instance_id: Uuid,
        external_resource_ref: &str,
        external_revision: &str,
    ) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, sqlx::Error> {
        let lock_key =
            format!("{tenant_id}:{instance_id}:{external_resource_ref}:{external_revision}");
        let mut transaction = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *transaction)
            .await?;
        Ok(transaction)
    }

    pub async fn register(
        &self,
        request: ConnectorRegistration<'_>,
    ) -> Result<(ConnectorInstanceRecord, bool), sqlx::Error> {
        let inserted = sqlx::query_as::<_, ConnectorInstanceRecord>(
            r#"INSERT INTO connector_instances
                (tenant_id, workspace_id, principal_id, connector_id, descriptor,
                 source_config, credential_ref, mode, idempotency_key, request_digest)
               VALUES ($1, $1, $2, $3, $4, $5, $6, $7, $8, $9)
               ON CONFLICT (tenant_id, principal_id, idempotency_key) DO NOTHING
               RETURNING id, connector_id, descriptor, source_config, credential_ref, cursor, mode,
                         lifecycle_state, health, last_success_at, retry_count, queued_count, last_error,
                         version, created_at, updated_at"#,
        )
        .bind(request.tenant_id)
        .bind(request.principal_id)
        .bind(request.connector_id)
        .bind(request.descriptor)
        .bind(request.source_config)
        .bind(request.credential_ref)
        .bind(request.mode)
        .bind(request.idempotency_key)
        .bind(request.request_digest.as_slice())
        .fetch_optional(&self.pool)
        .await?;

        if let Some(record) = inserted {
            return Ok((record, true));
        }

        let record = sqlx::query_as::<_, ConnectorInstanceRecord>(
            r#"SELECT id, connector_id, descriptor, source_config, credential_ref, cursor, mode,
                      lifecycle_state, health, last_success_at, retry_count, queued_count, last_error,
                      version, created_at, updated_at
               FROM connector_instances
               WHERE tenant_id = $1 AND principal_id = $2 AND idempotency_key = $3
                 AND request_digest = $4"#,
        )
        .bind(request.tenant_id)
        .bind(request.principal_id)
        .bind(request.idempotency_key)
        .bind(request.request_digest.as_slice())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
        Ok((record, false))
    }

    pub async fn get(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ConnectorInstanceRecord>, sqlx::Error> {
        sqlx::query_as::<_, ConnectorInstanceRecord>(
            r#"SELECT id, connector_id, descriptor, source_config, credential_ref, cursor, mode,
                      lifecycle_state, health, last_success_at, retry_count, queued_count, last_error,
                      version, created_at, updated_at
               FROM connector_instances
               WHERE tenant_id = $1 AND workspace_id = $1 AND principal_id = $2 AND id = $3"#,
        )
        .bind(tenant_id)
        .bind(principal_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
    ) -> Result<Vec<ConnectorInstanceRecord>, sqlx::Error> {
        sqlx::query_as::<_, ConnectorInstanceRecord>(
            r#"SELECT id, connector_id, descriptor, source_config, credential_ref, mode,
                      lifecycle_state, health, last_success_at, retry_count, queued_count, last_error,
                      version, created_at, updated_at
               FROM connector_instances
               WHERE tenant_id = $1 AND workspace_id = $1 AND principal_id = $2
               ORDER BY created_at DESC"#,
        )
        .bind(tenant_id)
        .bind(principal_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn mutate(
        &self,
        mutation: ConnectorMutation<'_>,
    ) -> Result<Option<ConnectorInstanceRecord>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query_as::<_, ConnectorInstanceRecord>(
            r#"UPDATE connector_instances
               SET source_config = COALESCE($5, source_config),
                   credential_ref = CASE WHEN $6 THEN $7 ELSE credential_ref END,
                   lifecycle_state = COALESCE($8, lifecycle_state),
                   health = CASE WHEN $8 = 'paused' THEN 'paused'
                                 WHEN $8 = 'disconnected' THEN 'auth_required'
                                 WHEN lifecycle_state = 'paused' AND $8 = 'active' THEN 'healthy'
                                 ELSE health END,
                   disconnected_at = CASE WHEN $8 = 'disconnected' THEN now() ELSE NULL END,
                   lease_id = CASE WHEN $8 IS NOT NULL THEN NULL ELSE lease_id END,
                   lease_expires_at = CASE WHEN $8 IS NOT NULL THEN NULL ELSE lease_expires_at END,
                   lease_fence = lease_fence + CASE WHEN $8 IS NOT NULL THEN 1 ELSE 0 END,
                   retry_count = CASE WHEN lifecycle_state = 'paused' AND $8 = 'active' THEN 0 ELSE retry_count END,
                   next_attempt_at = CASE WHEN lifecycle_state = 'paused' AND $8 = 'active' THEN NULL ELSE next_attempt_at END,
                   dead_lettered_at = CASE WHEN lifecycle_state = 'paused' AND $8 = 'active' THEN NULL ELSE dead_lettered_at END,
                   last_error = CASE WHEN lifecycle_state = 'paused' AND $8 = 'active' THEN NULL ELSE last_error END,
                   version = version + 1,
                   updated_at = now()
               WHERE tenant_id = $1 AND workspace_id = $1 AND principal_id = $2
                 AND id = $3 AND version = $4 AND disconnected_at IS NULL
               RETURNING id, connector_id, descriptor, source_config, credential_ref, cursor, mode,
                         lifecycle_state, health, last_success_at, retry_count, queued_count, last_error,
                         version, created_at, updated_at"#,
        )
        .bind(mutation.tenant_id)
        .bind(mutation.principal_id)
        .bind(mutation.id)
        .bind(mutation.expected_version)
        .bind(mutation.source_config)
        .bind(mutation.credential_ref.is_some())
        .bind(mutation.credential_ref.flatten())
        .bind(mutation.lifecycle_state)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(record) = &updated {
            if record.lifecycle_state == "disconnected" {
                sqlx::query(
                    "UPDATE connector_worker_credentials SET revoked_at = now() WHERE tenant_id = $1 AND instance_id = $2 AND revoked_at IS NULL",
                )
                .bind(mutation.tenant_id)
                .bind(mutation.id)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(updated)
    }

    pub async fn rotate_worker_credential(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
        instance_id: Uuid,
        credential_id: Uuid,
        token_hash: &[u8; 32],
        expires_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let instance = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM connector_instances
               WHERE tenant_id = $1 AND workspace_id = $1 AND principal_id = $2
                 AND id = $3 AND lifecycle_state = 'active' AND disconnected_at IS NULL
               FOR UPDATE"#,
        )
        .bind(tenant_id)
        .bind(principal_id)
        .bind(instance_id)
        .fetch_optional(&mut *tx)
        .await?;
        if instance.is_none() {
            return Err(sqlx::Error::RowNotFound);
        }
        sqlx::query(
            "UPDATE connector_worker_credentials SET revoked_at = now() WHERE tenant_id = $1 AND instance_id = $2 AND revoked_at IS NULL",
        )
        .bind(tenant_id)
        .bind(instance_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO connector_worker_credentials
                   (id, tenant_id, instance_id, principal_id, token_hash, expires_at)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(credential_id)
        .bind(tenant_id)
        .bind(instance_id)
        .bind(principal_id)
        .bind(token_hash.as_slice())
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }

    pub async fn authenticate_worker_credential(
        &self,
        token_hash: &[u8; 32],
    ) -> Result<Option<AuthenticatedConnectorWorkerRecord>, sqlx::Error> {
        sqlx::query_as::<_, AuthenticatedConnectorWorkerRecord>(
            r#"UPDATE connector_worker_credentials c
               SET last_used_at = now()
               FROM connector_instances i
               WHERE c.token_hash = $1 AND c.revoked_at IS NULL AND c.expires_at > now()
                 AND i.tenant_id = c.tenant_id AND i.id = c.instance_id
                 AND i.principal_id = c.principal_id AND i.workspace_id = c.tenant_id
                 AND i.lifecycle_state = 'active' AND i.disconnected_at IS NULL
               RETURNING c.id AS credential_id, c.tenant_id, c.principal_id,
                         c.instance_id, c.expires_at,
                         i.id, i.connector_id, i.descriptor, i.source_config,
                         i.credential_ref, i.cursor, i.mode, i.lifecycle_state, i.health,
                         i.last_success_at, i.retry_count, i.queued_count,
                         i.last_error, i.version, i.created_at, i.updated_at"#,
        )
        .bind(token_hash.as_slice())
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn claim_execution(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
        instance_id: Uuid,
        lease_id: Uuid,
        lease_seconds: i32,
    ) -> Result<Option<ConnectorExecutionLease>, sqlx::Error> {
        sqlx::query_as::<_, ConnectorExecutionLease>(
            r#"UPDATE connector_instances
               SET lease_id = $4, lease_expires_at = now() + make_interval(secs => $5),
                   lease_fence = lease_fence + 1
               WHERE tenant_id = $1 AND workspace_id = $1 AND principal_id = $2 AND id = $3
                 AND lifecycle_state = 'active' AND disconnected_at IS NULL
                 AND health <> 'auth_required' AND dead_lettered_at IS NULL
                 AND (lease_expires_at IS NULL OR lease_expires_at <= now())
                 AND (next_attempt_at IS NULL OR next_attempt_at <= now())
               RETURNING lease_id, lease_fence, lease_expires_at, checkpoint_version, cursor"#,
        )
        .bind(tenant_id)
        .bind(principal_id)
        .bind(instance_id)
        .bind(lease_id)
        .bind(lease_seconds)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn renew_execution(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
        instance_id: Uuid,
        lease_id: Uuid,
        lease_fence: i64,
        lease_seconds: i32,
    ) -> Result<Option<ConnectorExecutionLease>, sqlx::Error> {
        sqlx::query_as::<_, ConnectorExecutionLease>(
            r#"UPDATE connector_instances
               SET lease_expires_at = now() + make_interval(secs => $6)
               WHERE tenant_id = $1 AND workspace_id = $1 AND principal_id = $2 AND id = $3
                 AND lifecycle_state = 'active' AND lease_id = $4 AND lease_fence = $5
                 AND lease_expires_at > now()
               RETURNING lease_id, lease_fence, lease_expires_at, checkpoint_version, cursor"#,
        )
        .bind(tenant_id)
        .bind(principal_id)
        .bind(instance_id)
        .bind(lease_id)
        .bind(lease_fence)
        .bind(lease_seconds)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn acknowledge_execution(
        &self,
        acknowledgment: ConnectorExecutionAcknowledgment<'_>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE connector_instances
               SET cursor = $7, checkpoint_version = checkpoint_version + 1,
                   last_success_at = now(), retry_count = 0, next_attempt_at = NULL,
                   last_error = NULL, health = 'healthy', dead_lettered_at = NULL,
                   lease_id = NULL, lease_expires_at = NULL
               WHERE tenant_id = $1 AND workspace_id = $1 AND principal_id = $2 AND id = $3
                 AND lifecycle_state = 'active' AND lease_id = $4 AND lease_fence = $5
                 AND checkpoint_version = $6 AND lease_expires_at > now()"#,
        )
        .bind(acknowledgment.tenant_id)
        .bind(acknowledgment.principal_id)
        .bind(acknowledgment.instance_id)
        .bind(acknowledgment.lease_id)
        .bind(acknowledgment.lease_fence)
        .bind(acknowledgment.expected_checkpoint_version)
        .bind(acknowledgment.cursor)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn fail_execution(
        &self,
        tenant_id: Uuid,
        principal_id: Uuid,
        instance_id: Uuid,
        lease_id: Uuid,
        lease_fence: i64,
        error_category: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE connector_instances
               SET retry_count = CASE WHEN $6 IN ('authentication', 'invalid_content', 'policy_denied')
                                      THEN retry_count ELSE retry_count + 1 END,
                   health = CASE WHEN $6 = 'authentication' THEN 'auth_required'
                                 WHEN $6 = 'rate_limit' THEN 'rate_limited'
                                 WHEN $6 IN ('invalid_content', 'policy_denied') THEN 'failed'
                                 WHEN retry_count >= 9 THEN 'failed'
                                 ELSE 'degraded' END,
                   next_attempt_at = CASE
                       WHEN $6 IN ('authentication', 'invalid_content', 'policy_denied') OR retry_count >= 9 THEN NULL
                       ELSE now() + make_interval(secs => LEAST(3600, (5 * power(2, LEAST(retry_count, 9)))::int))
                   END,
                   dead_lettered_at = CASE
                       WHEN $6 IN ('invalid_content', 'policy_denied') OR retry_count >= 9 THEN now()
                       ELSE NULL
                   END,
                   last_error = $6, lease_id = NULL, lease_expires_at = NULL
               WHERE tenant_id = $1 AND workspace_id = $1 AND principal_id = $2 AND id = $3
                 AND lifecycle_state = 'active' AND lease_id = $4 AND lease_fence = $5
                 AND lease_expires_at > now()"#,
        )
        .bind(tenant_id)
        .bind(principal_id)
        .bind(instance_id)
        .bind(lease_id)
        .bind(lease_fence)
        .bind(error_category)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn record_mapping(
        &self,
        tenant_id: Uuid,
        instance_id: Uuid,
        external_resource_ref: &str,
        external_revision: &str,
        target_resource_ref: &str,
        provenance: ConnectorMappingProvenance<'_>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"INSERT INTO connector_external_mappings
                   (tenant_id, instance_id, external_resource_ref, external_revision, target_resource_ref, relative_path, source_observed_modified_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               ON CONFLICT (tenant_id, instance_id, external_resource_ref, external_revision)
               DO UPDATE SET relative_path = COALESCE(EXCLUDED.relative_path, connector_external_mappings.relative_path),
                             source_observed_modified_at = COALESCE(EXCLUDED.source_observed_modified_at, connector_external_mappings.source_observed_modified_at)
               WHERE connector_external_mappings.target_resource_ref = EXCLUDED.target_resource_ref
                 AND (connector_external_mappings.relative_path IS DISTINCT FROM EXCLUDED.relative_path
                   OR connector_external_mappings.source_observed_modified_at IS DISTINCT FROM EXCLUDED.source_observed_modified_at)"#,
        )
        .bind(tenant_id)
        .bind(instance_id)
        .bind(external_resource_ref)
        .bind(external_revision)
        .bind(target_resource_ref)
        .bind(provenance.relative_path)
        .bind(provenance.source_observed_modified_at)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 1 {
            return Ok(true);
        }
        let existing: Option<Option<String>> = sqlx::query_scalar(
            r#"SELECT target_resource_ref FROM connector_external_mappings
               WHERE tenant_id = $1 AND instance_id = $2 AND external_resource_ref = $3 AND external_revision = $4"#,
        )
        .bind(tenant_id)
        .bind(instance_id)
        .bind(external_resource_ref)
        .bind(external_revision)
        .fetch_optional(&self.pool)
        .await?;
        if existing.as_ref().and_then(Option::as_deref) == Some(target_resource_ref) {
            Ok(false)
        } else {
            Err(sqlx::Error::RowNotFound)
        }
    }

    pub async fn mapped_target(
        &self,
        tenant_id: Uuid,
        instance_id: Uuid,
        external_resource_ref: &str,
        external_revision: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT target_resource_ref FROM connector_external_mappings
               WHERE tenant_id = $1 AND instance_id = $2 AND external_resource_ref = $3
                 AND external_revision = $4 AND target_resource_ref IS NOT NULL"#,
        )
        .bind(tenant_id)
        .bind(instance_id)
        .bind(external_resource_ref)
        .bind(external_revision)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn mapping_forgotten(
        &self,
        tenant_id: Uuid,
        instance_id: Uuid,
        external_resource_ref: &str,
        external_revision: &str,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT forgotten_at IS NOT NULL FROM connector_external_mappings
               WHERE tenant_id = $1 AND instance_id = $2
                 AND external_resource_ref = $3 AND external_revision = $4"#,
        )
        .bind(tenant_id)
        .bind(instance_id)
        .bind(external_resource_ref)
        .bind(external_revision)
        .fetch_optional(&self.pool)
        .await
        .map(|forgotten| forgotten.unwrap_or(false))
    }

    pub async fn mapping_forget_requested_at(
        &self,
        tenant_id: Uuid,
        instance_id: Uuid,
        external_resource_ref: &str,
        external_revision: &str,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT forget_requested_at FROM connector_external_mappings
               WHERE tenant_id = $1 AND instance_id = $2
                 AND external_resource_ref = $3 AND external_revision = $4"#,
        )
        .bind(tenant_id)
        .bind(instance_id)
        .bind(external_resource_ref)
        .bind(external_revision)
        .fetch_optional(&self.pool)
        .await
        .map(Option::flatten)
    }

    pub async fn mark_mapping_forgotten(
        &self,
        tenant_id: Uuid,
        instance_id: Uuid,
        external_resource_ref: &str,
        external_revision: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE connector_external_mappings SET forgotten_at = COALESCE(forgotten_at, now())
               WHERE tenant_id = $1 AND instance_id = $2
                 AND external_resource_ref = $3 AND external_revision = $4"#,
        )
        .bind(tenant_id)
        .bind(instance_id)
        .bind(external_resource_ref)
        .bind(external_revision)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn request_mapping_forget(
        &self,
        tenant_id: Uuid,
        instance_id: Uuid,
        external_resource_ref: &str,
        external_revision: &str,
        requested_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"INSERT INTO connector_external_mappings
                 (tenant_id, instance_id, external_resource_ref, external_revision,
                  target_resource_ref, forget_requested_at, forgotten_at)
               VALUES ($1, $2, $3, $4, NULL, $5, $5)
               ON CONFLICT (tenant_id, instance_id, external_resource_ref, external_revision)
               DO UPDATE SET forget_requested_at = COALESCE(connector_external_mappings.forget_requested_at, EXCLUDED.forget_requested_at)
               WHERE connector_external_mappings.target_resource_ref IS NOT NULL
                 AND connector_external_mappings.forgotten_at IS NULL"#,
        )
        .bind(tenant_id)
        .bind(instance_id)
        .bind(external_resource_ref)
        .bind(external_revision)
        .bind(requested_at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}
