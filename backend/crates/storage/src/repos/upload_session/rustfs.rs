//! RustFS-backed upload session repository implementation

use chrono::{DateTime, Utc};
use rustshare_core::services::{
    upload_session::{ChunkInfo, UploadSession, UploadSessionStatus},
    UploadError,
};
use std::sync::Arc;
// tracing::debug is used as tracing::debug! in the code

use uuid::Uuid;

use crate::repos::PathBuilder;
use crate::upload_doc_store::{
    LocalFsDocumentStore, MetadataDocumentStore, MetadataDocumentStoreExt, PutOptions,
};

use rustshare_core::services::UploadSessionRepository;

/// RustFS-backed upload session repository
pub struct RustFsUploadSessionRepository {
    doc_store: Arc<LocalFsDocumentStore>,
    path_builder: PathBuilder,
}

/// Document structure for storing upload sessions
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct UploadSessionDocument {
    /// Schema version
    pub schema_version: u32,
    /// Session ID
    pub id: Uuid,
    /// Tenant ID
    pub tenant_id: Uuid,
    /// Owner user ID
    pub owner_id: Uuid,
    /// Target folder ID
    pub folder_id: Option<Uuid>,
    /// File name
    pub file_name: String,
    /// MIME type
    pub mime_type: String,
    /// Total file size
    pub total_size: u64,
    /// Chunk size
    pub chunk_size: u64,
    /// Bytes uploaded so far
    pub uploaded_bytes: u64,
    /// Chunks received bitmask
    pub chunks_received: Vec<u32>,
    /// Current status
    pub status: UploadSessionStatus,
    /// Expected file hash
    pub file_hash: Option<String>,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Expiration time
    pub expires_at: DateTime<Utc>,
    /// Completion time
    pub completed_at: Option<DateTime<Utc>>,
    /// Created file ID
    pub file_id: Option<Uuid>,
    /// Document version
    pub version: u64,
}

impl From<UploadSession> for UploadSessionDocument {
    fn from(session: UploadSession) -> Self {
        Self {
            schema_version: session.schema_version,
            id: session.id,
            tenant_id: session.tenant_id,
            owner_id: session.owner_id,
            folder_id: session.folder_id,
            file_name: session.file_name,
            mime_type: session.mime_type,
            total_size: session.total_size,
            chunk_size: session.chunk_size,
            uploaded_bytes: session.uploaded_bytes,
            chunks_received: session.chunks_received,
            status: session.status,
            file_hash: session.file_hash,
            created_at: session.created_at,
            expires_at: session.expires_at,
            completed_at: session.completed_at,
            file_id: session.file_id,
            version: session.version,
        }
    }
}

impl From<UploadSessionDocument> for UploadSession {
    fn from(doc: UploadSessionDocument) -> Self {
        Self {
            schema_version: doc.schema_version,
            id: doc.id,
            tenant_id: doc.tenant_id,
            owner_id: doc.owner_id,
            folder_id: doc.folder_id,
            file_name: doc.file_name,
            mime_type: doc.mime_type,
            total_size: doc.total_size,
            chunk_size: doc.chunk_size,
            uploaded_bytes: doc.uploaded_bytes,
            chunks_received: doc.chunks_received,
            status: doc.status,
            file_hash: doc.file_hash,
            created_at: doc.created_at,
            expires_at: doc.expires_at,
            completed_at: doc.completed_at,
            file_id: doc.file_id,
            version: doc.version,
        }
    }
}

/// Chunk info document for storage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ChunkInfoDocument {
    /// Schema version
    pub schema_version: u32,
    /// Session ID
    pub session_id: Uuid,
    /// Chunk index
    pub chunk_index: u32,
    /// Chunk hash
    pub chunk_hash: String,
    /// Chunk size
    pub size: u64,
    /// Received timestamp
    pub received_at: DateTime<Utc>,
}

impl From<ChunkInfo> for ChunkInfoDocument {
    fn from(info: ChunkInfo) -> Self {
        Self {
            schema_version: 1,
            session_id: info.session_id,
            chunk_index: info.chunk_index,
            chunk_hash: info.chunk_hash,
            size: info.size,
            received_at: info.received_at,
        }
    }
}

