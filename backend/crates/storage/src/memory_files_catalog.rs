//! Memory-owned Files source provenance and citation-version catalog.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

type ConnectorMappingRow = (
    Uuid,
    String,
    String,
    Option<String>,
    Option<DateTime<Utc>>,
    DateTime<Utc>,
);

#[derive(Debug, Clone)]
pub struct MemoryFilesCatalogRecord {
    pub tenant_id: Uuid,
    pub file_id: Uuid,
    pub projection_observed_at: DateTime<Utc>,
    pub current_version: String,
    pub current_version_number: i64,
    pub connector_instance_id: Uuid,
    pub source_resource_ref: String,
    pub source_revision: String,
    pub relative_path: String,
    pub observed_modified_at: Option<DateTime<Utc>>,
}

/// A Files-backed Connector publication eligible for the Memory catalog UI.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MemoryConnectorCatalogRecord {
    pub tenant_id: Uuid,
    pub file_id: Uuid,
    pub current_version: String,
    pub connector_instance_id: Uuid,
    pub source_resource_ref: String,
    pub source_revision: String,
    pub relative_path: String,
    pub updated_at: DateTime<Utc>,
    pub indexed: bool,
}

#[derive(Clone)]
pub struct MemoryFilesCatalogStore {
    pool: PgPool,
}

