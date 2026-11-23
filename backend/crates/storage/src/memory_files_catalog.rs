//! Memory-owned Files source provenance and citation-version catalog.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

type ConnectorMappingRow = (Uuid, String, String, Option<String>, Option<DateTime<Utc>>);

#[derive(Debug, Clone)]
pub struct MemoryFilesCatalogRecord {
    pub tenant_id: Uuid,
    pub file_id: Uuid,
    pub current_version: String,
    pub current_version_number: i64,
    pub connector_instance_id: Uuid,
    pub source_resource_ref: String,
    pub source_revision: String,
    pub relative_path: String,
    pub observed_modified_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct MemoryFilesCatalogStore {
    pool: PgPool,
}

impl MemoryFilesCatalogStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Append an idempotent, content-free audit record for a Memory source
    /// fetch attempt.
    pub async fn record_source_access(
        &self,
        access_id: Uuid,
        tenant_id: Uuid,
        file_id: Uuid,
        principal_id: Uuid,
        file_version: &str,
        authorized: bool,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO memory_files_source_access_audit (access_id, tenant_id, file_id, principal_id, file_version, outcome) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (access_id) DO UPDATE SET outcome=EXCLUDED.outcome, occurred_at=NOW()",
        )
        .bind(access_id)
        .bind(tenant_id)
        .bind(file_id)
        .bind(principal_id)
        .bind(file_version)
        .bind(if authorized { "authorized" } else { "denied_or_unavailable" })
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Insert or refresh one Files projection row. A lower Files version is
    /// ignored; equal-version delivery refreshes metadata idempotently.
    pub async fn upsert_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        record: &MemoryFilesCatalogRecord,
    ) -> Result<bool> {
        let row = sqlx::query(
            "INSERT INTO memory_files_catalog (tenant_id, file_id, current_version, current_version_number, connector_instance_id, source_resource_ref, source_revision, relative_path, observed_modified_at, deleted_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NULL) ON CONFLICT (tenant_id, file_id) DO UPDATE SET current_version = EXCLUDED.current_version, current_version_number = EXCLUDED.current_version_number, connector_instance_id = EXCLUDED.connector_instance_id, source_resource_ref = EXCLUDED.source_resource_ref, source_revision = EXCLUDED.source_revision, relative_path = EXCLUDED.relative_path, observed_modified_at = EXCLUDED.observed_modified_at, deleted_at = NULL, updated_at = NOW() WHERE COALESCE(memory_files_catalog.current_version_number, 0) <= EXCLUDED.current_version_number RETURNING file_id",
        )
        .bind(record.tenant_id)
        .bind(record.file_id)
        .bind(&record.current_version)
        .bind(record.current_version_number)
        .bind(record.connector_instance_id)
        .bind(&record.source_resource_ref)
        .bind(&record.source_revision)
        .bind(&record.relative_path)
        .bind(record.observed_modified_at)
        .fetch_optional(&mut **tx)
        .await?;
        Ok(row.is_some())
    }

    /// Refresh only the Files-side citation version during reconciliation;
    /// Connector source identity and revision remain separate provenance.
    pub async fn refresh_current_version_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        tenant_id: Uuid,
        file_id: Uuid,
        current_version: &str,
        current_version_number: i64,
    ) -> Result<bool> {
        let row = sqlx::query(
            "UPDATE memory_files_catalog SET current_version=$3, current_version_number=$4, updated_at=NOW() \
             WHERE tenant_id=$1 AND file_id=$2 AND deleted_at IS NULL \
               AND current_version_number <= $4 RETURNING file_id",
        )
        .bind(tenant_id)
        .bind(file_id)
        .bind(current_version)
        .bind(current_version_number)
        .fetch_optional(&mut **tx)
        .await?;
        Ok(row.is_some())
    }

    /// Current immutable Files version, omitted for tombstoned or unknown files.
    pub async fn version(&self, tenant_id: Uuid, file_id: Uuid) -> Result<Option<String>> {
        sqlx::query_scalar(
            "SELECT current_version FROM memory_files_catalog WHERE tenant_id = $1 AND file_id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(file_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Whether Memory owns a catalog row for this Files source, including a tombstone.
    pub async fn contains(&self, tenant_id: Uuid, file_id: Uuid) -> Result<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2)",
        )
        .bind(tenant_id)
        .bind(file_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Current immutable versions for live Files projections in one tenant.
    pub async fn versions(
        &self,
        tenant_id: Uuid,
        file_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, String>> {
        let rows: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT file_id, current_version FROM memory_files_catalog \
             WHERE tenant_id=$1 AND file_id=ANY($2) AND deleted_at IS NULL AND current_version IS NOT NULL",
        )
        .bind(tenant_id)
        .bind(file_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// Rebuild the latest Connector relationship for a live File from the
    /// authoritative mapping, even when Files has a newer manual edit.
    pub async fn connector_record_for_current_file(
        &self,
        tenant_id: Uuid,
        file_id: Uuid,
        current_version: String,
        current_version_number: i64,
    ) -> Result<Option<MemoryFilesCatalogRecord>> {
        let row: Option<ConnectorMappingRow> = sqlx::query_as(
            "SELECT instance_id, external_resource_ref, external_revision, relative_path, source_observed_modified_at \
             FROM connector_external_mappings \
             WHERE tenant_id=$1 \
               AND split_part(split_part(target_resource_ref, '/file/', 2), '?', 1)=$2 \
             ORDER BY observed_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .bind(file_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(
            |(
                connector_instance_id,
                source_resource_ref,
                source_revision,
                relative_path,
                observed_modified_at,
            )| MemoryFilesCatalogRecord {
                tenant_id,
                file_id,
                current_version,
                current_version_number,
                connector_instance_id,
                source_resource_ref,
                source_revision,
                relative_path: relative_path.unwrap_or_default(),
                observed_modified_at,
            },
        ))
    }

    /// Tombstone catalog references whose Files source is no longer live.
    pub async fn tombstone_missing_files(&self, tenant_id: Uuid) -> Result<Vec<Uuid>> {
        sqlx::query_scalar(
            "UPDATE memory_files_catalog m SET deleted_at=COALESCE(m.deleted_at,NOW()), updated_at=NOW() \
             WHERE m.tenant_id=$1 AND m.deleted_at IS NULL \
               AND NOT EXISTS (SELECT 1 FROM files f WHERE f.id=m.file_id AND f.tenant_id=m.tenant_id AND f.deleted_at IS NULL) \
             RETURNING m.file_id",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// Tombstone a Files reference while retaining its source provenance for audit.
    /// Inserting a placeholder also fences a delete delivered before its create/update.
    pub async fn tombstone_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        tenant_id: Uuid,
        file_id: Uuid,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO memory_files_catalog (tenant_id, file_id, deleted_at) VALUES ($1, $2, NOW()) ON CONFLICT (tenant_id, file_id) DO UPDATE SET deleted_at = COALESCE(memory_files_catalog.deleted_at, NOW()), updated_at = NOW()",
        )
        .bind(tenant_id)
        .bind(file_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}