impl From<ChunkInfoDocument> for ChunkInfo {
    fn from(doc: ChunkInfoDocument) -> Self {
        Self {
            session_id: doc.session_id,
            chunk_index: doc.chunk_index,
            chunk_hash: doc.chunk_hash,
            size: doc.size,
            received_at: doc.received_at,
        }
    }
}

impl RustFsUploadSessionRepository {
    /// Create a new RustFS upload session repository
    pub fn new(
        doc_store: Arc<LocalFsDocumentStore>,
        base_prefix: String,
        namespace: String,
    ) -> Self {
        Self {
            doc_store,
            path_builder: PathBuilder::new(base_prefix, namespace),
        }
    }

    /// Get the path for a session document
    fn session_path(&self, session_id: Uuid) -> String {
        format!(
            "{}/{}/uploads/sessions/{}.json",
            self.path_builder.base_prefix(),
            self.path_builder.namespace(),
            session_id
        )
    }

    /// Get the path for a chunk info document
    fn chunk_path(&self, session_id: Uuid, chunk_index: u32) -> String {
        format!(
            "{}/{}/uploads/chunks/{}/{}.json",
            self.path_builder.base_prefix(),
            self.path_builder.namespace(),
            session_id,
            chunk_index
        )
    }

    /// Get the prefix for listing user sessions
    fn user_sessions_prefix(&self, _user_id: Uuid) -> String {
        format!(
            "{}/{}/uploads/sessions/",
            self.path_builder.base_prefix(),
            self.path_builder.namespace(),
        )
    }
}

impl RustFsUploadSessionRepository {
    /// Get session or return error if not found (helper method)
    async fn get_required(&self, id: Uuid) -> Result<UploadSession, UploadError> {
        self.get_session(id)
            .await?
            .ok_or(UploadError::SessionNotFound(id))
    }

    /// Get all chunks for a session (helper method)
    async fn get_session_chunks(&self, session_id: Uuid) -> Result<Vec<ChunkInfo>, UploadError> {
        let prefix = format!(
            "{}/{}/uploads/chunks/{}/",
            self.path_builder.base_prefix(),
            self.path_builder.namespace(),
            session_id
        );

        let paths = self
            .doc_store
            .list_prefix(&prefix)
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))?;

        let mut chunks: Vec<ChunkInfo> = Vec::new();
        for path in paths {
            if let Ok(Some((doc, _))) = self.doc_store.get::<ChunkInfoDocument>(&path).await {
                chunks.push(doc.into());
            }
        }

        chunks.sort_by_key(|c| c.chunk_index);
        Ok(chunks)
    }
}

impl UploadSessionRepository for RustFsUploadSessionRepository {
    async fn create_session(&self, session: &UploadSession) -> Result<(), UploadError> {
        let doc: UploadSessionDocument = session.clone().into();
        let path = self.session_path(session.id);

        self.doc_store
            .put(&path, &doc, PutOptions::default())
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn get_session(&self, id: Uuid) -> Result<Option<UploadSession>, UploadError> {
        let path = self.session_path(id);

        match self.doc_store.get::<UploadSessionDocument>(&path).await {
            Ok(Some((doc, _))) => Ok(Some(doc.into())),
            Ok(None) => Ok(None),
            Err(e) => Err(UploadError::Storage(e.to_string())),
        }
    }

    async fn update_session(&self, session: &UploadSession) -> Result<(), UploadError> {
        let mut merged = session.clone();
        if let Some(existing) = self.get_session(session.id).await? {
            merged.merge_received_chunks_from(&existing);
            if matches!(
                existing.status,
                UploadSessionStatus::Completed | UploadSessionStatus::Aborted
            ) {
                merged.status = existing.status;
                merged.completed_at = existing.completed_at;
                merged.file_id = existing.file_id;
            }
            merged.version = merged.version.max(existing.version + 1);
        }

        let doc: UploadSessionDocument = merged.into();
        let path = self.session_path(session.id);

        self.doc_store
            .put(&path, &doc, PutOptions::default())
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn update_chunk_received(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        chunk_hash: &str,
        size: u64,
    ) -> Result<(), UploadError> {
        let chunk_info = ChunkInfo::new(session_id, chunk_index, chunk_hash.to_string(), size);
        let doc: ChunkInfoDocument = chunk_info.into();
        let path = self.chunk_path(session_id, chunk_index);

        if self
            .doc_store
            .head(&path)
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))?
            .is_some()
        {
            return Err(UploadError::ChunkAlreadyReceived(chunk_index));
        }

        self.doc_store
            .put(
                &path,
                &doc,
                PutOptions {
                    if_none_match: Some("*".to_string()),
                    ..PutOptions::default()
                },
            )
            .await
            .map_err(|e| {
                let message = e.to_string();
                if message.contains("Precondition")
                    || message.contains("PreconditionFailed")
                    || message.contains("412")
                {
                    UploadError::ChunkAlreadyReceived(chunk_index)
                } else {
                    UploadError::Storage(message)
                }
            })?;

        Ok(())
    }

