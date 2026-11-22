use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ConnectorInstanceRecord {
    pub id: Uuid,
    pub connector_id: String,
    pub descriptor: Value,
    pub source_config: Value,
    pub credential_ref: Option<String>,
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

#[derive(Clone)]
pub struct ConnectorStore {
    pool: PgPool,
}

impl ConnectorStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
               RETURNING id, connector_id, descriptor, source_config, credential_ref, mode,
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
            r#"SELECT id, connector_id, descriptor, source_config, credential_ref, mode,
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
            r#"SELECT id, connector_id, descriptor, source_config, credential_ref, mode,
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
        sqlx::query_as::<_, ConnectorInstanceRecord>(
            r#"UPDATE connector_instances
               SET source_config = COALESCE($5, source_config),
                   credential_ref = CASE WHEN $6 THEN $7 ELSE credential_ref END,
                   lifecycle_state = COALESCE($8, lifecycle_state),
                   health = CASE WHEN $8 = 'paused' THEN 'paused'
                                 WHEN $8 = 'disconnected' THEN 'auth_required'
                                 WHEN lifecycle_state = 'paused' AND $8 = 'active' THEN 'healthy'
                                 ELSE health END,
                   disconnected_at = CASE WHEN $8 = 'disconnected' THEN now() ELSE NULL END,
                   version = version + 1,
                   updated_at = now()
               WHERE tenant_id = $1 AND workspace_id = $1 AND principal_id = $2
                 AND id = $3 AND version = $4 AND disconnected_at IS NULL
               RETURNING id, connector_id, descriptor, source_config, credential_ref, mode,
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
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn record_mapping(
        &self,
        tenant_id: Uuid,
        instance_id: Uuid,
        external_resource_ref: &str,
        external_revision: &str,
        target_resource_ref: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"INSERT INTO connector_external_mappings
                   (tenant_id, instance_id, external_resource_ref, external_revision, target_resource_ref)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (tenant_id, instance_id, external_resource_ref, external_revision) DO NOTHING"#,
        )
        .bind(tenant_id)
        .bind(instance_id)
        .bind(external_resource_ref)
        .bind(external_revision)
        .bind(target_resource_ref)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 1 {
            return Ok(true);
        }
        let existing: Option<String> = sqlx::query_scalar(
            r#"SELECT target_resource_ref FROM connector_external_mappings
               WHERE tenant_id = $1 AND instance_id = $2 AND external_resource_ref = $3 AND external_revision = $4"#,
        )
        .bind(tenant_id)
        .bind(instance_id)
        .bind(external_resource_ref)
        .bind(external_revision)
        .fetch_optional(&self.pool)
        .await?;
        if existing.as_deref() == Some(target_resource_ref) {
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
               WHERE tenant_id = $1 AND instance_id = $2 AND external_resource_ref = $3 AND external_revision = $4"#,
        )
        .bind(tenant_id)
        .bind(instance_id)
        .bind(external_resource_ref)
        .bind(external_revision)
        .fetch_optional(&self.pool)
        .await
    }
}
