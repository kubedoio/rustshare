//! Vector storage abstraction for the AI content index.

use std::collections::HashMap;
use uuid::Uuid;

use super::indexing::{
    validate_and_project, EmbeddingPolicy, IndexAclProjection, IndexVisibility, IndexedDocument,
    NoteAclPayload, RetrievalPrincipal,
};

/// A persistent or ephemeral backend for indexed document chunks.
#[async_trait::async_trait]
pub trait VectorStore: Send + Sync {
    /// Upsert a chunk for a note.
    async fn upsert_chunk(
        &self,
        tenant_id: Uuid,
        chunk_id: Uuid,
        doc: &IndexedDocument,
        acl: &NoteAclPayload,
    ) -> anyhow::Result<()>;

    /// Search for chunks similar to the query, pre-filtered by tenant and ACL.
    async fn search_with_acl(
        &self,
        principal: &RetrievalPrincipal,
        query_embedding: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<(IndexedDocument, f32)>>;

    /// Search for chunks whose text contains the query terms, pre-filtered by
    /// tenant and ACL. Candidate producer for keyword search; final source
    /// authorization happens elsewhere.
    async fn keyword_search_with_acl(
        &self,
        principal: &RetrievalPrincipal,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<(IndexedDocument, f32)>>;

    /// Update the ACL projection for every chunk of a note.
    async fn update_note_acl(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
        new_acl: &NoteAclPayload,
    ) -> anyhow::Result<usize>;

    /// Remove every chunk for a note.
    async fn remove_note_chunks(&self, tenant_id: Uuid, note_id: Uuid) -> anyhow::Result<usize>;

    /// Remove a single chunk by id.
    async fn remove_chunk(&self, tenant_id: Uuid, chunk_id: Uuid) -> anyhow::Result<()>;

    /// Remove all chunks for a tenant.
    async fn clear_tenant(&self, tenant_id: Uuid) -> anyhow::Result<()>;

    /// Count chunks for a tenant.
    async fn document_count(&self, tenant_id: Uuid) -> anyhow::Result<usize>;

    /// Look up a single chunk by id, enforcing the caller's ACL.
    async fn get_chunk(
        &self,
        principal: &RetrievalPrincipal,
        chunk_id: Uuid,
    ) -> anyhow::Result<Option<IndexedDocument>>;
}

type TenantDocuments = HashMap<Uuid, HashMap<Uuid, IndexedDocument>>;

/// In-memory vector store used for tests and local development.
pub struct InMemoryVectorStore {
    documents: std::sync::Mutex<TenantDocuments>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self {
            documents: std::sync::Mutex::new(TenantDocuments::new()),
        }
    }
}

impl Default for InMemoryVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        (dot / (norm_a * norm_b)).clamp(0.0, 1.0)
    }
}

