//! Callback sink that forwards note indexing operations to the AI content index.
//!
//! `NoteService` does not own the `ContentIndexer`; instead it accepts an
//! optional `Arc<dyn NoteIndexSink>`. This keeps unit tests lightweight while
//! allowing production wiring to the real in-memory (later vector) index.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rustshare_core::domain::UserId;
use rustshare_core::services::{ContentIndexer, EmbeddingGenerator, IndexAclProjection};
use uuid::Uuid;

/// Trait for note indexing callbacks.
///
/// The trait is object-safe (it returns boxed futures) so that `NoteService`
/// can hold an `Option<Arc<dyn NoteIndexSink>>` without generics.
pub trait NoteIndexSink: Send + Sync {
    /// Index or re-index a note chunk.
    #[allow(clippy::too_many_arguments)]
    fn index_note(
        &self,
        file_id: Uuid,
        file_name: String,
        file_path: String,
        content: String,
        mime_type: String,
        owner_id: UserId,
        acl: IndexAclProjection,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>>;

    /// Update the ACL projection for every chunk of a note.
    fn update_acl(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
        acl: rustshare_core::services::NoteAclPayload,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>>;

    /// Remove every indexed chunk for a note.
    fn remove_note(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

/// No-op sink used when AI indexing is disabled.
pub struct NoOpNoteIndexSink;

impl NoteIndexSink for NoOpNoteIndexSink {
    #[allow(clippy::too_many_arguments)]
    fn index_note(
        &self,
        _file_id: Uuid,
        _file_name: String,
        _file_path: String,
        _content: String,
        _mime_type: String,
        _owner_id: UserId,
        _acl: IndexAclProjection,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            tracing::debug!("NoOpNoteIndexSink: index_note ignored");
        })
    }

    fn update_acl(
        &self,
        _tenant_id: Uuid,
        _note_id: Uuid,
        _acl: rustshare_core::services::NoteAclPayload,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            tracing::debug!("NoOpNoteIndexSink: update_acl ignored");
        })
    }

    fn remove_note(
        &self,
        _tenant_id: Uuid,
        _note_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            tracing::debug!("NoOpNoteIndexSink: remove_note ignored");
        })
    }
}

/// Real sink backed by a `ContentIndexer`.
pub struct ContentIndexerNoteSink<EG: EmbeddingGenerator> {
    indexer: Arc<ContentIndexer<EG>>,
}

impl<EG: EmbeddingGenerator> ContentIndexerNoteSink<EG> {
    pub fn new(indexer: Arc<ContentIndexer<EG>>) -> Self {
        Self { indexer }
    }
}

impl<EG: EmbeddingGenerator + Send + Sync + 'static> NoteIndexSink for ContentIndexerNoteSink<EG> {
    #[allow(clippy::too_many_arguments)]
    fn index_note(
        &self,
        file_id: Uuid,
        file_name: String,
        file_path: String,
        content: String,
        mime_type: String,
        owner_id: UserId,
        acl: IndexAclProjection,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let indexer = self.indexer.clone();
        Box::pin(async move {
            if let Err(e) = indexer
                .index_note(
                    file_id, file_name, file_path, content, mime_type, owner_id, acl,
                )
                .await
            {
                tracing::warn!("Failed to index note {}: {}", file_id, e);
            }
        })
    }

    fn update_acl(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
        acl: rustshare_core::services::NoteAclPayload,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let indexer = self.indexer.clone();
        Box::pin(async move {
            let updated = indexer.update_note_acl(tenant_id, note_id, acl).await;
            tracing::debug!(
                "Updated ACL for note {} in tenant {} ({} chunks)",
                note_id,
                tenant_id,
                updated
            );
        })
    }

    fn remove_note(
        &self,
        tenant_id: Uuid,
        note_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let indexer = self.indexer.clone();
        Box::pin(async move {
            let removed = indexer.remove_note_chunks(tenant_id, note_id).await;
            tracing::debug!(
                "Removed note {} chunks from tenant {} ({} chunks)",
                note_id,
                tenant_id,
                removed
            );
        })
    }
}