    async fn get_chunk_info(
        &self,
        session_id: Uuid,
        chunk_index: u32,
    ) -> Result<Option<ChunkInfo>, UploadError> {
        let path = self.chunk_path(session_id, chunk_index);

        match self.doc_store.get::<ChunkInfoDocument>(&path).await {
            Ok(Some((doc, _))) => Ok(Some(doc.into())),
            Ok(None) => Ok(None),
            Err(e) => Err(UploadError::Storage(e.to_string())),
        }
    }

    async fn complete_session(&self, session_id: Uuid, file_id: Uuid) -> Result<(), UploadError> {
        let mut session = self.get_required(session_id).await?;
        session.mark_completed(file_id);
        self.update_session(&session).await
    }

    async fn abort_session(&self, session_id: Uuid) -> Result<(), UploadError> {
        let mut session = self.get_required(session_id).await?;
        session.mark_aborted();
        self.update_session(&session).await
    }

    async fn delete_session(&self, session_id: Uuid) -> Result<(), UploadError> {
        // Delete session document - best effort, may not exist
        let session_path = self.session_path(session_id);
        if let Err(e) = self.doc_store.delete(&session_path).await {
            tracing::debug!(session_id = %session_id, path = %session_path, error = %e, "failed to delete session document");
        }

        // Delete all chunk info documents - best effort
        let chunks = self.get_session_chunks(session_id).await?;
        for chunk in chunks {
            let chunk_path = self.chunk_path(session_id, chunk.chunk_index);
            if let Err(e) = self.doc_store.delete(&chunk_path).await {
                tracing::debug!(session_id = %session_id, chunk_index = chunk.chunk_index, error = %e, "failed to delete chunk info");
            }
        }

        Ok(())
    }