fn score_and_rank(
    docs: impl Iterator<Item = IndexedDocument>,
    query_embedding: &[f32],
    limit: usize,
) -> Vec<(IndexedDocument, f32)> {
    let mut results: Vec<(IndexedDocument, f32)> = docs
        .map(|doc| {
            let score = cosine_similarity(query_embedding, &doc.embedding);
            (doc, score)
        })
        .filter(|(_, score)| *score > 0.1)
        .collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

/// Check whether a caller can access a note chunk according to its typed ACL
/// projection.
///
/// This mirrors the semantics of the legacy `indexing::can_access` but operates
/// on the canonical `IndexAclProjection` and `RetrievalPrincipal` types used by
/// the permission-aware retrieval path.
pub fn can_access(projection: &IndexAclProjection, principal: &RetrievalPrincipal) -> bool {
    if projection.embedding_policy != EmbeddingPolicy::Allowed {
        return false;
    }

    if let Some(min_version) = principal.min_acl_versions.get(&projection.object_id) {
        if projection.acl_version < *min_version {
            return false;
        }
    }

    // Owner match.
    if principal.user_id == projection.owner_id {
        return true;
    }

    // Explicit principal match (owner, user, group, or workspace).
    let caller_principals: std::collections::HashSet<_> =
        principal.to_index_principals().into_iter().collect();
    if projection
        .read_principals
        .iter()
        .any(|p| caller_principals.contains(&p.to_string()))
    {
        return true;
    }

    // Workspace visibility match.
    if projection.visibility == IndexVisibility::Workspace {
        if let Some(wid) = principal.workspace_id {
            if wid == projection.workspace_id {
                return true;
            }
        }
    }

    // Public visibility match.
    if projection.visibility == IndexVisibility::Public {
        return true;
    }

    false
}

/// Occurrence-based keyword score in (0,1]; 0.0 when no query term matches.
///
/// The query is split on ASCII whitespace and lowercased; the document's
/// `file_name`, `file_path`, and `content` are lowercased as well. For each
/// term, occurrences are counted with `str::matches` (substring match — the
/// same semantics as the SQL `ILIKE '%term%'` pre-filter) and weighted:
///
/// - `file_name` occurrences count 2.0 (a name match ranks above a content match),
/// - `file_path` occurrences count 1.0,
/// - `content` occurrences count 1.0.
///
/// A term with zero matches anywhere contributes 0. The weighted occurrences
/// are summed across terms, capped at 100.0, and mapped into (0,1] with
/// `score = capped / (capped + 10.0)`. The score is 0.0 when no term matches
/// anywhere (including an empty query). The function is deterministic: the
/// same document and query always produce the same value.
pub fn keyword_score(doc: &IndexedDocument, query: &str) -> f32 {
    let terms: Vec<String> = query
        .split_ascii_whitespace()
        .map(|t| t.to_lowercase())
        .collect();
    if terms.is_empty() {
        return 0.0;
    }

    let name = doc.file_name.to_lowercase();
    let path = doc.file_path.to_lowercase();
    let content = doc.content.to_lowercase();

    let raw: f64 = terms
        .iter()
        .map(|term| {
            2.0 * name.matches(term).count() as f64
                + path.matches(term).count() as f64
                + content.matches(term).count() as f64
        })
        .sum();

    if raw <= 0.0 {
        return 0.0;
    }

    let capped = raw.min(100.0);
    (capped / (capped + 10.0)) as f32
}

#[async_trait::async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn upsert_chunk(
        &self,
        tenant_id: Uuid,
        chunk_id: Uuid,
        doc: &IndexedDocument,
        _acl: &NoteAclPayload,
    ) -> anyhow::Result<()> {
        let mut docs = self.documents.lock().unwrap();
        docs.entry(tenant_id)
            .or_default()
            .insert(chunk_id, doc.clone());
        Ok(())
    }

    async fn search_with_acl(
        &self,
        principal: &RetrievalPrincipal,
        query_embedding: &[f32],
        limit: usize,
    ) -> anyhow::Result<Vec<(IndexedDocument, f32)>> {
        let docs = self.documents.lock().unwrap();
        let tenant_docs = docs.get(&principal.tenant_id).cloned().unwrap_or_default();

        let allowed = tenant_docs.into_values().filter(|doc| match &doc.acl {
            Some(acl) => match validate_and_project(acl) {
                Ok(projection) => can_access(&projection, principal),
                Err(e) => {
                    tracing::warn!(
                        chunk_id = %doc.chunk_id,
                        note_id = %acl.note_id,
                        tenant_id = %acl.tenant_id,
                        error = %e,
                        "Rejecting malformed ACL chunk in memory vector store"
                    );
                    false
                }
            },
            None => {
                tracing::warn!(
                    chunk_id = %doc.chunk_id,
                    tenant_id = %principal.tenant_id,
                    "Rejecting legacy ACL-less chunk in memory vector store"
                );
                false
            }
        });

        Ok(score_and_rank(allowed, query_embedding, limit))
    }

    async fn keyword_search_with_acl(
        &self,
        principal: &RetrievalPrincipal,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<(IndexedDocument, f32)>> {
        let docs = self.documents.lock().unwrap();
        let tenant_docs = docs.get(&principal.tenant_id).cloned().unwrap_or_default();

        let allowed = tenant_docs.into_values().filter(|doc| match &doc.acl {
            Some(acl) => match validate_and_project(acl) {
                Ok(projection) => can_access(&projection, principal),
                Err(e) => {
                    tracing::warn!(
                        chunk_id = %doc.chunk_id,
                        note_id = %acl.note_id,
                        tenant_id = %acl.tenant_id,
                        error = %e,
                        "Rejecting malformed ACL chunk in memory vector store"
                    );
                    false
                }
            },
            None => {
                tracing::warn!(
                    chunk_id = %doc.chunk_id,
                    tenant_id = %principal.tenant_id,
                    "Rejecting legacy ACL-less chunk in memory vector store"
                );
                false
            }
        });

        let mut results: Vec<(IndexedDocument, f32)> = allowed
            .map(|doc| {
                let score = keyword_score(&doc, query);
                (doc, score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        // Deterministic order: score descending, then file_id ascending.
        results.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.file_id.cmp(&b.0.file_id))
        });
        results.truncate(limit);
        Ok(results)
    }

    async fn update_note_acl(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
        new_acl: &NoteAclPayload,
    ) -> anyhow::Result<usize> {
        let mut docs = self.documents.lock().unwrap();
        let mut updated = 0;
        if let Some(tenant) = docs.get_mut(&tenant_id) {
            for doc in tenant.values_mut() {
                if doc.acl.as_ref().is_some_and(|acl| acl.note_id == note_id) {
                    doc.acl = Some(new_acl.clone());
                    updated += 1;
                }
            }
        }
        Ok(updated)
    }

    async fn remove_note_chunks(&self, tenant_id: Uuid, note_id: Uuid) -> anyhow::Result<usize> {
        let mut docs = self.documents.lock().unwrap();
        let mut removed = 0;
        if let Some(tenant) = docs.get_mut(&tenant_id) {
            tenant.retain(|_, doc| {
                let keep = doc.acl.as_ref().is_none_or(|acl| acl.note_id != note_id);
                if !keep {
                    removed += 1;
                }
                keep
            });
        }
        Ok(removed)
    }

    async fn remove_chunk(&self, tenant_id: Uuid, chunk_id: Uuid) -> anyhow::Result<()> {
        let mut docs = self.documents.lock().unwrap();
        if let Some(tenant) = docs.get_mut(&tenant_id) {
            tenant.remove(&chunk_id);
        }
        Ok(())
    }

    async fn clear_tenant(&self, tenant_id: Uuid) -> anyhow::Result<()> {
        let mut docs = self.documents.lock().unwrap();
        docs.remove(&tenant_id);
        Ok(())
    }

    async fn document_count(&self, tenant_id: Uuid) -> anyhow::Result<usize> {
        let docs = self.documents.lock().unwrap();
        Ok(docs.get(&tenant_id).map(|m| m.len()).unwrap_or(0))
    }

    async fn get_chunk(
        &self,
        principal: &RetrievalPrincipal,
        chunk_id: Uuid,
    ) -> anyhow::Result<Option<IndexedDocument>> {
        let docs = self.documents.lock().unwrap();
        let Some(doc) = docs
            .get(&principal.tenant_id)
            .and_then(|tenant| tenant.get(&chunk_id).cloned())
        else {
            return Ok(None);
        };

        let Some(acl) = &doc.acl else {
            tracing::warn!(
                chunk_id = %doc.chunk_id,
                tenant_id = %principal.tenant_id,
                "Rejecting legacy ACL-less chunk in memory vector store"
            );
            return Ok(None);
        };

        match validate_and_project(acl) {
            Ok(projection) => {
                if can_access(&projection, principal) {
                    Ok(Some(doc))
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                tracing::warn!(
                    chunk_id = %doc.chunk_id,
                    note_id = %acl.note_id,
                    tenant_id = %acl.tenant_id,
                    error = %e,
                    "Rejecting malformed ACL chunk in memory vector store"
                );
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::embedding::{EmbeddingGenerator, SimpleEmbeddingGenerator};
    use super::super::indexing::{IndexedDocument, NoteAclPayload, RetrievalPrincipal};
    use super::*;

    fn make_acl(
        tenant_id: Uuid,
        note_id: Uuid,
        source_file_id: Uuid,
        owner_id: Uuid,
        visibility: &str,
        embedding_policy: &str,
        acl_version: i64,
    ) -> NoteAclPayload {
        NoteAclPayload {
            tenant_id,
            workspace_id: tenant_id,
            note_id,
            source_file_id,
            source_folder_id: None,
            owner_id,
            read_acl: vec![format!("owner:{owner_id}")],
            visibility: visibility.to_string(),
            acl_hash: format!("hash-{acl_version}"),
            acl_version,
            embedding_policy: embedding_policy.to_string(),
        }
    }

    async fn make_doc(
        file_id: Uuid,
        tenant_id: Uuid,
        owner_id: Uuid,
        content: &str,
        acl: Option<NoteAclPayload>,
    ) -> IndexedDocument {
        IndexedDocument {
            file_id,
            file_name: "note.md".to_string(),
            file_path: "/note.md".to_string(),
            content: content.to_string(),
            embedding: SimpleEmbeddingGenerator::new().generate(content).await,
            mime_type: "text/markdown".to_string(),
            owner_id,
            tenant_id,
            indexed_at: chrono::Utc::now(),
            acl,
            chunk_id: file_id,
        }
    }

    #[tokio::test]
    async fn in_memory_upsert_and_search() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let acl = make_acl(
            tenant_id, file_id, file_id, owner_id, "private", "allowed", 1,
        );
        let doc = make_doc(
            file_id,
            tenant_id,
            owner_id,
            "rust programming language",
            Some(acl.clone()),
        )
        .await;

        store
            .upsert_chunk(tenant_id, file_id, &doc, &acl)
            .await
            .unwrap();

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let query = SimpleEmbeddingGenerator::new().generate("rust").await;
        let results = store.search_with_acl(&principal, &query, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.file_id, file_id);
    }

    #[tokio::test]
    async fn in_memory_search_with_acl_rejects_stranger() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let stranger_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let acl = make_acl(
            tenant_id, file_id, file_id, owner_id, "private", "allowed", 1,
        );
        let doc = make_doc(
            file_id,
            tenant_id,
            owner_id,
            "secret content",
            Some(acl.clone()),
        )
        .await;

        store
            .upsert_chunk(tenant_id, file_id, &doc, &acl)
            .await
            .unwrap();

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: stranger_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let query = SimpleEmbeddingGenerator::new().generate("secret").await;
        let results = store.search_with_acl(&principal, &query, 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn in_memory_search_with_acl_accepts_group_member() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let group_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let mut acl = make_acl(
            tenant_id,
            file_id,
            file_id,
            owner_id,
            "workspace",
            "allowed",
            1,
        );
        acl.read_acl = vec![format!("group:{group_id}")];
        let doc = make_doc(
            file_id,
            tenant_id,
            owner_id,
            "engineering notes",
            Some(acl.clone()),
        )
        .await;

        store
            .upsert_chunk(tenant_id, file_id, &doc, &acl)
            .await
            .unwrap();

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: Uuid::new_v4(),
            group_ids: vec![group_id],
            min_acl_versions: HashMap::new(),
        };
        let query = SimpleEmbeddingGenerator::new()
            .generate("engineering")
            .await;
        let results = store.search_with_acl(&principal, &query, 10).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn in_memory_search_with_acl_rejects_stale_version() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let acl = make_acl(
            tenant_id,
            file_id,
            file_id,
            owner_id,
            "workspace",
            "allowed",
            1,
        );
        let doc = make_doc(
            file_id,
            tenant_id,
            owner_id,
            "shared content",
            Some(acl.clone()),
        )
        .await;

        store
            .upsert_chunk(tenant_id, file_id, &doc, &acl)
            .await
            .unwrap();

        let mut min_acl_versions = HashMap::new();
        min_acl_versions.insert(file_id, 2);

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions,
        };
        let query = SimpleEmbeddingGenerator::new().generate("shared").await;
        let results = store.search_with_acl(&principal, &query, 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn in_memory_update_note_acl() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let acl = make_acl(
            tenant_id, file_id, file_id, owner_id, "private", "allowed", 1,
        );
        let doc = make_doc(file_id, tenant_id, owner_id, "content", Some(acl.clone())).await;

        store
            .upsert_chunk(tenant_id, file_id, &doc, &acl)
            .await
            .unwrap();

        let new_acl = make_acl(
            tenant_id, file_id, file_id, owner_id, "public", "allowed", 2,
        );
        let updated = store
            .update_note_acl(tenant_id, file_id, &new_acl)
            .await
            .unwrap();
        assert_eq!(updated, 1);

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: Uuid::new_v4(),
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let query = SimpleEmbeddingGenerator::new().generate("content").await;
        let results = store.search_with_acl(&principal, &query, 10).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn in_memory_remove_note_chunks() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let acl = make_acl(
            tenant_id, file_id, file_id, owner_id, "private", "allowed", 1,
        );
        let doc = make_doc(file_id, tenant_id, owner_id, "content", Some(acl.clone())).await;

        store
            .upsert_chunk(tenant_id, file_id, &doc, &acl)
            .await
            .unwrap();
        assert_eq!(store.document_count(tenant_id).await.unwrap(), 1);

        let removed = store.remove_note_chunks(tenant_id, file_id).await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(store.document_count(tenant_id).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn in_memory_clear_tenant_is_isolated() {
        let store = InMemoryVectorStore::new();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_a = Uuid::new_v4();
        let file_b = Uuid::new_v4();

        let acl_a = make_acl(tenant_a, file_a, file_a, owner_id, "private", "allowed", 1);
        let doc_a = make_doc(file_a, tenant_a, owner_id, "tenant a", Some(acl_a.clone())).await;
        store
            .upsert_chunk(tenant_a, file_a, &doc_a, &acl_a)
            .await
            .unwrap();

        let acl_b = make_acl(tenant_b, file_b, file_b, owner_id, "private", "allowed", 1);
        let doc_b = make_doc(file_b, tenant_b, owner_id, "tenant b", Some(acl_b.clone())).await;
        store
            .upsert_chunk(tenant_b, file_b, &doc_b, &acl_b)
            .await
            .unwrap();

        store.clear_tenant(tenant_a).await.unwrap();
        assert_eq!(store.document_count(tenant_a).await.unwrap(), 0);
        assert_eq!(store.document_count(tenant_b).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn in_memory_search_with_acl_rejects_missing_acl() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let doc = make_doc(
            file_id,
            tenant_id,
            owner_id,
            "legacy content without acl",
            None,
        )
        .await;

        // Upsert still stores the document so that re-indexing can add an ACL later.
        store
            .upsert_chunk(
                tenant_id,
                file_id,
                &doc,
                &make_acl(
                    tenant_id, file_id, file_id, owner_id, "private", "allowed", 1,
                ),
            )
            .await
            .unwrap();

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let query = SimpleEmbeddingGenerator::new()
            .generate("legacy content")
            .await;
        let results = store.search_with_acl(&principal, &query, 10).await.unwrap();
        assert!(results.is_empty());

        // get_chunk must also fail closed.
        let chunk = store.get_chunk(&principal, file_id).await.unwrap();
        assert!(chunk.is_none());
    }

    #[tokio::test]
    async fn in_memory_search_with_acl_rejects_malformed_acl() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let mut acl = make_acl(
            tenant_id, file_id, file_id, owner_id, "private", "allowed", 1,
        );
        // Invalid visibility makes validate_and_project fail.
        acl.visibility = "not-a-visibility".to_string();
        let doc = make_doc(
            file_id,
            tenant_id,
            owner_id,
            "malformed acl content",
            Some(acl.clone()),
        )
        .await;

        store
            .upsert_chunk(tenant_id, file_id, &doc, &acl)
            .await
            .unwrap();

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let query = SimpleEmbeddingGenerator::new().generate("malformed").await;
        let results = store.search_with_acl(&principal, &query, 10).await.unwrap();
        assert!(results.is_empty());

        let chunk = store.get_chunk(&principal, file_id).await.unwrap();
        assert!(chunk.is_none());
    }

    #[tokio::test]
    async fn in_memory_search_with_acl_accepts_workspace_member() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let workspace_id = tenant_id;
        let owner_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let acl = make_acl(
            tenant_id,
            file_id,
            file_id,
            owner_id,
            "workspace",
            "allowed",
            1,
        );
        let doc = make_doc(
            file_id,
            tenant_id,
            owner_id,
            "workspace visible content",
            Some(acl.clone()),
        )
        .await;

        store
            .upsert_chunk(tenant_id, file_id, &doc, &acl)
            .await
            .unwrap();

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: Some(workspace_id),
            user_id: member_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let query = SimpleEmbeddingGenerator::new()
            .generate("workspace visible")
            .await;
        let results = store.search_with_acl(&principal, &query, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.file_id, file_id);

        let chunk = store.get_chunk(&principal, file_id).await.unwrap();
        assert!(chunk.is_some());
        assert_eq!(chunk.unwrap().file_id, file_id);
    }

    #[tokio::test]
    async fn in_memory_search_with_acl_excludes_other_tenant() {
        let store = InMemoryVectorStore::new();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_a = Uuid::new_v4();
        let file_b = Uuid::new_v4();

        let acl_a = make_acl(tenant_a, file_a, file_a, owner_id, "private", "allowed", 1);
        let doc_a = make_doc(
            file_a,
            tenant_a,
            owner_id,
            "tenant a content",
            Some(acl_a.clone()),
        )
        .await;
        store
            .upsert_chunk(tenant_a, file_a, &doc_a, &acl_a)
            .await
            .unwrap();

        let acl_b = make_acl(tenant_b, file_b, file_b, owner_id, "private", "allowed", 1);
        let doc_b = make_doc(
            file_b,
            tenant_b,
            owner_id,
            "tenant b content",
            Some(acl_b.clone()),
        )
        .await;
        store
            .upsert_chunk(tenant_b, file_b, &doc_b, &acl_b)
            .await
            .unwrap();

        // Query tenant_a using the owner identity; only the tenant_a chunk should be returned.
        let principal = RetrievalPrincipal {
            tenant_id: tenant_a,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let query = SimpleEmbeddingGenerator::new().generate("content").await;
        let results = store.search_with_acl(&principal, &query, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.file_id, file_a);

        // Querying as the same user but under tenant_b should return the tenant_b chunk.
        let principal_b = RetrievalPrincipal {
            tenant_id: tenant_b,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results_b = store
            .search_with_acl(&principal_b, &query, 10)
            .await
            .unwrap();
        assert_eq!(results_b.len(), 1);
        assert_eq!(results_b[0].0.file_id, file_b);
    }

    /// Build an `IndexedDocument` without generating an embedding, for pure
    /// `keyword_score` tests.
    fn make_keyword_doc(
        file_id: Uuid,
        file_name: &str,
        file_path: &str,
        content: &str,
    ) -> IndexedDocument {
        IndexedDocument {
            file_id,
            file_name: file_name.to_string(),
            file_path: file_path.to_string(),
            content: content.to_string(),
            embedding: Vec::new(),
            mime_type: "text/markdown".to_string(),
            owner_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            indexed_at: chrono::Utc::now(),
            acl: None,
            chunk_id: file_id,
        }
    }

    #[test]
    fn keyword_score_empty_query_is_zero() {
        let doc = make_keyword_doc(Uuid::new_v4(), "notes.md", "/notes.md", "some content");
        assert_eq!(keyword_score(&doc, ""), 0.0);
        assert_eq!(keyword_score(&doc, "   "), 0.0);
    }

    #[test]
    fn keyword_score_no_match_is_zero() {
        let doc = make_keyword_doc(Uuid::new_v4(), "notes.md", "/notes.md", "unrelated content");
        assert_eq!(keyword_score(&doc, "aardvark"), 0.0);
    }

    #[test]
    fn keyword_score_name_match_ranks_above_content_match() {
        let name_match = make_keyword_doc(
            Uuid::new_v4(),
            "budget-plan.md",
            "/docs/budget-plan.md",
            "unrelated body text",
        );
        let content_match = make_keyword_doc(
            Uuid::new_v4(),
            "other.md",
            "/docs/other.md",
            "the budget plan is here",
        );
        let name_score = keyword_score(&name_match, "budget");
        let content_score = keyword_score(&content_match, "budget");
        assert!(name_score > 0.0);
        assert!(content_score > 0.0);
        assert!(
            name_score > content_score,
            "a file_name match must rank above a content-only match"
        );
    }

    #[test]
    fn keyword_score_is_deterministic() {
        let doc = make_keyword_doc(
            Uuid::new_v4(),
            "budget-plan.md",
            "/docs/budget-plan.md",
            "budget budget budget",
        );
        assert_eq!(keyword_score(&doc, "budget"), keyword_score(&doc, "budget"));
        assert_eq!(
            keyword_score(&doc, "budget plan"),
            keyword_score(&doc, "budget plan")
        );
    }

    #[test]
    fn keyword_score_is_case_insensitive_and_matches_any_term() {
        let doc = make_keyword_doc(Uuid::new_v4(), "notes.md", "/notes.md", "Rust code here");
        // Lowercased matching: "RUST" and "rust" agree.
        assert_eq!(keyword_score(&doc, "rust"), keyword_score(&doc, "RUST"));
        // A query with one matching term and one non-matching term is still > 0.
        assert!(keyword_score(&doc, "rust zebra") > 0.0);
        // A query with no matching terms is 0.
        assert_eq!(keyword_score(&doc, "zebra"), 0.0);
    }

    #[tokio::test]
    async fn in_memory_keyword_search_allows_owner() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let acl = make_acl(
            tenant_id, file_id, file_id, owner_id, "private", "allowed", 1,
        );
        let doc = make_doc(
            file_id,
            tenant_id,
            owner_id,
            "the quarterly budget plan",
            Some(acl.clone()),
        )
        .await;

        store
            .upsert_chunk(tenant_id, file_id, &doc, &acl)
            .await
            .unwrap();

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = store
            .keyword_search_with_acl(&principal, "budget", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.file_id, file_id);
        assert!(results[0].1 > 0.0);
    }

    #[tokio::test]
    async fn in_memory_keyword_search_denies_stranger() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let stranger_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let acl = make_acl(
            tenant_id, file_id, file_id, owner_id, "private", "allowed", 1,
        );
        let doc = make_doc(
            file_id,
            tenant_id,
            owner_id,
            "secret keyword content",
            Some(acl.clone()),
        )
        .await;

        store
            .upsert_chunk(tenant_id, file_id, &doc, &acl)
            .await
            .unwrap();

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: stranger_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = store
            .keyword_search_with_acl(&principal, "secret", 10)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn in_memory_keyword_search_denies_stale_version() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();
        let acl = make_acl(
            tenant_id,
            file_id,
            file_id,
            owner_id,
            "workspace",
            "allowed",
            1,
        );
        let doc = make_doc(
            file_id,
            tenant_id,
            owner_id,
            "shared keyword content",
            Some(acl.clone()),
        )
        .await;

        store
            .upsert_chunk(tenant_id, file_id, &doc, &acl)
            .await
            .unwrap();

        let mut min_acl_versions = HashMap::new();
        min_acl_versions.insert(file_id, 2);

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions,
        };
        let results = store
            .keyword_search_with_acl(&principal, "shared", 10)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn in_memory_keyword_search_excludes_other_tenant() {
        let store = InMemoryVectorStore::new();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_a = Uuid::new_v4();
        let file_b = Uuid::new_v4();

        let acl_a = make_acl(tenant_a, file_a, file_a, owner_id, "private", "allowed", 1);
        let doc_a = make_doc(
            file_a,
            tenant_a,
            owner_id,
            "tenant a keyword content",
            Some(acl_a.clone()),
        )
        .await;
        store
            .upsert_chunk(tenant_a, file_a, &doc_a, &acl_a)
            .await
            .unwrap();

        let acl_b = make_acl(tenant_b, file_b, file_b, owner_id, "private", "allowed", 1);
        let doc_b = make_doc(
            file_b,
            tenant_b,
            owner_id,
            "tenant b keyword content",
            Some(acl_b.clone()),
        )
        .await;
        store
            .upsert_chunk(tenant_b, file_b, &doc_b, &acl_b)
            .await
            .unwrap();

        // Query tenant_a; only the tenant_a chunk may appear.
        let principal = RetrievalPrincipal {
            tenant_id: tenant_a,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let results = store
            .keyword_search_with_acl(&principal, "keyword", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.file_id, file_a);
    }

    #[tokio::test]
    async fn in_memory_keyword_search_is_deterministic_and_truncates() {
        let store = InMemoryVectorStore::new();
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let acl = |file_id: Uuid| {
            make_acl(
                tenant_id, file_id, file_id, owner_id, "private", "allowed", 1,
            )
        };

        let mut file_ids = Vec::new();
        for i in 0..5 {
            let file_id = Uuid::new_v4();
            file_ids.push(file_id);
            let doc = make_doc(
                file_id,
                tenant_id,
                owner_id,
                &format!("item {i} has the same keyword once"),
                Some(acl(file_id).clone()),
            )
            .await;
            store
                .upsert_chunk(tenant_id, file_id, &doc, &acl(file_id))
                .await
                .unwrap();
        }

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: None,
            user_id: owner_id,
            group_ids: vec![],
            min_acl_versions: HashMap::new(),
        };
        let first = store
            .keyword_search_with_acl(&principal, "keyword", 3)
            .await
            .unwrap();
        let second = store
            .keyword_search_with_acl(&principal, "keyword", 3)
            .await
            .unwrap();
        assert_eq!(first.len(), 3);
        assert_eq!(first.len(), second.len());
        let ids_first: Vec<Uuid> = first.iter().map(|(doc, _)| doc.file_id).collect();
        let ids_second: Vec<Uuid> = second.iter().map(|(doc, _)| doc.file_id).collect();
        assert_eq!(
            ids_first, ids_second,
            "keyword results must be deterministic"
        );
        // All docs have identical scores, so the tie-break is file_id ascending.
        let mut sorted = ids_first.clone();
        sorted.sort();
        assert_eq!(ids_first, sorted);
    }
}
