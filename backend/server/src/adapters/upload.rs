use bytes;
use futures_util::StreamExt;
use rustshare_core::services::UploadError;
use rustshare_storage::{MetadataStore, ObjectStore};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

/// Adapter for ObjectStore to implement UploadObjectStore trait
#[derive(Clone)]
pub struct UploadObjectStoreAdapter {
    inner: Arc<ObjectStore>,
}

impl UploadObjectStoreAdapter {
    pub fn new(inner: Arc<ObjectStore>) -> Self {
        Self { inner }
    }

    fn map_put_if_absent_error(error: anyhow::Error, chunk_index: u32) -> UploadError {
        let message = error.to_string();
        if message.contains("Precondition")
            || message.contains("PreconditionFailed")
            || message.contains("412")
        {
            UploadError::ChunkAlreadyReceived(chunk_index)
        } else {
            UploadError::Storage(message)
        }
    }
}

#[async_trait::async_trait]
impl rustshare_core::services::UploadObjectStore for UploadObjectStoreAdapter {
    async fn put_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        data: bytes::Bytes,
    ) -> Result<(), UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.inner
            .put_if_absent(&key, data)
            .await
            .map_err(|e| Self::map_put_if_absent_error(e, chunk_index))
    }

    async fn put_chunk_from_path(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        path: &std::path::Path,
    ) -> Result<(), UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.inner
            .put_from_path_if_absent(&key, path)
            .await
            .map_err(|e| Self::map_put_if_absent_error(e, chunk_index))
    }

    async fn get_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
    ) -> Result<Option<bytes::Bytes>, UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        match self.inner.get(&key).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => Err(UploadError::Storage(e.to_string())),
        }
    }

    async fn delete_chunk(&self, session_id: Uuid, chunk_index: u32) -> Result<(), UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.inner
            .delete(&key)
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))
    }

    async fn delete_session_chunks(
        &self,
        session_id: Uuid,
        total_chunks: u32,
    ) -> Result<(), UploadError> {
        for chunk_index in 0..total_chunks {
            let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
            if let Err(e) = self.inner.delete(&key).await {
                tracing::warn!(key = %key, error = %e, "failed to delete object during cleanup");
            }
        }
        Ok(())
    }

    async fn chunk_exists(&self, session_id: Uuid, chunk_index: u32) -> Result<bool, UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.inner
            .exists(&key)
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))
    }

    async fn assemble_chunks_to_prefix(
        &self,
        session_id: Uuid,
        total_chunks: u32,
        final_key_prefix: &str,
    ) -> Result<String, UploadError> {
        let temp_file = tokio::task::spawn_blocking(tempfile::NamedTempFile::new)
            .await
            .map_err(|e| UploadError::Storage(format!("Failed to create temp file: {e}")))?
            .map_err(|e| UploadError::Storage(format!("Failed to create temp file: {e}")))?;
        let mut assembled_file = tokio::fs::File::from_std(
            temp_file
                .reopen()
                .map_err(|e| UploadError::Storage(format!("Failed to reopen temp file: {e}")))?,
        );
        let mut hasher = Sha256::new();

        for chunk_index in 0..total_chunks {
            let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
            let stream = self
                .inner
                .get_stream(&key)
                .await
                .map_err(|e| UploadError::Storage(e.to_string()))?
                .2;
            futures_util::pin_mut!(stream);

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| UploadError::Storage(e.to_string()))?;
                hasher.update(&chunk);
                assembled_file
                    .write_all(&chunk)
                    .await
                    .map_err(|e| UploadError::Storage(e.to_string()))?;
            }
        }

        assembled_file
            .flush()
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))?;
        drop(assembled_file);

        let final_hash = hex::encode(hasher.finalize());
        let final_key = format!("{final_key_prefix}{final_hash}");

        self.inner
            .put_from_path(&final_key, temp_file.path())
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))?;

        Ok(final_hash)
    }
}

/// Adapter for MetadataStore to implement UploadMetadataStore trait
#[derive(Clone)]
pub struct UploadMetadataStoreAdapter {
    inner: Arc<MetadataStore>,
}

impl UploadMetadataStoreAdapter {
    pub fn new(inner: Arc<MetadataStore>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl rustshare_core::services::UploadMetadataStore for UploadMetadataStoreAdapter {
    async fn find_folder_by_id(
        &self,
        id: Uuid,
        owner_id: Uuid,
    ) -> Result<Option<rustshare_core::domain::Folder>, UploadError> {
        self.inner
            .find_folder_by_id(id, owner_id)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }

    async fn find_folder_by_id_unchecked(
        &self,
        id: Uuid,
    ) -> Result<Option<rustshare_core::domain::Folder>, UploadError> {
        self.inner
            .find_folder_by_id_unchecked(id)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }

    async fn find_file_by_path(
        &self,
        path: &str,
        owner_id: Uuid,
    ) -> Result<Option<rustshare_core::domain::File>, UploadError> {
        self.inner
            .find_file_by_path(path, owner_id)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }

    async fn create_file(&self, file: &rustshare_core::domain::File) -> Result<(), UploadError> {
        self.inner
            .create_file(file)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }

    async fn update_file(&self, file: &rustshare_core::domain::File) -> Result<(), UploadError> {
        self.inner
            .update_file(file)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }

    async fn create_file_version(
        &self,
        _file: &rustshare_core::domain::File,
        version: &rustshare_core::domain::FileVersion,
    ) -> Result<(), UploadError> {
        self.inner
            .create_file_version(version)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }
}