impl MemoryFilesCatalogStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn projection_timestamp(&self) -> Result<DateTime<Utc>> {
        sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    /// List live Connector publications in one tenant. Files remains the
    /// source authority; callers must reauthorize each Files resource.
    pub async fn list_connector_records(
        &self,
        tenant_id: Uuid,
        limit: usize,
    ) -> Result<Vec<MemoryConnectorCatalogRecord>> {
        sqlx::query_as(
            "SELECT c.tenant_id, c.file_id, c.current_version, c.connector_instance_id,
                    c.source_resource_ref, c.source_revision, c.relative_path, c.updated_at,
                    EXISTS (SELECT 1 FROM note_index_chunks i
                            WHERE i.tenant_id=c.tenant_id AND i.source_file_id=c.file_id
                              AND i.indexed_at >= c.updated_at) AS indexed
             FROM memory_files_catalog c
             WHERE c.tenant_id=$1 AND c.deleted_at IS NULL
               AND c.current_version IS NOT NULL AND c.connector_instance_id IS NOT NULL
             ORDER BY c.updated_at DESC, c.file_id ASC LIMIT $2",
        )
        .bind(tenant_id)
        .bind(limit.clamp(1, 500) as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// File IDs remain reconciliation candidates when their Connector mapping
    /// is forgotten, so reconciliation can tombstone an older catalog/index
    /// projection. The current-source lookup below excludes forgotten maps.
    pub async fn mapped_file_ids_for_reconcile(
        &self,
        tenant_id: Uuid,
        after_file_id: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<Uuid>> {
        sqlx::query_scalar(
            "WITH mapped AS (SELECT CASE WHEN split_part(split_part(target_resource_ref, '/file/', 2), '?', 1) ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN split_part(split_part(target_resource_ref, '/file/', 2), '?', 1)::uuid END AS file_id FROM connector_external_mappings WHERE tenant_id=$1 AND target_resource_ref IS NOT NULL) SELECT DISTINCT file_id FROM mapped WHERE file_id IS NOT NULL AND ($2::uuid IS NULL OR file_id > $2) ORDER BY file_id LIMIT $3",
        )
        .bind(tenant_id)
        .bind(after_file_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
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

    /// Record a content-free retrieval access under its Ask/Search request.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_retrieval_access(
        &self,
        request_id: Uuid,
        tenant_id: Uuid,
        file_id: Option<Uuid>,
        principal_id: Uuid,
        principal_kind: &str,
        initiator_principal_id: Uuid,
        resource_ref: &str,
        source_version: &str,
        owner_application: &str,
        outcome: &str,
        context_entered: bool,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO memory_files_source_access_audit \
             (access_id, tenant_id, file_id, principal_id, file_version, outcome, request_id, principal_kind, initiator_principal_id, source_resource_ref, owner_application, context_entered) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
             ON CONFLICT (request_id, source_resource_ref) WHERE request_id IS NOT NULL AND source_resource_ref IS NOT NULL \
             DO UPDATE SET outcome=EXCLUDED.outcome, context_entered=EXCLUDED.context_entered, occurred_at=NOW()",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(file_id)
        .bind(principal_id)
        .bind(source_version)
        .bind(outcome)
        .bind(request_id)
        .bind(principal_kind)
        .bind(initiator_principal_id)
        .bind(resource_ref)
        .bind(owner_application)
        .bind(context_entered)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Record which successful search sources reached the request's LLM prompt.
    pub async fn finalize_retrieval_context(
        &self,
        request_id: Uuid,
        searched_refs: &[String],
        entered_refs: &[String],
    ) -> Result<()> {
        if !searched_refs.is_empty() {
            let recorded: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM memory_files_source_access_audit WHERE request_id=$1 AND source_resource_ref=ANY($2)",
            )
            .bind(request_id)
            .bind(searched_refs)
            .fetch_one(&self.pool)
            .await?;
            anyhow::ensure!(
                recorded as usize == searched_refs.len(),
                "retrieval audit rows do not match searched sources"
            );
        }
        if !entered_refs.is_empty() {
            let entered = sqlx::query(
                "UPDATE memory_files_source_access_audit SET context_entered=TRUE, occurred_at=NOW() \
                 WHERE request_id=$1 AND source_resource_ref=ANY($2)",
            )
            .bind(request_id)
            .bind(entered_refs)
            .execute(&self.pool)
            .await?;
            anyhow::ensure!(
                entered.rows_affected() as usize == entered_refs.len(),
                "retrieval audit rows do not match materialized sources"
            );
        }
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
            "INSERT INTO memory_files_catalog (tenant_id, file_id, current_version, current_version_number, connector_instance_id, source_resource_ref, source_revision, relative_path, observed_modified_at, deleted_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NULL, $10) ON CONFLICT (tenant_id, file_id) DO UPDATE SET current_version = EXCLUDED.current_version, current_version_number = EXCLUDED.current_version_number, connector_instance_id = EXCLUDED.connector_instance_id, source_resource_ref = EXCLUDED.source_resource_ref, source_revision = EXCLUDED.source_revision, relative_path = EXCLUDED.relative_path, observed_modified_at = EXCLUDED.observed_modified_at, deleted_at = NULL, updated_at = EXCLUDED.updated_at WHERE COALESCE(memory_files_catalog.current_version_number, 0) <= EXCLUDED.current_version_number AND memory_files_catalog.updated_at < EXCLUDED.updated_at RETURNING file_id",
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
        .bind(record.projection_observed_at)
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
            "SELECT instance_id, external_resource_ref, external_revision, relative_path, source_observed_modified_at, clock_timestamp() \
             FROM connector_external_mappings \
             WHERE tenant_id=$1 \
               AND target_resource_ref IS NOT NULL \
               AND forget_requested_at IS NULL \
               AND forgotten_at IS NULL \
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
                projection_observed_at,
            )| MemoryFilesCatalogRecord {
                tenant_id,
                file_id,
                projection_observed_at,
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

    /// Apply a source removal only if no newer projection has been committed.
    pub async fn tombstone_snapshot_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        tenant_id: Uuid,
        file_id: Uuid,
        observed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool> {
        let result = sqlx::query(
            "INSERT INTO memory_files_catalog (tenant_id, file_id, deleted_at, updated_at) VALUES ($1, $2, $3, $3) \
             ON CONFLICT (tenant_id, file_id) DO UPDATE SET deleted_at = COALESCE(memory_files_catalog.deleted_at, EXCLUDED.deleted_at), updated_at = EXCLUDED.updated_at \
             WHERE memory_files_catalog.updated_at < EXCLUDED.updated_at",
        )
        .bind(tenant_id)
        .bind(file_id)
        .bind(observed_at)
        .execute(&mut **tx)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires migrated PostgreSQL"]
    async fn forgotten_mapping_is_not_live_but_remains_a_reconcile_cleanup_candidate() {
        let pool =
            PgPool::connect(&std::env::var("DATABASE_URL").expect("set disposable DATABASE_URL"))
                .await
                .expect("connect to disposable PostgreSQL");
        let store = MemoryFilesCatalogStore::new(pool.clone());
        let fixture = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let instance_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let source_ref = format!("elembra://io.elembra.connector.shell-memory/file/{fixture}");
        let target_ref =
            format!("elembra://io.elembra.files/file/{file_id}?version=sha256:forgotten");
        sqlx::query("INSERT INTO tenants (id,name) VALUES ($1,$2)")
            .bind(tenant_id)
            .bind(format!("memory-forget-{fixture}"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO users (id,username,email,password_hash,tenant_id,display_name,storage_quota) VALUES ($1,$2,$3,'hash',$4,'owner',10737418240)")
            .bind(owner_id)
            .bind(format!("memory-forget-{fixture}"))
            .bind(format!("memory-forget-{fixture}@example.test"))
            .bind(tenant_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO connector_instances (id,tenant_id,workspace_id,principal_id,connector_id,descriptor,source_config,mode,idempotency_key,request_digest) VALUES ($1,$2,$2,$3,'io.elembra.connector.shell-memory',$4,'{}','import',$5,$6)")
            .bind(instance_id)
            .bind(tenant_id)
            .bind(owner_id)
            .bind(serde_json::json!({"metadata":{"id":"io.elembra.connector.shell-memory"}}))
            .bind(format!("memory-forget-{fixture}"))
            .bind([0_u8; 32].as_slice())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO files (id,name,path,size,mime_type,content_hash,storage_key,owner_id,tenant_id,current_version) VALUES ($1,'restored.md',$2,1,'text/markdown','forgotten','key',$3,$4,1)")
            .bind(file_id)
            .bind(format!("/memory-forget-{fixture}.md"))
            .bind(owner_id)
            .bind(tenant_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO connector_external_mappings (tenant_id,instance_id,external_resource_ref,external_revision,target_resource_ref,forgotten_at) VALUES ($1,$2,$3,'sha256:forgotten',$4,now())")
            .bind(tenant_id)
            .bind(instance_id)
            .bind(&source_ref)
            .bind(&target_ref)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO connector_external_mappings (tenant_id,instance_id,external_resource_ref,external_revision,target_resource_ref,forget_requested_at,forgotten_at) VALUES ($1,$2,$3,'sha256:never-imported',NULL,now(),now())")
            .bind(tenant_id)
            .bind(instance_id)
            .bind(format!("elembra://io.elembra.connector.shell-memory/file/{}", Uuid::new_v4()))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO memory_files_catalog (tenant_id,file_id,current_version,current_version_number,connector_instance_id,source_resource_ref,source_revision,relative_path) VALUES ($1,$2,'sha256:forgotten',1,$3,$4,'sha256:forgotten','restored.md')")
            .bind(tenant_id)
            .bind(file_id)
            .bind(instance_id)
            .bind(&source_ref)
            .execute(&pool)
            .await
            .unwrap();

        let candidate_ids = store
            .mapped_file_ids_for_reconcile(tenant_id, None, 10)
            .await
            .unwrap();
        assert_eq!(candidate_ids, vec![file_id]);
        assert!(store
            .connector_record_for_current_file(tenant_id, file_id, "sha256:forgotten".into(), 1,)
            .await
            .unwrap()
            .is_none());
        let observed_at = store.projection_timestamp().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(store
            .tombstone_snapshot_in_tx(&mut tx, tenant_id, file_id, observed_at)
            .await
            .unwrap());
        tx.commit().await.unwrap();
        let tombstoned: bool = sqlx::query_scalar(
            "SELECT deleted_at IS NOT NULL FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2",
        )
        .bind(tenant_id)
        .bind(file_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            tombstoned,
            "reconciliation removes the stale catalog projection"
        );

        sqlx::query("DELETE FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2")
            .bind(tenant_id)
            .bind(file_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM files WHERE id=$1")
            .bind(file_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM tenants WHERE id=$1")
            .bind(tenant_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires migrated PostgreSQL"]
    async fn stale_removal_snapshot_cannot_tombstone_a_newer_projection() {
        let pool =
            PgPool::connect(&std::env::var("DATABASE_URL").expect("set disposable DATABASE_URL"))
                .await
                .expect("connect to disposable PostgreSQL");
        let store = MemoryFilesCatalogStore::new(pool.clone());
        let tenant_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO memory_files_catalog (tenant_id,file_id,current_version,current_version_number) VALUES ($1,$2,'sha256:newer',2)",
        )
        .bind(tenant_id)
        .bind(file_id)
        .execute(&pool)
        .await
        .expect("insert newer Memory projection");
        let projected_at: DateTime<Utc> = sqlx::query_scalar(
            "SELECT updated_at FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2",
        )
        .bind(tenant_id)
        .bind(file_id)
        .fetch_one(&pool)
        .await
        .expect("read projection write time");
        let mut tx = pool.begin().await.expect("begin equal-time removal");
        let tied_removal = store
            .tombstone_snapshot_in_tx(&mut tx, tenant_id, file_id, projected_at)
            .await
            .expect("conditionally apply equal-time removal");
        tx.commit().await.expect("commit equal-time removal");
        assert!(!tied_removal);
        let mut tx = pool.begin().await.expect("begin stale removal");
        let applied = store
            .tombstone_snapshot_in_tx(
                &mut tx,
                tenant_id,
                file_id,
                projected_at - chrono::Duration::microseconds(1),
            )
            .await
            .expect("apply conditional tombstone");
        tx.commit().await.expect("commit stale removal");
        assert!(!applied);
        let deleted_at: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT deleted_at FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2",
        )
        .bind(tenant_id)
        .bind(file_id)
        .fetch_one(&pool)
        .await
        .expect("verify newer projection remains live");
        assert!(deleted_at.is_none());
        let deleted_file_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO memory_files_catalog (tenant_id,file_id,deleted_at,updated_at) VALUES ($1,$2,clock_timestamp(),clock_timestamp())",
        )
        .bind(tenant_id)
        .bind(deleted_file_id)
        .execute(&pool)
        .await
        .expect("insert newer Files removal");
        let tombstone_at: DateTime<Utc> = sqlx::query_scalar(
            "SELECT updated_at FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2",
        )
        .bind(tenant_id)
        .bind(deleted_file_id)
        .fetch_one(&pool)
        .await
        .expect("read removal watermark");
        let delayed_available = MemoryFilesCatalogRecord {
            tenant_id,
            file_id: deleted_file_id,
            projection_observed_at: tombstone_at - chrono::Duration::microseconds(1),
            current_version: "sha256:old".into(),
            current_version_number: 1,
            connector_instance_id: Uuid::new_v4(),
            source_resource_ref: "elembra://io.elembra.connector.test/file/old".into(),
            source_revision: "sha256:old".into(),
            relative_path: "old.md".into(),
            observed_modified_at: None,
        };
        let mut tx = pool.begin().await.expect("begin stale availability");
        let indexed = store
            .upsert_in_tx(&mut tx, &delayed_available)
            .await
            .expect("conditionally apply delayed availability");
        tx.commit().await.expect("commit stale availability");
        assert!(!indexed);
        let still_deleted: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT deleted_at FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=$2",
        )
        .bind(tenant_id)
        .bind(deleted_file_id)
        .fetch_one(&pool)
        .await
        .expect("verify newer tombstone remains");
        assert!(still_deleted.is_some());
        sqlx::query("DELETE FROM memory_files_catalog WHERE tenant_id=$1 AND file_id=ANY($2)")
            .bind(tenant_id)
            .bind(vec![file_id, deleted_file_id])
            .execute(&pool)
            .await
            .expect("remove isolated test row");
    }
}