    async fn list_expired_sessions(
        &self,
        before: DateTime<Utc>,
    ) -> Result<Vec<UploadSession>, UploadError> {
        let prefix = format!(
            "{}/{}/uploads/sessions/",
            self.path_builder.base_prefix(),
            self.path_builder.namespace(),
        );

        let paths = self
            .doc_store
            .list_prefix(&prefix)
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))?;

        let mut expired = Vec::new();
        for path in paths {
            if let Ok(Some((doc, _))) = self.doc_store.get::<UploadSessionDocument>(&path).await {
                if doc.expires_at < before {
                    expired.push(doc.into());
                }
            }
        }

        Ok(expired)
    }

    async fn list_user_sessions(&self, user_id: Uuid) -> Result<Vec<UploadSession>, UploadError> {
        let prefix = self.user_sessions_prefix(user_id);

        let paths = self
            .doc_store
            .list_prefix(&prefix)
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))?;

        let mut sessions: Vec<UploadSession> = Vec::new();
        for path in paths {
            if let Ok(Some((doc, _))) = self.doc_store.get::<UploadSessionDocument>(&path).await {
                if doc.owner_id == user_id {
                    sessions.push(doc.into());
                }
            }
        }

        // Sort by created_at descending
        sessions.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upload_doc_store::{LocalFsDocumentStore, MetadataBackendConfig};
    use tempfile::TempDir;

    async fn create_test_repository() -> (RustFsUploadSessionRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = MetadataBackendConfig {
            base_prefix: "test".to_string(),
            namespace: "default".to_string(),
        };

        let doc_store = Arc::new(LocalFsDocumentStore::new(
            temp_dir.path().to_path_buf(),
            config,
        ));

        let repo = RustFsUploadSessionRepository::new(
            doc_store,
            "apps/rustshare".to_string(),
            "test".to_string(),
        );

        (repo, temp_dir)
    }

    fn create_test_session(id: Uuid, owner_id: Uuid) -> UploadSession {
        UploadSession::new(
            id,
            Uuid::new_v4(), // tenant_id
            owner_id,
            None, // folder_id
            "test.pdf".to_string(),
            "application/pdf".to_string(),
            10 * 1024 * 1024, // 10MB
            5 * 1024 * 1024,  // 5MB chunks
            None,             // file_hash
        )
    }

    #[tokio::test]
    async fn test_create_and_get_session() {
        let (repo, _temp) = create_test_repository().await;
        let session_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let session = create_test_session(session_id, owner_id);
        repo.create_session(&session).await.unwrap();

        let retrieved = repo.get_session(session_id).await.unwrap().unwrap();
        assert_eq!(retrieved.id, session_id);
        assert_eq!(retrieved.owner_id, owner_id);
        assert_eq!(retrieved.file_name, "test.pdf");
        assert_eq!(retrieved.total_chunks(), 2);
    }

    #[tokio::test]
    async fn test_update_session() {
        let (repo, _temp) = create_test_repository().await;
        let session_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let mut session = create_test_session(session_id, owner_id);
        repo.create_session(&session).await.unwrap();

        // Update session
        session.mark_in_progress();
        session.mark_chunk_received(0);
        repo.update_session(&session).await.unwrap();

        let retrieved = repo.get_session(session_id).await.unwrap().unwrap();
        assert!(retrieved.has_chunk(0));
        assert_eq!(retrieved.status, UploadSessionStatus::InProgress);
    }

    #[tokio::test]
    async fn test_chunk_tracking() {
        let (repo, _temp) = create_test_repository().await;
        let session_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let session = create_test_session(session_id, owner_id);
        repo.create_session(&session).await.unwrap();

        // Add chunk
        repo.update_chunk_received(session_id, 0, "hash123", 1024)
            .await
            .unwrap();

        let chunk = repo.get_chunk_info(session_id, 0).await.unwrap().unwrap();
        assert_eq!(chunk.chunk_index, 0);
        assert_eq!(chunk.chunk_hash, "hash123");
        assert_eq!(chunk.size, 1024);

        // Get all chunks
        let chunks = repo.get_session_chunks(session_id).await.unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_index, 0);
    }

    #[tokio::test]
    async fn test_complete_and_abort_session() {
        let (repo, _temp) = create_test_repository().await;
        let session_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();
        let file_id = Uuid::new_v4();

        let session = create_test_session(session_id, owner_id);
        repo.create_session(&session).await.unwrap();

        // Complete
        repo.complete_session(session_id, file_id).await.unwrap();
        let completed = repo.get_session(session_id).await.unwrap().unwrap();
        assert_eq!(completed.status, UploadSessionStatus::Completed);
        assert_eq!(completed.file_id, Some(file_id));

        // Create another for abort test
        let session_id2 = Uuid::new_v4();
        let session2 = create_test_session(session_id2, owner_id);
        repo.create_session(&session2).await.unwrap();

        repo.abort_session(session_id2).await.unwrap();
        let aborted = repo.get_session(session_id2).await.unwrap().unwrap();
        assert_eq!(aborted.status, UploadSessionStatus::Aborted);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let (repo, _temp) = create_test_repository().await;
        let session_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let session = create_test_session(session_id, owner_id);
        repo.create_session(&session).await.unwrap();

        // Add a chunk
        repo.update_chunk_received(session_id, 0, "hash123", 1024)
            .await
            .unwrap();

        // Delete
        repo.delete_session(session_id).await.unwrap();

        // Verify deleted
        let retrieved = repo.get_session(session_id).await.unwrap();
        assert!(retrieved.is_none());

        let chunk = repo.get_chunk_info(session_id, 0).await.unwrap();
        assert!(chunk.is_none());
    }

    #[tokio::test]
    async fn test_list_user_sessions() {
        let (repo, _temp) = create_test_repository().await;
        let owner_id = Uuid::new_v4();
        let other_user = Uuid::new_v4();

        // Create sessions for owner
        for i in 0..3 {
            let session = create_test_session(Uuid::new_v4(), owner_id);
            let mut session = session;
            session.file_name = format!("file{}.pdf", i);
            repo.create_session(&session).await.unwrap();
        }

        // Create session for other user
        let other_session = create_test_session(Uuid::new_v4(), other_user);
        repo.create_session(&other_session).await.unwrap();

        let user_sessions = repo.list_user_sessions(owner_id).await.unwrap();
        assert_eq!(user_sessions.len(), 3);
    }
}
