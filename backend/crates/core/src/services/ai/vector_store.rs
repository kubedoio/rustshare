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
/// This mirrors the semantics of `indexing::can_access` but operates on the
/// canonical `IndexAclProjection` and `RetrievalPrincipal` types used by the
/// permission-aware retrieval path.
fn can_access(projection: &IndexAclProjection, principal: &RetrievalPrincipal) -> bool {
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
}
