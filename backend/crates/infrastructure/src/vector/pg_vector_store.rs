//! PostgreSQL/pgvector backend for the AI content index.
//!
//! All SQL uses `sqlx::query` (non-macro) so the crate compiles with
//! `SQLX_OFFLINE=true` even though the local PostgreSQL does not have the
//! `pgvector` extension installed. Vectors are passed as text and cast to
//! `vector` in SQL; retrieved vectors are selected as `text` and parsed.

use anyhow::Result;
use rustshare_core::services::ai::{
    can_access, validate_and_project, IndexAclProjection, IndexedDocument, NoteAclPayload,
    RetrievalPrincipal, VectorStore,
};
use sqlx::{postgres::PgRow, PgPool, Row};
use uuid::Uuid;

pub struct PgVectorStore {
    pool: PgPool,
}

impl PgVectorStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Encode a slice of f32 as a pgvector text literal.
fn encode_vector(v: &[f32]) -> String {
    let elements: Vec<String> = v.iter().map(|x| x.to_string()).collect();
    format!("[{}]", elements.join(","))
}

/// Parse a pgvector text literal (`[a,b,c]`) into a vector of f32.
fn decode_vector(text: &str) -> anyhow::Result<Vec<f32>> {
    let trimmed = text.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| anyhow::anyhow!("vector text does not start/end with brackets: {text}"))?;

    if inner.is_empty() {
        return Ok(Vec::new());
    }

    inner
        .split(',')
        .map(|s| s.trim().parse::<f32>().map_err(Into::into))
        .collect()
}

/// Convert a returned `note_index_chunks` row into an indexed document.
fn row_to_indexed_doc(row: &PgRow, tenant_id: Uuid) -> Result<IndexedDocument> {
    let acl = NoteAclPayload {
        tenant_id,
        workspace_id: row.try_get("workspace_id")?,
        note_id: row.try_get("note_id")?,
        source_file_id: row.try_get("source_file_id")?,
        source_folder_id: row.try_get("source_folder_id")?,
        owner_id: row.try_get("owner_id")?,
        read_acl: row.try_get("read_acl")?,
        visibility: row.try_get("visibility")?,
        acl_hash: row.try_get("acl_hash")?,
        acl_version: row.try_get("acl_version")?,
        embedding_policy: row.try_get("embedding_policy")?,
    };

    let embedding_text: String = row.try_get("embedding")?;
    let embedding = decode_vector(&embedding_text)?;

    Ok(IndexedDocument {
        file_id: row.try_get("id")?,
        file_name: row.try_get("file_name")?,
        file_path: row.try_get("file_path")?,
        content: row.try_get("content")?,
        embedding,
        mime_type: row.try_get("mime_type")?,
        owner_id: row.try_get("owner_id")?,
        tenant_id,
        indexed_at: row.try_get("indexed_at")?,
        acl: Some(acl),
        chunk_id: row.try_get("id")?,
    })
}

/// Convert a returned `note_index_chunks` row into an indexed document and similarity score.
fn row_to_doc(row: &PgRow, tenant_id: Uuid) -> Result<(IndexedDocument, f32)> {
    let doc = row_to_indexed_doc(row, tenant_id)?;
    let similarity: f64 = row.try_get("similarity")?;
    Ok((doc, similarity as f32))
}

/// Validate that the indexed document has a well-formed, allowed ACL and
/// return the typed projection.
///
/// Logs a warning on missing, malformed, or denied ACLs so retrieval fails
/// closed. Uses only safe identifiers in logs.
fn validate_chunk_acl(doc: &IndexedDocument) -> Option<IndexAclProjection> {
    let acl = doc.acl.as_ref()?;

    match validate_and_project(acl) {
        Ok(projection) => Some(projection),
        Err(e) => {
            tracing::warn!(
                chunk_id = %doc.chunk_id,
                note_id = %acl.note_id,
                tenant_id = %acl.tenant_id,
                error = %e,
                "skipping indexed chunk with malformed ACL"
            );
            None
        }
    }
}

