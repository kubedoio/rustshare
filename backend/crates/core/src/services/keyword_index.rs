//! Keyword-only access to the shared Files/Notes content index.

use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use super::ai::{
    indexing::{note_acl_payload_from_projection, strip_frontmatter, truncate_to_length},
    EmbeddingPolicy, IndexAclProjection, IndexedDocument, RetrievalPrincipal, VectorStore,
};

/// Keyword indexing and search over the same rebuildable store used by AI.
/// This service does not construct or call an embedding or LLM provider.
pub struct KeywordIndexer {
    store: Arc<dyn VectorStore>,
}

impl KeywordIndexer {
    pub fn new(store: Arc<dyn VectorStore>) -> Self {
        Self { store }
    }

    /// Persist the current note body and its authoritative ACL projection
    /// without generating a vector. A denied indexing policy removes prior
    /// chunks instead of being overridden by the keyword-only path.
    #[allow(clippy::too_many_arguments)]
    pub async fn index_note(
        &self,
        file_id: Uuid,
        file_name: String,
        file_path: String,
        content: String,
        mime_type: String,
        owner_id: Uuid,
        acl: IndexAclProjection,
    ) -> Result<()> {
        if acl.embedding_policy == EmbeddingPolicy::Denied {
            self.store
                .remove_source_file_chunks(acl.tenant_id, file_id)
                .await?;
            return Ok(());
        }

        let acl_payload = note_acl_payload_from_projection(file_id, &acl);
        let content = truncate_to_length(
            &strip_frontmatter(&content),
            super::ai::indexing::MAX_CONTENT_LENGTH,
        );
        let document = IndexedDocument {
            file_id,
            file_name,
            file_path,
            content,
            embedding: Vec::new(),
            mime_type,
            owner_id,
            tenant_id: acl.tenant_id,
            indexed_at: chrono::Utc::now(),
            acl: Some(acl_payload.clone()),
            chunk_id: file_id,
        };

        self.store
            .upsert_keyword_chunk(acl.tenant_id, file_id, &document, &acl_payload)
            .await
    }

    /// Find keyword candidates after the store's tenant and coarse ACL filters.
    /// Callers must still reauthorize each source through Files.
    pub async fn search_with_acl(
        &self,
        principal: &RetrievalPrincipal,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(IndexedDocument, f32)>> {
        self.store
            .keyword_search_with_acl(principal, query, limit)
            .await
    }

    pub async fn update_note_acl(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
        acl: &super::ai::NoteAclPayload,
    ) -> Result<usize> {
        if !matches!(
            acl.embedding_policy.parse::<EmbeddingPolicy>(),
            Ok(EmbeddingPolicy::Allowed)
        ) {
            return self.store.remove_note_chunks(tenant_id, note_id).await;
        }
        self.store.update_note_acl(tenant_id, note_id, acl).await
    }

    pub async fn remove_note_chunks(&self, tenant_id: Uuid, note_id: Uuid) -> Result<usize> {
        self.store.remove_note_chunks(tenant_id, note_id).await
    }

    pub async fn remove_source_file_chunks(
        &self,
        tenant_id: Uuid,
        source_file_id: Uuid,
    ) -> Result<usize> {
        self.store
            .remove_source_file_chunks(tenant_id, source_file_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::services::ai::{InMemoryVectorStore, IndexPrincipal, IndexVisibility};

    #[tokio::test]
    async fn denied_acl_update_removes_indexed_body() {
        let store = Arc::new(InMemoryVectorStore::new());
        let indexer = KeywordIndexer::new(store);
        let tenant_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let note_id = Uuid::new_v4();
        let mut projection = IndexAclProjection {
            tenant_id,
            workspace_id: tenant_id,
            object_id: note_id,
            source_folder_id: None,
            owner_id,
            read_principals: vec![IndexPrincipal::Owner(owner_id)],
            visibility: IndexVisibility::Private,
            acl_hash: "acl".to_string(),
            acl_version: 1,
            embedding_policy: EmbeddingPolicy::Allowed,
        };
        indexer
            .index_note(
                note_id,
                "note.md".to_string(),
                "/note.md".to_string(),
                "private body keyword".to_string(),
                "text/markdown".to_string(),
                owner_id,
                projection.clone(),
            )
            .await
            .unwrap();

        projection.embedding_policy = EmbeddingPolicy::Denied;
        let acl =
            super::super::ai::indexing::note_acl_payload_from_projection(note_id, &projection);
        assert_eq!(
            indexer
                .update_note_acl(tenant_id, note_id, &acl)
                .await
                .unwrap(),
            1
        );

        let principal = RetrievalPrincipal {
            tenant_id,
            workspace_id: Some(tenant_id),
            user_id: owner_id,
            group_ids: Vec::new(),
            min_acl_versions: HashMap::new(),
        };
        assert!(indexer
            .search_with_acl(&principal, "private body", 10)
            .await
            .unwrap()
            .is_empty());
    }
}
