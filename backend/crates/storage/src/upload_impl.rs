//! Direct `UploadObjectStore` / `UploadMetadataStore` implementations for the
//! concrete storage types. This collapses the previous server-side adapter
//! wrappers so `UploadService` can be constructed with `Arc<ObjectStore>` and
//! `Arc<MetadataStore>` directly.

use bytes::Bytes;
use futures::StreamExt;
use rustshare_core::domain::{File, Folder};
use rustshare_core::services::{UploadError, UploadMetadataStore, UploadObjectStore};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::{MetadataStore, ObjectStore};

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

impl UploadObjectStore for ObjectStore {
    async fn put_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        data: Bytes,
    ) -> Result<(), UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.put_if_absent(&key, data)
            .await
            .map_err(|e| map_put_if_absent_error(e, chunk_index))
    }

    async fn put_chunk_from_path(
        &self,
        session_id: Uuid,
        chunk_index: u32,
        path: &std::path::Path,
    ) -> Result<(), UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.put_from_path_if_absent(&key, path)
            .await
            .map_err(|e| map_put_if_absent_error(e, chunk_index))
    }

    async fn get_chunk(
        &self,
        session_id: Uuid,
        chunk_index: u32,
    ) -> Result<Option<Bytes>, UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        match self.get(&key).await {
            Ok(data) => Ok(Some(data)),
            Err(e) => Err(UploadError::Storage(e.to_string())),
        }
    }

    async fn delete_chunk(&self, session_id: Uuid, chunk_index: u32) -> Result<(), UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.delete(&key)
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
            if let Err(e) = self.delete(&key).await {
                tracing::warn!(key = %key, error = %e, "failed to delete object during cleanup");
            }
        }
        Ok(())
    }

    async fn delete_object(&self, key: &str) -> Result<(), UploadError> {
        self.delete(key)
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))
    }

    async fn chunk_exists(&self, session_id: Uuid, chunk_index: u32) -> Result<bool, UploadError> {
        let key = format!("temp/uploads/{}/{}", session_id, chunk_index);
        self.exists(&key)
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))
    }

    async fn assemble_chunks_to_prefix(
        &self,
        session_id: Uuid,
        total_chunks: u32,
        final_key_prefix: &str,
    ) -> Result<(String, Box<dyn Send>), UploadError> {
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
                .get_stream(&key)
                .await
                .map_err(|e| UploadError::Storage(e.to_string()))?
                .2;
            futures::pin_mut!(stream);

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
        let blob_write_lock = self
            .acquire_blob_lock(&final_key)
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))?;

        self.put_from_path(&final_key, temp_file.path())
            .await
            .map_err(|e| UploadError::Storage(e.to_string()))?;

        Ok((final_hash, Box::new(blob_write_lock)))
    }
}

impl UploadMetadataStore for MetadataStore {
    async fn find_folder_by_id(
        &self,
        id: Uuid,
        owner_id: Uuid,
    ) -> Result<Option<Folder>, UploadError> {
        self.find_folder_by_id(id, owner_id)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }

    async fn find_folder_by_id_unchecked(&self, id: Uuid) -> Result<Option<Folder>, UploadError> {
        self.find_folder_by_id_unchecked(id)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }

    async fn find_file_by_path(
        &self,
        path: &str,
        owner_id: Uuid,
    ) -> Result<Option<File>, UploadError> {
        self.find_file_by_path(path, owner_id)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }

    async fn create_file(&self, file: &File) -> Result<(), UploadError> {
        self.create_file(file)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }

    async fn update_file(&self, file: &File) -> Result<(), UploadError> {
        self.update_file(file)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }

    async fn create_file_version(
        &self,
        _file: &File,
        version: &rustshare_core::domain::FileVersion,
    ) -> Result<(), UploadError> {
        self.create_file_version(version)
            .await
            .map_err(|e| UploadError::Database(e.to_string()))
    }
}
