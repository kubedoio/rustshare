//! PostgreSQL/pgvector backend for the AI content index.
//!
//! All SQL uses `sqlx::query` (non-macro) so the crate compiles with
//! `SQLX_OFFLINE=true` even though the local PostgreSQL does not have the
//! `pgvector` extension installed. Vectors are passed as text and cast to
//! `vector` in SQL; retrieved vectors are selected as `text` and parsed.

use anyhow::Result;
use rustshare_core::services::{
    can_access, AclSearchFilter, IndexedDocument, NoteAclPayload, VectorStore,
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

/// Convert a returned `note_index_chunks` row into an indexed document and similarity score.
fn row_to_doc(row: &PgRow, tenant_id: Uuid) -> Result<(IndexedDocument, f32)> {
    let acl = NoteAclPayload {
        tenant_id,
        workspace_id: tenant_id,
        note_id: row.try_get("note_id")?,
        source_file_id: row.try_get("source_file_id")?,
        source_folder_id: None,
        owner_id: row.try_get("owner_id")?,
        read_acl: row.try_get("read_acl")?,
        visibility: row.try_get("visibility")?,
        acl_hash: row.try_get("acl_hash")?,
        acl_version: row.try_get("acl_version")?,
        embedding_policy: row.try_get("embedding_policy")?,
    };

    let embedding_text: String = row.try_get("embedding")?;
    let embedding = decode_vector(&embedding_text)?;

    let doc = IndexedDocument {
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
    };

    let similarity: f64 = row.try_get("similarity")?;
    Ok((doc, similarity as f32))
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
                id, tenant_id, note_id, source_file_id, file_name, file_path,
                content, mime_type, owner_id, embedding, acl_hash, acl_version,
                read_acl, visibility, embedding_policy, indexed_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10::vector, $11, $12, $13, $14, $15, $16
            )
            ON CONFLICT (id) DO UPDATE SET
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
        filter: &AclSearchFilter,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(IndexedDocument, f32)>> {
        let caller_user_id = filter.caller_user_id;
        let limit = limit as i64;

        // Build the caller's principal list for a SQL ACL pre-filter.
        // Exact enforcement happens in Rust via can_access after the rows are fetched.
        let mut caller_principals = vec![
            format!("owner:{caller_user_id}"),
            format!("user:{caller_user_id}"),
        ];
        for group_id in &filter.caller_group_ids {
            caller_principals.push(format!("group:{group_id}"));
        }

        let query_vector_text = encode_vector(query_embedding);

        let rows = sqlx::query(
            r#"
            SELECT
                id, note_id, source_file_id, file_name, file_path,
                content, mime_type, owner_id, embedding::text as embedding,
                acl_hash, acl_version, read_acl,
                visibility, embedding_policy, indexed_at,
                1 - (embedding <=> $1::vector) AS similarity
            FROM note_index_chunks
            WHERE tenant_id = $2
              AND embedding_policy = 'allowed'
              AND (
                  owner_id = $3
                  OR visibility = 'public'
                  OR read_acl && $4::text[]
              )
            ORDER BY embedding <=> $1::vector
            LIMIT $5
            "#,
        )
        .bind(&query_vector_text)
        .bind(filter.tenant_id)
        .bind(caller_user_id)
        .bind(&caller_principals)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut results: Vec<(IndexedDocument, f32)> = Vec::new();
        for row in rows {
            let (doc, similarity) = row_to_doc(&row, filter.tenant_id)?;
            if let Some(acl) = &doc.acl {
                if can_access(acl, filter) {
                    results.push((doc, similarity));
                }
            }
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }

    async fn search(
        &self,
        tenant_id: Uuid,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(IndexedDocument, f32)>> {
        let limit = limit as i64;
        let query_vector_text = encode_vector(query_embedding);

        let rows = sqlx::query(
            r#"
            SELECT
                id, note_id, source_file_id, file_name, file_path,
                content, mime_type, owner_id, embedding::text as embedding,
                acl_hash, acl_version, read_acl,
                visibility, embedding_policy, indexed_at,
                1 - (embedding <=> $1::vector) AS similarity
            FROM note_index_chunks
            WHERE tenant_id = $2
            ORDER BY embedding <=> $1::vector
            LIMIT $3
            "#,
        )
        .bind(&query_vector_text)
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row_to_doc(&row, tenant_id)?);
        }

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
            SET acl_hash = $1,
                acl_version = $2,
                read_acl = $3,
                visibility = $4,
                embedding_policy = $5
            WHERE tenant_id = $6 AND note_id = $7
            "#,
        )
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
