use bytes;
use rustshare_core::services::UploadError;
use rustshare_storage::{MetadataStore, ObjectStore};
use std::sync::Arc;
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
            .put(&key, data)
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))
    }

    async fn put_chunk_from_path(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        path: &std::path::Path,
    ) -> Result<(), UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.inner
            .put_from_path(&key, path)
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))
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

    async fn assemble_chunks(
        &self,
        session_id: Uuid,
        total_chunks: u32,
        final_key: &str,
    ) -> Result<(), UploadError> {
        // Download all chunks and concatenate
        let mut assembled = Vec::new();
        for chunk_index in 0..total_chunks {
            let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
            let chunk_data = self
                .inner
                .get(&key)
                .await
                .map_err(|e| UploadError::Storage(e.to_string()))?;
            assembled.extend_from_slice(&chunk_data);
        }

        // Upload assembled file
        self.inner
            .put(final_key, bytes::Bytes::from(assembled))
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))
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