#[async_trait::async_trait]
impl VectorStore for PgVectorStore {
    async fn upsert_chunk(
        &self,
        tenant_id: Uuid,
        chunk_id: Uuid,
        doc: &IndexedDocument,
        acl: &NoteAclPayload,
    ) -> Result<()> {
        let embedding_text = encode_vector(&doc.embedding);

        sqlx::query(
            r#"
            INSERT INTO note_index_chunks (
                id, tenant_id, workspace_id, source_folder_id, note_id, source_file_id,
                file_name, file_path, content, mime_type, owner_id, embedding, acl_hash,
                acl_version, read_acl, visibility, embedding_policy, indexed_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12::vector, $13, $14, $15, $16, $17, $18
            )
            ON CONFLICT (id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                workspace_id = EXCLUDED.workspace_id,
                source_folder_id = EXCLUDED.source_folder_id,
                note_id = EXCLUDED.note_id,
                source_file_id = EXCLUDED.source_file_id,
                file_name = EXCLUDED.file_name,
                file_path = EXCLUDED.file_path,
                content = EXCLUDED.content,
                mime_type = EXCLUDED.mime_type,
                owner_id = EXCLUDED.owner_id,
                embedding = EXCLUDED.embedding,
                acl_hash = EXCLUDED.acl_hash,
                acl_version = EXCLUDED.acl_version,
                read_acl = EXCLUDED.read_acl,
                visibility = EXCLUDED.visibility,
                embedding_policy = EXCLUDED.embedding_policy,
                indexed_at = EXCLUDED.indexed_at
            "#,
        )
        .bind(chunk_id)
        .bind(tenant_id)
        .bind(acl.workspace_id)
        .bind(acl.source_folder_id)
        .bind(acl.note_id)
        .bind(acl.source_file_id)
        .bind(&doc.file_name)
        .bind(&doc.file_path)
        .bind(&doc.content)
        .bind(&doc.mime_type)
        .bind(doc.owner_id)
        .bind(&embedding_text)
        .bind(&acl.acl_hash)
        .bind(acl.acl_version)
        .bind(&acl.read_acl)
        .bind(&acl.visibility)
        .bind(&acl.embedding_policy)
        .bind(doc.indexed_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn search_with_acl(
        &self,
        principal: &RetrievalPrincipal,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(IndexedDocument, f32)>> {
        let tenant_id = principal.tenant_id;
        let limit = limit as i64;

        // Build the caller's principal list for a SQL ACL pre-filter.
        // Exact enforcement happens in Rust via can_access after the rows are fetched.
        let caller_principals = principal.to_index_principals();

        let query_vector_text = encode_vector(query_embedding);

        let rows = sqlx::query(
            r#"
            SELECT
                id, tenant_id, workspace_id, source_folder_id, note_id, source_file_id,
                file_name, file_path, content, mime_type, owner_id, embedding::text as embedding,
                acl_hash, acl_version, read_acl,
                visibility, embedding_policy, indexed_at,
                1 - (embedding <=> $1::vector) AS similarity
            FROM note_index_chunks
            WHERE tenant_id = $2
              AND embedding_policy = 'allowed'
              AND 1 - (embedding <=> $1::vector) > 0.1
              AND (
                  owner_id = $3
                  OR visibility = 'public'
                  OR visibility = 'workspace'
                  OR read_acl && $4::text[]
              )
            ORDER BY embedding <=> $1::vector
            LIMIT $5
            "#,
        )
        .bind(&query_vector_text)
        .bind(tenant_id)
        .bind(principal.user_id)
        .bind(&caller_principals)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut results: Vec<(IndexedDocument, f32)> = Vec::new();
        for row in rows {
            match row_to_doc(&row, tenant_id) {
                Ok((doc, similarity)) => {
                    if let Some(projection) = validate_chunk_acl(&doc) {
                        if can_access(&projection, principal) {
                            results.push((doc, similarity));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        tenant_id = %tenant_id,
                        error = %e,
                        "failed to decode indexed row during ACL search"
                    );
                }
            }
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }

    async fn update_note_acl(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
        new_acl: &NoteAclPayload,
    ) -> Result<usize> {
        let result = sqlx::query(
            r#"
            UPDATE note_index_chunks
            SET workspace_id = $1,
                source_folder_id = $2,
                acl_hash = $3,
                acl_version = $4,
                read_acl = $5,
                visibility = $6,
                embedding_policy = $7
            WHERE tenant_id = $8 AND note_id = $9
            "#,
        )
        .bind(new_acl.workspace_id)
        .bind(new_acl.source_folder_id)
        .bind(&new_acl.acl_hash)
        .bind(new_acl.acl_version)
        .bind(&new_acl.read_acl)
        .bind(&new_acl.visibility)
        .bind(&new_acl.embedding_policy)
        .bind(tenant_id)
        .bind(note_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() as usize)
    }

    async fn remove_note_chunks(&self, tenant_id: Uuid, note_id: Uuid) -> Result<usize> {
        let result =
            sqlx::query("DELETE FROM note_index_chunks WHERE tenant_id = $1 AND note_id = $2")
                .bind(tenant_id)
                .bind(note_id)
                .execute(&self.pool)
                .await?;

        Ok(result.rows_affected() as usize)
    }

    async fn remove_chunk(&self, tenant_id: Uuid, chunk_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM note_index_chunks WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id)
            .bind(chunk_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn clear_tenant(&self, tenant_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM note_index_chunks WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn document_count(&self, tenant_id: Uuid) -> Result<usize> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM note_index_chunks WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_one(&self.pool)
                .await?;

        Ok(count as usize)
    }

    async fn get_chunk(
        &self,
        principal: &RetrievalPrincipal,
        chunk_id: Uuid,
    ) -> Result<Option<IndexedDocument>> {
        let tenant_id = principal.tenant_id;
        let row = sqlx::query(
            r#"
            SELECT
                id, tenant_id, workspace_id, source_folder_id, note_id, source_file_id,
                file_name, file_path, content, mime_type, owner_id, embedding::text as embedding,
                acl_hash, acl_version, read_acl,
                visibility, embedding_policy, indexed_at
            FROM note_index_chunks
            WHERE tenant_id = $1 AND id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(chunk_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let doc = row_to_indexed_doc(&row, tenant_id)?;
        let Some(projection) = validate_chunk_acl(&doc) else {
            return Ok(None);
        };

        if can_access(&projection, principal) {
            Ok(Some(doc))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_vector_roundtrip() {
        let v = vec![1.0f32, 2.5, -3.75, 0.0];
        let text = encode_vector(&v);
        assert_eq!(text, "[1,2.5,-3.75,0]");
        let decoded = decode_vector(&text).unwrap();
        assert_eq!(decoded, v);
    }

    #[test]
    fn decode_vector_with_spaces() {
        let decoded = decode_vector("[1.0, 2.0, 3.0]").unwrap();
        assert_eq!(decoded, vec![1.0f32, 2.0, 3.0]);
    }

    #[test]
    fn decode_empty_vector() {
        let decoded = decode_vector("[]").unwrap();
        assert!(decoded.is_empty());
    }
}
